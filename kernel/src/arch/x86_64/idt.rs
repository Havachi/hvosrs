use core::arch::asm;

use lazy_static::lazy_static;
use x86_64::{PhysAddr, VirtAddr, structures::{idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode}, paging::Translate}};

use crate::{arch::x86_64::paging::KERNEL_MAPPER, drivers::acpi::LAPIC, klog, kpanic, ksprint, memory::{HHDM_OFFSET, KERNEL_DYNAMIC_MEM_RANGE, vmm::phys_to_virt}, scheduler::reschedule};

pub const TIMER_INT_VECTOR: u8 = 0x32;
pub const KEYBOARD_INT_VECTOR: u8 = 0x33;
pub const NVME_INT_VECTOR: u8 = 0x40;

lazy_static! {
	static ref IDT: InterruptDescriptorTable = {
		let mut idt = InterruptDescriptorTable::new();
		idt.breakpoint.set_handler_fn(breakpoint_handler).set_privilege_level(x86_64::PrivilegeLevel::Ring3);
		idt.double_fault.set_handler_fn(double_fault_handler);
		idt.page_fault.set_handler_fn(page_fault_handler);
		idt.segment_not_present.set_handler_fn(segment_not_present_handler);
		idt.invalid_opcode.set_handler_fn(invalid_opcode_handler);
		idt.general_protection_fault.set_handler_fn(gpf_handler);
		idt[TIMER_INT_VECTOR].set_handler_fn(idt_timer_interrupt_handler);
		idt[KEYBOARD_INT_VECTOR].set_handler_fn(idt_kbd_interrupt_handler);
		idt[NVME_INT_VECTOR].set_handler_fn(idt_nvme_interrupt_handler);
		idt
	};
}

extern "x86-interrupt" fn segment_not_present_handler(_sf: InterruptStackFrame, code: u64) {
	if let Some(sel) = x86_64::structures::idt::SelectorErrorCode::new(code) {
		let is_external = sel.external();
		let tbl = sel.descriptor_table();
		let idx = sel.index();
		kpanic!("-------- SEGMENT NOT PRESENT --------\next: {is_external}\ntbl:{:?}\nidx:{:#x}\n----------------------------\n", tbl, idx);
	} else {
		kpanic!("-------- SEGMENT NOT PRESENT --------\nno error code\n----------------------------\n");
	}

}

extern "x86-interrupt" fn page_fault_handler(sf:InterruptStackFrame, error_code: PageFaultErrorCode) {
	use x86_64::registers::control::Cr2;
	let cr2 = Cr2::read_raw();

	match try_handle_page_fault(cr2, error_code) {
		Ok(_) => {
			ksprint!("Page fault is maybe fixed\n");
		}
		Err(e) => {
			ksprint!("-------- PAGE FAULT --------\n");
			ksprint!("err: {}\n", e);
			ksprint!("cr2: {:#x}\n", cr2);
			ksprint!("code: {}\n", error_code.bits());
			ksprint!("rip: {:#x}\n", sf.instruction_pointer.as_u64());
			ksprint!("cs: {:#x}\n", sf.code_segment.0);
			ksprint!("----------------------------\n");
			loop {
				unsafe { core::arch::asm!("hlt") }
			}
		}

	}

}


//If page is mapped, check the flags
//if page is not mapped, try map
//if page invalid, return error and panic
//If any remediation failed, return error and panic
fn try_handle_page_fault(cr2: u64, error_code: PageFaultErrorCode) -> Result<(), &'static str>{
	use x86_64::structures::paging::{PageTableFlags, mapper::TranslateResult};
	let offset = HHDM_OFFSET.lock().expect("HHDM_OFFSET not set");
	let virt = 
	if cr2 < offset {
		phys_to_virt(PhysAddr::new(cr2))
	} else {
		VirtAddr::new(cr2)
	};

	let mapper = KERNEL_MAPPER.lock();

	match mapper.translate(virt) {
		TranslateResult::Mapped { frame:_, offset:_, flags } => {
			if !flags.contains(PageTableFlags::PRESENT) {
				return Err("Page not present");
			}
			else if !flags.contains(PageTableFlags::WRITABLE) && error_code.contains(PageFaultErrorCode::CAUSED_BY_WRITE) {
				return Err("Tried to write on a page not marked writable");
			}

			else if !flags.contains(PageTableFlags::USER_ACCESSIBLE) && error_code.contains(PageFaultErrorCode::USER_MODE) {
				return Err("Usermode access to kernel page");
			}

			else if flags.contains(PageTableFlags::NO_EXECUTE) && error_code.contains(PageFaultErrorCode::INSTRUCTION_FETCH) {
				return Err("Tried to execute code on NXE page");
			} 
			else {
				return Err("unknown page fault");
			}
		},
		TranslateResult::NotMapped =>  {

			if KERNEL_DYNAMIC_MEM_RANGE.contains(virt.as_u64()) {
				return Err("access to unmapped page in kernel dynamic range");
			}

			return Err("access to unmapped page");
		},
		TranslateResult::InvalidFrameAddress(_phys_addr) => {
			return Err("invalid frame address");
		},
	}
}

extern "x86-interrupt" fn double_fault_handler(
	_sf: InterruptStackFrame, _e: u64) -> !
{

	kpanic!("DOUBLE FAULT !");
}

extern "x86-interrupt" fn invalid_opcode_handler(
	sf: InterruptStackFrame)
{
	kpanic!("INVALID OPCODE !\n {:#?}", sf);
}

extern "x86-interrupt" fn gpf_handler(
	sf: InterruptStackFrame, err: u64)
{
	ksprint!("-------- # GPF # --------\n");
	ksprint!("code: {:#x}\n", err);
	ksprint!("rip: {:#x}\n", sf.instruction_pointer.as_u64());
	ksprint!("rsp: {:#x}\n", sf.stack_pointer.as_u64());
	ksprint!("cs: {:#x}\n", sf.code_segment.0);
	ksprint!("----------------------------\n");
	loop {
		unsafe { core::arch::asm!("hlt") }
	}

	kpanic!("GENERAL PROTECTION FAULT ! {err}\n");
}

extern "x86-interrupt" fn breakpoint_handler(
	_sf: InterruptStackFrame)
{
	let mut regs = [0u64; 16];
	unsafe {
		asm!(
			"mov [{0} + 0], rax",
			"mov [{0} + 8], rbx",
			"mov [{0} + 16], rcx",
			"mov [{0} + 24], rdx",
			"mov [{0} + 32], rsi",
			"mov [{0} + 40], rdi",
			"mov [{0} + 48], rbp",
			"mov [{0} + 56], rsp",
			"mov [{0} + 64], r8",
			"mov [{0} + 72], r9",
			"mov [{0} + 80], r10",
			"mov [{0} + 88], r11",
			"mov [{0} + 96], r12",
			"mov [{0} + 104], r13",
			"mov [{0} + 112], r14",
			"mov [{0} + 120], r15",
			in(reg) regs.as_mut_ptr(),
			options(nostack, preserves_flags)
    	);
	}
		ksprint!("=== REGISTER DUMP ===\n");
		ksprint!("RAX: {:#x}  RBX: {:#x}\n", regs[0], regs[1]);
		ksprint!("RCX: {:#x}  RDX: {:#x}\n", regs[2], regs[3]);
		ksprint!("RSI: {:#x}  RDI: {:#x}\n", regs[4], regs[5]);
		ksprint!("RBP: {:#x}  RSP: {:#x}\n", regs[6], regs[7]);
		ksprint!("R8 : {:#x}  R9 : {:#x}\n", regs[8], regs[9]);
		ksprint!("R10: {:#x}  R11: {:#x}\n", regs[10], regs[11]);
		ksprint!("R12: {:#x}  R13: {:#x}\n", regs[12], regs[13]);
		ksprint!("R14: {:#x}  R15: {:#x}\n", regs[14], regs[15]);
		ksprint!("=====================\n");
	loop {}
}

#[unsafe(no_mangle)]
extern "x86-interrupt" fn idt_timer_interrupt_handler(_stack_frame: InterruptStackFrame) {
	crate::scheduler::tick();
	crate::interrupts::timer_interrupt_handler();

	if let Some(lapic) = LAPIC.lock().as_mut() {
		unsafe { 
			lapic.eoi();
		}
	}
	reschedule()
}

#[unsafe(no_mangle)]
extern "x86-interrupt" fn idt_kbd_interrupt_handler(_stack_frame: InterruptStackFrame) {
	crate::interrupts::keyboard_interrupt_handler();
	if let Some(lapic) = LAPIC.lock().as_mut() {
		unsafe { 
			lapic.eoi();
		}
	}
}

extern "x86-interrupt" fn idt_nvme_interrupt_handler (_stack_frame: InterruptStackFrame) {
	crate::drivers::block::nvme::interrupt_service();

	if let Some(lapic) = LAPIC.lock().as_mut() {
		unsafe { 
			lapic.eoi();
		}
	}
}

pub fn init() {
	IDT.load();
	klog!("IDT init done")
}