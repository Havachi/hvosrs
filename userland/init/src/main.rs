#![no_std]
#![no_main]

const O_CREAT: u64 = 0o100;
const O_RDWR: u64 = 0o2;

const SYS_READ: u64 = 0;
const SYS_WRITE: u64 = 1;
const SYS_OPEN: u64 = 2;
const SYS_CLOSE: u64 = 3;
const SYS_LSEEK: u64 = 4;

const SEEK_SET: i32 = 0;

static PATH: &[u8] = b"/tmp/hello\0";

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
	loop {}
}

unsafe fn syscall3(nr: u64, a0: u64, a1: u64, a2: u64) -> i64 {
	let ret: i64;
	core::arch::asm!(
		"syscall",
		inout("rax") nr => ret,
		in("rdi") a0,
		in("rsi") a1,
		in("rdx") a2,
		lateout("rcx") _,
		lateout("r11") _,
	);
	ret
}

fn write_str(fd: i64, s: &str) {
	let _ = unsafe { syscall3(SYS_WRITE, fd as u64, s.as_ptr() as u64, s.len() as u64) };
}

fn write(fd: i64, buf: &[u8]) {
	let _ = unsafe { syscall3(SYS_WRITE, fd as u64, buf.as_ptr() as u64, buf.len() as u64) };
}

fn read(fd: i64, buf: &mut [u8]) -> i64 {
	unsafe { syscall3(SYS_READ, fd as u64, buf.as_mut_ptr() as u64, buf.len() as u64) }
}

fn lseek(fd: i64, off: i64, whence: i32) -> i64 {
	unsafe { syscall3(SYS_LSEEK, fd as u64, off as u64, whence as u64) }
}

fn close(fd: i64) {
	let _ = unsafe { syscall3(SYS_CLOSE, fd as u64, 0, 0) };
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
	write_str(1, "init: vfs syscalls test\n");

	let fd = unsafe { syscall3(SYS_OPEN, PATH.as_ptr() as u64, O_CREAT | O_RDWR, 0) };
	write_str(fd, "Hello World!");
	let mut buf = [0;255];
	lseek(fd, 0, 0);
	read(fd, &mut buf);
	write(1, &buf);
	loop {
		unsafe { core::arch::asm!("pause", options(nomem, nostack)) };
	}
}
