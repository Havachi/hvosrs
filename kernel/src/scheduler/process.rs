
use core::sync::atomic::AtomicU32;

use x86_64::{PhysAddr, VirtAddr, structures::paging::{Page, PageTableFlags, PhysFrame, Size4KiB}};

use crate::{arch::x86_64::{asm::current_cr3, gdt::set_tss_rsp0, paging::{duplicate_user_address_space, free_page_table, map_phys_page, new_page_table, switch_pml4}, syscall::{SyscallFrame, fork_child_return}}, elf::load_elf, errno::KernelErrors, klog, ksprint, memory::{KERNEL_DYNAMIC_MEM_START, PAGE_SIZE, USER_RANGE_END, allocate_n_pages, pmm::{KERNEL_FRAME_ALLOCATOR, PhysAllocator, get_used_frames}}, scheduler::{CURRENT_KSTACK_TOP, MAX_PROCESS, PROCESS_TABLE, Priority, exit_to_user}, vfs::file::VfsFile};

pub type ProcessId = u32;
pub static NEXT_PID: AtomicU32 = AtomicU32::new(10);
pub const MAX_FD_PER_PROCESS: usize = 32;
pub const PROCESS_BASE_NB_PAGE: usize = 1;
pub const KERNEL_STACK_DEFAULT_NB_PAGES: usize = 8;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
	Ready,
	Running,
	Blocked,
	Sleeping,
	Dead,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockedBy {
	Stdin,
	Child
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
	pub fd_table: [Option<VfsFile>; MAX_FD_PER_PROCESS],
	pub fork_resume_frame: Option<SyscallFrame>,
	pub exit_code: u32,
	pub blocked_by: Option<BlockedBy>
}



impl Process {

	pub fn new(
		pid: ProcessId, ppid: ProcessId, entry_point: usize, process_cr3: u64,
	) -> Process {

		let user_stack_nb_pages = PROCESS_BASE_NB_PAGE;
		let user_stack_phys  = match PhysAllocator::allocate_n_frame(user_stack_nb_pages) {
			Some(p) => p,
			None => panic!("OOM while creating a new process")
		};

		let user_stack_page = Page::<Size4KiB>::containing_address(
			VirtAddr::new(USER_RANGE_END - (user_stack_nb_pages * PAGE_SIZE) as u64)
		);

		if user_stack_nb_pages > 1 {
			for i in 0..user_stack_nb_pages {

				let page = Page::<Size4KiB>::containing_address(
					VirtAddr::new(USER_RANGE_END - (PAGE_SIZE * i) as u64)
				);
				let frame = PhysFrame::<Size4KiB>::containing_address(user_stack_phys + (PAGE_SIZE * i) as u64);
				unsafe { let _ = map_phys_page(
					page,
					frame, 
					PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE | PageTableFlags::NO_EXECUTE, 
					&mut PhysAllocator,
				); };

			}
		} else {
			let frame = PhysFrame::<Size4KiB>::containing_address(user_stack_phys);
			unsafe { let _ = map_phys_page(
				user_stack_page,
				frame, 
				PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE | PageTableFlags::NO_EXECUTE, 
				&mut PhysAllocator,
			); };
		}

		let user_rsp = user_stack_page.start_address().as_u64() + (PAGE_SIZE * user_stack_nb_pages) as u64;
	
		let kernel_stack_nb_pages = KERNEL_STACK_DEFAULT_NB_PAGES;
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
			fd_table,
			fork_resume_frame: None,
			exit_code: 0,
			blocked_by: None,
		}
	}

	/// Kernel thread. `context_switch` `ret`s straight to `entry_point` with CS still ring 0.
	pub fn new_kernel(
		pid: ProcessId, ppid: ProcessId, entry_point: usize, process_cr3: u64,
	) -> Process {
		let kernel_stack_nb_pages = KERNEL_STACK_DEFAULT_NB_PAGES;
		let kernel_stack_buffer_addr = allocate_n_pages(kernel_stack_nb_pages);
		let kernel_stack_top = kernel_stack_buffer_addr.as_u64() + (PAGE_SIZE * kernel_stack_nb_pages) as u64;
		// 16 zeroed regs, then the entry point. The return slot is 16 bytes below the
		// top so `ret` enters the function with RSP ≡ 8 (mod 16).
		let mut ksp = kernel_stack_top as *mut u64;
		unsafe {
			ksp = ksp.offset(-2);
			*ksp = entry_point as u64;
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
			rsp: 0,
			krsp: ksp as u64,
			next_pid: None,
			kstack_top: kernel_stack_top,
			ctx: CpuContext::new(),
			fd_table,
			fork_resume_frame: None,
			exit_code: 0,
			blocked_by: None,
		}
	}


	pub fn sleep(&mut self, wake_tick: u32) {
		self.wake_tick = wake_tick;
		self.state = ProcessState::Sleeping;
	}

	pub fn block(&mut self, reason: BlockedBy) {
		self.blocked_by = Some(reason);
		self.state = ProcessState::Blocked;
	}

	pub fn wake(&mut self) {
		self.wake_tick = 0;
		self.state = ProcessState::Ready;
	}

	pub fn unblock(&mut self) {
		self.blocked_by = None;
		self.state = ProcessState::Ready;
	}
}

pub fn fork_child_from(parent: &Process, frame: &SyscallFrame) -> Result<Process, KernelErrors> {
	let child_cr3 = duplicate_user_address_space(parent.cr3)?;
	let child_pid = get_next_pid();
	if child_pid as usize >= MAX_PROCESS {
		return Err(KernelErrors::EAGAIN);
	}
	
	let kernel_stack_nb_pages = KERNEL_STACK_DEFAULT_NB_PAGES;
	let kernel_stack_buffer_addr = allocate_n_pages(kernel_stack_nb_pages);
	let kernel_stack_base = kernel_stack_buffer_addr.as_u64() as usize;
	let kernel_stack_top = kernel_stack_buffer_addr.as_u64() + (PAGE_SIZE * kernel_stack_nb_pages) as u64;
	let mut ksp = (kernel_stack_base + PAGE_SIZE) as *mut u64;

	unsafe {
		ksp = ksp.offset(-1);
		*ksp = frame.user_rsp;
		ksp = ksp.offset(-1);
		*ksp = frame.rip_saved;
		ksp = ksp.offset(-1);
		*ksp = fork_child_return as *const () as u64;
	
		for _ in 0..16 {
			ksp = ksp.offset(-1);
			*ksp = 0;
		}
	}

	Ok(
		Process {
			pid: child_pid,
			ppid: parent.pid,
			state: ProcessState::Ready,
			ctx: CpuContext::new(),
			cr3: child_cr3,
			rsp: frame.user_rsp,
			fd_table: parent.fd_table.clone(),
			krsp: ksp as u64,
			kstack_top: kernel_stack_top,
			ticks: 0, wake_tick: 0, next_pid: None, priority: Priority::Critial,
			fork_resume_frame: Some(*frame),
			exit_code: 0,
			blocked_by: None,
		}
	)

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

pub fn get_next_pid() -> ProcessId {
	NEXT_PID.fetch_add(1, core::sync::atomic::Ordering::Relaxed)
}

pub fn exec_replace_image(process: &mut Process, elf_bytes: &[u8]) -> Result<u64, KernelErrors> {
	let (_, new_cr3_frame) = new_page_table(&mut PhysAllocator);
	let kernel_cr3 = PhysFrame::containing_address(PhysAddr::new(current_cr3()));
	unsafe { switch_pml4(new_cr3_frame) };
	let entry = load_elf(elf_bytes).map_err(|_| KernelErrors::EINVAL)?;
	let user_rsp = map_user_stack_in_current_cr3(PROCESS_BASE_NB_PAGE)?;
	unsafe { switch_pml4(kernel_cr3) };

	process.cr3 = new_cr3_frame.start_address().as_u64();
	process.rsp = user_rsp;
	process.fork_resume_frame = None;
	(process.krsp, process.kstack_top) = kernel_stack_bootstrap(user_rsp, entry, exit_to_user as *const () as u64);
	set_tss_rsp0(VirtAddr::new(process.kstack_top));
	CURRENT_KSTACK_TOP.store(process.kstack_top, core::sync::atomic::Ordering::Release);
	
	Ok(entry)
}

pub fn map_user_stack_in_current_cr3(nb_pages: usize) -> Result<u64, KernelErrors> {
	let user_stack_phys  = match PhysAllocator::allocate_n_frame(nb_pages) {
		Some(p) => p,
		None => panic!("OOM while creating a new process")
	};

	let user_stack_page = Page::<Size4KiB>::containing_address(
		VirtAddr::new(USER_RANGE_END - (nb_pages * PAGE_SIZE) as u64)
	);

	if nb_pages > 1 {
		for i in 0..nb_pages {

			let page = Page::<Size4KiB>::containing_address(
				VirtAddr::new(USER_RANGE_END - (PAGE_SIZE * i) as u64)
			);
			let frame = PhysFrame::<Size4KiB>::containing_address(user_stack_phys + (PAGE_SIZE * i) as u64);
			unsafe { let _ = map_phys_page(
				page,
				frame, 
				PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE | PageTableFlags::NO_EXECUTE, 
				&mut PhysAllocator,
			); };

		}
	} else {
		let frame = PhysFrame::<Size4KiB>::containing_address(user_stack_phys);
		unsafe { let _ = map_phys_page(
			user_stack_page,
			frame, 
			PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE | PageTableFlags::NO_EXECUTE, 
			&mut PhysAllocator,
		); };
	}
	let user_rsp = user_stack_page.start_address().as_u64() + (PAGE_SIZE * nb_pages) as u64;
	Ok(user_rsp)
}

fn kernel_stack_bootstrap(user_rsp: u64, entry_point: u64, return_fn: u64) -> (u64, u64) {

	let kernel_stack_nb_pages = KERNEL_STACK_DEFAULT_NB_PAGES;
	let kernel_stack_buffer_addr = allocate_n_pages(kernel_stack_nb_pages);
	let kernel_stack_base = kernel_stack_buffer_addr.as_u64() as usize;
	let kernel_stack_top = kernel_stack_buffer_addr.as_u64() + (PAGE_SIZE * kernel_stack_nb_pages) as u64;
	let mut ksp = (kernel_stack_base + PAGE_SIZE) as *mut u64;

	unsafe {
		ksp = ksp.offset(-1);
		*ksp = user_rsp;
		ksp = ksp.offset(-1);
		*ksp = entry_point;
		ksp = ksp.offset(-1);
		*ksp = return_fn;
	
		for _ in 0..16 {
			ksp = ksp.offset(-1);
			*ksp = 0;
		}
	}
	(ksp as u64, kernel_stack_top)
}

pub fn find_zombie(parent_pid: ProcessId, id: u64) -> Option<(ProcessId, u32)> {
	let mut pt = PROCESS_TABLE.lock();

	for p in pt.iter_mut() {
		if let Some(proc) = p {
			let proc_pid = proc.pid;
			let proc_exit_code = proc.exit_code;
			if proc.state == ProcessState::Dead && proc.ppid == parent_pid {
				if id == 0 {
					drop(pt);
					return Some((proc_pid, proc_exit_code));
				} else {
					if proc.pid == id as ProcessId {
						drop(pt);
						return Some((proc_pid, proc_exit_code));
					}
				}
			}
		}
	}
	drop(pt);
	None
}

pub fn has_waitable_child(parent_pid: ProcessId) -> bool {
	let mut pt = PROCESS_TABLE.lock();
	for p in pt.iter_mut() {
		if let Some(proc) = p {
			if proc.ppid == parent_pid {
				drop(pt);
				return true;
			}
		}
	}
	drop(pt);
	false
}

pub fn reap(pid: ProcessId) {
	let used_before = get_used_frames() as i64;
	let mut pt = PROCESS_TABLE.lock();
	if let Some(proc) = pt[pid as usize].take().as_mut() {
		if proc.pid == 0 { drop(pt);return; }
		let mut allocator = KERNEL_FRAME_ALLOCATOR.lock();
		free_page_table(proc.cr3, &mut *allocator);

		let proc_kernel_stack_base = proc.kstack_top - (KERNEL_STACK_DEFAULT_NB_PAGES * PAGE_SIZE) as u64;
		let phys_proc_kernel_stack_base = PhysAddr::new(proc_kernel_stack_base - KERNEL_DYNAMIC_MEM_START);
		PhysAllocator::free_n_frame(phys_proc_kernel_stack_base, KERNEL_STACK_DEFAULT_NB_PAGES);

		for fd in proc.fd_table.as_mut() {
			fd.take();
		}

	}
	let used_after = get_used_frames() as i64;
	let freed = used_before - used_after;
	klog!("Reap freed {} frames", freed);
	drop(pt);
}