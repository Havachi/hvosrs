use alloc::sync::Arc;

use crate::{errno::KernelErrors, vfs::inode::Inode};


pub struct Statfs {
	pub fs_type: u64,
	pub blocksize: usize,
	pub total_blocks: u64,
	pub free_blocks: u64,
	pub total_inodes: u64,
	pub free_inodes: u64,
}

pub trait Superblock: Send + Sync {
	fn root(&self) -> Arc<dyn Inode>;
	fn sync(&self) -> Result<(), KernelErrors>{
		Ok(())
	}

	fn statfs(&self) -> Result<Statfs, KernelErrors>;
	fn unmount(&self) -> Result<(), KernelErrors> {
		self.sync()
	}
}

