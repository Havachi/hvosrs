use core::ptr::NonNull;

use talc::{TalcLock, base::Talc};

use crate::sbrk;

pub const USER_HEAP_START: u64 = 0x0000_0000_4000_0000;
const PAGE_SIZE: usize = 4096;
const CHUNK_METADATA_OVERHEAD: usize = 64;

#[global_allocator]
static ALLOCATOR: TalcLock<spin::Mutex<()>, UserHeapSource> =
    TalcLock::new(UserHeapSource::new());

#[derive(Debug)]
pub struct UserHeapSource {
    pub current_brk: u64,
    pub current_heap_end: Option<NonNull<u8>>,
}

unsafe impl Sync for UserHeapSource {}
unsafe impl Send for UserHeapSource {}

unsafe impl talc::source::Source for UserHeapSource {
    const TRACK_HEAP_END: bool = false;

    fn acquire<B: talc::base::binning::Binning>(
        talc: &mut Talc<Self, B>,
        layout: core::alloc::Layout,
    ) -> Result<(), ()> {
        let needed_for_request = layout
            .size()
            .saturating_add(layout.align())
            .saturating_add(CHUNK_METADATA_OVERHEAD);

        let min_bytes = if talc.source.current_heap_end.is_none() {
            talc::min_first_heap_size::<B>().saturating_add(needed_for_request)
        } else {
            needed_for_request
        };

        let needed = min_bytes.next_multiple_of(PAGE_SIZE);

        let old = sbrk(needed as u64);
        if old < 0 {
            return Err(());
        }

        let old_brk = old as u64;
        let new_break = old_brk.checked_add(needed as u64).ok_or(())?;
        talc.source.current_brk = new_break;

        unsafe {
            if let Some(old_end) = talc.source.current_heap_end {
                let res = talc.extend(old_end, new_break as *mut u8);
                talc.source.current_heap_end = Some(res);
            } else {
                let res = talc.claim(old_brk as *mut u8, needed).ok_or(())?;
                talc.source.current_heap_end = Some(res);
            }
        }

        Ok(())
    }
}

impl UserHeapSource {
    pub const fn new() -> Self {
        Self {
            current_brk: USER_HEAP_START,
            current_heap_end: None,
        }
    }
}