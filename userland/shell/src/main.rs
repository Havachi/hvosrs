#![no_std]
#![no_main]

use uhv::{exit, print};

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    print("[SHELL] shell started successfully\n");
	exit(42);
	loop {
		unsafe { core::arch::asm!("pause", options(nomem, nostack)) };
	}
}

