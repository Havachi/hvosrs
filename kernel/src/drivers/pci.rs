use x86_64::{VirtAddr, instructions::port::Port};
use crate::klog;

const PCI_CONFIG_ADDRESS: u16 = 0xCF8;
const PCI_DATA_ADDRESS: u16 = 0xCFC;


#[derive(Debug, Clone, Copy)]
pub struct PciAddress {
	pub bus: u8,
	pub device: u8,
	pub function: u8,
}

impl PciAddress {
	pub fn new(bus:u8, device:u8, function:u8) -> Self {
		Self { bus, device, function }
	}
	pub fn to_u32(&self) -> u32 {
		(1u32 << 31)
		| ((self.bus as u32) << 16)
		| ((self.device as u32) << 11)
		| ((self.function as u32) << 8)
	}
	pub fn to_u32_with_offset(&self, offset: u8) -> u32 {
		(1u32 << 31)
		| ((self.bus as u32) << 16)
		| ((self.device as u32) << 11)
		| ((self.function as u32) << 8)
		| ((offset as u32) & 0xFC)
	}
}

pub struct PciDeviceInfo {
	pub addr: PciAddress,
	pub vendor_id: u16,
	pub device_id: u16,
	pub class_code: u8,
	pub subclass: u8,
	pub prog_if: u8,
}


pub fn pci_read32(addr: &PciAddress, offset: u8) -> u32 {
	let address = addr.to_u32_with_offset(offset);
	let mut port_config =  Port::<u32>::new(PCI_CONFIG_ADDRESS);
	let mut port_data =  Port::<u32>::new(PCI_DATA_ADDRESS);

	unsafe {
		port_config.write(address);
		port_data.read()
	}
}

pub fn pci_read16(addr: &PciAddress, offset: u8) -> u16{
	let val32 = pci_read32(addr, offset);
	let shift = (offset & 2) * 8;
	((val32 >> shift) & 0xFFFF) as u16
}

pub fn pci_read8(addr: &PciAddress, offset: u8) -> u8{
	let val32 = pci_read32(addr, offset);
	let shift = (offset & 3) * 8;
	((val32 >> shift) & 0xFF) as u8
}

pub fn pci_write32(addr: &PciAddress, offset: u8, data: u32){
	let address = addr.to_u32_with_offset(offset);
	let mut port_config =  Port::<u32>::new(PCI_CONFIG_ADDRESS);
	let mut port_data =  Port::<u32>::new(PCI_DATA_ADDRESS);

	unsafe {
		port_config.write(address);
		port_data.write(data)
	}
}

pub fn pci_write16(addr: &PciAddress, offset: u8, data: u16){
	let address = addr.to_u32_with_offset(offset);
	let mut port_config =  Port::<u32>::new(PCI_CONFIG_ADDRESS);
	let mut port_data =  Port::<u16>::new(PCI_DATA_ADDRESS);
	unsafe {
		port_config.write(address);
		port_data.write(data)
	}
}



pub fn scan_bus<F>(mut cb: F)
where 
	F: FnMut(PciDevice),
{
	for bus in 0..=255 {
		for device in 0..32 {
			let base_addr = PciAddress::new(bus, device, 0);
			let vendor_id = pci_read16(&base_addr, 0x0);
			if vendor_id == 0xFFFF {
				continue;
			}

			let header_type = (pci_read32(&base_addr, 0x0C) >> 16) as u8;
			let max_functions = if ( header_type & 0x80) != 0 { 8 } else { 1 };

			for function in 0..max_functions {
				let addr = PciAddress::new(bus, device, function);
				let v_id = pci_read16(&addr, 0x00);
				if v_id == 0xFFFF {
					continue;
				}
				let header = PciDevice::new(addr);
				cb(header)
			}
		}
	}
}
pub struct PciBar {
	pub phys_addr: u64,
	pub size: u64,
	pub is_mmio: bool,
	pub is_64bit: bool,
}

pub fn read_pci_bar(addr: &PciAddress, bar_idx: u8) -> Option<PciBar> {
	if bar_idx >= 6 { return None; }
	let offset = 0x10 + (bar_idx * 4);
	let original_val = pci_read32(addr, offset);
	if original_val == 0 {
		return None;
	} 

	let is_mmio = (original_val & 0x01) == 0;

	if !is_mmio {
		let port_base = (original_val & !0x03) as u16;
		return Some(
			PciBar {
				phys_addr: port_base as u64,
				size: 0,
				is_mmio: false,
				is_64bit: false,
			}
		);
	}

	let mem_type = (original_val >> 7) & 0x03;
	let is_64bit = mem_type == 0x02;
	let phys_addr = if is_64bit {
		let hi_v = pci_read32(addr, offset + 4) as u64;
		let lo_v = (original_val & !0x0F) as u64;
		(hi_v << 32) | lo_v
	} else {
		(original_val & !0x0F) as u64
	};

	Some(
		PciBar {
			phys_addr,
			size: 0,
			is_mmio: true,
			is_64bit
		}
	)
}

pub fn init() {
	klog!("PCI Driver init done");
}

#[derive(Debug, Clone, Copy)]
pub struct PciDevice {
	pub addr: PciAddress,
	pub common_header: PciCommonHeader,
	pub specific_header: PciSpecificHeader,
}

#[derive(Debug, Clone, Copy)]
pub struct PciCommonHeader {
	pub device_id: u16,
	pub vendor_id: u16,
	pub status: u16,
	pub command: u16,
	pub class: u8,
	pub subclass: u8,
	pub prog_if: u8,
	pub revision_id: u8,
	pub bist: u8,
	pub header_type: u8,
	pub latency_timer: u8,
	pub cache_line_size: u8,
}

#[derive(Debug, Clone, Copy)]
pub enum PciSpecificHeader {
	Type0 {
		bar0: u32,
		bar1: u32,
		bar2: u32,
		bar3: u32,
		bar4: u32,
		bar5: u32,
		///Cardbus CIS Pointer
		ccis_ptr: u32,
		subsystem_id: u16,
		subsystem_vendor_id: u16,
		///Expansion ROM base address
		exp_rom_bar: u32,
		_res0: [u8;3],
		cap_ptr: u8,
		_res1: u32,
		max_lat: u8,
		min_grant: u8,
		int_pin: u8,
		int_line: u8,
	},
	Type1 {
		bar0: u32,

		bar1: u32,

		sec_lat_timer: u8,
		sub_bus_nb: u8,
		sec_bus_nb: u8,
		prim_bus_nb: u8,

		sec_status: u16,
		io_limit: u8,
		io_base: u8,

		mem_limit: u16,
		mem_base: u16,

		pref_mem_limit: u16,
		pref_mem_base: u16,

		pref_base_hi: u32,

		pref_base_lo: u32,

		io_limit_hi: u16,
		io_base_hi: u16,

		res0: [u8; 3],
		cap_ptr: u8,

		exp_rom_base_addr: u32,

		bridge_ctrl: u16,
		int_pin: u8,
		int_line: u8,
	},
	Type2 {
		///CardBus Socket/ExCa base address
		cb_sock_base_addr: u32,

		sec_status: u16,
		res0: u8,
		cap_list_offset: u8,
		
		cb_lat_timer: u8,
		sub_bus_nb: u8,
		cb_bus_nb: u8,
		pci_bus_nb: u8,

		mem_base_addr_0: u32,

		mem_limit_0: u32,

		mem_base_addr_1: u32,

		mem_limit_1: u32,

		io_base_addr_0: u32,

		io_limit_0: u32,

		io_base_addr_1: u32,

		io_limit_1: u32,

		bridge_ctrl: u16,
		int_pin: u8,
		int_line: u8,
		
		subsystem_vendor_id: u16,
		subsystem_device_id: u16,

		pc_card_legacy_base_addr: u32
	},
}

impl PciDevice {
	pub fn new(address: PciAddress) -> PciDevice {
		let vendor_id = pci_read16(&address, 0x00);
        let device_id = pci_read16(&address, 0x02);
        let command = pci_read16(&address, 0x04);
        let status = pci_read16(&address, 0x06);

        let dword_8 = pci_read32(&address, 0x08);
        let revision_id = (dword_8 & 0xFF) as u8;
        let prog_if = ((dword_8 >> 8) & 0xFF) as u8;
        let subclass = ((dword_8 >> 16) & 0xFF) as u8;
        let class = ((dword_8 >> 24) & 0xFF) as u8;

        let dword_c = pci_read32(&address, 0x0C);
        let cache_line_size = (dword_c & 0xFF) as u8;
        let latency_timer = ((dword_c >> 8) & 0xFF) as u8;
        let header_type_raw = ((dword_c >> 16) & 0xFF) as u8;
        let bist = ((dword_c >> 24) & 0xFF) as u8;

		let header_type = header_type_raw & 0x7F;
		let common_header = PciCommonHeader {
			device_id,
			vendor_id,
			status,
			command,
			class,
			subclass,
			prog_if,
			revision_id,
			bist,
			header_type: header_type_raw,
			latency_timer,
			cache_line_size,
		};

		let specific_header = match header_type {
            0 => {
                let bar0 = pci_read32(&address, 0x10);
                let bar1 = pci_read32(&address, 0x14);
                let bar2 = pci_read32(&address, 0x18);
                let bar3 = pci_read32(&address, 0x1C);
                let bar4 = pci_read32(&address, 0x20);
                let bar5 = pci_read32(&address, 0x24);
                let ccis_ptr = pci_read32(&address, 0x28);
                
                let sub_ids = pci_read32(&address, 0x2C);
                let subsystem_vendor_id = (sub_ids & 0xFFFF) as u16;
                let subsystem_id = ((sub_ids >> 16) & 0xFFFF) as u16;

                let exp_rom_bar = pci_read32(&address, 0x30);

                let dword_34 = pci_read32(&address, 0x34);
                let cap_ptr = (dword_34 & 0xFF) as u8;
                let _res0 = [
                    ((dword_34 >> 8) & 0xFF) as u8,
                    ((dword_34 >> 16) & 0xFF) as u8,
                    ((dword_34 >> 24) & 0xFF) as u8,
                ];

                let _res1 = pci_read32(&address, 0x38);

                let dword_3c = pci_read32(&address, 0x3C);
                let int_line = (dword_3c & 0xFF) as u8;
                let int_pin = ((dword_3c >> 8) & 0xFF) as u8;
                let min_grant = ((dword_3c >> 16) & 0xFF) as u8;
                let max_lat = ((dword_3c >> 24) & 0xFF) as u8;

                PciSpecificHeader::Type0 {
                    bar0,
                    bar1,
                    bar2,
                    bar3,
                    bar4,
                    bar5,
                    ccis_ptr,
                    subsystem_id,
                    subsystem_vendor_id,
                    exp_rom_bar,
                    _res0,
                    cap_ptr,
                    _res1,
                    max_lat,
                    min_grant,
                    int_pin,
                    int_line,
                }
            }
            1 => {
                let bar0 = pci_read32(&address, 0x10);
                let bar1 = pci_read32(&address, 0x14);

                let dword_18 = pci_read32(&address, 0x18);
                let prim_bus_nb = (dword_18 & 0xFF) as u8;
                let sec_bus_nb = ((dword_18 >> 8) & 0xFF) as u8;
                let sub_bus_nb = ((dword_18 >> 16) & 0xFF) as u8;
                let sec_lat_timer = ((dword_18 >> 24) & 0xFF) as u8;

                let dword_1c = pci_read32(&address, 0x1C);
                let sec_status = (dword_1c & 0xFFFF) as u16;
                let io_base = ((dword_1c >> 16) & 0xFF) as u8;
                let io_limit = ((dword_1c >> 24) & 0xFF) as u8;

                let dword_20 = pci_read32(&address, 0x20);
                let mem_base = (dword_20 & 0xFFFF) as u16;
                let mem_limit = ((dword_20 >> 16) & 0xFFFF) as u16;

                let dword_24 = pci_read32(&address, 0x24);
                let pref_mem_base = (dword_24 & 0xFFFF) as u16;
                let pref_mem_limit = ((dword_24 >> 16) & 0xFFFF) as u16;

                let pref_base_lo = pci_read32(&address, 0x28);
                let pref_base_hi = pci_read32(&address, 0x2C);

                let dword_30 = pci_read32(&address, 0x30);
                let io_base_hi = (dword_30 & 0xFFFF) as u16;
                let io_limit_hi = ((dword_30 >> 16) & 0xFFFF) as u16;

                let dword_34 = pci_read32(&address, 0x34);
                let cap_ptr = (dword_34 & 0xFF) as u8;
                let res0 = [
                    ((dword_34 >> 8) & 0xFF) as u8,
                    ((dword_34 >> 16) & 0xFF) as u8,
                    ((dword_34 >> 24) & 0xFF) as u8,
                ];

                let exp_rom_base_addr = pci_read32(&address, 0x38);

                let dword_3c = pci_read32(&address, 0x3C);
                let bridge_ctrl = (dword_3c & 0xFFFF) as u16;
                let int_pin = ((dword_3c >> 16) & 0xFF) as u8;
                let int_line = ((dword_3c >> 24) & 0xFF) as u8;

                PciSpecificHeader::Type1 {
                    bar0,
                    bar1,
                    sec_lat_timer,
                    sub_bus_nb,
                    sec_bus_nb,
                    prim_bus_nb,
                    sec_status,
                    io_limit,
                    io_base,
                    mem_limit,
                    mem_base,
                    pref_mem_limit,
                    pref_mem_base,
                    pref_base_hi,
                    pref_base_lo,
                    io_limit_hi,
                    io_base_hi,
                    res0,
                    cap_ptr,
                    exp_rom_base_addr,
                    bridge_ctrl,
                    int_pin,
                    int_line,
                }
            }
            2 => {
                let cb_sock_base_addr = pci_read32(&address, 0x10);

                let dword_14 = pci_read32(&address, 0x14);
                let sec_status = (dword_14 & 0xFFFF) as u16;
                let res0 = ((dword_14 >> 16) & 0xFF) as u8;
                let cap_list_offset = ((dword_14 >> 24) & 0xFF) as u8;

                let dword_18 = pci_read32(&address, 0x18);
                let cb_lat_timer = (dword_18 & 0xFF) as u8;
                let sub_bus_nb = ((dword_18 >> 8) & 0xFF) as u8;
                let cb_bus_nb = ((dword_18 >> 16) & 0xFF) as u8;
                let pci_bus_nb = ((dword_18 >> 24) & 0xFF) as u8;

                let mem_base_addr_0 = pci_read32(&address, 0x1C);
                let mem_limit_0 = pci_read32(&address, 0x20);
                let mem_base_addr_1 = pci_read32(&address, 0x24);
                let mem_limit_1 = pci_read32(&address, 0x28);
                let io_base_addr_0 = pci_read32(&address, 0x2C);
                let io_limit_0 = pci_read32(&address, 0x30);
                let io_base_addr_1 = pci_read32(&address, 0x34);
                let io_limit_1 = pci_read32(&address, 0x38);

                let dword_3c = pci_read32(&address, 0x3C);
                let bridge_ctrl = (dword_3c & 0xFFFF) as u16;
                let int_pin = ((dword_3c >> 8) & 0xFF) as u8;
                let int_line = ((dword_3c >> 16) & 0xFF) as u8;

                let sub_ids = pci_read32(&address, 0x40);
                let subsystem_vendor_id = (sub_ids & 0xFFFF) as u16;
                let subsystem_device_id = ((sub_ids >> 16) & 0xFFFF) as u16;

                let pc_card_legacy_base_addr = pci_read32(&address, 0x44);

                PciSpecificHeader::Type2 {
                    cb_sock_base_addr,
                    sec_status,
                    res0,
                    cap_list_offset,
                    cb_lat_timer,
                    sub_bus_nb,
                    cb_bus_nb,
                    pci_bus_nb,
                    mem_base_addr_0,
                    mem_limit_0,
                    mem_base_addr_1,
                    mem_limit_1,
                    io_base_addr_0,
                    io_limit_0,
                    io_base_addr_1,
                    io_limit_1,
                    bridge_ctrl,
                    int_pin,
                    int_line,
                    subsystem_vendor_id,
                    subsystem_device_id,
                    pc_card_legacy_base_addr,
                }
            }
            other => panic!("Unsupported PCI header type: {}", other),
        };

		PciDevice {
			addr: address,
            common_header,
            specific_header,
        }
	}

	pub fn update_all(&mut self) {
		let addr = self.addr;
		let new = Self::new(addr);
		*self = new;
	}

	pub fn get_cmd_reg(&mut self) -> u16 {
		self.common_header.command = pci_read16(&self.addr, 0x6);
		self.common_header.command
	}
	
	pub fn get_status_reg(&mut self) -> u16 {
		self.common_header.status = pci_read16(&self.addr, 0x4);
		self.common_header.status
	}

	pub fn set_cmd_reg(&mut self, data: u16) {
		let to_write = data & 0b0000010101000111;
		self.common_header.command = to_write;
		pci_write16(&self.addr, 0x6, to_write)
	}

	pub fn set_status_reg(&mut self, data: u16) {
		let to_write = data & 0b1111100100000000;
		pci_write16(&self.addr, 0x4, to_write);
		self.common_header.status = pci_read16(&self.addr, 0x4);
	}

	pub fn find_msix_offset(&self) -> Result<u8, &'static str> {
		self.find_pci_capabilities(0x11)
	}

	pub fn find_pci_capabilities(&self, target_pci_cap_id: u8) -> Result<u8, &'static str> {
		if self.common_header.status & 0x0010 == 0 {
			return Err("This PCI device has no capabilities list");
		}
		let header_type = self.common_header.header_type;
		if header_type == 0x0 {
			let PciSpecificHeader::Type0 { cap_ptr, ..} = self.specific_header else { return Err("aaa") } ; 
			
			let mut current_ptr = cap_ptr & 0xFC;

			while current_ptr != 0 {
				let cap_id = pci_read8(&self.addr, current_ptr);
				let next_ptr = pci_read8(&self.addr, current_ptr + 1) & 0xFC;

				if cap_id == target_pci_cap_id {
					return Ok(current_ptr);
				}
				current_ptr = next_ptr;
			}
		}	
		Err("PCI Capability not found")
	}

	pub fn has_bar(&self) -> bool {
		let header_type = self.common_header.header_type & 0x7F;
		match header_type {
			0 => true,
			1 => true,
			_ => false,
		}
	}

	pub fn get_nb_bar(&self) -> u32 {
		let header_type = self.common_header.header_type & 0x7F;
		match header_type {
			0 => 6,
			1 => 2,
			_ => 0,
		}
	}

	pub fn read_bar(&mut self, bar_idx: u8) -> Option<u32> {
		match self.specific_header {
			PciSpecificHeader::Type0 { ref mut bar0,  ref mut bar1,  ref mut bar2,  ref mut bar3,  ref mut bar4,  ref mut bar5, .. } => {
				let value = pci_read32(&self.addr, 0x10 + (0x4 * bar_idx));
				match bar_idx {
					0 => *bar0 = value, 
					1 => *bar1 = value,
					2 => *bar2 = value,
					3 => *bar3 = value,
					4 => *bar4 = value,
					5 => *bar5 = value,
					_ => return None,
				}
				Some(value)
			},
			PciSpecificHeader::Type1 { ref mut bar0, ref mut bar1, ..} => {
				let value = pci_read32(&self.addr, 0x10 + (0x4 * bar_idx));
				match bar_idx {
					0 => *bar0 = value, 
					1 => *bar1 = value,
					_ => return None,
				}
				Some(value)
			},
			PciSpecificHeader::Type2 {..} => {
				None
			},
		}
	}

	pub fn write_bar(&mut self, bar_idx: u8, value: u32) {
		match self.specific_header {
			PciSpecificHeader::Type0 { .. } => {
				if bar_idx > 5 { return; }
				pci_write32(&self.addr,  0x10 + (0x4 * bar_idx), value);
				self.read_bar(bar_idx);
			},
			PciSpecificHeader::Type1 { .. } => {
				if bar_idx > 1 { return; }
				pci_write32(&self.addr,  0x10 + (0x4 * bar_idx), value);
				self.read_bar(bar_idx);
			},
			_ => return,
		}
	}

	pub fn find_mmio_region_size(&mut self, is_64bit: bool, bar_idx: u8) -> Option<u32> {

		if !self.has_bar() || bar_idx > self.get_nb_bar() as u8 {
			return None;
		}

		if is_64bit {
			if bar_idx + 1 >= self.get_nb_bar() as u8 {
				return None;
			}

			let saved_barx = self.read_bar(bar_idx).unwrap();
			let saved_bary = self.read_bar(bar_idx + 1).unwrap();

			self.write_bar(bar_idx, 0xFFFFFFFF);
			self.write_bar(bar_idx+1, 0xFFFFFFFF);

			let bar_ret = self.read_bar(bar_idx).unwrap();
			let size = !(bar_ret & 0xFFFFFFF0) + 1;
			self.write_bar(bar_idx, saved_barx); 
			self.write_bar(bar_idx+1, saved_bary); 

			return Some(size);
		} else {
			let saved_barx = self.read_bar(bar_idx).unwrap();
			self.write_bar(bar_idx, 0xFFFFFFFF);
			let bar_ret = self.read_bar(bar_idx).unwrap();
			let size = !(bar_ret & 0xFFFFFFF0) + 1;
			self.write_bar(bar_idx, saved_barx); 
			return Some(size);
		}
	}

	pub fn enable_and_parse_msix(&self, offset: u8) -> MsixConfig{
		let mut control = pci_read16(&self.addr, offset + 0x2);
		let table_size = (control & 0x07FF) + 1;
		let table_dword = pci_read32(&self.addr, offset + 0x4);
		let bar_idx = (table_dword & 0x07) as u8;
		let table_byte_offset = table_dword & !0x07;

		control |= 1 << 15;
		control &= !(1 << 14);
		pci_write16(&self.addr, offset + 0x2, control);
		MsixConfig {
			table_size,
			bar_index: bar_idx,
			table_byte_offset
		}
	}
}


pub struct MsixConfig {
	pub table_size: u16,
	pub bar_index: u8,
	pub table_byte_offset: u32,
}

pub struct MsixTableEntry {
	pub msg_addr_low: u32,
	pub msg_addr_high: u32,
	pub msg_data: u32,
	pub vector_control: u32,
}

pub unsafe fn configure_msix_entry(
	tbl_base_virt: VirtAddr,
	entry_idx: usize,
	vector: u8,
	apic_id: u8,
) {
	let entry_ptr = unsafe { tbl_base_virt.as_mut_ptr::<u32>().add(entry_idx * 16) };
	let msg_addr_low = 0xFEE0_0000 | ((apic_id as u32) << 12);
    let msg_addr_high = 0x0000_0000;

	let msg_data = vector as u32;
	let vector_control = 0x0;
	unsafe {
		core::ptr::write_volatile(entry_ptr.add(0), msg_addr_low);
		core::ptr::write_volatile(entry_ptr.add(1), msg_addr_high);
		core::ptr::write_volatile(entry_ptr.add(2), msg_data);
		core::ptr::write_volatile(entry_ptr.add(3), vector_control);
	}
}