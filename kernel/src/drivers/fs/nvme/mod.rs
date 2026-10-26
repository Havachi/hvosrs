pub mod driver;
pub mod namespace;
pub mod protocol;
pub mod queues;
pub mod registers;

use alloc::{boxed::Box, sync::Arc, vec::Vec};
use spin::Mutex;

use crate::{
	drivers::fs::DeviceManager,
	kerror, ksprint,
	memory::mmio::mmio_alloc,
};

pub use driver::NvmeDriver;
pub use protocol::{NvmeCommand, NvmeIdCtrl, NvmeIdNs};

static NVME_DRIVERS: Mutex<Vec<Arc<Mutex<NvmeDriver>>>> = Mutex::new(Vec::new());

pub fn register_driver(driver: Arc<Mutex<NvmeDriver>>) {
	NVME_DRIVERS.lock().push(driver);
}

pub fn interrupt_service() {
	for driver in NVME_DRIVERS.lock().iter() {
		let mut d = driver.lock();
		if let Err(e) = d.service_io_completions() {
			kerror!("NVMe interrupt service error: {}", e);
		}
	}
}

/// Returns the number of block devices registered.
pub fn probe(dev_mgr: &mut DeviceManager) -> Result<usize, &'static str> {
	let before = dev_mgr.registered_count();
	let mut driver = NvmeDriver::discover()?;
	unsafe { driver.init()? };
	unsafe { driver.identify_controller()? };

	let shared = Arc::new(Mutex::new(driver));
	register_driver(shared.clone());

	let (list_buf_phys, _) = mmio_alloc(1).ok_or("Failed to allocate memory")?;
	let list = {
		let mut d = shared.lock();
		d.get_active_namespace(list_buf_phys)?
	};
	ksprint!("Found {} active namespace\n", list.len());

	let cntlid = shared.lock().cntlid();
	for nsid in list {
		let (ns_buf_phys, ns_buf_virt) = mmio_alloc(1).ok_or("Failed to allocate memory")?;
		{
			let mut d = shared.lock();
			d.identify_namespace(nsid, ns_buf_phys)?;
		}
		let ns = NvmeDriver::namespace_from_identify(shared.clone(), ns_buf_virt, nsid);
		let mut buf = [0u8; 16];
		let _ = generate_device_name(cntlid, nsid, &mut buf);
		let dev = Box::leak(Box::new(ns));
		dev_mgr.register(buf, dev)?;
	}

	Ok(dev_mgr.registered_count() - before)
}

pub fn generate_device_name(cntlid: u16, nsid: u32, buf: &mut [u8; 16]) -> &str {
	let mut idx = 0;
	buf[0..4].copy_from_slice(b"nvme");
	idx += 4;
	idx = write_decimal(cntlid as u32, buf, idx);
	buf[idx] = b'n';
	idx += 1;
	idx = write_decimal(nsid, buf, idx);
	core::str::from_utf8(&buf[..idx]).unwrap_or("nvme0n1")
}

fn write_decimal(mut val: u32, buf: &mut [u8; 16], mut idx: usize) -> usize {
	if val == 0 {
		buf[idx] = b'0';
		return idx + 1;
	}
	let mut temp = [0u8; 10];
	let mut t_idx = 0;
	while val > 0 {
		temp[t_idx] = b'0' + (val % 10) as u8;
		val /= 10;
		t_idx += 1;
	}
	while t_idx > 0 {
		t_idx -= 1;
		buf[idx] = temp[t_idx];
		idx += 1;
	}
	idx
}
