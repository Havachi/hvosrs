use crate::{kerror, klog};

pub mod ramfs;
pub mod dirent;
pub mod cpio;
pub mod ext2;
pub mod devtmpfs;

pub fn init() {

	if let Err(err) = ramfs::init() {
		kerror!("Error while initializing ramfs\nError: {:?}", err);
	}

	if let Err(err) = devtmpfs::init() {
		kerror!("Error while initializing Devtmpfs\nError {:?}", err);
	}

	if let Err(_err) = ext2::init() {
		kerror!("No ext2 fs found");
	}

	klog!("Fs init done");
}