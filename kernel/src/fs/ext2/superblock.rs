use core::{ptr};

use alloc::{slice, sync::Arc, vec};
use bitflags::bitflags;

use crate::{drivers::block::BlockDevice, errno::KernelErrors, fs::ext2::{EXT2_FSNAME, EXT2_ROOT_INO, group::Ext2BGDT, inode::{Ext2InodeRaw, Ext2Vnode}, lba_conv}, memory::allocate_n_pages, vfs::{InodeId, filesystem::FST_REG, superblock::{Statfs, Superblock}}};

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
		let blksz = blkdev.block_size();
		let raw_addr = allocate_n_pages(blksz as usize);
		let slice: &mut [u8] = unsafe {
			slice::from_raw_parts_mut(raw_addr.as_u64() as *mut u8, 1024)
		};
		let raw: &mut [u8;1024] = slice.try_into().map_err(|_| KernelErrors::ENOMEM)?;
		if blksz <= 1024 {
			let lba = (1024 / blksz) as u64;
			let count = 1024 / blksz;
			blkdev.read_block(lba, count, raw)?;
		} else {
			let mut sect = alloc::vec![0u8; blksz as usize];
			blkdev.read_block(0, 1, &mut sect)?;
			raw.copy_from_slice(&sect[1024..2048]);
		}
		let ar = raw.as_array::<1024>().ok_or(KernelErrors::ENOMEM)?;
		Self::from_bytes(ar)
	}

	pub fn from_bytes(raw: &[u8; 1024]) -> Result<Self, KernelErrors> {
		let sb = unsafe {
			let sb_ptr = raw.as_ptr() as *const Ext2SuperblockRaw;
			ptr::read_unaligned(sb_ptr)
		};
		let magic = sb.magic;
		if magic != 0xEF53 {
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

pub struct Ext2Sb {
	dev: Arc<dyn BlockDevice>,
	root: Arc<Ext2Vnode>,
	raw_sb: Ext2SuperblockRaw,
	bgdt: Ext2BGDT,
	block_size: u32,
	inode_size: u32,
}

impl Superblock for Ext2Sb {
	fn root(&self) -> Arc<dyn crate::vfs::inode::Inode> {
		self.root.clone()
	}

	fn statfs(&self) -> Result<crate::vfs::superblock::Statfs, KernelErrors> {
		Ok(Statfs {
			fs_type: FST_REG.lock().find_id(EXT2_FSNAME)?,
			blocksize: self.block_size as usize,
			total_blocks: self.raw_sb.total_blocks as u64,
			free_blocks: self.raw_sb.free_blocks as u64,
			total_inodes: self.raw_sb.total_inodes as u64,
			free_inodes: self.raw_sb.free_inodes as u64,
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
			Self {
				dev,
				raw_sb,
				bgdt,
				block_size,
				inode_size,
				root
			}
		}))
	}

	pub fn read_block(&self, lba: u64, buf: &mut vec::Vec<u8>) -> Result<(), KernelErrors> {
		if buf.len() < self.block_size as usize {
			return Err(KernelErrors::EINVAL);
		}
		self.dev.read_block(lba, 1, buf)?;
		Ok(())
	}

	pub fn read_n_blocks(&self, lba: u64, n: usize, buf: &mut vec::Vec<u8>) -> Result<(), KernelErrors> {
		let total_len = n * self.block_size as usize;
		if buf.len() < total_len { return Err(KernelErrors::EINVAL); }
		self.dev.read_block(lba, n as u32, buf)?;
		Ok(())
	}

	pub fn write_block(&self, lba: u64, buf: &vec::Vec<u8>) -> Result<(), KernelErrors> {
		if buf.len() < self.block_size as usize {
			return Err(KernelErrors::EINVAL);
		}
		self.dev.write_block(lba, 1, buf)?;
		Ok(())
	}

	pub fn write_n_blocks(&self, lba: u64, n: usize, buf: &vec::Vec<u8>) -> Result<(), KernelErrors> {
		let total_len = n * self.block_size as usize;
		if buf.len() < total_len { return Err(KernelErrors::EINVAL); }
		self.dev.write_block(lba, n as u32, buf)?;
		Ok(())
	}

	pub fn read_inode(self: Arc<Self>, ino: InodeId) -> Result<Ext2Vnode, KernelErrors> {
		let raw_inode = Self::read_raw_inode_from_parts(
			&self.dev,
			&self.raw_sb,
			&self.bgdt,
			ino,
		)?;
		Ok(Ext2Vnode::from_raw(ino, raw_inode, Arc::downgrade(&self)))
	}

	pub fn write_inode(self: Arc<Self>, ino: InodeId, inode: &Ext2Vnode) -> Result<(), KernelErrors> {
		if ino == 0 {
			return Err(KernelErrors::EINVAL);
		}

		let ino_idx = (ino as usize) - 1;
		let bg = ino_idx / self.raw_sb.inodes_per_group as usize;
		let idx = ino_idx % self.raw_sb.inodes_per_group as usize;

		let table_lba = self.bgdt.table.get(bg)
			.ok_or(KernelErrors::EINVAL)?
			.inode_table as u64;

		let inode_tbl_lba = lba_conv(
			self.block_size as u64,
			self.dev.block_size() as u64, 
			table_lba
		);
		
		let mut blkbuf = vec![0u8; self.block_size as usize];
        self.read_block(inode_tbl_lba, &mut blkbuf)?;
		let byte_offset = idx.checked_mul(self.inode_size as usize)
            .ok_or(KernelErrors::EINVAL)?;

        if byte_offset + self.inode_size as usize > blkbuf.len() {
            return Err(KernelErrors::EINVAL);
        }

		unsafe {
			let dest = blkbuf.as_mut_ptr().add(byte_offset) as *mut Ext2InodeRaw;
			core::ptr::write_unaligned(dest, *&inode.raw());
		}
		self.write_block(inode_tbl_lba, &blkbuf)?;
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
			return Err(KernelErrors::ENOENT);
		}

		let fs_blk_size = raw_sb.block_size() as usize;
		let dev_blk_size = dev.block_size() as usize;
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

		let target_lba = lba_conv(
			fs_blk_size as u64,
			dev_blk_size as u64,
			table_start_block + block_offset,
		);
		let sectors_per_block = (fs_blk_size / dev_blk_size) as u32;

		let mut blkbuf = vec![0u8; fs_blk_size];
		dev.read_block(target_lba, sectors_per_block, &mut blkbuf)?;

		let inode = unsafe {
			core::ptr::read_unaligned(
				blkbuf.as_ptr().add(offset_in_block) as *const Ext2InodeRaw,
			)
		};

		Ok(inode)
	}

}