use alloc::{sync::Arc, vec::Vec, vec};
use crate::{drivers::block::BlockDevice, errno::KernelErrors, fs::ext2::{lba_conv, superblock::Ext2SuperblockRaw}, klog, vfs::InodeId, };



#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Ext2BGDRaw {
	pub block_bitmap: u32,
	pub inode_bitmap: u32,
	pub inode_table: u32,
	pub nb_free_blocks: u16,
	pub nb_free_inodes: u16,
	pub nb_dir: u16,
	_reserved: [u8; 14]
}

pub struct Ext2BGDT {
	pub table: Vec<Ext2BGDRaw>,
	nb_bg: u64,
}

impl Ext2BGDT {
	pub fn parse(dev: Arc<dyn BlockDevice>, rawsb: &Ext2SuperblockRaw) -> Result<Self, KernelErrors> {
		const BGD_ENTRY_SIZE: usize = core::mem::size_of::<Ext2BGDRaw>();
		let fs_blk_size = rawsb.block_size() as u64;
		let dev_blk_size = dev.block_size() as u64;

		let total_bgd = rawsb.total_blocks.div_ceil(rawsb.blocks_per_group) as usize;
		let bgdt_byte_size = (total_bgd * BGD_ENTRY_SIZE) as u64;
		let bgdt_fs_blocks = bgdt_byte_size.div_ceil(fs_blk_size).max(1);

		let start_lba = lba_conv(fs_blk_size, dev_blk_size, (rawsb.super_block_id + 1) as u64);
		let buf_len = (bgdt_fs_blocks * fs_blk_size) as usize;
		let sectors_to_read = (buf_len as u64 / dev_blk_size) as u32;
		klog!("bgdt start: {start_lba}, size: {bgdt_byte_size}, blocks: {bgdt_fs_blocks}");
		let mut buf = vec![0u8; buf_len];
		dev.read_block(start_lba, sectors_to_read, &mut buf)?;
		let table = buf
			.chunks_exact(BGD_ENTRY_SIZE)
			.take(total_bgd)
			.map(|chunk| unsafe { (chunk.as_ptr() as *const Ext2BGDRaw).read_unaligned() })
			.collect();
		Ok(Self {
			table,
			nb_bg: total_bgd as u64,
		})
	}
}