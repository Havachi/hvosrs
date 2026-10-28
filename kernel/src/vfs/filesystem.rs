use spin::Mutex;

use crate::{errno::KernelErrors, vfs::dentry::VfsDentry};

///Every future filesystem drivers must implement this trait to work with the Vfs
///Vfs will internally call those function

pub static FST_REG: Mutex<VfsFilesystemRegistry> = Mutex::new(VfsFilesystemRegistry::new());

pub struct VfsFilesystemRegistry {
	fs_types: [Option<VfsRegistredFilesystem>; 16],
	count: usize,
}

impl VfsFilesystemRegistry {
	pub const fn new() -> Self {
		Self {
			fs_types: [ const {None}; 16 ],
			count: 0
		}
	}

	pub fn unregister(&mut self, name: [u8; 16]) -> Result<(), &'static str> {
		for entry in self.fs_types.iter_mut() {
			if let Some(e) = entry {
				if e.name == name {
					*entry = None
				}
			}
		}
		Err("Failed unregistering fs type, not found in registry")
	} 

	pub fn register(&mut self, name: [u8; 16], fs_type: &'static dyn VfsFilesystem) -> Result<(), &'static str> {
		if self.count >= 16 {
			return Err("Failed to register filesystem, registry is full");
		}
		for entry in self.fs_types.iter(){
			if let Some(reg) = entry {
				if reg.name == name {
					return Err("Detected duplicate registration");
				}
			}
		}
		for entry in self.fs_types.iter_mut() {
			if entry.is_none() {
				 *entry = Some(VfsRegistredFilesystem { name, fs_type });
				self.count += 1;
				return Ok(());
			}
		}
		Err("Failed to register fs")
	}

	pub fn find(&self, name: [u8; 16]) -> Result<&'static dyn VfsFilesystem, &'static str>  {
		for fst_e in self.fs_types.iter() {
			if let Some(fs) = fst_e {
				if fs.name == name {
					return Ok(fs.fs_type)
				}
			}
		}
		Err("fs type not found")
	}

}

pub struct VfsRegistredFilesystem {
	pub name: [u8; 16],
	pub fs_type: &'static dyn VfsFilesystem,
}

pub trait VfsFilesystem: Sync + Send {
	fn get_name(&self) -> [u8; 16];
	fn mount(&self, path: &str) -> Result<VfsDentry, KernelErrors>;
}

pub struct VfsFileSystemType {
	id: u32,
	name: &'static str,
	fs: dyn VfsFilesystem,
}


pub fn vfs_register_filesystem(fs_type: &'static dyn VfsFilesystem) -> Result<(), &'static str>{
	let mut reg = FST_REG.lock();
	let name = fs_type.get_name();
	reg.register(name, fs_type)?;
	Ok(())
}

pub fn vfs_unregister_filesystem(fs_type: &'static dyn VfsFilesystem) -> Result<(), &'static str> {
	let mut reg = FST_REG.lock();
	let name = fs_type.get_name();
	let _ = reg.unregister(name)?;
	Ok(())
}