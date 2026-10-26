use crate::vfs::{dentry::VfsDentry, filesystem::VfsFileSystemType, inode::VfsInode};


pub struct VfsSuperblock {
	block_size: usize,
	max_file_size: usize,
	fs_type: VfsFileSystemType,
	root: VfsDentry,

}


impl VfsSuperblock {
	pub fn alloc_inode(&mut self) {}
	pub fn free_inode(&mut self, inode: &VfsInode) {}
	pub fn write_inode(inode: &VfsInode) {}
}