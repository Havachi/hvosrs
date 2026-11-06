#![no_std]
#![no_main]

use uhv::*;



#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    print("> ");
	loop {
		let mut c = getchar();
		while c != b'\n' {
			write(1, &[c]);
			c = getchar();
		}
    	print("\n> ");
	}
	exit(42);
	loop {
		unsafe { core::arch::asm!("pause", options(nomem, nostack)) };
	}
}

