use alloc::sync::Arc;

use crate::{errno::KernelErrors, vfs::{dentry::VfsDentry, inode::{Inode, VfsInode}}};


pub struct VfsSuperblock {
	block_size: usize,
	max_file_size: usize,
	fs_type_id: u32,
	root: VfsDentry,

}

pub trait SuperOperation {
	fn alloc_inode(&mut self) -> Result<Option<Arc<dyn Inode>>, KernelErrors>;
	fn destroy_inode(&mut self, inode: Arc<dyn Inode>) -> Result<(), KernelErrors>;
	fn free_inode(&mut self, inode: Arc<dyn Inode>) -> Result<(), KernelErrors>;
	fn dirty_inode(&mut self, inode: Arc<dyn Inode>) -> Result<(), KernelErrors>;
}

impl VfsSuperblock {
	pub fn alloc_inode(&mut self) {}
	pub fn free_inode(&mut self, inode: &VfsInode) {}
	pub fn write_inode(inode: &VfsInode) {}
}