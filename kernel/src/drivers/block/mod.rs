
use alloc::{format, sync::Arc};
use spin::Mutex;

use crate::{drivers::block::{ahci::AhciDevice, atapio::AtaPioDevice}, errno::KernelErrors, fs::ext2::inode::{Ext2InodeMode}, kerror, klog, vfs::{self, inode::DeviceId}};

pub mod ahci;
pub mod nvme;
pub mod atapio;

pub static DEV_MGR: Mutex<DeviceManager> = Mutex::new(DeviceManager::new());

pub trait BlockDevice: Sync + Send  {
	fn block_size(&self) -> u32;
	fn block_count(&self) -> u64;
	fn read_block(&self, lba: u64, count: u32, buf: &mut [u8]) -> Result<(), KernelErrors>;
	fn write_block(&self, lba: u64, count: u32, buf: &[u8]) -> Result<(), KernelErrors>;
}

pub struct RegisteredDevice {
	pub dev_id: DeviceId,
	pub name: [u8; 16],
	pub device: Arc<dyn BlockDevice>,
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

	pub fn register(&mut self,id: DeviceId, name: [u8; 16], device: Arc<dyn BlockDevice>) -> Result<(), KernelErrors> {
		if self.count >= 16 {
			return Err(KernelErrors::ENOMEM);
		}

		self.devices[self.count] = Some(RegisteredDevice {dev_id: id, name, device });
		self.count += 1;
		Ok(())
	}
	
	//To be run after VFS is inited
	pub fn register_in_vfs(&self) -> Result<(), KernelErrors> {
		for device in self.devices.as_ref() {
			if let Some(dev) = device {
				let end = dev.name.iter().position(|&b| b == 0).unwrap_or(dev.name.len());

				if let Ok(dev_name) = str::from_utf8(&dev.name[..end]) {
					if dev_name.is_empty() {
						continue;
					}
					let path = format!("/dev/{}", dev_name);
					let mode = Ext2InodeMode::BlkDev | Ext2InodeMode::from_bits_truncate(0o660);
					vfs::mkdev(path.as_str(), mode.bits() as u16, dev.dev_id)?;
				}
			}
		}
		Ok(())
	}

	pub fn find_by_name(&mut self, name: &[u8]) -> Option<Arc<dyn BlockDevice>> {

		for slot in &mut self.devices {
			if let Some(dev) = slot {
				if dev.name.starts_with(name){
					return Some(dev.device.clone());
				}
			}
		}

		None
	}

	pub fn find_by_id(&self, id: DeviceId) -> Option<Arc<dyn BlockDevice>> {
		for slot in &self.devices {
			if let Some(dev) = slot {
				if dev.dev_id == id {
					return Some(dev.device.clone());
				}
			}
		}

		None
	}


}

impl IntoIterator for DeviceManager {
	type Item = Option<RegisteredDevice>;
	type IntoIter = core::array::IntoIter<Option<RegisteredDevice>, 16>;

	fn into_iter(self) -> Self::IntoIter {
		self.devices.into_iter()
	}
}
impl<'a> IntoIterator for &'a DeviceManager {
    type Item = &'a Option<RegisteredDevice>;
    type IntoIter = core::slice::Iter<'a, Option<RegisteredDevice>>;
	
	fn into_iter(self) -> Self::IntoIter {
		self.devices.iter()
	}
}
impl<'a> IntoIterator for &'a mut DeviceManager {
    type Item = &'a mut Option<RegisteredDevice>;
    type IntoIter = core::slice::IterMut<'a, Option<RegisteredDevice>>;
	
	fn into_iter(self) -> Self::IntoIter {
		self.devices.iter_mut()
	}
}

pub fn init() {
	let mut reg = DEV_MGR.lock();
	match nvme::probe(&mut reg) {
		Ok(added) => {
			klog!("NVMe Drivers init done, found {added} drives")
		},
		Err(err) => {
			kerror!("NVMe drivers init failed: {:?}", err)
		},
	}
	drop(reg);
	klog!("Block devices driver init done")
}

fn _ahci_probe() -> Option<AhciDevice> { None }
fn _ata_pio_probe() -> Option<AtaPioDevice> { None }
