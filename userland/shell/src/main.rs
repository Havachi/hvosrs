#![no_std]
#![no_main]

use core::sync::atomic::AtomicI64;

use alloc::{string::String, vec::Vec};
use uhv::*;
extern crate alloc;
pub static SHELL_STATUS: AtomicI64 = AtomicI64::new(0);

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
	let mut env = Env::default();
	env.pwd = String::from("/");

	//test
	let f = open(b"/", 0o000200000);
	if f < 0 {
		println!("open on / return -> {}", errno_to_str(f));
	} else {
		println!("opened a dir ! FD: {f}")
	}



	let mut buf = Vec::with_capacity(1024);
    print!("-> {} > ", env.pwd);
	loop {
		let mut c = getchar();
		while c != b'\n' {
			write(1, &[c]);
			buf.push(c);
			c = getchar();
		}
		println!();
		let res = repl(&mut env, &buf);
		match res {
			Ok(_) => {
				SHELL_STATUS.store(0, core::sync::atomic::Ordering::Relaxed);
			},
			Err(e) => {
				eprintln!("err: {}", errno_to_str(e));
				SHELL_STATUS.store(e, core::sync::atomic::Ordering::Relaxed);
			},
		}
		print!("-> {} > ", env.pwd);
		buf.clear();
	}
	exit(42);
	loop {
		unsafe { core::arch::asm!("pause", options(nomem, nostack)) };
	}
}

fn repl(env: &mut Env, buf: &Vec<u8>) -> Result<(), i64> {
	if buf.len() == 0 {
		return Ok(());
	}
	let (cmd, args) = split_cmd(buf)?;
	match cmd.as_str() {
		"exit" => {
			exit(0);
		}
		"pwd" => pwd(env)?,
		"cd" => cd(env, &args)?,
		"ls" => ls(env, &args)?,
		_ => {
			eprintln!("shell: command not found: {}", cmd);
			return Err(127)
		}
	}

	Ok(())
}

fn split_cmd(buf: &Vec<u8>) -> Result<(String, Vec<String>), i64>{

	let trim = buf.trim_ascii();
	let full = str::from_utf8(trim).map_err(|_| -22)?;
	let mut args = Vec::new();
	let mut cmd = String::new();

	for (i, sub) in full.split_terminator(" ").enumerate() {
		if i == 0 {
			cmd = String::from(sub)
		}
		args.push(String::from(sub));
	}
	Ok((cmd, args))
}

fn pwd(env: &Env) -> Result<(), i64> {
	println!("{}", env.pwd);
	Ok(())
}

fn cd(env: &mut Env, arg: &Vec<String>) -> Result<(), i64> {
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
	Ok(())
}

//TODO, retreive file/dir metadata and print them with the name
fn ls(env: &mut Env, arg: &Vec<String>) -> Result<(), i64> {

	let path = if arg.len() <= 1 {
		&env.pwd
	} else {
		&arg[1]
	};
	println!("ls path: {path}");
	let f = open(path.as_str().as_bytes(), 0o000200000);
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
			println!("\t{name}");
			off += reclen;
		}
	}

	close(f);
	Ok(())
}


#[derive(Debug, Default)]
pub struct Env {
	pub pwd: String,
}