pub mod rtc;

use core::{ fmt::Display, ops::{Add, Sub}, sync::atomic::AtomicU64};

use crate::{cmos::CmosRtc, kprint, time::rtc::init_epoch_from_rtc};


pub const MILLI_PER_SEC: u64 = 1000;
pub const SEC_PER_MIN: u64 = 60;
pub const SEC_PER_HOURS: u64 = SEC_PER_MIN * SEC_PER_MIN;
pub const SEC_PER_DAY: u64 = SEC_PER_HOURS * 24;
pub const SEC_PER_WEEK: u64 = SEC_PER_DAY * 7;
pub const MILLI_PER_DAY: u64 = 24 * 60 * 60 * 1000;

///Seconds per normal years
pub const SEC_PER_YEAR_N: u64 = SEC_PER_DAY * 365;
///Seconds per leap years
pub const SEC_PER_YEAR_L: u64 = SEC_PER_DAY * 366;

pub static EPOCH: AtomicU64 = AtomicU64::new(0);
pub static BOOT_EPOCH: AtomicU64 = AtomicU64::new(0);
pub static PIT_ACC: AtomicU64 = AtomicU64::new(0);
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

	pub fn to_unixtime(&self) -> UnixTime {
		let y = self.date.ymd.year;
		let yd = (self.date.yearday as i64) - 1;
		let prev_y = y - 1;
		let leap_days = prev_y.div_euclid(4) - prev_y.div_euclid(100) + prev_y.div_euclid(400);

		let days_since_epoch = y * 365 + leap_days + yd - 719_527;
		let total_seconds = days_since_epoch * 86_400
			+ (self.time.hour as i64) * 3_600 
			+ (self.time.minute as i64) * 60 
			+ (self.time.second as i64);
		
		UnixTime { ms: (total_seconds * 1000 + (self.time.millisecond as i64)) as u64  }
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

pub fn init() {
	let mut cmosrtc = CmosRtc::new();

	init_epoch_from_rtc(&mut cmosrtc, 2000).unwrap_or(0);
	//let boot_dt = DateTime::from_unixtime(UnixTime { ms: boot_ms });
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

pub fn tick_clock(ms: u64) {
	EPOCH.fetch_add(ms, core::sync::atomic::Ordering::Relaxed);
}

pub fn current_time() -> DateTime {
	let ms = EPOCH.load(core::sync::atomic::Ordering::Relaxed);
	DateTime::from_unixtime(UnixTime { ms })
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SysTime {
	pub millis: u64
}


pub fn tick_from_pit_hz(hz: u64) {
	let prev = PIT_ACC.load(core::sync::atomic::Ordering::Relaxed);
	let total = prev + 1000;
	let whole_ms = total / hz;
	let rem = total % hz;
	PIT_ACC.store(rem, core::sync::atomic::Ordering::Relaxed);
	if whole_ms > 0 {
		EPOCH.fetch_add(whole_ms, core::sync::atomic::Ordering::Relaxed);
	}
}