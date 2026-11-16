use crate::{errno::KernelErrors, klog, vfs::{self, file::VfsFileOpenflags}};



#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct CpioNewcHeader {
    pub c_magic: [u8; 6],
    pub c_ino: [u8; 8],
    pub c_mode: [u8; 8],
    pub c_uid: [u8; 8],
    pub c_gid: [u8; 8],
    pub c_nlink: [u8; 8],
    pub c_mtime: [u8; 8],
    pub c_filesize: [u8; 8],
    pub c_devmajor: [u8; 8],
    pub c_devminor: [u8; 8],
    pub c_rdevmajor: [u8; 8],
    pub c_rdevminor: [u8; 8],
    pub c_namesize: [u8; 8],
    pub c_check: [u8; 8],
}

const _: () = assert!(core::mem::size_of::<CpioNewcHeader>() == 110);


impl CpioNewcHeader {
    pub fn is_valid(&self) -> bool {
        &self.c_magic == b"070701"
    }

    pub fn namesize(&self) -> Option<usize> {
        Self::parse_hex(&self.c_namesize)
    }

    pub fn filesize(&self) -> Option<usize> {
        Self::parse_hex(&self.c_filesize)
    }

    pub fn mode(&self) -> Option<u32> {
        Self::parse_hex(&self.c_mode).map(|v| v as u32)
    }

    fn parse_hex(bytes: &[u8; 8]) -> Option<usize> {
        let s = core::str::from_utf8(bytes).ok()?;
        usize::from_str_radix(s, 16).ok()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CpioEntry<'a> {
    pub name: &'a str,
    pub mode: u32,
    pub data: &'a [u8],
}

pub struct CpioIter<'a> {
    archive: &'a [u8],
    offset: usize,
}

impl<'a> CpioIter<'a> {
    pub fn new(archive: &'a [u8]) -> Self {
        Self { archive, offset: 0 }
    }

    fn align4(val: usize) -> usize {
        (val + 3) & !3
    }

    fn parse_hex(bytes: &[u8]) -> Option<usize> {
        let s = str::from_utf8(bytes).ok()?;
        usize::from_str_radix(s, 16).ok()
    }
}

impl<'a> Iterator for CpioIter<'a> {
    type Item = CpioEntry<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        const HEADER_LEN: usize = 110;

        // Ensure enough bytes remain for the 110-byte header
        let hdr_bytes = self.archive.get(self.offset..self.offset + HEADER_LEN)?;

        // Verify magic "070701"
        if &hdr_bytes[0..6] != b"070701" {
            return None;
        }

        // Parse mode, filesize (offset 54..62), and namesize (offset 94..102)
        let mode = Self::parse_hex(&hdr_bytes[14..22])? as u32;
        let filesize = Self::parse_hex(&hdr_bytes[54..62])?;
        let namesize = Self::parse_hex(&hdr_bytes[94..102])?;

        // Extract filename (includes trailing NUL byte)
        let name_start = self.offset + HEADER_LEN;
        let name_end = name_start.checked_add(namesize)?;
        let raw_name = self.archive.get(name_start..name_end)?;

        // Strip trailing NUL byte and convert to &str
        let name_bytes = raw_name.strip_suffix(b"\0").unwrap_or(raw_name);
        let name = str::from_utf8(name_bytes).ok()?;

        // Check for end-of-archive marker
        if name == "TRAILER!!!" {
            self.offset = self.archive.len();
            return None;
        }

        // Calculate 4-byte aligned start and end of file payload
        let data_start = Self::align4(name_end);
        let data_end = data_start.checked_add(filesize)?;
        let data = self.archive.get(data_start..data_end)?;

        // Advance offset to next 4-byte aligned header
        self.offset = Self::align4(data_end);

        Some(CpioEntry { name, mode, data })
    }
}

pub fn load_initrd(initrd_slice: &[u8]) -> Result<(), KernelErrors> {
    for entry in CpioIter::new(initrd_slice) {
    	let path = entry.name.trim_start_matches("./").trim_start_matches("/");
		if path.is_empty() || path == "." {
			continue;
		}

		let file_type = entry.mode & S_IFMT;

		match file_type {
			S_IFDIR => {
				let _ = vfs::mkdir(path);
			},
			S_IFREG => {
				if let Some((parent, _)) = path.rsplit_once('/') {
					let _ = vfs::mkdir(parent);
				}

				vfs::create(path)?;
				let mut file = vfs::open(path,VfsFileOpenflags::O_RDWR.bits())?;
				vfs::write(&mut file, entry.data)?;
			}
			_ => {

			}
		}
    }
	Ok(())
}

const S_IFMT: u32  = 0o170000;
const S_IFDIR: u32 = 0o040000;
const S_IFREG: u32 = 0o100000;
const S_IFLNK: u32 = 0o120000;