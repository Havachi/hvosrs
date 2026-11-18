use alloc::sync::{Arc, Weak};
use bitflags::bitflags;

use crate::{errno::KernelErrors, fs::ext2::{EXT2_N_BLOCKS, EXT2_ROOT_INO, superblock::Ext2Sb}, vfs::{InodeId, inode::{Inode, VfsInodeType}}};


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
		todo!()
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
		todo!()
	}

	fn stat(&self) -> Result<crate::vfs::Stat, crate::errno::KernelErrors> {
		todo!()
	}
}
