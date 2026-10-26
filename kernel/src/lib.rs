#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![allow(internal_features)]
#![feature(ptr_internals)]
extern crate alloc;

pub mod arch;
pub mod drivers;
pub mod memory;
pub mod debug;
pub mod sync;
pub mod interrupts;
pub mod scheduler;
pub mod elf;
pub mod syscalls;
//pub mod vfs;
pub mod errno;

pub mod limine_requests {
	use limine::{BaseRevision, RequestsEndMarker, RequestsStartMarker, request::*};
	#[used]
	#[unsafe(link_section = ".request_start")]
	pub static REQUESTS_START: RequestsStartMarker = RequestsStartMarker::new();
	#[unsafe(link_section = ".requests")]
	pub static BASE_REVISION: BaseRevision = BaseRevision::new();
	#[unsafe(link_section = ".requests")]
	pub static FRAMEBUFFER: FramebufferRequest = FramebufferRequest::new();
	#[unsafe(link_section = ".requests")]
	pub static MEMMAP: MemmapRequest = MemmapRequest::new();
	#[unsafe(link_section = ".requests")]
	pub static HHDM: HhdmRequest = HhdmRequest::new();
	#[unsafe(link_section = ".requests")]
	pub static RSDP: RsdpRequest = RsdpRequest::new();
	#[unsafe(link_section = ".requests")]
	pub static MODULES: ModulesRequest = ModulesRequest::new();
	#[used]
	#[unsafe(link_section = ".requests_end")]
	pub static REQUESTS_END: RequestsEndMarker = RequestsEndMarker::new();
}


#[cfg(not(test))]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
	if let Some(loc) = info.location() {
		kpanic!("msg: {}\nloc: {}:{}:{}", info.message(), loc.file(), loc.line(), loc.column());
	} else {
		kpanic!("msg: {}\n", info.message());
	}
}

#[macro_export]
macro_rules! kprint {
	($($arg:tt)*) => {
		$crate::drivers::console::_console_print(format_args!($($arg)*));
		$crate::debug::_ksprint(format_args!($($arg)*));
	};
}

#[macro_export]
macro_rules! ksprint {
	($($arg:tt)*) => { $crate::debug::_ksprint(format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! klog {
	($($arg:tt)*) => { $crate::debug::_klog(format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! kwarn {
	($($arg:tt)*) => { $crate::debug::_kwarn(format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! kerror {
	($($arg:tt)*) => { $crate::debug::_kerror(format_args!($($arg)*)) };
}

#[macro_export]
macro_rules! kpanic {
	($($arg:tt)*) => { $crate::debug::_kpanic(format_args!($($arg)*)) };
}

pub struct InitChecklist {
	pub core: bool,
	pub memory: bool,
	pub console: bool
}

impl InitChecklist {
	pub const fn new() -> Self {
		Self { core: false, memory:false, console:false }
	}
}

pub fn limine_base_revision() -> u64 {
	use crate::limine_requests::BASE_REVISION;
	if let Some(rev) = BASE_REVISION.actual_revision() {
		rev
	} else {
		0
	}
}