mod errno;
mod handlers;
mod user_access;

use core::sync::atomic::Ordering;

use x86_64::registers::rflags::RFlags;

use crate::{
	arch::x86_64::{asm::current_cr3, syscall::SyscallFrame},
	klog,
	memory::USER_RANGE,
	scheduler::{CURRENT_PID, with_current_process},
	syscalls::{
		handlers::*,
		user_access::{clear_user_cr3, enter_kernel_page_table, leave_kernel_page_table, set_user_cr3},
	},
};

const NR_SYSCALLS: usize = 512;

type SyscallFn = fn(u64,u64,u64,u64,u64,u64, &mut SyscallFrame) -> i64;

fn sys_nosys(_:u64,_:u64,_:u64,_:u64,_:u64,_:u64, _: &mut SyscallFrame) -> i64 { -38 }

const fn make_table() -> [SyscallFn; NR_SYSCALLS] {
	let mut t = [sys_nosys as SyscallFn; NR_SYSCALLS];
	t[SYS_READ] = |_, fd, buf, len, _, _, _| sys_read(fd as i32, buf, len as usize);
	t[SYS_WRITE] = |_, fd, buf, len, _, _, _| sys_write(fd as i32, buf, len as usize);
	t[SYS_OPEN] = |_, path, flags, mode, _, _, _| sys_open(path, flags, mode);
	t[SYS_CLOSE] = |_, fd, _, _, _, _, _| sys_close(fd as i32);
	t[SYS_LSEEK] = |_, fd, offset, whence, _, _, _| sys_lseek(fd as i32, offset as i64, whence as i32);
	t[SYS_GETPID] = |_,_,_,_,_,_,_|  sys_getpid();
	t[SYS_FORK] = |_,_,_,_,_,_,f| sys_fork(f);
	t[SYS_EXECVE] = |_,path,argv,envp,_,_,f| sys_execve(path, argv, envp, f);
	t[SYS_EXIT] = |_,code,_,_,_,_,_| sys_exit(code);
	t[SYS_MKDIR] = |_,path,_,_,_,_,_| sys_mkdir(path);
	t[SYS_WAITID] = |_,idtype,id,infop,option,_,f| sys_waitid(idtype, id, infop, option, f);
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
