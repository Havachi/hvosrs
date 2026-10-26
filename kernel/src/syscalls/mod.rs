mod errno;
mod handlers;
mod user_access;

use crate::{
	arch::x86_64::{asm::current_cr3, syscall::SyscallFrame}, klog, syscalls::{
		handlers::*, user_access::{clear_user_cr3, enter_kernel_page_table, leave_kernel_page_table, set_user_cr3},
	},
};

const NR_SYSCALLS: usize = 512;

type SyscallFn = fn(u64,u64,u64,u64,u64,u64, &mut SyscallFrame) -> i64;

fn sys_nosys(_:u64,_:u64,_:u64,_:u64,_:u64,_:u64, _: &mut SyscallFrame) -> i64 { -38 }

const fn make_table() -> [SyscallFn; NR_SYSCALLS] {
	let mut t = [sys_nosys as SyscallFn; NR_SYSCALLS];
	t[SYS_WRITE] = |_,fd,buf,len,_,_,_| sys_write(fd as i32, buf, len as usize);
	t[SYS_OPEN] = |_,path,flags,mode,_,_,_| sys_open(path, flags, mode);
	t
}

static DISPATCH_TABLE:[SyscallFn; NR_SYSCALLS] = make_table();



/// Called from the syscall asm stub: kernel stack is already set up.
#[unsafe(no_mangle)]
pub extern "C" fn syscall_dispatch_rs(syscall_no: u64, frame: &mut SyscallFrame) -> i64 {
	let user_cr3 = current_cr3();
	set_user_cr3(user_cr3);
	enter_kernel_page_table(user_cr3);

	let ret = syscall_dispatcher(syscall_no, frame);

	leave_kernel_page_table(user_cr3);
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

	if retval == -38 {
		klog!("[SYSCALL] syscall not implemented: {nr}")
	}

	retval
}