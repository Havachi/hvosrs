use core::{fmt::{Debug, Display}, sync::atomic::AtomicU64};

use alloc::{sync::Arc, vec::Vec};
use crate::{errno::KernelErrors, vfs::{InodeId, Stat, dentry::{VfsDentry, VfsDirent}, superblock::Statfs}};

//Start at one because 0 is root
pub static NEXT_INO: AtomicU64 = AtomicU64::new(1);

#[derive(PartialEq, Eq, Debug, Clone, Copy, PartialOrd, Ord)]
pub enum VfsInodeType {
	Unknown = 0,
	FIFO = 1,
	CharDev = 2,
	Directory = 4,
	BlockDev = 6,
	Regular = 8,
	Symlink = 10,
	Socket = 12,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DeviceId {
	pub major: u16,
	pub minor: u16,
}

impl Display for DeviceId {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		write!(f, "{}:{}", self.major, self.minor)
	}
}

pub trait Inode: Send + Sync + Debug {
	fn lookup(self: Arc<Self>, name: &str, parent: Option<Arc<VfsDentry>>)	-> Result<Arc<VfsDentry>, KernelErrors>;
	fn create(self: Arc<Self>, basename: &str) -> Result<(), KernelErrors>;
	fn mknod(self: Arc<Self>, name: &str, mode: u16, dev_id: DeviceId) -> Result<(), KernelErrors>;
	fn mkdir(self: Arc<Self>, name: &str) -> Result<Arc<dyn Inode>, KernelErrors>;
	fn read_at(self: Arc<Self>, offset: u64, buf: &mut[u8]) -> Result<usize, KernelErrors>;
	fn write_at(self: Arc<Self>, offset: u64, buf: &[u8]) -> Result<usize, KernelErrors>;
 	fn is_dir(&self) -> bool;
 	fn is_file(&self) -> bool;
	fn id(&self) -> InodeId;
	fn statfs(&self) -> Result<Statfs, KernelErrors>;
	fn len(&self) -> usize;
	fn itype(&self) -> VfsInodeType;
	fn reset(&self);
	fn readdir(self: Arc<Self>, index: u64) -> Result<Option<VfsDirent>, KernelErrors>;
	fn getdents(&self, offset: &mut u64, max_bytes: usize) -> Result<Vec<u8>, KernelErrors>;
	fn stat(&self) -> Result<Stat, KernelErrors>;
	fn dev_id(&self) -> Option<DeviceId> {
		None
	}
}

