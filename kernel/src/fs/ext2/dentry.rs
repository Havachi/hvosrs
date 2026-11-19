
pub struct Ext2Dentry {
	d_inode: u32,
	d_size: u16,
	d_name_len: u8,
	d_type: Ext2DentryType,
	d_name: [u8]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum Ext2DentryType {
	Unknown = 0,
	Regular = 1,
	Directory = 2,
	CharDev = 3,
	BlockDev = 4,
	FIFO = 5,
	Socket = 6,
	Symlink =  7
}

