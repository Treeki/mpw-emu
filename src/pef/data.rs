use binread::BinRead;

#[derive(BinRead, Debug)]
#[br(big, magic = b"Joy!peff")]
pub struct ContainerHeader {
	pub architecture: Architecture,
	pub format_version: u32,
	pub date_time_stamp: u32,
	pub old_def_version: u32,
	pub old_imp_version: u32,
	pub current_version: u32,
	pub section_count: u16,
	pub inst_section_count: u16,
	pub reserved_a: u32
}

#[derive(BinRead, Debug)]
#[br(big)]
pub struct SectionHeader {
	pub name_offset: i32,
	pub default_address: u32,
	pub total_size: u32,
	pub unpacked_size: u32,
	pub packed_size: u32,
	pub container_offset: u32,
	pub section_kind: SectionType,
	pub share_kind: ShareType,
	pub alignment: u8,
	pub reserved_a: u8
}

#[derive(BinRead, Copy, Clone, Debug, PartialEq, Eq)]
#[br(big, repr = u32)]
#[allow(non_camel_case_types)]
pub enum Architecture {
	PowerPC_CFM = 0x70777063, // 'pwpc'
	CFM_68K = 0x6d36386b, // 'm68k'
}

#[derive(BinRead, Copy, Clone, Debug, PartialEq, Eq)]
#[br(big, repr = u8)]
pub enum SectionType {
	Code = 0,
	UnpackedData = 1,
	PatternInitData = 2,
	Constant = 3,
	Loader = 4,
	Debug = 5,
	ExecutableData = 6,
	Exception = 7,
	Traceback = 8
}

#[derive(BinRead, Copy, Clone, Debug, PartialEq, Eq)]
#[br(big, repr = u8)]
pub enum ShareType {
	ProcessShare = 1,
	GlobalShare = 4,
	ProtectedShare = 5
}

#[derive(BinRead, Debug)]
#[br(big)]
pub struct LoaderHeader {
	pub main_section: i32,
	pub main_offset: u32,
	pub init_section: i32,
	pub init_offset: u32,
	pub term_section: i32,
	pub term_offset: u32,
	pub imported_library_count: u32,
	pub total_imported_symbol_count: u32,
	pub reloc_section_count: u32,
	pub reloc_instr_offset: u32,
	pub loader_strings_offset: u32,
	pub export_hash_offset: u32,
	pub export_hash_table_power: u32,
	pub exported_symbol_count: u32
}

#[derive(BinRead, Debug)]
#[br(big)]
pub struct ImportedLibrary {
	pub name_offset: u32,
	pub old_imp_version: u32,
	pub current_version: u32,
	pub imported_symbol_count: u32,
	pub first_imported_symbol: u32,
	pub options: u8,
	pub reserved_a: u8,
	pub reserved_b: u16
}

#[derive(BinRead, Debug)]
#[br(big)]
pub struct RelocationHeader {
	pub section_index: u16,
	pub reserved_a: u16,
	pub reloc_count: u32,
	pub first_reloc_offset: u32
}

#[derive(BinRead, Debug)]
#[br(big)]
pub struct ExportedSymbol {
	pub class_and_name: u32,
	pub symbol_value: u32,
	pub section_index: i16
}
