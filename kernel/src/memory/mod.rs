pub mod pmm;
pub mod vmm;
pub mod heap;
pub mod mmio;
pub mod bitmap;

use core::{sync::atomic::{AtomicU64, Ordering}};

use spin::Mutex;
use x86_64::{PhysAddr, VirtAddr, registers::control::Cr3, structures::paging::{Mapper, Page, PageTable, PageTableFlags, PhysFrame, Size4KiB, page::PageRange}};

use crate::{arch::x86_64::{paging::{self, KERNEL_MAPPER}}, klog, limine_requests::{HHDM, MEMMAP}, memory::pmm::{KERNEL_FRAME_ALLOCATOR, PhysAllocator}};


pub const DIRECT_MAP_START: u64 = 0xFFFF_8000_0000_0000;
pub const DIRECT_MAP_END: u64 = 0xFFFF_CFFF_FFFF_FFFF;
pub const DIRECT_MAP_RANGE: AddrRange = AddrRange::new(DIRECT_MAP_START, DIRECT_MAP_END);

pub const KERNEL_DYNAMIC_MEM_START: u64 = 0xFFFF_D000_0000_0000;
pub const KERNEL_DYNAMIC_MEM_END: u64 = 0xFFFF_DFFF_FFFF_FFFF;
pub static KERNEL_DYNAMIC_MEM_PTR: AtomicU64 = AtomicU64::new(KERNEL_DYNAMIC_MEM_START);
pub const KERNEL_DYNAMIC_MEM_RANGE: AddrRange = AddrRange::new(KERNEL_DYNAMIC_MEM_START, KERNEL_DYNAMIC_MEM_END);


pub const MMIO_START: u64 = 0xFFFF_E000_0000_0000;
pub const MMIO_END: u64 = 0xFFFF_EFFF_FFFF_FFFF;
pub const MMIO_RANGE: AddrRange = AddrRange::new(MMIO_START, MMIO_END);

pub const RESERVED_START: u64 = 0xFFFF_F000_0000_0000;
pub const RESERVED_END: u64 = 0xFFFF_FF7F_FFFF_FFFF;
pub const RESERVED_RANGE: AddrRange = AddrRange::new(RESERVED_START, RESERVED_END);

pub const KCODE_START: u64 = 0xFFFF_FFFF_8000_0000;
pub const KCODE_END: u64 = 0xFFFF_FFFF_FFFF_FFFF;
pub const KCODE_RANGE: AddrRange = AddrRange::new(KCODE_START, KCODE_END);

pub const USER_RANGE_START: u64 = 0x0000_0000_0000_0000;
pub const USER_RANGE_END: u64 = 0x0000_7FFF_FFFF_FFFF;
pub const USER_RANGE: AddrRange = AddrRange::new(USER_RANGE_START, USER_RANGE_END);

pub const USER_HEAP_START: u64 = 0x0000_0000_4000_0000;
pub const USER_HEAP_END: u64 = 0x0000_0000_4400_0000;
pub const USER_HEAP_RANGE: AddrRange = AddrRange::new(USER_HEAP_START, USER_HEAP_END);


pub static HHDM_OFFSET: Mutex<Option<u64>> = Mutex::new(None);
pub const PAGE_SIZE: usize = 4096;

pub fn init() {
	let hhdm_offset = HHDM.response().expect("HHDM request not honored").offset;
    let memory_map = MEMMAP.response().expect("Memory map request not honored").entries();
	let _ = HHDM_OFFSET.lock().get_or_insert(hhdm_offset);

	pmm::init(memory_map);
	let mem_total = (PhysAllocator::get_total_frames() as f64 * 4096.0) / (1024.0 * 1024.0 * 1024.0);
	let mem_free = (PhysAllocator::get_free_frames() as f64 * 4096.0) / (1024.0 * 1024.0 * 1024.0);
	klog!("PMM init done total: {:.2} GiB free: {:.2} Gib",mem_total, mem_free);
	paging::init();
	klog!("Paging init done");
	heap::init();
	mmio::init_mmio_bitmap();
}


///Allocate n pages and map them in KERNEL_DYNAMIC_MEM range
pub fn allocate_n_pages(n: usize) -> VirtAddr {
	if let Some(phys_start) = PhysAllocator::allocate_n_frame(n) {
		let range_start = KERNEL_DYNAMIC_MEM_PTR.load(Ordering::Relaxed);
		let range_end = range_start + (PAGE_SIZE * n) as u64;
		let page_range = {	
			let start = Page::<Size4KiB>::containing_address(VirtAddr::new(range_start));
			let end = Page::<Size4KiB>::containing_address(VirtAddr::new(range_end));
			PageRange::<Size4KiB> { start, end }
		};	

		KERNEL_DYNAMIC_MEM_PTR.store(
			range_start + (n * PAGE_SIZE) as u64 + PAGE_SIZE as u64,
			Ordering::Relaxed);
		for (i, page) in page_range.enumerate() {
			let frame_addr = PhysAddr::new(phys_start.as_u64() + ((i * PAGE_SIZE) as u64));
			let frame = PhysFrame::containing_address(frame_addr);
			let flags=  PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE;
			unsafe {
				let mut mapper = KERNEL_MAPPER.lock();
				let mut kfa = KERNEL_FRAME_ALLOCATOR.lock();
				if let Ok(res) =
				mapper.map_to(
					page, 
					frame, 
					flags, 
					&mut (*kfa)
				) {
					res.flush();
				}
			}
		}
		VirtAddr::new(range_start)
	} else {
		panic!("OOM")
	}
}

pub fn virt_to_phys_dyn(virt: VirtAddr) -> Option<PhysAddr> {
	let hhdm_offset = HHDM_OFFSET.lock().expect("HHDM_OFFSET not set");
	 let v = virt.as_u64();
	if v >= hhdm_offset && !KERNEL_DYNAMIC_MEM_RANGE.contains(v) {
        return Some(PhysAddr::new(v - hhdm_offset));
    }
    let (level_4_frame, _) = Cr3::read();
    let mut table_phys = level_4_frame.start_address().as_u64();

    for level in (1..=4).rev() {
        let table_virt = (table_phys + hhdm_offset) as *const PageTable;
        let table = unsafe { &*table_virt };

        let index = ((v >> (12 + (level - 1) * 9)) & 0x1FF) as usize;
        let entry = &table[index];

        if !entry.flags().contains(PageTableFlags::PRESENT) {
            return None;
        }

        if level > 1 && entry.flags().contains(PageTableFlags::HUGE_PAGE) {
            let page_mask = (1u64 << (12 + (level - 1) * 9)) - 1;
            let phys_base = entry.addr().as_u64() & !page_mask;
            return Some(PhysAddr::new(phys_base + (v & page_mask)));
        }

        if level == 1 {
            return Some(PhysAddr::new(entry.addr().as_u64() + (v & 0xFFF)));
        }

        table_phys = entry.addr().as_u64();
    }

    None
}

pub struct AddrRange {
	start: u64,
	end: u64,
}

impl AddrRange {
	pub const fn new(start: u64, end: u64) -> AddrRange {
		Self {start, end}
	}

	pub fn contains (&self, addr: u64) -> bool {
		if addr >= self.start && addr <= self.end { return true; }
		return false;
	}
	pub const fn nbpages(&self) -> usize {
		let size = (self.end - self.start) as usize;
		let pg = (size + PAGE_SIZE - 1) / PAGE_SIZE;
		pg
	}
}