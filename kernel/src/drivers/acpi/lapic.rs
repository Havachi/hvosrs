pub struct LocalApic {
	base_addr: u64,
}

impl LocalApic {
	const SIVR: u32 = 0x0F0;
	const EOI: u32 = 0x0B0;

	pub fn new(base_addr: u64) -> Self {
		Self { base_addr }
	}

	unsafe fn read(&self, offset: u32) -> u32 {
		unsafe { core::ptr::read_volatile((self.base_addr + offset as u64) as *const u32) }
	}

	unsafe fn write(&self, offset: u32, val: u32) {
		unsafe { core::ptr::write_volatile((self.base_addr + offset as u64) as *mut u32, val); }
	}

	pub unsafe fn init(&self) {
		unsafe {
			let sivr = self.read(Self::SIVR);
			self.write(Self::SIVR, sivr | (1<<8) | 0xFF);
		}
	}

	pub unsafe fn eoi(&self) {
		unsafe {
			self.write(Self::EOI, 0);
		}
	}
}