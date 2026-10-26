

/*
pub mod superblock;
pub mod filesystem;
pub mod inode;
pub mod dentry;
pub mod file;

 */
pub type InodeId = u64;

/*
pub fn lookup(path: &str) -> Result<InodeId, KernelErrors> {
	if path.len() == 0 {
		return Err(errno::KernelErrors::EINVAL);
	}

	let dentry = path_traverse(path);

}

pub fn path_traverse(path: &str) -> VfsDentry {

	
}


fn get_root_inode() {
	
}

 */