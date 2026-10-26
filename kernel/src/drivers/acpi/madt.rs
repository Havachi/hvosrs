use core::sync::atomic::Ordering::Relaxed;

use x86_64::{PhysAddr, structures::paging::PhysFrame};

use crate::{arch::x86_64::paging::map_mmio, drivers::acpi::{AcpiHeader, INTERRUPT_OVERRIDE, IOAPIC, ISO_COUNT, LAPIC, ioapic::IoApic, lapic::LocalApic}, ksprint};


#[repr(packed)]
pub struct MadtTable {
	header: AcpiHeader,
	lapic_addr: u32,
	_flags: u32,
} 

impl MadtTable {
	pub fn display_entries(ptr: *const MadtTable) {
		let iterator = Self::iter(ptr);

		for entry in iterator {
			match entry {
				MadtEntry::Lapic(madt_lapic) => {
					ksprint!("Found LAPIC | ID: {} | PROC: {} |\n", madt_lapic.apic_id, madt_lapic.acpi_proc_id)
				},
				MadtEntry::Ioapic(madt_ioapic) => {
					let ioapic_addr= madt_ioapic.ioapi_addr;
					let gsib = madt_ioapic.gsi_base;
					ksprint!("Found I/O APIC | ID: {} | ADDR: {:#x} | GSIB: {}|\n", madt_ioapic.ioapic_id, ioapic_addr, gsib);
				},
				MadtEntry::IoapicIso(madt_ioapic_iso) => {
					let gsi = madt_ioapic_iso.gsi;
					ksprint!("Found I/O APIC ISO | BUS: {} | IRQ: {} | GSI: {} |\n", madt_ioapic_iso.bus_src, madt_ioapic_iso.irq_src, gsi)
				},
				MadtEntry::IoapicNmiSrc(madt_ioapic_nmi_src) => {
					let gsi = madt_ioapic_nmi_src.gsi;
					ksprint!("Found I/O APIC NMI Source | SRC: {} | GSI: {} |\n", madt_ioapic_nmi_src.nmi_src, gsi)
				},
				MadtEntry::IoapicNmi(madt_ioapic_nmi) => {
					ksprint!("Found I/O APIC NMI | PROC: {} | LINT#: {} |\n", madt_ioapic_nmi.acpi_proc_id, madt_ioapic_nmi.lint_num)
				},
				MadtEntry::LapicAddrOverride(madt_lapic_addr_override) => {
					let addr = madt_lapic_addr_override.addr;
					ksprint!("Found LAPIC Address Override | ADDR: {} |\n", addr)
				},
				MadtEntry::Lx2Apic(_madt_lx2_apic) => {
					ksprint!("Found Lx2APIC | | |\n")
				},
			}
		}
	}

	pub fn iter(ptr: *const MadtTable) -> MadtIterator {
		let total_size = unsafe {
			core::ptr::read_unaligned(core::ptr::addr_of!((*ptr).header.length)) as usize};
		let header_size = core::mem::size_of::<MadtTable>();
		let entries_start_ptr = unsafe {(ptr as *const u8).add(header_size)};
		let entries_end_ptr = unsafe {(ptr as *const u8).add(total_size)};
		
		MadtIterator {
			current_ptr: entries_start_ptr,
			end_ptr: entries_end_ptr,
		}
	}
}
#[derive(Debug, Clone, Copy)]
#[repr(packed)]
pub struct MadtEntryHeader {
	entry_type: u8,
	length: u8,
}
#[repr(C, packed)]
pub struct MadtLapic {
    pub header: MadtEntryHeader,
    pub acpi_proc_id: u8,
    pub apic_id: u8,
    pub flags: u32, 
}

#[repr(C, packed)]
pub struct MadtIOApic {
    pub header: MadtEntryHeader,
    pub ioapic_id: u8,
    pub _reserved: u8,
    pub ioapi_addr: u32,
    /// Global System Interrupt Base
    pub gsi_base: u32,
}

/// I/O APIC Interrupt Source Override
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct MadtIOApicIso {
    pub header: MadtEntryHeader,
    pub bus_src: u8,
    pub irq_src: u8,
    pub gsi: u32,
    pub flags: u16,
}

#[repr(C, packed)]
pub struct MadtIOApicNmiSrc {
    pub header: MadtEntryHeader,
    pub nmi_src: u8,
    pub _reserved: u8,
    pub flags: u16,
    pub gsi: u32,
}

#[repr(C, packed)]
pub struct MadtIOApicNmi {
    pub header: MadtEntryHeader,
    pub acpi_proc_id: u8,
    pub flags: u16,
    pub lint_num: u8,
}

#[repr(C, packed)]
pub struct MadtLapicAddrOverride {
    pub header: MadtEntryHeader,
    pub _reserved: u16,
    pub addr: u64
}

#[repr(C, packed)]
pub struct MadtLx2APIC {
    pub header: MadtEntryHeader,
    pub _reserved: u16,
    pub x2apic_id: u32,
    pub flags: u32,
    pub acpi_id: u32
}

pub enum MadtEntry {
	Lapic(MadtLapic),
	Ioapic(MadtIOApic),
	IoapicIso(MadtIOApicIso),
	IoapicNmiSrc(MadtIOApicNmiSrc),
	IoapicNmi(MadtIOApicNmi),
	LapicAddrOverride(MadtLapicAddrOverride),
	Lx2Apic(MadtLx2APIC),
}

pub struct MadtIterator {
	current_ptr: *const u8,
	end_ptr: *const u8,
}

impl Iterator for MadtIterator {
	type Item = MadtEntry;
	
	fn next(&mut self) -> Option<Self::Item> {
		if self.current_ptr >= self.end_ptr {
			return None;
		}

		unsafe {
			let header = core::ptr::read_unaligned(self.current_ptr as *const MadtEntryHeader);
			let entry_len = header.length as usize;

			let entry = match header.entry_type {
				0 => MadtEntry::Lapic(core::ptr::read_unaligned(self.current_ptr as *const MadtLapic)),
				1 => MadtEntry::Ioapic(core::ptr::read_unaligned(self.current_ptr as *const MadtIOApic)),
				2 => MadtEntry::IoapicIso(core::ptr::read_unaligned(self.current_ptr as *const MadtIOApicIso)),
				3 => MadtEntry::IoapicNmiSrc(core::ptr::read_unaligned(self.current_ptr as *const MadtIOApicNmiSrc)),
				4 => MadtEntry::IoapicNmi(core::ptr::read_unaligned(self.current_ptr as *const MadtIOApicNmi)),
				5 => MadtEntry::LapicAddrOverride(core::ptr::read_unaligned(self.current_ptr as *const MadtLapicAddrOverride)),
				6 => MadtEntry::Lx2Apic(core::ptr::read_unaligned(self.current_ptr as *const MadtLx2APIC)),
				_ => return None
			}; 

			self.current_ptr = self.current_ptr.add(entry_len);
			Some(entry)
		}
	}
	
}

pub fn init(madt_addr: *const u8) {
	let madt = unsafe { core::ptr::read_unaligned( madt_addr as *const MadtTable) };
	let lapic_addr = madt.lapic_addr;
	let lapic_virt = unsafe { map_mmio(
		PhysFrame::containing_address(PhysAddr::new(lapic_addr as u64)),
	) };
	*LAPIC.lock() = Some(LocalApic::new(lapic_virt.as_u64()));
	//MadtTable::display_entries(madt_addr as *const MadtTable);
	let madt_iter = MadtTable::iter(madt_addr as *const MadtTable);
	for entry in madt_iter {
		match entry {
			MadtEntry::Ioapic(ioapic) => {
				unsafe {
					let phys_addr = ioapic.ioapi_addr as u64;
					let phys = PhysAddr::new_truncate(phys_addr);
					let frame = PhysFrame::containing_address(phys);
					let virt = map_mmio(frame);
					
					let mut guard = IOAPIC.lock();
					*guard = Some(IoApic::new(virt.as_u64()));
				}
			}
			MadtEntry::IoapicIso(iso) => {
				let cnt = ISO_COUNT.fetch_add(1, Relaxed);
				INTERRUPT_OVERRIDE.lock()[cnt] = Some(iso);
				//ksprint!("Found I/O APIC ISO | BUS: {} | IRQ: {} | GSI: {} |\n", iso.bus_src, iso.irq_src, gsi)
			}
			_ => {
				//do nothing for the entries I don't care
			}
		}	
	}
}

