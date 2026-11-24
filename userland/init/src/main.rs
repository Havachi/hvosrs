#![no_std]
#![no_main]
use uhv::*;

static PATH: &[u8] = b"/tmp/hello\0";

#[unsafe(no_mangle)]
extern "Rust" fn main(_: isize, _: *const *const u8) -> i32 {
	let f = fork();
	if f == 0 {
		let path = b"/bin/shell\0";
		let r = execve(path, core::ptr::null(), core::ptr::null());
		if r < 0 {
			println!("execve failed: {}\n", errno_to_str(r));
			exit(1);
		}
	}
	if f > 0 {
		let _ = waitid(0, 0, 0, 0);
	}
	loop {
		unsafe { core::arch::asm!("pause", options(nomem, nostack)) };
	}
}


