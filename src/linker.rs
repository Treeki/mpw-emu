use super::pef;

pub struct Executable {
	pub memory: Vec<u8>,
	pub memory_base: u32,
	pub code_addr: u32,
	pub data_addr: u32,
	pub stack_addr: u32,
	pub stack_size: u32,
	pub init_vector: u32,
	pub main_vector: u32,
	pub shim_addrs: Vec<u32>,
	pub imports: Vec<pef::ImportedSymbol>,
	pub libraries: Vec<String>
}

impl Executable {
	pub fn new() -> Self {
		Executable {
			memory: Vec::new(),
			memory_base: 0x10000000,
			code_addr: 0,
			data_addr: 0,
			stack_addr: 0,
			stack_size: 0,
			init_vector: 0,
			main_vector: 0,
			shim_addrs: Vec::new(),
			imports: Vec::new(),
			libraries: Vec::new()
		}
	}

	pub fn memory_end_addr(&self) -> u32 {
		self.memory_base + (self.memory.len() as u32)
	}
	
	fn allocate_memory(&mut self, amount: usize) -> u32 {
		let addr = self.memory_end_addr();
		self.memory.resize(self.memory.len() + amount, 0);
		addr
	}

	fn align_memory_to(&mut self, amount: usize) {
		while (self.memory.len() & (amount - 1)) != 0 {
			self.memory.push(0);
		}
	}

	pub fn load_pef(&mut self, pef: pef::PEF) {
		for section in &pef.sections {
			debug!(
				target: "linker",
				"Section: {:?} Default={:X} Size(Total={:X}, Unpacked={:X}, Packed={:X}) Kind(Section={:?}, Share={:?}) Align={:?}",
				section.name, section.default_address,
				section.total_size, section.unpacked_size, section.packed_size,
				section.section_kind, section.share_kind, section.alignment);
		}

		self.memory.clear();

		// Load code
		self.code_addr = self.memory_end_addr();
		self.memory.extend_from_slice(pef.sections[0].packed_contents.as_ref().unwrap());
		self.align_memory_to(0x10);

		// Load data
		let data_start = self.memory.len();
		self.data_addr = self.allocate_memory(pef.sections[1].total_size as usize);
		let data_end = self.memory.len();
		pef::unpack_pattern_data(pef.sections[1].packed_contents.as_ref().unwrap(), &mut self.memory[data_start .. data_end]);

		self.align_memory_to(0x10);

		// Create a stack
		self.stack_size = 0x100000;
		self.stack_addr = self.allocate_memory(self.stack_size as usize);

		// Parse loader
		let loader = pef::parse_loader(pef.sections[2].packed_contents.as_ref().unwrap()).unwrap();
		if loader.init_section > 0 {
			self.init_vector = self.data_addr + loader.init_offset;
		}
		if loader.main_section > 0 {
			self.main_vector = self.data_addr + loader.main_offset;
		}

		// Create shims for imported symbols
		let sc_thunk = self.memory_end_addr();
		self.memory.push(0x44);
		self.memory.push(0);
		self.memory.push(0);
		self.memory.push(2);
		self.memory.push(0x4E);
		self.memory.push(0x80);
		self.memory.push(0);
		self.memory.push(0x20);
		self.memory.push(0x4E); // double to work around unicorn merging https://github.com/unicorn-engine/unicorn/pull/1558
		self.memory.push(0x80);
		self.memory.push(0);
		self.memory.push(0x20);

		for (i, sym) in loader.imported_symbols.iter().enumerate() {
			match sym.class {
				pef::SymbolClass::TVect => {
					let shim = self.allocate_memory(8);
					self.set_u32(shim, sc_thunk);
					self.set_u32(shim + 4, i as u32);
					self.shim_addrs.push(shim);
				}
				pef::SymbolClass::Data => {
					let shim = self.allocate_memory(1024);
					self.shim_addrs.push(shim);
				}
				_ => panic!()
			}
		}

		for reloc_section in &loader.reloc_sections {
			self.handle_reloc_section(&loader, reloc_section);
		}

		self.imports = loader.imported_symbols;
		for lib in loader.imported_libraries {
			self.libraries.push(lib.name);
		}
	}

	pub fn get_u32(&self, address: u32) -> u32 {
		let offset = (address - self.memory_base) as usize;
		let bytes: [u8; 4] = self.memory[offset .. offset + 4].try_into().unwrap();
		u32::from_be_bytes(bytes)
	}

	pub fn set_u32(&mut self, address: u32, value: u32) {
		let offset = (address - self.memory_base) as usize;
		self.memory[offset .. offset + 4].copy_from_slice(&value.to_be_bytes());
	}

	fn handle_reloc_section(&mut self, loader: &pef::Loader, relocs: &pef::RelocSection) {
		let mut next_block = 0;
		let mut reloc_address = self.data_addr;
		let mut import_index = 0u32;
		let mut repeat_info = None;

		while next_block < relocs.data.len() {
			let block_pos = next_block;
			let block = relocs.data[next_block] as u32;
			next_block += 1;
	
			if (block & 0xC000) == 0 {
				// RelocBySectDWithSkip
				let skip_count = (block >> 6) & 0xFF;
				let reloc_count = block & 0x3F;
				reloc_address += skip_count * 4;
				trace!(target: "linker", "[{block_pos:04X}] BySectDWithSkip @ {reloc_address:X} (x{reloc_count})");
				for _ in 0..reloc_count {
					self.set_u32(reloc_address, self.data_addr + self.get_u32(reloc_address));
					reloc_address += 4;
				}
			} else if (block & 0xFE00) == 0x4000 {
				// RelocBySectC
				let run_length = (block & 0x1FF) + 1;
				trace!(target: "linker", "[{block_pos:04X}] BySectC @ {reloc_address:X} (x{run_length})"); 
				for _ in 0..run_length {
					self.set_u32(reloc_address, self.code_addr + self.get_u32(reloc_address));
					reloc_address += 4;
				}
			} else if (block & 0xFE00) == 0x4200 {
				// RelocBySectD
				let run_length = (block & 0x1FF) + 1;
				trace!(target: "linker", "[{block_pos:04X}] BySectD @ {reloc_address:X} (x{run_length})");
				for _ in 0..run_length {
					self.set_u32(reloc_address, self.data_addr + self.get_u32(reloc_address));
					reloc_address += 4;
				}
			} else if (block & 0xFE00) == 0x4400 {
				// RelocTVector12
				let run_length = (block & 0x1FF) + 1;
				trace!(target: "linker", "[{block_pos:04X}] TVector12 @ {reloc_address:X} (x{run_length})");
				for _ in 0..run_length {
					self.set_u32(reloc_address, self.code_addr + self.get_u32(reloc_address));
					reloc_address += 4;
					self.set_u32(reloc_address, self.data_addr + self.get_u32(reloc_address));
					reloc_address += 8;
				}
			} else if (block & 0xFE00) == 0x4600 {
				// RelocTVector8
				let run_length = (block & 0x1FF) + 1;
				trace!(target: "linker", "[{block_pos:04X}] TVector8 @ {reloc_address:X} (x{run_length})");
				for _ in 0..run_length {
					self.set_u32(reloc_address, self.code_addr + self.get_u32(reloc_address));
					reloc_address += 4;
					self.set_u32(reloc_address, self.data_addr + self.get_u32(reloc_address));
					reloc_address += 4;
				}
			} else if (block & 0xFE00) == 0x4800 {
				// RelocVTable8
				let run_length = (block & 0x1FF) + 1;
				trace!(target: "linker", "[{block_pos:04X}] VTable8 @ {reloc_address:X} (x{run_length})");
				for _ in 0..run_length {
					self.set_u32(reloc_address, self.data_addr + self.get_u32(reloc_address));
					reloc_address += 8;
				}
			} else if (block & 0xFE00) == 0x4A00 {
				// RelocImportRun
				let run_length = (block & 0x1FF) + 1;
				trace!(target: "linker", "[{block_pos:04X}] ImportRun @ {reloc_address:X} (x{run_length})");
				for _ in 0..run_length {
					let symbol = &loader.imported_symbols[import_index as usize];
					trace!(target: "linker", "  {reloc_address:X} -> {import_index} - {}", &symbol.name);
					self.set_u32(reloc_address, self.shim_addrs[import_index as usize]);
					reloc_address += 4;
					import_index += 1;
				}
			} else if (block & 0xFE00) == 0x6000 {
				// RelocSmByImport
				let index = block & 0x1FF;
				let symbol = &loader.imported_symbols[index as usize];
				trace!(target: "linker", "[{block_pos:04X}] SmByImport @ {reloc_address:X} (sym={index} - {})", &symbol.name);
				self.set_u32(reloc_address, self.shim_addrs[index as usize]);
				reloc_address += 4;
				import_index = index + 1;
			} else if (block & 0xFE00) == 0x6200 {
				// RelocSmSetSectC
				let index = block & 0x1FF;
				warn!(target: "linker", "[{block_pos:04X}] UNIMPLEMENTED SmSetSectC (sect={index})");
			} else if (block & 0xFE00) == 0x6400 {
				// RelocSmSetSectD
				let index = block & 0x1FF;
				warn!(target: "linker", "[{block_pos:04X}] UNIMPLEMENTED SmSetSectD (sect={index})");
			} else if (block & 0xFE00) == 0x6600 {
				// RelocSmBySection
				let index = block & 0x1FF;
				warn!(target: "linker", "[{block_pos:04X}] UNIMPLEMENTED SmBySection @ {reloc_address:X} (sect={index})");
				reloc_address += 4;
			} else if (block & 0xF000) == 0x8000 {
				// RelocIncrPosition
				let offset = (block & 0xFFF) + 1;
				trace!(target: "linker", "[{block_pos:04X}] IncrPosition @ {reloc_address:X} += {offset:X} -> {:X}", reloc_address + offset);
				reloc_address += offset;
			} else if (block & 0xF000) == 0x9000 {
				// RelocSmRepeat
				let block_count = ((block >> 8) & 0xF) + 1;
				let repeat_count = (block & 0xFF) + 1;
				let repeat_start = block_pos - (block_count as usize);
				match repeat_info {
					Some((pos, counter)) if pos == block_pos => {
						// repeat, maybe
						trace!(target: "linker", "[{block_pos:04X}] SmRepeat from {repeat_start:04X}, iteration {counter}/{repeat_count}");
						if counter < repeat_count {
							next_block = repeat_start;
							repeat_info = Some((pos, counter + 1));
						}
					}
					_ => {
						// start a repeat
						trace!(target: "linker", "[{block_pos:04X}] SmRepeat from {repeat_start:04X}, iteration 0/{repeat_count}");
						next_block = repeat_start;
						repeat_info = Some((block_pos, 1));
					}
				}
			} else if (block & 0xFC00) == 0xA000 {
				// RelocSetPosition
				let offset = ((block & 0x3FF) << 16) | (relocs.data[next_block] as u32);
				next_block += 1;
				trace!(target: "linker", "[{block_pos:04X}] SetPosition = {offset:X}");
				reloc_address = self.data_addr + offset;
			} else if (block & 0xFC00) == 0xA400 {
				// RelocLgByImport
				let index = ((block & 0x3FF) << 16) | (relocs.data[next_block] as u32);
				next_block += 1;
				let symbol = &loader.imported_symbols[index as usize];
				trace!(target: "linker", "[{block_pos:04X}] LgByImport @ {reloc_address:X} (sym={index} - {})", &symbol.name);
				self.set_u32(reloc_address, self.shim_addrs[index as usize]);
				reloc_address += 4;
				import_index = index + 1;
			} else if (block & 0xFC00) == 0xB000 {
				// RelocSmRepeat
				let block_count = ((block >> 8) & 0xF) + 1;
				let repeat_count = ((block & 0xFF) << 16) | (relocs.data[next_block] as u32);
				next_block += 1;
				let repeat_start = block_pos - (block_count as usize);
				warn!(target: "linker", "[{block_pos:04X}] UNIMPLEMENTED LgRepeat from {repeat_start:04X}, {repeat_count} times");
			} else if (block & 0xFFC0) == 0xB400 {
				// RelocLgBySection
				let index = ((block & 0x3F) << 16) | (relocs.data[next_block] as u32);
				next_block += 1;
				warn!(target: "linker", "[{block_pos:04X}] UNIMPLEMENTED LgBySection @ {reloc_address:X} (sect={index})");
				reloc_address += 4;
			} else if (block & 0xFFC0) == 0xB440 {
				// RelocLgSetSectC
				let index = ((block & 0x3F) << 16) | (relocs.data[next_block] as u32);
				next_block += 1;
				warn!(target: "linker", "[{block_pos:04X}] UNIMPLEMENTED LgSetSectC (sect={index})");
			} else if (block & 0xFFC0) == 0xB480 {
				// RelocLgSetSectD
				let index = ((block & 0x3F) << 16) | (relocs.data[next_block] as u32);
				next_block += 1;
				warn!(target: "linker", "[{block_pos:04X}] UNIMPLEMENTED LgSetSectD (sect={index})");
			} else {
				warn!(target: "linker", "[{block_pos:04X}] UNKNOWN OPCODE {block:04X}");
			}
		}
	}
}
