use crate::vfs::InodeId;

use bitflags::bitflags;

pub enum VfsInodeType {
	File,
	Dir,
}

pub struct VfsInode {
	id: InodeId,
	vtype: VfsInodeType,
	size: usize,
}

pub trait VfsInodeOps {
	fn create();
	fn lookup();
	fn link();
	fn unlink();
	fn symlink();
	fn mkdir();
	fn rmdir();
}