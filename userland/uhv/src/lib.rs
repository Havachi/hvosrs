#![no_std]
#![no_main]

pub mod heap;
mod syscalls;
mod files;
use core::time::Duration;

use alloc::boxed::Box;
pub use files::*;
pub use syscalls::*;

use crate::time::{DateTime, UnixTime};

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
		-3 => "No such process",
		-4 => "Interrupted system call",
		-5 => "Input/Output error",
		-9 => "Bad file descriptor",
		-10 => "No child processes",
		-11 => "Resource temporarily unavailable",
		-12 => "Cannot allocate memory",
		-13 => "Permission Denied",
		-14 => "Bad address",
		-15 => "Block device required",
		-16 => "Ressource or device busy",
		-17 => "File Exist",
		-19 => "No such device",
		-20 => "Not a directory",
		-21 => "Is a directory",
		-22 => "Invalid argument",
		-27 => "File too large",
		-28 => "No space left on device",
		-38 => "Function not implemented",
		-62 => "Timer expired",
		-110 => "Connection timed out",
		_ => "unknown error"
	}
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SysTime {
	pub millis: u64
}

pub fn now() {
	let mut time = SysTime { ..Default::default() }; 
	sys_clock_gettime(0, &mut time);
	let unix = UnixTime::from(time) + Duration::from_secs_f32(2.0);
	let dt = DateTime::from_unixtime(unix);
	println!("{}",dt)
}

impl From<SysTime> for UnixTime {
	fn from(value: SysTime) -> Self {
		Self { ms: value.millis }
	}
}

mod time {
    use core::{fmt::Display, ops::{Add, Sub}};

	#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
	pub struct YMD {
		year: i64,
		month: u8,
		day: u8,
	}

	#[repr(u8)]
	#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
	pub enum Weekday {
		Monday		= 0,
		Tuesday		= 1,
		Wednesday	= 2,
		Thursday	= 3,
		Friday		= 4,
		Saturday	= 5,
		Sunday		= 6,
	}

	impl From<u8> for Weekday {
		fn from(value: u8) -> Self {
			match value % 7 {
				0 => Self::Monday,
				1 => Self::Tuesday,
				2 => Self::Wednesday,
				3 => Self::Thursday,
				4 => Self::Friday,
				5 => Self::Saturday,
				_ => Self::Sunday,
			}
		}
	}

	#[repr(u8)]
	#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
	pub enum Month {
		January=1, February, March, April,
		May, June, July, August,
		September, October, November, December
	}

	#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
	pub struct LocalDate {
		ymd: YMD,
		yearday: i16,
		weekday: Weekday
	}

	impl LocalDate {
		pub fn from_unixtime(unixtime: UnixTime) -> Self {
			let z_orig: u64 = unixtime.ms.div_euclid(MILLI_PER_DAY);
			let weekday = Weekday::from((z_orig + 4).rem_euclid(7) as u8);
			let z = z_orig + 719468;
			let era = z.div_euclid(146097);
			let doe = z - era * 146097;
			let yoe = (doe - doe.div_euclid(1460) + doe.div_euclid(36524) - doe.div_euclid(146096))
				.div_euclid(365);
			let y = yoe + era * 400;
			let doy = doe - (365 * yoe + yoe.div_euclid(4) - yoe.div_euclid(100));
			let mp = (5 * doy + 2).div_euclid(153);
			let d = doy - (153 * mp + 2).div_euclid(5) + 1;
			let m = if mp < 10 { mp + 3 } else { mp - 9 };
			let year = if m <= 2 { y + 1 } else { y };

			let is_leap = ((year % 4 == 0) && (year % 100 != 0 || year % 400 == 0)) as u64;
			let yearday = if m <= 2 {
				doy - 305
			} else {
				doy + 60 + is_leap
			};

			Self {
				ymd: YMD {
					year: year as i64,
					month: m as u8,
					day: d as u8,
				},
				weekday,
				yearday: yearday as i16,
			}
		}
	}

	#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
	pub struct LocalTime {
		hour: i8,
		minute: i8,
		second: i8,
		millisecond: i16,
	}

	impl LocalTime {
		pub fn from_unixtime(unixtime: UnixTime) -> Self {
			let total_sec = unixtime.ms.div_euclid(MILLI_PER_SEC);
			let day_sec = total_sec.rem_euclid(SEC_PER_DAY);

			Self {
				hour: day_sec.div_euclid(SEC_PER_HOURS) as i8,
				minute: day_sec.rem_euclid(SEC_PER_HOURS).div_euclid(SEC_PER_MIN) as i8,
				second: day_sec.rem_euclid(SEC_PER_MIN) as i8,
				millisecond: unixtime.ms.rem_euclid(MILLI_PER_SEC) as i16,
			}
		}
	}

	#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
	pub struct DateTime {
		date: LocalDate,
		time: LocalTime,
	}

	impl DateTime {
		pub fn from_unixtime(unixtime: UnixTime) -> Self {
			
			Self {
				date: LocalDate::from_unixtime(unixtime),
				time: LocalTime::from_unixtime(unixtime),
			}

		}
	}

	#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
	pub struct UnixTime {
		pub ms: u64,
	}

	impl Add<core::time::Duration> for UnixTime {
		type Output = UnixTime;
		fn add(mut self, rhs: core::time::Duration) -> Self::Output {
			self.ms = self.ms.saturating_add(rhs.as_millis() as u64);
			self
		}
	}

	impl Sub<core::time::Duration> for UnixTime {
		type Output = UnixTime;
		fn sub(mut self, rhs: core::time::Duration) -> Self::Output {
			self.ms = self.ms.saturating_sub(rhs.as_millis() as u64);
			self
		}
	}
	
	impl Display for YMD {
		fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
			write!(f, "{:02}/{:02}/{:02}", self.day, self.month, self.year)
		}
	}

	impl Display for LocalTime {
		fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
			write!(f, "{:02}:{:02}:{:02}.{:03}", self.hour, self.minute, self.second, self.millisecond)
		}
	}
	impl Display for DateTime {
		fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
			write!(f, "{:?} {} {}", self.date.weekday, self.date.ymd, self.time)
		}
	}

	pub const MILLI_PER_SEC: u64 = 1000;
	pub const SEC_PER_MIN: u64 = 60;
	pub const SEC_PER_HOURS: u64 = SEC_PER_MIN * SEC_PER_MIN;
	pub const SEC_PER_DAY: u64 = SEC_PER_HOURS * 24;
	pub const SEC_PER_WEEK: u64 = SEC_PER_DAY * 7;
	pub const MILLI_PER_DAY: u64 = 24 * 60 * 60 * 1000;
}