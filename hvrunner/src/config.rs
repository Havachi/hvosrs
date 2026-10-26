use std::path::{Path, PathBuf};
use anyhow::{Context, Result, anyhow};
use toml::{Value};

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Config {
	pub image_path: PathBuf,
	pub build_command: Vec<String>,
	pub run_command: Vec<String>,
	pub run_args: Option<Vec<String>>,
	pub test_args: Option<Vec<String>>,
	pub test_timeout: u32,
	pub test_success_exit_code: Option<i32>,
	pub test_no_reboot: bool,
	pub prebuilder: Option<String>,
	pub filesystem: Option<String>,
}

pub fn read_config(manifest_path: &Path) -> Result<Config> {
	read_config_inner(manifest_path).context("Failed to read hvrunner configuration")
}

fn read_config_inner(manifest_path: &Path) -> Result<Config> {
	use std::{fs::File, io::Read};
    let cargo_toml: Value = {
        let mut content = String::new();
        File::open(manifest_path)
            .context("Failed to open Cargo.toml")?
            .read_to_string(&mut content)
            .context("Failed to read Cargo.toml")?;
        content
            .parse::<Value>()
            .context("Failed to parse Cargo.toml")?
    };
	println!("cargotoml: {:?}", cargo_toml);
	let metadata = cargo_toml
		.get("package")
		.and_then(|table| table.get("metadata"))
		.and_then(|table| table.get("hvrunner"));
	let metadata = match metadata {
		None => {
			let config = ConfigBuilder::default();
			println!("using default config");
			return Ok(config.into());
		}
		Some(metadata) => metadata
			.as_table()
			.ok_or_else(|| anyhow!("hvrunner configuration invalid: {:?}", metadata))?,
	};

	let mut config = ConfigBuilder::default();

	for (key, value) in metadata {
		match (key.as_str(), value.clone()) {
			("image_path", Value::String(path)) => {
				config.image_path = Some(PathBuf::from(path));
			}
            ("test-timeout", Value::Integer(timeout)) if timeout.is_negative() => {
                return Err(anyhow!("test-timeout must not be negative"))
            }
            ("test-timeout", Value::Integer(timeout)) => {
                config.test_timeout = Some(timeout as u32);
            }
            ("test-success-exit-code", Value::Integer(exit_code)) => {
                config.test_success_exit_code = Some(exit_code as i32);
            }
            ("build-command", Value::Array(array)) => {
                config.build_command = Some(parse_string_array(array, "build-command")?);
            }
            ("run-command", Value::Array(array)) => {
                config.run_command = Some(parse_string_array(array, "run-command")?);
            }
            ("run-args", Value::Array(array)) => {
                config.run_args = Some(parse_string_array(array, "run-args")?);
            }
            ("test-args", Value::Array(array)) => {
                config.test_args = Some(parse_string_array(array, "test-args")?);
            }
            ("test-no-reboot", Value::Boolean(no_reboot)) => {
                config.test_no_reboot = Some(no_reboot);
            }
            ("prebuilder", Value::String(prebuilder)) => {
                config.prebuilder = Some(prebuilder);
            }
            ("filesystem", Value::String(filesystem)) => {
                config.filesystem = Some(filesystem);
            }
			(key, value) => {
                return Err(anyhow!(
                    "unexpected `package.metadata.hvrunner` \
                 key `{}` with value `{}`",
                    key,
                    value
                ))
            }
		}
	}
	Ok(config.into())
}

fn parse_string_array(array: Vec<Value>, prop_name: &str) -> Result<Vec<String>> {
	let mut parsed = Vec::new();
	for value in array {
		match value {
			Value::String(s) => parsed.push(s),
			_ => return Err(anyhow!("{} must be list of strings", prop_name))
		}
	}
	Ok(parsed)
}

#[derive(Default)]
struct ConfigBuilder {
	pub image_path: Option<PathBuf>,
	pub build_command: Option<Vec<String>>,
	pub run_command: Option<Vec<String>>,
	pub run_args: Option<Vec<String>>,
	pub test_args: Option<Vec<String>>,
	pub test_timeout: Option<u32>,
	pub test_success_exit_code: Option<i32>,
	pub test_no_reboot: Option<bool>,
	pub prebuilder: Option<String>,
	pub filesystem: Option<String>,
}

impl Into<Config> for ConfigBuilder {
	fn into(self) -> Config {
		Config {
			image_path: self.image_path.unwrap_or("target/kernel.iso".into()),
			build_command: self.build_command.unwrap_or_else(|| vec!["build".into()]),
			run_command: self.run_command.unwrap_or_else(|| {
				vec![
					"qemu-system-x86_64".into(),
					"-m".into(), "2G".into(),
					"-cdrom".into(), "{}".into(),
                    "-drive".into(), "if=pflash,unit=0,format=raw,file=target/ovmf/ovmf-code-x86_64.fd,readonly=on".into(),
                    "-drive".into(), "if=pflash,unit=1,format=raw,file=target/ovmf/ovmf-vars-x86_64.fd".into(),
				]
			}),
			run_args: self.run_args,
			test_args: self.test_args,
			test_timeout: self.test_timeout.unwrap_or(60 * 5),
			test_success_exit_code: self.test_success_exit_code,
			test_no_reboot: self.test_no_reboot.unwrap_or(true),
			prebuilder: self.prebuilder,
			filesystem: self.filesystem,
		}
	}
}