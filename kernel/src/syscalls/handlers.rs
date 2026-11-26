use core::str;
use alloc::slice;
use x86_64::{PhysAddr, VirtAddr, registers::control::Cr3, structures::paging::{Page, PageTableFlags, PhysFrame, Size4KiB}};
use crate::{
	arch::x86_64::{paging::{map_phys_page, switch_pml4}, syscall::SyscallFrame}, errno::KernelErrors::{self, EAGAIN}, kstrace, memory::{PAGE_SIZE, USER_HEAP_RANGE, pmm::PhysAllocator}, scheduler::{self, CURRENT_PID, PROCESS_TABLE, add_proc, alloc_fd, block_current, process::{self, find_zombie, fork_child_from, get_cwd, has_waitable_child, reap}, schedule, wake_by_blockedby, with_current_process}, syscalls::{
		copy_user_args, copy_user_env, errno::EFAULT, user_access::{copy_from_user_into, copy_to_user, copy_user_cstr},
	}, time::{BOOT_EPOCH, EPOCH, SysTime}, vfs::{self, SeekOrigin, file::{AT_FDCWD, VfsFileOpenflags}, get_root_dentry, inode::VfsInodeType, lookup, mkdir, path::{is_absolute, path_basename, path_parent, path_walk}},
};

pub const SYS_READ: usize = 0;
pub const SYS_WRITE: usize = 1;
pub const SYS_OPEN: usize = 2;
pub const SYS_CLOSE: usize = 3;
pub const SYS_STAT: usize = 4;
pub const SYS_FSTAT: usize = 5;
pub const SYS_LSTAT: usize = 6;
pub const SYS_LSEEK: usize = 8;
pub const SYS_BRK: usize = 12;
pub const SYS_PWRITE64: usize = 18;
pub const SYS_PIPE: usize = 22;
pub const SYS_DUP: usize = 32;
pub const SYS_DUP2: usize = 33;
pub const SYS_GETPID: usize = 39;
pub const SYS_FORK: usize = 57;
pub const SYS_EXECVE: usize = 59;
pub const SYS_EXIT: usize = 60;
pub const SYS_GETDENTS: usize = 78;
pub const SYS_CHDIR: usize = 80;
pub const SYS_MKDIR: usize = 83;
pub const SYS_MOUNT: usize = 165;
pub const SYS_CLOCK_GETTIME: usize = 228;
pub const SYS_WAITID: usize = 247;
pub const SYS_OPENAT: usize = 257;

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
	let fd_idx = fd as usize;

	loop {
		let mut pt = PROCESS_TABLE.lock();
		let pid = CURRENT_PID.load(core::sync::atomic::Ordering::Relaxed);

		let Some(proc) = pt[pid as usize].as_mut() else {
			return neg(KernelErrors::EINVAL);
		};
		if fd_idx >= proc.fd_table.len() {
			return neg(KernelErrors::EBADF);
		}
		let Some(file) = proc.fd_table[fd_idx].as_mut() else {
			return neg(KernelErrors::EBADF);
		};

		match vfs::read(file, &mut kbuf) {
			Ok(n) => {
				let itype = file.inode.itype();
				drop(pt);
				if n > 0 {
					if itype == VfsInodeType::FIFO {
						wake_by_blockedby(process::BlockedBy::Pipe);
					}
					if copy_to_user(user_buf, &kbuf[..n]).is_err() {
						return neg(KernelErrors::EFAULT);
					}
				}
			
				return n as i64;
			}
			Err(EAGAIN) => {
				let reason = match file.inode.itype() {
					VfsInodeType::FIFO => process::BlockedBy::Pipe,
					_ => process::BlockedBy::Stdin
				};
				drop(pt);
				block_current(reason);
			}
			Err(er) => return neg(er),
		}
	}
}

pub fn sys_write(fd: i32, user_buf: u64, count: usize) -> i64 {
	if count == 0 {
		return 0;
	}
	if count > MAX_IO {
		return neg(KernelErrors::EINVAL);
	}

	let mut kbuf = alloc::vec![0u8; count];
	if copy_from_user_into(&mut kbuf, user_buf).is_err() {
		return EFAULT;
	}
	let kbuf2 = kbuf;
	let fd_idx = fd as usize;
	let mut written = 0;
	loop {
		let mut pt = PROCESS_TABLE.lock();
		let pid = CURRENT_PID.load(core::sync::atomic::Ordering::Relaxed);
		let Some(proc) = pt[pid as usize].as_mut() else {
			return neg(KernelErrors::EINVAL);
		};
		if fd_idx >= proc.fd_table.len() {
			return neg(KernelErrors::EBADF);
		}
		let Some(file) = proc.fd_table[fd_idx].as_mut() else {
			return neg(KernelErrors::EBADF);
		};

		let itype = file.inode.itype();
		match vfs::write(file, &kbuf2[written..]) {
			Ok(n) => {
				drop(pt);
				if n == 0 {
					return written as i64;
				}
				written += n;
				if itype == VfsInodeType::FIFO {
					wake_by_blockedby(process::BlockedBy::Pipe);
				}
				if written < count {
					continue;
				}
				return written as i64;
			},
			Err(KernelErrors::EPIPE) => {
				if written > 0 {
					return written as i64;
				} else {
					return neg(KernelErrors::EPIPE);
				}
			},
			Err(KernelErrors::EAGAIN) => {
				if itype == VfsInodeType::FIFO {
					drop(pt);
					block_current(process::BlockedBy::Pipe);
				} else {
					return neg(KernelErrors::EAGAIN);
				}
			},
			Err(err) => return neg(err),
		}
	}
}

pub fn sys_open(path_ptr: u64, flags: u64, mode: u64) -> i64 {
	let path_bytes = match copy_user_cstr(path_ptr, MAX_PATH) {
		Ok(b) => b,
		Err(e) => {
			kstrace!("sys_open(<invalid>, {flags}, {mode}) = {}\n", e);
			return e
		},
	};
	let path = match str::from_utf8(&path_bytes) {
		Ok(s) => s,
		Err(_) => {
			kstrace!("sys_open(<invalid>, {flags}, {mode}) = {}\n", neg(KernelErrors::EINVAL));
			return neg(KernelErrors::EINVAL)
		},
	};
	let file = match vfs::open(path, flags as u32, mode as u16) {
		Ok(f) => f,
		Err(e) => {
			kstrace!("sys_open({path}, {flags}, {mode}) = {}\n", neg(e));
			return neg(e)
		},
	};

	let ret = 	match with_current_process(|proc| {
		let fd = alloc_fd(proc).ok_or(KernelErrors::EINVAL)?;
		proc.fd_table[fd as usize] = Some(file);
		Ok(fd)
	}) {
		Some(Ok(fd)) => fd as i64,
		Some(Err(e)) => neg(e),
		None => neg(KernelErrors::EBADF),
	};

	kstrace!("sys_open({path}, {flags}, {mode}) = {}\n", ret);

	ret
}

pub fn sys_close(fd: i32) -> i64 {
	match with_current_process(|proc| {
		let fd = fd as usize;
		if fd >= proc.fd_table.len() {
			return Err(KernelErrors::EBADF);
		}
		if proc.fd_table[fd].is_none() {
			return Err(KernelErrors::EBADF);
		}
		let f = proc.fd_table[fd].take().ok_or(KernelErrors::EBADF)?;
		Ok(f)
	}) {
		Some(Ok(f)) =>  {
			let is_fifo = f.inode.itype() == VfsInodeType::FIFO;
			drop(f);
			if is_fifo {
				wake_by_blockedby(process::BlockedBy::Pipe);
			}
			0
		},
		Some(Err(e)) => neg(e),
		None => neg(KernelErrors::EBADF),
	}
}

pub fn sys_stat(path_ptr: u64, stat_buf: u64) -> i64 {
	let path_bytes = match copy_user_cstr(path_ptr, MAX_PATH) {
		Ok(b) => b,
		Err(e) => return e,
	};
	let path = match str::from_utf8(&path_bytes) {
		Ok(s) => s,
		Err(_) => {
			return neg(KernelErrors::EINVAL)
		},
	};
	//kstrace!("stat: {}\n", path);
	let root = match get_root_dentry() {
		Ok(d) => d,
		Err(e) => return neg(e),
	};
	let cwd = get_cwd().unwrap_or(root.clone());
	let dentry = match path_walk(path, root, cwd) {
		Ok(d) => d,
		Err(e) => return neg(e),
	};

	let st = match dentry.inode.stat() {
		Ok(s) => s,
		Err(e) => return neg(e),
	};

	match copy_to_user(stat_buf, st.as_bytes()).map_err(|_| KernelErrors::EFAULT) {
		Ok(_) => {},
		Err(e) => return neg(e),
	};

	0
}

pub fn sys_fstat(fd: u64, stat_buf: u64) -> i64 {
	if stat_buf == 0 {
		kstrace!("fstat({fd}, {:#X}) = {}\n", stat_buf, neg(KernelErrors::EFAULT));
		return neg(KernelErrors::EFAULT);
	}

	let file = match with_current_process(|p| {
			p.fd_table[fd as usize].clone()
		}).unwrap_or_else(|| None) {
		Some(f) => f,
		None => {
			kstrace!("fstat({fd}, {:#X}) = {}\n", stat_buf, neg(KernelErrors::EBADF));
			return neg(KernelErrors::EBADF)
		},
	};

	let st = match file.inode.stat() {
		Ok(s) => s,
		Err(e) => {
			kstrace!("fstat({fd}, {:#X}) = {}\n", stat_buf, neg(e));
			return neg(e)
		},
	};
	match copy_to_user(stat_buf, st.as_bytes()).map_err(|_| KernelErrors::EFAULT) {
		Ok(_) => {},
		Err(e) => {
			kstrace!("fstat({fd}, {:#X}) = {}\n", stat_buf, neg(e));
			return neg(e)
		},
	};
	kstrace!("fstat({fd}, {:#X}) = {}\n", stat_buf, 0);
	0

}

pub fn sys_lstat(_path_ptr: u64, _stat_buf: u64) -> i64 {
	-127
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
		Err(_) => {
			kstrace!("mkdir(<invalid>) = {}\n",neg(KernelErrors::EINVAL));
			return neg(KernelErrors::EINVAL)
		},
	};
	let path = match str::from_utf8(&path_bytes) {
		Ok(s) => s,
		Err(_) => {
			kstrace!("mkdir(<invalid>) = {}\n",neg(KernelErrors::EINVAL));
			return neg(KernelErrors::EINVAL)
		},
	};

	let result = (|| -> Result<(), KernelErrors> {
		let parent_path = path_parent(path)?;
		let bname = path_basename(path)?;
		let parent_dentry = lookup(parent_path, get_cwd())?;

		mkdir(&parent_dentry, bname, 0o777)?;
		Ok(())
	})();

	match result {
		Ok(()) =>  {
			kstrace!("mkdir({}) = 0\n", path);
			return 0;
		},
		Err(err) => {
			kstrace!("mkdir({}) = {}\n", path, neg(err));
			return neg(err)
		},
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

pub fn sys_execve(path_ptr: u64, argv_ptr: u64, envp_ptr: u64, frame: &mut SyscallFrame) -> i64 {
	x86_64::instructions::interrupts::without_interrupts(|| {
		let path_bytes = copy_user_cstr(path_ptr, MAX_PATH).map_err(|_| KernelErrors::EINVAL)?;
		let path = str::from_utf8(&path_bytes).map_err(|_| KernelErrors::EINVAL)?;
		let args = copy_user_args(argv_ptr, &path_bytes)?;
		let env = copy_user_env(envp_ptr)?;
		//TODO send something else that 777 for the mode
		let mut file = vfs::open(path, VfsFileOpenflags::O_RDONLY.bits(), 0o777)?;
		let elf_buf =  vfs::read_file_all(&mut file)?;
		let current_pid = CURRENT_PID.load(core::sync::atomic::Ordering::Relaxed) as usize;
		let (entry, user_rsp) = {
			let mut pt = PROCESS_TABLE.lock();
			let proc = pt[current_pid].as_mut().ok_or(KernelErrors::EINVAL)?;
			process::exec_replace_image(proc, &elf_buf, &args, &env)?
		};

		frame.rip_saved = entry;
		frame.user_rsp = user_rsp;
		frame.rax = 0;

		Ok(0)
	}).unwrap_or_else(|e| neg(e))
}

pub fn sys_exit(code: u64) -> ! {
	x86_64::instructions::interrupts::without_interrupts(|| {
		let mut pt = PROCESS_TABLE.lock();
		let current_pid = CURRENT_PID.load(core::sync::atomic::Ordering::Acquire);
		kstrace!("[{}] exit {}\n",current_pid, code);
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

pub fn sys_waitid(id_type: u64, id: u64, _infop: u64, _options: u64, _frame: &SyscallFrame) -> i64 {
	
	let parent_pid = CURRENT_PID.load(core::sync::atomic::Ordering::Acquire);
	kstrace!("waitid({},{},{},{},{})\n",parent_pid, id_type, id, _infop, _options);

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

pub fn sys_brk(addr: u64) -> i64 {
	if addr == 0 {
		return with_current_process(|proc| {
			kstrace!("brk({:#x}) = {:#x}\n", addr, proc.brk as i64);
			proc.brk
		}).unwrap() as i64;
	} else if USER_HEAP_RANGE.contains(addr) {
		with_current_process(|p| {
			let current_brk = p.brk;
			if addr <= current_brk {
				return current_brk as i64;
			}
			let diff = addr - current_brk;
			let nb_pages = (diff + (PAGE_SIZE as u64) - 1) / PAGE_SIZE as u64;
			let new_brk = current_brk + nb_pages * PAGE_SIZE as u64;

			if !USER_HEAP_RANGE.contains(new_brk - 1) {
				return neg(KernelErrors::ENOMEM);
			}

			let frames = match PhysAllocator::allocate_n_frame(nb_pages as usize) {
				Some(ok) => ok,
				None => return neg(KernelErrors::ENOMEM),
			};

			let (current_cr3, _) = Cr3::read();
			if current_cr3.start_address().as_u64() != p.cr3 {
				unsafe { switch_pml4(PhysFrame::containing_address(PhysAddr::new(p.cr3))) };
			}

			let mut failed = false;
			for i in 0..nb_pages {
				let phys = frames.as_u64() + (i * PAGE_SIZE as u64);
				let page = Page::<Size4KiB>::containing_address(
					VirtAddr::new(current_brk + (i * PAGE_SIZE as u64))
				);

				unsafe {
					match map_phys_page(
						page,
						PhysFrame::containing_address(PhysAddr::new(phys)),
						PageTableFlags::PRESENT
							| PageTableFlags::WRITABLE
							| PageTableFlags::USER_ACCESSIBLE
							| PageTableFlags::NO_EXECUTE,
						&mut PhysAllocator,
					) {
						Ok(_) => {
							page.start_address().as_mut_ptr::<u8>().write_bytes(0, PAGE_SIZE);
						}
						Err(_) => {
							failed = true;
							break;
						}
					}
				}
			}
			unsafe { switch_pml4(current_cr3) };

			if failed {
				PhysAllocator::free_n_frame(frames, nb_pages as usize);
				return neg(KernelErrors::ENOMEM);
			}

			p.brk = new_brk;
			kstrace!("brk({:#x}) = {:#x}\n", addr, p.brk as i64);
			p.brk as i64
		}).unwrap_or_else(|| neg(KernelErrors::ENOMEM))
	} else {
		neg(KernelErrors::ENOMEM)
	}
}

pub fn sys_pwrite64(fd: i32, user_buf: u64, count: usize, pos: u64) -> i64 {
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
	sys_lseek(fd, pos as i64, 0);
	match fd {
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

pub fn sys_pipe(fildes_ptr: u64) -> i64 {
	let (r, w) = crate::fs::pipe::pair() ;
	
	let res = match with_current_process(|proc| {

		let rfd = alloc_fd(proc).ok_or(KernelErrors::EMFILE)?;
		proc.fd_table[rfd as usize] = Some(
			vfs::file::VfsFile::new(r, 0, VfsFileOpenflags::O_RDONLY)
		);

		let wfd = match alloc_fd(proc) {
			Some(fd) => fd,
			None => {
				proc.fd_table[rfd as usize] = None;
				return Err(KernelErrors::EMFILE);
			}
		};
		proc.fd_table[wfd as usize] = Some(
			vfs::file::VfsFile::new(w, 0, VfsFileOpenflags::O_WRONLY)
		);
		
		Ok([rfd, wfd])
	})
	.unwrap_or(Err(KernelErrors::EBADF)) {
		Ok(fds) => fds,
		Err(er) => return neg(er),
	};

	let mut buf = [0u8;8];
	buf[0..4].copy_from_slice(&res[0].to_le_bytes());
	buf[4..8].copy_from_slice(&res[1].to_le_bytes());

	match copy_to_user(fildes_ptr, &buf) {
		Ok(()) => {},
		Err(er) => {

			with_current_process(|proc| {
				proc.fd_table[res[0] as usize] = None;
				proc.fd_table[res[1] as usize] = None;
			});
			return er;
		}
	};

	0
}	

pub fn sys_dup(old_fd: u64) -> i64 {
	with_current_process(|proc| {
		let idx = usize::try_from(old_fd).map_err(|_| KernelErrors::EBADF)?;
		if proc.fd_table.get(idx).and_then(|s| s.as_ref()).is_none() {
			return Err(KernelErrors::EBADF);
		}
		let new_fd = alloc_fd(proc).ok_or(KernelErrors::EMFILE)?;
		let old_idx = usize::try_from(old_fd).map_err(|_| KernelErrors::EBADF)?;
		let new_idx = usize::try_from(new_fd).map_err(|_| KernelErrors::EBADF)?;
		if old_fd >= proc.fd_table.len() as u64 || new_idx >= proc.fd_table.len() {
			return Err(KernelErrors::EBADF);
		}
		let mut file = proc.fd_table[old_idx].clone().ok_or(KernelErrors::EBADF)?;
		file.flags.remove(VfsFileOpenflags::O_CLOEXEC);
		proc.fd_table[new_idx] = Some(file);
		Ok(new_fd as i64)
	})
	.unwrap_or(Err(KernelErrors::EBADF))
	.unwrap_or_else(neg)
}

pub fn sys_dup2(old_fd: u64, new_fd: u64) -> i64 {
	with_current_process(|proc| {
		if old_fd == new_fd {
			let idx = usize::try_from(old_fd).map_err(|_| KernelErrors::EBADF)?;
			if proc.fd_table.get(idx).and_then(|s| s.as_ref()).is_none() {
				return Err(KernelErrors::EBADF);
			}
			return Ok(new_fd as i64);
		}

		let old_idx = usize::try_from(old_fd).map_err(|_| KernelErrors::EBADF)?;
		let new_idx = usize::try_from(new_fd).map_err(|_| KernelErrors::EBADF)?;
		if old_fd >= proc.fd_table.len() as u64 || new_idx >= proc.fd_table.len() {
			return Err(KernelErrors::EBADF);
		}
		let mut file = proc.fd_table[old_idx].clone().ok_or(KernelErrors::EBADF)?;
		file.flags.remove(VfsFileOpenflags::O_CLOEXEC);
		proc.fd_table[new_idx] = Some(file);
		Ok(new_fd as i64)
	})
	.unwrap_or(Err(KernelErrors::EBADF))
	.unwrap_or_else(neg)
}

pub fn sys_chdir(path_ptr: u64) -> i64 {

	let path_bytes = match copy_user_cstr(path_ptr, MAX_PATH) {
		Ok(b) => b,
		Err(e) => return e,
	};
	let path = match str::from_utf8(&path_bytes) {
		Ok(s) => s,
		Err(_) => return neg(KernelErrors::EINVAL),
	};
	kstrace!("chdir({})\n", path);

	let cwd = process::get_cwd();

	let dir = match vfs::lookup(path, cwd) {
		Ok(d) => d,
		Err(_) => return neg(KernelErrors::ENOENT),
	};
	if dir.inode.is_dir() {
		process::set_cwd(dir);
		return 0;
	} else {
		return neg(KernelErrors::ENOTDIR);
	}
}

pub fn sys_getdents(fd: u64, dirp: u64, count: u64) -> i64 {
	let res = sys_getdents_inner(fd, dirp, count).unwrap_or_else(neg);
	kstrace!("getdents({}, {:#x}, {}) = {}\n", fd, dirp, count, res);
	res
}

fn sys_getdents_inner(fd: u64, dirp: u64, count: u64) -> Result<i64, KernelErrors> {
	if count == 0 {
		return Err(KernelErrors::EINVAL);
	}

	with_current_process(|proc| {
		let file = proc
			.fd_table
			.get_mut(fd as usize)
			.and_then(|slot| slot.as_mut())
			.ok_or(KernelErrors::EBADF)?;
		let mut offset = *file.offset.lock();
		let list = file.inode.getdents(&mut offset, count as usize)?;
		if !list.is_empty() {
			copy_to_user(dirp, &list).map_err(|_| KernelErrors::EFAULT)?;
		}
		*file.offset.lock() = offset;
		Ok(list.len() as i64)
	}).unwrap_or(Err(KernelErrors::ESRCH))

}

pub fn sys_openat(dfd: u64, path_ptr: u64, flags: u64, mode: u64) -> i64 {
	let path_bytes = match copy_user_cstr(path_ptr, MAX_PATH) {
		Ok(b) => b,
		Err(e) => return e,
	};
	let path = match str::from_utf8(&path_bytes) {
		Ok(s) => s,
		Err(_) => {
			return neg(KernelErrors::EINVAL)
		},
	};

	if !is_absolute(path) && dfd != AT_FDCWD as u64 {
		return neg(KernelErrors::EBADF);
	} else{
		let file = match vfs::open(path, flags as u32, mode as u16) {
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
}

pub fn sys_mount(dev_name_ptr: u64, dir_name_ptr: u64, fs_type_ptr: u64, _flags: u64, _data_ptr: u64) -> i64 {
	let devname_bytes = match copy_user_cstr(dev_name_ptr, MAX_PATH) {
		Ok(b) => b,
		Err(e) => return e,
	};
	let devname = match str::from_utf8(&devname_bytes) {
		Ok(s) => s,
		Err(_) => {
			return neg(KernelErrors::EINVAL)
		},
	};

	let dirname_bytes = match copy_user_cstr(dir_name_ptr, MAX_PATH) {
		Ok(b) => b,
		Err(e) => return e,
	};
	let dirname = match str::from_utf8(&dirname_bytes) {
		Ok(s) => s,
		Err(_) => {
			return neg(KernelErrors::EINVAL)
		},
	};
	let mut fs_name = [0u8;16];
	if fs_type_ptr != 0 {
		let Ok(fs_str) = copy_user_cstr(fs_type_ptr, 16) else { return neg(KernelErrors::EINVAL);};
		if fs_str.len() > 16 {
			return neg(KernelErrors::EINVAL);
		}
		fs_name[..fs_str.len()].copy_from_slice(&fs_str);
	} else {
		fs_name[..4].copy_from_slice(b"ext2");
	}
	match vfs::mount(fs_name, &dirname, Some(devname)) {
		Ok(_) => {},
		Err(e) => {
			kstrace!("mount({devname}, {dirname}) => {}\n", neg(e));
			return neg(e)
		},
	}
	kstrace!("mount({devname}, {dirname}) => 0\n");
	0
}

pub fn sys_clock_gettime(which: u64, dt: u64) -> i64 {
	if dt == 0 {
		return neg(KernelErrors::EINVAL);
	}

	let ms = match which {
		0 => EPOCH.load(core::sync::atomic::Ordering::Acquire),
		1 => EPOCH.load(core::sync::atomic::Ordering::Acquire)
			.saturating_sub(BOOT_EPOCH.load(core::sync::atomic::Ordering::Acquire)),
		_ => return neg(KernelErrors::EINVAL),
	};
	let systime = SysTime { millis: ms };

	let bytes = unsafe {
		slice::from_raw_parts((&systime as *const SysTime).cast::<u8>(), size_of::<SysTime>())
	};

	match copy_to_user(dt, bytes) {
		Ok(()) => 0,
		Err(err) => err
	}
}