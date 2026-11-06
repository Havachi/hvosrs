use core::{arch::global_asm, ptr::addr_of};

use x86_64::{VirtAddr, registers::{model_specific::{LStar, SFMask, Star}, rflags::RFlags}};

use crate::{arch::x86_64::gdt::{KERNEL_CODE, KERNEL_DATA, USER_CODE, USER_DATA}, klog, kpanic, scheduler};

/// Scratch space below the saved `SyscallFrame` for `call syscall_dispatch_rs` (callee stack).
/// 512 was too small; deep Rust + alloc could clobber the frame and corrupt `rip_saved` for `sysretq`.
pub const SYSCALL_DISPATCH_REDZONE: usize = 4096;

pub const SYSCALL_STACK_SIZE: usize = 8192;
/// Trap frame and `sysret` epilogue only; handlers run on the process kernel stack.
#[unsafe(no_mangle)]
static mut SYSCALL_STACK: [u8; SYSCALL_STACK_SIZE] = [0; SYSCALL_STACK_SIZE];

/// User RSP for the stack switch. IF is clear here (`SFMASK`), so one static is enough.
#[unsafe(no_mangle)]
#[used]
static mut SYSCALL_USER_RSP: u64 = 0;

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
// Must match syscall_entry push order (see asm): r15 at r12+0 … rflags at +120, rip at +128, user_rsp at +136.
const _: () = assert!(core::mem::offset_of!(SyscallFrame, rflags_saved) == 120);
const _: () = assert!(core::mem::offset_of!(SyscallFrame, rip_saved) == 128);
const _: () = assert!(core::mem::offset_of!(SyscallFrame, user_rsp) == 136);


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
.global SYSCALL_USER_RSP

.global syscall_entry
syscall_entry:
	// Park user RSP without touching rbx or r8 (both are live across `syscall` in userland).
	// rax stays the syscall number until it is passed to syscall_dispatch_rs.
	mov qword ptr [rip + SYSCALL_USER_RSP], rsp
	mov rsp, qword ptr [rip + {kstack_top}]
	test rsp, rsp
	jnz 1f
	lea rsp, [rip + SYSCALL_STACK]
	add rsp, {stack_size}
1:
	and rsp, -16

	// SyscallFrame memory layout (rsp=r12 points to low address after pushes):
	// +0 r15 .. +56 r8, +64 rbp, +72 rdi, +80 rsi, +88 rdx, +96 rcx, +104 rbx, +112 rax,
	// +120 rflags_saved, +128 rip_saved, +136 user_rsp  (see const offset_of! above).
	// Placeholder 0 in rax slot; return value written to [r12+112] before syscall_exit.
	push qword ptr [rip + SYSCALL_USER_RSP]
	push rcx
	push r11
	push 0
	push rbx
	push rcx
	push rdx
	push rsi
	push rdi
	push rbp
	push r8
	push r9
	push r10
	push r11
	push r12
	push r13
	push r14
	push r15

	mov r12, rsp
	// 18 pushes * 8 = 144 B frame; RSP stays 16-byte aligned. Do not sub 8 here — before `call`
	// the AMD64 ABI requires RSP % 16 == 0; misalignment makes callee `movaps` fault (e.g. sys_execve).
	sub rsp, {red_zone}
	mov rdi, rax
	mov rsi, r12
	call syscall_dispatch_rs
	add rsp, {red_zone}
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
	pop r8
	pop rbp
	pop rdi
	pop rsi
	pop rdx
	pop rcx
	pop rbx
	pop rax
	pop r11
	// sysretq loads RFLAGS from R11; clear TF and RF (resume flag) on return to user.
	and r11, 0xfffffffffffcfeff
	pop rcx
	pop rsp

	cli
	sysretq
.global resume_syscall_frame
resume_syscall_frame:
	mov rsp, rdi
	jmp syscall_exit
	"#,
	kstack_top = sym crate::scheduler::CURRENT_KSTACK_TOP,
	stack_size = const SYSCALL_STACK_SIZE,
	red_zone = const SYSCALL_DISPATCH_REDZONE,
);
