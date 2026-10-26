use core::ptr::{self, Unique};
use bitflags::bitflags;


#[derive(Debug)]
pub struct IoApicRegisters {
	ioregsel: Unique<u32>,
	ioregwin: Unique<u32>,
}


impl IoApicRegisters {
	pub unsafe fn new(base_addr: u64) -> Self {
		let base = base_addr as *mut u32;
		IoApicRegisters {
			ioregsel: unsafe { Unique::new_unchecked(base.offset(0)) },
			ioregwin: unsafe { Unique::new_unchecked(base.offset(4)) },
		}
	}

	pub unsafe fn read(&mut self, reg: u32) -> u32 {
		unsafe {
			ptr::write_volatile(self.ioregsel.as_ptr(), reg);
			ptr::read_volatile(self.ioregwin.as_ptr())
		}
	}

	pub unsafe fn write(&mut self, reg: u32, data: u32) {
		unsafe {
			ptr::write_volatile(self.ioregsel.as_ptr(), reg);
			ptr::write_volatile(self.ioregwin.as_ptr(), data);
		}	
	}

	pub unsafe fn set(&mut self, reg: u32, mask: u32) {
		unsafe {
			ptr::write_volatile(self.ioregsel.as_ptr(), reg);
			let val = ptr::read_volatile(self.ioregwin.as_ptr());
			ptr::write_volatile(self.ioregwin.as_ptr(), val | mask);
		}
	}

	pub unsafe fn clear(&mut self, reg: u32, mask: u32) {
		unsafe {
			ptr::write_volatile(self.ioregsel.as_ptr(), reg);
			let val = ptr::read_volatile(self.ioregwin.as_ptr());
			ptr::write_volatile(self.ioregwin.as_ptr(), val & !mask);
		}
	}
}


#[derive(Debug)]
pub struct IoApic {
	regs: IoApicRegisters,
}


impl IoApic {
	pub unsafe fn new(base_addr: u64) -> Self {
		IoApic {
			regs: unsafe { IoApicRegisters::new(base_addr) }
		}
	}

	pub unsafe fn init(&mut self) {
		unsafe {
			let end = self.max_table_entry().saturating_add(1);
			for i in 0..end {
				let lo_val = IrqFlags::MASKED.bits();
				self.regs.write(lo(i), lo_val);
				self.regs.write(hi(i), 0x0);	
			}
		}
	}

	pub unsafe fn route(&mut self, gsi: u8, vector: u8, apic_id: u8) {

		let hi_val = (u32::from(apic_id)) << 24;
		let lo_val = u32::from(vector);
		unsafe {
			self.regs.write(hi(gsi), hi_val);
			self.regs.write(lo(gsi), lo_val);
		}
	}

	pub unsafe fn id(&mut self) -> u8 {
		unsafe {
			((self.regs.read(IOAPIC_ID_REG) >> 24) & 0xF) as u8
		}
	}

	pub unsafe fn set_id(&mut self, id: u8) {
		unsafe {
			self.regs.write(IOAPIC_ID_REG, u32::from(id) << 24);
		}
	}

	pub unsafe fn version(&mut self) -> u8 {
		unsafe {
			(self.regs.read(IOAPIC_VERSION_REG) & 0xFF) as u8
		}
	}

	pub unsafe fn max_table_entry(&mut self) -> u8 {
		unsafe {
			((self.regs.read(IOAPIC_VERSION_REG) >> 16) & 0xFF) as u8
		}
		
	}

	pub unsafe fn arbitration_id(&mut self) -> u8 {
		unsafe {
			((self.regs.read(IOAPIC_ARBITRATION_REG) >> 24) & 0xF) as u8
		}
	}

	pub unsafe fn set_arbitration_id(&mut self, id: u8) {
		unsafe {
			self.regs.write(IOAPIC_ARBITRATION_REG, u32::from(id) << 24);
		}
	}

	pub unsafe fn table_entry(&mut self, irq: u8) -> RedirectionTableEntry {
		unsafe {
			let lo = self.regs.read(lo(irq));
			let hi = self.regs.read(hi(irq));
			RedirectionTableEntry::from_raw(lo,hi)
		}
	}

	pub unsafe fn set_table_entry (
		&mut self,
		irq: u8,
		entry: RedirectionTableEntry
	) {
		unsafe {
			let (lo_val, hi_val) = entry.into_raw();
			self.regs.write(lo(irq), lo_val);
			self.regs.write(hi(irq), hi_val);
		}
	}

	pub unsafe fn enable_irq(&mut self, irq: u8) {
		unsafe {
			self.regs.clear(lo(irq), IrqFlags::MASKED.bits());
		}
	}

	pub unsafe fn disable_irq(&mut self, irq: u8) {
		unsafe {
			self.regs.set(lo(irq), IrqFlags::MASKED.bits());
		}
	}
}

#[inline]
fn lo(irq: u8) -> u32 {
	IOAPIC_TBLBASE_REG + (2 * u32::from(irq))
}

#[inline]
fn hi(irq: u8) -> u32 {
	lo(irq) + 1
}

#[derive(Default)]
pub struct RedirectionTableEntry {
	low:u32,
	high:u32,
}

impl RedirectionTableEntry {
	pub fn from_raw(low: u32, high: u32) -> Self {
		Self {low,high}
	}

	pub fn into_raw(&self) -> (u32,u32) {
		(self.low, self.high)
	}
	/// Returns the interrupt vector.
    pub fn vector(&self) -> u8 {
        (self.low & 0xff) as u8
    }
    /// Sets the interrupt vector to `vector`.
    pub fn set_vector(&mut self, vector: u8) {
        self.low = self.low & !0xff | vector as u32
    }
    /// Returns the interrupt delivery mode.
    pub fn mode(&self) -> IrqMode {
        self.low.try_into().unwrap()
    }
    /// Sets the interrupt delivery mode to `mode`.
    pub fn set_mode(&mut self, mode: IrqMode) {
        self.low = self.low & !IRQ_MODE_MASK | mode.as_u32()
    }

    /// Returns the redirection table entry flags.
    pub fn flags(&self) -> IrqFlags {
        IrqFlags::from_bits_truncate(self.low)
    }
    /// Sets the redirection table entry flags to `flags`.
    pub fn set_flags(&mut self, flags: IrqFlags) {
        self.low = self.low & !(IrqFlags::all() - (IrqFlags::SEND_PENDING | IrqFlags::REMOTE_IRR)).bits()
            | (flags - (IrqFlags::SEND_PENDING | IrqFlags::REMOTE_IRR)).bits()
    }
    /// Returns the destination field.
    pub fn dest(&self) -> u8 {
        (self.high >> 24) as u8
    }
    /// Sets the destination field to `dest`.
    pub fn set_dest(&mut self, dest: u8) {
        self.high = (dest as u32) << 24;
    }
}


#[derive(Debug)]
#[repr(u8)]
pub enum IrqMode {
	Fixed	= 0b000,
	Lowest	= 0b001,
	Smi		= 0b010,
	Nmi		= 0b100,
	Init	= 0b101,
	ExtInt	= 0b111,
}

impl IrqMode {
	pub fn as_u32(self) -> u32 {
		(self as u32) << 8
	}
}

impl TryFrom<u32> for IrqMode {
	type Error = u32;
	fn try_from(value: u32) -> Result<Self, Self::Error> {
		match (value & IRQ_MODE_MASK) >> 8 {
			0b000 => Ok(IrqMode::Fixed),
			0b001 => Ok(IrqMode::Lowest),
			0b010 => Ok(IrqMode::Smi),
			0b100 => Ok(IrqMode::Nmi),
			0b101 => Ok(IrqMode::Init),
			0b111 => Ok(IrqMode::ExtInt),
			other => Err(other),
		}
	}
}

bitflags! {
	pub struct IrqFlags: u32 {
		const LOGICAL_DEST	= 1 << 11;
		const SEND_PENDING	= 1 << 12;
		const LOW_ACTIVE	= 1 << 13;
		const REMOTE_IRR	= 1 << 14;
		const LEVEL_TRIG	= 1 << 15;
		const MASKED		= 1 << 16;
	}
}


const IOAPIC_ID_REG: u32			= 0x0000;
const IOAPIC_VERSION_REG: u32		= 0x0001;
const IOAPIC_ARBITRATION_REG: u32	= 0x0002;
const IOAPIC_TBLBASE_REG: u32		= 0x0010;

const IRQ_MODE_MASK: u32			= 0x0000_0700;