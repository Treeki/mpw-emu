use std::io;

use crc::{Crc, CRC_16_XMODEM};
use binread::{BinRead, BinReaderExt, BinResult};
use num::Integer;

/// MacBinary header
///
/// 128-byte MacBinary header. The header has the same size on MacBinary I, II, and III. There
/// are some unused portions which were given meaning in later versions.
///
/// <https://web.archive.org/web/20050305044255/http://www.lazerware.com/formats/macbinary/macbinary_iii.html>
#[derive(BinRead, Debug)]
#[br(big)]
struct Header {
	_old_version_number: u8,
	filename_part_1: [u8; 32],
	filename_part_2: [u8; 32],
	file_type: u32,
	file_creator: u32,
	finder_flags: u8,
	_pad: u8,
	v_pos: i16,
	h_pos: i16,
	_folder_id: u16,
	_protected: u8,
	_pad2: u8,
	data_length: u32,
	resource_length: u32,
	_creation_date: u32,
	_modified_date: u32,
	_comment_length: u16,
	finder_flags_2: u8,
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
	pub finder_flags: u16,
	pub location: (i16, i16),
	pub data: Vec<u8>,
	pub resource: Vec<u8>
}

// Determine whether a file looks like a valid MacBinary file
//
// > To determine if a header is a valid MacBinary header, first take advantage of the new
// > MacBinary III signature located at offset 102. If it matches, then you know that this is a
// > valid MacBinary III header and can continue as such including the restoration of the new
// > extended Finder info.
// >
// > If it is not a MacBinary III header, start by checking bytes 0 and 74 - they should both be
// > zero. If they are both zero, either (a) the CRC should match, which means it is a MacBinary II
// > file, or (b) byte 82 is zero, which means it may be a MacBinary I file. (Note that, at the
// > current version level, byte 82 is kept zero to maintain compatibility with MacBinary I. If at
// > some point the MacBinary versions change sufficiently that it is necessary to keep MacBinary I
// > programs from downloading these files, we can change byte 82 to non-zero.)
//
// -- <https://web.archive.org/web/20050305044255/http://www.lazerware.com/formats/macbinary/macbinary_iii.html>
pub fn probe(file: &[u8]) -> bool {
	if file.len() < 0x80 {
		return false;
	}

	let data_size = u32::from_be_bytes(file[0x53 .. 0x57].try_into().unwrap()).next_multiple_of(&0x80);
	let resource_size = u32::from_be_bytes(file[0x57 .. 0x5B].try_into().unwrap()).next_multiple_of(&0x80);
	let expected_size = 0x80 + data_size as usize + resource_size as usize;
	trace!(target: "macbinary", "probe: data_size={data_size:X} resource_size={resource_size:X} expected_size={expected_size:X}");

	if file[0x66..0x6A] == *b"mBIN" && file.len() == expected_size {
		// MacBinary III
		return true;
	}

	// Maybe MacBinary II, check CRC
	file[0] == 0
		&& file[74] == 0
		&& u16::from_be_bytes(file[124..][..2].try_into().unwrap()) == crc(&file[..124])
}

pub fn unpack(file: &[u8]) -> BinResult<File> {
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
		finder_flags: ((header.finder_flags as u16) << 8) | (header.finder_flags_2 as u16),
		location: (header.h_pos, header.v_pos),
		data,
		resource
	})
}

fn crc(file: &[u8]) -> u16 {
	let crc: Crc<u16> = Crc::<u16>::new(&CRC_16_XMODEM);
	crc.checksum(file)
}
