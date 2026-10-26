use crate::vfs::InodeId;
use bitflags::bitflags;

bitflags! {
	struct VfsFileOpenflags: u8 {
		const READ_PERM		=  0b00000001;
		const WRITE_PERM	=  0b00000010;
	}
}


pub struct VfsFile {
	inode: InodeId,
	offset: u64,
	flags: VfsFileOpenflags
}
