use core::fmt;

pub struct KernelLogger;

impl fmt::Write for KernelLogger {
	fn write_str(&mut self, s: &str) -> core::fmt::Result {
		crate::drivers::serial::write_str(s);
		Ok(())
	}
}

pub fn _ksprint(args: fmt::Arguments) {
	use fmt::Write;
	x86_64::instructions::interrupts::without_interrupts(|| {
		let mut l = KernelLogger;
		let _ = l.write_fmt(args);
	});
}

pub fn _klog(args: fmt::Arguments) {
	use fmt::Write;
	x86_64::instructions::interrupts::without_interrupts(|| {
		let mut l = KernelLogger;
		let _ = l.write_str("[HvOS] ");
		let _ = l.write_fmt(args);
		let _ = l.write_str("\n");
	});
}

pub fn _kwarn(args: fmt::Arguments) {
	use fmt::Write;
	x86_64::instructions::interrupts::without_interrupts(|| {
		let mut l = KernelLogger;
		let _ = l.write_str("[WARN] ");
		let _ = l.write_fmt(args);
		let _ = l.write_str("\n");
	});
}

pub fn _kerror(args: fmt::Arguments) {
	use fmt::Write;
	x86_64::instructions::interrupts::without_interrupts(|| {
		let mut l = KernelLogger;
		let _ = l.write_str("[ERROR] ");
		let _ = l.write_fmt(args);
		let _ = l.write_str("\n");
	});
}

pub fn _kpanic(args: fmt::Arguments) -> ! {
	use fmt::Write;
	x86_64::instructions::interrupts::without_interrupts(|| {
		let mut l = KernelLogger;
		let _ = l.write_str("-------- [KERNEL PANIC] --------\n");
		let _ = l.write_fmt(args);
		let _ = l.write_str("\n");
		let _ = l.write_str("--------------------------------\n");
	});
	loop { unsafe {core::arch::asm!("hlt")} };
}