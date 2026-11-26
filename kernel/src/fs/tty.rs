use alloc::sync::Arc;

use crate::{drivers::{console::{self}, io::getchar}, errno::KernelErrors, fs::devtmpfs::makedev, vfs::{self, InodeId, file::S_IFCHR, inode::Inode}};


#[derive(Debug)]
pub struct TtyInode { 
	id: InodeId,
	stderr: bool,
}

impl Inode for TtyInode {

	fn read_at(self: alloc::sync::Arc<Self>, _: u64, buf: &mut[u8]) -> Result<usize, crate::errno::KernelErrors> {
		if buf.is_empty() {
			return Ok(0);
		}
		let mut n = 0;
		while n < buf.len() {
			match getchar() {
				Ok(byte) => {
					buf[n] = byte;
					n += 1;
				}
				Err(KernelErrors::EAGAIN) => {
					if n == 0 {
						return Err(KernelErrors::EAGAIN);
					}
					return Ok(n);
				}
				Err(e) => return Err(e),
			}
		}
		Ok(n)
	}

	fn write_at(self: alloc::sync::Arc<Self>, _: u64, buf: &[u8]) -> Result<usize, crate::errno::KernelErrors> {
		if self.stderr {
			console::_console_eprint(format_args!("{}", str::from_utf8(&buf).unwrap_or("<invalid utf8>")));
		} else {
			console::write_bytes(buf);
		}
		Ok(buf.len())
	}

	fn id(&self) -> crate::vfs::InodeId {
		self.id
	}

	fn stat(&self) -> Result<crate::vfs::Stat, crate::errno::KernelErrors> {

		Ok(crate::vfs::Stat {
			st_rdev: makedev(5, 0),
			st_mode: S_IFCHR | 0o666,
			st_ino: self.id(),
			..Default::default()
		})
	}

	fn statfs(&self) -> Result<crate::vfs::superblock::Statfs, crate::errno::KernelErrors> {
		return Err(KernelErrors::EINVAL);
	}
	fn reset(&self) {}
	fn len(&self) -> usize {
		0
	}
	fn itype(&self) -> crate::vfs::inode::VfsInodeType {
		crate::vfs::inode::VfsInodeType::CharDev
	}
	fn is_dir(&self) -> bool {
		false
	}
	fn is_file(&self) -> bool {
		true
	}
	fn lookup(self: alloc::sync::Arc<Self>, _: &str, _: Option<alloc::sync::Arc<crate::vfs::dentry::VfsDentry>>)	-> Result<alloc::sync::Arc<crate::vfs::dentry::VfsDentry>, crate::errno::KernelErrors> {
		return Err(KernelErrors::ENOTDIR);
	}
	fn create(self: alloc::sync::Arc<Self>, _: &str, _: u16) -> Result<alloc::sync::Arc<dyn Inode>, crate::errno::KernelErrors> {
		return Err(KernelErrors::ENOTDIR);
	}
	fn mkdir(self: alloc::sync::Arc<Self>, _: &str, _: u16) -> Result<alloc::sync::Arc<dyn Inode>, crate::errno::KernelErrors> {
		return Err(KernelErrors::ENOTDIR);
	}
	fn mknod(self: alloc::sync::Arc<Self>, _: &str, _: u16, _: crate::vfs::inode::DeviceId) -> Result<(), crate::errno::KernelErrors> {
		return Err(KernelErrors::ENOTDIR);
	}
	fn readdir(self: alloc::sync::Arc<Self>, _: u64) -> Result<Option<crate::vfs::dentry::VfsDirent>, crate::errno::KernelErrors> {
		return Err(KernelErrors::ENOTDIR);
	}
	fn getdents(&self, _: &mut u64, _: usize) -> Result<alloc::vec::Vec<u8>, crate::errno::KernelErrors> {
		return Err(KernelErrors::ENOTDIR);
	}
}

pub fn stdout() -> Arc<TtyInode> {
	Arc::new(TtyInode { id: vfs::inode::NEXT_INO.fetch_add(1, core::sync::atomic::Ordering::Relaxed), stderr: false })
}

pub fn stderr() -> Arc<TtyInode>  {
	Arc::new(TtyInode { id: vfs::inode::NEXT_INO.fetch_add(1, core::sync::atomic::Ordering::Relaxed), stderr: true })

}