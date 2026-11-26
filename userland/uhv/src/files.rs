
use alloc::fmt;
use bitflags::bitflags;

bitflags! {
	#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
	pub struct InodeMode: u16 {
		const FIFO			= 0x1000;
		const CharDev		= 0x2000;
		const Directory		= 0x4000;
		const BlkDev		= 0x6000;
		const Regular		= 0x8000;
		const SymLink		= 0xA000;
		const Socket		= 0xC000;

		const OtherExecute	= 0x001;
		const OtherWrite	= 0x002;
		const OtherRead		= 0x004;

		const GroupExecute	= 0x008;
		const GroupWrite	= 0x010;
		const GroupRead		= 0x020;

		const UserExecute	= 0x040;
		const UserWrite		= 0x080;
		const UserRead		= 0x100;

		const StickyBit		= 0x200;
		const SetGID		= 0x400;
		const SetUID		= 0x800;
	}
}

bitflags! {
	#[derive(Debug, Clone, Copy)]
	pub struct VfsFileOpenflags: u32 {
		const O_PATH 		= 	0o010000000;
		const O_SYNC		= 	0o004010000;
		///Enable the close-on-exec flag for the new file descriptor.
		///Means that this file is closed when the process runs a exec/ve()
		const O_CLOEXEC		=	0o002000000;
		const O_NOATIME		=	0o001000000;
		const O_NOFOLLOW	=	0o000400000;
		///If the path is not a directory, cause open() to fail.
		const O_DIRECTORY	=	0o000200000;
		const O_LARGEFILE	=	0o000100000;
		const O_DIRECT		=	0o000040000;
		const FASYNC		=	0o000020000;
		const O_DSYNC		=	0o000010000;
		const O_NONBLOCK	=	0o000004000;
		///Open file in append mode. Before each write(), the file offset is positioned at the end of the file, as if with lseek.
		const O_APPEND		=	0o000002000;
		const O_TRUNC		=	0o000001000;
		const O_NOCTTY		=	0o000000400;
		const O_EXCL		=	0o000000200;
		///If the path does not exist, create it as a regular file.
		const O_CREAT		=	0o000000100;
		const O_RDWR		=	0o000000002;
		const O_ACCMODE		=	0o000000003;
		const O_WRONLY		=	0o000000001;
		const O_RDONLY		=	0o000000000;
	}
}

impl InodeMode {
	pub fn to_ls_bytes(self) -> [u8; 10] {
		let bits = self.bits();
		let mut buf = [b'-'; 10];
		buf[0] = match bits & 0xF000 {
			0x1000 => b'p', // FIFO
			0x2000 => b'c', // CharDev
			0x4000 => b'd', // Directory
			0x6000 => b'b', // BlkDev
			0x8000 => b'-', // Regular
			0xA000 => b'l', // SymLink
			0xC000 => b's', // Socket
			_ => b'?',
		};
		if bits & Self::UserRead.bits() != 0 {
			buf[1] = b'r';
		}
		if bits & Self::UserWrite.bits() != 0 {
			buf[2] = b'w';
		}
		buf[3] = match (bits & Self::UserExecute.bits() != 0, bits & Self::SetUID.bits() != 0) {
			(true, true) => b's',
			(false, true) => b'S',
			(true, false) => b'x',
			(false, false) => b'-',
		};

		if bits & Self::GroupRead.bits() != 0 {
			buf[4] = b'r';
		}
		if bits & Self::GroupWrite.bits() != 0 {
			buf[5] = b'w';
		}
		buf[6] = match (bits & Self::GroupExecute.bits() != 0, bits & Self::SetGID.bits() != 0) {
			(true, true) => b's',
			(false, true) => b'S',
			(true, false) => b'x',
			(false, false) => b'-',
		};
		if bits & Self::OtherRead.bits() != 0 {
			buf[7] = b'r';
		}
		if bits & Self::OtherWrite.bits() != 0 {
			buf[8] = b'w';
		}
		buf[9] = match (bits & Self::OtherExecute.bits() != 0, bits & Self::StickyBit.bits() != 0) {
			(true, true) => b't',
			(false, true) => b'T',
			(true, false) => b'x',
			(false, false) => b'-',
		};

		buf
	}
}

impl fmt::Display for InodeMode {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		let bytes = self.to_ls_bytes();
		let s = unsafe {core::str::from_utf8_unchecked(&bytes)};
		f.write_str(s)
	}
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Stat {
    pub st_dev: u64,
    pub st_ino: u64,
    pub st_nlink: u64,
    pub st_mode: u32,
    pub st_uid: u32,
    pub st_gid: u32,
    pub __pad0: u32,
    pub st_rdev: u64,
    pub st_size: i64,
    pub st_blksize: i64,
    pub st_blocks: i64,
    pub st_atime: i64,
    pub st_atime_nsec: i64,
    pub st_mtime: i64,
    pub st_mtime_nsec: i64,
    pub st_ctime: i64,
    pub st_ctime_nsec: i64,
    pub __unused: [i64; 3],
}

