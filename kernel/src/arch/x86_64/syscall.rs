use core::{arch::global_asm, ptr::addr_of};

use x86_64::{VirtAddr, registers::{model_specific::{LStar, SFMask, Star}, rflags::RFlags}};

use crate::{arch::x86_64::gdt::{KERNEL_CODE, KERNEL_DATA, USER_CODE, USER_DATA}, klog, kpanic};

pub const SYSCALL_STACK_SIZE: usize = 8192;
/// Trap frame and `sysret` epilogue only; handlers run on the process kernel stack.
#[unsafe(no_mangle)]
static mut SYSCALL_STACK: [u8; SYSCALL_STACK_SIZE] = [0; SYSCALL_STACK_SIZE];

/// Fallback when no process is scheduled (`CURRENT_KSTACK_TOP == 0`).
pub fn syscall_stack_top() -> u64 {
	unsafe { addr_of!(SYSCALL_STACK).cast::<u8>() as u64 + SYSCALL_STACK_SIZE as u64 }
}

#[repr(C)]
#[derive(Debug)]
pub struct SyscallFrame {
    pub r15: u64, pub r14: u64, pub r13: u64, pub r12: u64,
    pub r11: u64, pub r10: u64, pub r9:  u64, pub r8:  u64,
    pub rbp: u64, pub rdi: u64, pub rsi: u64, pub rdx: u64,
    pub rcx: u64, pub rbx: u64, pub rax: u64,
    pub rflags_saved: u64,
    pub rip_saved:    u64,
    pub user_rsp: u64,
}


pub fn init(){
	Star::write(USER_CODE, USER_DATA, KERNEL_CODE, KERNEL_DATA).map_err(|e| {
		kpanic!("Failed init STAR register:\n{e}")
	});
	LStar::write(VirtAddr::new(syscall_entry as *const () as u64));
	SFMask::write(RFlags::INTERRUPT_FLAG | RFlags::DIRECTION_FLAG);
	klog!("Syscall enabled");
}

unsafe extern "C" {
	pub unsafe fn syscall_entry();
}

global_asm!(
	r#"
.global SYSCALL_STACK

.global syscall_entry
syscall_entry:
	mov r8, rsp
	lea r10, [rip + SYSCALL_STACK]
	add r10, {stack_size}
	mov rsp, r10

	// Push order (first = highest address): user_rsp, rip, rflags, nr, rbx, rcx, rdx, rsi, rdi, rbp, pad, r9, r10, r11, r12..r15
	push r8
	push rcx
	push r11
	push rax
	push rbx
	push rcx
	push rdx
	push rsi
	push rdi
	push rbp
	push QWORD PTR 0
	push r9
	push r10
	push r11
	push r12
	push r13
	push r14
	push r15

	mov rbx, rax
	mov r12, rsp
	call hvos_syscall_kstack_top
	mov rsp, rax
	and rsp, -16
	mov rdi, rbx
	mov rsi, r12
	call syscall_dispatch_rs
	mov rsp, r12

	// Store syscall return value into saved rax slot (rsp+112 at this point)
	mov [rsp + 112], rax

.global syscall_exit
syscall_exit:
	// Pop order reverses push: r15..r12, r11, r10, r9, pad, rbp, rdi, rsi, rdx, rcx, rbx, rax, rflags, rip, user_rsp
	pop r15
	pop r14
	pop r13
	pop r12
	pop r11
	pop r10
	pop r9
	add rsp, 8
	pop rbp
	pop rdi
	pop rsi
	pop rdx
	pop rcx
	pop rbx
	pop rax
	pop r11
	pop rcx
	pop r8
	mov rsp, r8

	cli
	sysretq
	"#,
	stack_size = const SYSCALL_STACK_SIZE,
);
