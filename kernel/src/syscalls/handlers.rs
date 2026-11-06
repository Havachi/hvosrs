use core::str;


use crate::{
	arch::x86_64::syscall::SyscallFrame, drivers::{console, io::getchar, serial}, errno::KernelErrors::{self, EAGAIN}, klog, scheduler::{self, CURRENT_PID, PROCESS_TABLE, add_proc, alloc_fd, block_current, process::{self, find_zombie, fork_child_from, has_waitable_child, reap}, schedule, with_current_process}, syscalls::{
		errno::EFAULT, user_access::{copy_from_user_into, copy_to_user, copy_user_cstr},
	}, vfs::{self, SeekOrigin, file::VfsFileOpenflags, mkdir},
};

pub const SYS_READ: usize = 0;
pub const SYS_WRITE: usize = 1;
pub const SYS_OPEN: usize = 2;
pub const SYS_CLOSE: usize = 3;
pub const SYS_LSEEK: usize = 4;
pub const SYS_GETPID: usize = 39;
pub const SYS_FORK: usize = 57;
pub const SYS_EXECVE: usize = 59;
pub const SYS_EXIT: usize = 60;
pub const SYS_MKDIR: usize = 83;
pub const SYS_WAITID: usize = 247;

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

	let mut pt = PROCESS_TABLE.lock();
	let pid = CURRENT_PID.load(core::sync::atomic::Ordering::Relaxed);

	if let Some(proc) = pt[pid as usize].as_mut() {
		let fd = fd as usize;
		if fd >= proc.fd_table.len() {
			drop(pt);
			return neg(KernelErrors::EBADF);
		}

		let n = match fd {
			0 => {
				//dont need fd_table for stdin
				drop(pt);
				let mut i = 0;
				while i < count {
					match getchar() {
						Ok(c) => {
							kbuf[i] = c;
							i+=1;
						}
						Err(k) => {
							if k == KernelErrors::EAGAIN {
								block_current(process::BlockedBy::Stdin);
							} else {
								return neg(k);
							}
						}
					}
				}
				i
			},
			_ => {
				let file = match proc.fd_table[fd].as_mut().ok_or(KernelErrors::EBADF) {
					Ok(f) => f,
					Err(er) => {
						drop(pt);
						return neg(er);
					},
				};
				match vfs::read(file, &mut kbuf) {
					Ok(n) => n,
					Err(er) => {
						drop(pt);
						return neg(er);
					},
				}
			}
		};

		if n > 0 && copy_to_user(user_buf, &kbuf[..n]).is_err() {
			return neg(KernelErrors::EFAULT);
		}
		return n as i64
	} else {
		return neg(KernelErrors::EINVAL);
	}
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

pub fn sys_mkdir(path_ptr: u64) -> i64 {
	let path_bytes = match copy_user_cstr(path_ptr, MAX_PATH) {
		Ok(b) => b,
		Err(e) => return e,
	};
	let path = match str::from_utf8(&path_bytes) {
		Ok(s) => s,
		Err(_) => return neg(KernelErrors::EINVAL),
	};

	match mkdir(path) {
		Ok(_) => 0,
		Err(err) => neg(err),
	}
}

pub fn sys_fork(frame: &mut SyscallFrame) -> i64 {
	x86_64::instructions::interrupts::without_interrupts(|| {

		let current_pid = CURRENT_PID.load(core::sync::atomic::Ordering::Relaxed);
		let mut pt = PROCESS_TABLE.lock();
		let current_process = pt[current_pid as usize].as_ref().ok_or_else(|| EAGAIN)?;
		let child = fork_child_from(current_process, frame)?;
		let child_pid = child.pid;
		assert!(pt[child_pid as usize].is_none());
		pt[current_pid as usize].as_mut().expect("should be some").rsp = frame.user_rsp;

		drop(pt);
		add_proc(child_pid, child);
		Ok(child_pid as i64)
	}).unwrap_or_else(|e| neg(e))
}

pub fn sys_execve(path_ptr: u64, _argv: u64, _envp: u64, frame: &mut SyscallFrame) -> i64 {
	x86_64::instructions::interrupts::without_interrupts(|| {

		let path_bytes = copy_user_cstr(path_ptr, MAX_PATH).map_err(|_| KernelErrors::EINVAL)?;
		let path = str::from_utf8(&path_bytes).map_err(|_| KernelErrors::EINVAL)?;
		let mut file = vfs::open(path, VfsFileOpenflags::O_RDONLY.bits())?;
		let elf_buf =  vfs::read_file_all(&mut file)?;
		let current_pid = CURRENT_PID.load(core::sync::atomic::Ordering::Relaxed) as usize;

		let mut pt = PROCESS_TABLE.lock();
		let proc = pt[current_pid].as_mut().ok_or(KernelErrors::EINVAL)?;

		frame.rip_saved = process::exec_replace_image(proc, &elf_buf)?;
		let cr3 = proc.cr3;
		let proc_rsp = proc.rsp;
		drop(pt);

		frame.user_rsp = proc_rsp;
		frame.rax = 0;
		// Return through syscall_dispatch_rs so CR3 for sysret matches proc.cr3 (see leave_cr3).
		Ok(0)
	}).unwrap_or_else(|e| neg(e))
}


pub fn sys_exit(code: u64) -> ! {
	x86_64::instructions::interrupts::without_interrupts(|| {

		let mut pt = PROCESS_TABLE.lock();
		let current_pid = CURRENT_PID.load(core::sync::atomic::Ordering::Acquire);
		let proc = pt[current_pid as usize].as_mut().unwrap();

		proc.state = process::ProcessState::Dead;
		proc.exit_code = ((code & 0xFF) << 8) as u32;

		let ppid = proc.ppid;
		drop(pt);
		scheduler::wake_parent(ppid);
		schedule();
		crate::kpanic!("schedule returned after sys_exit (pid={current_pid})");
	});
	unreachable!()
}

pub fn sys_waitid(id_type: u64, id: u64, _infop: u64, _options: u64, frame: &SyscallFrame) -> i64 {
	let parent_pid = CURRENT_PID.load(core::sync::atomic::Ordering::Acquire);

	loop {
		let cpid = if id_type == 0 { 0 } else if id_type == 1 { id } else { return neg(KernelErrors::EINVAL); };

		if let Some((pid, _status)) = find_zombie(parent_pid, cpid) {
			reap(pid);
			return pid as i64;
		}
		if !has_waitable_child(parent_pid) { return neg(KernelErrors::ECHILD); }
		x86_64::instructions::interrupts::without_interrupts(|| {
			let mut pt = PROCESS_TABLE.lock();
			if let Some(pproc) = pt[parent_pid as usize].as_mut() {
				pproc.state = process::ProcessState::Blocked;
			}
			drop(pt);
		});
		schedule();
	}
}

pub fn sys_getpid() -> i64 {
	let current_pid = CURRENT_PID.load(core::sync::atomic::Ordering::Relaxed);
	return current_pid as i64;
}