

#[repr(u8)]
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DirentTypes {
	Unknown = 0,
	FIFO = 1,
	CharDev = 2,
	Directory = 4,
	BlockDev = 6,
	Regular = 8,
	Symlink = 10,
	Socket = 12,
}