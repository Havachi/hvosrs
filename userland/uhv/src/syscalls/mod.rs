mod macros;
use macros::*;

mod rax;
use rax::*;

pub fn read(fd: i64, buf: &mut [u8]) -> i64 {
	unsafe { syscall3(SYS_READ, fd as u64, buf.as_mut_ptr() as u64, buf.len() as u64) }
}

pub fn write(fd: i64, buf: &[u8]) {
	let _ = unsafe { syscall3(SYS_WRITE, fd as u64, buf.as_ptr() as u64, buf.len() as u64) };
}

pub fn write_str(fd: i64, s: &str) {
	let _ = unsafe { syscall3(SYS_WRITE, fd as u64, s.as_ptr() as u64, s.len() as u64) };
}

pub fn open(path: &[u8], flags: u64) -> i64 {
	let buf = path_bytes_cstr(path);
	unsafe { syscall2(SYS_OPEN, buf.as_ptr() as u64, flags) }
}

pub fn close(fd: i64) {
	let _ = unsafe { syscall1(SYS_CLOSE, fd as u64) };
}

pub fn lseek(fd: i64, off: i64, whence: i32) -> i64 {
	unsafe { syscall3(SYS_LSEEK, fd as u64, off as u64, whence as u64) }
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

pub fn getpid() -> u32 {
	unsafe { syscall0(SYS_GETPID) as u32 } 
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

pub fn getdents(fd: i64, buf: &mut [u8]) -> i64 {
	unsafe { syscall3(SYS_GETDENTS, fd as u64, buf.as_mut_ptr() as u64, buf.len() as u64)}
}

pub fn chdir(path: &str) -> i64 {
	let buf = path_cstr(path);
	unsafe { syscall1(SYS_CHDIR, buf.as_ptr() as u64) }
}

pub fn mkdir(path: &str) -> i64 {
	let buf = path_cstr(path);
	let e = unsafe { syscall1(SYS_MKDIR, buf.as_ptr() as u64) };
	if e != 0 {
		return -1;
	}
	return 0;
}

pub fn waitid(idtype: u64, id:u64, infop: u64, option: u64) -> i64 {
	unsafe {syscall4(SYS_WAITID,idtype, id, infop, option)}
}

fn path_bytes_cstr(path: &[u8]) -> alloc::vec::Vec<u8> {
	let end = path.iter().position(|&b| b == 0).unwrap_or(path.len());
	let mut buf = alloc::vec::Vec::with_capacity(end + 1);
	buf.extend_from_slice(&path[..end]);
	buf.push(0);
	buf
}

fn path_cstr(path: &str) -> alloc::vec::Vec<u8> {
	let mut buf = alloc::vec::Vec::with_capacity(path.len() + 1);
	buf.extend_from_slice(path.as_bytes());
	buf.push(0);
	buf
}