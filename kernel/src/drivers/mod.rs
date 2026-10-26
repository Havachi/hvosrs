use crate::{kwarn, limine_requests::{FRAMEBUFFER, RSDP}};

pub mod serial;
pub mod display;
pub mod console;
pub mod apic;
pub mod acpi;
pub mod keyboard;
pub mod fs;
pub mod pci;


pub fn init() {

	if let Some(fb_res) = FRAMEBUFFER.response() {
        if let Some(fb) = fb_res.framebuffers().first() {
            console::init(*fb);
        }
    } else {
        kwarn!("No framebuffer found")
    }

    if let Some(rsdp_res) = RSDP.response() {
        let addr = rsdp_res.address as *const u8;
        acpi::init(addr);
    } else {
        kwarn!("No rsdp available");
    }

	keyboard::init();
    pci::init();
    fs::init();
}