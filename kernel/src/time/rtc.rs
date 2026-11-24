use crate::{errno::KernelErrors::{self, EINVAL}, time::{BOOT_EPOCH, EPOCH, MILLI_PER_DAY}};

pub trait RtcBus {
	fn read_reg(&mut self, reg: u8) -> u8;
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct RawRtcFields {
	pub second: u8,
	pub minute: u8,
	pub hour: u8,
	pub day: u8,
	pub month: u8,
	pub year: u8,
	pub status_b: u8,
}

fn read_raw_snapshot<B: RtcBus>(bus: &mut B) -> RawRtcFields {
	while (bus.read_reg(0x0A) & 0x80) != 0 {
		core::hint::spin_loop();
	}

	RawRtcFields {
		second: bus.read_reg(0x00),
		minute: bus.read_reg(0x02),
		hour: bus.read_reg(0x04),
		day: bus.read_reg(0x07),
		month: bus.read_reg(0x08),
		year: bus.read_reg(0x09),
		status_b: bus.read_reg(0x0B)
	}
} 

pub fn read_rtc_stable<B: RtcBus>(bus: &mut B) -> RawRtcFields {
	loop {
		let first = read_raw_snapshot(bus);
		let second = read_raw_snapshot(bus);
		if first == second {
			return second;
		}
	}
}

fn bcd_to_bin(val:u8) -> u8 {
	(val & 0x0F) + ((val >> 4) * 10)
}

pub fn decode_field(raw: u8, is_bcd: bool) -> u8 {
	if is_bcd {
		bcd_to_bin(raw)
	} else {
		raw
	}
}

fn decode_hour(raw_hour: u8, is24h: bool, is_bcd: bool) -> u8 {
	let pm = !is24h && ((raw_hour & 0x80) != 0);
	let mut h = raw_hour & if is24h { 0x3F } else { 0x7F };
	if is_bcd {
		h = bcd_to_bin(h);
	}  
	if !is24h {
		h = if h == 12 { 0 } else { h };
		if pm {
			h += 12;
		}
	}
	h
}

fn ymd_to_days_since_epoch(year: i64, month: u8, day: u8) -> i64 {
	let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400) as u64;
    let m = month as u64;
    let d = day as u64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + (doe as i64) - 719_468
}

pub fn is_leap_year(year: i64) -> bool {
	(year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

pub fn days_in_month(year: i64, month: u8) -> Option<u8> {
	match month {
		1 | 3 | 5 | 7 | 8 | 10 | 12 => Some(31),
		4 | 6 | 9 | 11 => Some(30),
		2 => Some(if is_leap_year(year) { 29 } else { 28 }),
		_ => None,
	}
}
pub fn init_epoch_from_rtc<B: RtcBus>(bus: &mut B, base_century: i64) -> Result<u64, KernelErrors> {
	let raw = read_rtc_stable(bus);
	let is_binary = (raw.status_b & 0x04) != 0;
	let is_24h = (raw.status_b & 0x02) != 0;
	let is_bcd = !is_binary;

	let second = decode_field(raw.second, is_bcd);
    let minute = decode_field(raw.minute, is_bcd);
    let hour = decode_hour(raw.hour, is_24h, is_bcd);
    let day = decode_field(raw.day, is_bcd);
    let month = decode_field(raw.month, is_bcd);
    let year_short = decode_field(raw.year, is_bcd) as i64;
    let year = base_century + year_short;

	if !(1..=12).contains(&month) {
		return Err(KernelErrors::EINVAL);
	}
	let max_day = days_in_month(year, month).unwrap_or(31);
	if day == 0 || day > max_day {
        return Err(KernelErrors::EINVAL);
    }
    if hour > 23 {
        return Err(KernelErrors::EINVAL);
    }
    if minute > 59 {
        return Err(KernelErrors::EINVAL);
    }
    if second > 59 {
       return Err(KernelErrors::EINVAL);
    }

	let days = ymd_to_days_since_epoch(year, month, day);
	let ms_of_day = ((hour as i64) * 3600 + (minute as i64) * 60 + (second as i64)) * 1000;
	let epoch_ms = days * (MILLI_PER_DAY as i64) + ms_of_day;

	if epoch_ms < 0 {
		return Err(EINVAL);
	}

	let epoch64 = epoch_ms as u64;
	EPOCH.store(epoch64, core::sync::atomic::Ordering::Relaxed);
	BOOT_EPOCH.store(epoch64, core::sync::atomic::Ordering::Relaxed);
	Ok(epoch64)
}
