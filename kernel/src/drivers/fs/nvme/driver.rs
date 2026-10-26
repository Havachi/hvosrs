use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};

use x86_64::{PhysAddr, VirtAddr};

use crate::{
	arch::x86_64::{asm::{mmio_read, mmio_write}, idt::NVME_INT_VECTOR},
	drivers::pci::{configure_msix_entry, scan_bus, PciDevice},
	kerror, ksprint,
	memory::{mmio::{map_mmio_region, mmio_alloc}, PAGE_SIZE, vmm::phys_to_virt},
};

use super::{
	namespace::NvmeNamespace,
	protocol::{NvmeCommand, NvmeIdCtrl},
	queues::{CompletionQueue, SubmissionQueue},
	registers::*,
};

pub struct MsixState {
	pub table_virt: VirtAddr,
	pub entry_index: usize,
}

pub struct NvmeDriver {
	mmio_base: VirtAddr,
	doorbell_stride: u32,
	timeout_unit: u8,
	cntlid: u16,
	admin_sq: SubmissionQueue,
	admin_cq: CompletionQueue,
	io_sq: SubmissionQueue,
	io_cq: CompletionQueue,
	msix: Option<MsixState>,
	io_completion_pending: AtomicBool,
	next_io_cid: u16,
}

impl NvmeDriver {
	pub fn discover() -> Result<Self, &'static str> {
		let mut target = None;
		scan_bus(|dev| {
			if dev.common_header.class == 0x01
				&& dev.common_header.subclass == 0x08
				&& dev.common_header.prog_if == 0x02
			{
				target = Some(dev);
			}
		});
		let mut pci = target.ok_or("No NVMe PCI device found")?;
		let msix = Self::setup_msix(&mut pci)?;

		if let Some(bar0) = pci.read_bar(0) {
			if bar0 & 0b1111 != 0b0100 {
				kerror!("NVMe PCI Bar is in an invalid state");
				return Err("Invalid NVMe BAR");
			}
		}

		let mmio_size = pci
			.find_mmio_region_size(true, 0)
			.ok_or("Failed to size NVMe MMIO")?;
		let b0 = pci.read_bar(0).ok_or("Failed to read BAR0")?;
		let b1 = pci.read_bar(1).ok_or("Failed to read BAR1")?;
		let phys_addr = ((b1 as u64) << 32) | ((b0 as u64) & 0xFFFFFFF0);
		let mmio_base = map_mmio_region(phys_addr, mmio_size as usize)
			.ok_or("Failed to map NVMe MMIO")?;

		let cap_raw: u64 = unsafe { mmio_read(mmio_base.as_ptr(), REG_CAP) };
		let dstrd = ((cap_raw >> 32) & 0xf) as u32;
		let doorbell_stride = 4 * (1 << dstrd);

		Ok(Self {
			mmio_base,
			doorbell_stride,
			timeout_unit: 0,
			cntlid: 0,
			admin_sq: SubmissionQueue::new(VirtAddr::zero(), 0),
			admin_cq: CompletionQueue::new(VirtAddr::zero(), 0),
			io_sq: SubmissionQueue::new(VirtAddr::zero(), 0),
			io_cq: CompletionQueue::new(VirtAddr::zero(), 0),
			msix,
			io_completion_pending: AtomicBool::new(false),
			next_io_cid: 1,
		})
	}

	fn setup_msix(pci: &mut PciDevice) -> Result<Option<MsixState>, &'static str> {
		let msix_offset = match pci.find_msix_offset() {
			Ok(off) => off,
			Err(_) => return Ok(None),
		};
		ksprint!("NVMe controller has msi-x cap. offset={:#x}\n", msix_offset);
		let msix_cfg = pci.enable_and_parse_msix(msix_offset);
		ksprint!(
			"MSI-X Config: bar:{:#x},tbl_off:{:#x},tblsz={}\n",
			msix_cfg.bar_index,
			msix_cfg.table_byte_offset,
			msix_cfg.table_size
		);

		let bar_idx = msix_cfg.bar_index;
		let bar_lo = pci.read_bar(bar_idx).ok_or("MSI-X BAR read failed")?;
		let bar_phys = (bar_lo & 0xFFFFFFF0) as u64;
		let table_phys = bar_phys + msix_cfg.table_byte_offset as u64;
		let table_virt = map_mmio_region(table_phys, PAGE_SIZE)
			.ok_or("Failed to map MSI-X table")?;

		unsafe {
			configure_msix_entry(table_virt, 0, NVME_INT_VECTOR, 0);
		}

		Ok(Some(MsixState {
			table_virt,
			entry_index: 0,
		}))
	}

	pub unsafe fn init(&mut self) -> Result<(), &'static str> {
		unsafe { self.reset()? };

		let cap_raw = self.read_cap();
		let mqes = cap_raw & 0xFFFF;
		self.timeout_unit = ((cap_raw >> 24) & 0xFF) as u8;

		let n_asq = PAGE_SIZE as u32 / 64;
		let n_acq = PAGE_SIZE as u32 / 16;

		if mqes < (n_acq + n_asq) as u64 {
			return Err("MQES to small");
		}

		let aqa_val = (((n_acq - 1) & 0xFFF) << 16) | ((n_asq - 1) & 0xFFF);
		self.write_aqa(aqa_val);

		let (asq_phys, asq_virt) = mmio_alloc(1).ok_or("Failed to allocated memory for ASQ")?;
		let (acq_phys, acq_virt) = mmio_alloc(1).ok_or("Failed to allocated memory for ACQ")?;
		let (iosq_phys, iosq_virt) = mmio_alloc(1).ok_or("Failed to allocated memory for IOSQ")?;
		let (iocq_phys, iocq_virt) = mmio_alloc(1).ok_or("Failed to allocated memory for IOCQ")?;

		self.admin_sq = SubmissionQueue::new(asq_virt, n_asq);
		self.admin_cq = CompletionQueue::new(acq_virt, n_acq);
		self.io_sq = SubmissionQueue::new(iosq_virt, n_asq);
		self.io_cq = CompletionQueue::new(iocq_virt, n_acq);

		self.write_asq(asq_phys.as_u64());
		self.write_acq(acq_phys.as_u64());

		let mut cc = 0u32;
		cc |= 6 << 16;
		cc |= 4 << 20;
		cc &= !1;
		self.write_cc(cc);
		self.enable();
		self.poll_ready(self.timeout_unit as u32)?;

		let io_iv = 0u16;
		self.create_io_completion_queue(1, iocq_phys.as_u64(), n_acq as u64, io_iv)?;
		if self.msix.is_some() {
			unsafe { mmio_write(self.mmio_base.as_mut_ptr(), REG_INTMC, 1) };
		}
		self.create_io_submission_queue(1, 1, iosq_phys.as_u64(), n_asq as u64)?;

		Ok(())
	}

	pub fn cntlid(&self) -> u16 {
		self.cntlid
	}

	pub unsafe fn identify_controller(&mut self) -> Result<(), &'static str> {
		let (id_buf_phys, id_buf_virt) =
			mmio_alloc(1).ok_or("Failed to allocate memory for identify response buffer")?;
		let cmd = NvmeCommand::new_identify(0, 1, id_buf_phys.as_u64());
		self.submit_admin_cmd(cmd)?;
		let id = NvmeIdCtrl::from_virt(id_buf_virt);
		self.cntlid = id.cntlid;
		Ok(())
	}

	pub fn get_active_namespace(&mut self, list_buf_phys: PhysAddr) -> Result<Vec<u32>, &'static str> {
		let mut cmd = NvmeCommand::zero();
		cmd.opcode = 0x06;
		cmd.nsid = 0;
		cmd.cdw10 = 0x02;
		cmd.prp1 = list_buf_phys.as_u64();
		self.submit_admin_cmd(cmd)?;

		let nsids_ptr = phys_to_virt(list_buf_phys).as_ptr() as *const u32;
		let mut active_nsid = Vec::new();
		for i in 0..1024 {
			let nsid = unsafe { core::ptr::read_volatile(nsids_ptr.add(i)) };
			if nsid == 0 {
				break;
			}
			active_nsid.push(nsid);
		}
		Ok(active_nsid)
	}

	pub fn identify_namespace(&mut self, nsid: u32, ns_buf_phys: PhysAddr) -> Result<(), &'static str> {
		let mut cmd = NvmeCommand::zero();
		cmd.opcode = 0x06;
		cmd.nsid = nsid;
		cmd.cdw10 = 0x0;
		cmd.prp1 = ns_buf_phys.as_u64();
		self.submit_admin_cmd(cmd)
	}

	pub fn namespace_from_identify(
		shared: alloc::sync::Arc<spin::Mutex<NvmeDriver>>,
		ns_buf_virt: VirtAddr,
		nsid: u32,
	) -> NvmeNamespace {
		let id = super::protocol::NvmeIdNs::from_virt(ns_buf_virt);
		let flbas_idx = (id.flbas & 0x0F) as usize;
		let lbaf_val = id.lbaf[flbas_idx];
		let sector_shift = ((lbaf_val >> 16) & 0xFF) as u32;
		let sector_shift = if sector_shift == 0 { 9 } else { sector_shift };
		let sector_size = 1u64 << sector_shift;
		NvmeNamespace::new(nsid, sector_size as u32, id.nsze, shared)
	}

	pub fn submit_io_read(&mut self, nsid: u32, lba: u64, count: u32, buf_phys: u64) -> Result<(), &'static str> {
		self.submit_io_nvm(0x02, nsid, lba, count, buf_phys)
	}

	pub fn submit_io_write(&mut self, nsid: u32, lba: u64, count: u32, buf_phys: u64) -> Result<(), &'static str> {
		self.submit_io_nvm(0x01, nsid, lba, count, buf_phys)
	}

	fn submit_io_nvm(
		&mut self,
		opcode: u8,
		nsid: u32,
		lba: u64,
		count: u32,
		buf_phys: u64,
	) -> Result<(), &'static str> {
		if count == 0 {
			return Err("NVMe I/O requires at least one block");
		}
		let mut cmd = NvmeCommand::zero();
		cmd.opcode = opcode;
		cmd.cid = self.next_io_cid;
		self.next_io_cid = self.next_io_cid.wrapping_add(1);
		cmd.nsid = nsid;
		cmd.prp1 = buf_phys;
		cmd.cdw10 = lba as u32;
		cmd.cdw11 = (lba >> 32) as u32;
		cmd.cdw12 = count - 1;
		self.submit_io_cmd(cmd)?;
		self.wait_io_completion()
	}

	pub fn service_io_completions(&mut self) -> Result<(), &'static str> {
		let n = self.io_cq.drain_available()?;
		if n > 0 {
			self.ring_cq_doorbell(1);
			self.io_completion_pending.store(true, Ordering::Release);
		}
		Ok(())
	}

	fn wait_io_completion(&mut self) -> Result<(), &'static str> {
		loop {
			if self.io_completion_pending.swap(false, Ordering::AcqRel) {
				return Ok(());
			}
			if self.io_cq.try_complete_one()? {
				self.ring_cq_doorbell(1);
				return Ok(());
			}
			core::hint::spin_loop();
		}
	}

	unsafe fn reset(&mut self) -> Result<(), &'static str> {
		if self.is_enable() {
			self.disable();
		}
		Ok(())
	}

	pub fn poll_ready(&mut self, timeout_units: u32) -> Result<(), &'static str> {
		let mut timeout = if timeout_units > 0 { timeout_units } else { 4 } * 1000;
		while unsafe { (mmio_read::<u32>(self.mmio_base.as_ptr(), REG_CSTS) & 1) == 0 } && timeout > 0 {
			let csts = unsafe { mmio_read::<u32>(self.mmio_base.as_ptr(), REG_CSTS) };
			if (csts & 0x2) != 0 {
				kerror!("NVMe fatal error\n");
				return Err("NVMe fatal error");
			}
			timeout -= 1;
			for _ in 0..10000 {
				core::hint::spin_loop();
			}
		}
		if timeout == 0 {
			ksprint!("Timeout\n");
			return Err("Timeout");
		}
		Ok(())
	}

	pub fn submit_admin_cmd(&mut self, cmd: NvmeCommand) -> Result<(), &'static str> {
		let new_tail = self.admin_sq.submit(cmd);
		self.ring_sq_doorbell(0, new_tail);
		loop {
			self.check_fatal()?;
			if self.admin_cq.try_complete_one()? {
				break;
			}
			core::hint::spin_loop();
		}
		self.ring_cq_doorbell(0);
		Ok(())
	}

	pub fn submit_io_cmd(&mut self, cmd: NvmeCommand) -> Result<(), &'static str> {
		self.io_completion_pending.store(false, Ordering::Release);
		let new_tail = self.io_sq.submit(cmd);
		self.ring_sq_doorbell(1, new_tail);
		Ok(())
	}

	fn check_fatal(&self) -> Result<(), &'static str> {
		if self.read_csts() & 2 != 0 {
			return Err("NVMe controller fatal error");
		}
		Ok(())
	}

	fn ring_sq_doorbell(&self, q_id: u32, tail: u32) {
		let offset = self.sq_doorbell_offset(q_id) as usize;
		unsafe { mmio_write(self.mmio_base.as_mut_ptr(), offset, tail) };
	}

	fn ring_cq_doorbell(&self, q_id: u32) {
		let head = if q_id == 0 {
			self.admin_cq.head
		} else {
			self.io_cq.head
		};
		let offset = self.cq_doorbell_offset(q_id) as usize;
		unsafe { mmio_write(self.mmio_base.as_mut_ptr(), offset, head) };
	}

	fn create_io_completion_queue(
		&mut self,
		qid: u16,
		cq_phys: u64,
		entry_count: u64,
		vector: u16,
	) -> Result<(), &'static str> {
		let mut cmd = NvmeCommand::zero();
		cmd.opcode = 0x05;
		cmd.prp1 = cq_phys;
		cmd.cdw10 = (((entry_count - 1) as u32) << 16) | (qid as u32);
		let mut cdw11 = 1u32;
		if self.msix.is_some() {
			cdw11 |= 1 << 1;
			cdw11 |= (vector as u32) << 16;
		}
		cmd.cdw11 = cdw11;
		ksprint!("qid={qid},cq_phys={cq_phys:#x}\n");
		self.submit_admin_cmd(cmd)
	}

	fn create_io_submission_queue(
		&mut self,
		qid: u16,
		assoc_cqid: u16,
		sq_phys: u64,
		entry_count: u64,
	) -> Result<(), &'static str> {
		let mut cmd = NvmeCommand::zero();
		cmd.opcode = 0x01;
		cmd.prp1 = sq_phys;
		cmd.cdw10 = (((entry_count - 1) as u32) << 16) | (qid as u32);
		cmd.cdw11 = (assoc_cqid as u32) | (1 << 16);
		self.submit_admin_cmd(cmd)
	}

	fn is_enable(&self) -> bool {
		self.read_cc() & 1 == 1
	}

	fn disable(&mut self) {
		unsafe { mmio_write(self.mmio_base.as_mut_ptr(), REG_CC, self.read_cc() & !1) };
	}

	fn enable(&mut self) {
		unsafe { mmio_write(self.mmio_base.as_mut_ptr(), REG_CC, self.read_cc() | 1) };
	}

	fn sq_doorbell_offset(&self, q_id: u32) -> u32 {
		DOORBELL_BASE + (2 * q_id * self.doorbell_stride)
	}

	fn cq_doorbell_offset(&self, q_id: u32) -> u32 {
		DOORBELL_BASE + ((2 * q_id + 1) * self.doorbell_stride)
	}

	fn read_cap(&self) -> u64 {
		unsafe { mmio_read(self.mmio_base.as_ptr(), REG_CAP) }
	}

	fn read_cc(&self) -> u32 {
		unsafe { mmio_read(self.mmio_base.as_ptr(), REG_CC) }
	}

	fn write_cc(&self, value: u32) {
		unsafe { mmio_write(self.mmio_base.as_mut_ptr(), REG_CC, value) };
	}

	fn read_csts(&self) -> u32 {
		unsafe { mmio_read(self.mmio_base.as_ptr(), REG_CSTS) }
	}

	fn write_aqa(&self, value: u32) {
		unsafe { mmio_write(self.mmio_base.as_mut_ptr(), REG_AQA, value) }
	}

	fn write_asq(&self, asq: u64) {
		unsafe { mmio_write(self.mmio_base.as_mut_ptr(), REG_ASQ, asq) }
	}

	fn write_acq(&self, acq: u64) {
		unsafe { mmio_write(self.mmio_base.as_mut_ptr(), REG_ACQ, acq) }
	}
}
