use core::sync::atomic::AtomicU64;

use lazyinit::LazyInit;
use spin::Mutex;
use x86_64::{PhysAddr, VirtAddr, registers::control::Cr3, structures::paging::{FrameAllocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame, Size4KiB, mapper::MapToError}};

use crate::{errno::KernelErrors, memory::{HHDM_OFFSET, PAGE_SIZE, mmio::map_mmio_region, pmm::{self, PhysAllocator}, vmm::phys_to_virt}};

pub static KERNEL_MAPPER: Mutex<LazyInit<OffsetPageTable<'static>>> = Mutex::new(LazyInit::new());
/// Physical address of the kernel's active PML4 (restore during syscalls from user CR3).
pub static KERNEL_PML4: AtomicU64 = AtomicU64::new(0);

pub fn init() {
	let mut allocator = pmm::KERNEL_FRAME_ALLOCATOR.lock();
	let (mapper, pml4_frame) = new_page_table(&mut *allocator);
	KERNEL_MAPPER.lock().call_once(|| {
		mapper
	});
	unsafe { switch_pml4(pml4_frame) };
	KERNEL_PML4.store(pml4_frame.start_address().as_u64(), core::sync::atomic::Ordering::Release);
}

pub fn new_page_table(frame_allocator: &mut impl FrameAllocator<Size4KiB>) -> (OffsetPageTable<'static>, PhysFrame<Size4KiB>) {
	let hhdm_offset = HHDM_OFFSET.lock().expect("HHDM_OFFSET not set");
	let hhdm_virt = VirtAddr::new(hhdm_offset);

	let new_pml4_frame: PhysFrame<Size4KiB> = frame_allocator
		.allocate_frame()
		.expect("failed allocating new frame\n");

	let new_pml4_virt = hhdm_virt + new_pml4_frame.start_address().as_u64();
	let new_pml4 = unsafe { &mut *new_pml4_virt.as_mut_ptr::<PageTable>() };
	new_pml4.zero();

	let (current_pml4_frame, _) = Cr3::read();
	let current_pml4_virt = hhdm_virt + current_pml4_frame.start_address().as_u64();
	let current_pml4 = unsafe { &*current_pml4_virt.as_mut_ptr::<PageTable>() };

	for i in 256..512 {
		if !current_pml4[i].is_unused() {
			new_pml4[i] = current_pml4[i].clone()
		}
	}

	let mapper = unsafe { OffsetPageTable::new(new_pml4, hhdm_virt) };
	(mapper, new_pml4_frame)
}


pub unsafe fn get_active_pml4() -> &'static mut PageTable {
	let (pml4_phys, _) = Cr3::read();
	let pml4_virt = phys_to_virt(pml4_phys.start_address());
	unsafe { &mut *pml4_virt.as_mut_ptr() }
}

pub unsafe fn switch_pml4(pml4_phys: PhysFrame<Size4KiB>) {
	let (_, flags) = Cr3::read();
	unsafe { Cr3::write(pml4_phys, flags) };
}

pub unsafe fn map_phys_page(
	page: Page<Size4KiB>,
	frame: PhysFrame<Size4KiB>,
	flags: PageTableFlags,
	frame_allocator: &mut impl FrameAllocator<Size4KiB>
) -> Result<(), MapToError<Size4KiB>>{
	let pml4 = unsafe { get_active_pml4() };
	let hhdm_offset = HHDM_OFFSET.lock().expect("HHDM_OFFSET not set\n");
	let mut mapper = unsafe { OffsetPageTable::new(pml4, VirtAddr::new(hhdm_offset)) };
	let result = unsafe { mapper.map_to(page, frame, flags, frame_allocator) };
	result?.flush();
	Ok(())
}



pub unsafe fn map_mmio(
	frame: PhysFrame<Size4KiB>,
) -> VirtAddr {
	map_mmio_region(frame.start_address().as_u64(), 1).unwrap_or_else(|| {
		panic!("Failed to map mmio region")
	})
}

pub fn duplicate_user_address_space(parent_cr3: u64) -> Result<u64, KernelErrors> {

	let mut allocator = pmm::KERNEL_FRAME_ALLOCATOR.lock();

	let (_, child_pml4_frame) = new_page_table(&mut *allocator);

	let parent_pml4 = unsafe { &*phys_to_virt(PhysAddr::new(parent_cr3)).as_ptr::<PageTable>() };
	let child_pml4 = unsafe { &mut *phys_to_virt(child_pml4_frame.start_address()).as_mut_ptr::<PageTable>()};

	for pml4_idx in 0..256 {
		let parent_pte = &parent_pml4[pml4_idx];
		if parent_pte.is_unused() { continue; }
		if parent_pte.flags().contains(PageTableFlags::HUGE_PAGE) {
			return Err(KernelErrors::EINVAL);
		}
		
		let child_pdpt_frame = allocator.allocate_frame().ok_or(KernelErrors::ENOMEM)?;
		let child_pdpt_virt = phys_to_virt(child_pdpt_frame.start_address());
		let child_pdpt: &mut PageTable = unsafe { &mut *child_pdpt_virt.as_mut_ptr()};

		child_pdpt.zero();
		child_pml4[pml4_idx].set_flags(parent_pte.flags());
		child_pml4[pml4_idx]
			.set_frame(child_pdpt_frame, parent_pte.flags());
		
		let parent_pdpt_frame = parent_pte.frame().map_err(|_| KernelErrors::ENOMEM)?;
		let parent_pdpt_virt = phys_to_virt(parent_pdpt_frame.start_address());
		let parent_pdpt: &PageTable = unsafe { &*parent_pdpt_virt.as_ptr() };

		copy_subtree(parent_pdpt, child_pdpt, 3, &mut *allocator)?;
	}

	return Ok(child_pml4_frame.start_address().as_u64());
}

fn copy_subtree(parent_table: &PageTable, child_table: &mut PageTable, level:u32, frame_allocator: &mut PhysAllocator) -> Result<(), KernelErrors>{
	for i in 0..512 {
		let parent_pte = &parent_table[i];
		if parent_pte.is_unused() { continue; }
		if parent_pte.flags().contains(PageTableFlags::HUGE_PAGE) { return Err(KernelErrors::EINVAL);}
		if level > 1 {
			let parent_subtable_frame = parent_pte.frame().map_err(|_| KernelErrors::ENOMEM)?;
			let parent_subtable_virt = phys_to_virt(parent_subtable_frame.start_address());
			let parent_subtable: &PageTable =
				unsafe { &*parent_subtable_virt.as_ptr() };
			let child_subtable_frame = frame_allocator.allocate_frame().ok_or(KernelErrors::ENOMEM)?;
			let child_subtable_virt = phys_to_virt(child_subtable_frame.start_address());
			let child_subtable: &mut PageTable = 
				unsafe { &mut *child_subtable_virt.as_mut_ptr() };

			child_subtable.zero();

			child_table[i].set_frame(child_subtable_frame, parent_pte.flags());
			copy_subtree(parent_subtable, child_subtable, level-1, frame_allocator)?;
		} else {
			let leaf_flags = parent_pte.flags();
			if !leaf_flags.contains(PageTableFlags::USER_ACCESSIBLE) {
				continue;
			}
			let parent_leaf_frame = parent_pte.frame().map_err(|_| KernelErrors::ENOMEM)?;
			let child_leaf_frame = frame_allocator.allocate_frame().ok_or(KernelErrors::ENOMEM)?;

			let src = phys_to_virt(parent_leaf_frame.start_address()).as_mut_ptr::<u8>();
			let dst = phys_to_virt(child_leaf_frame.start_address()).as_mut_ptr::<u8>();
			unsafe {
				core::ptr::copy_nonoverlapping(src, dst, PAGE_SIZE);
			}
			child_table[i].set_frame(child_leaf_frame, leaf_flags);
		}
	}

	Ok(())
}