use core::{cmp::min, sync::atomic::AtomicUsize};

use alloc::{boxed::Box, collections::BTreeMap, sync::Arc, vec::Vec};
use spin::Mutex;
use crate::{errno::KernelErrors::{self, EEXIST, EINVAL, ENOTDIR}, kerror, vfs::{InodeId, dentry::VfsDentry, filesystem::VfsFilesystem, inode::{Inode, VfsInodeType::{self, Dir, File}}, next_inode_id}};

pub type RamfsDir = BTreeMap<Box<str>, Arc<RamFsInode>>;
pub type RamfsFile = Vec<u8>;

pub const RAMFS_NAME: [u8;16] = [b'r',b'a',b'm',b'f',b's',0,0,0,0,0,0,0,0,0,0,0];


pub struct RamFsInode {
	id: InodeId,
	vtype: VfsInodeType,
	size: AtomicUsize,
	dir: Option<Mutex<RamfsDir>>,
	file: Option<Mutex<RamfsFile>>,
}

impl RamFsInode {
	pub fn new_root() -> Arc<Self> {
		Arc::new(
				Self {
				id: 0,
				vtype: Dir,
				size: AtomicUsize::new(0),
				dir: Some(Mutex::new(RamfsDir::new())),
				file:None
			}
		)
	}

	pub fn new_dir(id: InodeId) -> Arc<Self> {
		Arc::new(
			Self {
				id,
				vtype: Dir,
				size: AtomicUsize::new(0),
				dir: Some(Mutex::new(RamfsDir::new())),
				file: None
			}
		)
	}
	pub fn new_file(id: InodeId) -> Arc<Self> {
		Arc::new(
			Self {
				id,
				vtype: File,
				size: AtomicUsize::new(0),
				dir: None,
				file: Some(Mutex::new(Vec::<u8>::new()))
			}
		)
	}
}

impl Inode for RamFsInode {
	fn lookup(self: Arc<Self>, name: &str, parent: Option<Arc<VfsDentry>>) -> Result<Arc<VfsDentry>, crate::errno::KernelErrors> {
		if !self.is_dir() {
			return Err(KernelErrors::ENOTDIR);
		}

		if self.dir.is_none() {
			return Err(KernelErrors::ENOTDIR);
		}

		let dir = self.dir.as_ref().unwrap();

		let res = dir.lock();

		if let Some(ino) = res.get(name) {
			let name_bytes = name.as_bytes();
			if name_bytes.len() > 255 {
				return Err(EINVAL);
			}
			return Ok(Arc::new(VfsDentry::from_name(name, ino.clone(), parent)?))
		} else {
			return Err(KernelErrors::ENOENT);
		}
	}

	fn is_dir(&self) -> bool {
		self.vtype == Dir
	}
	fn is_file(&self) -> bool {
		self.vtype == File
	}
	fn id(&self) -> InodeId {
		self.id
	}

	fn len(&self) -> usize {
		self.size.load(core::sync::atomic::Ordering::Relaxed)
	}

	fn reset(&self) {
		self.size.store(0, core::sync::atomic::Ordering::Relaxed);
		self.file.as_ref().unwrap().lock().clear();
	}

	fn create(self: Arc<Self>, basename: &str) -> Result<(), KernelErrors> {
		if !self.is_dir() || self.dir.is_none() {
			return Err(ENOTDIR);
		}
		if basename.is_empty() || basename.len() > 255 || basename.contains('/') {
			return Err(EINVAL);
		}
		let mut dir = self.dir.as_ref().unwrap().lock();
		if dir.contains_key(basename){
			return Err(EEXIST);
		}
		let next_id = next_inode_id();
		let child = RamFsInode::new_file(next_id);
		dir.insert(basename.into(), child);

		Ok(())
	}
	
	fn mkdir(self: Arc<Self>, basename: &str) -> Result<(), KernelErrors> {
		if !self.is_dir() || self.dir.is_none() {
			kerror!("Parent is not a dir");
			return Err(ENOTDIR);
		}
		if basename.is_empty() || basename.len() > 255 || basename.contains('/') {
			kerror!("Invalid name: '{}'", basename);
			return Err(EINVAL);
		}
		let mut dir = self.dir.as_ref().unwrap().lock();
		if dir.contains_key(basename){
			kerror!("'{}' Already exists", basename);
			return Err(EEXIST);
		}
		let next_id = next_inode_id();
		let child = RamFsInode::new_dir(next_id);
		dir.insert(basename.into(), child);

		Ok(())
	}

	fn read_at(self: Arc<Self>, offset: u64, buf: &mut[u8]) -> Result<usize, KernelErrors> {
		if !self.is_file() || self.file.is_none() { return Err(EINVAL); }
		let data = self.file.as_ref().unwrap().lock();
		let data_len = data.len();

		let start = offset as usize;
		if offset as usize >= data_len {
			return Ok(0);
		}
		let to_read = min(buf.len(), data_len - start);
		buf[..to_read].copy_from_slice(&data[start..start+to_read]);
		Ok(to_read)
	}

	fn write_at(self: Arc<Self>, offset: u64, buf: &[u8]) -> Result<usize, KernelErrors> {
		if !self.is_file() || self.file.is_none() { return Err(EINVAL); }
		let mut data = self.file.as_ref().unwrap().lock();
		let start = offset as usize;
		let end = start.saturating_add(buf.len());

		if data.len() < end {
			data.resize(end, 0);
		}
		data[start..end].copy_from_slice(buf);
		self.size.store(data.len(), core::sync::atomic::Ordering::Relaxed);
		Ok(buf.len())
	}
}

pub struct RamFs;

impl VfsFilesystem for RamFs {
	fn get_name(&self) -> [u8; 16] {
		RAMFS_NAME
	}

	fn mount(&self, path: &str) -> Result<crate::vfs::dentry::VfsDentry, KernelErrors> {
		if path != "/" {
			return Err(EINVAL);
		}
		let mut name: [u8;255] = [0;255];
		name[..1].copy_from_slice(b"/");
		VfsDentry::from_name(
			"/", 
			RamFsInode::new_root(), 
			None
		)
	}
}

