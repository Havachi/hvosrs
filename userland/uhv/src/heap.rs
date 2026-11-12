use core::ptr::NonNull;

use talc::{TalcLock, base::Talc};

use crate::sbrk;

pub const USER_HEAP_START: u64 = 0x0000_0000_4000_0000;
#[global_allocator]
static ALLOCATOR: TalcLock<spin::Mutex<()>, UserHeapSource> = TalcLock::new(UserHeapSource::new());

#[derive(Debug)]
pub struct UserHeapSource {
	pub current_brk: u64,
	pub current_heap_end: Option<NonNull<u8>>,
}

unsafe impl Sync for UserHeapSource{}
unsafe impl Send for UserHeapSource{}

unsafe impl talc::source::Source for UserHeapSource {
	fn acquire<B: talc::base::binning::Binning>(talc: &mut Talc<Self, B>, layout: core::alloc::Layout) -> Result<(), ()> {
		let needed = layout.size().next_multiple_of(4096);
		let old = sbrk(needed as u64);
		if old < 0 {
			return Err(());
		}
		let new_break = old as u64 + needed as u64 ;
		talc.source.current_brk = new_break as u64;
		unsafe {
			if let Some(old_end) = talc.source.current_heap_end {
				let res = talc.extend(old_end, new_break as *mut u8);
				talc.source.current_heap_end = Some(res);
			} else {
				let res = talc.claim(old as *mut u8, needed).ok_or(())?;
				talc.source.current_heap_end = Some(res);
			}
		}
		return Ok(());
	}
}

impl UserHeapSource {
	pub const fn new() -> Self {
		Self { current_brk: USER_HEAP_START, current_heap_end: None  }
	}
}