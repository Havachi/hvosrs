pub mod key;

use spin::Mutex;
use x86_64::instructions::port::Port;

use crate::{drivers::keyboard::key::{KeyCode, map_scancode}, klog, kprint};

pub static KEYBOARD: Mutex<KeyboardState> = Mutex::new(KeyboardState::new());

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyState{
	Pressed,
	Released
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
	pub key: KeyCode,
	pub state: KeyState,
}

pub fn init() {
	unsafe {
		let mut cmd_port = Port::<u8>::new(0x64);
		while (cmd_port.read() & 2) != 0 {}
		cmd_port.write(0xAE);
	}
	klog!("PS/2 Keyboard driver init done");
}

pub fn handle_scancode(scancode: u8) {
	let mut kbd = KEYBOARD.lock();

	if let Some(keyevent) = kbd.process_scancode(scancode) {
		let state = keyevent.state;
		if state == KeyState::Released {
			match keyevent.key {
				KeyCode::LShift | KeyCode::RShift => {kbd.shift_pressed = false}, 
				KeyCode::LControl | KeyCode::RControl  => {kbd.ctrl_pressed = false},
				KeyCode::LAlt  => {kbd.alt_pressed = false},
				_ => {}
			}
		} else {
			match keyevent.key {
				KeyCode::CapsLock						=> kbd.caps_locked = !kbd.caps_locked,
				KeyCode::NumpadLock						=> kbd.num_locked = !kbd.num_locked ,
				KeyCode::ScrollLock						=> kbd.scroll_locked = !kbd.scroll_locked,
				KeyCode::LShift | KeyCode::RShift		=> kbd.shift_pressed = true, 
				KeyCode::LControl | KeyCode::RControl	=> kbd.ctrl_pressed = true,
				KeyCode::LAlt							=> kbd.alt_pressed = true,
				_ => {
					let c = kbd.to_ascii(keyevent.key);
					kprint!("{c}");
				}
			}
		}
	}
}


pub struct KeyboardState {
	pub shift_pressed: bool,
	pub ctrl_pressed: bool,
	pub alt_pressed: bool,
	pub caps_locked: bool,
	pub num_locked: bool,
	pub scroll_locked: bool,
}

impl KeyboardState {
	pub const fn new() -> Self {
		Self { shift_pressed: false, ctrl_pressed: false, alt_pressed: false, caps_locked: false, num_locked: false, scroll_locked: false }
	}

	pub fn process_scancode(&mut self, scancode: u8) -> Option<KeyEvent> {
		let is_released = (scancode & 0x80) != 0;
		let base_code = scancode & 0x7F;
		let state = if is_released { KeyState::Released } else { KeyState::Pressed };
		if let Some(key) = map_scancode(base_code) {
			Some(KeyEvent { key, state })
		} else {
			None
		}
	}

	fn to_ascii(&self, base_code: KeyCode) -> char {
		let upper = self.shift_pressed ^ self.caps_locked;
		match base_code {

			//Numbers
			KeyCode::Key1 => if self.shift_pressed {'!'} else {'1'},
			KeyCode::Key2 => if self.shift_pressed {'@'} else {'2'},
			KeyCode::Key3 => if self.shift_pressed {'#'} else {'3'},
			KeyCode::Key4 => if self.shift_pressed {'$'} else {'4'},
			KeyCode::Key5 => if self.shift_pressed {'%'} else {'5'},
			KeyCode::Key6 => if self.shift_pressed {'^'} else {'6'},
			KeyCode::Key7 => if self.shift_pressed {'&'} else {'7'},
			KeyCode::Key8 => if self.shift_pressed {'*'} else {'8'},
			KeyCode::Key9 => if self.shift_pressed {'('} else {'9'},
			KeyCode::Key0 => if self.shift_pressed {')'} else {'0'},

			//Letters
            KeyCode::A => if upper { 'A' } else { 'a' },
            KeyCode::B => if upper { 'B' } else { 'b' },
            KeyCode::C => if upper { 'C' } else { 'c' },
            KeyCode::D => if upper { 'D' } else { 'd' },
            KeyCode::E => if upper { 'E' } else { 'e' },
            KeyCode::F => if upper { 'F' } else { 'f' },
            KeyCode::G => if upper { 'G' } else { 'g' },
            KeyCode::H => if upper { 'H' } else { 'h' },
            KeyCode::I => if upper { 'I' } else { 'i' },
            KeyCode::J => if upper { 'J' } else { 'j' },
            KeyCode::K => if upper { 'K' } else { 'k' },
            KeyCode::L => if upper { 'L' } else { 'l' },
            KeyCode::M => if upper { 'M' } else { 'm' },
            KeyCode::N => if upper { 'N' } else { 'n' },
			KeyCode::O => if upper { 'O' } else { 'o' },
            KeyCode::P => if upper { 'P' } else { 'p' },
            KeyCode::Q => if upper { 'Q' } else { 'q' },
            KeyCode::R => if upper { 'R' } else { 'r' },
            KeyCode::S => if upper { 'S' } else { 's' },
            KeyCode::T => if upper { 'T' } else { 't' },
            KeyCode::U => if upper { 'U' } else { 'u' },
            KeyCode::V => if upper { 'V' } else { 'v' },
            KeyCode::W => if upper { 'W' } else { 'w' },
            KeyCode::X => if upper { 'X' } else { 'x' },
            KeyCode::Y => if upper { 'Y' } else { 'y' },
            KeyCode::Z => if upper { 'Z' } else { 'z' },

			//Special Characters
			KeyCode::OemMinus => if self.shift_pressed { '_' } else { '-' }
			KeyCode::OemEqual => if self.shift_pressed { '+' } else { '=' }
			KeyCode::OemComma => if self.shift_pressed { '<' } else { ',' }
			KeyCode::OemPeriod => if self.shift_pressed { '>' } else { '.' }
            KeyCode::Oem1 => if self.shift_pressed { ':' } else { ';' },
            KeyCode::Oem2 => if self.shift_pressed { '?' } else { '/' },
            KeyCode::Oem3 => if self.shift_pressed { '"' } else { '\'' },
            KeyCode::Oem4 => if self.shift_pressed { '{' } else { '[' },
            KeyCode::Oem6 => if self.shift_pressed { '}' } else { ']' },
            KeyCode::Oem7 => if self.shift_pressed { '|' } else { '\\' },
			KeyCode::Oem8 => if self.shift_pressed {'~'} else {'`'},

			//Numpad fun
			KeyCode::NumpadDivide => '/',
			KeyCode::NumpadMultiply => '*',
			KeyCode::NumpadSubstract => '-',
			KeyCode::NumpadAdd => '+',
			KeyCode::NumpadEnter => '\n',
			KeyCode::Numpad0 => if self.num_locked {'0'} else {'\0'},
			KeyCode::Numpad1 => if self.num_locked {'1'} else {'\0'},
			KeyCode::Numpad2 => if self.num_locked {'2'} else {'\0'},
			KeyCode::Numpad3 => if self.num_locked {'3'} else {'\0'},
			KeyCode::Numpad4 => if self.num_locked {'4'} else {'\0'},
			KeyCode::Numpad5 => if self.num_locked {'5'} else {'\0'},
			KeyCode::Numpad6 => if self.num_locked {'6'} else {'\0'},
			KeyCode::Numpad7 => if self.num_locked {'7'} else {'\0'},
			KeyCode::Numpad8 => if self.num_locked {'8'} else {'\0'},
			KeyCode::Numpad9 => if self.num_locked {'9'} else {'\0'},
			KeyCode::NumpadPeriod => if self.num_locked {'.'} else {'\0'},

			//Special Keys
			KeyCode::Backspace => 0x08 as char,
			KeyCode::Spacebar => ' ',
			KeyCode::Return => '\n',
			KeyCode::Tab => '\t',
			KeyCode::Oem5 => '\0',

			_ => '\0',
		}
	}
}