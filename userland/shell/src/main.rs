#![no_std]
#![no_main]

use core::sync::atomic::AtomicI64;

use alloc::{collections::BTreeMap, format, string::{String, ToString}, vec::Vec};
use uhv::*;
extern crate alloc;
pub static SHELL_STATUS: AtomicI64 = AtomicI64::new(0);

pub fn init(env: &mut Env) {
	env.pwd = String::from("/");
	env.history = Vec::new();
	env.prompt = String::from(format!("\x1b[92m-> \x1b[0m\x1b[94m{}\x1b[0m > ", env.pwd));
	env.vars = BTreeMap::new();
	env.vars.insert(String::from("?"), ShellVar::Int(0));
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
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

	match cmd.as_str() {
		"cd" => cd(env, &parsed)?,
		"echo" => echo(env, &parsed)?,
		"exit" => exit(0),
		"help" => help(),
		"history" => history(env),
		"ls" => ls(env, &parsed)?,
		"mkdir" => mkdir_wrapper(&parsed)?,
		"pwd" => pwd(env)?,

		_ => {
			if args[0].contains('=') {
				return set_var(env, &parsed[0]);
			}
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
	env.update();
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

fn history(env: &Env) {
	if env.history.len() == 0 {
		println!("history is empty");
		return;
	}
	for (i, line) in env.history.iter().enumerate() {
		println!("{} {}", i, line);
	}
}

fn help() {
	println!("command available:");
	println!("cd <dir>\t| Change directory");
	println!("exit\t\t| Exit current program");
	println!("help\t\t| This");
	println!("history\t| Display command history");
	println!("ls [dir]\t| List file and directories");
	println!("pwd\t\t| Show current directory");
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

fn get_var(env: &Env, arg: &String) -> Result<Option<ShellVar>, i64> {
	if arg.starts_with("$") {
		//skip the $
		let varname = &arg[1..];
		//Variable exists
		if let Some(val) = env.vars.get(&varname.to_string()) {
			return Ok(Some(val.clone()))
		} else {//Variable doesn't exists
			return Ok(None);
		}
	}
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
