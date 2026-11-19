use core::cmp::min;

use alloc::{string::String, sync::{Arc, Weak}, vec};
use bitflags::bitflags;

use crate::{errno::KernelErrors, fs::{dirent::DirentTypes, ext2::{EXT2_N_BLOCKS, EXT2_ROOT_INO, superblock::Ext2Sb}}, vfs::{InodeId, dentry::{VfsDirent, try_push_dirent}, inode::{Inode, VfsInodeType}}};


#[repr(C, align(128))]
#[derive(Debug, Clone, Copy)]
pub struct Ext2InodeRaw {
	///	Type and Permissions 
	pub i_mode: Ext2InodeMode,
	/// User ID
	pub i_uid: u16,
	/// Lower 32 bits of size in bytes
	pub i_size: u32,
	/// Last Access Time 
	pub i_atime: u32,
	/// Creation Time 
	pub i_ctime: u32,
	/// Last Modification time 
	pub i_mtime: u32,
	/// Deletion time
	pub i_dtime: u32,
	/// Group ID
	pub i_gid: u16,
	/// Count of hard links (dentry)
	pub i_lnk_count: u16,
	/// Count of disk sectors in use by this inode
	pub i_sect_count: u32,
	/// Flags
	pub i_flags: Ext2InodeFlags,
	/// OS specific value #1
	pub i_osd1: u32,
	/// Pointers to blocks
	pub i_block: [u32; EXT2_N_BLOCKS],
	/// File Version (for NFS)
	pub i_gen: u32,
	/// File ACL
	pub i_file_acl: u32,
	/// Dir ACL
	pub i_dir_acl: u32,
	/// Fragment address
	pub i_faddr: u32,

	pub osd2: [u8; 12],
}

bitflags! {
	#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
	pub struct Ext2InodeMode: u16 {
		const FIFO			= 0x1000;
		const CharDev		= 0x2000;
		const Directory		= 0x4000;
		const BlkDev		= 0x6000;
		const Regular		= 0x8000;
		const SymLink		= 0xA000;
		const Socket		= 0xC000;

		const OtherExecute	= 0x001;
		const OtherWrite	= 0x002;
		const OtherRead		= 0x004;

		const GroupExecute	= 0x008;
		const GroupWrite	= 0x010;
		const GroupRead		= 0x020;

		const UserExecute	= 0x040;
		const UserWrite		= 0x080;
		const UserRead		= 0x100;

		const StickyBit		= 0x200;
		const SetGID		= 0x400;
		const SetUID		= 0x800;
	}
}

impl Into<VfsInodeType> for Ext2InodeMode  {
	fn into(self) -> VfsInodeType {
		if self.contains(Ext2InodeMode::FIFO)		{ return VfsInodeType::FIFO }
		if self.contains(Ext2InodeMode::CharDev)	{ return VfsInodeType::CharDev }
		if self.contains(Ext2InodeMode::Directory)	{ return VfsInodeType::Directory }
		if self.contains(Ext2InodeMode::BlkDev)		{ return VfsInodeType::BlockDev }
		if self.contains(Ext2InodeMode::Regular)	{ return VfsInodeType::Regular }
		if self.contains(Ext2InodeMode::SymLink)	{ return VfsInodeType::Symlink }
		if self.contains(Ext2InodeMode::Socket)		{ return VfsInodeType::Socket }
		VfsInodeType::Unknown
	}
}

bitflags! {
	#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
	pub struct Ext2InodeFlags: u32 {
		///Secure deletion (not used)
		const SecureDelete		= 0x00000001;
		///Keep a copy of data when deleted (not used)
		const KeepCopy			= 0x00000002;
		///File compression (not used)
		const FileComp			= 0x00000004;
		///Synchronous updates—new data is written immediately to disk
		const Sync				= 0x00000008;
		///Immutable file (content cannot be changed)
		const Immutable			= 0x00000010;
		///Append only
		const AppendOnly		= 0x00000020;
		///File is not included in 'dump' command
		const NoDump			= 0x00000040;
		///Last accessed time should not updated
		const NoUpdateAtime		= 0x00000080;
		///Hash indexed directory
		const HashDir			= 0x00010000;
		///AFS directory
		const AFSDir			= 0x00020000;
		///Journal file data
		const JournalData		= 0x00040000;
	}
}

#[derive(Debug)]
pub struct Ext2Vnode {
	ino: InodeId,
	raw: Ext2InodeRaw,
	sb: Weak<Ext2Sb>,
}

impl Ext2Vnode {
	pub fn from_raw(ino: InodeId, raw: Ext2InodeRaw, sb: Weak<Ext2Sb> ) -> Ext2Vnode {
		Ext2Vnode { ino, raw, sb }
	}

	pub fn new_root(sb: Weak<Ext2Sb>, raw: Ext2InodeRaw) -> Arc<Self> {
		Arc::new(		
			Self{
				ino: EXT2_ROOT_INO,
				raw,
				sb
		})
	}

	pub fn raw(&self) -> Ext2InodeRaw {
		self.raw
	}
}

unsafe impl Sync for Ext2Vnode{}
unsafe impl Send for Ext2Vnode{}

impl Inode for Ext2Vnode {
	fn lookup(self: alloc::sync::Arc<Self>, name: &str, parent: Option<alloc::sync::Arc<crate::vfs::dentry::VfsDentry>>)	-> Result<alloc::sync::Arc<crate::vfs::dentry::VfsDentry>, crate::errno::KernelErrors> {
		todo!()
	}

	fn create(self: alloc::sync::Arc<Self>, basename: &str) -> Result<(), crate::errno::KernelErrors> {
		todo!()
	}

	fn mknod(self: alloc::sync::Arc<Self>, name: &str, mode: u16, dev_id: crate::vfs::inode::DeviceId) -> Result<(), crate::errno::KernelErrors> {
		Err(KernelErrors::ENOSYS)
	}

	fn mkdir(self: alloc::sync::Arc<Self>, name: &str) -> Result<alloc::sync::Arc<dyn Inode>, crate::errno::KernelErrors> {
		todo!()
	}

	fn read_at(self: alloc::sync::Arc<Self>, offset: u64, buf: &mut[u8]) -> Result<usize, crate::errno::KernelErrors> {
		let sb = self.sb.upgrade().ok_or(KernelErrors::EINVAL)?;
		let file_size = self.raw.i_size as u64;

		if offset >= file_size || buf.is_empty() {
			return Ok(0);
		}

		let max_readable = (file_size - offset) as usize;
		let total_to_read = min(buf.len(), max_readable);
		
		let block_size = sb.clone().block_size() as u64;
		let mut block_buf = vec![0u8; block_size as usize];
		let mut bytes_read = 0usize;
		let mut current_offset = offset;
		while bytes_read < total_to_read {
			let offset_in_block = (current_offset % block_size) as usize;
			let remaining_in_block = (block_size as usize) - offset_in_block;
			let remaining_to_read = total_to_read - bytes_read;

			let chunk_len = min(remaining_in_block, remaining_to_read);
			let dst_slice = &mut buf[bytes_read..bytes_read + chunk_len];

			match sb.file_offset_to_block(&self.raw.i_block, current_offset)? {
				Some(phys_block) => {
					sb.read_block(phys_block, &mut block_buf)?;
					dst_slice.copy_from_slice(
						&block_buf[offset_in_block..offset_in_block+chunk_len]
					);
				}
				None => {
					dst_slice.fill(0);
				}
			}
			bytes_read += chunk_len;
			current_offset += chunk_len as u64 
		}

		Ok(0)
	}

	fn write_at(self: alloc::sync::Arc<Self>, offset: u64, buf: &[u8]) -> Result<usize, crate::errno::KernelErrors> {
		todo!()
	}

	fn is_dir(&self) -> bool {
		self.raw().i_mode.contains(Ext2InodeMode::Directory)
	}

	fn is_file(&self) -> bool {
		!self.is_dir()
	}

	fn id(&self) -> InodeId {
		self.ino
	}

	fn statfs(&self) -> Result<crate::vfs::superblock::Statfs, crate::errno::KernelErrors> {
		todo!()
	}

	fn len(&self) -> usize {
		self.raw.i_size as usize
	}

	fn itype(&self) -> crate::vfs::inode::VfsInodeType {
		self.raw().i_mode.into()
	}

	fn reset(&self) {
		todo!()
	}

	fn readdir(self: alloc::sync::Arc<Self>, index: u64) -> Result<Option<crate::vfs::dentry::VfsDirent>, crate::errno::KernelErrors> {
		todo!()
	}

	fn getdents(&self, offset: &mut u64, max_bytes: usize) -> Result<alloc::vec::Vec<u8>, crate::errno::KernelErrors> {

		if !self.is_dir() {
			return Err(KernelErrors::ENOTDIR);
		}

		let dir_size = self.len() as u64;
		if *offset >= dir_size || max_bytes == 0 {
			return Ok(vec::Vec::new());
		}

		let sb = self.sb.upgrade().ok_or(KernelErrors::EIO)?;
		let block_size = sb.clone().block_size();
		let mut list: vec::Vec<u8> = vec::Vec::new();
		let mut block_buf = vec![0u8; block_size as usize];
		let mut cached_block_idx: Option<u64> = None;
		let mut cur_offset = *offset;
		while cur_offset < dir_size {
			let block_idx = cur_offset / block_size;
			let in_block_off = (cur_offset % block_size) as usize;
			if in_block_off + 8 > block_size as usize {
				cur_offset = (block_idx + 1) * block_size;
				continue;
			}
			if cached_block_idx != Some(block_idx) {
				match sb.file_offset_to_block(&self.raw.i_block, block_idx * block_size)? {
					Some(phys_block) => sb.read_block(phys_block as u64, &mut block_buf)?,
					None => {
						cur_offset = (block_idx + 1) * block_size;
						continue;
					}
				}
				cached_block_idx = Some(block_idx);
			}
			let hdr = &block_buf[in_block_off..in_block_off + 8];
			let d_inode = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]);
			let d_size = u16::from_le_bytes([hdr[4], hdr[5]]) as usize;
			let d_name_len = hdr[6] as usize;
			let d_type = map_ext2_dentry_type(hdr[7]);

			if d_size < 8 || in_block_off + d_size > block_size as usize {
				return Err(KernelErrors::EIO);
			}
			let next_offset = cur_offset + (d_size as u64);
			if d_inode != 0 {
				if 8 + d_name_len > d_size {
					return Err(KernelErrors::EIO);
				}

				let name_bytes = &block_buf[in_block_off + 8..in_block_off + 8 + d_name_len];
				let name = String::from_utf8_lossy(name_bytes).into_owned();
				let entry = VfsDirent::new(d_inode as u64, next_offset, d_type, name);
				let mut dummy_idx = cur_offset;

				if !try_push_dirent(&mut list, &mut dummy_idx, max_bytes, entry)?{
					break;
				}
			}
			cur_offset = next_offset;
		}
		*offset = cur_offset;
		Ok(list)
	}

	fn stat(&self) -> Result<crate::vfs::Stat, crate::errno::KernelErrors> {
		let sb = self.sb.upgrade().ok_or(KernelErrors::EIO)?;
		Ok(crate::vfs::Stat {
			st_ino: self.ino,
			st_blksize: sb.block_size() as i64,
			st_blocks: self.raw.i_sect_count as i64,
			st_nlink: 1,
			st_size: self.raw.i_size as i64,
			st_mode: self.raw.i_mode.bits() as u32,
			..Default::default()
		})
	}
}

impl Ext2Vnode {


	
}

fn map_ext2_dentry_type(raw_type: u8) -> DirentTypes {
	match raw_type {
		1 => DirentTypes::Regular,
		2 => DirentTypes::Directory,
		3 => DirentTypes::CharDev,
		4 => DirentTypes::BlockDev,
		5 => DirentTypes::FIFO,
		6 => DirentTypes::Socket,
		7 => DirentTypes::Symlink,
		_ => DirentTypes::Unknown,
	}
}