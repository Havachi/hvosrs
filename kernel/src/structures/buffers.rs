use crate::errno::KernelErrors;


pub struct CircularBuffer<T, const N:usize> {
	buffer: [T; N],
	head: usize,
	tail: usize,
	size: usize,
}

impl<const N:usize> CircularBuffer<u8, N>
{
	pub const fn new() -> Self {
		Self {
			head: 0,
			tail: 0,
			buffer:[0; N],
			size: 0,
		}
	}

	pub fn push(&mut self, val: u8) -> Result<(), KernelErrors> {
		if self.is_full() {
			return Err(KernelErrors::EAGAIN);
		}
		self.buffer[self.tail] = val;
		self.tail = (self.tail + 1) % N;
		self.size += 1;
		return Ok(());
	}

	pub fn pop(&mut self) -> Result<u8, KernelErrors> {
		if self.is_empty() {
			return Err(KernelErrors::EAGAIN);
		}
		let val = self.buffer[self.head];
		self.head = (self.head + 1) % N;
		self.size -= 1;
		return Ok(val);
	}

	pub fn peek(&self) -> Result<u8, KernelErrors> {
		if self.is_empty() {
			return Err(KernelErrors::EAGAIN);
		}
		return Ok(self.buffer[self.head]);
	}

	pub fn buffer_len(&self) -> usize {
		self.size
	}

	pub fn is_empty(&self) -> bool {
		self.size == 0
	}

	pub fn is_full(&self) -> bool {
		self.size == N
	}
}