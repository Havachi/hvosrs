use limine::memmap::*;
use spin::Mutex;
use x86_64::{PhysAddr, structures::paging::{FrameAllocator, FrameDeallocator, PhysFrame, Size4KiB, frame}};
use crate::{kpanic, ksprint, memory::PAGE_SIZE};

//Whole bitmap can cover 256Gio
const BITMAP_LEN: usize = 1024 * 1024;
const USED_CHUNK_MASK: u64 = 0xFFFFFFFFFFFFFFFF;
pub static MEMORY_BITMAP: Mutex<[u64; BITMAP_LEN]> = Mutex::new([0xFFFFFFFFFFFFFFFF; BITMAP_LEN]);

pub static TOTAL_FRAMES: Mutex<u64> = Mutex::new(0);
pub static FREE_FRAMES: Mutex<u64> = Mutex::new(0);


pub static KERNEL_FRAME_ALLOCATOR: Mutex<PhysAllocator> = Mutex::new(PhysAllocator);

pub struct PhysAllocator;

unsafe impl FrameAllocator<Size4KiB> for PhysAllocator {
	fn allocate_frame(&mut self) -> Option<frame::PhysFrame<Size4KiB>> {
		let frame_idx = bitmap_next_free()?;
		bitmap_set(frame_idx);
		return Some(PhysFrame::containing_address(PhysAddr::new_truncate(frame_idx << 12)));
	}
}

impl FrameDeallocator<Size4KiB> for PhysAllocator {
	unsafe fn deallocate_frame(&mut self, frame: PhysFrame<Size4KiB>) {
		let frame_idx = frame.start_address().as_u64() / PAGE_SIZE as u64;
		bitmap_free(frame_idx);
	}
}

impl PhysAllocator {
	pub fn allocate_n_frame(n: usize) -> Option<PhysAddr>{
		let frame_idx = bitmap_next_free_range(n)?;
		bitmap_set_n(frame_idx, n);
		return Some(PhysAddr::new_truncate(frame_idx << 12));
	}

	pub fn free_frame(addr: PhysAddr) {
		let frame_idx = addr.as_u64() / PAGE_SIZE as u64;
		bitmap_free(frame_idx);
	}
	pub fn free_n_frame(start: PhysAddr, n: usize) {
		let start_idx = start.as_u64() / PAGE_SIZE as u64;
		for frame_idx in start_idx..start_idx+n as u64 {
			bitmap_free(frame_idx);
		}
	}

	pub fn get_free_frames() -> u64 {
		*FREE_FRAMES.lock()
	}

	pub fn get_total_frames() -> u64 {
		*TOTAL_FRAMES.lock()
	}
}




pub fn init(memory_map: &[&Entry]) {
	let (total, free) = bitmap_init(memory_map);
	*TOTAL_FRAMES.lock() = total;
	*FREE_FRAMES.lock() = free;
}



fn find_highest_address(memory_map: &[&Entry]) -> u64 {
	let mut highest_address = 0u64;
	for e in memory_map {
		if e.type_ == MEMMAP_USABLE || e.type_ == MEMMAP_BOOTLOADER_RECLAIMABLE {
			let top = e.base + e.length;
			if top > highest_address {
				highest_address = top
			}
		}
	}
	highest_address
}

fn bitmap_init(memory_map: &[&Entry]) -> (u64, u64) {
	let mut free = 0;
	let mem_top = find_highest_address(memory_map);
	let total = (mem_top + PAGE_SIZE as u64 - 1) / PAGE_SIZE as u64;

	if mem_top > (BITMAP_LEN * super::PAGE_SIZE * 64) as u64 {
		kpanic!("pmm bitmap to small")
	}
	for e in memory_map {
		let nb_page = (e.length as usize + PAGE_SIZE - 1) / PAGE_SIZE;
  		if e.type_ == MEMMAP_USABLE || e.type_ == MEMMAP_BOOTLOADER_RECLAIMABLE {
			for p in 0..nb_page {
				let frame_idx = (e.base + (p * PAGE_SIZE) as u64) >> 12;
				if frame_idx == 0{ continue; }
				bitmap_free(frame_idx);
				free+=1;
			}
		}
	}

	(total, free)
}

fn bitmap_set(frame_idx: u64) {
	let map_idx = (frame_idx >> 6) as usize;
	let bit_idx = frame_idx & 63;
	MEMORY_BITMAP.lock()[map_idx] |= 1u64 << bit_idx;
}


fn bitmap_set_n(start: u64, len: usize) {
	let mut bmap = MEMORY_BITMAP.lock();
	for frame_idx in start..start+len as u64 {
		let map_idx = (frame_idx >> 6) as usize;
		let bit_idx = frame_idx & 63;
		bmap[map_idx] |= 1u64 << bit_idx;
	}
}

fn bitmap_free(frame_idx: u64) {
	let map_idx = (frame_idx >> 6) as usize;
	let bit_idx = frame_idx & 63;
	MEMORY_BITMAP.lock()[map_idx] &= !(1u64 << bit_idx);
}

//Return true is used, false if free
fn bitmap_test(frame_idx: u64) -> bool {
	let map_idx = (frame_idx >> 6) as usize;
	let bit_idx = frame_idx & 63;

	MEMORY_BITMAP.lock()[map_idx] & (1u64 << bit_idx) != 0
}

fn bitmap_find_free_chunk() -> usize {
	let bmap = MEMORY_BITMAP.lock();
	for (i, chunk) in bmap.iter().skip(1).enumerate() {
		
		if !(*chunk == USED_CHUNK_MASK) {
			return i;
		}
	}
	kpanic!("OOM in pmm");
}

//Return the next free frame idx
fn bitmap_next_free() -> Option<u64> {
	let chunk_idx = bitmap_find_free_chunk();
	let start_frame_idx = (chunk_idx << 6) as u64;
	for frame_idx in start_frame_idx..(start_frame_idx+64){
		if !bitmap_test(frame_idx) {
			return Some(frame_idx);
		}
	}
	None
}

fn bitmap_next_free_range(n: usize) -> Option<u64> {
	if n == 0 { return None;}
	let bmap = MEMORY_BITMAP.lock();
	let mut consecutive_free = 0;
	let mut start_frame = 0;
	for (chunk_idx, chunk) in bmap.iter().enumerate() {
		if *chunk == USED_CHUNK_MASK {
			consecutive_free = 0;
			continue;
		}
		for bit_idx in 0..64 {
			let current_frame = (chunk_idx as u64 * 64) + bit_idx;
			if (chunk & (1u64 << bit_idx)) == 0 {
				if consecutive_free == 0 {
					start_frame = current_frame;
				}
				consecutive_free += 1;
				if consecutive_free == n {
					return Some(start_frame);
				}
			} else {
				consecutive_free = 0;
			}
		}
	}
	return None;
}


pub fn print_memory_map() {
	let bmap = MEMORY_BITMAP.lock();
	
	let mut current_streak_val = bmap[0];
    let mut current_streak_start_idx = 0;
    let mut streak_count = 1;
	for chunk_idx in 1..bmap.len() {
		let chunk = bmap[chunk_idx];

		if chunk == current_streak_val {
			streak_count += 1;
		} else {
			flush_streak(current_streak_val, current_streak_start_idx, streak_count);

			current_streak_val = chunk;
			current_streak_start_idx = chunk_idx;
			streak_count = 1;
		}
	}
	flush_streak(current_streak_val, current_streak_start_idx, streak_count);
}

fn flush_streak(chunk_val: u64, start_idx: usize, count: usize) {
    let start_addr = start_idx as u64 * 64;

    if count == 1 {
        print_chunk(chunk_val, start_addr);
    } else {
        print_chunk(chunk_val, start_addr);
        
        if count > 2 {
            ksprint!("...same for {} chunks...\n", count - 2);
        }
        
        let end_addr = (start_idx + count - 1) as u64 * 64;
        print_chunk(chunk_val, end_addr);
    }
}

fn print_chunk(chunk: u64, addr: u64) {
		ksprint!("{:#08X} [",addr);
		for i in 0..64 {
			if (chunk & (1u64 << i)) == 0 {
				ksprint!(".")
			} else {
				ksprint!("X")
			}
		}
		ksprint!("]\n");
}