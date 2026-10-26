pub struct Bitmap<'a> {
	bits: &'a mut [u64],
	total_bits: usize,
}

impl<'a> Bitmap<'a> {
	pub const fn new(bits: &'a mut [u64], total_bits: usize) -> Self {
		Self { bits, total_bits }
	}

	pub fn test(&self, bit: usize) -> bool {
		let array_idx = bit / 64;
		let bit_idx = bit % 64;
		(self.bits[array_idx] & (1 << bit_idx)) != 0
	}

	pub fn set(&mut self, bit: usize) {
		let array_idx = bit / 64;
		let bit_idx = bit % 64;
		self.bits[array_idx]  |= 1 << bit_idx;
	}

	pub fn clear(&mut self, bit: usize) {
		let array_idx = bit / 64;
		let bit_idx = bit % 64;
		self.bits[array_idx] &= !(1 << bit_idx);
	}

	pub fn alloc_contiguous(&mut self, n: usize) -> Option<usize> {
        if n == 0 || n > self.total_bits {
            return None;
        }

        let mut current_run = 0;
        let mut run_start = 0;

        for i in 0..self.total_bits {
            if !self.test(i) {
                if current_run == 0 {
                    run_start = i;
                }
                current_run += 1;

                if current_run == n {
                    for j in run_start..(run_start + n) {
                        self.set(j);
                    }
                    return Some(run_start);
                }
            } else {
                current_run = 0;
            }
        }

        None
    }

	pub fn free_contiguous(&mut self, start: usize, n: usize) {
        assert!(start + n <= self.total_bits, "Range out of bounds");
        for i in start..(start + n) {
            self.clear(i);
        }
    }
}

