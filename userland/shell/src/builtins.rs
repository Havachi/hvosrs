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

pub fn history(env: &Env) -> Result<(), i64>{
	if env.history.len() == 0 {
		println!("history is empty");
		return Ok(());
	}
	for (i, line) in env.history.iter().enumerate() {
		println!("{} {}", i, line);
	}
	return Ok(());
}

pub fn help(args: &String) -> Result<(), i64>{
	if args.is_empty() {
		println!("HvOs Shell version 1");
		println!("List of available commands:");
		for (n,a) in HELP_STR {
			println!("{} {}", n, a);
		}
		return Ok(());
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
	return Ok(());

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


