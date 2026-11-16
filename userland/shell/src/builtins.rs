use core::ptr::null_mut;

use alloc::{string::{String, ToString}, vec::Vec};
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
	println!("ls path: {path}");
	let f = open(path.as_str().as_bytes(), VfsFileOpenflags::O_DIRECTORY.bits() as u64);
	if f < 0 {
		return Err(f);
	}

	let mut raw = [0u8; 256];
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
			match filetype {
				4 => {
					println!("\t\x1b[94m{name}\x1b[0m");
				}
				8 => {
					println!("\t\x1b[97m{name}\x1b[0m")
				}
				_ => {
					println!("\t{name}");
				}
			}
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