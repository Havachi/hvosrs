#![no_std]
#![no_main]
extern crate alloc;
use alloc::{string::String, vec::Vec};
use uhv::*;

///
/// USAGE: 
/// cat -> read stdin and write to stdout
/// cat [file] -> read file and write to stdout
/// cat [fileA] [fileB] -> read fileA and fileB, concatenate their content and write to stdout
/// [command] | cat -> take stdout of [command] as his stdin and write it to stdout, this behaviour is made by the pipe, cat act like there was no args (case 1)
#[unsafe(no_mangle)]
extern "Rust" fn main(argc: isize, _argv: *const *const u8) -> i32 {
    let mut args = uhv::args::args_lossy_skip_invalid().skip(1).peekable();

    if argc < 2 {
        //no args

        let mut buf = [0u8; 1024];
        loop {
            let n = read(0, &mut buf);
            if n < 0 {
                return 1;
            }
            if n == 0 {
                break;
            }

            let mut off = 0;
            while off < n  as usize {
                let w = write(1, &buf[off..n as usize]);
                if w < 0 {
                    return 1;
                }
                off += w as usize
            }
        }
    } else {
        //some args
        let mut files = Vec::new();
        while let Some(f) = args.next() {
            files.push(f);
        }
    };
    0
}
