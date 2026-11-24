use x86_64::instructions::port::Port;

use crate::time::rtc::RtcBus;

pub struct CmosRtc {
	index_port: Port<u8>,
	data_port: Port<u8>,
}

impl CmosRtc {
	pub const fn new() -> Self {
		Self {
			index_port: Port::new(0x70),
			data_port: Port::new(0x71),
		}
	}
}

impl RtcBus for CmosRtc {
	fn read_reg(&mut self, reg: u8) -> u8 {
		unsafe {
			self.index_port.write(reg);
			self.data_port.read()
		}
	}
}
