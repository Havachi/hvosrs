use core::mem::MaybeUninit;

use alloc::sync::Arc;

use crate::{errno::KernelErrors::{self, EINVAL}, vfs::{InodeId, inode::Inode}};

pub struct VfsDentry {
	pub name: [u8; 255],
	pub inode: Arc<dyn Inode>,
	pub parent: Option<Arc<VfsDentry>>
}

impl VfsDentry {
	pub fn new(name: [u8; 255], inode: Arc<dyn Inode>, parent: Option<Arc<VfsDentry>>) -> Self {
		Self {
			name,
			inode,
			parent
		}
	}
	
	pub fn from_name(name_str: &str, inode: Arc<dyn Inode>, parent: Option<Arc<VfsDentry>>) -> Result<Self, KernelErrors> {
		if name_str.len() > 255 {
			return Err(EINVAL);
		}

		let bytes = name_str.as_bytes();
		let mut buf: MaybeUninit<[u8; 255]> = MaybeUninit::uninit();
		let ptr = buf.as_mut_ptr() as *mut u8;
		unsafe {
			core::ptr::copy_nonoverlapping(
				bytes.as_ptr(), ptr, bytes.len());

			for i in bytes.len()..255 {
				core::ptr::write(ptr.add(i), 0);
			}
		}
		let name = unsafe { buf.assume_init() };
		Ok(VfsDentry::new(name, inode, parent))
	}

	pub fn get_inode_id(&self) -> InodeId {
		self.inode.id()
	}
}