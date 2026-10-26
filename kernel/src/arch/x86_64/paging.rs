use core::sync::atomic::AtomicU64;

use lazyinit::LazyInit;
use spin::Mutex;
use x86_64::{VirtAddr, registers::control::Cr3, structures::paging::{FrameAllocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame, Size4KiB, mapper::MapToError}};

use crate::memory::{HHDM_OFFSET, mmio::map_mmio_region, pmm, vmm::phys_to_virt};

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

	let new_pml4_frame = frame_allocator
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