use alloc::{string::String, sync::Arc, vec::Vec};
use spin::RwLock;

use crate::{drivers::block::DEV_MGR, errno::KernelErrors, fs::{self, devtmpfs::DEVTMPFS_NAME, ramfs::RAMFS_NAME}, klog, kpanic, scheduler::process::get_cwd, vfs::{dentry::VfsDentry, file::{VfsFile, VfsFileOpenflags}, filesystem::{FST_REG, VFS_MOUNTS, VfsMount}, inode::{DeviceId, NEXT_INO, VfsInodeType}, path::{path_basename, path_parent, path_walk}}};

static ROOT_DENTRY: RwLock<Option<Arc<VfsDentry>>> = RwLock::new(None);
pub const MAX_EXEC_IMAGE: usize = 16 * 1024 * 1024;


pub mod superblock;
pub mod filesystem;
pub mod inode;
pub mod dentry;
pub mod file;
pub mod path;

pub type InodeId = u64;

pub enum SeekOrigin {	
	SeekSet,
	SeekCur,
	SeekEnd,
}

pub fn init() {
	fs::init();

	let root_dentry = match mount(RAMFS_NAME, "/", None) {
		Ok(root_dentry) => root_dentry,
		Err(err) => {
			kpanic!("failed to mount root: {:#?}", err)
		},
	};
	set_root_dentry(root_dentry.clone());

	if let Err(e) =  mount(DEVTMPFS_NAME, "/dev", None) {
		kpanic!("Failed to mount devtmpfs on /dev, erro: {:?}",e)
	}

	let dev_mgr = DEV_MGR.lock();
	let _ = dev_mgr.register_in_vfs();

	klog!("VFS init done");
}

pub fn set_root_dentry(root: Arc<VfsDentry>) {
	*ROOT_DENTRY.write() = Some(root);
}

pub fn get_root_dentry() -> Result<Arc<VfsDentry>, KernelErrors> {
	ROOT_DENTRY.read().clone().ok_or(KernelErrors::ENOENT)
}

pub fn next_inode_id() -> InodeId {
	NEXT_INO.fetch_add(1, core::sync::atomic::Ordering::Relaxed)
}

pub fn lookup(path: &str, cwd: Option<Arc<VfsDentry>>) -> Result<Arc<VfsDentry>, KernelErrors> {
	let (root, cwd) = {
		let root = get_root_dentry()?;
		let cwd = cwd.unwrap_or(get_root_dentry()?);
		(root, cwd)
	};
	path_walk(path, root, cwd)
}

pub fn mkdir(parent_dentry: &Arc<VfsDentry>, basename: &str, mode: u16) -> Result<Arc<VfsDentry>, KernelErrors> {
	let parent_inode = parent_dentry.inode.clone();
	if !parent_inode.is_dir() {
		return Err(KernelErrors::ENOTDIR);
	}
	let child_inode = parent_inode.mkdir(basename, mode)?;
	let child_dentry = Arc::new(VfsDentry::from_name(
		basename,
		child_inode,
		Some(Arc::clone(parent_dentry)),
	)?);

	Ok(child_dentry)
}

pub fn create(path: &str, mode: u16) -> Result<Arc<VfsDentry>, KernelErrors> {
	let parent = path_parent(path)?;
	let basename = path_basename(path)?;
	let parent_dentry = lookup(parent, get_cwd())?;
	if !parent_dentry.inode.is_dir() {
		return Err(KernelErrors::ENOTDIR);
	}

	let child = parent_dentry.inode.clone().create(basename, mode)?;
	Ok(Arc::new(VfsDentry::from_name(
		path_basename(path)?,
		child,
		Some(parent_dentry)
	)?))
}

pub fn mkdev(path: &str, mode: u16, device_id: DeviceId) -> Result<(), KernelErrors> {

	let parent_path = path_parent(path)?;
	let basename = path_basename(path)?;

	let root = get_root_dentry()?;
	let parent_dentry = path_walk(parent_path, root.clone(), root)?;
	if !parent_dentry.inode.is_dir() {
		return Err(KernelErrors::ENOTDIR);
	}

	parent_dentry.inode.clone().mknod(basename, mode, device_id)?;
	parent_dentry.invalidate_child(basename);
	Ok(())
}

pub fn open(path: &str, flags: u32, mode: u16) -> Result<VfsFile, KernelErrors> {
	let vfs_flags = VfsFileOpenflags::from_bits_truncate(flags);
	let dentry = match lookup(path, get_cwd()) {
		Ok(dentry) => {
			if vfs_flags.contains(VfsFileOpenflags::O_DIRECTORY) && !dentry.inode.is_dir() {
				return Err(KernelErrors::ENOTDIR);
			}
			//file exist
			if vfs_flags.contains(VfsFileOpenflags::O_CREAT | VfsFileOpenflags::O_EXCL) {
				return Err(KernelErrors::EEXIST);
			}
			if dentry.inode.is_dir() && !vfs_flags.contains(VfsFileOpenflags::O_DIRECTORY) {
				return Err(KernelErrors::EISDIR);
			}
			if vfs_flags.contains(VfsFileOpenflags::O_TRUNC) {
				dentry.inode.reset();
			}

			dentry
		},
		Err(err) if err == KernelErrors::ENOENT && vfs_flags.contains(VfsFileOpenflags::O_CREAT) => {
			//file not exists but flags O_CREAT
			create(path, mode)?;
			match lookup(path, get_cwd()) {
				Ok(dentry) => dentry,
				Err(err) => return Err(err),
			}
		}
		Err(err) => {
			return Err(err);
		},
	};

	Ok(
		VfsFile::new(dentry.inode.clone(), 0, vfs_flags)
	)
}

pub fn read(file: &mut VfsFile, buf: &mut [u8]) -> Result<usize, KernelErrors> {

	let vfs_flags = file.flags;

	if vfs_flags.contains(VfsFileOpenflags::O_WRONLY) {
		return Err(KernelErrors::EINVAL);
	}
	let n = file.inode.clone().read_at(file.offset, buf)?;
	file.offset += n as u64;

	Ok(n)
}

pub fn write(file: &mut VfsFile,  buf: &[u8]) -> Result<usize, KernelErrors> {

	let vfs_flags = file.flags;
	let access_mode = vfs_flags & VfsFileOpenflags::O_ACCMODE;
	if !access_mode.contains(VfsFileOpenflags::O_RDWR) {
		if access_mode.contains(VfsFileOpenflags::O_RDONLY){
			return Err(KernelErrors::EINVAL);
		}
	}

	if vfs_flags.contains(VfsFileOpenflags::O_APPEND) {
		file.offset = file.inode.len() as u64;
	}

	let n = file.inode.clone().write_at(file.offset, buf)?;
	file.offset += n as u64;

	Ok(n)
}

pub fn seek(file: &mut VfsFile, offset: u64, origin: SeekOrigin) -> Result<u64, KernelErrors>{
	match origin {
		SeekOrigin::SeekSet => {
			file.offset = offset;
		},
		SeekOrigin::SeekCur => {
			file.offset += offset;
		},
		SeekOrigin::SeekEnd => {
			file.offset = file.len() as u64 + offset
		},
	}
	Ok(file.offset)
}

pub fn read_file_all(file: &mut VfsFile) -> Result<Vec<u8>, KernelErrors> {
	let file_len:usize = file.len();
	
	if file_len == 0 {
		return Err(KernelErrors::EINVAL);
	}

	if file_len > MAX_EXEC_IMAGE {
		return Err(KernelErrors::ENOMEM);
	}

	let mut buf: Vec<u8> = alloc::vec![0u8; file_len];
	file.offset = 0;
	let mut filled: usize = 0;
	while filled < file_len {
		let n = read(file, &mut buf[filled..])?;
		if n == 0 {
			return Err(KernelErrors::EINVAL);
		}
		filled += n;
	}
	Ok(buf)
}

pub fn mount(fs_name: [u8; 16], target: &str, source: Option<&str>) -> Result<Arc<VfsDentry>, KernelErrors> {

	let dev_id = if let Some(src_path) = source.filter(|s| !s.is_empty()) {
		let root = get_root_dentry()?;
		let src_dentry = path_walk(src_path, root.clone(), root.clone())?;
		let id = src_dentry.inode.dev_id().ok_or(KernelErrors::ENOTBLK)?;
		Some(id)
	} else {
		None
	};
	let fs_driver = FST_REG.lock().find(fs_name)?;
	let sb = fs_driver.mount(dev_id)?;

	let (mountpoint_dentry, parent_for_root) = if target == "/" {
		(None, None)
	} else {
		let root = get_root_dentry()?;
		let target_dentry = path_walk(target, root.clone(), root)?;
		if !target_dentry.inode.is_dir() {
			return Err(KernelErrors::ENOTDIR);
		}

		let parent = target_dentry.parent.clone();
		(Some(target_dentry), parent)
	};

	let name = target.rsplit('/').find(|s| s.is_empty()).unwrap_or("/");
	let root_dentry = Arc::new(VfsDentry::from_name(name, sb.root(), parent_for_root)?);

	if let Some(ref mp) = mountpoint_dentry {
		mp.set_mounted_root(Some(root_dentry.clone()));
	} 

	VFS_MOUNTS.lock().push(VfsMount {
		mountpoint: String::from(target),
		mountpoint_dentry,
		fs_name,
		sb,
		root_dentry: root_dentry.clone(),
	});
	Ok(root_dentry)
}

pub fn unmount(target: &str) -> Result<(), KernelErrors> {

	let mount = {
		let mut mounts = VFS_MOUNTS.lock();
		let pos = mounts
			.iter()
			.position(|m| m.mountpoint == target)
			.ok_or(KernelErrors::EINVAL)?;
		mounts.remove(pos)
	};

	mount.sb.sync()?;

	if let Some(ref mp) = mount.mountpoint_dentry {
		mp.set_mounted_root(None);
	}

	mount.root_dentry.clear_cache_recursive();

	Ok(())
}

pub fn mode_to_type(mode: u16) -> VfsInodeType {
	let masked = mode & 0xF000;

	match masked {
		0x1000 => VfsInodeType::FIFO,
		0x2000 => VfsInodeType::CharDev,
		0x4000 => VfsInodeType::Directory,
		0x6000 => VfsInodeType::BlockDev,
		0x8000 => VfsInodeType::Regular,
		0xA000 => VfsInodeType::Symlink,
		0xC000 => VfsInodeType::Socket,
		_ => VfsInodeType::Unknown
	}
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Stat {
    pub st_dev: u64,
    pub st_ino: u64,
    pub st_nlink: u64,
    pub st_mode: u32,
    pub st_uid: u32,
    pub st_gid: u32,
    pub __pad0: u32,
    pub st_rdev: u64,
    pub st_size: i64,
    pub st_blksize: i64,
    pub st_blocks: i64,
    pub st_atime: i64,
    pub st_atime_nsec: i64,
    pub st_mtime: i64,
    pub st_mtime_nsec: i64,
    pub st_ctime: i64,
    pub st_ctime_nsec: i64,
    pub __unused: [i64; 3],
}

impl Stat {
	pub fn as_bytes(&self) -> &[u8] {
		unsafe {
			core::slice::from_raw_parts(
				(self as *const Self) as *const u8,
				core::mem::size_of::<Self>()
			)
		}
	}
}