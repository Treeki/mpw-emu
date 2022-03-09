use std::{io::{Cursor, Read}, collections::HashMap, rc::Rc, cell::RefCell};

use binread::{BinRead, BinReaderExt, BinResult};

use crate::{common::FourCC, filesystem::MacFile};

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
	attributes: u16,
	type_list_offset: u16,
	name_list_offset: u16,
	#[br(map = |x: i16| (x + 1) as u32)]
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
	pub name: Option<Vec<u8>>,
	pub attributes: u8,
	pub data: Vec<u8>	
}

pub struct Resources {
	pub file: Rc<RefCell<MacFile>>,
	pub attributes: u16,
	pub types: HashMap<FourCC, Vec<Rc<RefCell<Resource>>>>
}

impl Resources {
	pub fn add(&mut self, ty: FourCC, id: i16, name: Option<Vec<u8>>) -> Option<Rc<RefCell<Resource>>> {
		if !self.types.contains_key(&ty) {
			self.types.insert(ty, Vec::new());
		}

		let list = self.types.get_mut(&ty).unwrap();
		let mut insert_pos = list.len();
		for (pos, res) in list.iter().enumerate() {
			let res_id = res.borrow().id;
			if res_id == id {
				// resource is already there
				return None;
			} else if res_id > id {
				// we've found the place to insert the new resource at
				insert_pos = pos;
				break;
			}
		}

		let res = Resource {
			id,
			name,
			attributes: 0,
			data: Vec::new()
		};
		let res = Rc::new(RefCell::new(res));
		list.insert(insert_pos, Rc::clone(&res));
		Some(res)
	}

	pub fn get(&self, ty: FourCC, id: i16) -> Option<Rc<RefCell<Resource>>> {
		if let Some(list) = self.types.get(&ty) {
			for res in list {
				if res.borrow().id == id {
					return Some(Rc::clone(res));
				}
			}
		}
		None
	}

	pub fn remove(&mut self, ty: FourCC, id: i16) {
		let list = match self.types.get_mut(&ty) {
			Some(l) => l,
			None => return
		};

		list.retain(|e| e.borrow().id != id);
		if list.is_empty() {
			self.types.remove(&ty);
		}
	}

	pub fn pack(&self) -> Vec<u8> {
		// rebuild the resource fork
		let mut buffer = Vec::new();

		let data_offset = 256;
		buffer.resize(data_offset, 0);

		// construct a map
		let mut map_buffer = Vec::new();
		map_buffer.resize(30 + 8 * self.types.len(), 0);

		let mut name_buffer = Vec::new();

		map_buffer[22] = (self.attributes >> 8) as u8;
		map_buffer[23] = (self.attributes & 0xFF) as u8;

		let type_list_offset = 28;
		map_buffer[24] = (type_list_offset >> 8) as u8;
		map_buffer[25] = (type_list_offset & 0xFF) as u8;

		// we'll put in the name list offset later

		let number_of_types_minus_1 = (self.types.len() as u16).wrapping_sub(1);
		map_buffer[28] = (number_of_types_minus_1 >> 8) as u8;
		map_buffer[29] = (number_of_types_minus_1 & 0xFF) as u8;

		// write all types
		for (type_index, (&type_id, list)) in self.types.iter().enumerate() {
			// type header
			let type_offset = type_list_offset + 2 + 8 * type_index;

			map_buffer[type_offset] = (type_id.0 >> 24) as u8;
			map_buffer[type_offset + 1] = (type_id.0 >> 16) as u8;
			map_buffer[type_offset + 2] = (type_id.0 >> 8) as u8;
			map_buffer[type_offset + 3] = type_id.0 as u8;

			let number_minus_1 = (list.len() as u16).wrapping_sub(1);
			map_buffer[type_offset + 4] = (number_minus_1 >> 8) as u8;
			map_buffer[type_offset + 5] = (number_minus_1 & 0xFF) as u8;

			let relative_ref_offset = map_buffer.len() - type_list_offset;
			map_buffer[type_offset + 6] = (relative_ref_offset >> 8) as u8;
			map_buffer[type_offset + 7] = (relative_ref_offset & 0xFF) as u8;

			// produce ref lists
			let ref_list_offset = map_buffer.len();
			map_buffer.resize(map_buffer.len() + 12 * list.len(), 0);

			for (res_index, res) in list.iter().enumerate() {
				let res = res.borrow();
				let ref_offset = ref_list_offset + 12 * res_index;

				map_buffer[ref_offset] = (res.id >> 8) as u8;
				map_buffer[ref_offset + 1] = res.id as u8;

				// append name
				let name_offset = match &res.name {
					Some(name) => {
						let o = name_buffer.len();
						name_buffer.push(name.len() as u8);
						name_buffer.extend_from_slice(&name);
						o as u16
					}
					None => 0xFFFF
				};
				map_buffer[ref_offset + 2] = (name_offset >> 8) as u8;
				map_buffer[ref_offset + 3] = name_offset as u8;

				map_buffer[ref_offset + 4] = res.attributes;

				let res_data_offset = buffer.len() - data_offset;
				map_buffer[ref_offset + 5] = (res_data_offset >> 16) as u8;
				map_buffer[ref_offset + 6] = (res_data_offset >> 8) as u8;
				map_buffer[ref_offset + 7] = res_data_offset as u8;

				// append data
				let res_size = res.data.len();
				buffer.push((res_size >> 24) as u8);
				buffer.push((res_size >> 16) as u8);
				buffer.push((res_size >> 8) as u8);
				buffer.push(res_size as u8);
				buffer.extend_from_slice(&res.data);
			}
		}

		// all done, now add the name list offset
		let name_list_offset = map_buffer.len();
		map_buffer[26] = (name_list_offset >> 8) as u8;
		map_buffer[27] = (name_list_offset & 0xFF) as u8;

		// we can now assemble the file
		// build the resource header
		buffer[0] = (data_offset >> 24) as u8;
		buffer[1] = (data_offset >> 16) as u8;
		buffer[2] = (data_offset >> 8) as u8;
		buffer[3] = data_offset as u8;

		let map_offset = buffer.len();
		buffer[4] = (map_offset >> 24) as u8;
		buffer[5] = (map_offset >> 16) as u8;
		buffer[6] = (map_offset >> 8) as u8;
		buffer[7] = map_offset as u8;

		let data_size = buffer.len() - data_offset;
		buffer[8] = (data_size >> 24) as u8;
		buffer[9] = (data_size >> 16) as u8;
		buffer[10] = (data_size >> 8) as u8;
		buffer[11] = data_size as u8;

		let map_size = map_buffer.len() + name_buffer.len();
		buffer[12] = (map_size >> 24) as u8;
		buffer[13] = (map_size >> 16) as u8;
		buffer[14] = (map_size >> 8) as u8;
		buffer[15] = map_size as u8;

		// copy it to the map
		map_buffer[0..16].copy_from_slice(&buffer[0..16]);

		// and finally get it all sorted out
		buffer.extend_from_slice(&map_buffer);
		buffer.extend_from_slice(&name_buffer);

		buffer
	}

	pub fn save_to_file(&self) {
		let mut file = self.file.borrow_mut();
		file.resource_fork = self.pack();
		file.set_dirty();
	}
}

pub fn parse_resources(file: Rc<RefCell<MacFile>>) -> BinResult<Resources> {
	let file_ref = file.borrow();
	let mut cursor = Cursor::new(&file_ref.resource_fork);

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
				Some(buf)
			} else {
				None
			};
			
			let res_header = data_offset + (ref_list_entry.attributes_and_data_offset & 0xFFFFFF) as u64;
			cursor.set_position(res_header);
			let res_size: u32 = cursor.read_be()?;
			let res_start = res_header + 4;
			let res_end = res_start + res_size as u64;

			let data = file_ref.resource_fork[res_start as usize .. res_end as usize].to_vec();

			let res = Resource {
				id: ref_list_entry.id,
				name,
				attributes: (ref_list_entry.attributes_and_data_offset >> 24) as u8,
				data
			};
			resources.push(Rc::new(RefCell::new(res)));
		}

		types.insert(type_list_entry.type_id, resources);
	}
	drop(file_ref);

	Ok(Resources {
		file,
		attributes: map.attributes,
		types
	})
}

