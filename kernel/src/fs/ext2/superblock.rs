use alloc::{sync::Arc, vec};
use bitflags::bitflags;
use spin::Mutex;

use crate::{drivers::block::BlockDevice, errno::KernelErrors, fs::ext2::{EXT2_FSNAME, EXT2_ROOT_INO, group::{Ext2BGDRaw, Ext2BGDT}, inode::{Ext2InodeRaw, Ext2Vnode}, lba_conv}, kerror, klog, vfs::{InodeId, filesystem::FST_REG, superblock::{Statfs, Superblock}}};

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Ext2SuperblockRaw{
	///Total number of inodes in file system
	pub total_inodes: u32,
	///Total number of blocks in file system
	pub total_blocks: u32,
	///Number of blocks reserved for superuser
	pub rsvd_blocks: u32,
	///Total number of unallocated blocks
	pub free_blocks: u32,
	///Total number of unallocated inodes
	pub free_inodes: u32,
	///Block number of the block containing the superblock
	pub super_block_id: u32,
	///Block size, log2 (block size) - 10
	pub block_size: u32,
	///Fragment size, log2 (fragment size) - 10
	pub frag_size: u32,
	///Number of blocks in each block group
	pub blocks_per_group: u32,
	///Number of fragments in each block group
	pub frags_per_group: u32,
	///Number of inodes in each block group
	pub inodes_per_group: u32,
	///Last mount time
	pub last_mount_time: u32,
	///Last written time
	pub last_write_time: u32,
	///Number of times the volume has been mounted since its last consistency check
	pub nb_mnt_fsck: u16,
	///Number of mounts allowed before a consistency check must be done
	pub nb_mnt_allow_fsck: u16,
	///Ext2 signature (0xef53)
	pub magic: u16,
	///File system state
	pub state: Ext2FsState,
	///	What to do when an error is detected 
	pub err_act: Ext2ErrAct,
	///Minor portion of version
	pub minor: u16,
	///last consistency check time
	pub last_fsck_time: u32,
	///Interval between forced consistency checks
	pub interval_fsck: u32,
	///Operating system ID from which the filesystem on this volume was created 
	pub osid: Ext2OsId,
	///Major portion of version
	pub major: u32,
	///User ID that can use reserved blocks
	pub rsvd_usrid: u16,
	///Group ID that can use reserved blocks
	pub rsvd_grpid: u16,

	//Extended Superblock fields

	///First non-reserved inode in file system.
	///(In versions < 1.0, this is fixed as 11)
	pub first_inode: u32,
	///Size of each inode structure in bytes. (In versions < 1.0, this is fixed as 128)
	pub inode_size: u16,
	///Block group that this superblock is part of (if backup copy)
	pub block_group_id: u16,
	///Optional features present
	pub opt_features: u32,
	///Required features present 
	pub req_features: u32,
	///	Features that if not supported, the volume must be mounted read-only
	pub ro_features: u32,
	///File system ID
	pub fsid: [u8;16],
	///Volume name (C-String, null terminated)
	pub vol_name: [u8; 16],
	///Path volume was last mounted to (C-String, null terminated)
	pub last_mnt_path: [u8; 64],
	///Compression algorithms used
	pub comp_alg: u32,
	///Number of blocks to preallocate for files
	pub blk_prealloc_file: u8,
	///Number of blocks to preallocate for directories
	pub blk_prealloc_dir: u8,
	///unused
	unused: u16,
	///Journal ID
	pub journal_id: [u8; 16],
	///Journal inode
	pub journal_inode: u32,
	///Journal device
	pub journal_device: u32,
	///Head of orphan inode list
	pub orphan_list_head: u32,
	_res_unused: [u8; 788],
}
const _: () = assert!(core::mem::size_of::<Ext2SuperblockRaw>() == 1024);


#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Ext2FsState {
	Clean = 1,
	Error = 2,
}

#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Ext2ErrAct {
	Ignore = 1,
	Remount = 2,
	Panic = 3,
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Ext2OsId {
	Linux = 0,
	HURD = 1,
	MASIX = 2,
	FreeBSD = 3,
	Other = 4,
}

bitflags! {
	#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
	///These are optional features for an implementation to support, but offer performance or reliability gains to implementations that do support them.
	pub struct Ext2OptFeatures: u32 {
		///Preallocate some number of blocks (see byte 205 in the superblock) to a directory when creating a new one
		const PREALLOC_DIR			= 0x0001;
		///AFS server inodes exist
		const AFS_SRV_INO_EXITST	= 0x0002;
		///File system has a journal (Ext3)
		const FS_HAS_JOURNAL		= 0x0004;
		///Inodes have extended attributes
		const INO_EXT_ATTR			= 0x0008;
		///File system can resize itself for larger partitions
		const FS_RESIZE				= 0x0010;
		///Directories use hash index
		const DIR_HASH				= 0x0020;		
	}
}

bitflags! {
	#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
	///These features if present on a file system are required to be supported by an implementation in order to correctly read from or write to the file system.
	pub struct Ext2ReqFeatures: u32 {
		///Compression is used
		const USE_COMPRESSION		= 0x0001;
		///Directory entries contain a type field
		const DIR_TYPE				= 0x0002;
		///File system needs to replay its journal
		const JOURNAL_REPLAY		= 0x0004;
		///File system uses a journal device
		const JOURNAL_DEV			= 0x0008;
	}
}

bitflags! {
	#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
	///These features, if present on a file system, are required in order for an implementation to write to the file system, but are not required to read from the file system.
	pub struct Ext2RoFeatures: u32 {
		///Sparse superblocks and group descriptor tables
		const SPARSE				= 0x0001;
		///File system uses a 64-bit file size
		const BIG_FILES				= 0x0002;
		///Directory contents are stored in the form of a Binary Tree
		const BTREE					= 0x0004;
	}
}

impl Ext2SuperblockRaw {
	pub fn from_blkdev(blkdev: Arc<dyn BlockDevice>) -> Result<Self, KernelErrors> {
		let mut raw = [0u8; 1024];
		let blksz = blkdev.block_size();
		if blksz == 0 { return Err(KernelErrors::EINVAL); }
		if blksz <= 1024 {
			let lba = (1024 / blksz) as u64;
			let count = 1024 / blksz;
			klog!("fromblkdev: lba:{}, count:{}, blksz:{}", lba, count, blksz);
			blkdev.read_block(lba, count, &mut raw)?; 
		} else {
			let mut sect = alloc::vec![0u8; blksz as usize];
			blkdev.read_block(0, 1, &mut sect)?;
			raw.copy_from_slice(&sect[1024..2048]);
		}
		Self::from_bytes(&raw)
	}

	pub fn from_bytes(raw: &[u8; 1024]) -> Result<Self, KernelErrors> {
		let sb = unsafe {
			let sb_ptr = raw.as_ptr() as *const Ext2SuperblockRaw;
			core::ptr::read_unaligned(sb_ptr)
		};
		let magic = sb.magic;
		if magic != 0xEF53 {
			kerror!("ext2: invalid magic found: {magic}\n");
			return Err(KernelErrors::EINVAL);
		}
		Ok(sb)
	}

	pub fn precent_free(&self) -> u32 {
		((self.free_blocks as f32 / self.total_blocks as f32) * 100f32) as u32
	}

	pub fn percent_used(&self) -> u32 {
		100 - self.precent_free()
	}

	pub fn block_size(&self) -> u32 {
		1024 << self.block_size
	}

}

pub struct Ext2Meta {
	pub raw_sb: Ext2SuperblockRaw,
	pub bgdt: Ext2BGDT,
}

pub struct Ext2Sb {
	dev: Arc<dyn BlockDevice>,
	root: Arc<Ext2Vnode>,
	pub meta: Mutex<Ext2Meta>,
	block_size: u32,
	inode_size: u32,
}

impl Superblock for Ext2Sb {
	fn root(&self) -> Arc<dyn crate::vfs::inode::Inode> {
		self.root.clone()
	}

	fn statfs(&self) -> Result<crate::vfs::superblock::Statfs, KernelErrors> {
		let meta= self.meta.lock();
		Ok(Statfs {
			fs_type: FST_REG.lock().find_id(EXT2_FSNAME)?,
			blocksize: self.block_size as usize,
			total_blocks: meta.raw_sb.total_blocks as u64,
			free_blocks: meta.raw_sb.free_blocks as u64,
			total_inodes: meta.raw_sb.total_inodes as u64,
			free_inodes: meta.raw_sb.free_inodes as u64,
		})
	}

	fn sync(&self) -> Result<(), KernelErrors> {
		//should write SB + BGDT + every dirty inodes
		Ok(())	
	}
}

impl Ext2Sb {
	pub fn new(dev: Arc<dyn BlockDevice>, raw_sb: Ext2SuperblockRaw, bgdt: Ext2BGDT) -> Result<Arc<dyn Superblock>, KernelErrors> {
		let block_size = raw_sb.block_size();
		let inode_size = (if raw_sb.inode_size == 0 { 128 } else { raw_sb.inode_size }) as u32 ;
		let root_raw = Ext2Sb::read_raw_inode_from_parts(&dev, &raw_sb, &bgdt, EXT2_ROOT_INO)?;
		Ok(Arc::new_cyclic(|weak_sb| {
			let root = Ext2Vnode::new_root(weak_sb.clone(), root_raw);
			let meta = Mutex::new(Ext2Meta{
				raw_sb,bgdt
			});
			Self {
				dev,
				meta,
				block_size,
				inode_size,
				root
			}
		}))
	}

	pub fn read_block(&self, fs_block: u64, buf: &mut vec::Vec<u8>) -> Result<(), KernelErrors> {
		let fs_blk_size = self.block_size as u64;
		if buf.len() < fs_blk_size as usize {
			return Err(KernelErrors::EINVAL);
		}
		let dev_blk_size = self.dev.block_size() as u64;
		let lba = lba_conv(fs_blk_size, dev_blk_size, fs_block);
		let sect_count = (fs_blk_size /dev_blk_size) as u32;
		self.dev.read_block(lba, sect_count, &mut buf[..fs_blk_size as usize])
	}

	pub fn write_block(&self, fs_block: u64, buf: &[u8]) -> Result<(), KernelErrors> {
		let fs_blk_size = self.block_size as u64;
		if buf.len() < fs_blk_size as usize {
			return Err(KernelErrors::EINVAL);
		}

		let dev_blk_size = self.dev.block_size() as u64;
		let lba = lba_conv(fs_blk_size, dev_blk_size, fs_block);
		let sector_count = (fs_blk_size / dev_blk_size) as u32;

		self.dev.write_block(lba, sector_count, &buf[..fs_blk_size as usize])
	}

	pub fn write_n_blocks(&self, lba: u64, n: usize, buf: &vec::Vec<u8>) -> Result<(), KernelErrors> {
		let total_len = n * self.block_size as usize;
		if buf.len() < total_len { return Err(KernelErrors::EINVAL); }
		self.dev.write_block(lba, n as u32, buf)?;
		Ok(())
	}

	pub fn read_inode(self: Arc<Self>, ino: InodeId) -> Result<Ext2Vnode, KernelErrors> {
		let meta = self.meta.lock();
		let raw_inode = Self::read_raw_inode_from_parts(
			&self.dev,
			&meta.raw_sb,
			&meta.bgdt,
			ino,
		)?;
		Ok(Ext2Vnode::from_raw(ino, raw_inode, Arc::downgrade(&self)))
	}

	pub fn write_inode(self: Arc<Self>, ino: InodeId, inode: &Ext2InodeRaw) -> Result<(), KernelErrors> {
		let meta = self.meta.lock();

		if ino == 0 || ino > meta.raw_sb.total_inodes as u64 {
			return Err(KernelErrors::EINVAL);
		}

		let fs_blk_size = self.block_size as usize;
		let inode_size = self.inode_size as usize;
		let inodes_per_group = meta.raw_sb.inodes_per_group as usize;

		let ino_idx = (ino as usize) - 1;
		let bg = ino_idx / inodes_per_group;
		let idx = ino_idx % inodes_per_group;

		let byte_offset = idx.checked_mul(inode_size).ok_or(KernelErrors::EINVAL)?;
		let block_offset = (byte_offset / fs_blk_size) as u64;
		let offset_in_block = byte_offset % fs_blk_size;

		if offset_in_block + core::mem::size_of::<Ext2InodeRaw>() > fs_blk_size {
			return Err(KernelErrors::EINVAL);
		}

		let table_start_block = meta
			.bgdt
			.table
			.get(bg)
			.ok_or(KernelErrors::EINVAL)?
			.inode_table as u64;

		let target_fs_block = table_start_block + block_offset;
		let mut blkbuf = alloc::vec![0u8; fs_blk_size];
		self.read_block(target_fs_block, &mut blkbuf);

		unsafe {
			core::ptr::write_unaligned(
				blkbuf.as_mut_ptr().add(offset_in_block) as *mut Ext2InodeRaw,
				inode.clone(),
			);
		}
		self.write_block(target_fs_block, &blkbuf)?;
		Ok(())
	}

	pub fn block_size(self: Arc<Self>) -> u64 {
		self.block_size as u64
	}

	pub fn read_indirect_ptr(&self, block_num: u32, index: u64) -> Result<u32, KernelErrors> {
		if block_num == 0 {
			return Ok(0);
		}

		let mut buf = vec![0u8; self.block_size as usize];
		self.read_block(block_num as u64, &mut buf)?;

		let offset = (index as usize) * 4;
		let bytes = [
			buf[offset],
			buf[offset + 1],
			buf[offset + 2],
			buf[offset + 3],
		];
		Ok(u32::from_le_bytes(bytes))
	}

	pub fn file_offset_to_block(
		&self,
		i_block: &[u32; 15],
		file_offset: u64,
	) -> Result<Option<u64>, KernelErrors>
	{
		let block_size = self.block_size as u64;
		let logical_block = file_offset / block_size;
		let ptrs_per_block = block_size / 4;

		let single_limit = 12 + ptrs_per_block;
		let double_limit = single_limit + (ptrs_per_block * ptrs_per_block);
		let triple_limit = double_limit + (ptrs_per_block * ptrs_per_block * ptrs_per_block);

		let phys_block = if logical_block < 12 {
			i_block[logical_block as usize]
		} else if logical_block < single_limit {
			let idx = logical_block - 12;
			self.read_indirect_ptr(i_block[12], idx)?
		} else if logical_block < double_limit {
			let idx = logical_block - single_limit;
			let l1_block = self.read_indirect_ptr(i_block[13], idx / ptrs_per_block)?;
			self.read_indirect_ptr(l1_block, idx % ptrs_per_block)?
		} else if logical_block < triple_limit {
			let idx = logical_block - double_limit;
			let ptrs_sq = ptrs_per_block * ptrs_per_block;
			let l1_block = self.read_indirect_ptr(i_block[14], idx / ptrs_sq)?;
			let l2_block = self.read_indirect_ptr(l1_block, (idx / ptrs_per_block) % ptrs_per_block)?;
			self.read_indirect_ptr(l2_block, idx % ptrs_per_block)?
		} else {
			return Err(KernelErrors::EINVAL);
		};

		if phys_block == 0 {
			return Ok(None);
		} else {
			Ok(Some(phys_block as u64))
		}
	}

	fn read_raw_inode_from_parts(
		dev: &Arc<dyn BlockDevice>,
		raw_sb: &Ext2SuperblockRaw,
		bgdt: &Ext2BGDT,
		ino: InodeId,
	) -> Result<Ext2InodeRaw, KernelErrors> {
		if ino == 0 || ino > raw_sb.total_inodes as u64 {
			return Err(KernelErrors::EINVAL);
		}

		let fs_blk_size = raw_sb.block_size() as usize;
		let inode_size = raw_sb.inode_size as usize;
		let ino_idx = (ino as usize) - 1;
		let bg = ino_idx / raw_sb.inodes_per_group as usize;
		let idx = ino_idx % raw_sb.inodes_per_group as usize;
		let byte_offset = idx.checked_mul(inode_size).ok_or(KernelErrors::EINVAL)?;
		let block_offset = (byte_offset / fs_blk_size) as u64;
		let offset_in_block = byte_offset % fs_blk_size;

		if offset_in_block + core::mem::size_of::<Ext2InodeRaw>() > fs_blk_size {
			return Err(KernelErrors::EINVAL);
		}

		let table_start_block = bgdt
			.table
			.get(bg)
			.ok_or(KernelErrors::EINVAL)?
			.inode_table as u64;
		let mut blkbuf = vec![0u8; fs_blk_size];
		let disk_lba = lba_conv(fs_blk_size as u64, dev.block_size() as u64, table_start_block + block_offset);
		dev.read_block(disk_lba, 1, &mut blkbuf)?;

		let inode = unsafe {
			core::ptr::read_unaligned(
				blkbuf.as_ptr().add(offset_in_block) as *const Ext2InodeRaw,
			)
		};

		klog!("ext2 root ino type: {:#x}", inode.i_mode);

		Ok(inode)
	}

	pub fn alloc_block(&self, prefered_bg: usize) -> Result<u32, KernelErrors> {
		let mut meta = self.meta.lock();
		if meta.raw_sb.free_blocks == 0 {
			return Err(KernelErrors::ENOSPC);
		}
		let num_groups = meta.bgdt.table.len();
		if num_groups == 0 {
			return Err(KernelErrors::EIO);
		}

		let start_bg = prefered_bg % num_groups;
		let block_size = self.block_size as usize;
		let blocks_per_group = meta.raw_sb.blocks_per_group;
		let first_data_block = meta.raw_sb.super_block_id;
		let total_block = meta.raw_sb.total_blocks;

		let mut bitmap_buf = vec![0u8; block_size];

		for i in 0..num_groups {
			let bg = (start_bg + i) % num_groups;
			if meta.bgdt.table[bg].nb_free_blocks == 0 {
				continue;
			}

			let group_first_block = first_data_block + (bg as u32) * blocks_per_group;
			if group_first_block >= total_block {
				continue;
			}

			let valid_blocks_in_group = (total_block - group_first_block).min(blocks_per_group);
			let bitmap_block = meta.bgdt.table[bg].block_bitmap as u64;
			self.read_block(bitmap_block, &mut bitmap_buf)?;

			if let Some(bit_idx) = find_first_zero_bit(&bitmap_buf, valid_blocks_in_group as usize) {
				let byte_idx = bit_idx / 8;
				let bit_in_byte = bit_idx % 8;
				bitmap_buf[byte_idx] |= 1 << bit_in_byte;
				self.write_block(bitmap_block, &bitmap_buf);
				meta.bgdt.table[bg].nb_free_blocks -= 1;
				meta.raw_sb.free_blocks -= 1;
				self.flush_bgdt_entry(&meta.bgdt, bg, meta.raw_sb.super_block_id)?;
				self.flush_superblock(&meta.raw_sb)?;
				let allocated_block = group_first_block + (bit_idx as u32);
				return Ok(allocated_block);
			}
		}
		Err(KernelErrors::ENOSPC)
	}

	pub fn alloc_inode(&self, prefered_bg: usize, is_dir: bool) -> Result<InodeId, KernelErrors> {
		let mut meta = self.meta.lock();
		if meta.raw_sb.free_inodes == 0 {
			return Err(KernelErrors::ENOSPC);
		}

		let num_groups = meta.bgdt.table.len();
		if num_groups == 0 {
			return Err(KernelErrors::EIO);
		}

		let start_bg = prefered_bg % num_groups;
		let block_size = self.block_size as usize;
		let inodes_per_group = meta.raw_sb.inodes_per_group;

		let mut bitmap_buf = alloc::vec![0u8; block_size];

		for i in 0..num_groups {
			let bg = (start_bg + i) % num_groups;
			if meta.bgdt.table[bg].nb_free_inodes == 0 {
				continue;
			}

			let bitmap_block = meta.bgdt.table[bg].inode_bitmap as u64;
			self.read_block(bitmap_block, &mut bitmap_buf)?;

			if let Some(bit_idx) = find_first_zero_bit(&bitmap_buf, inodes_per_group as usize) {
				bitmap_buf[bit_idx / 8] |= 1 << (bit_idx % 8);
				self.write_block(bitmap_block, &bitmap_buf)?;
				meta.bgdt.table[bg].nb_free_inodes -= 1;
				if is_dir {
					meta.bgdt.table[bg].nb_dir += 1;
				}
				meta.raw_sb.free_inodes -= 1;

				let superblock_id = meta.raw_sb.super_block_id;
				self.flush_bgdt_entry(&meta.bgdt, bg, superblock_id)?;
				self.flush_superblock(&meta.raw_sb)?;

				let allocated_ino = (bg as u32) * inodes_per_group + (bit_idx as u32) + 1;
				return Ok(allocated_ino as InodeId);
			}
		}
		Err(KernelErrors::ENOSPC)
	}

	pub fn get_or_alloc_block(&self, i_block: &mut [u32; 15], blocks_512: &mut u32, file_offset: u64, preferred_bg: usize) -> Result<u32, KernelErrors> {
		let block_size = self.block_size as u64;
		let logical_block = file_offset / block_size;
		let ptrs_per_block = block_size / 4;
		let sectors_per_block = (block_size / 512) as u32;

		let single_limit = 12 + ptrs_per_block;
		let double_limit = single_limit + (ptrs_per_block * ptrs_per_block);
		let triple_limit = double_limit + (ptrs_per_block * ptrs_per_block * ptrs_per_block);

		let mut alloc_zeroed = || -> Result<u32, KernelErrors> {
			let new_blk = self.alloc_block(preferred_bg)?;
			let zero_buf = alloc::vec![0u8; block_size as usize];
			self.write_block(new_blk as u64, &zero_buf)?;
			*blocks_512 = blocks_512.checked_add(sectors_per_block).ok_or(KernelErrors::EFBIG)?;
			Ok(new_blk)
		};

		let mut ensure_indirect_entry = 
			|parent_block: u32, index: u64, alloc_fn: &mut dyn FnMut() -> Result<u32, KernelErrors>| -> Result<u32, KernelErrors> {
				let mut buf = alloc::vec![0u8; block_size as usize];
				self.read_block(parent_block as u64, &mut buf)?;
				let off = (index as usize) * 4;
				let existing = u32::from_le_bytes([buf[off], buf[off+1], buf[off+2],buf[off+3]]);
				if existing != 0 {
					return Ok(existing);
				}

				let new_blk = alloc_fn()?;
				buf[off..off+4].copy_from_slice(&new_blk.to_le_bytes());
				self.write_block(parent_block as u64, &buf)?;
				Ok(new_blk)
			};
			
		if logical_block < 12 {
			let slot = logical_block as usize;
			if i_block[slot] == 0 {
				i_block[slot] = alloc_zeroed()?;
			}
			Ok(i_block[slot])
		} else if logical_block < single_limit {
			if i_block[12] == 0 {
				i_block[12] = alloc_zeroed()?;
			}
			let idx = logical_block - 12;
			ensure_indirect_entry(i_block[12], idx, &mut alloc_zeroed)
		} else if logical_block < double_limit {
			if i_block[13] == 0 {
				i_block[13] = alloc_zeroed()?;
			}
			let idx = logical_block - single_limit;
			let l1 = ensure_indirect_entry(i_block[13], idx / ptrs_per_block, &mut alloc_zeroed)?;
			ensure_indirect_entry(l1, idx % ptrs_per_block, &mut alloc_zeroed)
		} else if logical_block < triple_limit {
			if i_block[14] == 0 {
				i_block[14] = alloc_zeroed()?;
			}
			let idx = logical_block - double_limit;
			let ptrs_sq = ptrs_per_block * ptrs_per_block;
			let l1 = ensure_indirect_entry(i_block[14], idx / ptrs_sq, &mut alloc_zeroed)?;
			let l2 = ensure_indirect_entry(l1, (idx / ptrs_per_block) % ptrs_per_block, &mut alloc_zeroed)?;
			ensure_indirect_entry(l2, idx % ptrs_per_block, &mut alloc_zeroed)
		} else {
			Err(KernelErrors::EFBIG)
		}
	}

	pub fn flush_superblock(&self, raw_sb: &Ext2SuperblockRaw) -> Result<(), KernelErrors> {
		let mut raw = [0u8; 1024];
		unsafe {
			core::ptr::write_unaligned(raw.as_mut_ptr() as *mut Ext2SuperblockRaw, *raw_sb);
		}

		let dev_blk_size = self.dev.block_size();
		let fs_blk_size = self.block_size;

		if fs_blk_size <= 1024 {
			let lba = (1024 / dev_blk_size) as u64;
			let count = 1024 / dev_blk_size;
			self.dev.write_block(lba, count, &raw)?;
		} else {
			let mut block0 = alloc::vec![0u8; fs_blk_size as usize];
			let count = fs_blk_size / dev_blk_size;
			self.dev.read_block(0, count, &mut block0)?;
			block0[1024..2048].copy_from_slice(&raw);
			self.dev.write_block(0, count, &block0)?;
		}

		Ok(())
	}

	pub fn flush_bgdt_entry(&self, bgdt: &Ext2BGDT, bg: usize, superblock_id: u32) -> Result<(), KernelErrors> {
		const BGD_ENTRY_SIZE: usize = size_of::<Ext2BGDRaw>();
		let entry = bgdt.table.get(bg).ok_or(KernelErrors::EINVAL)?;
		let fs_blk_size = self.block_size as usize;

		let byte_offset = bg * BGD_ENTRY_SIZE;
		let block_offset = (byte_offset / fs_blk_size) as u64;
		let offset_in_block = byte_offset % fs_blk_size;

		let target_fs_block = (superblock_id as u64) + 1 + block_offset;

		let mut blkbuf = alloc::vec![0u8; fs_blk_size];
		self.read_block(target_fs_block, &mut blkbuf)?;

		unsafe {
			core::ptr::write_unaligned(
				blkbuf.as_mut_ptr().add(offset_in_block) as *mut Ext2BGDRaw, *entry);
		}
		self.write_block(target_fs_block, &blkbuf)?;
		Ok(())
	}
}

fn find_first_zero_bit(bitmap: &[u8], max_bits: usize) -> Option<usize> {
	let full_bytes = max_bits / 8;
	for (byte_idx, &byte) in bitmap.iter().take(full_bytes).enumerate()  {
		if byte != 0xFF {
			let bit = byte.trailing_ones() as usize;
			return Some(byte_idx * 8 + bit);
		}
	}

	let rem_bits = max_bits % 8;
	if rem_bits > 0 && full_bytes < bitmap.len() {
		let last_byte = bitmap[full_bytes];
		for bit in 0..rem_bits {
			if (last_byte & (1 << bit)) == 0 {
				return Some(full_bytes * 8 + bit);
			}
		}
	}

	None
}