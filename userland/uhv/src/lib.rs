#![no_std]
#![no_main]

pub const O_CREAT: u64 = 0o100;
pub const O_RDWR: u64 = 0o2;
const SYS_READ: u64 = 0;
const SYS_WRITE: u64 = 1;
const SYS_OPEN: u64 = 2;
const SYS_CLOSE: u64 = 3;
const SYS_LSEEK: u64 = 4;
const SYS_FORK: u64 = 57;
const SYS_EXECVE: u64 = 59;
const SYS_MKDIR: u64 = 83;
pub const SEEK_SET: i32 = 0;

pub fn write_str(fd: i64, s: &str) {
	let _ = unsafe { syscall3(SYS_WRITE, fd as u64, s.as_ptr() as u64, s.len() as u64) };
}

pub fn write(fd: i64, buf: &[u8]) {
	let _ = unsafe { syscall3(SYS_WRITE, fd as u64, buf.as_ptr() as u64, buf.len() as u64) };
}

pub fn print(str: &str) {
	write_str(1, str);
}

pub fn read(fd: i64, buf: &mut [u8]) -> i64 {
	unsafe { syscall3(SYS_READ, fd as u64, buf.as_mut_ptr() as u64, buf.len() as u64) }
}

pub fn lseek(fd: i64, off: i64, whence: i32) -> i64 {
	unsafe { syscall3(SYS_LSEEK, fd as u64, off as u64, whence as u64) }
}

pub fn open(path: &[u8], flags: u64) -> i64 {
	unsafe { syscall2(SYS_OPEN, path.as_ptr() as u64, flags) }
}

pub fn close(fd: i64) {
	let _ = unsafe { syscall1(SYS_CLOSE, fd as u64) };
}

pub fn mkdir(path: &str) {
	let _ = unsafe { syscall1(SYS_MKDIR, path.as_ptr() as u64) };
}

pub fn fork() -> i64 {
	unsafe { syscall0(SYS_FORK )}
}

pub fn execve(path: &[u8], argv: *const u8, envp: *const u8) -> i64 {
	unsafe { syscall3(SYS_EXECVE, path.as_ptr() as u64, argv as u64, envp as u64) }
}

unsafe fn syscall0(nr: u64) -> i64 {
	let ret: i64;
	unsafe {	
		core::arch::asm!(
			"syscall",
			inout("rax") nr => ret,
			lateout("rcx") _,
			lateout("r11") _,
		);
	}
	ret
}

unsafe fn syscall1(nr: u64, a0: u64) -> i64 {
	let ret: i64;
	unsafe {	
		core::arch::asm!(
			"syscall",
			inout("rax") nr => ret,
			in("rdi") a0,
			lateout("rcx") _,
			lateout("r11") _,
		);
	}
	ret
}

unsafe fn syscall2(nr: u64, a0: u64, a1: u64) -> i64 {
	let ret: i64;
	unsafe {	
		core::arch::asm!(
			"syscall",
			inout("rax") nr => ret,
			in("rdi") a0,
			in("rsi") a1,
			lateout("rcx") _,
			lateout("r11") _,
		);
	}
	ret
}

unsafe fn syscall3(nr: u64, a0: u64, a1: u64, a2: u64) -> i64 {
	let ret: i64;
	unsafe {	
		core::arch::asm!(
			"syscall",
			inout("rax") nr => ret,
			in("rdi") a0,
			in("rsi") a1,
			in("rdx") a2,
			lateout("rcx") _,
			lateout("r11") _,
		);
	}
	ret
}

unsafe fn syscall4(nr: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> i64 {
	let ret: i64;
	unsafe {	
		core::arch::asm!(
			"syscall",
			inout("rax") nr => ret,
			in("rdi") a0,
			in("rsi") a1,
			in("rdx") a2,
			in("r10") a3,
			lateout("rcx") _,
			lateout("r11") _,
		);
	}
	ret
}

unsafe fn syscall5(nr: u64, a0: u64, a1: u64, a2: u64, a3: u64, a4: u64) -> i64 {
	let ret: i64;
	unsafe {	
		core::arch::asm!(
			"syscall",
			inout("rax") nr => ret,
			in("rdi") a0,
			in("rsi") a1,
			in("rdx") a2,
			in("r10") a3,
			in("r8") a4,
			lateout("rcx") _,
			lateout("r11") _,
		);
	}
	ret
}

unsafe fn syscall6(nr: u64, a0: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64) -> i64 {
	let ret: i64;
	unsafe {	
		core::arch::asm!(
			"syscall",
			inout("rax") nr => ret,
			in("rdi") a0,
			in("rsi") a1,
			in("rdx") a2,
			in("r10") a3,
			in("r8") a4,
			in("r9") a5,
			lateout("rcx") _,
			lateout("r11") _,
		);
	}
	ret
}


#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
	loop {}
}