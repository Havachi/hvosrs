pub mod handlers;
pub mod user_access;

use core::sync::atomic::Ordering;

use alloc::vec::{Vec};
use x86_64::registers::rflags::RFlags;

use crate::{
	arch::x86_64::{asm::current_cr3, syscall::SyscallFrame}, errno::KernelErrors, klog, memory::USER_RANGE, scheduler::{CURRENT_PID, with_current_process}, syscalls::{
		handlers::*, user_access::{clear_user_cr3, copy_from_user_into, copy_user_cstr, enter_kernel_page_table, leave_kernel_page_table, set_user_cr3},
	},
};

const NR_SYSCALLS: usize = 512;
const MAX_ARGS: usize = 64;
const MAX_ARG_LEN: usize = 256;
type SyscallFn = fn(u64,u64,u64,u64,u64,u64, &mut SyscallFrame) -> i64;

fn sys_nosys(_:u64,_:u64,_:u64,_:u64,_:u64,_:u64, _: &mut SyscallFrame) -> i64 { -38 }

const fn make_table() -> [SyscallFn; NR_SYSCALLS] {
	let mut t = [sys_nosys as SyscallFn; NR_SYSCALLS];
	t[SYS_READ] = |_, fd, buf, len, _, _, _| sys_read(fd as i32, buf, len as usize);
	t[SYS_WRITE] = |_, fd, buf, len, _, _, _| sys_write(fd as i32, buf, len as usize);
	t[SYS_OPEN] = |_, path, flags, mode, _, _, _| sys_open(path, flags, mode);
	t[SYS_CLOSE] = |_, fd, _, _, _, _, _| sys_close(fd as i32);
	t[SYS_STAT] = |_, path, buf, _, _, _, _| sys_stat(path, buf);
	t[SYS_FSTAT] = |_, fd, buf, _, _, _, _| sys_fstat(fd, buf);
	t[SYS_LSTAT] = |_, path, buf, _, _, _, _| sys_lstat(path, buf);
	t[SYS_LSEEK] = |_, fd, offset, whence, _, _, _| sys_lseek(fd as i32, offset as i64, whence as i32);
	t[SYS_BRK] = |_,addr,_,_,_,_,_| sys_brk(addr);
	t[SYS_PWRITE64] = |_, fd, buf, len, pos, _, _| sys_pwrite64(fd as i32, buf, len as usize, pos);
	t[SYS_PIPE] = |_, fildes,_,_,_,_,_| sys_pipe(fildes);
	t[SYS_DUP] = |_, old_fd, _, _,_,_,_| sys_dup(old_fd);
	t[SYS_DUP2] = |_, old_fd, new_fd, _,_,_,_| sys_dup2(old_fd, new_fd);
	t[SYS_GETPID] = |_,_,_,_,_,_,_|  sys_getpid();
	t[SYS_FORK] = |_,_,_,_,_,_,f| sys_fork(f);
	t[SYS_EXECVE] = |_,path,argv,envp,_,_,f| sys_execve(path, argv, envp, f);
	t[SYS_EXIT] = |_,code,_,_,_,_,_| sys_exit(code);
	t[SYS_GETDENTS] = |_,fd,dirp,count,_,_,_| sys_getdents(fd, dirp, count);
	t[SYS_CHDIR] = |_,path,_,_,_,_,_| sys_chdir(path);
	t[SYS_MKDIR] = |_,path,_,_,_,_,_| sys_mkdir(path);
	t[SYS_MOUNT] = |_, dev_name, dir_name, mtype, flags, data,_| sys_mount(dev_name, dir_name, mtype, flags, data);
	t[SYS_CLOCK_GETTIME] = |_,which,dt,_,_,_,_| sys_clock_gettime(which, dt);
	t[SYS_WAITID] = |_,idtype,id,infop,option,_,f| sys_waitid(idtype, id, infop, option, f);
	t[SYS_OPENAT] = |_,dfd, path_ptr, flags, mode, _,_| sys_openat(dfd, path_ptr, flags, mode);
	t
}

static DISPATCH_TABLE:[SyscallFn; NR_SYSCALLS] = make_table();

/// Called from the syscall asm stub: kernel stack is already set up.
#[inline(never)]
#[unsafe(no_mangle)]
pub extern "C" fn syscall_dispatch_rs(syscall_no: u64, frame: &mut SyscallFrame) -> i64 {
	let user_cr3 = current_cr3();
	set_user_cr3(user_cr3);
	enter_kernel_page_table(user_cr3);

	let ret = syscall_dispatcher(syscall_no, frame);

	// execve replaces proc.cr3; returning to user must load the *current* page table, not the
	// snapshot from syscall entry (would map wrong AS → bogus RCX/RIP on sysret, e.g. idle_process).
	let leave_cr3 = with_current_process(|p| p.cr3).unwrap_or(user_cr3);

	if !USER_RANGE.contains(frame.rip_saved) {
		let pid = CURRENT_PID.load(Ordering::Acquire);
		klog!(
			"[syscall] rip_saved not in USER_RANGE (pid={} nr={} rip={:#x})",
			pid,
			syscall_no,
			frame.rip_saved
		);
	}

	if RFlags::from_bits_truncate(frame.rflags_saved).contains(RFlags::TRAP_FLAG) {
		let pid = CURRENT_PID.load(Ordering::Acquire);
		klog!(
			"[syscall] saved rflags has TF set (pid={} nr={} rip={:#x})",
			pid,
			syscall_no,
			frame.rip_saved
		);
	}
	leave_kernel_page_table(leave_cr3);
	clear_user_cr3();
	ret
}

pub fn syscall_dispatcher(nr: u64, frame: &mut SyscallFrame) -> i64 {
	let idx = nr as usize;

	if idx >= NR_SYSCALLS {
		klog!("syscall {} out of range", nr);
		return -38;
	}

	let retval = DISPATCH_TABLE[idx] (
		nr,
		frame.rdi,
		frame.rsi,
		frame.rdx,
		frame.r10,
		frame.r8,
		frame
	);

	if nr == SYS_EXIT as u64 {
		crate::kpanic!("sys_exit returned to syscall dispatcher");
	}

	if retval == -38 {
		let pid = CURRENT_PID.load(Ordering::Acquire);
		klog!(
			"[SYSCALL] syscall not implemented: {nr} pid={pid} user_rip={:#x}",
			frame.rip_saved
		);
	}

	retval
}

pub fn copy_user_args(argv_ptr: u64, fallback_arg0: &[u8]) -> Result<Vec<Vec<u8>>, KernelErrors> {
	if argv_ptr == 0 {
		return Ok(alloc::vec![fallback_arg0.to_vec()]);
	}

	let mut args: Vec<_> = Vec::new();
	for i in 0..MAX_ARGS {
		let slot_addr = argv_ptr
			.checked_add((i * core::mem::size_of::<u64>()) as u64)
			.ok_or(KernelErrors::EFAULT)?;
		if !USER_RANGE.contains(slot_addr) {
			return Err(KernelErrors::EFAULT);
		}

		let mut ptr_buf = [0u8; 8];
		copy_from_user_into(&mut ptr_buf, slot_addr).map_err(|_| KernelErrors::EBADF)?;
		let str_ptr = u64::from_le_bytes(ptr_buf);
		if str_ptr == 0 {
			break;
		}
		if !USER_RANGE.contains(str_ptr) {
			
			return Err(KernelErrors::EFAULT);
		}
		let mut arg_bytes = copy_user_cstr(str_ptr, MAX_ARG_LEN).map_err(|_| KernelErrors::EFAULT)?;
		arg_bytes.push(0);
		args.push(arg_bytes);
	}
	if args.is_empty() {
		args.push(fallback_arg0.to_vec());
	}
	Ok(args)
}

/// `envp_ptr == 0` is an empty environment. A non-null pointer is a NULL-terminated
/// array of C strings, same layout as `argv`.
pub fn copy_user_env(envp_ptr: u64) -> Result<Vec<Vec<u8>>, KernelErrors> {
	if envp_ptr == 0 {
		return Ok(Vec::new());
	}

	let mut env = Vec::new();
	for i in 0..MAX_ARGS {
		let slot_addr = envp_ptr
			.checked_add((i * core::mem::size_of::<u64>()) as u64)
			.ok_or(KernelErrors::EFAULT)?;
		if !USER_RANGE.contains(slot_addr) {
			return Err(KernelErrors::EFAULT);
		}

		let mut ptr_buf = [0u8; 8];
		copy_from_user_into(&mut ptr_buf, slot_addr).map_err(|_| KernelErrors::EFAULT)?;
		let str_ptr = u64::from_le_bytes(ptr_buf);
		if str_ptr == 0 {
			break;
		}
		if !USER_RANGE.contains(str_ptr) {
			return Err(KernelErrors::EFAULT);
		}
		let mut bytes = copy_user_cstr(str_ptr, MAX_ARG_LEN).map_err(|_| KernelErrors::EFAULT)?;
		bytes.push(0);
		env.push(bytes);
	}
	Ok(env)
}

fn push_user_cstr(rsp: &mut u64, bytes: &[u8]) -> Result<u64, KernelErrors> {
	let len = bytes.len();
	let total_bytes = len.checked_add(1).ok_or(KernelErrors::EINVAL)? as u64;
	*rsp = rsp.checked_sub(total_bytes).ok_or(KernelErrors::EINVAL)?;
	unsafe {
		core::ptr::copy_nonoverlapping(bytes.as_ptr(), *rsp as *mut u8, len);
		*((*rsp + len as u64) as *mut u8) = 0;
	}
	Ok(*rsp)
}

pub fn push_args_to_user_stack(mut rsp: u64, argv: &[Vec<u8>], envp: &[Vec<u8>]) -> Result<u64, KernelErrors> {
	let mut arg_ptrs: Vec<u64> = Vec::with_capacity(argv.len());
	for arg in argv {
		arg_ptrs.push(push_user_cstr(&mut rsp, arg)?);
	}
	let mut env_ptrs: Vec<u64> = Vec::with_capacity(envp.len());
	for var in envp {
		env_ptrs.push(push_user_cstr(&mut rsp, var)?);
	}

	// argc, argv pointers, argv NULL, env pointers, env NULL.
	let total_words = arg_ptrs.len() + env_ptrs.len() + 3;
	rsp &= !0xF;
	if total_words % 2 == 1 {
		rsp = rsp.checked_sub(8).ok_or(KernelErrors::EINVAL)?;
	}

	// High addresses first: env NULL, then env pointers, then argv NULL, argv, argc.
	rsp = rsp.checked_sub(8).ok_or(KernelErrors::EINVAL)?;
	unsafe { *(rsp as *mut u64) = 0; }
	for &ptr in env_ptrs.iter().rev() {
		rsp = rsp.checked_sub(8).ok_or(KernelErrors::EINVAL)?;
		unsafe { *(rsp as *mut u64) = ptr; }
	}

	rsp = rsp.checked_sub(8).ok_or(KernelErrors::EINVAL)?;
	unsafe { *(rsp as *mut u64) = 0; }
	for &ptr in arg_ptrs.iter().rev() {
		rsp = rsp.checked_sub(8).ok_or(KernelErrors::EINVAL)?;
		unsafe { *(rsp as *mut u64) = ptr; }
	}

	rsp = rsp.checked_sub(8).ok_or(KernelErrors::EINVAL)?;
	unsafe { *(rsp as *mut u64) = arg_ptrs.len() as u64; }

	Ok(rsp)
}