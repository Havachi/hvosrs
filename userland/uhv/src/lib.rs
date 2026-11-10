#![no_std]
#![no_main]

use core::{ptr::NonNull, sync::atomic::AtomicI64};

use alloc::string::String;
use talc::{TalcLock, base::Talc};

extern crate alloc;
pub const O_CREAT: u64 = 0o100;
pub const O_RDWR: u64 = 0o2;
const SYS_READ: u64 = 0;
const SYS_WRITE: u64 = 1;
const SYS_OPEN: u64 = 2;
const SYS_CLOSE: u64 = 3;
const SYS_LSEEK: u64 = 4;
const SYS_BRK: u64 = 12;
const SYS_GETPID: u64 = 39;
const SYS_FORK: u64 = 57;
const SYS_EXECVE: u64 = 59;
const SYS_EXIT: u64 = 60;
const SYS_GETDENTS: u64 = 78;
const SYS_CHDIR: u64 = 80;
const SYS_MKDIR: u64 = 83;
const SYS_WAITID: u64 = 247;
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

fn path_bytes_cstr(path: &[u8]) -> alloc::vec::Vec<u8> {
	let end = path.iter().position(|&b| b == 0).unwrap_or(path.len());
	let mut buf = alloc::vec::Vec::with_capacity(end + 1);
	buf.extend_from_slice(&path[..end]);
	buf.push(0);
	buf
}

pub fn open(path: &[u8], flags: u64) -> i64 {
	let buf = path_bytes_cstr(path);
	unsafe { syscall2(SYS_OPEN, buf.as_ptr() as u64, flags) }
}

pub fn close(fd: i64) {
	let _ = unsafe { syscall1(SYS_CLOSE, fd as u64) };
}

fn path_cstr(path: &str) -> alloc::vec::Vec<u8> {
	let mut buf = alloc::vec::Vec::with_capacity(path.len() + 1);
	buf.extend_from_slice(path.as_bytes());
	buf.push(0);
	buf
}

pub fn mkdir(path: &str) -> i64 {
	let buf = path_cstr(path);
	let e = unsafe { syscall1(SYS_MKDIR, buf.as_ptr() as u64) };
	if e != 0 {
		return -1;
	}
	return 0;
}

pub fn fork() -> i64 {
	unsafe { syscall0(SYS_FORK )}
}

pub fn execve(path: &[u8], argv: *const u8, envp: *const u8) -> i64 {
	let buf = path_bytes_cstr(path);
	unsafe { syscall3(SYS_EXECVE, buf.as_ptr() as u64, argv as u64, envp as u64) }
}

pub fn exit(code:u32) {
	unsafe { syscall1(SYS_EXIT, code as u64) };
}

pub fn waitid(idtype: u64, id:u64, infop: u64, option: u64) -> i64 {
	unsafe {syscall4(SYS_WAITID,idtype, id, infop, option)}
}

pub fn getpid() -> u32 {
	unsafe { syscall0(SYS_GETPID) as u32 } 
}

pub fn brk(addr: u64) -> i64 {
	unsafe { syscall1(SYS_BRK, addr as u64) }
}

pub fn sbrk(increment: u64) -> i64 {
	let old = brk(0 );
	let ret: i64 = brk((old + increment as i64) as u64);
	if ret < 0 {
		return ret;
	}
	return old;
}

pub fn chdir(path: &str) -> i64 {
	let buf = path_cstr(path);
	unsafe { syscall1(SYS_CHDIR, buf.as_ptr() as u64) }
}

pub fn getdents(fd: i64, buf: &mut [u8]) -> i64 {
	unsafe { syscall3(SYS_GETDENTS, fd as u64, buf.as_mut_ptr() as u64, buf.len() as u64)}
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

pub fn getchar() -> u8 {
	let mut b = [0u8];
	let res = read(0, &mut b);
	b[0]
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
	print("userland panic\n");
	exit(1);
	loop {}
}

pub const USER_HEAP_START: u64 = 0x0000_0000_4000_0000;
#[global_allocator]
static ALLOCATOR: TalcLock<spin::Mutex<()>, UserHeapSource> = TalcLock::new(UserHeapSource::new());

#[derive(Debug)]
pub struct UserHeapSource {
	pub current_brk: u64,
	pub current_heap_end: Option<NonNull<u8>>,
}

unsafe impl Sync for UserHeapSource{}
unsafe impl Send for UserHeapSource{}

unsafe impl talc::source::Source for UserHeapSource {
	fn acquire<B: talc::base::binning::Binning>(talc: &mut Talc<Self, B>, layout: core::alloc::Layout) -> Result<(), ()> {
		let needed = layout.size().next_multiple_of(4096);
		let old = sbrk(needed as u64);
		if old < 0 {
			return Err(());
		}
		let new_break = old as u64 + needed as u64 ;
		talc.source.current_brk = new_break as u64;
		unsafe {
			if let Some(old_end) = talc.source.current_heap_end {
				let res = talc.extend(old_end, new_break as *mut u8);
				talc.source.current_heap_end = Some(res);
			} else {
				let res = talc.claim(old as *mut u8, needed).ok_or(())?;
				talc.source.current_heap_end = Some(res);
			}
		}
		return Ok(());
	}
}

impl UserHeapSource {
	pub const fn new() -> Self {
		Self { current_brk: USER_HEAP_START, current_heap_end: None  }
	}
}

pub struct Stdout;

impl core::fmt::Write for Stdout {
	fn write_str(&mut self, s: &str) -> core::fmt::Result {
		write(1, s.as_bytes());
		Ok(())
	}
}

pub struct Stderr;

impl core::fmt::Write for Stderr {
	fn write_str(&mut self, s: &str) -> core::fmt::Result {
		write(2, s.as_bytes());
		Ok(())
	}
}

pub fn _print(args: core::fmt::Arguments) {
	use core::fmt::Write;
	let _ = Stdout.write_fmt(args);
}

pub fn _eprint(args: core::fmt::Arguments) {
	use core::fmt::Write;
	let _ = Stderr.write_fmt(args);
}

#[macro_export]
macro_rules! print {
	($($arg:tt)*) => { $crate::_print(format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! println {
	() => { $crate::print!("\n") };
	($($arg:tt)*) => { $crate::print!("{}\n", format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! eprint {
	($($arg:tt)*) => { $crate::_eprint(format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! eprintln {
	() => { $crate::eprint!("\n") };
	($($arg:tt)*) => { $crate::eprint!("{}\n", format_args!($($arg)*)) };
}

pub fn errno_to_str(err: i64) -> &'static str {
	match err {
		-1 => "Permission Denied",
		-2 => "No such file or directory",
		-9 => "Bad file descriptor",
		-10 => "No child processes",
		-11 => "Resource temporarily unavailable",
		-12 => "Cannot allocate memory",
		-13 => "Permission Denied",
		-14 => "Bad address",
		-16 => "Ressource or device busy",
		-17 => "File Exist",
		-20 => "Not a directory",
		-21 => "Is a directory",
		-22 => "Invalid argument",
		_ => "unknown error"
	}
}


pub fn join_pwd(current: &str, arg: &str) -> String {
	let mut parts: alloc::vec::Vec<&str> = alloc::vec::Vec::new();
	if !arg.starts_with('/') {
		for p in current.split('/') {
			if !p.is_empty() { parts.push(p);}
		}
	}
	for p in arg.split('/') {
		match p {
			"" | "." => {}
			".." => { parts.pop(); }
			name => parts.push(name),
		}	
	} 
	if parts.is_empty() {
		return alloc::string::String::from("/");
	}
	let mut out = alloc::string::String::new();
	for p in parts {
		out.push('/');
		out.push_str(p);
	}
	out
}