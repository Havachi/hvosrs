use core::ffi::CStr;
use core::sync::atomic::{AtomicIsize, AtomicPtr, Ordering};

static ARGC: AtomicIsize = AtomicIsize::new(0);
static ARGV: AtomicPtr<*const u8> = AtomicPtr::new(core::ptr::null_mut());
static ENVP: AtomicPtr<*const u8> = AtomicPtr::new(core::ptr::null_mut());

pub unsafe fn init_args(argc: isize, argv: *const *const u8, envp: *const *const u8) {
	ARGC.store(argc.max(0), Ordering::Release);
	ARGV.store(argv as *mut *const u8, Ordering::Release);
	ENVP.store(envp as *mut *const u8, Ordering::Release);
}

pub struct ArgsIter {
	argv: *const *const u8,
	index: usize,
	count: usize,
}

impl Iterator for ArgsIter {
	type Item = &'static CStr;
	fn next(&mut self) -> Option<Self::Item> {
		if self.index >= self.count || self.argv.is_null() {
			return None;
		}

		let arg_ptr = unsafe { *self.argv.add(self.index) };
		self.index += 1;
		if arg_ptr.is_null() {
			return None;
		}
		
		Some(unsafe {
			CStr::from_ptr(arg_ptr as *const core::ffi::c_char)
		})
	}
}

pub fn args_cstr() -> ArgsIter {
	let count = ARGC.load(Ordering::Acquire).max(0) as usize;
	let argv = ARGV.load(Ordering::Acquire) as *const *const u8;
	ArgsIter {
		argv,
		index: 0,
		count
	}
}

pub fn args_lossy_skip_invalid() -> impl Iterator<Item = &'static str> {
	args_cstr().filter_map(|cstr| cstr.to_str().ok())
}

pub fn program_name() -> Option<&'static str> {
	args_lossy_skip_invalid().next()
}

