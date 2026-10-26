#[inline]
pub unsafe fn mmio_read<T>(base: *const u8, offset: usize) -> T {
	unsafe {
		let target = base.add(offset) as *mut T;
		core::ptr::read_volatile::<T>(target)
	}
}

#[inline]
pub unsafe fn mmio_write<T>(base: *mut u8, offset: usize, data: T) {
	unsafe {
	let target = base.add(offset) as *mut T;
	 core::ptr::write_volatile::<T>(target, data)
	}
}

pub fn current_cr3() -> u64 {
	use x86_64::registers::control::Cr3;
	let (cr3, _) = Cr3::read();
	cr3.start_address().as_u64()
}