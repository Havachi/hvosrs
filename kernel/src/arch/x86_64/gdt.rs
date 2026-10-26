use core::cell::UnsafeCell;

use spin::{Mutex, Once, RwLock};
use x86_64::{PrivilegeLevel::{Ring0, Ring3}, VirtAddr, instructions::tables::load_tss, registers::segmentation::{CS, SS, Segment}, structures::{gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector}, tss::TaskStateSegment}};

use crate::klog;

pub const KERNEL_CODE_DESC: Descriptor = Descriptor::kernel_code_segment();
pub const KERNEL_CODE: SegmentSelector = SegmentSelector::new(1, Ring0);

pub const KERNEL_DATA_DESC: Descriptor = Descriptor::kernel_data_segment();
pub const KERNEL_DATA: SegmentSelector = SegmentSelector::new(2, Ring0);

pub const USER_DATA_DESC: Descriptor = Descriptor::user_data_segment();
pub const USER_DATA: SegmentSelector = SegmentSelector::new(3, Ring3);

pub const USER_CODE_DESC: Descriptor = Descriptor::user_code_segment();
pub const USER_CODE: SegmentSelector = SegmentSelector::new(4, Ring3);

pub const TSS_SEL: SegmentSelector = SegmentSelector::new(5, Ring0);

pub const DOUBLE_FAULT_IST_INDEX: usize = 0;
pub const PAGE_FAULT_IST_INDEX: usize = 1;
const DOUBLE_FAULT_STACK_SIZE:  usize = 4096 * 5;
static DOUBLE_FAULT_STACK: Mutex<UnsafeCell<[u8; DOUBLE_FAULT_STACK_SIZE]>> = Mutex::new(UnsafeCell::new([0; DOUBLE_FAULT_STACK_SIZE]));
static TSS: Once<RwLock<TaskStateSegment>> = Once::new();
static GDT: Once<GlobalDescriptorTable> = Once::new();

fn get_tss() -> &'static RwLock<TaskStateSegment> {
	TSS.call_once(|| {
		let mut stacklock = DOUBLE_FAULT_STACK.lock();
		let mut tss: TaskStateSegment = TaskStateSegment::new();
		let stack_start = VirtAddr::from_ptr((*stacklock).get_mut().as_mut_ptr());
		tss.interrupt_stack_table[0] = stack_start;
		RwLock::new(tss)
	})
}

fn get_gdt() -> &'static GlobalDescriptorTable {
	GDT.call_once(|| {
		let mut gdt = GlobalDescriptorTable::new();
		gdt.append(KERNEL_CODE_DESC);
		gdt.append(KERNEL_DATA_DESC);
		gdt.append(USER_DATA_DESC);
		gdt.append(USER_CODE_DESC);
		let tss_lock_ref = get_tss();
		let tss_raw_ptr = tss_lock_ref.as_mut_ptr();
		unsafe {
			let tss_static_ref = &*tss_raw_ptr;
			gdt.append(Descriptor::tss_segment(tss_static_ref));
		}
		gdt
	})
}

pub fn init() {
	get_gdt().load();
	unsafe {
		CS::set_reg(KERNEL_CODE);
		SS::set_reg(KERNEL_DATA);
		load_tss(TSS_SEL);
	};
	klog!("GDT init done")
}

pub fn set_tss_rsp0(stack_end: VirtAddr) {
	let mut tss = get_tss().write();
	tss.privilege_stack_table[0] = stack_end;
}