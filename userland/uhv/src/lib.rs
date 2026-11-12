#![no_std]
#![no_main]

pub mod heap;
mod syscalls;

pub use syscalls::*;

extern crate alloc;
pub const O_CREAT: u64 = 0o100;
pub const O_RDWR: u64 = 0o2;
pub const SEEK_SET: i32 = 0;


pub fn print(str: &str) {
	write_str(1, str);
}

pub fn getchar() -> u8 {
	let mut b = [0u8];
	read(0, &mut b);
	b[0]
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
	eprintln!("userland panic\n");
	exit(1);
	loop {}
}

pub struct Stdout;

impl core::fmt::Write for Stdout {
	fn write_str(&mut self, s: &str) -> core::fmt::Result {
		write(1, s.as_bytes());
		Ok(())
	}
}

pub struct Stderr;

impl core::fmt::Write for Stderr {
	fn write_str(&mut self, s: &str) -> core::fmt::Result {
		write(2, s.as_bytes());
		Ok(())
	}
}

pub fn _print(args: core::fmt::Arguments) {
	use core::fmt::Write;
	let _ = Stdout.write_fmt(args);
}

pub fn _eprint(args: core::fmt::Arguments) {
	use core::fmt::Write;
	let _ = Stderr.write_fmt(args);
}

#[macro_export]
macro_rules! print {
	($($arg:tt)*) => { $crate::_print(format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! println {
	() => { $crate::print!("\n") };
	($($arg:tt)*) => { $crate::print!("{}\n", format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! eprint {
	($($arg:tt)*) => { $crate::_eprint(format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! eprintln {
	() => { $crate::eprint!("\n") };
	($($arg:tt)*) => { $crate::eprint!("{}\n", format_args!($($arg)*)) };
}

pub fn errno_to_str(err: i64) -> &'static str {
	match err {
		-1 => "Permission Denied",
		-2 => "No such file or directory",
		-9 => "Bad file descriptor",
		-10 => "No child processes",
		-11 => "Resource temporarily unavailable",
		-12 => "Cannot allocate memory",
		-13 => "Permission Denied",
		-14 => "Bad address",
		-16 => "Ressource or device busy",
		-17 => "File Exist",
		-20 => "Not a directory",
		-21 => "Is a directory",
		-22 => "Invalid argument",
		_ => "unknown error"
	}
}

