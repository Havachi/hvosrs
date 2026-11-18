use crate::errno::KernelErrors;

pub struct AtaPioDevice {
	block_size: u32,
	block_count: u64,
}

impl super::BlockDevice for AtaPioDevice {
	fn block_size(&self) -> u32 { self.block_size }
	fn block_count(&self) -> u64 { self.block_count }
	fn read_block(&self, _lba: u64, _count: u32, _buf: &mut [u8]) -> Result<(), KernelErrors> {
		Ok(())
	}

	fn write_block(&self, _lba: u64, _count: u32, _buf: &[u8]) -> Result<(), KernelErrors> {
		Ok(())
	}
}	
