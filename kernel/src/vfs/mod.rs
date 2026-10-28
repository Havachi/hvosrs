use alloc::sync::Arc;
use spin::Mutex;

use crate::{errno::KernelErrors, fs::{self, ramfs::RAMFS_NAME}, kerror, klog, ksprint, vfs::{dentry::VfsDentry, file::{VfsFile, VfsFileOpenflags}, filesystem::FST_REG, inode::NEXT_INO, path::{path_basename, path_parent, path_walk}}};

static ROOT_DENTRY: Mutex<Option<Arc<VfsDentry>>> = Mutex::new(None);

pub mod superblock;
pub mod filesystem;
pub mod inode;
pub mod dentry;
pub mod file;
pub mod path;

pub type InodeId = u64;


pub fn init() {
	fs::init();
	if let Ok(ramfs) = FST_REG.lock().find(RAMFS_NAME) {
		let root_dentry = ramfs.mount("/").expect("should mount");
		let root = Arc::new(root_dentry);
		*ROOT_DENTRY.lock() = Some(root.clone())
	} else {
		return;
	}

	let dirs = ["/bin", "/boot", "/dev", "/etc", "/home", "/mnt", "/lib", "/opt", "/usr", "/var", "/tmp"];
	for dir in dirs {
		if let Err(e) = mkdir(dir) {
			kerror!("Error while creating fs layout: {:?}", e)
		}
	}
	klog!("VFS init done");
}

pub fn next_inode_id() -> InodeId {
	NEXT_INO.fetch_add(1, core::sync::atomic::Ordering::Relaxed)
}

pub fn lookup(path: &str, cwd: Option<Arc<VfsDentry>>) -> Result<Arc<VfsDentry>, KernelErrors> {
	let (root, cwd) = {
		let guard = ROOT_DENTRY.lock();
		let root = guard.as_ref().ok_or(KernelErrors::EINVAL)?.clone();
		let cwd = cwd.unwrap_or_else(|| root.clone());
		(root, cwd)
	};
	path_walk(path, root, cwd)
}


pub fn mkdir(path: &str) -> Result<(), KernelErrors> {
	let parent = path_parent(path)?;
	let basename = path_basename(path)?;

	let parent_dentry = lookup(parent, None)?;
	if !parent_dentry.inode.is_dir() {
		return Err(KernelErrors::ENOTDIR);
	}

	parent_dentry.inode.clone().mkdir(basename)
}

pub fn create(path: &str) -> Result<(), KernelErrors> {
	let parent = path_parent(path)?;
	let basename = path_basename(path)?;
	let parent_dentry = lookup(parent, None)?;
	if !parent_dentry.inode.is_dir() {
		return Err(KernelErrors::ENOTDIR);
	}
	parent_dentry.inode.clone().create(basename)
}


pub fn open(path: &str, flags: u32) -> Result<VfsFile, KernelErrors> {
	let vfs_flags = VfsFileOpenflags::from_bits_truncate(flags);
	let dentry = match lookup(path, None) {
		Ok(dentry) => {
			//file exist
			if vfs_flags.contains(VfsFileOpenflags::O_CREAT | VfsFileOpenflags::O_EXCL) {
				return Err(KernelErrors::EEXIST);
			}
			if dentry.inode.is_dir() {
				return Err(KernelErrors::EISDIR);
			}
			if vfs_flags.contains(VfsFileOpenflags::O_TRUNC) {
				dentry.inode.reset();
			}

			dentry
		},
		Err(err) if err == KernelErrors::ENOENT && vfs_flags.contains(VfsFileOpenflags::O_CREAT) => {
			//file not exists but flags O_CREAT
			create(path)?;
			match lookup(path, None) {
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


pub enum SeekOrigin {
	SeekSet,
	SeekCur,
	SeekEnd,
}