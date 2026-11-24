use core::cmp::min;

use alloc::{string::String, sync::{Arc, Weak}, vec};
use bitflags::bitflags;
use spin::Mutex;

use crate::{errno::KernelErrors, fs::{dirent::DirentTypes, ext2::{EXT2_N_BLOCKS, EXT2_ROOT_INO, superblock::Ext2Sb}}, vfs::{InodeId, dentry::{VfsDentry, VfsDirent, try_push_dirent}, inode::{Inode, VfsInodeType}}};


#[repr(C, align(128))]
#[derive(Debug, Clone, Copy, Default)]
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
	pub i_blocks: u32,
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

impl Ext2InodeRaw {
	pub fn to_raw(&self) -> Self {
		self.clone()
	}
}

bitflags! {
	#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
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

impl Ext2InodeMode {
	pub const TYPE_MASK: u16 = 0xF000;
	pub const PERM_MASK: u16 = 0x0FFF;
	pub fn file_type(self) -> u16 {
		self.bits() & Self::TYPE_MASK
	}

	pub fn to_vfs_inode_type(self) -> VfsInodeType {
		VfsInodeType::from((self.file_type() >> 12) as u8)
	}

	pub fn is_dir(self) -> bool {
		self.file_type() == Self::Directory.bits()
	}

	pub fn is_regular(self) -> bool {
		self.file_type() == Self::Regular.bits()
	}

	pub fn is_symlink(self) -> bool {
		self.file_type() == Self::SymLink.bits()
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
	#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
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
	data: Mutex<Ext2InodeRaw>,
	sb: Weak<Ext2Sb>,
}

impl Ext2Vnode {
	pub fn from_raw(ino: InodeId, raw: Ext2InodeRaw, sb: Weak<Ext2Sb> ) -> Ext2Vnode {
		Ext2Vnode { ino, data: Mutex::new(raw), sb }
	}

	pub fn new_root(sb: Weak<Ext2Sb>, raw: Ext2InodeRaw) -> Arc<Self> {
		
		Arc::new(		
			Self{
				ino: EXT2_ROOT_INO,
				data: Mutex::new(raw),
				sb
		})
	}

	pub fn raw(&self) -> Ext2InodeRaw {
		self.data.lock().clone()
	}
}

unsafe impl Sync for Ext2Vnode{}
unsafe impl Send for Ext2Vnode{}

impl Inode for Ext2Vnode {
	fn lookup(self: alloc::sync::Arc<Self>, name: &str, parent: Option<alloc::sync::Arc<crate::vfs::dentry::VfsDentry>>) -> Result<alloc::sync::Arc<crate::vfs::dentry::VfsDentry>, crate::errno::KernelErrors> {
		if !self.is_dir() {
			return Err(KernelErrors::ENOTDIR);
		}
		if name.is_empty() || name.len() > 255 || name.contains('/') {
			return Err(KernelErrors::EINVAL);
		}

		let sb = self.sb.upgrade().ok_or(KernelErrors::EIO)?;
		let block_size = sb.clone().block_size() as u64;
		let (i_block, dir_size) = {
			let data = self.data.lock();
			(data.i_block, data.i_size as u64)
		};

		let mut block_buf = vec![0u8; block_size as usize];
		let mut cur_offset = 0u64;

		while cur_offset < dir_size {
			let phys_block = match sb.file_offset_to_block(&i_block, cur_offset)? {
				Some(blk) => blk,
				None => {
					cur_offset += block_size;
					continue;
				}
			};

			sb.read_block(phys_block as u64, &mut block_buf)?;
			let mut in_block_off = 0usize;
			while in_block_off + 8 <= block_size as usize {
				let hdr = &block_buf[in_block_off..in_block_off + 8];
				let d_inode = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]);
				let rec_len = u16::from_le_bytes([hdr[4], hdr[5]]) as usize;
				let name_len = hdr[6] as usize;
				if rec_len < 8 || in_block_off + rec_len > block_size as usize {
					return Err(KernelErrors::EIO);
				}
				if d_inode != 0 && name_len == name.len() {
					if 8 + name_len > rec_len {
						return Err(KernelErrors::EIO);
					}
					let entry_name = &block_buf[in_block_off + 8..in_block_off + 8 + name_len];
					if entry_name == name.as_bytes() {
						let child_vnode = sb.read_inode(d_inode as InodeId)?;
						let dentry = VfsDentry::from_name(
							name,
							Arc::new(child_vnode),
							parent
						)?;
						return Ok(Arc::new(dentry));
					}
				}
				in_block_off += rec_len;
			}
			cur_offset += block_size;
		}
		Err(KernelErrors::ENOENT)
	}

	fn create(self: alloc::sync::Arc<Self>, basename: &str, mode: u16) -> Result<Arc<dyn Inode>, crate::errno::KernelErrors> {
		if !self.is_dir() {
			return Err(KernelErrors::ENOTDIR);
		}
		match self.clone().lookup(basename, None) {
			Ok(_) => return Err(KernelErrors::EEXIST),
			Err(KernelErrors::EINVAL) => {},
			Err(e) => return Err(e),
		}

		let sb = self.sb.upgrade().ok_or(KernelErrors::EIO)?;
		let inodes_per_group = sb.meta.lock().raw_sb.inodes_per_group as usize;
		let preferred_bg = ((self.ino as usize) - 1) / inodes_per_group;
		
		let new_ino = sb.alloc_inode(preferred_bg, false)?;

		let raw_ino = Ext2InodeRaw {
			i_mode: Ext2InodeMode::from_bits_truncate(0x8000 | mode),
			i_lnk_count: 1,
			i_size: 0,
			i_blocks: 0,
			i_block: [0u32; EXT2_N_BLOCKS],
			..Default::default()
		};

		sb.write_inode(new_ino, &raw_ino)?;
		self.add_dir_entry(basename, new_ino, VfsInodeType::Regular as u8)?;
		Ok(Arc::new(Ext2Vnode::from_raw(new_ino, raw_ino, self.sb.clone())))
	}

	fn mknod(self: alloc::sync::Arc<Self>, name: &str, mode: u16, dev_id: crate::vfs::inode::DeviceId) -> Result<(), crate::errno::KernelErrors> {
		Err(KernelErrors::ENOSYS)
	}

	fn mkdir(self: alloc::sync::Arc<Self>, name: &str, mode: u16) -> Result<alloc::sync::Arc<dyn Inode>, crate::errno::KernelErrors> {
		if !self.is_dir() {
			return Err(KernelErrors::ENOTDIR);
		}

		if self.clone().lookup(name, None).is_ok() {
			return Err(KernelErrors::EEXIST);
		}

		let sb = self.sb.upgrade().ok_or(KernelErrors::EIO)?;
		let inodes_per_group = sb.meta.lock().raw_sb.inodes_per_group as usize;
		let preferred_bg = ((self.ino as usize) - 1) / inodes_per_group;
		let block_size = sb.clone().block_size() as usize;

		let new_ino = sb.clone().alloc_inode(preferred_bg, true)?;
		let dir_block = sb.clone().alloc_block(preferred_bg)?;

		let mut dir_buf = alloc::vec![0u8; block_size];
		write_raw_dentry(
			&mut dir_buf[0..12],
			new_ino as u32,
			12,
			2,
			b"."
		);
		write_raw_dentry(
			&mut dir_buf[12..block_size], 
			self.ino as u32, 
			(block_size - 12) as u16, 
			VfsInodeType::Directory as u8, 
			b".."
		);
		
		sb.clone().write_block(dir_block as u64, &dir_buf)?;

		let mut raw_inode = Ext2InodeRaw {
			i_mode: Ext2InodeMode::from_bits_truncate(mode) | Ext2InodeMode::Directory,
			i_lnk_count: 2,
			i_size: block_size as u32,
			i_blocks: (block_size / 512) as u32,
			..Default::default()
		};
		raw_inode.i_block[0] = dir_block;

		self.add_dir_entry(name, new_ino, 2)?;

		{
			let mut parent_data = self.data.lock();
			parent_data.i_lnk_count += 1;
			sb.clone().write_inode(self.ino, &parent_data)?;
		}

		Ok(Arc::new(Ext2Vnode::from_raw(
			new_ino, 
			raw_inode,
			Arc::downgrade(&sb))))
	}

	fn read_at(self: alloc::sync::Arc<Self>, offset: u64, buf: &mut[u8]) -> Result<usize, crate::errno::KernelErrors> {
		let sb = self.sb.upgrade().ok_or(KernelErrors::EINVAL)?;
		let raw = self.data.lock();
		let file_size = raw.i_size as u64;

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

			match sb.file_offset_to_block(&raw.i_block, current_offset)? {
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
		if buf.is_empty() {
			return Ok(0);
		}

		let sb = self.sb.upgrade().ok_or(KernelErrors::EIO)?;
		let block_size = sb.clone().block_size() as u64;

		let mut data = self.data.lock();
		if data.i_mode.contains(Ext2InodeMode::Directory) {
			return Err(KernelErrors::EISDIR);
		}

		let inodes_per_group = sb.meta.lock().raw_sb.inodes_per_group as usize;
		let preferred_bg = ((self.ino as usize) - 1) / inodes_per_group;

		let mut block_buf = vec![0u8; block_size as usize];
		let mut bytes_written = 0usize;
		let mut current_offset = offset;

		let mut i_block = data.i_block;
		let mut i_blocks = data.i_blocks;

		while bytes_written < buf.len() {
			let offset_in_block = (current_offset % block_size) as usize;
			let remaining_in_block = (block_size as usize) - offset_in_block;
			let chunk_len = min(remaining_in_block, buf.len() - bytes_written);
			let phys_block = sb.get_or_alloc_block(
				&mut i_block,
				&mut i_blocks,
				current_offset,
				preferred_bg
			)?;

			if offset_in_block > 0 || chunk_len < (block_size as usize) {
				sb.read_block(phys_block as u64, &mut block_buf)?;
			}

			block_buf[offset_in_block..offset_in_block + chunk_len]
				.copy_from_slice(&buf[bytes_written..bytes_written + chunk_len]);
			sb.write_block(phys_block as u64, &block_buf)?;
			bytes_written += chunk_len;
			current_offset += chunk_len as u64;
		}
		if current_offset > data.i_size as u64 {
			data.i_size = current_offset as u32;
		}
		let raw_inode = data.to_raw();
		sb.write_inode(self.ino, &raw_inode)?;
		Ok(bytes_written)
	}

	fn is_dir(&self) -> bool {
		self.raw().i_mode.is_dir()
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
		self.raw().i_size as usize
	}

	fn itype(&self) -> crate::vfs::inode::VfsInodeType {
		self.raw().i_mode.to_vfs_inode_type()
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
				match sb.file_offset_to_block(&self.raw().i_block, block_idx * block_size)? {
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
		let raw: Ext2InodeRaw = self.raw();
		Ok(crate::vfs::Stat {
			st_ino: self.ino,
			st_blksize: sb.block_size() as i64,
			st_blocks: raw.i_blocks as i64,
			st_nlink: 1,
			st_size: raw.i_size as i64,
			st_mode: raw.i_mode.bits() as u32,
			..Default::default()
		})
	}
}

impl Ext2Vnode {
	fn add_dir_entry(&self, name: &str, child_ino: InodeId, file_type: u8) -> Result<(), KernelErrors> {
		if name.is_empty() || name.len() > 255 || name.contains('/') {
			return Err(KernelErrors::EINVAL);
		}

		let sb = self.sb.upgrade().ok_or(KernelErrors::EIO)?;
		let block_size = sb.clone().block_size() as usize;
		let needed_len = (8 + name.len() + 3) & !3;

		let mut data = self.data.lock();
		let mut i_block = data.i_block;
		let mut i_blocks = data.i_blocks;
		let dir_size = data.i_size as u64;

		let inodes_per_group = sb.meta.lock().raw_sb.inodes_per_group as usize;
		let preferred_bg = ((self.ino as usize) - 1) / inodes_per_group;

		let mut block_buf = alloc::vec![0u8; block_size];
		let mut cur_offset = 0u64;

		while cur_offset < dir_size {
			let phys_block = sb
				.file_offset_to_block(&i_block, cur_offset)?
				.ok_or(KernelErrors::EIO)?;
			sb.read_block(phys_block as u64, &mut block_buf)?;
			let mut in_block_off = 0usize;
			while in_block_off + 8 <= block_size {
				let hdr = &block_buf[in_block_off..in_block_off + 8];
				let entry_ino = u32::from_le_bytes([hdr[0],hdr[1],hdr[2],hdr[3]]);
				let rec_len = u16::from_le_bytes([hdr[4], hdr[5]]) as usize;
				let name_len = hdr[6] as usize;
				if rec_len < 8 || in_block_off + rec_len > block_size {
					return Err(KernelErrors::EIO);
				}

				let actual_len = if entry_ino == 0 {
					0
				} else {
					(8 + name_len + 3) & !3
				};
				let free_space = rec_len.saturating_sub(actual_len);

				if free_space >= needed_len{
					let insert_off = if entry_ino == 0 {
						in_block_off
					} else {
						block_buf[in_block_off + 4..in_block_off + 6]
							.copy_from_slice(&(actual_len as u16).to_le_bytes());
						in_block_off + actual_len
					};
					write_raw_dentry(
						&mut block_buf[insert_off..insert_off + free_space],
						child_ino as u32,
						free_space as u16,
						file_type,
						name.as_bytes(),
					);
					sb.write_block(phys_block as u64, &block_buf)?;
					return Ok(());
				}
				in_block_off += rec_len;
			}
			cur_offset += block_size as u64;
		}
		let new_phys_block = sb.get_or_alloc_block(
			&mut i_block,
			&mut i_blocks,
			dir_size,
			preferred_bg
		)?;
		block_buf.fill(0);
		write_raw_dentry(
			&mut block_buf,
			child_ino as u32,
			block_size as u16,
			file_type,
			name.as_bytes(),
		);
		sb.write_block(new_phys_block as u64, &block_buf)?;
		data.i_block = i_block;
		data.i_blocks = i_blocks;
		data.i_size = (dir_size + block_size as u64) as u32;
		sb.write_inode(self.ino, &data)?;
		Ok(())
	}


	
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

fn write_raw_dentry(dst: &mut [u8], inode: u32, rec_len: u16, file_type: u8, name: &[u8]) {
	dst[0..4].copy_from_slice(&inode.to_le_bytes());
	dst[4..6].copy_from_slice(&rec_len.to_le_bytes());
	dst[6] = name.len() as u8;
	dst[7] = file_type;
	dst[8..8 + name.len()].copy_from_slice(name);
}