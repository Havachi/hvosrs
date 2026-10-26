use x86_64::registers::{control::{Cr4, Cr4Flags, Efer, EferFlags}};

pub fn init() {
	unsafe { Cr4::update(|f| {
		f.insert(Cr4Flags::OSFXSR);
	}) };

	unsafe{
		Efer::update(|f| {
			f.insert(EferFlags::NO_EXECUTE_ENABLE);
			f.insert(EferFlags::SYSTEM_CALL_EXTENSIONS);
		});
	}

	

}