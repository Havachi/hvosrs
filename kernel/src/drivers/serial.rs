use core::fmt::{self, Write};

use spin::Mutex;
use x86_64::instructions::port::Port;



const COM1: u16 = 0x3F8;

pub fn init() {
	unsafe {
		Port::<u8>::new(COM1 + 1).write(0x00);
		Port::<u8>::new(COM1 + 3).write(0x80);
		Port::<u8>::new(COM1 + 0).write(0x03);
		Port::<u8>::new(COM1 + 1).write(0x00);
		Port::<u8>::new(COM1 + 3).write(0x03);
		Port::<u8>::new(COM1 + 2).write(0xC7);
		Port::<u8>::new(COM1 + 4).write(0x0B);
	}
}

fn is_transmit_empty() -> bool {
	unsafe { Port::<u8>::new(COM1 + 5).read() & 0x20 != 0 }
}

fn received() -> bool {
	unsafe { Port::<u8>::new(COM1 + 5).read() & 1 != 0 }
}

pub fn write_byte(b: u8) {
	while !is_transmit_empty() {}
	if b == b'\n' {
		unsafe { Port::<u8>::new(COM1).write(b'\r'); }
	}
	unsafe { Port::<u8>::new(COM1).write(b); }
}

pub fn read_byte() -> Option<u8> {
	if received() {
		Some(unsafe { Port::<u8>::new(COM1).read() })
	} else {
		None
	}
}

pub fn write_str(s: &str) {
	for b in s.bytes() {
		if b == b'\n' { write_byte(b'\r'); }
		write_byte(b);
	}
}

pub struct SerialWriter;

impl SerialWriter {
	#[inline]
	pub fn write_byte(&mut self, byte: u8) {
		while !is_transmit_empty() {}
		unsafe { Port::<u8>::new(COM1).write(byte); }
	}
}

impl Write for SerialWriter {
	fn write_str(&mut self, s: &str) -> fmt::Result {
		for byte in s.bytes() {
			if byte == b'\n' {
				self.write_byte(b'\r');
			}
			self.write_byte(byte);
		}
		Ok(())
	}
}

pub static SERIAL: Mutex<SerialWriter> = Mutex::new(SerialWriter);

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
	use fmt::Write;
	x86_64::instructions::interrupts::without_interrupts(|| {
		SERIAL.lock().write_fmt(args).unwrap();
	});
}


#[macro_export]
macro_rules! sprint {
    ($($arg:tt)*) => {
        $crate::drivers::serial::_print(format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! sprintln {
    () => ($crate::sprint!("\n"));
    ($($arg:tt)*) => {
        $crate::sprint!("{}\n", format_args!($($arg)*))
    };
}