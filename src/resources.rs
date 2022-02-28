use std::{io::{Cursor, Read}, collections::HashMap, rc::Rc};

use binread::{BinRead, BinReaderExt, BinResult};

use crate::common::FourCC;

#[derive(BinRead, Debug)]
struct Header {
	data_offset: u32,
	map_offset: u32,
	_data_size: u32,
	_map_size: u32,
}

#[derive(BinRead, Debug)]
struct Map {
	_header_copy: Header,
	_reserved_next: u32,
	_reserved_refnum: u16,
	_attributes: u16,
	type_list_offset: u16,
	name_list_offset: u16,
	#[br(map = |x: u16| (x as u32) + 1)]
	type_count: u32
}

#[derive(BinRead, Debug)]
struct TypeListEntry {
	type_id: FourCC,
	#[br(map = |x: u16| (x as u32) + 1)]
	resource_count: u32,
	ref_list_offset: u16
}

#[derive(BinRead, Debug)]
struct RefListEntry {
	id: i16,
	name_offset: u16,
	attributes_and_data_offset: u32,
	_reserved: u32
}

pub struct Resource {
	pub id: i16,
	pub name: Option<String>,
	pub data: Vec<u8>	
}

pub struct Resources {
	pub types: HashMap<FourCC, Vec<Rc<Resource>>>
}

impl Resources {
	pub fn get(&self, ty: FourCC, id: i16) -> Option<Rc<Resource>> {
		if let Some(list) = self.types.get(&ty) {
			for res in list {
				if res.id == id {
					return Some(Rc::clone(res));
				}
			}
		}
		None
	}
}

pub fn parse_resources(res_fork: &[u8]) -> BinResult<Resources> {
	let mut cursor = Cursor::new(res_fork);

	let header: Header = cursor.read_be()?;
	let data_offset = header.data_offset as u64;
	let map_offset = header.map_offset as u64;
	cursor.set_position(map_offset);
	
	let map: Map = cursor.read_be()?;
	let type_list_offset = map_offset + map.type_list_offset as u64;
	let name_list_offset = map_offset + map.name_list_offset as u64;

	let mut types = HashMap::new();

	for i in 0 .. map.type_count.into() {
		cursor.set_position(type_list_offset + 2 + 8 * i);
		let type_list_entry: TypeListEntry = cursor.read_be()?;

		let mut resources = Vec::new();

		let ref_list_offset = type_list_offset + type_list_entry.ref_list_offset as u64;
		for j in 0 .. type_list_entry.resource_count.into() {
			cursor.set_position(ref_list_offset + 12 * j);
			let ref_list_entry: RefListEntry = cursor.read_be()?;

			let name = if ref_list_entry.name_offset != 0xFFFF {
				cursor.set_position(name_list_offset + ref_list_entry.name_offset as u64);
				let len: u8 = cursor.read_be()?;
				let mut buf = Vec::new();
				buf.resize(len as usize, 0u8);
				cursor.read_exact(&mut buf)?;
				Some(String::from_utf8(buf).unwrap())
			} else {
				None
			};
			
			let res_header = data_offset + (ref_list_entry.attributes_and_data_offset & 0xFFFFFF) as u64;
			cursor.set_position(res_header);
			let res_size: u32 = cursor.read_be()?;
			let res_start = res_header + 4;
			let res_end = res_start + res_size as u64;

			let data = res_fork[res_start as usize .. res_end as usize].to_vec();

			resources.push(Rc::new(Resource { id: ref_list_entry.id, name, data }));
		}

		types.insert(type_list_entry.type_id, resources);
	}

	Ok(Resources { types })
}

