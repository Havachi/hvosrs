use core::ops::Add;

use x86_64::{VirtAddr, structures::paging::{Page, PageTableFlags, PhysFrame, Size4KiB, mapper::MapToError}};

use crate::{arch::x86_64::paging::map_phys_page, kpanic, memory::{PAGE_SIZE, pmm::PhysAllocator}};


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

pub const PF_X: u32 = 1;
pub const PF_W: u32 = 2;
pub const PF_R: u32 = 4;

pub fn load_elf(data: &[u8]) -> Result<u64,&'static str> {
	if data.len() < core::mem::size_of::<Elf64Header>() {
		return Err("Elf file too small");
	}
	let elf_header = unsafe {&*(data.as_ptr() as *const Elf64Header)};
	if elf_header.e_ident[0..4] != ELF_MAGIC {
		return Err("Invalid elf magic");
	}

	if elf_header.e_ident[4] != 2 || elf_header.e_ident[5] != 1 {
		return Err("Unsupported ELF class or endianness (Supports only ELF64 LE)");
	}

	let phdr_start = elf_header.e_phoff as usize;
	let phdr_size = elf_header.e_phentsize as usize;	

	for i in 0..elf_header.e_phnum as usize {
		let offset = phdr_start + (i * phdr_size);
		if offset + phdr_size > data.len() {
			return Err("Program header offset out of bounds");
		}
		let phdr = unsafe {&*(data.as_ptr().add(offset) as *const Elf64Phdr)};
		if phdr.p_type == PT_LOAD {
			let dest_ptr = phdr.p_vaddr as *mut u8;
			if phdr.p_filesz > 0 {
				let src_offset = phdr.p_offset as usize;
				if src_offset + phdr.p_filesz as usize > data.len() {
					return Err("Segment data offset out of bounds");
				}
				unsafe {
					let src_ptr = data.as_ptr().add(src_offset);
					let nb_pages = (phdr.p_filesz as usize + PAGE_SIZE - 1) / PAGE_SIZE;
					if let Some(frame) = PhysAllocator::allocate_n_frame(nb_pages){
						let flags = PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE | PageTableFlags::WRITABLE;
						for p in 0..nb_pages as u64 {

							let virt = VirtAddr::new(phdr.p_vaddr).add(p * PAGE_SIZE as u64);
							let page = Page::<Size4KiB>::containing_address(virt);
							let physframe = PhysFrame::containing_address(frame.add(p * PAGE_SIZE as u64));
							let res = map_phys_page(page, physframe, flags, &mut PhysAllocator);
							match res {
								Ok(_) => {},
								Err(e) => {
									match e {
										MapToError::PageAlreadyMapped(_) => continue,
										err => { kpanic!("Allocation Failed\n {:?}", err) }
									}
								}
							}

						}
						core::ptr::copy_nonoverlapping(src_ptr, dest_ptr, phdr.p_filesz as usize);
					} else {
						kpanic!("OOM while loading a ELF")
					}

				};
			}
			if phdr.p_memsz > phdr.p_filesz {
				unsafe {
					let zero_start = dest_ptr.add(phdr.p_filesz as usize);
					let zero_len = (phdr.p_memsz - phdr.p_filesz) as usize;
					core::ptr::write_bytes(zero_start, 0, zero_len);
				}
			}
		}
	}
	Ok(elf_header.e_entry as *const () as u64)
}
