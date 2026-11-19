use core::sync::atomic::{AtomicU64, AtomicUsize};

use alloc::{boxed::Box, collections::BTreeMap, string::String, sync::{Arc, Weak}, vec::Vec};
use spin::Mutex;

use crate::{drivers::block::DEV_MGR, errno::KernelErrors::{self, EEXIST, EINVAL, ENODEV, ENOTDIR}, fs::dirent::DirentTypes, klog, memory::PAGE_SIZE, vfs::{InodeId, Stat, dentry::{VfsDentry, VfsDirent, try_push_dirent}, file::{S_IFBLK, S_IFCHR, S_IFDIR, S_IFREG}, filesystem::{FST_REG, VfsFilesystem, vfs_register_filesystem}, inode::{DeviceId, Inode, VfsInodeType}, mode_to_type, superblock::{Statfs, Superblock}}};

pub const DEVTMPFS_NAME: [u8; 16] = *b"devtmpfs\0\0\0\0\0\0\0\0";

pub type DevDir = BTreeMap<Box<str>, Arc<dyn Inode>>;

pub struct DevtmpfsSuperblock {
	root: Arc<DevInode>,
	next_ino: AtomicU64,
}


impl DevtmpfsSuperblock {
	pub fn new() -> Arc<Self> {
		Arc::new_cyclic(|weak_sb| Self {
			root: DevInode::new_dir(0,0, weak_sb.clone()),
			next_ino: AtomicU64::new(1),
		})
	}

	pub fn alloc_inode_id(&self) -> InodeId {
		self.next_ino.fetch_add(1, core::sync::atomic::Ordering::Relaxed)
	}
}

impl Superblock for DevtmpfsSuperblock {
	fn root(&self) -> Arc<dyn Inode> {
		self.root.clone()
	}
	fn statfs(&self) -> Result<crate::vfs::superblock::Statfs, KernelErrors> {
		Ok(Statfs {
			fs_type: FST_REG.lock().find_id(DEVTMPFS_NAME)?,
			blocksize: PAGE_SIZE,
			total_blocks: 0,
			free_blocks: 0,
			total_inodes: self.next_ino.load(core::sync::atomic::Ordering::Relaxed),
			free_inodes: 0
		})
	}
}

#[derive(Debug)]
pub struct DevInode {
	id: InodeId,
	vtype: VfsInodeType,
	size: AtomicUsize,
	dir: Option<Mutex<DevDir>>,
	file: Option<Mutex<Vec<u8>>>,
	dev_id: Option<DeviceId>,
	parent_id: InodeId,
	sb: Weak<DevtmpfsSuperblock>,
}

unsafe impl Send for DevInode {}
unsafe impl Sync for DevInode {}

impl DevInode {
	pub fn new_dir(id: InodeId, parent_id: InodeId, sb: Weak<DevtmpfsSuperblock>) -> Arc<Self> {
		Arc::new(Self {
			id,
			vtype: VfsInodeType::Directory,
			size: AtomicUsize::new(0),
			dir: Some(Mutex::new(DevDir::new())),
			file: None,
			dev_id: None,
			parent_id,
			sb,
		})
	}
	
	pub fn new_device(id: InodeId, parent_id: InodeId, vtype: VfsInodeType, dev_id: DeviceId, sb: Weak<DevtmpfsSuperblock>) -> Arc<Self> {
		Arc::new(Self {
			id,
			vtype,
			size: AtomicUsize::new(0),
			dir: None,
			file: None,
			dev_id: Some(dev_id),
			parent_id,
			sb,
		})
	}

	pub fn new_file(id: InodeId, parent_id: u64, sb: Weak<DevtmpfsSuperblock>) -> Arc<Self> {
		Arc::new(
			Self {
				id,
				vtype: VfsInodeType::Regular,
				size: AtomicUsize::new(0),
				dir: None,
				file: Some(Mutex::new(Vec::<u8>::new())),
				parent_id: parent_id,
				dev_id: None,
				sb
			}
		)
	}
}

impl Inode for DevInode {
	fn lookup(self: Arc<Self>, name: &str, parent: Option<Arc<VfsDentry>>)	-> Result<Arc<VfsDentry>, KernelErrors> {
		if !self.is_dir() || self.dir.is_none() {
			return Err(KernelErrors::ENOTDIR);
		}
		let dir = self.dir.as_ref().ok_or(KernelErrors::ENOENT)?;
		let res = dir.lock();

		if let Some(ino) = res.get(name) {
			let name_bytes = name.as_bytes();
			if name_bytes.len() > 255{
				return Err(KernelErrors::EINVAL);
			}
			return Ok(Arc::new(VfsDentry::from_name(name, ino.clone(), parent)?))
		} else {
			return Err(KernelErrors::ENOENT);
		}

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
		let child = DevInode::new_file(next_id, self.id, self.sb.clone());
		dir.insert(basename.into(), child);

		Ok(())
	}

	fn mknod(self: Arc<Self>, name: &str, mode: u16, dev_id: DeviceId) -> Result<(), KernelErrors> {
		let dir_lock = self.dir.as_ref().ok_or(KernelErrors::ENOTDIR)?;
		if name.is_empty() || name.len() > 255 || name.contains("/") {
			return Err(EINVAL);
		}

		let mut dir = dir_lock.lock();
		if dir.contains_key(name) {
			return Err(EEXIST);
		}

		let vtype = mode_to_type(mode);

		let sb = self.sb.upgrade().ok_or(KernelErrors::EIO)?;
		let next_id = sb.alloc_inode_id();
		let node = DevInode::new_device(next_id, self.id, vtype, dev_id, self.sb.clone());
		dir.insert(name.into(), node);
		Ok(())
	}

	fn mkdir(self: Arc<Self>, name: &str) -> Result<Arc<dyn Inode>, KernelErrors> {
		if !self.is_dir() || self.dir.is_none() {
			return Err(ENOTDIR);
		}
		if name.is_empty() || name.len() > 255 || name.contains('/') {
			return Err(EINVAL);
		}
		let mut dir = self.dir.as_ref().unwrap().lock();
		if dir.contains_key(name){
			return Err(EEXIST);
		}
		
		let sb = self.sb.upgrade().ok_or(KernelErrors::EIO)?;
		let next_id = sb.alloc_inode_id();
		let child = DevInode::new_dir(next_id, self.id, self.sb.clone());
		dir.insert(name.into(), child.clone());
		Ok(child)
	}

	fn read_at(self: Arc<Self>, offset: u64, buf: &mut[u8]) -> Result<usize, KernelErrors> {
		match self.vtype {
			VfsInodeType::CharDev | VfsInodeType::BlockDev => {
				let dev_id = self.dev_id.ok_or(KernelErrors::ENODEV)?;
				let devmgr = DEV_MGR.lock();
				let driver = devmgr.find_by_id(dev_id).ok_or(KernelErrors::ENODEV)?;
				driver.read_block(offset, buf.len() as u32 / driver.block_size() , buf).map_err(|_| KernelErrors::EIO)?;
				Ok(buf.len())
			},
			VfsInodeType::Regular => {
				let data = self.file.as_ref().ok_or(KernelErrors::EINVAL)?.lock();
				let start = offset as usize;
				if start >= data.len() {
					return Ok(0);
				}
				let to_read = core::cmp::min(buf.len(), data.len() - start);
				buf[..to_read].copy_from_slice(&data[start..start + to_read]);
				Ok(to_read)
			}
			_ => Err(KernelErrors::EINVAL)
		}
	}

	fn write_at(self: Arc<Self>, offset: u64, buf: &[u8]) -> Result<usize, KernelErrors> {
		match self.vtype {
			VfsInodeType::CharDev | VfsInodeType::BlockDev => {
				let dev_id = self.dev_id.ok_or(ENODEV)?;
				let devmgr = DEV_MGR.lock();
				let driver = devmgr.find_by_id(dev_id).ok_or(KernelErrors::ENODEV)?;
				driver.write_block(offset, buf.len() as u32 / driver.block_size(), buf).map_err(|_| KernelErrors::EIO)?;
				Ok(buf.len())
			},
			VfsInodeType::Regular => {
				let mut data = self.file.as_ref().ok_or(EINVAL)?.lock();
                let start = offset as usize;
                let end = start.saturating_add(buf.len());
                if data.len() < end {
                    data.resize(end, 0);
                }
                data[start..end].copy_from_slice(buf);
                self.size.store(data.len(), core::sync::atomic::Ordering::Relaxed);
                Ok(buf.len())	
			}
			_ => Err(KernelErrors::EINVAL)
		}
	}

	fn is_dir(&self) -> bool {
		self.vtype == VfsInodeType::Directory
	}

	fn is_file(&self) -> bool {
		!self.is_dir()
	}

	fn id(&self) -> InodeId {
		self.id
	}

	fn len(&self) -> usize {
		self.size.load(core::sync::atomic::Ordering::Relaxed)
	}
	fn itype(&self) -> VfsInodeType {
		self.vtype
	}

	fn statfs(&self) -> Result<Statfs, KernelErrors> {
		self.sb.upgrade().unwrap().statfs()
	}
	fn reset(&self) {
		if let Some(file) = &self.file {
			self.size.store(0, core::sync::atomic::Ordering::Relaxed);
			file.lock().clear();
		}
	}

	fn readdir(self: Arc<Self>, index: u64) -> Result<Option<crate::vfs::dentry::VfsDirent>, KernelErrors> {
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
						value.id(),
						0,
						DirentTypes::from(value.itype()),
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

	fn getdents(&self, offset: &mut u64, max_bytes: usize) -> Result<alloc::vec::Vec<u8>, KernelErrors> {
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
				let d_type = DirentTypes::from(value.itype());
				let entry = VfsDirent::new(value.id(), idx+1, d_type, String::from(key.as_ref()));
				if !try_push_dirent(&mut list, &mut idx, max_bytes, entry)? {
					break ;
				}
			}
		}
		*offset = idx;
		Ok(list)
	}

	fn stat(&self) -> Result<crate::vfs::Stat, KernelErrors> {
		let (mode, rdev) = match self.vtype {
			VfsInodeType::Directory => (S_IFDIR | 0o755, 0),
			VfsInodeType::CharDev => {
				let dev = self.dev_id.unwrap_or(DeviceId::default());
				(S_IFCHR | 0o666, makedev(dev.major, dev.minor))
			}
			VfsInodeType::BlockDev => {
				let dev = self.dev_id.unwrap_or(DeviceId::default());
				(S_IFBLK | 0o660, makedev(dev.major, dev.minor))
			}
			_ => (S_IFREG | 0o644, 0),
		};
		klog!("{} -> {}", self.id, mode);
		let size = self.len() as i64;
		Ok(Stat {
			st_ino: self.id(),
            st_nlink: 1,
            st_mode: mode,
            st_rdev: rdev,
            st_size: size,
            st_blksize: 4096,
            st_blocks: (size + 511) / 512,
            ..Default::default()
		})
	}
	fn dev_id(&self) -> Option<DeviceId> {
		self.dev_id
	}
}

pub struct DevTmpFs;


impl VfsFilesystem for DevTmpFs {
	fn get_name(&self) -> [u8; 16] {
		DEVTMPFS_NAME
	}

	fn mount(&self, _dev: Option<DeviceId>) -> Result<Arc<dyn Superblock>, KernelErrors> {
		let sb = DevtmpfsSuperblock::new();
		let root = sb.root();
		root.clone().create("test")?;
		root.clone().mknod("null", 0o020666, DeviceId { major: 1, minor: 3 })?;
		root.clone().mknod("zero", 0o020666, DeviceId { major: 1, minor: 5 })?;
		Ok(sb)
	}
}

pub fn init() -> Result<(), KernelErrors> {
	vfs_register_filesystem(Box::leak(Box::new(DevTmpFs)))
}

pub const fn makedev(major:u16, minor: u16) -> u64 {
	(((major as u64) & 0xFFF) << 8)
		| ((minor as u64) & 0xFF)
		| (((minor as u64) & ! 0xFF) << 12)
		| (((major as u64) & ! 0xFFF) << 32)
}