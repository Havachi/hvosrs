use core::ptr::{self, null_mut};

use alloc::{string::{String, ToString}, vec::Vec, vec};
use uhv::*;

use crate::{Env, join_pwd};

pub const HELP_STR: [(&str, &str); 9] = [
	("cd", "[DIR]"),
	("clear", ""),
	("echo", "[arg ...]"),
	("exit", "[n]"),
	("help", "[pattern ...]"),
	("history", ""),
	("ls", "[dir]"),
	("mkdir", "[dir]"),
	("pwd", ""),
];

pub fn pwd(env: &Env) -> Result<(), i64> {
	println!("{}", env.pwd);
	Ok(())
}

pub fn cd(env: &mut Env, arg: &Vec<String>) -> Result<(), i64> {
	if arg.len() < 2 {
		return Err(-22);
	}
	let path = arg[1].as_str();

	if path.len() < 1 {
		return Err(-22);
	}

	let res = chdir(path);
	if res < 0 {
		return Err(res);
	}
	
	env.pwd = join_pwd(&env.pwd, path);
	env.update();
	Ok(())
}

//TODO, retreive file/dir metadata and print them with the name
pub fn ls(env: &mut Env, arg: &Vec<String>) -> Result<(), i64> {

	let path = if arg.len() <= 1 {
		&env.pwd
	} else {
		&arg[1]
	};
	let f = open(
		path.as_str().as_bytes(), 
		(VfsFileOpenflags::O_DIRECTORY | VfsFileOpenflags::O_CLOEXEC | VfsFileOpenflags::O_NONBLOCK | VfsFileOpenflags::O_RDONLY).bits() as u64
	);
	if f < 0 {
		return Err(f);
	}

	let mut raw = [0u8; 256];
	let mut statbuf = [0u8; size_of::<Stat>()];
	fstat(f as u64, &mut statbuf);
	let dirstat = unsafe { ptr::read_unaligned(statbuf.as_ptr() as *const Stat) };
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
			let mut filepath = path.clone();
			filepath.push_str("/");
			filepath.push_str(name);
			stat(&filepath, &mut statbuf);
			let filestat = unsafe { ptr::read_unaligned(statbuf.as_ptr() as *const Stat) };

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
	Ok(())
}

pub fn history(env: &Env) {
	if env.history.len() == 0 {
		println!("history is empty");
		return;
	}
	for (i, line) in env.history.iter().enumerate() {
		println!("{} {}", i, line);
	}
}

pub fn help(args: &String) {
	if args.is_empty() {
		println!("HvOs Shell version 1");
		println!("List of available commands:");
		for (n,a) in HELP_STR {
			println!("{} {}", n, a);
		}
		return;
	}


	match args.as_str() {
		"cd" => {
			let (n,a) = HELP_STR[0];
			println!("{n} {a}");
			println!("\tChange the shell working directory to DIR");
			println!();
		}
		_ => {
			println!("no help topics match `{}`", args)
		}
	}

	println!("HVos shell, version 1");

}

pub fn echo(env: &Env, arg: &Vec<String>) -> Result<(), i64> {
	//remove the echo from args
	let args = &arg[1..];
	for s in args {
		if s.contains('$') {
			match env.get_var(&s[1..].to_string()) {
				Some(v) => print!("{}", v),
				None => print!(""),
			}
		} else {
			print!("{}", s)
		}
	}
	println!();
	Ok(())
}

pub fn clear() -> Result<(), i64> {
	print!("\x1b[2J\x1b[H");
	Ok(())
}

pub fn exec(env: &Env, arg: &Vec<String>) -> Result<(), i64> {
	if arg.len() < 2 {
		return Err(-22);
	}
	let path = arg[1].as_str();
	let res = execve(path.as_bytes(), null_mut(), null_mut());
	if res < 0 {
		return Err(res);
	}
	Ok(())
}

pub fn mount(arg: &Vec<String>) -> Result<(), i64> {
	if arg.len() < 3 {
		return Err(-22);
	}
	let dev = arg[1].as_str();
	let mnt = arg[2].as_str();

	let res = uhv::mount(dev, mnt);
	if res < 0 {
		return Err(res);
	}
	Ok(())
}

pub fn mkfs(arg: &Vec<String>) -> Result<(), i64> {
	if arg.len() < 2 {
		return Err(-22);
	}
	let path = arg[1].as_str();
	let fd = open(path.as_bytes(), O_RDWR);
	if fd < 0 {
		return Err(fd);
	}
	println!("opened {}", path);
	let mut statbuf = vec![0u8; size_of::<Stat>()];
	let res = fstat(fd as u64, &mut statbuf);
	if res < 0 {
		close(fd);
		return Err(res);
	}
	let filestat = unsafe { ptr::read_unaligned(statbuf.as_ptr() as *const Stat) };
	println!("stat: {}", filestat.st_rdev);

	Ok(())
}


