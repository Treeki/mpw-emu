use std::io;

use binread::{BinRead, BinResult, BinReaderExt};

#[derive(BinRead, Debug)]
#[br(big)]
struct Header {
	_old_version_number: u8,
	filename_part_1: [u8; 32],
	filename_part_2: [u8; 32],
	file_type: u32,
	file_creator: u32,
	_finder_flags: u8,
	_pad: u8,
	_v_pos: u16,
	_h_pos: u16,
	_folder_id: u16,
	_protected: u8,
	_pad2: u8,
	data_length: u32,
	resource_length: u32,
	_creation_date: u32,
	_modified_date: u32,
	_comment_length: u16,
	_finder_flags_2: u8,
	_pad3: [u8; 14],
	_unused_74: u32,
	_secondary_header_length: u16,
	_version_number: u8,
	_minimum_version_number: u8,
	_crc: u16
}

pub struct File {
	pub name: String,
	pub type_id: u32,
	pub creator_id: u32,
	pub data: Vec<u8>,
	pub resource: Vec<u8>
}

pub fn extract_file(file: &[u8]) -> BinResult<File> {
	let mut cursor = io::Cursor::new(file);
	let header: Header = cursor.read_be()?;

	let mut name_bytes = [0u8; 64];
	name_bytes[0..32].copy_from_slice(&header.filename_part_1);
	name_bytes[32..64].copy_from_slice(&header.filename_part_2);
	let name_len = name_bytes[0] as usize;
	let name = String::from_utf8(name_bytes[1 .. name_len + 1].to_vec()).unwrap();

	let data_start = 0x80usize;
	let resource_start = (data_start + (header.data_length as usize + 0x7F)) & !0x7F;

	let data_end = data_start + (header.data_length as usize);
	let data = file[data_start .. data_end].to_vec();

	let resource_end = resource_start + (header.resource_length as usize);
	let resource = file[resource_start .. resource_end].to_vec();

	Ok(File {
		name,
		type_id: header.file_type,
		creator_id: header.file_creator,
		data,
		resource
	})
}
