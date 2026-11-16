
use bitflags::bitflags;

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
