use alloc::{sync::Arc, vec::Vec, vec};
use crate::{drivers::block::BlockDevice, fs::ext2::{inode::Ext2Vnode, lba_conv, superblock::Ext2SuperblockRaw}, vfs::InodeId};



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
	pub fn parse(dev: Arc<dyn BlockDevice>, rawsb: &Ext2SuperblockRaw) -> Self {
		let total_bgd = rawsb.total_blocks / rawsb.blocks_per_group;
		let start = lba_conv(rawsb.block_size() as u64, dev.block_size() as u64, (rawsb.super_block_id + 1) as u64);
		let bgdt_size = total_bgd * 32;
		let mut bgdt_nb_blocks = bgdt_size / rawsb.block_size();
		if bgdt_nb_blocks == 0 { bgdt_nb_blocks = 1}
		let mut buf = vec![0u8; (bgdt_nb_blocks * rawsb.block_size()) as usize];
		let _ = dev.read_block(start, bgdt_nb_blocks, &mut buf);
		let mut tbl: Vec<Ext2BGDRaw> = Vec::new();
		for i in 0..total_bgd as usize {
			let raw = &buf[i*32..i*32+32];
			let ptr = raw.as_ptr() as *const Ext2BGDRaw;
			let bgdraw = unsafe { ptr.read_unaligned() };
			tbl.push(bgdraw);
		}
		Self {
			table: tbl,
			nb_bg: total_bgd as u64,
		}
	}
}