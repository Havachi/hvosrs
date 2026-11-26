use alloc::{boxed::Box, sync::Arc};

use crate::{drivers::block::DEV_MGR, errno::KernelErrors, fs::ext2::{group::Ext2BGDT, superblock::{Ext2Sb, Ext2SuperblockRaw}}, vfs::{InodeId, filesystem::{VfsFilesystem, vfs_register_filesystem}, inode::DeviceId}};

pub mod superblock;
pub mod group;
pub mod inode;
pub mod dentry;

pub const EXT2_BLOCK_SIZE: usize = 1024;

pub const EXT2_NDIR_BLOCKS: usize = 12;
pub const EXT2_IND_BLOCK: usize = EXT2_NDIR_BLOCKS;
pub const EXT2_DIND_BLOCK: usize = EXT2_IND_BLOCK + 1;
pub const EXT2_TIND_BLOCK: usize = EXT2_DIND_BLOCK + 1;
pub const EXT2_N_BLOCKS: usize = EXT2_TIND_BLOCK + 1;

pub const EXT2_INDIRECT: usize = EXT2_BLOCK_SIZE / size_of::<u32>();
pub const EXT2_DINDIRECT: usize = (EXT2_BLOCK_SIZE / size_of::<u32>()) * EXT2_INDIRECT;
pub const EXT2_TINDIRECT: usize = (EXT2_BLOCK_SIZE / size_of::<u32>()) * EXT2_DINDIRECT;
pub const EXT2_MAXFILE: usize = EXT2_NDIR_BLOCKS + EXT2_INDIRECT + EXT2_DINDIRECT + EXT2_TINDIRECT;
pub const EXT2_NAME_LEN: usize = 255;
pub const EXT2_FSNAME: [u8; 16] = *b"ext2\0\0\0\0\0\0\0\0\0\0\0\0";
pub const EXT2_ROOT_INO: InodeId = 2;

pub fn init() -> Result<(), KernelErrors> {
	vfs_register_filesystem(Box::leak(Box::new(Ext2Fs)))?;

	Ok(())
}

pub struct Ext2Fs;

impl VfsFilesystem for Ext2Fs {
	fn get_name(&self) -> [u8; 16] {
		EXT2_FSNAME
	}

	fn mount(&self, dev: Option<DeviceId>) -> Result<Arc<dyn crate::vfs::superblock::Superblock>, KernelErrors> {
		let devmgr = DEV_MGR.lock();
		let blkdev = devmgr.find_by_id(dev.ok_or(KernelErrors::ENODEV)?).ok_or(KernelErrors::ENODEV)?;
		let sbraw = Ext2SuperblockRaw::from_blkdev(blkdev.clone())?;
		let bgdt = Ext2BGDT::parse(blkdev.clone(), &sbraw)?;
		Ext2Sb::new(blkdev, sbraw, bgdt)
	}
}

pub fn lba_conv(src_size: u64, dest_size: u64, lba: u64) -> u64 {
	lba * (src_size / dest_size as u64)
}