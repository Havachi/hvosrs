use std::{env, path::Path, process};

use anyhow::{Context, Result, anyhow};
use hvrunner::{args::{HvrunnerArgs, HvrunnerCommand}, builder::{self, Builder}, config::{self, Config}, runner};

pub fn main() -> Result<()> {
    validate_executable_name()?;
    validate_arguments()?;

    let builder = Builder::new(None)?;
    let config = config::read_config(builder.manifest_path())?;
    process(&config)?;

    Ok(())
}

fn validate_executable_name() -> Result<()> {
	let executable_name = env::args()
		.next()
		.ok_or_else(|| anyhow!("no first argument (executable name)"))?;
	let file_stem = Path::new(&executable_name)
		.file_stem()
		.and_then(|s| s.to_str());
	if file_stem != Some("hvrunner") {
		return Err(anyhow!(
			"Unexpected executable name: expected 'hvrunner', got: `{:?}`",
			file_stem
		));
	}
	Ok(())
}

fn validate_arguments() -> Result<()> {
	let mut raw_args = env::args();
	raw_args.next();

	match raw_args.next().as_deref() {
		Some("clean") | Some("build") | Some("run") | None => (),
		Some("--help") | Some("-h") => {
			print_help();
			return Ok(());
		}
		Some("--version") | Some("-v") => {
			let version = env!("CARGO_PKG_VERSION");
			println!("HvRunner {version}");
			return Ok(());
		},
		Some(arg) => return Err(anyhow!(
			"invalid command: `{}`.\nTry 'hvrunner --help' for more information.",
			arg
		))
	}
	Ok(())
}

fn process(config: &Config) -> Result<()> {
	
	let raw_args = env::args();
	let exit_code = match HvrunnerCommand::parse_args(raw_args)? {
		HvrunnerCommand::Build => Some(builder::build(&config)?),
		HvrunnerCommand::Run(args) => Some(runner(args)?),
		HvrunnerCommand::Version => None,
		HvrunnerCommand::Help => None,
	};

	if let Some(code) = exit_code {
		process::exit(code);
	}

	Ok(())
}

pub(crate) fn runner(args: HvrunnerArgs) -> Result<i32> {

	let mut builder = Builder::new(None)?;
	let config = config::read_config(builder.manifest_path())?;
	let exe_parent = args
		.executable
		.parent()
		.ok_or_else(|| anyhow!("kernel executable has no parent"))?;
	let is_doctest = exe_parent
		.file_name()
		.ok_or_else(|| anyhow!("kernel executable's parent has no file name"))?
		.to_str()
		.ok_or_else(|| anyhow!("kernel executable's parent file name is not valid UTF-8"))?
		.starts_with("rustdoctest");
	let is_test = is_doctest || exe_parent.ends_with("deps");

	let executable_canon = args.executable.canonicalize().with_context(|| {
		format!(
			"failed to canonicalize executable path `{}`",
			args.executable.display(),
		)
	})?;

	builder.build(&config, &Some(executable_canon))?;

	let exit_code = runner::run(config, is_test)?;
	process::exit(exit_code);
}



fn print_help() {
	println!("HvRunner is a tool to manage, build and run HvOS");
	println!();
	println!("USAGE:");
	println!("hvrunner [OPTIONS] <COMMAND> [ARGS]");
	println!();
	println!("OPTIONS:");
	println!("		-h, --help		Print this help message");
	println!("		-v,	--version	Prints version informations");
	println!();
	println!("COMMANDS:");
	println!("		build			Builds the kernel");
	println!("		run				Runs the kernel");
	println!("		clean			Deletes all build artefacts");
}