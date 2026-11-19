use x86_64::{PhysAddr, VirtAddr};

use crate::memory::{HHDM_OFFSET, virt_to_phys_dyn};


pub fn phys_to_virt(phys: PhysAddr) -> VirtAddr {
	let offset = HHDM_OFFSET.lock().expect("HHDM_OFFSET not set");
	let virt_addr = phys.as_u64() + offset;
	VirtAddr::new(virt_addr)
}

pub fn virt_to_phys(virt: VirtAddr) -> PhysAddr {
	virt_to_phys_dyn(virt).unwrap_or(PhysAddr::zero())
}