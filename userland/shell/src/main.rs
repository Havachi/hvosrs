#![no_std]
#![no_main]

use core::sync::atomic::AtomicI64;

use alloc::{string::{String, ToString}, vec::Vec};
use uhv::*;
extern crate alloc;
pub static SHELL_STATUS: AtomicI64 = AtomicI64::new(0);

fn move_left(n: usize) {
	if n == 0 {
		return;
	}
	let mut digits = [0u8; 10];
	let mut v = n;
	let mut i = digits.len();
	while v > 0 {
		i -= 1;
		digits[i] = b'0' + (v % 10) as u8;
		v /= 10;
	}
	let digits = &digits[i..];
	let mut seq = [0u8; 16];
	seq[0] = 0x1b;
	seq[1] = b'[';
	seq[2..2 + digits.len()].copy_from_slice(digits);
	let end = 2 + digits.len();
	seq[end] = b'D';
	write(1, &seq[..=end]);
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
	let mut env = Env::default();
	env.pwd = String::from("/");

	let mut history: Vec<String> = Vec::new();
	let mut line_buffer = Vec::with_capacity(1024);
	let mut cursor = 0usize;

    print!("-> {} > ", env.pwd);
	loop {
		let mut c = getchar();
		while c != b'\n' {
			if c == 0x1B {//Handle extended codes
				let bracket = getchar();
				let dir = getchar();
				if bracket == b'[' {
					if dir == b'D' && cursor > 0 {
						cursor -= 1;
						write(1, b"\x1b[D");
					} else if dir == b'C' && cursor < line_buffer.len() {
						cursor += 1;
						write(1, b"\x1b[C");
					}
				}
			
			} else if c == 0x08 { //Handle Backspace
				if cursor > 0 {
					line_buffer.remove(cursor - 1);
					cursor -= 1;
					write(1, b"\x1b[D");
					write(1, &line_buffer[cursor..]);
					write(1, b" ");
					move_left(line_buffer.len() - cursor + 1);
				}
			} else if c != b'\0' {//Handle any other characters
				if cursor < line_buffer.len() {
					line_buffer.insert(cursor, c);
				} else {
					line_buffer.push(c);
				}
				write(1, &line_buffer[cursor..]);
				move_left(line_buffer.len() - cursor - 1);
				cursor += 1;
			}
			c = getchar();
		}
		
		let line = str::from_utf8(&line_buffer).unwrap_or("");
		history.push(line.to_string());
		println!();
		let res = repl(&mut env, &line_buffer);

		match res {
			Ok(_) => {
				SHELL_STATUS.store(0, core::sync::atomic::Ordering::Relaxed);
			},
			Err(e) => {
				if e == 127 {
					
				} else {
					eprintln!("err: {}", errno_to_str(e));
				}
				SHELL_STATUS.store(e, core::sync::atomic::Ordering::Relaxed);
			},
		}
		print!("-> {} > ", env.pwd);
		line_buffer.clear();
		cursor = 0;
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