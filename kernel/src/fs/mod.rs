use alloc::boxed::Box;

use crate::{fs::ramfs::RamFs, kerror, klog, vfs::filesystem::vfs_register_filesystem};

pub mod ramfs;

pub fn init() {

	let ramfs = Box::leak(Box::new(RamFs));
	if let Err(err) = vfs_register_filesystem(ramfs) {
		kerror!("Error while initializing ramfs\nError: {err}");
	}

	klog!("Fs init done");
}