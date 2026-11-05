#![no_std]
#![no_main]

use uhv::print;

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    print("[SHELL] shell started successfully");
	loop {
		unsafe { core::arch::asm!("pause", options(nomem, nostack)) };
	}
}

