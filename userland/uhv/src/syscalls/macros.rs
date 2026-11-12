pub unsafe fn syscall0(nr: u64) -> i64 {
	let ret: i64;
	unsafe {	
		core::arch::asm!(
			"syscall",
			inout("rax") nr => ret,
			lateout("rcx") _,
			lateout("r11") _,
		);
	}
	ret
}

pub unsafe fn syscall1(nr: u64, a0: u64) -> i64 {
	let ret: i64;
	unsafe {	
		core::arch::asm!(
			"syscall",
			inout("rax") nr => ret,
			in("rdi") a0,
			lateout("rcx") _,
			lateout("r11") _,
		);
	}
	ret
}

pub unsafe fn syscall2(nr: u64, a0: u64, a1: u64) -> i64 {
	let ret: i64;
	unsafe {	
		core::arch::asm!(
			"syscall",
			inout("rax") nr => ret,
			in("rdi") a0,
			in("rsi") a1,
			lateout("rcx") _,
			lateout("r11") _,
		);
	}
	ret
}

pub unsafe fn syscall3(nr: u64, a0: u64, a1: u64, a2: u64) -> i64 {
	let ret: i64;
	unsafe {	
		core::arch::asm!(
			"syscall",
			inout("rax") nr => ret,
			in("rdi") a0,
			in("rsi") a1,
			in("rdx") a2,
			lateout("rcx") _,
			lateout("r11") _,
		);
	}
	ret
}

pub unsafe fn syscall4(nr: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> i64 {
	let ret: i64;
	unsafe {	
		core::arch::asm!(
			"syscall",
			inout("rax") nr => ret,
			in("rdi") a0,
			in("rsi") a1,
			in("rdx") a2,
			in("r10") a3,
			lateout("rcx") _,
			lateout("r11") _,
		);
	}
	ret
}

#[allow(dead_code)]
pub unsafe fn syscall5(nr: u64, a0: u64, a1: u64, a2: u64, a3: u64, a4: u64) -> i64 {
	let ret: i64;
	unsafe {	
		core::arch::asm!(
			"syscall",
			inout("rax") nr => ret,
			in("rdi") a0,
			in("rsi") a1,
			in("rdx") a2,
			in("r10") a3,
			in("r8") a4,
			lateout("rcx") _,
			lateout("r11") _,
		);
	}
	ret
}

#[allow(dead_code)]
pub unsafe fn syscall6(nr: u64, a0: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64) -> i64 {
	let ret: i64;
	unsafe {	
		core::arch::asm!(
			"syscall",
			inout("rax") nr => ret,
			in("rdi") a0,
			in("rsi") a1,
			in("rdx") a2,
			in("r10") a3,
			in("r8") a4,
			in("r9") a5,
			lateout("rcx") _,
			lateout("r11") _,
		);
	}
	ret
}