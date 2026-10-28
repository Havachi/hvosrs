use core::sync::atomic::AtomicU64;

use alloc::sync::Arc;
use crate::{errno::KernelErrors, vfs::{InodeId, dentry::VfsDentry}};

//Start at one because 0 is root
pub static NEXT_INO: AtomicU64 = AtomicU64::new(1);

#[derive(PartialEq, Eq)]
pub enum VfsInodeType {
	File,
	Dir,
}

pub struct VfsInode {
	id: InodeId,
	vtype: VfsInodeType,
	size: usize,
}

impl VfsInode {
	pub fn new(id: InodeId, vtype: VfsInodeType, size: usize) -> Self {
		Self { id, vtype, size }
	}
}

pub trait Inode: Send + Sync {
	fn lookup(self: Arc<Self>, name: &str, parent: Option<Arc<VfsDentry>>)	-> Result<Arc<VfsDentry>, KernelErrors>;
	fn create(self: Arc<Self>, basename: &str)	-> Result<(), KernelErrors>;
	fn mkdir(self: Arc<Self>, name: &str) -> Result<(), KernelErrors>;
	fn read_at(self: Arc<Self>, offset: u64, buf: &mut[u8]) -> Result<usize, KernelErrors>;
	fn write_at(self: Arc<Self>, offset: u64, buf: &[u8]) -> Result<usize, KernelErrors>;
 	fn is_dir(&self) -> bool;
 	fn is_file(&self) -> bool;
	fn id(&self) -> InodeId;
	fn len(&self) -> usize;
	fn reset(&self);
}