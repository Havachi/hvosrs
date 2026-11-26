use core::sync::atomic::Ordering::Relaxed;

use x86_64::instructions::port::Port;

use crate::{arch::x86_64::idt::{KEYBOARD_INT_VECTOR, TIMER_INT_VECTOR}, drivers::acpi::{INTERRUPT_OVERRIDE, IOAPIC, ISO_COUNT}, kerror};

pub const PIT_HZ: u32 = 100;

pub fn init() {
	init_pit(PIT_HZ);
	if let Some(ioapic) = IOAPIC.lock().as_mut() {
		unsafe { ioapic.route(lookup_gsi(2), TIMER_INT_VECTOR, 0) };
		unsafe { ioapic.route(lookup_gsi(1), KEYBOARD_INT_VECTOR, 0) };
	}
	x86_64::instructions::interrupts::enable();
}

pub fn init_pit(hz: u32) {
	let divisor = (1_193_182u32 / hz).clamp(1, 65_535) as u16;
    let mut cmd = Port::<u8>::new(0x43);
    let mut ch0 = Port::<u8>::new(0x40);

	unsafe {
        cmd.write(0x34);
        ch0.write((divisor & 0xFF) as u8);
        ch0.write((divisor >> 8) as u8);
    }
}

fn lookup_gsi(legacy_irq: u8) -> u8 {
	let count = ISO_COUNT.load(Relaxed);
	let overrides  = *INTERRUPT_OVERRIDE.lock();
	for i in 0..count {
		if let Some(iso) = overrides[i] {
			if iso.irq_src == legacy_irq {
				return iso.gsi as u8;
			}
		}
	}
	legacy_irq
}

pub fn timer_interrupt_handler() {
	
}

pub fn keyboard_interrupt_handler() {
	let scancode = unsafe { Port::<u8>::new(0x60).read() };
	if let Err(e) = crate::drivers::keyboard::handle_scancode(scancode) {
		kerror!("error handle keyboard: {:?}", e)
	}
}