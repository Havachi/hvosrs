use embedded_graphics::{mono_font::{MonoFont, MonoTextStyle}, pixelcolor::Rgb888, prelude::*, text::Text};
use x86_64::{PhysAddr, VirtAddr, structures::paging::{PhysFrame, Size4KiB}};

use crate::{
	arch::x86_64::paging::map_mmio,
	klog,
	memory::{
		DIRECT_MAP_RANGE, KCODE_RANGE, MMIO_RANGE, PAGE_SIZE,
	},
};

///This is only the graphics driver
pub struct KernelDisplay {
	address: *mut u32,
	width: u64,
	height: u64,
	pitch: u64,
	_bpp: u16,
	pub fg_color: Rgb888,
	pub bg_color: Rgb888,
	font: MonoFont<'static>,
}

unsafe impl Send for KernelDisplay {}
unsafe impl Sync for KernelDisplay {}

/// Map Limine's framebuffer into the kernel page table and build a display handle.
pub fn from_limine_framebuffer(value: &limine::framebuffer::Framebuffer) -> KernelDisplay {
	let raw = value.address() as u64;
	let address = kernel_framebuffer_ptr(value, raw);
	KernelDisplay {
		address,
		width: value.width,
		height: value.height,
		pitch: value.pitch,
		_bpp: value.bpp,
		fg_color: Rgb888::WHITE,
		bg_color: Rgb888::BLACK,
		font: ibm437::IBM437_9X14_REGULAR,
	}
}

fn kernel_framebuffer_ptr(fb: &limine::framebuffer::Framebuffer, raw: u64) -> *mut u32 {
	if DIRECT_MAP_RANGE.contains(raw) || MMIO_RANGE.contains(raw) || KCODE_RANGE.contains(raw) {
		return raw as *mut u32;
	}

	let byte_len = (fb.height * fb.pitch) as usize;
	let page_count = (byte_len + PAGE_SIZE - 1) / PAGE_SIZE;
	let page_off = (raw & (PAGE_SIZE as u64 - 1)) as usize;
	let phys_base = PhysAddr::new(raw - page_off as u64);

	let mut virt_base = VirtAddr::new(0);
	for i in 0..page_count {
		let frame = PhysFrame::<Size4KiB>::from_start_address(phys_base + (i * PAGE_SIZE) as u64)
			.expect("framebuffer phys not aligned");
		let virt = unsafe { map_mmio(frame) };
		if i == 0 {
			virt_base = virt;
		}
	}

	unsafe { virt_base.as_mut_ptr::<u8>().add(page_off) as *mut u32 }
}

impl From<&limine::framebuffer::Framebuffer> for KernelDisplay {
	fn from(value: &limine::framebuffer::Framebuffer) -> Self {
		from_limine_framebuffer(value)
	}
}

impl KernelDisplay {
	fn pixel_ptr(&self, x: u64, y: u64) -> *mut u32 {
		let stride = (self.pitch / 4) as usize;
		unsafe { self.address.add(y as usize * stride + x as usize) }
	}

	pub fn write_str(&mut self, s: &str, pos: Point) -> core::fmt::Result {
		let font = self.font;
		let text_style = MonoTextStyle::new(&font, self.fg_color);
		Text::with_alignment(
			s, 
			pos,
			text_style,
			embedded_graphics::text::Alignment::Left,
		).draw(self);
		Ok(())
	}

	pub fn character_size(&self) -> Size {
		self.font.character_size
	}

	/// Draw one ASCII byte into the framebuffer (syscall-safe; no full-FB slice).
	#[inline(never)]
	pub fn draw_ascii_byte(&mut self, c: u8, col: u32, row: u32) {
		if c < 0x20 || c == 0x7f {
			return;
		}
		let fs = self.character_size();
		let px0 = col as u64 * fs.width as u64;
		let py0 = row as u64 * fs.height as u64;
		let fg = color_to_u32(self.fg_color);
		let bg = color_to_u32(self.bg_color);
		let data = ibm437::IBM437_9X14_REGULAR_DATA;
		// ASCII maps 1:1 in IBM437; avoid `char_offset_impl` on the syscall stack.
		let idx = if c < 0x80 {
			c as usize
		} else {
			b'?' as usize
		};
		let glyph_col = idx % ibm437::CHARS_PER_ROW;
		let glyph_row = idx / ibm437::CHARS_PER_ROW;
		let row_bits = ibm437::CHARS_PER_ROW * 9;

		for gy in 0..14usize {
			for gx in 0..9usize {
				let bit_index = (glyph_row * 14 + gy) * row_bits + glyph_col * 9 + gx;
				if bit_index / 8 >= data.len() {
					continue;
				}
				let on = (data[bit_index / 8] >> (7 - bit_index % 8)) & 1 != 0;
				let color = if on { fg } else { bg };
				let x = px0 + gx as u64;
				let y = py0 + gy as u64;
				if x >= self.width || y >= self.height {
					continue;
				}
				unsafe {
					self.pixel_ptr(x, y).write_volatile(color);
				}
			}
		}
	}

	pub fn scroll_up(&mut self, nb_lines: u32) {
		if nb_lines == 0 { return; }
		let char_size = self.character_size();
		let shift_pixels = nb_lines * char_size.height;
		let height = self.height as usize;
		let width = self.width as usize;

		let stride = if self.pitch > 0 {
			(self.pitch / 4) as usize
		} else {
			width
		};

		if shift_pixels as usize >= height {
			self.clear(self.bg_color);
			return;
		}

		let line_to_move = height - shift_pixels as usize;
		let ptr = self.address;

		let bg_val = color_to_u32(self.bg_color);
		unsafe {
			for y in 0..line_to_move {
				let srcy = y + shift_pixels as usize;
				let src_ptr = ptr.add(srcy * stride);
				let dst_ptr = ptr.add(y * stride);
				core::ptr::copy(src_ptr, dst_ptr, width);
			}
		
			for y in line_to_move..height {
				let row_ptr = ptr.add(y * width);
				for x in 0..width {
					row_ptr.add(x).write(bg_val);
				}
			}
		}
	}

}

impl DrawTarget for KernelDisplay {
	type Color = Rgb888;
	type Error = core::convert::Infallible;

	fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
	where
		I: IntoIterator<Item = embedded_graphics::prelude::Pixel<Self::Color>>
	{
		for Pixel(coord, color) in pixels.into_iter() {
			let addr = self.pixel_ptr(coord.x as u64, coord.y as u64);
			let val = color_to_u32(color);
			unsafe { addr.write_volatile(val) };
		}
		Ok(())
	}
	
}

impl OriginDimensions for KernelDisplay {
	fn size(&self) -> Size {
		Size::new(self.width as u32, self.height as u32)
	}
}

fn color_to_u32(color: Rgb888) -> u32 {
	let r = color.r() as u32;
	let g = color.g() as u32;
	let b = color.b() as u32;
	(r << 16) + (g << 8) + b
}
