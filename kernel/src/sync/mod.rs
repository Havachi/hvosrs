use core::{marker::PhantomData, sync::atomic::{AtomicBool, Ordering}};
use talc::lock_api::{GuardSend, RawMutex};

pub trait Relax: Default {
	fn relax(&mut self);
}
#[derive(Default)]
pub struct Spin;

impl Relax for Spin {
	#[inline]
	fn relax(&mut self) {
		core::hint::spin_loop();
	}
}


pub struct RawHeapLock<R: Relax = Spin> {
	locked: AtomicBool,
	relax: PhantomData<R>,
}


impl RawHeapLock {
	#[inline]
	fn _try_lock_weak(&self) -> bool {
		self.locked
			.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
			.is_ok()
	}
}

unsafe impl<R: Relax> RawMutex for RawHeapLock<R> {
	const INIT: RawHeapLock<R> = RawHeapLock {
		locked: AtomicBool::new(false),
		relax: PhantomData,
	};

	type GuardMarker = GuardSend;

	#[inline]
	fn lock(&self) {
		let mut relax = R::default();
		while !(self.locked
					.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
					.is_ok())
		{
			while self.is_locked() {
				relax.relax();
			}
		}
	}

	#[inline]
	fn try_lock(&self) -> bool {
		self.locked
			.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
			.is_ok()
	}

	#[inline]
	unsafe fn unlock(&self) {
		self.locked.store(false, Ordering::Release);
	}

	#[inline]
	fn is_locked(&self) -> bool {
		self.locked.load(Ordering::Relaxed)
	}

}
