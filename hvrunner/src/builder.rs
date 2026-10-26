use std::{fs::{self}, path::{Path, PathBuf}, process::Stdio};

use cargo_metadata::Metadata;
use thiserror::Error;

use crate::config::Config;

pub fn build(config: &Config) -> Result<i32, BuildError> {
    println!("Building kernel: {}", config.image_path.to_str().unwrap());
	let mut builder = Builder::new(None)?;
	builder.build(&config, &None)
}


pub struct Builder {
	manifest_path: PathBuf,
	project_metadata: Option<Metadata>,
}

impl Builder {
	pub fn new(manifest_path: Option<PathBuf>) -> Result<Self, BuildError> {
		let manifest_path = match manifest_path.or_else(|| {
			std::env::var("CARGO_MANIFEST_DIR")
				.ok()
				.map(|dir| Path::new(&dir).join("Cargo.toml"))
		}) {
			Some(path) => path,
			None => {
				//println!("WARNING: `CARGO_MANIFEST_DIR` env variable not set");
				
                locate_cargo_manifest::locate_manifest()?
			}
		};
		Ok(Builder {
			manifest_path,
			project_metadata: None,
		})
	}
	pub fn manifest_path(&self) -> &Path {
		&self.manifest_path
	}

	pub fn project_metadata(&mut self) -> Result<&Metadata, cargo_metadata::Error> {
		if let Some(ref metadata) = self.project_metadata {
			return Ok(metadata);
		}
		let metadata = cargo_metadata::MetadataCommand::new()
			.manifest_path(&self.manifest_path)
			.exec()?;
		Ok(self.project_metadata.get_or_insert(metadata))
	}

	pub fn build(&mut self, config: &Config, bin_path: &Option<PathBuf>) -> Result<i32, BuildError> {
        self.execute_prebuilder(&config)?;
		self.prepare_ovmf_files()?;
		self.prepare_limine_files()?;
        self.build_kernel()?;
		self.copy_kernel(&bin_path)?;
        self.build_userland_programs()?;
        self.copy_userland_programs()?;
		self.create_limine_iso(&config)?;
		
		Ok(0)	
	}

    fn build_kernel(&mut self) -> Result<(), BuildError> {
        println!("Building kernel binary");
        let output = std::process::Command::new("cargo")
            .current_dir("kernel")
            .arg("build")
            .arg("--manifest-path")
            .arg("Cargo.toml")
            .arg("--target-dir")
            .arg("../target")
            .stdout(Stdio::piped())
            .output()
            .map_err(|e| {
                    eprintln!("Failed to execute command: {}",e);
                    BuildError::CargoBuildFailed
                }
            )?;
        if !output.status.success() {
            eprintln!("Cargo build failed:\n{}", String::from_utf8_lossy(&output.stderr));
            return Err(BuildError::CargoBuildFailed);
        }
        Ok(())
    }

	fn execute_prebuilder(&self, config: &Config) -> Result<(), BuildError> {
		std::process::Command::new("sh")
			.arg("-c")
			.arg(config.prebuilder.as_ref().unwrap_or(&"None".to_string()))
			.stdout(Stdio::piped())
			.output()
			.map_err(|_| BuildError::CargoBuildFailed)?;
		Ok(())
	}

    fn prepare_ovmf_files(&self) -> Result<(), BuildError> {
        std::fs::create_dir_all("./target/ovmf-source").unwrap();
        std::fs::create_dir_all("./target/ovmf").unwrap();
        let prebuilt = ovmf_prebuilt::Prebuilt::fetch(
            ovmf_prebuilt::Source::LATEST,
            "target/ovmf-source"
        )
        .ok()
        .ok_or_else(|| anyhow::anyhow!("Failed fetching OVMF Prebuilt"))
        .unwrap();
        let ovmf_code = prebuilt.get_file(ovmf_prebuilt::Arch::X64, ovmf_prebuilt::FileType::Code);
        let ovmf_vars = prebuilt.get_file(ovmf_prebuilt::Arch::X64, ovmf_prebuilt::FileType::Vars);

        std::fs::copy(ovmf_code, "./target/ovmf/ovmf-code-x86_64.fd")
            .map_err(|e| {
                BuildError::CopyOvmfFromSource(e.to_string())
            })?;
        std::fs::copy(ovmf_vars, "./target/ovmf/ovmf-vars-x86_64.fd")
            .map_err(|e| {
                BuildError::CopyOvmfFromSource(e.to_string())
            })?;
        Ok(())
    }

	fn prepare_limine_files(&self) -> Result<(), BuildError> {
        std::fs::create_dir_all("./target").unwrap();
        
        self.clone_limine_binary()?;
        self.copy_limine_config()?;
        self.copy_limine_binary()?;
        Ok(())
    }

    fn clone_limine_binary(&self) -> Result<(), BuildError> {


        if !std::path::Path::new("./target/limine").exists() 
            || !std::path::Path::new("./target/limine/limine-bios.sys").exists() 
            || !std::path::Path::new("./target/limine/limine-bios-cd.bin").exists() 
            || !std::path::Path::new("./target/limine/limine-uefi-cd.bin").exists() {
            if !std::path::Path::new("./target/limine").exists() {
                std::fs::create_dir_all("./target/limine").unwrap();
            }

            let branch = "v8.x-binary";
            let url = "https://github.com/limine-bootloader/limine.git";
            let path = Path::new("target/limine");
            println!("Cloning Limine {} into {}", branch, path.to_str().unwrap());
            let repo = match git2::build::RepoBuilder::new().branch(branch).clone(url, path) {
                Ok(repo) => repo,
                Err(e) => panic!("failed to clone: {}", e),
            };

            let head = repo.head().map_err(|_| BuildError::CloneLimineBinaryFailed)?;
            let head_id = head.target().unwrap();
            let head_commit = repo.find_commit(head_id).map_err(|_| BuildError::CloneLimineBinaryFailed)?;
            println!("Clone limine repo with head_commit: {:?}", head_commit);
        }

        Ok(())
    }

    fn copy_limine_config(&self) -> Result<(), BuildError> {
        std::fs::create_dir_all("./target/iso_root/boot/limine").unwrap();
        std::fs::copy("./limine.conf", "./target/iso_root/boot/limine/limine.conf")
            .map_err(|_| BuildError::CopyLimineConfigFailed)?;
        Ok(())
    }

    fn copy_limine_binary(&self) -> Result<(), BuildError> {
        std::fs::create_dir_all("./target/iso_root/EFI/BOOT").unwrap();
        
        std::fs::copy("target/limine/limine-bios.sys", "target/iso_root/boot/limine/limine-bios.sys")
            .map_err(|e| {
                println!("Failed to copy limine-bios.sys: {}", e);
                BuildError::CopyLimineBinaryFailed
            })?;
        std::fs::copy("target/limine/limine-bios-cd.bin", "target/iso_root/boot/limine/limine-bios-cd.bin")
            .map_err(|e| {
                println!("fila to copy limine-bios-cd.bin: {}", e);
                BuildError::CopyLimineBinaryFailed
            })?;
        std::fs::copy("target/limine/limine-uefi-cd.bin", "target/iso_root/boot/limine/limine-uefi-cd.bin")
            .map_err(|e| {
                println!("Failed to copy limine-uefi-cd.bin: {}", e);
                BuildError::CopyLimineBinaryFailed
            })?;
        
        std::fs::copy("target/limine/BOOTX64.EFI", "target/iso_root/EFI/BOOT/BOOTX64.EFI")
            .map_err(|_| BuildError::CopyLimineBinaryFailed)?;
        std::fs::copy("target/limine/BOOTIA32.EFI", "target/iso_root/EFI/BOOT/BOOTIA32.EFI")
            .map_err(|_| BuildError::CopyLimineBinaryFailed)?;
        Ok(())
    }
	
	fn copy_kernel(&mut self, bin_path: &Option<PathBuf>) -> Result<(), BuildError> {
        std::fs::create_dir_all("target/iso_root/boot/kernel")
            .map_err(|_| BuildError::CreateDirectoryFailed)?;

        let kernel_binary = if let Some(path) = bin_path {
            path.clone()
        } else {
            PathBuf::from("target/x86_64-hvos/debug/kernel.elf")
        };
        
        std::fs::copy(
            &kernel_binary,
            "target/iso_root/boot/kernel/kernel"
        ).map_err(|e| {
            eprintln!("{}: {:#?}", e.to_string(), kernel_binary);
            BuildError::CopyKernelBinaryFailed
        })?;

        Ok(())
    }

    fn build_userland_programs(&mut self) -> Result<(), BuildError> {

        let userland_dirs = fs::read_dir("userland").unwrap();

        for d in userland_dirs {
            if let Ok(entry) = d {
                if entry.file_type().unwrap().is_dir() {
                    let crate_name = entry.file_name().to_ascii_lowercase();
                    let target_path = PathBuf::from("../../target/userland").join(crate_name);
                    std::fs::create_dir_all("target/userland")
                        .map_err(|_| BuildError::CreateDirectoryFailed)?;
                    println!("Building userland program: {}", entry.file_name().display());
                    let o = std::process::Command::new("cargo")
                        .current_dir(entry.path())
                        .arg("build")
                        .arg("--release")
                        .arg("--manifest-path")
                        .arg("Cargo.toml")
                        .arg("--target-dir")
                        .arg(target_path)
                        .arg("--target")
                        .arg("x86_64-unknown-none")
                        .stdout(Stdio::piped())
                        .output()
                        .map_err(|e| {
                                eprintln!("Failed to execute command: {}",e);
                                BuildError::CargoBuildFailed
                            }
                        )?;
                    if !o.status.success() {
                        eprintln!("{}", String::from_utf8_lossy(&o.stderr));
                        return Err(BuildError::CargoBuildFailed);
                    }
                }
            }
        }
        Ok(())
    }

    fn copy_userland_programs(&mut self) -> Result<(), BuildError> {
        std::fs::create_dir_all("./target/iso_root/userland").unwrap();
        let userland_dirs = fs::read_dir("userland").unwrap();
        for d in userland_dirs {
            if let Ok(entry) = d {
                if entry.file_type().unwrap().is_dir() {
                    let crate_name = entry.file_name().to_ascii_lowercase();
                    let src_path = PathBuf::from("./target/userland")
                        .join(&crate_name)
                        .join("x86_64-unknown-none/release/")
                        .join(&crate_name);
                    let target = if crate_name == "init" || crate_name == "shell" {
                        PathBuf::from("./target/iso_root/boot").join(&crate_name)
                    } else {
                        PathBuf::from("./target/iso_root/userland").join(&crate_name)
                    };
                    std::fs::copy(
                        &src_path,
                        &target
                    ).map_err(|e| {
                        eprintln!("{}: {:#?}", e.to_string(), src_path);
                        BuildError::CopyKernelBinaryFailed
                    })?;
                }
            }
        }
        Ok(())
    }
    fn create_limine_iso(&self, config: &Config) -> Result<(), BuildError> {
        if let Some(parent) = Path::new(&config.image_path).parent() {
            std::fs::create_dir_all(parent)
                .map_err(|_| BuildError::CreateDirectoryFailed)?;
        }

        self.create_raw_iso(&config)?;
        //self.install_limine_to_iso(&config)?;
        Ok(())
    }

    fn create_raw_iso(&self, config: &Config) -> Result<(), BuildError> {
        std::process::Command::new("xorriso")
            .arg("-as")
            .arg("mkisofs")
            .arg("-b").arg("boot/limine/limine-bios-cd.bin")
            .arg("-no-emul-boot").arg("-boot-load-size").arg("4").arg("-boot-info-table")
            .arg("--efi-boot").arg("boot/limine/limine-uefi-cd.bin")
            .arg("-efi-boot-part").arg("--efi-boot-image").arg("--protective-msdos-label")
            .arg("target/iso_root")
            .arg("-o").arg(config.image_path.clone())
            .stdout(Stdio::piped())
            .output()
            .map_err(|_| BuildError::CreateLimineIsoFailed)?;
        Ok(())
    }

    fn _install_limine_to_iso(&self, config: &Config) -> Result<(), BuildError> {
        std::process::Command::new("target/limine/limine")
            .arg("bios-install")
            .arg(config.image_path.clone())
            .stdout(Stdio::piped())
            .output()
            .map_err(|e| {
                println!("{e}");
                BuildError::InstallLimineToIsoFailed
            })?;
        Ok(())
    }
}


#[derive(Debug, Error)]
pub enum BuildError {
    #[error("Failed to download OVMF firmware: {0}")]
    DownloadOvmfFirmwareFailed(#[from] reqwest::Error),

    #[error("Failed to clone limine binary file(s)")]
    CloneLimineBinaryFailed,

    #[error("Failed to copy limine.conf")]
    CopyLimineConfigFailed,

    #[error("Failed to copy limine binary file(s)")]
    CopyLimineBinaryFailed,

    #[error("Could not find Cargo.toml file starting from current folder: {0:?}")]
    LocateCargoManifest(#[from] locate_cargo_manifest::LocateManifestError),
    
    #[error("Failed to build the kernel through cargo")]
    CargoBuildFailed,

    #[error("Failed to create the Limine ISO")]
    CreateLimineIsoFailed,

    #[error("Failed to copy kernel binary")]
    CopyKernelBinaryFailed,

    #[error("Failed to create empty image")]
    CreateEmptyImgFailed,

    #[error("Failed to format filesystem image")]
    FormatImgFailed,

    #[error("Failed to add directory to filesystem image")]
    AddImgDirectoryFailed,

    #[error("Failed to add content to filesystem image")]
    AddImgContentFailed,

    #[error("Failed to create directory")]
    CreateDirectoryFailed,

    #[error("Failed to install Limine to the ISO")]
    InstallLimineToIsoFailed,

    #[error("Failed to retrieve cargo metadata")]
    CargoMetadataFailed(#[from] cargo_metadata::Error),

    #[error("Failed to create file: {0}")]
    FileCreationFailed(#[from] std::io::Error),

    #[error("Failed to write to file: {0}")]
    FileWriteFailed(std::io::Error),

    #[error("Limine binary file {0} is missing")]
    LimineBinaryMissing(String),

    
    #[error("Failed to copy from source {0}")]
    CopyOvmfFromSource(String),
}


mod locate_cargo_manifest {
    use std::{convert, env, fmt, io, path::PathBuf, string};
	pub fn locate_manifest() -> Result<PathBuf, LocateManifestError> {
		let cargo = env::var("CARGO").unwrap_or("cargo".to_owned());
		let output = std::process::Command::new(cargo)
			.arg("locate-project")
			.output()?;
		if !output.status.success() {
			return Err(LocateManifestError::CargoExecution { stderr: output.stderr });
		}

		let output = String::from_utf8(output.stdout)?;
		let parsed = json::parse(&output)?;
		let root = parsed["root"].as_str().ok_or(LocateManifestError::NoRoot)?;
		Ok(PathBuf::from(root))
	}

	#[derive(Debug)]
	pub enum LocateManifestError {
		Io(io::Error),
		CargoExecution {
			stderr: Vec<u8>,
		},
		StringConversion(string::FromUtf8Error),
		ParseJson(json::Error),
		NoRoot,
	}

	impl fmt::Display for LocateManifestError {
		fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
			match self {
				LocateManifestError::Io(err) => {
					write!(f, "An I/O error occured while trying to execute `cargo locate-project`: {}", err)
				}
				LocateManifestError::CargoExecution { stderr } => {
					write!(f, "The command `cargo locate-project` dit not exit successfully.\n\
					Stderr: {}", String::from_utf8_lossy(stderr))
				}
				LocateManifestError::StringConversion(err) => {
					write!(f, "The output of `cargo locate-project` was not valid UTF-8: {}", err)
				}
				LocateManifestError::ParseJson(err) => {
					write!(f, "The output of `cargo locate-project` was not valid JSON: {}", err)
				}
				LocateManifestError::NoRoot => {
					write!(f, "The JSON output of `cargo locate-project` did not contain the expected \"root\" string.")
				}
			}
		}
	}

	impl std::error::Error for LocateManifestError {
		fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
			match self {
				LocateManifestError::Io(err) => Some(err),
				LocateManifestError::CargoExecution{stderr: _} => None,
				LocateManifestError::StringConversion(err) => Some(err),
				LocateManifestError::ParseJson(err) => Some(err),
				LocateManifestError::NoRoot => None,

			}
		}
	}

	impl convert::From<io::Error> for LocateManifestError {
		fn from(source: io::Error) -> Self {
			LocateManifestError::Io(source)
		}
	}

	impl convert::From<string::FromUtf8Error> for LocateManifestError {
		fn from(source: string::FromUtf8Error) -> Self {
			LocateManifestError::StringConversion(source)
		}
	}

	impl convert::From<json::Error> for LocateManifestError {
		fn from(source: json::Error) -> Self {
			LocateManifestError::ParseJson(source)
		}
	}
}



