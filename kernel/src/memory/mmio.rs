use spin::Mutex;
use x86_64::{PhysAddr, VirtAddr, structures::paging::{Mapper, Page, PageTableFlags, PhysFrame, Size4KiB, page::PageRange}};

use crate::{arch::x86_64::paging::KERNEL_MAPPER, klog, memory::{MMIO_RANGE, MMIO_START, PAGE_SIZE, bitmap::Bitmap, pmm::PhysAllocator}};

const MMIO_BITMAP_PAGES:usize = MMIO_RANGE.nbpages();
const MMIO_BITMAP_SIZE_WORDS:usize = MMIO_BITMAP_PAGES / 64;
static MMIO_BITMAP_STORAGE: Mutex<([u64; MMIO_BITMAP_SIZE_WORDS], usize)> = 
	Mutex::new(([0; MMIO_BITMAP_SIZE_WORDS], 0));


pub fn init_mmio_bitmap() {
	let mmio_page_range = {
		let page_start = Page::<Size4KiB>::containing_address(VirtAddr::new(MMIO_RANGE.start));
		let page_end = Page::<Size4KiB>::containing_address(VirtAddr::new(MMIO_RANGE.end - PAGE_SIZE as u64));
		PageRange::<Size4KiB> {start: page_start, end: page_end}
	};

	let nb_pages = ((mmio_page_range.clone().size() as usize) + PAGE_SIZE - 1) / PAGE_SIZE;
	assert!(nb_pages <= MMIO_BITMAP_PAGES, "MMIO range exceeds MMIO_BITMAP_PAGES: requested:{nb_pages}");
	let mut storage = MMIO_BITMAP_STORAGE.lock();
	storage.1 = nb_pages;

	klog!("MMIO bitmap init done")
}

pub fn map_mmio_region(phys_addr: u64, size: usize) -> Option<VirtAddr> {
	let needed_pages = (size + PAGE_SIZE - 1) / PAGE_SIZE;
	let mut bmap_storage = MMIO_BITMAP_STORAGE.lock();
	let total_pages = bmap_storage.1;
	if needed_pages > total_pages { return None; }

	let mut bitmap = Bitmap::new(&mut bmap_storage.0, total_pages);

	if let Some(p_start) = bitmap.alloc_contiguous(needed_pages){
		let mut mapper = KERNEL_MAPPER.lock();

		let start_addr = VirtAddr::new(MMIO_START + (p_start * PAGE_SIZE) as u64);
		for (i, p) in (p_start..p_start+needed_pages).enumerate() {
			let virt_addr = VirtAddr::new(MMIO_START + (p * PAGE_SIZE) as u64);
			if !MMIO_RANGE.contains(virt_addr.as_u64()) { return None; }

			let page = Page::<Size4KiB>::containing_address(virt_addr);
			let frame = PhysFrame::<Size4KiB>::containing_address(PhysAddr::new(phys_addr + (i * PAGE_SIZE) as u64));

			unsafe {
				match mapper.map_to(
					page,
					frame,
					PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_CACHE | PageTableFlags::WRITE_THROUGH, 
					&mut PhysAllocator
				) {
						Ok(flush) => flush.flush(),
						Err(_err) => panic!("Mapping failed :["),
				};
			}
		}
		return Some(start_addr)
	}
	None
}

pub fn mmio_alloc(size: usize) -> Option<(PhysAddr, VirtAddr)> {
	let needed_pages = (size + PAGE_SIZE - 1) / PAGE_SIZE;
	if let Some(phys_start) = PhysAllocator::allocate_n_frame(needed_pages){
		if let Some(virt) = map_mmio_region(phys_start.as_u64(), size)  {
			return Some((phys_start, virt));
		}
	}
	None
}