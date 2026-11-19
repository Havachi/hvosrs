use core::{cmp::min, sync::atomic::{AtomicU64, AtomicUsize}};

use alloc::{boxed::Box, collections::BTreeMap, string::String, sync::{Arc, Weak}, vec::Vec};
use spin::Mutex;
use crate::{errno::KernelErrors::{self, EEXIST, EINVAL, ENOTDIR}, fs::dirent::DirentTypes, memory::PAGE_SIZE, vfs::{InodeId, Stat, dentry::{VfsDentry, VfsDirent, try_push_dirent}, file::{S_IFDIR, S_IFREG}, filesystem::{FST_REG, VfsFilesystem, vfs_register_filesystem}, inode::{DeviceId, Inode, VfsInodeType}, superblock::{Statfs, Superblock}}};

pub type RamfsDir = BTreeMap<Box<str>, Arc<RamFsInode>>;
pub type RamfsFile = Vec<u8>;

pub const RAMFS_NAME: [u8;16] = [b'r',b'a',b'm',b'f',b's',0,0,0,0,0,0,0,0,0,0,0];

pub struct RamfsSuperblock {
	root: Arc<RamFsInode>,
	next_ino: AtomicU64,
}

impl RamfsSuperblock {
	pub fn new() -> Arc<Self> {
		Arc::new_cyclic(|weak_sb| {
			let root = RamFsInode::new_root(weak_sb.clone());
			Self {
				root,
				next_ino: AtomicU64::new(1),
			}
		})
	}

	pub fn alloc_inode_id(&self) -> InodeId {
		self.next_ino.fetch_add(1, core::sync::atomic::Ordering::Relaxed)
	}
}

impl Superblock for RamfsSuperblock {
	fn root(&self) -> Arc<dyn Inode> {
		self.root.clone()
	}

	fn statfs(&self) -> Result<crate::vfs::superblock::Statfs, KernelErrors> {
		Ok(crate::vfs::superblock::Statfs {
			fs_type: FST_REG.lock().find_id(RAMFS_NAME)?,
			blocksize: PAGE_SIZE,
			total_blocks: 0,
			free_blocks: 0,
			total_inodes: self.next_ino.load(core::sync::atomic::Ordering::Relaxed),
			free_inodes: 0
		})
	}
}

#[derive(Debug)]
pub struct RamFsInode {
	id: InodeId,
	vtype: VfsInodeType,
	size: AtomicUsize,
	dir: Option<Mutex<RamfsDir>>,
	file: Option<Mutex<RamfsFile>>,
	parent_id: InodeId,
	sb: Weak<RamfsSuperblock>,
}

unsafe impl Send for RamFsInode {}
unsafe impl Sync for RamFsInode {}

impl RamFsInode {
	pub fn new_root(sb: Weak<RamfsSuperblock>) -> Arc<Self> {
		Arc::new(
				Self {
				id: 0,
				vtype: VfsInodeType::Directory,
				size: AtomicUsize::new(0),
				dir: Some(Mutex::new(RamfsDir::new())),
				file:None,
				parent_id: 0,
				sb
			}
		)
	}

	pub fn new_dir(id: InodeId, parent_id: u64, sb: Weak<RamfsSuperblock>) -> Arc<Self> {
		Arc::new(
			Self {
				id,
				vtype: VfsInodeType::Directory,
				size: AtomicUsize::new(0),
				dir: Some(Mutex::new(RamfsDir::new())),
				file: None,
				parent_id: parent_id,
				sb
			}
		)
	}
	pub fn new_file(id: InodeId, parent_id: u64, sb: Weak<RamfsSuperblock>) -> Arc<Self> {
		Arc::new(
			Self {
				id,
				vtype: VfsInodeType::Regular,
				size: AtomicUsize::new(0),
				dir: None,
				file: Some(Mutex::new(Vec::<u8>::new())),
				parent_id: parent_id,
				sb
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
		self.vtype == VfsInodeType::Directory
	}
	fn is_file(&self) -> bool {
		self.vtype == VfsInodeType::Regular
	}
	fn id(&self) -> InodeId {
		self.id
	}
	fn statfs(&self) -> Result<Statfs, KernelErrors> {
		self.sb.upgrade().unwrap().statfs()
	}

	fn stat(&self) -> Result<crate::vfs::Stat, KernelErrors> {
		let mode = if self.is_dir() {
			S_IFDIR | 0o755
		} else {
			S_IFREG | 0o644
		};

		let size = self.len() as i64;
		Ok(Stat {
			st_ino: self.id(),
			st_nlink: 1,
			st_mode: mode,
			st_size: size,
			st_blksize: 4096,
			st_blocks: (size + 511) / 512,
			..Default::default()
		})
	}

	fn len(&self) -> usize {
		self.size.load(core::sync::atomic::Ordering::Relaxed)
	}

	fn itype(&self) -> VfsInodeType {
		self.vtype
	}

	fn reset(&self) {
		if let Some(file) = &self.file {
			self.size.store(0, core::sync::atomic::Ordering::Relaxed);
			file.lock().clear();
		}
	}

	fn mknod(self: Arc<Self>, _name: &str, _mode: u16, _dev_id: crate::vfs::inode::DeviceId) -> Result<(), KernelErrors> {
		return Ok(());	
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
		let sb = self.sb.upgrade().ok_or(KernelErrors::EIO)?;
		let next_id = sb.alloc_inode_id();
		let child = RamFsInode::new_file(next_id, self.id, self.sb.clone());
		dir.insert(basename.into(), child);

		Ok(())
	}
	
	fn mkdir(self: Arc<Self>, basename: &str) -> Result<Arc<dyn Inode>, KernelErrors> {
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
		
		let sb = self.sb.upgrade().ok_or(KernelErrors::EIO)?;
		let next_id = sb.alloc_inode_id();
		let child = RamFsInode::new_dir(next_id, self.id, self.sb.clone());
		dir.insert(basename.into(), child.clone());
		Ok(child)
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
	
	fn readdir(self: Arc<Self>, index: u64) -> Result<Option<VfsDirent>, KernelErrors> {
		if self.is_file() {
			return Err(KernelErrors::ENOTDIR);
		}
		if index == 0 {
			return Ok(Some(VfsDirent::new(self.id, 0,  DirentTypes::Directory, String::from("."))))
		} else if index == 1 {
			return Ok(Some(VfsDirent::new(self.parent_id, 0,DirentTypes::Directory, String::from(".."))))
		} else {
			if let Some(d) = self.dir.as_ref() {
				let parent = d.lock();
				let ent = parent.iter().nth((index - 2) as usize);
				if let Some((key, value)) = ent {
					let dirent = VfsDirent::new(
						value.id,
						0,
						if value.is_dir() { DirentTypes::Directory } else { DirentTypes::Regular },
						String::from(key.as_ref()),
					);
					drop(parent);
					return Ok(Some(dirent))
				} else {
					return Ok(None);
				}
			}
		}
		Ok(None)
	}
	fn getdents(&self, offset: &mut u64, max_bytes: usize) -> Result<Vec<u8>, KernelErrors> {
		if !self.is_dir() {
			return Err(KernelErrors::ENOTDIR);
		}
		let mut list: Vec<u8> = Vec::new();
		let mut idx = *offset;

		if idx == 0 {
			let dot = VfsDirent::new(self.id, 1, DirentTypes::Directory, String::from("."));
			if !try_push_dirent(&mut list, &mut idx, max_bytes, dot)? {
				*offset = idx;
				return Ok(list);
			}
		}
		if idx == 1 {
			let dotdot = VfsDirent::new(self.parent_id, 2, DirentTypes::Directory, String::from(".."));
			if !try_push_dirent(&mut list, &mut idx, max_bytes, dotdot)? {
				*offset = idx;
				return Ok(list);
			}
		}

		if let Some(dir_lock) = self.dir.as_ref() {
			let entries = dir_lock.lock();
			let skip_count = idx.saturating_sub(2) as usize;

			for (key, value) in entries.iter().skip(skip_count) {
				let d_type = if value.is_dir() {
					DirentTypes::Directory
				} else {
					DirentTypes::Regular
				};
				let entry = VfsDirent::new(value.id, idx+1, d_type, String::from(key.as_ref()));
				if !try_push_dirent(&mut list, &mut idx, max_bytes, entry)? {
					break ;
				}
			}
		}
		*offset = idx;
		Ok(list)
	}
}

pub struct RamFs;

impl VfsFilesystem for RamFs {
	fn get_name(&self) -> [u8; 16] {
		RAMFS_NAME
	}

	fn mount(&self, _: Option<DeviceId>) -> Result<Arc<dyn Superblock>, KernelErrors> {
		let sb = RamfsSuperblock::new();
		let root = sb.root();
		root.clone().mkdir("bin")?;
		root.clone().mkdir("boot")?;
		root.clone().mkdir("dev")?;
		root.clone().mkdir("etc")?;
		root.clone().mkdir("home")?;
		root.clone().mkdir("lib")?;
		root.clone().mkdir("media")?;
		root.clone().mkdir("mnt")?;
		root.clone().mkdir("opt")?;
		root.clone().mkdir("root")?;
		root.clone().mkdir("run")?;
		root.clone().mkdir("srv")?;
		root.clone().mkdir("sys")?;
		root.clone().mkdir("tmp")?;
		root.clone().mkdir("usr")?;
		root.clone().mkdir("var")?;
		Ok(sb)
	}
}

pub fn init() -> Result<(), KernelErrors>{
	vfs_register_filesystem(Box::leak(Box::new(RamFs)))
}