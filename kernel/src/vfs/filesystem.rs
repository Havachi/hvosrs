use alloc::{string::String, sync::Arc, vec::Vec};
use spin::Mutex;

use crate::{errno::KernelErrors, vfs::{dentry::VfsDentry, inode::DeviceId, superblock::Superblock}};

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

	pub fn unregister(&mut self, name: [u8; 16]) -> Result<(), KernelErrors> {
		for entry in self.fs_types.iter_mut() {
			if let Some(e) = entry {
				if e.name == name {
					*entry = None;
					self.count -= 1;
					return Ok(());
				}
			}
		}
		Err(KernelErrors::EINVAL)
	} 

	pub fn register(&mut self, name: [u8; 16], fs_type: &'static dyn VfsFilesystem) -> Result<(), KernelErrors> {
		if self.count >= 16 {
			return Err(KernelErrors::EBUSY);
		}
		for entry in self.fs_types.iter(){
			if let Some(reg) = entry {
				if reg.name == name {
					return Err(KernelErrors::EBUSY);
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
		Err(KernelErrors::EINVAL)
	}

	pub fn find(&self, name: [u8; 16]) -> Result<&'static dyn VfsFilesystem, KernelErrors>  {
		for fst_e in self.fs_types.iter() {
			if let Some(fs) = fst_e {
				if fs.name == name {
					return Ok(fs.fs_type)
				}
			}
		}
		Err(KernelErrors::EINVAL)
	}

	pub fn find_id(&self, name: [u8; 16]) -> Result<u64, KernelErrors> {
		for (i, fst_e) in self.fs_types.iter().enumerate() {
			if let Some(fs) = fst_e {
				if fs.name == name {
					return Ok(i as u64)
				}
			}
		}
		Err(KernelErrors::EINVAL)
	}

}

pub struct VfsRegistredFilesystem {
	pub name: [u8; 16],
	pub fs_type: &'static dyn VfsFilesystem,
}

pub trait VfsFilesystem: Sync + Send {
	fn get_name(&self) -> [u8; 16];
	fn mount(&self, dev: Option<DeviceId>) -> Result<Arc<dyn Superblock>, KernelErrors>;
}

pub fn vfs_register_filesystem(fs_type: &'static dyn VfsFilesystem) -> Result<(), KernelErrors>{
	let mut reg = FST_REG.lock();
	let name = fs_type.get_name();
	reg.register(name, fs_type)?;
	Ok(())
}

pub fn vfs_unregister_filesystem(fs_type: &'static dyn VfsFilesystem) -> Result<(), KernelErrors> {
	let mut reg = FST_REG.lock();
	let name = fs_type.get_name();
	let _ = reg.unregister(name)?;
	Ok(())
}

pub struct VfsMount {
	pub mountpoint: String,
	pub mountpoint_dentry: Option<Arc<VfsDentry>>,
	pub fs_name: [u8; 16],
	pub sb: Arc<dyn Superblock>,
	pub root_dentry: Arc<VfsDentry>,
}

pub static VFS_MOUNTS: Mutex<Vec<VfsMount>> = Mutex::new(Vec::new());

