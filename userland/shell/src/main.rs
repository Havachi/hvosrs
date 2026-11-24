#![no_std]
#![no_main]

pub mod builtins;
use core::ptr::null;

use builtins::*;

use alloc::{collections::BTreeMap, format, string::{String, ToString}, vec::{self, Vec}};
use uhv::*;
extern crate alloc;

pub fn init(env: &mut Env) {
	env.pwd = String::from("/");
	env.history = Vec::new();
	env.prompt = String::from(format!("\x1b[92m-> \x1b[0m\x1b[94m{}\x1b[0m > ", env.pwd));
	env.vars = BTreeMap::new();
	env.vars.insert(String::from("?"), ShellVar::Int(0));
	env.vars.insert(String::from("PATH"), ShellVar::Str(String::from("/bin")));
}

#[unsafe(no_mangle)]
extern "Rust" fn main(_: isize, _: *const *const u8) -> i32 {
	let mut env = Env::default();
	init(&mut env);
	let mut line_buffer = Vec::with_capacity(1024);
	let mut cursor = 0usize;
	print!("{}", env.prompt);
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
		env.history.push(line.to_string());
		println!();
		let res = repl(&mut env, &line_buffer);

		match res {
			Ok(_) => {
				env.set_var("?".to_string(), ShellVar::Int(0));
			},
			Err(e) => {
				if e == 127 {
					
				} else {
					eprintln!("err: {}", errno_to_str(e));
				}
				env.set_var("?".to_string(), ShellVar::Int(e));
			},
		}
		print!("{}", env.prompt);
		line_buffer.clear();
		cursor = 0;
	}
}

fn repl(env: &mut Env, buf: &Vec<u8>) -> Result<(), i64> {
	if buf.len() == 0 {
		return Ok(());
	}
	let (_, args) = split_cmd(buf)?;
	let parsed = replace_vars(env, &args);
	let cmd = parsed[0].clone();

	match run_builtin(env, &parsed) {
		Some(res) => return res,
		None => {}
	}

	if parsed[0].contains('=') {
		return set_var(env, &parsed[0]);
	}


	let path = match  resolve_prog_path(env, &args) {
		Ok(p) => p,
		Err(_) => {
			eprintln!("shell: command not found: {}", cmd);
			return Err(127);
		}
	};
	spawn(&path, &parsed)
}

fn resolve_prog_path(env: &Env, args: &Vec<String>) -> Result<String, i64> {
	if args[0].contains("/") {
		let res = open(args[0].as_bytes(), 0);
		if res > 0 {
			close(res);
			return Ok(String::from(args[0].clone()));
		} else {
			return Err(127);
		}
	} else {
		if let ShellVar::Str(shellpath) = env.get_var(&"PATH".to_string()).ok_or(-1)? {
			for dirs_path in shellpath.split_terminator(':') {
				if dirs_path.is_empty() {
					continue;
				}
				let mut try_resolve_path = String::from(dirs_path);
				try_resolve_path.push('/');
				try_resolve_path.push_str(&args[0]);
				let res = open(try_resolve_path.as_bytes(), 0);
				if res > 0 {
					close(res);
					return Ok(String::from(try_resolve_path));
				}
			}
			return Err(127);
		}
	}
	return Err(127);
}

fn spawn(path: &str, args: &Vec<String>) -> Result<(), i64> {
	let p = fork();
	if p == 0 {
		let mut path_cstr = Vec::with_capacity(path.len() + 1);
		path_cstr.extend_from_slice(path.as_bytes());
		path_cstr.push(0);
		let argc = args.len();
		let prog_name =  &path.clone();
		let mut bufs: Vec<Vec<u8>> = Vec::new();
		for p in args {
			let mut buf = Vec::with_capacity(p.len() + 1);
			buf.extend_from_slice(p.as_bytes());
			buf.push(0);
			bufs.push(buf);
		}
		let mut ptrs = Vec::with_capacity(argc);
		for buf in &bufs {
			ptrs.push(buf.as_ptr());
		}
		ptrs.push(null());
		let ret = execve(path_cstr.as_slice(), ptrs.as_ptr(), null());
		if ret < 0 {
			eprintln!("shell: {}: {}", errno_to_str(ret),prog_name);
			exit(ret as u32);
		} 
	} else if p > 0 {
		let _ = waitid(0, 0, 0, 0);
		return Ok(());
	} else {
		eprintln!("shell: failed to fork");
		exit(p as u32);
	}
	Ok(())
}

fn run_builtin(env: &mut Env, args: &Vec<String>) -> Option<Result<(), i64>> {
	let cmd = args[0].clone();

	let res = match cmd.as_str() {
		"cd" => cd(env, &args),
		"clear" => clear(),
		"echo" => echo(env, &args),
		"exec" => exec(env, &args),
		"exit" => exit(0),
		"help" => {
			if args.len() == 1 {
				help(&String::new())
			} else {
				help(&args[1])
			}
		},
		"history" => history(env),
		//"ls" => ls(env, &parsed)?,
		"mkdir" => mkdir_wrapper(&args),
		"mkfs" => mkfs(&args),
		"mount" => builtins::mount(&args),
		"now" => now(),
		"pwd" => pwd(env),
		_ => {
			return None;
		}
	};
	Some(res)

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

fn mkdir_wrapper(arg: &Vec<String>) -> Result<(), i64> {
	if arg.len() < 2 {
		return Err(-22);
	}
	let ret = mkdir(arg[1].as_str());
	if ret < 0 {
		return Err(ret);
	}
	Ok(())
}

#[derive(Debug, Default)]
pub struct Env {
	pub pwd: String,
	pub history: Vec<String>,
	pub prompt: String,
	pub vars: BTreeMap<String, ShellVar>,
}

impl Env {
	pub fn update(&mut self) {
		self.prompt = String::from(format!("\x1b[92m-> \x1b[0m\x1b[94m{}\x1b[0m > ", self.pwd));
	}
	pub fn set_var(&mut self, var_name: String, var_value: ShellVar) {
		if let Some(var) = self.vars.get_mut(&var_name) {
			*var = var_value;
		}
	}
	pub fn get_var(&self, var_name: &String) -> Option<ShellVar> {
		if let Some(var) = self.vars.get(var_name) {
			return Some(var.clone());
		}
		None
	}
}

fn join_pwd(current: &str, arg: &str) -> String {
	let mut parts: alloc::vec::Vec<&str> = alloc::vec::Vec::new();
	if !arg.starts_with('/') {
		for p in current.split('/') {
			if !p.is_empty() { parts.push(p);}
		}
	}
	for p in arg.split('/') {
		match p {
			"" | "." => {}
			".." => { parts.pop(); }
			name => parts.push(name),
		}	
	} 
	if parts.is_empty() {
		return alloc::string::String::from("/");
	}
	let mut out = alloc::string::String::new();
	for p in parts {
		out.push('/');
		out.push_str(p);
	}
	out
}

#[derive(Debug, Clone)]
pub enum ShellVar {
	Int(i64),
	Str(String),
}
impl core::fmt::Display for ShellVar {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		match self {
			ShellVar::Int(n) => write!(f, "{}", n),
			ShellVar::Str(s) => write!(f, "{}", s),
		}
	}
}

fn replace_vars(env: &Env, args: &Vec<String>) -> Vec<String> {
	args.iter().map(|arg| expand_string(env, arg)).collect()
}

fn expand_string(env: &Env, input: &str) -> String {
	let mut res= String::with_capacity(input.len());
	let mut chars = input.chars().peekable();

	while let Some(c) = chars.next() {
		if c == '$' {
			let mut var_name = String::new();

			while let Some(&next_c) = chars.peek() {
				if next_c.is_ascii_alphanumeric() || next_c == '_' {
					var_name.push(chars.next().unwrap());
				} else {
					break;
				}
			}
			if var_name.is_empty() {
				res.push('$');
			} else if let Some(val) = env.get_var(&var_name) {
				res.push_str(&val.to_string());
			}
		} else {
			res.push(c);
		}
	}
	res
}

//x=y
fn set_var(env: &mut Env, args: &String) -> Result<(), i64> {
	if args.contains("=") {
		if let Some((var, val_str)) = args.split_once('=') {
			let val = val_str
				.parse::<i64>()
				.map(ShellVar::Int)
				.unwrap_or_else(|_| ShellVar::Str(val_str.to_string()));
			//Update existing
			if env.vars.contains_key(&var.to_string()) {
				env.set_var(var.to_string(), val);
			} else { // Create new var
				env.vars.insert(var.to_string(), val);
			}
			return Ok(());
		}
	}
	// goes here if arg doesn't contain '='
	Err(-22)
}

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
