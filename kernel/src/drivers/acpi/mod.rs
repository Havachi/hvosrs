use core::{ops::Add, sync::atomic::AtomicUsize};

use alloc::str;
use spin::Mutex;
use x86_64::{PhysAddr, VirtAddr, structures::paging::Mapper};

use crate::{arch::x86_64::paging::KERNEL_MAPPER, drivers::acpi::{ioapic::IoApic, lapic::LocalApic, madt::MadtIOApicIso}, klog, kwarn, limine_base_revision, memory::{HHDM_OFFSET, PAGE_SIZE, pmm::KERNEL_FRAME_ALLOCATOR}};

pub mod madt;
pub mod ioapic;
pub mod lapic;

const BASE_SDT_ENTRY_VIRT: VirtAddr = VirtAddr::new(0x4000_4000);

pub static LAPIC: Mutex<Option<LocalApic>> = Mutex::new(None);
pub static IOAPIC: Mutex<Option<IoApic>> = Mutex::new(None);
pub static INTERRUPT_OVERRIDE: Mutex<[Option<MadtIOApicIso>; 8]> = Mutex::new( [None; 8]);
pub static ISO_COUNT: AtomicUsize = AtomicUsize::new(0);

#[repr(C,packed)]
pub struct AcpiHeader {
	signature: [u8; 4],
	length: u32,
	rev: u8,
	checksum: u8,
	oemid: [u8;6],
	oem_table_id: [u8; 8],
	oem_rev: u32,
	creator_id: [u8;4],
	creator_rev: u32,
}

#[derive(Debug)]
#[repr(C,packed)]
pub struct Rsdp {
	signature: [u8;8],
	checksum: u8,
	oemid: [u8; 6],
	rev: u8,
	rsdt_addr: u32,

	//Revison 2.0+ fields
	length: u32,
	xsdt_addr: u64,
	ext_checksum: u8,
	_reserved: [u8;3],
}

impl Rsdp {
	pub fn parse_rsdp(rsdp_ptr: *const u8) -> Option<Rsdp>{
		let rsdp_phys_addr = if limine_base_revision() < 3 {
			let offset = HHDM_OFFSET.lock().expect("HHDM_OFFSET not set");
			(rsdp_ptr.addr() as u64) - offset
		} else {
			rsdp_ptr.addr() as u64
		};
		let scratch_virt_addr = VirtAddr::new(0x4000_0000);
		let rsdp_phys = PhysAddr::new(rsdp_phys_addr);
		map_acpi_frame(scratch_virt_addr, rsdp_phys);
		let rsdp_virt_ptr = scratch_virt_addr.as_ptr::<u8>();
		let maybe_rsdp: Rsdp = unsafe { core::ptr::read_unaligned(rsdp_virt_ptr as *const Rsdp) };
		if let Ok(signature) = str::from_utf8(&maybe_rsdp.signature) {
			if !(signature == "RSD PTR ") {
				None
			} else {
				Some(maybe_rsdp)
			}
		} else {
			None
		}
	}

	pub fn get_table_ptr(&self) -> *const u8 {
		if self.rev >= 2 {
			return self.xsdt_addr as *const u8;
		} else {
			return self.rsdt_addr as *const u8;
		}
	}

}

#[repr(C,packed)]
pub struct Rsdt {
	header: AcpiHeader,
}

impl Rsdt {
	pub fn parse_rsdt(rsdt_ptr: *const u8) -> Option<*const Rsdt> {
		let scratch_virt_addr = VirtAddr::new(0x4000_2000).add(PAGE_SIZE as u64);
		let rsdt_phys = PhysAddr::new(rsdt_ptr as u64);
		map_acpi_frame(scratch_virt_addr, rsdt_phys);
		let rsdt_ptr = scratch_virt_addr.as_ptr() as *const Rsdt;

		let signature = unsafe { core::ptr::read_unaligned(core::ptr::addr_of!((*rsdt_ptr).header.signature)) };


		if &signature == b"RSDT" {
			Some(rsdt_ptr)			
		} else {
			None
		}
	}

	pub fn entries(ptr: *const Rsdt) -> &'static [u32] {
		let total_size = unsafe { core::ptr::read_unaligned(
			core::ptr::addr_of!((*ptr).header.length)) } as usize;
		let header_size = core::mem::size_of::<AcpiHeader>();
		if header_size > total_size {
			return &[];
		}

		let entry_count = (total_size - header_size) / core::mem::size_of::<u32>() ;

		unsafe {
			let entries_ptr = ptr.add(1) as *const u32;
			core::slice::from_raw_parts(entries_ptr, entry_count)
		}
	}
}


#[repr(C,packed)]
pub struct Xsdt {
	header: AcpiHeader,
}

impl Xsdt {
	pub fn parse_xsdt(xsdt_ptr: *const u8) -> Option<*const Xsdt> {
		let scratch_virt_addr = VirtAddr::new(0x4000_2000);
		let xsdt_phys = PhysAddr::new(xsdt_ptr as u64);
		map_acpi_frame(scratch_virt_addr, xsdt_phys);
		let xsdt_ptr = scratch_virt_addr.as_ptr() as *const Xsdt;

		let signature = unsafe { core::ptr::read_unaligned(core::ptr::addr_of!((*xsdt_ptr).header.signature))  };
		if &signature == b"XSDT" {
			Some(xsdt_ptr)			
		} else {
			None
		}
	}

	pub fn entries(ptr: *const Xsdt) -> &'static [u64] {
		let total_size = unsafe { core::ptr::read_unaligned(
			core::ptr::addr_of!((*ptr).header.length)) } as usize;
		let header_size = core::mem::size_of::<AcpiHeader>();
		if header_size > total_size {
			return &[];
		}

		let entry_count = (total_size - header_size) / core::mem::size_of::<u64>() ;

		unsafe {
			let entries_ptr = ptr.add(1) as *const u64;
			let entries = core::slice::from_raw_parts(entries_ptr, entry_count);
			return entries;
		}
	}
}



fn map_acpi_frame(virt_addr: VirtAddr, phys_addr: PhysAddr) -> Option<()> {
	use x86_64::structures::paging::page_table::PageTableFlags;
	use x86_64::structures::paging::{Page, PhysFrame, Size4KiB};

	let mut mapper = KERNEL_MAPPER.lock();
	let mut allocator = KERNEL_FRAME_ALLOCATOR.lock();

	let rsdp_page = Page::<Size4KiB>::containing_address(virt_addr);
	let frame = PhysFrame::<Size4KiB>::containing_address(phys_addr);

	unsafe {
		mapper.map_to(
			rsdp_page,
			frame,
			PageTableFlags::PRESENT,
			&mut *allocator
		).ok().and(Some(()))
	}
}

fn dispatch_parsing(entry_addr: u64, index_in_table: usize) {
	let entry_virt = BASE_SDT_ENTRY_VIRT.add((index_in_table * 0x1000) as u64);
	let entry_phys = PhysAddr::new(entry_addr);
	map_acpi_frame(entry_virt, entry_phys);
	let sig = unsafe { core::ptr::read_unaligned(entry_virt.as_ptr() as *const [u8;4]) };
	if let Some(sig_str) = str::from_utf8(&sig).ok() {
		match sig_str {
			"APIC" => {
				madt::init(entry_virt.as_ptr());
				if let Some(ioapic) = IOAPIC.lock().as_mut() {
					unsafe { ioapic.init() };
					klog!("I/O APIC init done");
				} else {
					kwarn!("Failed to init I/O APIC\n");
				}

				if let Some(lapic) = LAPIC.lock().as_mut() {
					unsafe { lapic.init(); }
					klog!("Local APIC init done");
				} else {
					kwarn!("Failed to init Local APIC\n");
				}
			}
			_ => {
				//ksprint!("Found \"{}\" entry\n", sig_str);
			}
		}
	}
}

pub fn init(rsdp_ptr: *const u8) {
	if let Some(rsdp) = Rsdp::parse_rsdp(rsdp_ptr) {
		let tbl_ptr = rsdp.get_table_ptr();
		if rsdp.rev >= 2 {
			if let Some(xsdt) = Xsdt::parse_xsdt(tbl_ptr) {
				let entries = Xsdt::entries(xsdt);
				for (i, entry) in entries.iter().enumerate() {
					dispatch_parsing(*entry as u64, i);
				}
				klog!("ACPI init done")
			}
		} else {
			if let Some(rsdt) = Rsdt::parse_rsdt(tbl_ptr) {
				let entries = Rsdt::entries(rsdt);
				for (i, entry) in entries.iter().enumerate() {
					dispatch_parsing(*entry as u64, i);
				}
				klog!("ACPI init done")
			}
		}
	}
}