use crate::{
	drivers::{console, serial},
	syscalls::{errno::EFAULT, user_access::copy_from_user_into},
};

#[allow(dead_code)]
pub const SYS_READ: usize = 0;
pub const SYS_WRITE: usize = 1;
pub const SYS_OPEN: usize = 2;

const MAX_WRITE: usize = 4096;

pub fn sys_write(fd: i32, user_buf: u64, count: usize) -> i64 {
	if count == 0 {
		return 0;
	}
	if count > MAX_WRITE {
		return -1;
	}

	let mut buf = alloc::vec::Vec::new();
	buf.resize(count, 0);
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
		_ => -1,
	}
}


pub fn sys_open(_path: u64, _flags: u64, _mode: u64 ) -> i64 {

	0
}