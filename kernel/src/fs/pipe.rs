use alloc::{boxed::Box, sync::{Arc, Weak}};
use spin::Mutex;

use crate::{errno::KernelErrors, structures::buffers::CircularBuffer, vfs::inode::Inode};



#[derive(Debug)]
pub struct PipeInner {
	pub buf: Mutex<Box<CircularBuffer<u8, 4096>>>,
	read: Mutex<Weak<PipeInode>>,
	write: Mutex<Weak<PipeInode>>,
}

#[derive(Debug)]
pub struct PipeInode {
	inner: Arc<PipeInner>,
	_side: PipeSide,
}


impl Inode for PipeInode {
		
	fn read_at(self: Arc<Self>, _: u64, buf: &mut[u8]) -> Result<usize, crate::errno::KernelErrors> {
		if buf.is_empty() {
			return Ok(0);
		}
		let mut ring = self.inner.buf.lock();
		let mut n = 0;
		while n < buf.len() {
			match ring.pop() {
				Ok(byte) => {
					buf[n] = byte;
					n += 1;
				}
				Err(KernelErrors::EAGAIN) => break,
				Err(e) => return Err(e),
			}
		}
		if n > 0 {
			return Ok(n);
		}
		if self.inner.write.lock().strong_count() == 0 {
			Ok(0)
		} else {
			Err(KernelErrors::EAGAIN)
		}
	}
	
	fn write_at(self: Arc<Self>, _: u64, buf: &[u8]) -> Result<usize, crate::errno::KernelErrors> {
		if buf.is_empty() {
			return Ok(0);
		}
		let mut ring = self.inner.buf.lock();
		let mut n = 0;

		while n < buf.len() {
			if self.inner.read.lock().strong_count() == 0 {
				break;
			}
			if ring.push(buf[n]).is_err() {
				break;
			}
			n += 1;
		}
		if n > 0 {
			return Ok(n);
		}
		if self.inner.read.lock().strong_count() == 0 {
			Err(KernelErrors::EPIPE)
		} else {
			Err(KernelErrors::EAGAIN)
		}
	}
	
	fn id(&self) -> crate::vfs::InodeId {
		todo!()
	}
	
	fn stat(&self) -> Result<crate::vfs::Stat, crate::errno::KernelErrors> {
		todo!()
	}

	fn itype(&self) -> crate::vfs::inode::VfsInodeType {
		crate::vfs::inode::VfsInodeType::FIFO
	}

	fn statfs(&self) -> Result<crate::vfs::superblock::Statfs, crate::errno::KernelErrors> {
		return Err(KernelErrors::EINVAL);
	}
	fn reset(&self) {}
	fn len(&self) -> usize {0}
	fn is_dir(&self) -> bool {false}
	fn is_file(&self) -> bool {true}
	fn lookup(self: alloc::sync::Arc<Self>, _: &str, _: Option<alloc::sync::Arc<crate::vfs::dentry::VfsDentry>>) -> Result<alloc::sync::Arc<crate::vfs::dentry::VfsDentry>, crate::errno::KernelErrors> {
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

#[derive(Debug)]
pub enum PipeSide {
	Read, Write,
}

pub fn pair() -> (Arc<PipeInode>, Arc<PipeInode>) {
	let inner = Arc::new(PipeInner {
		buf: spin::Mutex::new(empty_ring()),
		read: spin::Mutex::new(Weak::new()),
		write: spin::Mutex::new(Weak::new()),
	});
	let read = Arc::new(PipeInode { inner: Arc::clone(&inner), _side: PipeSide::Read });
	let write = Arc::new(PipeInode { inner: Arc::clone(&inner), _side: PipeSide::Write });
	*inner.read.lock() = Arc::downgrade(&read);
	*inner.write.lock() = Arc::downgrade(&write);
	(read, write)
}

fn empty_ring() -> Box<CircularBuffer<u8, 4096>> {
	let mut ring = Box::<CircularBuffer<u8, 4096>>::new_uninit();
	unsafe {
		core::ptr::write_bytes(ring.as_mut_ptr(), 0, 1);
		ring.assume_init()
	}
}