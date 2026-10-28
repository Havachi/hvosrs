use spin::Mutex;



static ERRNO: Mutex<i64> = Mutex::new(0);


pub fn set_errno(value: i64) {
	*ERRNO.lock() = value
}

pub fn errno() -> i64 {
	*ERRNO.lock()
}

#[non_exhaustive]
#[repr(u64)]
#[derive(Debug, PartialEq, Eq)]
pub enum KernelErrors {
	///Operation not permitted
	EPERM = 1,
	///No such file or directory
	ENOENT = 2,
	///Permission Denied
	EACCESS = 13,
	//Ressource or device busy
	EBUSY = 16,
	///File Exist
	EEXIST = 17,
	///Not a directory
	ENOTDIR = 20,
	EISDIR = 21,
	EINVAL = 22,
	EBADF = 9,
}

impl KernelErrors {
	pub fn to_neg_errno(self) -> i64 {
		-(self as u64 as i64)
	}
}