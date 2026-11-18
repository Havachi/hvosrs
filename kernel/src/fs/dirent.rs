use crate::vfs::inode::VfsInodeType;



#[repr(u8)]
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
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

impl From<VfsInodeType> for DirentTypes {
	fn from(value: VfsInodeType) -> Self {
		match value {
			VfsInodeType::Unknown => DirentTypes::Unknown,
			VfsInodeType::FIFO => DirentTypes::FIFO,
			VfsInodeType::CharDev => DirentTypes::CharDev,
			VfsInodeType::Directory => DirentTypes::Directory,
			VfsInodeType::BlockDev => DirentTypes::BlockDev,
			VfsInodeType::Regular => DirentTypes::Regular,
			VfsInodeType::Symlink => DirentTypes::Symlink,
			VfsInodeType::Socket => DirentTypes::Socket,
		}
	}
}

impl From<u8> for DirentTypes {
	fn from(value: u8) -> Self {
		match value {
			1  => DirentTypes::FIFO,
			2  => DirentTypes::CharDev,
			4  => DirentTypes::Directory,
			6  => DirentTypes::BlockDev,
			8  => DirentTypes::Regular,
			10 => DirentTypes::Symlink,
			12 => DirentTypes::Socket,
			_  => DirentTypes::Unknown 
		}
	}
}