use alloc::sync::Arc;

use crate::{errno::KernelErrors::{self, EINVAL}, kerror, klog, ksprint, vfs::dentry::VfsDentry};


pub fn path_walk(path: &str, root_dentry: Arc<VfsDentry>, cwd_dentry: Arc<VfsDentry>) -> Result<Arc<VfsDentry>, KernelErrors> {
	if path.is_empty() {
		return Err(KernelErrors::EINVAL);
	}

	let mut current = if path.starts_with('/') {
		root_dentry
	} else {
		cwd_dentry
	};

	let bytes = path.as_bytes();
	let len = bytes.len();
	let mut idx = 0;

	while idx < len {
		while idx < len && bytes[idx] == b'/' {
			idx += 1;
		}
		if idx >= len {
			break;
		}
		let start = idx;
		while idx < len && bytes[idx] != b'/' {
			idx += 1;
		}
		let component = &path[start..idx];

		match component {
			"." => continue,
			".." => {
				current = current.parent.clone().unwrap_or_else(|| current.clone());
			}
			name => {
				if !current.inode.is_dir() {
					return Err(KernelErrors::ENOTDIR);
				}
				current = current.inode.clone().lookup(name, Some(current))?;
			}
		}
	}

	Ok(current)
}

pub fn path_parent(path: &str) -> Result<&str, KernelErrors> {
	if path.is_empty() {
		return Err(EINVAL);
	}

	let path = path.trim_end_matches(|c| c == '/');
	if path.is_empty() {
		return Err(EINVAL);
	}

	let mut last_sep = None;
	for (i,b) in path.bytes().enumerate().rev() {
		if b == b'/' {
			last_sep = Some(i);
			break;
		}
	}

	match last_sep {
		Some(0) => {
			Ok(&path[0..=0])
		}
		Some(idx) => {
			Ok(&path[..idx])
		}
		None => {
			Err(KernelErrors::EINVAL)
		}
	}
}

pub fn path_basename(path: &str) -> Result<&str, KernelErrors> {
	if path.is_empty() {
		return Err(EINVAL);
	}
	let path = path.trim_end_matches(|c| c == '/');
	if path.is_empty() {
		return Err(EINVAL);
	}

	let mut last_sep = None;
	for (i,b) in path.bytes().enumerate().rev() {
		if b == b'/' {
			last_sep = Some(i);
			break;
		}
	}

	let component = match last_sep {
		Some(idx) => &path[idx+1..],
		None => path,
	};

	if component.is_empty() {
		Err(KernelErrors::EINVAL)
	} else {
		Ok(component)
	}
}