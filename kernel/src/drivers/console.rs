use core::fmt;

use embedded_graphics::{draw_target::DrawTarget, geometry::Point};
use lazyinit::LazyInit;
use spin::Mutex;

use crate::{drivers::display::KernelDisplay, klog};

pub static KONSOLE: Mutex<lazyinit::LazyInit<KernelConsole>> = Mutex::new(LazyInit::new()); 


pub fn init(fb: &limine::framebuffer::Framebuffer) {
	KONSOLE.lock().call_once(|| {
		KernelConsole::new(fb)
	});
	klog!("kernel console init done")
}

#[derive(Clone, Copy, Debug)]
enum AnsiState {
	Normal,
	Escaped,
	Csi
}

pub struct KernelConsole{
	output: KernelDisplay,
	col: u32,
	line: u32,
	max_col: u64,
	max_lines: u64,
	ansi_state: AnsiState,
	ansi_args: [u8; 16],
	ansi_arg_len: usize,
}

impl KernelConsole {
	pub fn new(limine_fb: &limine::framebuffer::Framebuffer) -> Self {
		let screen_width = limine_fb.width;
		let screen_height = limine_fb.height;
		let mut kc = Self {
			output: KernelDisplay::from(limine_fb),
			col: 1,
			line: 1,
			max_col: 0,
			max_lines: 0,
			ansi_state: AnsiState::Normal,
			ansi_args: [0; 16],
			ansi_arg_len: 0,
		};
		let char_size = kc.output.character_size();
		let max_col = screen_width / char_size.width as u64;
		let max_lines = (screen_height / char_size.height as u64) -2;
		kc.max_col = max_col;
		kc.max_lines = max_lines;
		kc
	}

	/// Raw byte output for syscalls (no ANSI); minimal stack use.
	pub fn write_raw_byte(&mut self, c: u8) {
		if c == b'\n' {
			self.new_line();
			return;
		}
		if c < 0x20 || c == 0x7f {
			return;
		}
		self.output.draw_ascii_byte(c, self.col, self.line);
		self.col += 1;
		if self.col as u64 >= self.max_col {
			self.new_line();
		}
	}

	pub fn write_char(&mut self, c: u8) {


		match self.ansi_state {
			AnsiState::Normal => {
				if c == b'\x1B' {
					self.ansi_state = AnsiState::Escaped;
					self.ansi_arg_len = 0;
				} else if c == b'\n' {
					self.new_line();
				} else {
					let font_size = self.output.character_size();
					let pos = Point {
						x: ((self.col * font_size.width) as i32),
						y: ((self.line * font_size.height) as i32)
					};
					self.col += 1;
					if let Ok(s) = str::from_utf8(&[c]) {
						let _ = self.output.write_str(s, pos);
					}
				}
			},
			AnsiState::Escaped => {
				if c == b'[' {
					self.ansi_state = AnsiState::Csi;
				} else {
					self.ansi_state = AnsiState::Normal;
				}
			},
			AnsiState::Csi => {
				if c >= 64 && c <= 126 {
					self.execute_ansi_control(c);
					self.ansi_state = AnsiState::Normal;
				} else {
					if self.ansi_arg_len < self.ansi_args.len() {
						self.ansi_args[self.ansi_arg_len] = c;
						self.ansi_arg_len += 1;
					}
				}
			},
		}

	}

	fn execute_ansi_control(&mut self, terminator: u8) {
		let args = &self.ansi_args[..self.ansi_arg_len];

		match terminator {
			b'J' => {
				self.clear();
				return;
			}
			b'm' => {

			}
			b'H' => {
				let mut c = 1;
				let mut l = 1;
				if !args.is_empty() {
					if let Some(idx) = args.iter().position(|c| *c == b';') {
						let (line_raw, col_raw_with_sep) = args.split_at(idx);
						let col_raw = &col_raw_with_sep[1..];
						let line_str = str::from_utf8(line_raw)
							.map_err(|_| "Invalid utf8").unwrap_or("1");
						l = line_str.trim().parse::<u32>().map_err(|_| "i32 parsing failed" ).unwrap_or(1);
						let col_str = str::from_utf8(col_raw)
							.map_err(|_| "Invalid utf8").unwrap_or("1");
						c = col_str.trim().parse::<u32>().map_err(|_| "i32 parsing failed" ).unwrap_or(1);
					}
				}
				self.col = c;
				self.line = l;
				return;
			}
			_ => {

			}
 		}
		
	}

	pub fn clear(&mut self) {
		self.output.clear(self.output.bg_color);
	}

	pub fn backspace(&mut self) {
	}
	
	pub fn tab(&mut self) {
		let c = self.col.next_multiple_of(8);
		if c > self.max_col as u32 {
			self.new_line();
		} else {
			self.col = c;
		}
	}

	pub fn new_line(&mut self) {
		self.line += 1;
		self.col = 1;

		if self.line >= self.max_lines as u32 {
			self.scroll_up();
		}
	}

	pub fn scroll_up(&mut self) {
		self.line = (self.max_lines - 1) as u32;
		self.col = 1;
		self.output.scroll_up(1);
	}

	pub fn reset_cursor(&mut self) {
		self.col = 1;
		self.line = 1;
	}
}

impl core::fmt::Write for KernelConsole {
	fn write_str(&mut self, s: &str) -> core::fmt::Result {
		for c in s.bytes() {
			self.write_char(c);
		}
		Ok(())
	}
}

pub fn _console_print(args: fmt::Arguments) {
	use fmt::Write;
	x86_64::instructions::interrupts::without_interrupts(|| {
		let mut l = KONSOLE.lock();
		let _ = l.write_fmt(args);
	});
}

/// Raw byte output for syscalls (fd 1): framebuffer plus serial mirror for `-nographic`.
pub fn write_bytes(data: &[u8]) {
	let mut konsole = KONSOLE.lock();
	for &b in data {
		konsole.write_raw_byte(b);
		crate::drivers::serial::write_byte(b);
	}
}