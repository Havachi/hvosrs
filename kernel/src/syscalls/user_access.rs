use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

use x86_64::{
	PhysAddr, VirtAddr,
	structures::paging::{PageTable, PageTableFlags, PhysFrame},
};

use crate::{
	arch::x86_64::paging::{KERNEL_PML4, switch_pml4},
	memory::{PAGE_SIZE, USER_RANGE, vmm::phys_to_virt},
	syscalls::errno::EFAULT,
};

static SYSCALL_USER_CR3: AtomicU64 = AtomicU64::new(0);

pub fn set_user_cr3(cr3: u64) {
	SYSCALL_USER_CR3.store(cr3, Ordering::Release);
}

pub fn clear_user_cr3() {
	SYSCALL_USER_CR3.store(0, Ordering::Release);
}

#[allow(dead_code)]
pub fn active_user_cr3() -> u64 {
	SYSCALL_USER_CR3.load(Ordering::Acquire)
}

pub fn enter_kernel_page_table(user_cr3: u64) {
	let kernel_cr3 = KERNEL_PML4.load(Ordering::Acquire);
	if kernel_cr3 != 0 && user_cr3 != kernel_cr3 {
		unsafe {
			switch_pml4(PhysFrame::containing_address(PhysAddr::new(kernel_cr3)));
		}
	}
}

pub fn leave_kernel_page_table(user_cr3: u64) {
	let kernel_cr3 = KERNEL_PML4.load(Ordering::Acquire);
	if kernel_cr3 != 0 && user_cr3 != kernel_cr3 {
		unsafe {
			switch_pml4(PhysFrame::containing_address(PhysAddr::new(user_cr3)));
		}
	}
}

fn user_range_ok(ptr: u64, len: usize) -> bool {
	if len == 0 {
		return true;
	}
	let last = ptr + (len as u64) - 1;
	USER_RANGE.contains(ptr) && USER_RANGE.contains(last)
}

fn table_pte_ok(flags: PageTableFlags) -> bool {
	flags.contains(PageTableFlags::PRESENT) && !flags.contains(PageTableFlags::HUGE_PAGE)
}

fn leaf_pte_ok(flags: PageTableFlags) -> bool {
	table_pte_ok(flags) && flags.contains(PageTableFlags::USER_ACCESSIBLE)
}

/// Walk the user page tables (via HHDM) and return a kernel pointer to `virt`.
#[inline(never)]
fn user_host_ptr(user_cr3: u64, virt: VirtAddr) -> Result<*const u8, i64> {
	let v = virt.as_u64();
	let page_off = (v & (PAGE_SIZE as u64 - 1)) as usize;
	let pml4_idx = ((v >> 39) & 0x1FF) as usize;
	let pdpt_idx = ((v >> 30) & 0x1FF) as usize;
	let pd_idx = ((v >> 21) & 0x1FF) as usize;
	let pt_idx = ((v >> 12) & 0x1FF) as usize;

	let pml4 = unsafe { &*phys_to_virt(PhysAddr::new(user_cr3)).as_ptr::<PageTable>() };
	let l4 = &pml4[pml4_idx];
	if !table_pte_ok(l4.flags()) {
		return Err(EFAULT);
	}
	let l4_frame = l4.frame().map_err(|_| EFAULT)?;

	let pdpt = unsafe { &*phys_to_virt(l4_frame.start_address()).as_ptr::<PageTable>() };
	let l3 = &pdpt[pdpt_idx];
	if !table_pte_ok(l3.flags()) {
		return Err(EFAULT);
	}
	let l3_frame = l3.frame().map_err(|_| EFAULT)?;

	let pd = unsafe { &*phys_to_virt(l3_frame.start_address()).as_ptr::<PageTable>() };
	let l2 = &pd[pd_idx];
	if !table_pte_ok(l2.flags()) {
		return Err(EFAULT);
	}
	let l2_frame = l2.frame().map_err(|_| EFAULT)?;

	let pt = unsafe { &*phys_to_virt(l2_frame.start_address()).as_ptr::<PageTable>() };
	let l1 = &pt[pt_idx];
	if !leaf_pte_ok(l1.flags()) {
		return Err(EFAULT);
	}
	let l1_frame = l1.frame().map_err(|_| EFAULT)?;

	let host = phys_to_virt(l1_frame.start_address()).as_u64() + page_off as u64;
	Ok(host as *const u8)
}

/// Copy from user memory into `dst`. Kernel page tables must be active.
#[inline(never)]
pub fn copy_from_user_into(dst: &mut [u8], src: u64) -> Result<(), i64> {
	let len = dst.len();
	if len == 0 {
		return Ok(());
	}
	if !user_range_ok(src, len) {
		return Err(EFAULT);
	}
	let user_cr3 = SYSCALL_USER_CR3.load(Ordering::Acquire);
	if user_cr3 == 0 {
		return Err(EFAULT);
	}

	let mut offset = 0usize;
	while offset < len {
		let virt = VirtAddr::new(src + offset as u64);
		let page_off = (virt.as_u64() & (PAGE_SIZE as u64 - 1)) as usize;
		let chunk = (len - offset).min(PAGE_SIZE - page_off);

		let host = user_host_ptr(user_cr3, virt)?;
		unsafe {
			core::ptr::copy_nonoverlapping(host, dst.as_mut_ptr().add(offset), chunk);
		}
		offset += chunk;
	}
	Ok(())
}

/// Copy from user memory into a new kernel heap buffer.
#[inline(never)]
#[allow(dead_code)]
pub fn copy_from_user(len: usize, src: u64) -> Result<Vec<u8>, i64> {
	if len == 0 {
		return Ok(Vec::new());
	}
	let mut out = Vec::new();
	out.resize(len, 0);
	copy_from_user_into(&mut out, src)?;
	Ok(out)
}
