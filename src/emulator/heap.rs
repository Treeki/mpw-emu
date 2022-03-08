use super::{EmuUC, UcResult, helpers::UnicornExtras};

use bitvec::prelude::*;
use unicorn_engine::unicorn_const::Permission;

const FREE_FLAG: u32 = 0x80000000;
const SIZE_OF_HEADER: u32 = 0x10;

const HDR_USER_SIZE: u32 = 0;
const HDR_BLOCK_SIZE: u32 = 4;
const HDR_PREV: u32 = 8;
const HDR_NEXT: u32 = 0xC;

pub struct Heap {
	used_handles: BitVec,
	region_start: u32,
	region_size: u32,
	handles_start: u32,
	handle_count: u32,
	arena_start: u32,
	arena_size: u32,
	first_block: u32,
	last_block: u32
}

impl Heap {
	pub fn new(region_start: u32, region_size: u32, handle_count: u32) -> Heap {
		let handles_size = handle_count * 4;

		Heap {
			used_handles: BitVec::new(),
			region_start,
			region_size,
			handles_start: region_start,
			handle_count,
			arena_start: region_start + handles_size,
			arena_size: region_size - handles_size,
			first_block: 0,
			last_block: 0
		}
	}

	pub(super) fn init(&mut self, uc: &mut EmuUC) -> UcResult<()> {
		self.used_handles.clear();
		self.used_handles.resize(self.handle_count as usize, false);

		uc.mem_map(self.region_start as u64, self.region_size as usize, Permission::ALL)?;

		// create one large block, covering the entire arena
		let block = self.arena_start;
		uc.write_u32(block + HDR_USER_SIZE, FREE_FLAG)?;
		uc.write_u32(block + HDR_BLOCK_SIZE, self.arena_size)?;
		uc.write_u32(block + HDR_PREV, 0)?;
		uc.write_u32(block + HDR_NEXT, 0)?;

		self.first_block = block;
		self.last_block = block;

		Ok(())
	}

	fn get_handle_index_if_valid(&self, handle: u32) -> Option<usize> {
		if handle >= self.handles_start && handle < (self.handles_start + self.handle_count * 4) {
			if (handle & 3) == 0 {
				let handle_index = (handle - self.handles_start) as usize / 4;
				if self.used_handles[handle_index] {
					return Some(handle_index);
				}
			}
		}
		None
	}

	pub(super) fn new_handle(&mut self, uc: &mut EmuUC, size: u32) -> UcResult<u32> {
		let handle_index = match self.used_handles.first_zero() {
			Some(i) => i,
			None => {
				error!(target: "heap", "out of memory handles!");
				return Ok(0);
			}
		};
		let handle = self.handles_start + 4 * (handle_index as u32);

		let backing_ptr = self.new_ptr(uc, size)?;
		if backing_ptr == 0 {
			return Ok(0);
		}

		self.used_handles.set(handle_index, true);

		uc.write_u32(handle, backing_ptr)?;
		Ok(handle)
	}

	pub(super) fn dispose_handle(&mut self, uc: &mut EmuUC, handle: u32) -> UcResult<()> {
		if handle == 0 { return Ok(()); }

		let handle_index = self.get_handle_index_if_valid(handle).expect("disposing invalid handle");
		let backing_ptr = uc.read_u32(handle)?;
		self.dispose_ptr(uc, backing_ptr)?;
		uc.write_u32(handle, 0)?;
		self.used_handles.set(handle_index, false);

		Ok(())
	}

	pub(super) fn get_handle_size(&self, uc: &EmuUC, handle: u32) -> UcResult<u32> {
		let _handle_index = self.get_handle_index_if_valid(handle).expect("getting size of invalid handle");
		let backing_ptr = uc.read_u32(handle)?;
		self.get_ptr_size(uc, backing_ptr)
	}

	pub(super) fn set_handle_size(&mut self, uc: &mut EmuUC, handle: u32, new_size: u32) -> UcResult<bool> {
		let _handle_index = self.get_handle_index_if_valid(handle).expect("setting size of invalid handle");
		let backing_ptr = uc.read_u32(handle)?;

		if self.set_ptr_size(uc, backing_ptr, new_size)? {
			// easy mode
			Ok(true)
		} else {
			// hard mode, allocate a new buffer
			let new_backing_ptr = self.new_ptr(uc, new_size)?;
			if new_backing_ptr != 0 {
				let old_size = self.get_ptr_size(uc, backing_ptr)?;
				let amount_to_copy = old_size.min(new_size);

				// copy data over
				let buffer = uc.mem_read_as_vec(backing_ptr.into(), amount_to_copy as usize)?;
				uc.mem_write(new_backing_ptr.into(), &buffer)?;

				if amount_to_copy < new_size {
					for i in amount_to_copy..new_size {
						uc.write_u8(new_backing_ptr + i, 0)?;
					}
				}

				self.dispose_ptr(uc, backing_ptr)?;
				uc.write_u32(handle, new_backing_ptr)?;
				Ok(true)
			} else {
				error!(target: "heap", "Could not resize handle to {new_size} bytes!");
				Ok(false)
			}
		}
	}

	pub(super) fn new_ptr(&mut self, uc: &mut EmuUC, size: u32) -> UcResult<u32> {
		let aligned_size = (size + 0xF) & !0xF;

		if let Some(block) = self.find_free_block(uc, aligned_size)? {
			let ptr = block + SIZE_OF_HEADER;
			uc.write_u32(block + HDR_USER_SIZE, size)?;
			self.shrink_used_block_by_splitting(uc, block)?;

			for i in 0..size {
				uc.write_u8(ptr + i, 0)?;
			}

			Ok(ptr)
		} else {
			error!(target: "heap", "Failed to allocate {size} bytes!");
			self.dump(uc)?;
			Ok(0)
		}
	}

	pub(super) fn dump(&self, uc: &EmuUC) -> UcResult<()> {
		let mut block = self.first_block;
		let mut index = 0;

		while block != 0 {
			let user_size = uc.read_u32(block + HDR_USER_SIZE)?;
			let block_size = uc.read_u32(block + HDR_BLOCK_SIZE)?;
			let next = uc.read_u32(block + HDR_NEXT)?;
			let end = block + block_size;
			if (user_size & FREE_FLAG) == FREE_FLAG {
				trace!(target: "heap", "#{index:4} {block:8X}-{end:8X} ---FREE---");
			} else {
				trace!(target: "heap", "#{index:4} {block:8X}-{end:8X} U:{user_size:8X}");
				if block_size <= 0x50 {
					let data = uc.mem_read_as_vec(block as u64, block_size as usize)?;
					trace!(target: "heap", "{data:?}");
				}
			}
			index += 1;
			block = next;
		}

		Ok(())
	}

	pub(super) fn dispose_ptr(&mut self, uc: &mut EmuUC, ptr: u32) -> UcResult<()> {
		let block = ptr - SIZE_OF_HEADER;
		let prev = uc.read_u32(block + HDR_PREV)?;
		let next = uc.read_u32(block + HDR_NEXT)?;

		uc.write_u32(block + HDR_USER_SIZE, FREE_FLAG)?;

		if next != 0 && self.is_block_free(uc, next)? {
			self.merge_blocks(uc, block, next)?;
		}
		if prev != 0 && self.is_block_free(uc, prev)? {
			self.merge_blocks(uc, prev, block)?;
		}

		Ok(())
	}

	pub(super) fn get_ptr_size(&self, uc: &EmuUC, ptr: u32) -> UcResult<u32> {
		let block = ptr - SIZE_OF_HEADER;
		let size = uc.read_u32(block + HDR_USER_SIZE)?;
		Ok(size)
	}

	pub(super) fn set_ptr_size(&mut self, uc: &mut EmuUC, ptr: u32, new_size: u32) -> UcResult<bool> {
		let block = ptr - SIZE_OF_HEADER;
		let current_size = uc.read_u32(block + HDR_USER_SIZE)?;

		// The simplest option
		if new_size == current_size { return Ok(true); }

		// Occupy all room up to the next used block
		let next = uc.read_u32(block + HDR_NEXT)?;
		if next != 0 && self.is_block_free(uc, next)? {
			self.merge_blocks(uc, block, next)?;
		}

		// Can we fit the desired size in?
		let max_size = uc.read_u32(block + HDR_BLOCK_SIZE)?;
		let success = if new_size < (max_size - SIZE_OF_HEADER) {
			uc.write_u32(block + HDR_USER_SIZE, new_size)?;

			if new_size > current_size {
				for i in current_size..new_size {
					uc.write_u8(ptr + i, 0)?;
				}
			}

			true
		} else {
			false
		};

		// Give space back
		self.shrink_used_block_by_splitting(uc, block)?;

		Ok(success)
	}

	fn is_block_free(&self, uc: &EmuUC, block: u32) -> UcResult<bool> {
		let user_size = uc.read_u32(block + HDR_USER_SIZE)?;
		Ok((user_size & FREE_FLAG) == FREE_FLAG)
	}

	fn find_free_block(&self, uc: &EmuUC, min_size: u32) -> UcResult<Option<u32>> {
		let mut block = self.last_block;

		while block != 0 {
			let block_size = uc.read_u32(block + HDR_BLOCK_SIZE)?;

			if self.is_block_free(uc, block)? && block_size >= (SIZE_OF_HEADER + min_size) {
				return Ok(Some(block));
			}

			block = uc.read_u32(block + HDR_PREV)?;
		}

		Ok(None)
	}

	fn shrink_used_block_by_splitting(&mut self, uc: &mut EmuUC, block: u32) -> UcResult<()> {
		assert_ne!(block, 0);

		let user_size = uc.read_u32(block + HDR_USER_SIZE)?;
		let block_size = uc.read_u32(block + HDR_BLOCK_SIZE)?;

		let min_block_size = SIZE_OF_HEADER + ((user_size + 0xF) & !0xF);
		let free_space = block_size - min_block_size;
		if free_space < (SIZE_OF_HEADER + 0x10) {
			// too small to bother splitting!
			return Ok(());
		}

		let next = uc.read_u32(block + HDR_NEXT)?;

		// Update used block
		uc.write_u32(block + HDR_BLOCK_SIZE, min_block_size)?;

		// Mark new block as free
		let second_block = block + min_block_size;
		uc.write_u32(second_block + HDR_USER_SIZE, FREE_FLAG)?;
		uc.write_u32(second_block + HDR_BLOCK_SIZE, free_space)?;

		// Set up linkages
		uc.write_u32(block + HDR_NEXT, second_block)?;
		uc.write_u32(second_block + HDR_PREV, block)?;
		uc.write_u32(second_block + HDR_NEXT, next)?;

		if next == 0 {
			self.last_block = second_block;
		} else {
			uc.write_u32(next + HDR_PREV, second_block)?;
		}

		Ok(())
	}

	fn merge_blocks(&mut self, uc: &mut EmuUC, a: u32, b: u32) -> UcResult<()> {
		assert_ne!(a, 0);
		assert_ne!(b, 0);

		let a_block_size = uc.read_u32(a + HDR_BLOCK_SIZE)?;
		let b_block_size = uc.read_u32(b + HDR_BLOCK_SIZE)?;
		assert_eq!(a + a_block_size, b);

		let combined_block_size = a_block_size + b_block_size;
		uc.write_u32(a + HDR_BLOCK_SIZE, combined_block_size)?;

		let next = uc.read_u32(b + HDR_NEXT)?;
		if next == 0 {
			self.last_block = a;
		} else {
			uc.write_u32(next + HDR_PREV, a)?;
		}
		uc.write_u32(a + HDR_NEXT, next)?;

		Ok(())
	}
}
