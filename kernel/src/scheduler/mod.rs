pub mod process;

use core::{arch::naked_asm, sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64}};
use spin::Mutex;
use x86_64::{PhysAddr, VirtAddr, registers::control::{Cr3, Cr3Flags}, structures::paging::PhysFrame};
use crate::{arch::x86_64::{asm::current_cr3, gdt::set_tss_rsp0}, klog, ksprint, scheduler::process::{Process, ProcessId, ProcessState}};


const MAX_PROCESS: usize = 1024;
pub static PROCESS_TABLE: Mutex<[Option<Process>;MAX_PROCESS]> = Mutex::new([None; MAX_PROCESS]);
pub static SCHEDULER: Mutex<Scheduler> = Mutex::new(Scheduler::new());
pub static NEED_RESCHED: AtomicBool = AtomicBool::new(false);
pub static CURRENT_TICKS: AtomicU32 = AtomicU32::new(0);
pub static CURRENT_PRIO: AtomicU8 = AtomicU8::new(Priority::Low as u8);
pub static CURRENT_PID: AtomicU32 = AtomicU32::new(0xFFFF);
/// Top of the current process kernel stack (same value as TSS RSP0); used on syscall entry.
#[unsafe(no_mangle)]
pub static CURRENT_KSTACK_TOP: AtomicU64 = AtomicU64::new(0);

pub static GLOBAL_TICKS: AtomicU32 = AtomicU32::new(0);
const QUEUE_REFRESH_TICK: u32 = 5000;
pub static NEED_REFRESH: AtomicBool = AtomicBool::new(false);


pub fn init() {
	let idle_proc = Process::new(0, 0, idle_process as *const () as usize, current_cr3());
	{
		let mut pt = PROCESS_TABLE.lock();
		pt[0] = Some(idle_proc);
		drop(pt);
	}
	klog!("scheduler init done")
}

pub fn add_proc(pid: ProcessId, proc: Process) {
	{
		let mut pt = PROCESS_TABLE.lock();
		pt[pid as usize] = Some(proc);
		drop(pt);
	}
	{
		let mut sched = SCHEDULER.lock();
		sched.run_queues[proc.priority as u8 as usize].push(pid);
		drop(sched);
	}
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Priority {
	Critial = 0,
	High = 1,
	Medium = 2,
	Low = 3,
}

impl Priority {
	pub fn get_q(&self) -> u32 {
		match self {
			Self::Critial => 5,
			Self::High => 10,
			Self::Medium => 20,
			//99 because he can't get lower priority that that
			Self::Low => 99,
		}
	}
	pub fn demote(&self) -> Option<Self> {
		match self {
			Priority::Critial => Some(Priority::High),
			Priority::High => Some(Priority::Medium),
			Priority::Medium => Some(Priority::Low),
			Priority::Low => None,
		}
	}
}

impl TryFrom<u8> for Priority {
	type Error = core::convert::Infallible;
	fn try_from(value: u8) -> Result<Self, Self::Error> {
		match value {
			0 => Ok(Priority::Critial),
			1 => Ok(Priority::High),
			2 => Ok(Priority::Medium),
			3 => Ok(Priority::Low),
			_ => Ok(Priority::Low),
		}
	}
}


pub struct Scheduler {
	pub run_queues: [Queue; 4],
	pub sleeping_queue: Queue,
}

impl Scheduler {
	pub const fn new() -> Self {
		Self {
			run_queues: [Queue::new(); 4],
			sleeping_queue: Queue::new(),
		}
	}

	/// `context_switch` treats `new_rsp == 0` as "resume on the stack we just saved".
	const RSP_RESUME_SAVED: u64 = 0;

	pub fn execute_switch(&mut self, mut old_rsp_ptr: *mut u64) -> (*mut u64, u64, u64) {
		use core::sync::atomic::Ordering;
		x86_64::instructions::interrupts::disable();
		let current_pid = CURRENT_PID.load(Ordering::Acquire);
		let preempted_pid = current_pid;
		if current_pid < 1024 {
			let mut target_prio = None;
			let mut pid_to_push = 0;

			let mut pt = PROCESS_TABLE.lock();
			if let Some(current_proc) = pt[current_pid as usize].as_mut(){
				current_proc.ticks = CURRENT_TICKS.load(Ordering::Relaxed);
				if current_proc.state != ProcessState::Sleeping {

					if current_proc.ticks >= current_proc.priority.get_q() {
						current_proc.ticks = 0;
						if let Some(new_prio) = current_proc.priority.demote() {
							current_proc.priority = new_prio;
						}
					current_proc.state = ProcessState::Ready;
					}
					target_prio = Some(current_proc.priority);
					pid_to_push = current_proc.pid;
				}
				old_rsp_ptr = &mut current_proc.krsp as *mut u64;
			}
			drop(pt);
			if let Some(prio) = target_prio {
				self.run_queues[prio as usize].push(pid_to_push);
			}
		}
		CURRENT_PID.store(0xFFFF, Ordering::Release);

		for q in self.run_queues.iter_mut() {
			if let Some(pid) = q.pop(){
				CURRENT_PID.store(pid, Ordering::Release);
				let mut pt = PROCESS_TABLE.lock();
				if let Some(next_proc) = pt[pid as usize].as_mut().cloned() {
					let new_rsp = if pid == preempted_pid && preempted_pid < 1024 {
						Self::RSP_RESUME_SAVED
					} else {
						next_proc.krsp
					};
					let new_cr3 = next_proc.cr3;
					CURRENT_TICKS.store(next_proc.ticks, Ordering::Relaxed);
					CURRENT_PRIO.store( next_proc.priority as u8, Ordering::Relaxed);
					set_tss_rsp0(VirtAddr::new(next_proc.kstack_top));
					CURRENT_KSTACK_TOP.store(next_proc.kstack_top, core::sync::atomic::Ordering::Release);
					drop(pt);
					ksprint!("switching to {}\n", pid);
					return (old_rsp_ptr, new_rsp, new_cr3);
				}
				drop(pt);
			}
		}
		//switch to idle task here
		CURRENT_PID.store(0, Ordering::Relaxed);
		let mut pt = PROCESS_TABLE.lock();
		let idle_proc = pt[0].as_mut().expect("idle process missing");
		idle_proc.state = ProcessState::Running;
		let new_rsp = idle_proc.krsp;
		let new_cr3 = idle_proc.cr3;
		CURRENT_TICKS.store(idle_proc.ticks, Ordering::Relaxed);
		CURRENT_PRIO.store(idle_proc.priority as u8, Ordering::Relaxed);
		set_tss_rsp0(VirtAddr::new(idle_proc.kstack_top));
		CURRENT_KSTACK_TOP.store(idle_proc.kstack_top, core::sync::atomic::Ordering::Release);
		drop(pt);
		(old_rsp_ptr, new_rsp, new_cr3)
	}

}

pub fn schedule() {
	x86_64::instructions::interrupts::disable();
	let mut dummy_rsp: u64 = 0;
	let old_rsp_ptr: *mut u64 = &mut dummy_rsp as *mut u64;
	let current_pid: u32 = CURRENT_PID.load(core::sync::atomic::Ordering::Acquire);

	let mut sched = SCHEDULER.lock();
	if current_pid == 0 && sched.run_queues.iter().all(|q| q.is_empty()) {

		CURRENT_TICKS.store(0, core::sync::atomic::Ordering::Relaxed);
		drop(sched);
		x86_64::instructions::interrupts::enable();
		return;
	}

	let (final_old_rsp, new_rsp, new_cr3) = sched.execute_switch(old_rsp_ptr);
	drop(sched);
	ksprint!("old_rsp: {:#x} rsp: {:#x} cr3: {:#x}\n", final_old_rsp as u64, new_rsp, new_cr3);
	let active_cr3 = current_cr3();
	if active_cr3 != new_cr3 {
		unsafe {
			Cr3::write(
				PhysFrame::containing_address(PhysAddr::new(new_cr3)),
				Cr3Flags::empty()
			);
		}
	}
	unsafe {
		context_switch(final_old_rsp, new_rsp, new_cr3);
	}
}


pub fn tick() {
	use core::sync::atomic::Ordering;
	x86_64::instructions::interrupts::without_interrupts(|| {
		let pid = CURRENT_PID.load(Ordering::Relaxed);
		if pid == 0xFFFF {
			return;
		}
		let ticks = CURRENT_TICKS.fetch_add(1, Ordering::Relaxed);
		let current_prio = Priority::try_from(CURRENT_PRIO.load(Ordering::Relaxed)).unwrap() ;
		if ticks >= current_prio.get_q() {
			NEED_RESCHED.store(true, core::sync::atomic::Ordering::Relaxed);
		}

		let g_ticks = GLOBAL_TICKS.fetch_add(1, Ordering::Relaxed);
		//refresh every QUEUE_REFRESH_TICK ticks
		if g_ticks > 0 && (g_ticks % QUEUE_REFRESH_TICK) == 0 {
			NEED_REFRESH.store(true, Ordering::Relaxed);
		}

		wake_process();
	});
}

pub fn wake_process() {
	let mut sched = SCHEDULER.lock();
	let g_ticks = GLOBAL_TICKS.load(core::sync::atomic::Ordering::Relaxed);
	let mut prev_pid: Option<ProcessId> = None;
	let mut current_node = sched.sleeping_queue.head;
	let mut pids_to_wake = [0; 32];
	let mut wake_count = 0;
	let mut wake_priorities = [Priority::Critial; 32];

	let mut pt = PROCESS_TABLE.lock();

	while let Some(pid) = current_node {
		let mut should_wake = false;
		let mut prio = Priority::Critial;
		if let Some(proc) = pt[pid as usize].as_mut() {
			if g_ticks >= proc.wake_tick && proc.state == ProcessState::Sleeping {
				should_wake = true;
				prio = proc.priority;
				proc.wake();
			}
			current_node = proc.next_pid;
		}
		if should_wake {
			if wake_count < 32 {
				pids_to_wake[wake_count] = pid;
				wake_priorities[wake_count] = prio;
				wake_count += 1;
			}

			if let Some(prev) = prev_pid {
				if let Some(prev_proc) = pt[prev as usize].as_mut() {
					prev_proc.next_pid = current_node;
				}
			} else {
				sched.sleeping_queue.head = current_node;
			}

			if sched.sleeping_queue.tail == Some(pid) {
				sched.sleeping_queue.tail = prev_pid;
			}
		} else {
			prev_pid = Some(pid);
		}
	}
	drop(pt);

	for i in 0..wake_count {
		let pid = pids_to_wake[i];
		let prio = wake_priorities[i];
		sched.run_queues[prio as usize].push(pid);
	}

	drop(sched);

}

pub fn reschedule() {
	use core::sync::atomic::Ordering;
	if NEED_REFRESH.load(Ordering::Relaxed) {
		NEED_REFRESH.store(false, Ordering::Relaxed);
		refresh_queues();
	} 
	if NEED_RESCHED.load(Ordering::Relaxed) {
		ksprint!("RESHED\n");
		NEED_RESCHED.store(false, Ordering::Relaxed);
		schedule();
	}
}

pub fn refresh_queues() {
	ksprint!("R");
	let mut sched = SCHEDULER.lock();
	let mut pt = PROCESS_TABLE.lock();
    let current_active_pid = CURRENT_PID.load(core::sync::atomic::Ordering::Acquire);
	let mut pids_to_queue = [0; 1024];
	let mut count = 0;

	for slot in pt.iter_mut() {
		if let Some(proc) = slot.as_mut() {
			if proc.pid == 0 { continue; }
			proc.priority = Priority::Critial;
			proc.ticks = 0;
			if proc.pid != current_active_pid && proc.state == ProcessState::Ready {
				if count < 1024 {
					pids_to_queue[count] = proc.pid;
					count += 1;
				}
			}
		}
	}

	sched.run_queues[1] = Queue::new();
	sched.run_queues[2] = Queue::new();
	sched.run_queues[3] = Queue::new();
	let mut q0 = Queue::new();

	for idx in 0..count {
		let pid = pids_to_queue[idx] ;
		if let Some(tail_pid) = q0.tail {
			if let Some(tail_proc) = pt[tail_pid as usize].as_mut() {
				tail_proc.next_pid = Some(pid);
			}
			q0.tail = Some(pid);
		} else {
			q0.head = Some(pid);
			q0.tail = Some(pid);
		}

		if let Some(proc) = pt[pid as usize].as_mut() {
			proc.next_pid = None;
		}
	}

	sched.run_queues[0] = q0;
	drop(pt);
	drop(sched);
	GLOBAL_TICKS.store(0, core::sync::atomic::Ordering::Relaxed);
}

pub fn sleep_current(ticks: u32) {

	let mut pt = PROCESS_TABLE.lock();
    let current_active_pid = CURRENT_PID.load(core::sync::atomic::Ordering::Acquire);
	let g_ticks= GLOBAL_TICKS.load(core::sync::atomic::Ordering::Relaxed);

	if let Some(proc) = pt[current_active_pid as usize].as_mut() {
		proc.sleep(g_ticks + ticks);
	}
	drop(pt);

	let mut sched = SCHEDULER.lock();
	sched.sleeping_queue.push(current_active_pid);
	drop(sched);
	schedule();
}

pub fn process_with_pid(pid: ProcessId) -> Option<Process> {
	if let Some(pt) = PROCESS_TABLE.try_lock() {
		return pt[pid as usize];
	}
	None
}

#[derive(Debug, Clone, Copy)]
pub struct Queue {
	pub head: Option<ProcessId>,
	pub tail: Option<ProcessId>,
}

impl Queue {
	pub const fn new() -> Self {
		Self {
			head: None,
			tail: None,
		}
	}

	pub fn is_empty(&self) -> bool {
		self.head.is_none()
	}

	pub fn push_with_table(&mut self, pid: ProcessId, pt: &mut [Option<Process>; 1024]) {
		if let Some(tail_pid) = self.tail {
			if let Some(tail_proc) = pt[tail_pid as usize].as_mut() {
				tail_proc.next_pid = Some(pid);
			}
			self.tail = Some(pid);
		} else {
			self.head = Some(pid);
			self.tail = Some(pid)
		}
		
		if let Some(new_proc) = pt[pid as usize].as_mut() {
			new_proc.next_pid = None;
		}
	}

	pub fn push(&mut self, pid: ProcessId) {
		let mut pt = PROCESS_TABLE.lock();
		
		if let Some(tail_pid) = self.tail {
			if let Some(tail_proc) = pt[tail_pid as usize].as_mut() {
				tail_proc.next_pid = Some(pid);
			}
			self.tail = Some(pid);
		} else {
			self.head = Some(pid);
			self.tail = Some(pid)
		}
		
		if let Some(new_proc) = pt[pid as usize].as_mut() {
			new_proc.next_pid = None;
		}
		drop(pt);
	}

	pub fn pop(&mut self) -> Option<ProcessId> {
		let mut pt = PROCESS_TABLE.lock();
		if let Some(target_pid) = self.head {
			if let Some(target_proc) = pt[target_pid as usize].as_mut() {
				let target_next = target_proc.next_pid;
				self.head = target_next;
				if target_next.is_none() {
					self.tail = None;
				}
				target_proc.next_pid = None;
				drop(pt);
				return Some(target_pid);
			}
		}
		drop(pt);
		None
	}
}




extern "C" fn _worker_task_a() -> ! {
	x86_64::instructions::interrupts::enable();
	loop {
		for _ in 0..1_000_000 {
			unsafe { core::arch::asm!("nop")}
		}
		sleep_current(100);
	}
}

extern "C" fn _worker_task_b() -> ! {
	x86_64::instructions::interrupts::enable();
	loop {
		for _ in 0..1_000_000 {
			unsafe { core::arch::asm!("nop")}
		}
		sleep_current(500);
	}
}

extern "C" fn idle_process() -> ! {
	loop {
		x86_64::instructions::interrupts::enable_and_hlt();
		x86_64::instructions::interrupts::disable();
		reschedule();
		x86_64::instructions::interrupts::enable();
	}
}


macro_rules! save_context {
	() => {
		r#"
			pushfq
			push rax
			push rcx
			push rdx
			push rbx
			push rbp
			push rsi
			push rdi
			push r8
			push r9
			push r10
			push r11
			push r12
			push r13
			push r14
			push r15
		"#
	};
}

macro_rules! restore_context {
	() => {
		r#"
			pop r15
			pop r14
			pop r13
			pop r12
			pop r11
			pop r10
			pop r9
			pop r8
			pop rdi
			pop rsi
			pop rbp
			pop rbx
			pop rdx
			pop rcx
			pop rax
			popfq
			ret
		"#
	};
}

#[unsafe(naked)]
pub unsafe extern "C" fn context_switch(old_rsp_ptr: *mut u64, new_rsp: u64, new_cr3: u64) {
	naked_asm!(
		save_context!(),
		//Save current rsp at old_rsp_ptr
		"mov [rdi], rsp",
		// new_rsp == 0 means resume on the stack we just saved (same task re-selected)
		"test rsi, rsi",
		"jnz 2f",
		"mov rsi, [rdi]",
		"2:",
		//Change rsp to new_rsp
		"mov rsp, rsi",
		//Switch cr3		
		"mov cr3, rdx",
		restore_context!(),
	)
}

///Change current privilege level to Ring 3
#[unsafe(naked)]
pub unsafe extern "C" fn exit_to_user(new_rip: u64, new_rsp: u64) {
	naked_asm!(
		"pop rdi",
		"pop rsi",

		"mov rcx, rdi",
		"mov rsp, rsi",
		"mov r11, 0x202",
		"sysretq",
	)
}
