
use core::ptr::NonNull;

use talc::{TalcLock, base::Talc};
use x86_64::{VirtAddr, structures::paging::{FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB}};

use crate::{arch::x86_64::paging::KERNEL_MAPPER, klog, memory::{PAGE_SIZE, pmm::KERNEL_FRAME_ALLOCATOR}};

const KERNEL_HEAP_START: VirtAddr = VirtAddr::new(0xFFFF_9000_0000_0000);
const MIN_HEAP_GROWTH: usize = PAGE_SIZE;
const CHUNK_METADATA_OVERHEAD: usize = 64;


#[global_allocator]
static ALLOCATOR: TalcLock<spin::Mutex<()>, KernelHeapSource> =
	TalcLock::new(KernelHeapSource::new());

pub fn init() {
	klog!("Heap init done");
}

#[derive(Debug)]
pub struct KernelHeapSource {
	pub talc_heap_end: Option<NonNull<u8>>,
	pub mapped_break: u64,
}

unsafe impl Sync for KernelHeapSource{}
unsafe impl Send for KernelHeapSource{}

impl KernelHeapSource {
	pub const fn new() -> Self {
		Self {
			talc_heap_end: None,
			mapped_break: KERNEL_HEAP_START.as_u64(),
		}
	}
}

unsafe impl talc::source::Source for KernelHeapSource {
    const TRACK_HEAP_END: bool = false;

    fn acquire<B: talc::base::binning::Binning>(
        talc: &mut Talc<Self, B>,
        layout: core::alloc::Layout,
    ) -> Result<(), ()> {
        let mut mapper = KERNEL_MAPPER.lock();
        let mut frame_allocator = KERNEL_FRAME_ALLOCATOR.lock();

        let needed_for_request = layout
            .size()
            .saturating_add(layout.align())
            .saturating_add(CHUNK_METADATA_OVERHEAD);

        let min_bytes = if talc.source.talc_heap_end.is_none() {
            talc::min_first_heap_size::<B>()
                .saturating_add(needed_for_request)
                .max(MIN_HEAP_GROWTH)
        } else {
            needed_for_request.max(MIN_HEAP_GROWTH)
        };

        let bytes_to_map = min_bytes.next_multiple_of(PAGE_SIZE);
        let old_mapped_break = talc.source.mapped_break;
        let new_mapped_break = old_mapped_break.checked_add(bytes_to_map as u64).ok_or(())?;

        let start_page: Page<Size4KiB> =
            Page::containing_address(VirtAddr::new(old_mapped_break));
        let end_page: Page<Size4KiB> =
            Page::containing_address(VirtAddr::new(new_mapped_break - 1));
        let page_range = Page::range_inclusive(start_page, end_page);

        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE;

        for page in page_range {
            let frame = frame_allocator.allocate_frame().ok_or(())?;
            unsafe {
                mapper
                    .map_to(page, frame, flags, &mut *frame_allocator)
                    .map_err(|_| ())?
                    .flush();
            }
        }

        drop(frame_allocator);
        drop(mapper);

        talc.source.mapped_break = new_mapped_break;
        let new_end_ptr = new_mapped_break as *mut u8;

        unsafe {
            if let Some(old_talc_end) = talc.source.talc_heap_end {
                let updated_end = talc.extend(old_talc_end, new_end_ptr);
                talc.source.talc_heap_end = Some(updated_end);
            } else {
                let claimed_end = talc
                    .claim(old_mapped_break as *mut u8, bytes_to_map)
                    .ok_or(())?;
                talc.source.talc_heap_end = Some(claimed_end);
            }
        }

        Ok(())
    }
}