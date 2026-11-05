use core::{arch::global_asm, ptr::addr_of};

use x86_64::{VirtAddr, registers::{model_specific::{LStar, SFMask, Star}, rflags::RFlags}};

use crate::{arch::x86_64::gdt::{KERNEL_CODE, KERNEL_DATA, USER_CODE, USER_DATA}, klog, kpanic, scheduler};

pub const SYSCALL_STACK_SIZE: usize = 8192;
/// Trap frame and `sysret` epilogue only; handlers run on the process kernel stack.
#[unsafe(no_mangle)]
static mut SYSCALL_STACK: [u8; SYSCALL_STACK_SIZE] = [0; SYSCALL_STACK_SIZE];

/// Fallback when no process is scheduled (`CURRENT_KSTACK_TOP == 0`).
pub fn syscall_stack_top() -> u64 {
	addr_of!(SYSCALL_STACK).cast::<u8>() as u64 + SYSCALL_STACK_SIZE as u64
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SyscallFrame {
    pub r15: u64, pub r14: u64, pub r13: u64, pub r12: u64,
    pub r11: u64, pub r10: u64, pub r9:  u64, pub r8:  u64,
    pub rbp: u64, pub rdi: u64, pub rsi: u64, pub rdx: u64,
    pub rcx: u64, pub rbx: u64, pub rax: u64,
    pub rflags_saved: u64,
    pub rip_saved:    u64,
    pub user_rsp: u64,
}

const _: () = assert!(core::mem::size_of::<SyscallFrame>() == 144);
const _: () = assert!(core::mem::offset_of!(SyscallFrame, rax) == 112);


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
	pub unsafe fn resume_syscall_frame(frame: *const SyscallFrame) -> !;
}

pub unsafe extern "C" fn fork_child_return() -> ! {

	let frame = {
		let pid = scheduler::CURRENT_PID.load(core::sync::atomic::Ordering::Acquire);
		let mut pt = scheduler::PROCESS_TABLE.lock();
		let proc = pt[pid as usize]
			.as_mut()
			.expect("frk_child_return without process");
		let mut frame = proc
			.fork_resume_frame
			.take()
			.expect("fork child without frame");
		frame.rax = 0;
		frame
	};
	unsafe { resume_syscall_frame(&frame) }
	unreachable!()
}


global_asm!(
	r#"
.global SYSCALL_STACK

.global syscall_entry
syscall_entry:
	mov r8, rsp
	mov rbx, rax
	mov rax, qword ptr [rip + {kstack_top}]
	test rax, rax
	jnz 1f
	lea rax, [rip + SYSCALL_STACK]
	add rax, {stack_size}
1:
	mov rsp, rax
	and rsp, -16

	push r8
	push rcx
	push r11
	mov rax, rbx
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

	mov r12, rsp
	sub rsp, 512
	mov rdi, rbx
	mov rsi, r12
	call syscall_dispatch_rs
	add rsp, 512
	mov rsp, r12

	mov [rsp + 112], rax

.global syscall_exit
syscall_exit:
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
.global resume_syscall_frame
resume_syscall_frame:
	mov rsp, rdi
	jmp syscall_exit
	"#,
	kstack_top = sym crate::scheduler::CURRENT_KSTACK_TOP,
	stack_size = const SYSCALL_STACK_SIZE,
);
