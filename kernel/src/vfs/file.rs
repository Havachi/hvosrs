use crate::vfs::inode::Inode;
use core::fmt::Debug;
use alloc::sync::Arc;
use bitflags::bitflags;

bitflags! {
	#[derive(Debug, Clone, Copy)]
	pub struct VfsFileOpenflags: u32 {
		const O_PATH 		= 	0o010000000;
		const O_SYNC		= 	0o004010000;
		const O_CLOEXEC		=	0o002000000;
		const O_NOATIME		=	0o001000000;
		const O_NOFOLLOW	=	0o000400000;
		const O_DIRECTORY	=	0o000200000;
		const O_LARGEFILE	=	0o000100000;
		const O_DIRECT		=	0o000040000;
		const FASYNC		=	0o000020000;
		const O_DSYNC		=	0o000010000;
		const O_NONBLOCK	=	0o000004000;
		const O_APPEND		=	0o000002000;
		const O_TRUNC		=	0o000001000;
		const O_NOCTTY		=	0o000000400;
		const O_EXCL		=	0o000000200;
		const O_CREAT		=	0o000000100;
		const O_RDWR		=	0o000000002;
		const O_ACCMODE		=	0o000000003;
		const O_WRONLY		=	0o000000001;
		const O_RDONLY		=	0o000000000;
	}
}

#[derive(Clone)]
pub struct VfsFile {
	pub inode: Arc<dyn Inode>,
	pub offset: u64,
	pub flags: VfsFileOpenflags
}

impl VfsFile {
	pub fn new(inode: Arc<dyn Inode>, offset: u64, flags: VfsFileOpenflags) -> Self {
		Self { inode, offset, flags }
	}
	pub fn len(&self) -> usize {
		self.inode.len()
	} 
}

impl Debug for VfsFile {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		f.debug_struct("VfsFile").field("inode", &self.inode.id()).field("offset", &self.offset).field("flags", &self.flags).finish()
	}
}