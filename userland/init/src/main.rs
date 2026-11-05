#![no_std]
#![no_main]
use uhv::*;

static PATH: &[u8] = b"/tmp/hello\0";

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {

	print("[INIT] Started\n");


	let dirs = ["bin", "boot", "dev", "etc", "lib", "media", "mnt", "opt", "run", "sbin", "srv", "tmp", "usr", "var"];

	for dir in dirs {
		mkdir(dir);
	}
	print("[INIT] Created basic fs layout\n");
	let f = fork();

	if f == 0 {
		let path = b"/bin/shell\0";
		let r = execve(path, core::ptr::null(), core::ptr::null());
		print("execve failed\n");
	}

	if f > 0 {
		print("parent process here\n");
	}

	loop {
		unsafe { core::arch::asm!("pause", options(nomem, nostack)) };
	}
}


