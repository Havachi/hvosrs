#![no_std]
#![no_main]
use uhv::*;

static PATH: &[u8] = b"/tmp/hello\0";

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {

	print("[INIT] Started\n");


	let dirs = ["boot", "dev", "etc", "lib", "media", "mnt", "opt", "run", "sbin", "srv", "tmp", "usr", "var"];

	for dir in dirs {
		if mkdir(dir) != 0 {
			eprintln!("error creating dir {}", dir)
		}
	}
	//print("[INIT] Created basic fs layout\n");
	let f = fork();
	if f == 0 {
		let path = b"/bin/shell\0";
		let r = execve(path, core::ptr::null(), core::ptr::null());
		if r < 0 {
			print("execve failed\n");
			exit(1);
		}
	}
	if f > 0 {
		print("[INIT] parent process here\n");
		let _ = waitid(0, 0, 0, 0);
		print("[INIT] shell has exited\n");
	}
	loop {
		unsafe { core::arch::asm!("pause", options(nomem, nostack)) };
	}
}


