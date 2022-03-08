use std::io::Cursor;

use binread::{BinReaderExt, NullString};

mod data;
pub use data::{Architecture, SectionType, ShareType};

#[derive(Debug)]
pub struct PEF {
	pub sections: Vec<Section>
}

#[derive(Debug)]
pub struct Section {
	pub name: Option<String>,
	pub default_address: u32,
	pub total_size: u32,
	pub unpacked_size: u32,
	pub packed_size: u32,
	pub packed_contents: Option<Vec<u8>>,
	pub section_kind: SectionType,
	pub share_kind: ShareType,
	pub alignment: u8
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum SymbolClass {
	Code = 0,
	Data = 1,
	TVect = 2,
	TOC = 3,
	Glue = 4
}
impl SymbolClass {
	fn parse(value: u32) -> Self {
		match value {
			0 => Self::Code,
			1 => Self::Data,
			2 => Self::TVect,
			3 => Self::TOC,
			4 => Self::Glue,
			_ => panic!()
		}
	}
}

#[derive(Debug, Clone)]
pub struct ImportedSymbol {
	pub library: usize,
	pub name: String,
	pub class: SymbolClass,
	pub weak: bool
}

#[derive(Debug)]
pub struct ImportedLibrary {
	pub name: String,
	pub old_imp_version: u32,
	pub current_version: u32,
	pub imported_symbols: Vec<ImportedSymbol>,
	pub import_order: bool,
	pub is_weak: bool
}

#[derive(Debug)]
pub struct RelocSection {
	pub section_index: u16,
	pub data: Vec<u16>
}

#[derive(Debug)]
pub struct ExportedSymbol {
	pub name: String,
	pub class: SymbolClass,
	pub value: u32,
	pub section: i16
}

#[derive(Debug)]
pub struct Loader {
	pub main_section: i32,
	pub main_offset: u32,
	pub init_section: i32,
	pub init_offset: u32,
	pub term_section: i32,
	pub term_offset: u32,
	pub imported_libraries: Vec<ImportedLibrary>,
	pub imported_symbols: Vec<ImportedSymbol>,
	pub reloc_sections: Vec<RelocSection>,
	pub exported_symbols: Vec<ExportedSymbol>,
}

pub fn read_pef(data: &[u8]) -> Result<PEF, binread::Error> {
	let mut cursor = Cursor::new(data);
	let container_header: data::ContainerHeader = cursor.read_be()?;

	let mut section_headers: Vec<data::SectionHeader> = Vec::new();
	let mut sections: Vec<Section> = Vec::new();
	for _ in 0..container_header.section_count {
		let hdr: data::SectionHeader = cursor.read_be()?;
		sections.push(Section {
			name: None,
			default_address: hdr.default_address,
			total_size: hdr.total_size,
			unpacked_size: hdr.unpacked_size,
			packed_size: hdr.packed_size,
			packed_contents: None,
			section_kind: hdr.section_kind,
			share_kind: hdr.share_kind,
			alignment: hdr.alignment
		});
		section_headers.push(hdr);
	}

	// get names
	let name_base = cursor.position();
	for (hdr, section) in section_headers.iter().zip(&mut sections) {
		if hdr.name_offset >= 0 {
			cursor.set_position(name_base + (hdr.name_offset as u64));
			let name: NullString = cursor.read_be()?;
			section.name = Some(name.into_string());
		}

		if hdr.packed_size > 0 {
			let start = hdr.container_offset as usize;
			let end = start + (hdr.packed_size as usize);
			section.packed_contents = Some(Vec::from(&data[start .. end]));
		}
	}

	Ok(PEF {
		sections
	})
}


struct PatternReader<'a> {
	data: &'a [u8],
	position: usize
}
impl PatternReader<'_> {
	fn remaining(&self) -> bool {
		self.position < self.data.len()
	}

	fn read_byte(&mut self) -> u8 {
		let byte = self.data[self.position];
		self.position += 1;
		byte
	}

	fn read_arg(&mut self) -> usize {
		let mut arg = 0;
		while self.position < self.data.len() {
			let byte = self.data[self.position];
			self.position += 1;
			arg = (arg << 7) | (byte & 0x7F) as usize;
			if (byte & 0x80) == 0 {
				break;
			}
		}
		arg
	}
}

pub fn unpack_pattern_data(input: &[u8], output: &mut [u8]) {
	let mut reader: PatternReader = PatternReader { data: input, position: 0 };
	let mut out_pos = 0usize;

	while reader.remaining() {
		let insn = reader.read_byte();
		let opcode = insn >> 5;
		let count = if (insn & 31) == 0 { reader.read_arg() } else { (insn & 31) as usize };

		match opcode {
			0 => {
				// Zero
				for i in 0..count {
					output[out_pos + i] = 0;
				}
				out_pos += count;
			}
			1 => {
				// BlockCopy
				for i in 0..count {
					output[out_pos + i] = reader.read_byte();
				}
				out_pos += count;
			}
			2 => {
				// RepeatedBlock
				let repeat_count = reader.read_arg();
				let first_pos = out_pos;
				for i in 0..count {
					output[out_pos + i] = reader.read_byte();
				}
				out_pos += count;
				for _ in 0..repeat_count {
					output.copy_within(first_pos .. first_pos + count, out_pos);
					out_pos += count;
				}
			}
			3 => {
				// InterleaveRepeatBlockWithBlockCopy
				let common_size = count;
				let custom_size = reader.read_arg();
				let repeat_count = reader.read_arg();

				let first_pos = out_pos;
				for i in 0..common_size {
					output[out_pos + i] = reader.read_byte();
				}
				out_pos += common_size;

				for _ in 0..repeat_count {
					for i in 0..custom_size {
						output[out_pos + i] = reader.read_byte();
					}
					out_pos += custom_size;
					output.copy_within(first_pos .. first_pos + common_size, out_pos);
					out_pos += common_size;
				}
			}
			4 => {
				// InterleaveRepeatBlockWithZero
				let common_size = count;
				let custom_size = reader.read_arg();
				let repeat_count = reader.read_arg();

				for i in 0..common_size {
					output[out_pos + i] = 0;
				}
				out_pos += common_size;

				for _ in 0..repeat_count {
					for i in 0..custom_size {
						output[out_pos + i] = reader.read_byte();
					}
					out_pos += custom_size;
					for i in 0..common_size {
						output[out_pos + i] = 0;
					}
					out_pos += common_size;
				}
			}
			_ => unreachable!()
		}
	}
}

pub fn parse_loader(data: &[u8]) -> binread::BinResult<Loader> {
	let mut cursor = Cursor::new(data);
	let loader_header: data::LoaderHeader = cursor.read_be()?;

	let mut imported_libraries_data: Vec<data::ImportedLibrary> = Vec::new();
	let mut imported_symbols_data: Vec<u32> = Vec::new();
	let mut relocation_headers_data: Vec<data::RelocationHeader> = Vec::new();

	for _ in 0..loader_header.imported_library_count {
		imported_libraries_data.push(cursor.read_be()?);
	}
	for _ in 0..loader_header.total_imported_symbol_count {
		imported_symbols_data.push(cursor.read_be()?);
	}
	for _ in 0..loader_header.reloc_section_count {
		relocation_headers_data.push(cursor.read_be()?);
	}

	// --
	// IMPORTS

	let mut imported_symbols: Vec<ImportedSymbol> = Vec::new();
	for symbol in imported_symbols_data {
		cursor.set_position((loader_header.loader_strings_offset + (symbol & 0xFFFFFF)).into());
		let name = cursor.read_be::<NullString>()?.to_string();

		let class = SymbolClass::parse((symbol >> 24) & 0xF);
		let weak = (symbol & 0x80000000) != 0;
		imported_symbols.push(ImportedSymbol { library: 0, name, class, weak })
	}

	let mut imported_libraries: Vec<ImportedLibrary> = Vec::new();
	for lib in imported_libraries_data {
		cursor.set_position((loader_header.loader_strings_offset + lib.name_offset).into());
		let name = cursor.read_be::<NullString>()?.to_string();

		let sym_start = lib.first_imported_symbol as usize;
		let sym_end = (lib.first_imported_symbol + lib.imported_symbol_count) as usize;

		let index = imported_libraries.len();
		for sym in &mut imported_symbols[sym_start .. sym_end] {
			sym.library = index;
		}

		let imported_symbols = imported_symbols[sym_start .. sym_end].to_vec();

		imported_libraries.push(ImportedLibrary {
			name,
			old_imp_version: lib.old_imp_version,
			current_version: lib.current_version,
			imported_symbols,
			import_order: (lib.options & 0x80) != 0,
			is_weak: (lib.options & 0x40) != 0
		});
	}

	// --
	// RELOCATIONS

	let mut reloc_sections: Vec<RelocSection> = Vec::new();
	for header in relocation_headers_data {
		cursor.set_position((loader_header.reloc_instr_offset + header.first_reloc_offset).into());

		let mut data = Vec::new();
		for _ in 0..header.reloc_count {
			data.push(cursor.read_be()?);
		}

		reloc_sections.push(RelocSection {
			section_index: header.section_index,
			data
		});
	}

	// --
	// EXPORTS

	let hash_table_entry_count = 1u32 << loader_header.export_hash_table_power;

	let key_table_pos = loader_header.export_hash_offset + hash_table_entry_count * 4;
	let exports_pos = key_table_pos + loader_header.exported_symbol_count * 4;

	let mut exported_symbols = Vec::new();

	for i in 0..loader_header.exported_symbol_count {
		cursor.set_position((key_table_pos + i * 4).into());
		let key: u32 = cursor.read_be()?;

		cursor.set_position((exports_pos + i * 10).into());
		let export: data::ExportedSymbol = cursor.read_be()?;

		let name_offset = (loader_header.loader_strings_offset + (export.class_and_name & 0xFFFFFF)) as usize;
		let name_length = key >> 16;
		let name = data[name_offset .. name_offset + name_length as usize].to_vec();
		let name = String::from_utf8(name).unwrap();

		let class = SymbolClass::parse((export.class_and_name >> 24) & 0xF);

		exported_symbols.push(ExportedSymbol {
			name,
			class,
			value: export.symbol_value,
			section: export.section_index
		});
	}

	Ok(Loader {
		main_section: loader_header.main_section,
		main_offset: loader_header.main_offset,
		init_section: loader_header.init_section,
		init_offset: loader_header.init_offset,
		term_section: loader_header.term_section,
		term_offset: loader_header.term_offset,
		imported_libraries,
		imported_symbols,
		reloc_sections,
		exported_symbols
	})
}

