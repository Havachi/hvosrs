use alloc::sync::Arc;
use spin::Mutex;
use x86_64::VirtAddr;

use crate::{
	drivers::block::{BlockDevice, nvme::driver::NvmeDriver},
	memory::{PAGE_SIZE, vmm::virt_to_phys},
};

pub struct NvmeNamespace {
	nsid: u32,
	block_size: u32,
	block_count: u64,
	driver: Arc<Mutex<NvmeDriver>>,
}

impl NvmeNamespace {
	pub fn new(nsid: u32, block_size: u32, block_count: u64, driver: Arc<Mutex<NvmeDriver>>) -> Self {
		Self {
			nsid,
			block_size,
			block_count,
			driver,
		}
	}

	fn validate_io(&self, lba: u64, count: u32, buf_len: usize) -> Result<(), &'static str> {
		if count == 0 {
			return Err("zero block count");
		}
		let byte_len = (count as u64) * (self.block_size as u64);
		if byte_len > buf_len as u64 {
			return Err("buffer too small for block count");
		}
		let end = lba.checked_add(count as u64).ok_or("LBA overflow")?;
		if end > self.block_count {
			return Err("I/O past end of namespace");
		}
		Ok(())
	}

	/// Max blocks we can DMA to `ptr` without crossing a page boundary (PRP1 must be contiguous).
	fn max_blocks_in_page(ptr: *const u8, block_size: u32) -> u32 {
		let page_off = ptr as usize & (PAGE_SIZE - 1);
		let space = PAGE_SIZE - page_off;
		(space / block_size as usize) as u32
	}

	fn transfer_blocks(
		&mut self,
		lba: u64,
		count: u32,
		buf: &[u8],
		read: bool,
	) -> Result<(), &'static str> {
		self.validate_io(lba, count, buf.len())?;

		let block_size = self.block_size;
		let mut blocks_done = 0u32;
		let mut cur_lba = lba;

		while blocks_done < count {
			let byte_off = (blocks_done as u64) * (block_size as u64);
			let chunk_ptr = buf.as_ptr().wrapping_add(byte_off as usize);
			let chunk_blocks = Self::max_blocks_in_page(chunk_ptr, block_size)
				.min(count - blocks_done);
			let buf_phys = virt_to_phys(VirtAddr::from_ptr(chunk_ptr)).as_u64();

			let mut driver = self.driver.lock();
			let result = if read {
				driver.submit_io_read(self.nsid, cur_lba, chunk_blocks, buf_phys)
			} else {
				driver.submit_io_write(self.nsid, cur_lba, chunk_blocks, buf_phys)
			};
			result?;

			blocks_done += chunk_blocks;
			cur_lba += chunk_blocks as u64;
		}

		Ok(())
	}
}

impl BlockDevice for NvmeNamespace {
	fn block_size(&self) -> u32 {
		self.block_size
	}

	fn block_count(&self) -> u64 {
		self.block_count
	}

	fn read_block(&mut self, lba: u64, count: u32, buf: &mut [u8]) -> Result<(), &'static str> {
		self.transfer_blocks(lba, count, buf, true)
	}

	fn write_block(&mut self, lba: u64, count: u32, buf: &[u8]) -> Result<(), &'static str> {
		self.transfer_blocks(lba, count, buf, false)
	}
}
