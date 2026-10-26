use spin::Mutex;

use crate::vfs::dentry::VfsDentry;

///Every future filesystem drivers must implement this trait to work with the Vfs
///Vfs will internally call those function
pub trait VfsFilesystem {
	fn get_name(&self);
	fn mount(&self, path: &str) -> VfsDentry;
	
}

pub struct VfsFileSystemType {
	name: &'static str
}



pub fn vfs_register_filesystem(fs: impl VfsFilesystem) {

}

pub fn vfs_unregister_filesystem(fs: impl VfsFilesystem) {

}