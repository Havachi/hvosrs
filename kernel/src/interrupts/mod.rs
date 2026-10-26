use core::sync::atomic::Ordering::Relaxed;

use x86_64::instructions::port::Port;

use crate::{arch::x86_64::idt::{KEYBOARD_INT_VECTOR, TIMER_INT_VECTOR}, drivers::acpi::{INTERRUPT_OVERRIDE, IOAPIC, ISO_COUNT}};

pub fn init() {
	if let Some(ioapic) = IOAPIC.lock().as_mut() {
		unsafe { ioapic.route(lookup_gsi(2), TIMER_INT_VECTOR, 0) };
		unsafe { ioapic.route(lookup_gsi(1), KEYBOARD_INT_VECTOR, 0) };
	}
	x86_64::instructions::interrupts::enable();
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
	crate::drivers::keyboard::handle_scancode(scancode);
}