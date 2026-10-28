
use spin::Mutex;

use crate::{drivers::block::{ahci::AhciDevice, atapio::AtaPioDevice}, kerror, klog};

pub mod ahci;
pub mod nvme;
pub mod atapio;

pub static DEV_MGR: Mutex<DeviceManager> = Mutex::new(DeviceManager::new());

pub trait BlockDevice: Sync + Send  {
	fn block_size(&self) -> u32;
	fn block_count(&self) -> u64;
	fn read_block(&mut self, lba: u64, count: u32, buf: &mut [u8]) -> Result<(), &'static str>;
	fn write_block(&mut self, lba: u64, count: u32, buf: &[u8]) -> Result<(), &'static str>;
}

pub struct RegisteredDevice {
	pub name: [u8; 16],
	pub device: &'static mut dyn BlockDevice,
}

pub struct DeviceManager {
	devices: [Option<RegisteredDevice>; 16],
	count: usize,
}


impl DeviceManager {
	pub const fn new() -> Self {
		Self {
			devices: [const { None }; 16],
			count: 0,
		}
	}

	pub fn registered_count(&self) -> usize {
		self.count
	}

	pub fn register(&mut self, name: [u8; 16], device: &'static mut dyn BlockDevice) -> Result<(), &'static str> {
		if self.count >= 16 {
			return Err("Device registry full");
		}
		self.devices[self.count] = Some(RegisteredDevice { name, device });
		self.count += 1;
		Ok(())
	}

	pub fn find(&mut self, name: &[u8]) -> Option<&mut dyn BlockDevice> {

		for slot in &mut self.devices {
			if let Some(dev) = slot {
				if dev.name.starts_with(name){
					return Some(dev.device);
				}
			}
		}

		None
	}

	pub fn list_drive_names(&self) {
		
	}
}


pub fn init() {
	let mut reg = DEV_MGR.lock();
	match nvme::probe(&mut reg) {
		Ok(added) => {
			klog!("NVMe Drivers init done, found {added} drives")
		},
		Err(err) => {
			kerror!("NVMe drivers init failed: {}", err)
		},
	}
	drop(reg);
	klog!("Block devices driver init done")
}

fn _ahci_probe() -> Option<AhciDevice> { None }
fn _ata_pio_probe() -> Option<AtaPioDevice> { None }
