#![no_std]
#![no_main]

pub mod builtins;
use core::ptr::null;

use builtins::*;

use alloc::{collections::BTreeMap, format, string::{String, ToString}, vec::{self, Vec}};
use uhv::*;
extern crate alloc;

pub fn init(env: &mut Env) {
	env.history = Vec::new();
	env.prompt = String::from(format!("\x1b[92m-> \x1b[0m\x1b[94m{}\x1b[0m > ", String::from("/")));
	env.vars = BTreeMap::new();
	env.vars.insert(String::from("?"), ShellVar::Int(0));
	env.vars.insert(String::from("PATH"), ShellVar::Str(String::from("/bin")));
	env.vars.insert(String::from("PWD"), ShellVar::Str(String::from("/")));

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

	//contains a pipe, so go in pipe mode
	if buf.contains(&b'|') {
		let mut pipeidx = 0;
		for i in 0..parsed.len() {
			if parsed[i] == "|".to_string(){
				pipeidx = i;
				break;
			}
		}
		if pipeidx == 0 || pipeidx + 1 >= parsed.len() {
			return Err(127);
		}

		let mut full_left_cmd = Vec::from(&parsed[..pipeidx]);
		let mut full_right_cmd = Vec::from(&parsed[pipeidx+1..]);
		full_left_cmd[0] = resolve_prog_path(env, &full_left_cmd[0])?;
		full_right_cmd[0] = resolve_prog_path(env, &full_right_cmd[0])?;

		let mut fds = [0i32; 2];
		let ret = pipe(&mut fds);
		if ret < 0 {
			return Err(ret);
		}
		let read_fd = fds[0] as i64;
		let write_fd = fds[1] as i64;

		let fret1 = fork();
		if fret1 == 0 {
			let dupret = dup2(write_fd as u64, 1);
			if dupret < 0 {
				eprintln!("error during dup: {}", errno_to_str(dupret));
				exit(1);
			}
			close(read_fd);
			close(write_fd);
			exec_program(env, &full_left_cmd[0], &full_left_cmd);
		} else if fret1 > 0 {
			let fret2 = fork();
			if fret2 == 0 {
				let dupret = dup2(read_fd as u64, 0);
				if dupret < 0 {
					eprintln!("error during dup: {}", errno_to_str(dupret));
					exit(1);
				}
				close(read_fd);
				close(write_fd);
				exec_program(env, &full_right_cmd[0], &full_right_cmd);
			}
			close(read_fd);
			close(write_fd);
			let _ = waitid(0, 0, 0, 0);
			let _ = waitid(0, 0, 0, 0);
			return Ok(());
		}
		eprintln!("shell: failed to fork");
		return Err(fret1);
	}

	let path = match  resolve_prog_path(env, &args[0]) {
		Ok(p) => p,
		Err(_) => {
			eprintln!("shell: command not found: {}", cmd);
			return Err(127);
		}
	};
	spawn(env, &path, &parsed)
}

fn resolve_prog_path(env: &Env, cmd: &String) -> Result<String, i64> {
	if cmd.contains("/") {
		let res = open(cmd.as_bytes(), 0);
		if res > 0 {
			close(res);
			return Ok(String::from(cmd.clone()));
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
				try_resolve_path.push_str(&cmd);
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

fn spawn(env: &Env, path: &str, args: &Vec<String>) -> Result<(), i64> {
	let p = fork();
	if p == 0 {
		exec_program(env, path, args);
	} else if p > 0 {
		let _ = waitid(0, 0, 0, 0);
		return Ok(());
	} else {
		eprintln!("shell: failed to fork");
		exit(p as u32);
	}
}

/// Owned `execve` arguments. The pointer vectors alias `argv_bufs` and `env_bufs`,
/// so those buffers stay in this struct for the call.
struct ExecFrame {
	path: Vec<u8>,
	argv_bufs: Vec<Vec<u8>>,
	argv: Vec<*const u8>,
	env_bufs: Vec<Vec<u8>>,
	envp: Vec<*const u8>,
}

impl ExecFrame {
	fn new(env: &Env, path: &str, args: &[String]) -> Self {
		let mut path_buf = Vec::with_capacity(path.len() + 1);
		path_buf.extend_from_slice(path.as_bytes());
		path_buf.push(0);

		let mut argv_bufs = Vec::with_capacity(args.len());
		for arg in args {
			let mut buf = Vec::with_capacity(arg.len() + 1);
			buf.extend_from_slice(arg.as_bytes());
			buf.push(0);
			argv_bufs.push(buf);
		}
		let mut argv = Vec::with_capacity(args.len() + 1);
		for buf in &argv_bufs {
			argv.push(buf.as_ptr());
		}
		argv.push(null());

		let mut env_bufs = Vec::with_capacity(env.vars.len());
		for (name, value) in &env.vars {
			let mut buf = Vec::new();
			buf.extend_from_slice(name.as_bytes());
			buf.push(b'=');
			match value {
				ShellVar::Int(i) => buf.extend_from_slice(i.to_string().as_bytes()),
				ShellVar::Str(s) => buf.extend_from_slice(s.as_bytes()),
			}
			buf.push(0);
			env_bufs.push(buf);
		}
		let mut envp = Vec::with_capacity(env_bufs.len() + 1);
		for buf in &env_bufs {
			envp.push(buf.as_ptr());
		}
		envp.push(null());

		Self { path: path_buf, argv_bufs, argv, env_bufs, envp }
	}
}

fn exec_program(env: &Env, path: &str, args: &[String]) -> ! {
	let frame = ExecFrame::new(env, path, args);
	let ret = execve(frame.path.as_slice(), frame.argv.as_ptr(), frame.envp.as_ptr());
	eprintln!("shell: {}: {}", errno_to_str(ret), path);
	exit(ret as u32);
}

///Return None if the command was not found in the builtin command list
/// 
///Return Some(Ok) if the command was a builtin and return successfully
/// 
///Return Some(Err) if the command was a builtin and return an error
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
	pub history: Vec<String>,
	pub prompt: String,
	pub vars: BTreeMap<String, ShellVar>,
}

impl Env {
	pub fn update(&mut self) {
		if let Some(ShellVar::Str(pwd)) = self.get_var(&"PWD".to_string()) {
			self.prompt = String::from(format!("\x1b[92m-> \x1b[0m\x1b[94m{}\x1b[0m > ", pwd));
		} else {
			self.prompt = String::from(format!("\x1b[92m-> \x1b[0m\x1b[94m\x1b[0m > ",));
		};
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
