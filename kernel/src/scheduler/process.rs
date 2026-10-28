
use core::sync::atomic::AtomicU32;

use x86_64::{PhysAddr, VirtAddr, structures::paging::{Page, PageTableFlags, PhysFrame, Size4KiB}};

use crate::{arch::x86_64::{asm::current_cr3, paging::{map_phys_page, new_page_table, switch_pml4}}, elf::load_elf, ksprint, memory::{PAGE_SIZE, USER_RANGE_END, allocate_n_pages, pmm::PhysAllocator}, scheduler::{Priority, exit_to_user}, vfs::file::VfsFile};

pub type ProcessId = u32;
pub static NEXT_PID: AtomicU32 = AtomicU32::new(10);
pub const MAX_FD_PER_PROCESS: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
	Ready,
	Running,
	Blocked,
	Sleeping,
	Dead,
}


#[repr(C, packed)]
#[derive(Default, Debug, Clone, Copy)]
pub struct CpuContext {
	pub r15: u64,
	pub r14: u64,
	pub r13: u64,
	pub r12: u64,
	pub rbx: u64,
	pub rbp: u64,
	pub rip: u64,
}

impl CpuContext {
	pub fn new() -> Self {
		Self {
			r15: 0,
			r14: 0,
			r13: 0,
			r12: 0,
			rbx: 0,
			rbp: 0,
			rip: 0,
		}
	}
}

#[derive(Debug, Clone)]
pub struct Process {
	pub pid: ProcessId,
	pub ppid: ProcessId,
	pub state: ProcessState,
	pub ctx: CpuContext,
	pub cr3: u64,
	pub rsp: u64,
	pub krsp: u64,
	pub kstack_top: u64,
	pub ticks: u32,
	pub wake_tick: u32,
	pub next_pid: Option<ProcessId>,
	pub priority: Priority,
	pub fd_table: [Option<VfsFile>; MAX_FD_PER_PROCESS]
}



impl Process {

	pub fn new(
		pid: ProcessId, ppid: ProcessId, entry_point: usize, process_cr3: u64,
	) -> Process {

		let user_stack_nb_pages = 1;
		let user_stack_phys  = match PhysAllocator::allocate_n_frame(user_stack_nb_pages) {
			Some(p) => p,
			None => panic!("OOM while creating a new process")
		};
		let user_stack_phys_frame = PhysFrame::<Size4KiB>::containing_address(user_stack_phys);
		let user_stack_page = Page::<Size4KiB>::containing_address(
			VirtAddr::new(USER_RANGE_END - (user_stack_nb_pages * PAGE_SIZE) as u64)
		);

		unsafe { let _ = map_phys_page(
			user_stack_page,
			user_stack_phys_frame, 
			PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE | PageTableFlags::NO_EXECUTE, 
			&mut PhysAllocator,
		); };
		let user_rsp = user_stack_page.start_address().as_u64() + (PAGE_SIZE * user_stack_nb_pages) as u64;
	
		let kernel_stack_nb_pages = 8;
		let kernel_stack_buffer_addr = allocate_n_pages(kernel_stack_nb_pages);
		let kernel_stack_base = kernel_stack_buffer_addr.as_u64() as usize;
		let kernel_stack_top = kernel_stack_buffer_addr.as_u64() + (PAGE_SIZE * kernel_stack_nb_pages) as u64;
		let mut ksp = (kernel_stack_base + PAGE_SIZE) as *mut u64;

		unsafe {
			ksp = ksp.offset(-1);
			*ksp = user_rsp;
			ksp = ksp.offset(-1);
			*ksp = entry_point as u64;
			ksp = ksp.offset(-1);
			*ksp = exit_to_user as *const () as u64;
		
			for _ in 0..16 {
				ksp = ksp.offset(-1);
				*ksp = 0;
			}
		}
		let fd_table = [const {None}; MAX_FD_PER_PROCESS];

		Self {
			pid,
			ppid,
			state: ProcessState::Ready,
			ticks: 0,
			wake_tick: 0,
			priority: Priority::Critial,
			cr3: process_cr3,
			rsp: user_rsp as u64,
			krsp: ksp as u64,
			next_pid: None,
			kstack_top: kernel_stack_top,
			ctx: CpuContext::new(),
			fd_table
		}
	}

	pub fn sleep(&mut self, wake_tick: u32) {
		self.wake_tick = wake_tick;
		self.state = ProcessState::Sleeping;
	}

	pub fn wake(&mut self) {
		self.wake_tick = 0;
		self.state = ProcessState::Ready;
	}

}

fn _thread_exit_stub() -> ! {
	ksprint!("A process has exited\n");
	loop {
		unsafe { core::arch::asm!("hlt"); }
	}
}


pub fn create_process_from_elf(data: &[u8], ppid: ProcessId) -> Result<(ProcessId, Process), &'static str> {
	let (_, cr3) = new_page_table(&mut PhysAllocator);
	let current_pml4 = PhysFrame::containing_address(PhysAddr::new(current_cr3()));

	unsafe { switch_pml4(cr3) };
	if let Ok(entry_point) = load_elf(data) {
		let pid = NEXT_PID.fetch_add(1, core::sync::atomic::Ordering::Relaxed);

		let process = Process::new(pid, ppid, entry_point as usize, cr3.start_address().as_u64());
		unsafe {switch_pml4(current_pml4);}
		return Ok((pid, process));
	} else {
		return Err("Failed loading the process");
	}
}