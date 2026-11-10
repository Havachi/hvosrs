use core::fmt;

use embedded_graphics::{draw_target::DrawTarget, geometry::Point, pixelcolor::{Rgb888, RgbColor}};
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
	bold: bool,
	default_fg: Rgb888,
	default_bg: Rgb888,
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
			bold: false,
			default_fg: Rgb888::WHITE,
			default_bg: Rgb888::BLACK,
		};
		let char_size = kc.output.character_size();
		let max_col = screen_width / char_size.width as u64;
		let max_lines = (screen_height / char_size.height as u64) -2;
		kc.max_col = max_col;
		kc.max_lines = max_lines;
		kc
	}

	pub fn write_char(&mut self, c: u8) {
		match self.ansi_state {
			AnsiState::Normal => {
				if c == b'\x1B' {
					self.ansi_state = AnsiState::Escaped;
					self.ansi_arg_len = 0;
				} else if c == b'\n' {
					self.new_line();
				}  else if c == b'\t' {
					self.tab();
				} else if c == 0x08 {
					self.backspace();
				} else if c < 0x20 || c == 0x7f {
				} else {
					let font_size = self.output.character_size();
					self.output.clear_char_cell(self.col, self.line);
					let pos = Point {
						x: ((self.col * font_size.width) as i32),
						y: ((self.line * font_size.height) as i32)
					};
					if let Ok(s) = str::from_utf8(&[c]) {
						let _ = self.output.write_str(s, pos);
					}
					self.col += 1;
					if self.col as u64 >= self.max_col {
						self.new_line();
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
		let len = self.ansi_arg_len.min(self.ansi_args.len());
		let mut args_buf = [0u8; 16];
		args_buf[..len].copy_from_slice(&self.ansi_args[..len]);
		let args = &args_buf[..len];

		match terminator {
			b'J' => {
				self.clear();
			}
			b'm' => {
				self.apply_sgr(args);
			}
			b'C' => {
				let n = Self::csi_count(args);
				let next = self.col as u64 + n as u64;
				self.col = next.min(self.max_col) as u32;
			}
			b'D' => {
				let n = Self::csi_count(args);
				self.col = self.col.saturating_sub(n).max(1);
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
			}
			_ => {}
		}
	}

	fn csi_count(args: &[u8]) -> u32 {
		let mut n: u32 = 0;
		let mut seen = false;
		for &b in args {
			if b.is_ascii_digit() {
				seen = true;
				n = n.saturating_mul(10).saturating_add((b - b'0') as u32);
			} else {
				break;
			}
		}
		if !seen || n == 0 { 1 } else { n }
	}

	fn apply_sgr(&mut self, args: &[u8]) {
		if args.is_empty() {
			self.apply_sgr_code(0);
			return;
		}
		let mut value: u16 = 0;
		let mut seen_digit = false;
		for &b in args {
			if b == b';' {
				let code = if seen_digit { value.min(255) as u8 } else { 0 };
				self.apply_sgr_code(code);
				value = 0;
				seen_digit = false;
			} else if b.is_ascii_digit() {
				seen_digit = true;
				value = value.saturating_mul(10).saturating_add((b - b'0') as u16);
			}
		}
		let code = if seen_digit { value.min(255) as u8 } else { 0 };
		self.apply_sgr_code(code);
	}

	fn apply_sgr_code(&mut self, code: u8) {
		match code {
			0 => {
				self.bold = false;
				self.output.fg_color = self.default_fg;
				self.output.bg_color = self.default_bg;
			}
			1 => self.bold = true,
			39 => self.output.fg_color = self.default_fg,
			49 => self.output.bg_color = self.default_bg,
			30..=37 => {
				self.output.fg_color = vga_color((code - 30) as usize, self.bold);
			}
			90..=97 => {
				self.output.fg_color = vga_color((code - 90) as usize, true);
			}
			40..=47 => {
				self.output.bg_color = vga_color((code - 40) as usize, false);
			}
			100..=107 => {
				self.output.bg_color = vga_color((code - 100) as usize, true);
			}
			_ => {}
		}
	}

	pub fn clear(&mut self) {
		self.output.clear(self.output.bg_color);
	}

	pub fn backspace(&mut self) {
		if self.col <= 1 {
			return;
		}
		self.col -= 1;
		self.output.clear_char_cell(self.col, self.line);
	}
	
	pub fn tab(&mut self) {
		let mut c = self.col.next_multiple_of(8);
		if c == self.col {
			c = (c + 1).next_multiple_of(8);
		}
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

pub fn _console_eprint(args: fmt::Arguments) {
	use fmt::Write;
	x86_64::instructions::interrupts::without_interrupts(|| {
		let mut l = KONSOLE.lock();
		let args = format_args!("\x1b[31m{}\x1b[0m", args);
		let _ = l.write_fmt(args);
	});
}

/// Raw byte output for syscalls (fd 1): framebuffer plus serial mirror for `-nographic`.
pub fn write_bytes(data: &[u8]) {
	let mut konsole = KONSOLE.lock();
	for &b in data {
		konsole.write_char(b);
		crate::drivers::serial::write_byte(b);
	}
}

fn vga_color(index: usize, bright: bool) -> Rgb888 {
	const NORMAL: [(u8, u8, u8); 8] = [
		(0, 0, 0),
		(170, 0, 0),
		(0, 170, 0),
		(170, 85, 0),
		(0, 0, 170),
		(170, 0, 170),
		(0, 170, 170),
		(170, 170, 170),
	];
	const BRIGHT: [(u8, u8, u8); 8] = [
		(85, 85, 85),
		(255, 85, 85),
		(85, 255, 85),
		(255, 255, 85),
		(85, 85, 255),
		(255, 85, 255),
		(85, 255, 255),
		(255, 255, 255),
	];
	let (r, g, b) = if bright { BRIGHT[index] } else { NORMAL[index] };
	Rgb888::new(r, g, b)
}
