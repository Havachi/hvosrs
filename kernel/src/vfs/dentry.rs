use core::mem::MaybeUninit;

use alloc::{boxed::Box, collections::BTreeMap, string::String, sync::Arc, vec::Vec};
use spin::RwLock;

use crate::{errno::KernelErrors::{self, EINVAL}, fs::dirent::DirentTypes, vfs::{InodeId, inode::Inode}};

pub struct VfsDentry {
	pub name: [u8; 255],
	pub inode: Arc<dyn Inode>,
	pub parent: Option<Arc<VfsDentry>>,
	pub children: RwLock<BTreeMap<Box<str>, Arc<VfsDentry>>>,
	pub mounted_root: RwLock<Option<Arc<VfsDentry>>>,
}

impl VfsDentry {
	pub fn new(name: [u8; 255], inode: Arc<dyn Inode>, parent: Option<Arc<VfsDentry>>) -> Self {
		Self {
			name,
			inode,
			parent,
			children: RwLock::new(BTreeMap::new()),
			mounted_root: RwLock::new(None),
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

	pub fn lookup_child(self: &Arc<Self>, name: &str) -> Result<Arc<VfsDentry>, KernelErrors> {
		if let Some(cached) = self.children.read().get(name){
			return Ok(cached.clone());
		}

		let dentry = self.inode.clone().lookup(name, Some(self.clone()))?;
		let mut children = self.children.write();
		let entry = children.entry(name.into()).or_insert(dentry);
		Ok(entry.clone())
	}

	pub fn invalidate_child(&self, name: &str) {
		self.children.write().remove(name);
	}

	pub fn set_mounted_root(&self, root: Option<Arc<VfsDentry>>) {
		*self.mounted_root.write() = root;
	}

	pub fn follow_mount(self: &Arc<Self>) -> Arc<VfsDentry> {
		if let Some(ref mnt_root) = *self.mounted_root.read() {
			return mnt_root.clone();
		}
		self.clone()
	}

	pub fn clear_cache_recursive(&self) {
		let children = core::mem::take(&mut *self.children.write());
		for (_, child) in children {
			child.clear_cache_recursive();
		}
	}

	pub fn get_inode_id(&self) -> InodeId {
		self.inode.id()
	}
}

pub const DIRENT_HEADER_SIZE: usize = 19;

pub struct VfsDirent {
	pub ino: InodeId,
	pub off: u64,
	pub d_type: DirentTypes,
	pub name: alloc::string::String,
}

impl VfsDirent {
	pub fn new(ino: InodeId, off: u64,  d_type: DirentTypes, name: String) -> Self {
		Self {
			ino, off, d_type, name
		}
	}

	pub fn reclen(&self) -> usize {
		(DIRENT_HEADER_SIZE + self.name.len() + 1).next_multiple_of(8)
	}

	pub fn write_to_buf(&self, buf: &mut [u8]) {
		let reclen = self.reclen();
		buf[0..8].copy_from_slice(&self.ino.to_le_bytes());
		buf[8..16].copy_from_slice(&self.off.to_le_bytes());
		buf[16..18].copy_from_slice(&(reclen as u16).to_le_bytes());
		buf[18] = self.d_type as u8;
		buf[19..19 + self.name.len()].copy_from_slice(self.name.as_bytes());
	}
}

pub fn try_push_dirent(
	list: &mut Vec<u8>,
	idx: &mut u64,
	max_bytes: usize,
	entry: VfsDirent,
) -> Result<bool, KernelErrors> {
	let reclen = entry.reclen();
	if list.len() + reclen > max_bytes {
		if list.is_empty() {
			return Err(KernelErrors::EINVAL);
		}
		return Ok(false);
	}
	let start = list.len();
	list.resize(start+reclen, 0);
	entry.write_to_buf(&mut list[start..]);
	*idx += 1;
	Ok(true)
}