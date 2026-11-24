use alloc::sync::Arc;
use spin::Mutex;
use x86_64::{PhysAddr, VirtAddr};

use crate::{
	drivers::block::{BlockDevice, nvme::driver::NvmeDriver}, errno::KernelErrors, memory::{PAGE_SIZE, mmio::mmio_alloc},
};
#[derive(Clone)]
pub struct NvmeNamespace {
	nsid: u32,
	block_size: u32,
	block_count: u64,
	driver: Arc<Mutex<NvmeDriver>>,
	dma_virt: VirtAddr,
	dma_phys: PhysAddr,
	dma_cap: usize,
}

impl NvmeNamespace {
	pub fn new(nsid: u32, block_size: u32, block_count: u64, driver: Arc<Mutex<NvmeDriver>>) -> Self {
		let Some((dma_phys, dma_virt)) = mmio_alloc(PAGE_SIZE) else { panic!("oom")};

		Self {
			nsid,
			block_size,
			block_count,
			driver,
			dma_virt: dma_virt,
			dma_phys: dma_phys,
			dma_cap: PAGE_SIZE
		}
	}

	fn validate_io(&self, lba: u64, count: u32, buf_len: usize) -> Result<(), KernelErrors> {
		if count == 0 {
			return Err(KernelErrors::EINVAL);
		}
		let byte_len = (count as u64) * (self.block_size as u64);
		if byte_len > buf_len as u64 {
			return Err(KernelErrors::EINVAL);
		}
		let end = lba.checked_add(count as u64).ok_or(KernelErrors::EFAULT)?;
		if end > self.block_count {
			return Err(KernelErrors::EIO);
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
		&self,
		lba: u64,
		count: u32,
		buf_ptr: *mut u8,
		buf_len: usize,
		read: bool,
	) -> Result<(), KernelErrors> {
		self.validate_io(lba, count, buf_len)?;
		let block_size = self.block_size as usize;
		let max_blocks_per_chunk = (self.dma_cap / block_size) as u32;
		if max_blocks_per_chunk == 0 {
			return Err(KernelErrors::EINVAL);
		}

		let dma_ptr = self.dma_virt.as_mut_ptr::<u8>();
		let dma_phys = self.dma_phys.as_u64();

		let mut driver = self.driver.lock();

		let mut block_done = 0u32;
		let mut cur_lba = lba;

		while block_done < count {
			let chunk_blocks = (count - block_done).min(max_blocks_per_chunk);
			let chunk_bytes = (chunk_blocks as usize) * block_size;
			let user_offset = (block_done as usize) * block_size;

			if read {
				driver.submit_io_read(self.nsid, cur_lba, chunk_blocks, dma_phys)?;
				unsafe {
					core::ptr::copy_nonoverlapping(
						dma_ptr, 
						buf_ptr.add(user_offset), 
						chunk_bytes
					);
				}
			} else {
				unsafe {
					core::ptr::copy_nonoverlapping(
						buf_ptr.add(user_offset), 
						dma_ptr, 
						chunk_bytes
					);
				}
				driver.submit_io_write(self.nsid, cur_lba, chunk_blocks, dma_phys)?;
			}
			block_done += chunk_blocks;
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

	fn read_block(&self, lba: u64, count: u32, buf: &mut [u8]) -> Result<(), KernelErrors> {
		self.transfer_blocks(lba, count, buf.as_mut_ptr(), buf.len(), true)
	}

	fn write_block(&self, lba: u64, count: u32, buf: &[u8]) -> Result<(), KernelErrors> {
		self.transfer_blocks(lba, count, buf.as_ptr() as *mut u8, buf.len(), false)
	}
}
