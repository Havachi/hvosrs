#![no_std]
#![no_main]
use alloc::string::String;
use uhv::*;
extern crate alloc;
#[unsafe(no_mangle)]
extern "Rust" fn main(argc: isize, _argv: *const *const u8) -> i32 {
    let mut args = uhv::args::args_lossy_skip_invalid().skip(1).peekable();

    let path = if argc < 2 {
        "."
    } else {
        args.next().unwrap_or(".")
    };

    if !path.is_empty() {
        
	    let f = open(
		    path.as_bytes(), 
	    	(VfsFileOpenflags::O_DIRECTORY | VfsFileOpenflags::O_CLOEXEC | VfsFileOpenflags::O_NONBLOCK | VfsFileOpenflags::O_RDONLY).bits() as u64
        );
        let mut raw = [0u8; 256];
	    let mut statbuf = [0u8; size_of::<Stat>()];
        fstat(f as u64, &mut statbuf);
        let dirstat = unsafe { core::ptr::read_unaligned(statbuf.as_ptr() as *const Stat) };
        println!("total: {}", dirstat.st_blocks);

        loop {
            let n = getdents(f, &mut raw);
            if n <= 0 { break; }
            let mut off = 0usize;
            while off + 19 <= n as usize {
                let reclen = u16::from_le_bytes([raw[off+16], raw[off + 17]]) as usize;
                if reclen == 0 || off + reclen > n as usize { break; }
                let name_end = raw[off + 19..off + reclen].iter().position(|&b| b == 0).unwrap_or(0);
                let name = core::str::from_utf8(&raw[off + 19..off + 19 + name_end]).unwrap_or("?");
                let filetype = raw[off + 18];
                let mut filepath = String::from(path);
                filepath.push_str("/");
                filepath.push_str(name);
                stat(&filepath, &mut statbuf);
                let filestat = unsafe { core::ptr::read_unaligned(statbuf.as_ptr() as *const Stat) };

                print!("{}\t{}\t", filestat.st_ino, InodeMode::from_bits_truncate(filestat.st_mode as u16));
                match filetype {
                    //Directory
                    4 => {
                        print!("\x1b[94m");
                    }
                    //Devices
                    2 | 6 => {
                        print!("\x1b[0;93;40m")
                    }
                    //Regular files
                    8 => {
                        print!("\x1b[97m")
                    }
                    //Symlink files
                    10 => {
                        print!("\x1b[0;96;49m")
                    }
                    _ => {
                    }
                }
                println!("{name}\x1b[0m");
                off += reclen;
            }
	    }
	close(f);
    }

    0
}
