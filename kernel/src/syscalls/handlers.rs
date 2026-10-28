use core::str;

use crate::{
	drivers::{console, serial}, errno::KernelErrors, ksprint, scheduler::{alloc_fd, with_current_process}, syscalls::{
		errno::EFAULT,
		user_access::{copy_from_user_into, copy_to_user, copy_user_cstr},
	}, vfs::{self, SeekOrigin},
};

pub const SYS_READ: usize = 0;
pub const SYS_WRITE: usize = 1;
pub const SYS_OPEN: usize = 2;
pub const SYS_CLOSE: usize = 3;
pub const SYS_LSEEK: usize = 4;

const MAX_IO: usize = 4096;
const MAX_PATH: usize = 4096;

fn neg(e: KernelErrors) -> i64 {
	e.to_neg_errno()
}

pub fn sys_read(fd: i32, user_buf: u64, count: usize) -> i64 {
	if count == 0 {
		return 0;
	}
	if count > MAX_IO {
		return neg(KernelErrors::EINVAL);
	}

	let mut kbuf = alloc::vec![0u8; count];
	let n = match with_current_process(|proc| {
		let fd = fd as usize;
		if fd >= proc.fd_table.len() {
			return Err(KernelErrors::EBADF);
		}
		let file = proc.fd_table[fd].as_mut().ok_or(KernelErrors::EBADF)?;
		vfs::read(file, &mut kbuf)
	}) {
		Some(Ok(n)) => n,
		Some(Err(e)) => return neg(e),
		None => return neg(KernelErrors::EBADF),
	};

	if n > 0 && copy_to_user(user_buf, &kbuf[..n]).is_err() {
		return EFAULT;
	}
	n as i64
}

pub fn sys_write(fd: i32, user_buf: u64, count: usize) -> i64 {
	if count == 0 {
		return 0;
	}
	if count > MAX_IO {
		return neg(KernelErrors::EINVAL);
	}

	let mut buf = alloc::vec![0u8; count];
	if copy_from_user_into(&mut buf, user_buf).is_err() {
		return EFAULT;
	}

	match fd {
		1 => {
			console::write_bytes(&buf);
			count as i64
		}
		2 => {
			for &b in &buf {
				serial::write_byte(b);
			}
			count as i64
		}
		_ => match with_current_process(|proc| {
			let fd = fd as usize;
			if fd >= proc.fd_table.len() {
				return Err(KernelErrors::EBADF);
			}
			let file = proc.fd_table[fd].as_mut().ok_or(KernelErrors::EBADF)?;
			vfs::write(file, &buf)
		}) {
			Some(Ok(n)) => n as i64,
			Some(Err(e)) => neg(e),
			None => neg(KernelErrors::EBADF),
		},
	}
}

pub fn sys_open(path_ptr: u64, flags: u64, _mode: u64) -> i64 {
	let path_bytes = match copy_user_cstr(path_ptr, MAX_PATH) {
		Ok(b) => b,
		Err(e) => return e,
	};
	let path = match str::from_utf8(&path_bytes) {
		Ok(s) => s,
		Err(_) => return neg(KernelErrors::EINVAL),
	};

	let file = match vfs::open(path, flags as u32) {
		Ok(f) => f,
		Err(e) => return neg(e),
	};

	match with_current_process(|proc| {
		let fd = alloc_fd(proc).ok_or(KernelErrors::EINVAL)?;
		proc.fd_table[fd as usize] = Some(file);
		Ok(fd)
	}) {
		Some(Ok(fd)) => fd as i64,
		Some(Err(e)) => neg(e),
		None => neg(KernelErrors::EBADF),
	}
}

pub fn sys_close(fd: i32) -> i64 {
	if fd < 3 {
		return neg(KernelErrors::EBADF);
	}
	match with_current_process(|proc| {
		let fd = fd as usize;
		if fd >= proc.fd_table.len() {
			return Err(KernelErrors::EBADF);
		}
		if proc.fd_table[fd].is_none() {
			return Err(KernelErrors::EBADF);
		}
		proc.fd_table[fd] = None;
		Ok(())
	}) {
		Some(Ok(())) => 0,
		Some(Err(e)) => neg(e),
		None => neg(KernelErrors::EBADF),
	}
}

pub fn sys_lseek(fd: i32, offset: i64, whence: i32) -> i64 {
	if fd < 3 {
		return neg(KernelErrors::EBADF);
	}
	let origin = match whence {
		0 => SeekOrigin::SeekSet,
		1 => SeekOrigin::SeekCur,
		2 => SeekOrigin::SeekEnd,
		_ => return neg(KernelErrors::EINVAL),
	};
	if offset < 0 {
		return neg(KernelErrors::EINVAL);
	}
	let offset_u = offset as u64;

	match with_current_process(|proc| {
		let fd = fd as usize;
		if fd >= proc.fd_table.len() {
			return Err(KernelErrors::EBADF);
		}
		let file = proc.fd_table[fd].as_mut().ok_or(KernelErrors::EBADF)?;
		vfs::seek(file, offset_u, origin)
	}) {
		Some(Ok(pos)) => pos as i64,
		Some(Err(e)) => neg(e),
		None => neg(KernelErrors::EBADF),
	}
}
