use spin::Mutex;

use crate::{errno::KernelErrors, structures::buffers::CircularBuffer};

pub static STDIN_BUFFER: Mutex<CircularBuffer<u8,128>> = Mutex::new(CircularBuffer::new());

pub fn getchar() -> Result<u8, KernelErrors> {
	loop {
		if let Some(mut lock) = STDIN_BUFFER.try_lock() {
			return lock.pop()
		}
		unsafe { core::arch::asm!("pause") }
	}
}