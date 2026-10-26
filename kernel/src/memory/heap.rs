
use core::ptr::NonNull;

use talc::{TalcLock, base::Talc};
use x86_64::{VirtAddr, structures::paging::{FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB}};

use crate::{arch::x86_64::paging::KERNEL_MAPPER, klog, memory::{PAGE_SIZE, pmm::KERNEL_FRAME_ALLOCATOR}};

const KERNEL_HEAP_START: VirtAddr = VirtAddr::new(0xFFFF_9000_0000_0000);

#[global_allocator]
static ALLOCATOR: TalcLock<spin::Mutex<()>, KernelHeapSource> = TalcLock::new(KernelHeapSource::new());

pub fn init() {
	klog!("Heap init done");
}

#[derive(Debug)]
pub struct KernelHeapSource {
	pub current_heap_end: Option<NonNull<u8>>,
	pub current_virtual_break: u64,
}

unsafe impl Sync for KernelHeapSource{}
unsafe impl Send for KernelHeapSource{}

impl KernelHeapSource {
	pub const fn new() -> Self {
		Self {
			current_heap_end: None,
			current_virtual_break: KERNEL_HEAP_START.as_u64(),
		}
	}
}

unsafe impl talc::source::Source for KernelHeapSource {
	const TRACK_HEAP_END: bool = false;
	fn acquire<B: talc::base::binning::Binning>(talc: &mut Talc<Self, B>, layout: core::alloc::Layout) -> Result<(), ()> {

		let required_bytes = layout.size().next_multiple_of(PAGE_SIZE);
		let mut mapper = KERNEL_MAPPER.lock();
		let mut frame_allocator = KERNEL_FRAME_ALLOCATOR.lock();

		let current_break = talc.source.current_virtual_break;
		let new_break = current_break + required_bytes as u64;
		
		

		let page_range = {
			let start_page: Page<Size4KiB> = Page::containing_address(VirtAddr::new(current_break));
			let end_page: Page<Size4KiB> = Page::containing_address(VirtAddr::new(new_break - 1u64));
			Page::range_inclusive(start_page, end_page)
		};

		for page in page_range {
			let frame = frame_allocator
				.allocate_frame()
				.ok_or(()).expect("Failed allocating frame for kernel heap");
			let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE;
			unsafe {
				mapper
					.map_to(page, frame, flags, &mut *frame_allocator)
					.map_err(|_| ())?
					.flush();
			}
		}
		talc.source.current_virtual_break = new_break;
		let new_end_ptr = new_break as *mut u8;

		unsafe {
			if let Some(old_end) = talc.source.current_heap_end {
				let res = talc.extend(old_end, new_end_ptr);
				talc.source.current_heap_end = Some(res);
			} else {
				let init_slice = core::slice::from_raw_parts_mut(current_break as *mut u8, required_bytes).as_mut_ptr();
				let res = talc.claim(init_slice, required_bytes).ok_or(())?;
				talc.source.current_heap_end = Some(res)
			}
		}
		Ok(())

	}
}