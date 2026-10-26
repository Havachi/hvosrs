use crate::vfs::InodeId;

pub struct VfsDentry {
	name: [u8; 255],
	inode: InodeId,
}

impl VfsDentry {
	pub fn new(name: [u8; 255], inode: InodeId) -> Self {
		Self {
			name,
			inode,
		}
	}
}