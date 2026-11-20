use core::ops::Add;
use x86_64::VirtAddr;

use crate::{errno::KernelErrors, kerror};

use super::protocol::{CompletionQueueEntry, NvmeCommand};

pub struct SubmissionQueue {
	pub virt: VirtAddr,
	pub tail: u32,
	pub size: u32,
}

impl SubmissionQueue {
	pub fn new(virt: VirtAddr, size: u32) -> Self {
		Self {
			virt,
			tail: 0,
			size,
		}
	}

	pub fn submit(&mut self, cmd: NvmeCommand) -> u32 {
		let sq = self.virt.as_mut_ptr() as *mut NvmeCommand;
		let tail = self.tail;
		unsafe {
			sq.add(tail as usize).write_volatile(cmd);
		}
		self.tail = (self.tail + 1) % self.size;
		self.tail
	}
}

pub struct CompletionQueue {
	pub virt: VirtAddr,
	pub head: u32,
	pub size: u32,
	pub phase: bool,
}

impl CompletionQueue {
	pub fn new(virt: VirtAddr, size: u32) -> Self {
		Self {
			virt,
			head: 0,
			size,
			phase: true,
		}
	}

	fn entry_ptr(&self) -> *const CompletionQueueEntry {
		let entry_size = core::mem::size_of::<CompletionQueueEntry>() as u64;
		let offset = self.head as u64 * entry_size;
		self.virt.add(offset).as_ptr() as *const CompletionQueueEntry
	}

	pub fn try_complete_one(&mut self) -> Result<bool, KernelErrors> {
		let cqe = unsafe { core::ptr::read_volatile(self.entry_ptr()) };
		if cqe.phase_bit() != self.phase {
			return Ok(false);
		}
		let sc = cqe.status_code();
		if sc != 0 {
			kerror!("nvme: op failed: sc {sc}");
			return Err(KernelErrors::EIO);
		}
		self.head += 1;
		if self.head >= self.size {
			self.head = 0;
			self.phase = !self.phase;
		}
		Ok(true)
	}

	pub fn wait_one(&mut self, fatal_check: impl Fn() -> Result<(), KernelErrors>) -> Result<(), KernelErrors> {
		loop {
			fatal_check()?;
			if self.try_complete_one()? {
				return Ok(());
			}
			core::hint::spin_loop();
		}
	}

	pub fn drain_available(&mut self) -> Result<u32, KernelErrors> {
		let mut count = 0;
		while self.try_complete_one()? {
			count += 1;
		}
		Ok(count)
	}
}
