use x86_64::{VirtAddr, structures::paging::{Page, PageTableFlags, PhysFrame, Size4KiB, mapper::MapToError}};

use crate::{arch::x86_64::paging::{map_phys_page, or_page_flags, set_page_flags}, memory::{PAGE_SIZE, pmm::PhysAllocator}};


#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Header {
	pub e_ident: [u8; 16],
	pub e_type: u16,
	pub e_machine: u16,
	pub e_version: u32,
	pub e_entry: u64,
	pub e_phoff: u64,
	pub e_shoff: u64,
	pub e_flags: u32,
	pub e_ehsize: u16,
	pub e_phentsize: u16,
	pub e_phnum: u16,
	pub e_shentsize: u16,
	pub e_shnum: u16,
	pub e_shstrndx: u16,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Elf64Phdr {
	pub p_type: u32,
	pub p_flags: u32,
	pub p_offset: u64,
	pub p_vaddr: u64,
	pub p_paddr: u64,
	pub p_filesz: u64,
	pub p_memsz: u64,
	pub p_align: u64,
}

pub const ELF_MAGIC: [u8;4] = [0x7F, b'E', b'L', b'F'];
pub const PT_LOAD: u32 = 1;

pub const PF_X: u32 = 1 << 0;
pub const PF_W: u32 = 1 << 1;
pub const PF_R: u32 = 1 << 2;

pub fn load_elf(data: &[u8]) -> Result<u64,&'static str> {
	if data.len() < core::mem::size_of::<Elf64Header>() {
		return Err("Elf file too small");
	}
	let elf_header = unsafe { core::ptr::read_unaligned(data.as_ptr() as *const Elf64Header) };
	if elf_header.e_ident[0..4] != ELF_MAGIC {
		return Err("Invalid elf magic");
	}

	if elf_header.e_ident[4] != 2 || elf_header.e_ident[5] != 1 {
		return Err("Unsupported ELF class or endianness (Supports only ELF64 LE)");
	}

	let phdr_start = usize::try_from(elf_header.e_phoff).map_err(|_| "Invalid phdr offset")?;
	let phdr_size = usize::try_from(elf_header.e_phentsize).map_err(|_| "Invalid phdr size")?;	
	let phdr_count = usize::from(elf_header.e_phnum);

	if phdr_size < core::mem::size_of::<Elf64Phdr>() {
		return Err("ELF program header entry too small");
	}

	let page_size = PAGE_SIZE as u64;
	let page_mask = page_size - 1;

	for i in 0..phdr_count {
		let offset = i
			.checked_mul(phdr_size)
			.and_then(|v| phdr_start.checked_add(v))
			.ok_or("Program header offset overflow")?;
		let end = offset
			.checked_add(phdr_size)
			.ok_or("Program header end overflow")?;
		if end > data.len() {
			return Err("Program header out of bound");
		}

		let phdr = unsafe { core::ptr::read_unaligned(data.as_ptr().add(offset) as *const Elf64Phdr) };
		if phdr.p_type == PT_LOAD {
			if phdr.p_memsz < phdr.p_filesz {
				return Err("ELF segment memsz < filesz");
			}
			if phdr.p_memsz == 0 {
				continue;
			}
			let dest_ptr = phdr.p_vaddr as *mut u8;
			let seg_end = phdr.p_vaddr.checked_add(phdr.p_memsz).ok_or("ELF segment too large")?;

			let map_begin = phdr.p_vaddr &!page_mask;
			let map_end = seg_end
				.checked_add(page_mask)
				.ok_or("ELF segment map_end overflow")?
				& !page_mask;
			
			let nb_pages = usize::try_from((map_end - map_begin) / page_size)
				.map_err(|_| "Too many ELF pages")?;

			let phys_start = PhysAllocator::allocate_n_frame(nb_pages)
				.ok_or("OOM while loading ELF segment")?;

			// CR0.WP is set, so the kernel copy below faults on a read-only page.
			// Map writable for the fill, then drop W (and apply NX) once every segment is in.
			let mut flags = PageTableFlags::PRESENT
				| PageTableFlags::USER_ACCESSIBLE
				| PageTableFlags::WRITABLE;
			if (phdr.p_flags & PF_X) == 0 {
				flags |= PageTableFlags::NO_EXECUTE;
			}

			for p in 0..nb_pages {
				let page_offset = (p as u64) * page_size;
				let virt = VirtAddr::new(map_begin + page_offset);
				let page = Page::<Size4KiB>::containing_address(virt);
				let physframe = PhysFrame::containing_address(phys_start + page_offset);
				let res = unsafe { map_phys_page(page, physframe, flags, &mut PhysAllocator) };
				match res {
					Ok(_) => {},
					Err(e) => {
						match e {
							MapToError::PageAlreadyMapped(_) => {
								// Overlapping LOAD segments share a page: merge permissions
								// and release the unused frame we allocated for this slot.
								unsafe {
									or_page_flags(page, flags)
										.map_err(|_| "Failed to update ELF page flags")?;
								}
								PhysAllocator::free_frame(physframe.start_address());
							}
							_ => return Err("Failed to map ELF segment page"),
						}
					}
				}
			}

			if phdr.p_filesz > 0 {
				let src_offset = usize::try_from(phdr.p_offset).map_err(|_| "Invalid segment offset")?;
				let filesz = usize::try_from(phdr.p_filesz).map_err(|_| "Invalid filesz")?;
				let src_end = src_offset
					.checked_add(filesz)
					.ok_or("Segment file range overflow")?;
				
				if src_end > data.len() {
					return Err("Segment data offset out of bounds");
				}
				unsafe {
					let src_ptr = data.as_ptr().add(src_offset);
					core::ptr::copy_nonoverlapping(src_ptr, dest_ptr, filesz);
				}
			}
			if phdr.p_memsz > phdr.p_filesz {
				unsafe {
					let zero_offset = usize::try_from(phdr.p_filesz).map_err(|_| "Invalid BSS offset")?;
					let zero_len = usize::try_from(phdr.p_memsz - phdr.p_filesz)
						.map_err(|_| "Invalid BSS size")?;
					let zero_start = dest_ptr.add(zero_offset);
					core::ptr::write_bytes(zero_start, 0, zero_len);
				}
			}
		}
	}

	apply_final_segment_flags(data, phdr_start, phdr_size, phdr_count, page_size, page_mask)?;
	Ok(elf_header.e_entry)
}

/// Union of PF_* for every LOAD segment that covers `page_addr`, as page-table flags.
fn final_flags_for_page(
	data: &[u8],
	phdr_start: usize,
	phdr_size: usize,
	phdr_count: usize,
	page_addr: u64,
	page_mask: u64,
) -> Result<PageTableFlags, &'static str> {
	let mut writable = false;
	let mut executable = false;
	let mut covered = false;

	for i in 0..phdr_count {
		let offset = i
			.checked_mul(phdr_size)
			.and_then(|v| phdr_start.checked_add(v))
			.ok_or("Program header offset overflow")?;
		let phdr = unsafe { core::ptr::read_unaligned(data.as_ptr().add(offset) as *const Elf64Phdr) };
		if phdr.p_type != PT_LOAD || phdr.p_memsz == 0 {
			continue;
		}
		let seg_end = phdr.p_vaddr.checked_add(phdr.p_memsz).ok_or("ELF segment too large")?;
		let map_begin = phdr.p_vaddr & !page_mask;
		let map_end = seg_end
			.checked_add(page_mask)
			.ok_or("ELF segment map_end overflow")?
			& !page_mask;
		if page_addr < map_begin || page_addr >= map_end {
			continue;
		}
		covered = true;
		if (phdr.p_flags & PF_W) != 0 {
			writable = true;
		}
		if (phdr.p_flags & PF_X) != 0 {
			executable = true;
		}
	}

	if !covered {
		return Err("ELF page not covered by a LOAD segment");
	}

	let mut flags = PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;
	if writable {
		flags |= PageTableFlags::WRITABLE;
	}
	if !executable {
		flags |= PageTableFlags::NO_EXECUTE;
	}
	Ok(flags)
}

fn apply_final_segment_flags(
	data: &[u8],
	phdr_start: usize,
	phdr_size: usize,
	phdr_count: usize,
	page_size: u64,
	page_mask: u64,
) -> Result<(), &'static str> {
	for i in 0..phdr_count {
		let offset = i
			.checked_mul(phdr_size)
			.and_then(|v| phdr_start.checked_add(v))
			.ok_or("Program header offset overflow")?;
		let phdr = unsafe { core::ptr::read_unaligned(data.as_ptr().add(offset) as *const Elf64Phdr) };
		if phdr.p_type != PT_LOAD || phdr.p_memsz == 0 {
			continue;
		}
		let seg_end = phdr.p_vaddr.checked_add(phdr.p_memsz).ok_or("ELF segment too large")?;
		let map_begin = phdr.p_vaddr & !page_mask;
		let map_end = seg_end
			.checked_add(page_mask)
			.ok_or("ELF segment map_end overflow")?
			& !page_mask;

		let mut addr = map_begin;
		while addr < map_end {
			let flags = final_flags_for_page(
				data, phdr_start, phdr_size, phdr_count, addr, page_mask,
			)?;
			let page = Page::<Size4KiB>::containing_address(VirtAddr::new(addr));
			unsafe {
				set_page_flags(page, flags).map_err(|_| "Failed to set final ELF page flags")?;
			}
			addr += page_size;
		}
	}
	Ok(())
}
