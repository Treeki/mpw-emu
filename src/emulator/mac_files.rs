use std::{ffi::CString, rc::Rc, cell::RefCell};

// const ATTRIB_LOCKED: u8 = 1;
// const ATTRIB_RESOURCE_FORK_OPEN: u8 = 4;
// const ATTRIB_DATA_FORK_OPEN: u8 = 8;
const ATTRIB_DIRECTORY: u8 = 0x10;
// const ATTRIB_ANY_FORK_OPEN: u8 = 0x80;

use crate::{common::{OSErr, FourCC, system_time_to_mac_time}, filesystem::{MacFile, Fork}, mac_roman};

use super::{EmuState, EmuUC, FuncResult, helpers::{ArgReader, UnicornExtras}};

pub(super) struct FileHandle {
	file: Rc<RefCell<MacFile>>,
	position: usize,
	fork: Fork
}

fn nice_error(error: anyhow::Error) -> u32 {
	if let Some(io) = error.downcast_ref::<std::io::Error>() {
		if io.kind() == std::io::ErrorKind::NotFound {
			return OSErr::FileNotFound.to_u32();
		}
	}

	OSErr::IOError.to_u32()
}

fn pb_open_rf_sync(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let pb: u32 = reader.read1(uc)?;

	let name = uc.read_pascal_string(uc.read_u32(pb + 0x12)?)?;
	let volume = uc.read_i16(pb + 0x16)?; // ioVRefNum
	let permission = uc.read_i8(pb + 0x1B)?;

	// this is nasty
	info!(target: "files", "PBOpenRFSync(vol={volume}, name={name:?}, permission={permission})");

	let path = match state.filesystem.resolve_path(volume, 0, name.as_bytes()) {
		Ok(p) => p,
		Err(e) => {
			error!(target: "files", "PBOpenRFSync failed to resolve path: {e:?}");
			return Ok(Some(OSErr::BadName.to_u32()))
		}
	};

	let file = match state.filesystem.get_file(&path) {
		Ok(f) => f,
		Err(e) => {
			error!(target: "files", "PBOpenRFSync failed to get file: {e:?}");
			return Ok(Some(OSErr::IOError.to_u32()))
		}
	};

	let handle = state.next_file_handle;
	state.next_file_handle += 1;
	state.file_handles.insert(handle, FileHandle {
		file,
		fork: Fork::Resource,
		position: 0
	});

	info!(target: "files", "... returned handle {handle}");
	uc.write_u16(pb + 0x18, handle)?;
	Ok(Some(0))
}

fn pb_write_sync(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let pb: u32 = reader.read1(uc)?;

	let ref_num = uc.read_u16(pb + 0x18)?;
	let buffer_ptr = uc.read_u32(pb + 0x20)?;
	let req_count = uc.read_u32(pb + 0x24)?;
	let pos_mode = uc.read_i16(pb + 0x2C)?;
	let pos_offset = uc.read_i32(pb + 0x2E)?;
	let mut result = OSErr::NoError;

	// this is nasty
	info!(target: "files", "PBWriteSync(ref={ref_num}, buffer={buffer_ptr:08X}, req_count={req_count:X}, pos_mode={pos_mode}, pos_offset={pos_offset:X})");

	if let Some(handle) = state.file_handles.get_mut(&ref_num) {
		let mut file = handle.file.borrow_mut();
		file.set_dirty();

		// work out the new position
		let buffer = file.get_fork_mut(handle.fork);
		let write_start = match pos_mode & 3 {
			1 => pos_offset as isize,
			2 => (buffer.len() as isize + pos_offset as isize),
			3 => (handle.position as isize + pos_offset as isize),
			_ => handle.position as isize
		};
		if write_start >= 0 {
			let write_start = write_start as usize;
			let write_end = write_start + req_count as usize;
			if buffer.len() < write_end {
				buffer.resize(write_end, 0);
			}

			uc.mem_read(buffer_ptr.into(), &mut buffer[write_start..write_end])?;
			handle.position = write_end;

			uc.write_u32(pb + 0x28, req_count)?; // we always write the full amount
			uc.write_u32(pb + 0x2E, write_end as u32)?;
		} else {
			result = OSErr::Position;
		}
	} else {
		result = OSErr::RefNum;
	}

	uc.write_i16(pb + 0x10, result as i16)?;
	Ok(Some(0))
}

fn fs_close(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let ref_num: u16 = reader.read1(uc)?;

	trace!(target: "files", "FSClose({ref_num})");

	match state.file_handles.remove(&ref_num) {
		Some(handle) => {
			let mut file = handle.file.borrow_mut();
			trace!(target: "files", " ... {:?}", file.path);
			if let Err(err) = file.save_if_dirty() {
				error!(target: "files", "Error while saving modified file: {err:?}");
			}
			Ok(Some(0))
		}
		None => Ok(Some(OSErr::RefNum.to_u32())) // file not opened
	}
}

fn fs_read(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (ref_num, count_ptr, buf_ptr): (u16, u32, u32) = reader.read3(uc)?;

	trace!(target: "files", "FSRead(ref={ref_num}, count_ptr={count_ptr:08X}, buf_ptr={buf_ptr:08X})");

	let handle = match state.file_handles.get_mut(&ref_num) {
		Some(h) => h,
		None => return Ok(Some(OSErr::RefNum.to_u32()))
	};

	let file = handle.file.borrow();
	let buffer = file.get_fork(handle.fork);

	let count = uc.read_i32(count_ptr)?;
	if count < 0 {
		return Ok(Some(OSErr::Param.to_u32()));
	}
	let current_pos = handle.position;
	handle.position += count as usize;
	if handle.position > buffer.len() {
		handle.position = buffer.len();
	}

	let actual_count = handle.position - current_pos;
	uc.mem_write(buf_ptr.into(), &buffer[current_pos..handle.position])?;
	uc.write_u32(count_ptr, actual_count as u32)?;

	trace!(target: "files", "..requested {count:X} bytes at position {current_pos:X}, got {actual_count:X}, new position is {:X}", handle.position);

	if actual_count < count as usize {
		Ok(Some(OSErr::Eof.to_u32()))
	} else {
		Ok(Some(0))
	}
}

fn fs_write(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (ref_num, count_ptr, buf_ptr): (u16, u32, u32) = reader.read3(uc)?;

	trace!(target: "files", "FSWrite(ref={ref_num}, count_ptr={count_ptr:08X}, buf_ptr={buf_ptr:08X})");

	let handle = match state.file_handles.get_mut(&ref_num) {
		Some(h) => h,
		None => return Ok(Some(OSErr::RefNum.to_u32()))
	};

	let mut file = handle.file.borrow_mut();
	file.set_dirty();

	let buffer = file.get_fork_mut(handle.fork);

	let count = uc.read_i32(count_ptr)?;
	if count < 0 {
		return Ok(Some(OSErr::Param.to_u32()));
	}
	let current_pos = handle.position;
	handle.position += count as usize;
	if handle.position > buffer.len() {
		buffer.resize(handle.position, 0);
	}

	uc.mem_read(buf_ptr.into(), &mut buffer[current_pos..handle.position])?;
	Ok(Some(0))
}

fn get_v_info(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (drive_number, name_ptr, ref_num_ptr, free_bytes_ptr): (i16, u32, u32, u32) = reader.read4(uc)?;

	trace!(target: "files", "GetVInfo(drive={drive_number})");

	match state.filesystem.get_volume_info_by_drive_number(drive_number) {
		Some((name, volume_ref)) => {
			// we have data
			uc.write_pascal_string(name_ptr, &mac_roman::encode_string(&name, false))?;
			uc.write_i16(ref_num_ptr, volume_ref)?;
			uc.write_u32(free_bytes_ptr, 0x100000)?;
			Ok(Some(0))
		}
		None => {
			// no idea
			if drive_number == 0 {
				Ok(Some(OSErr::Param.to_u32())) // no default volume
			} else {
				Ok(Some(OSErr::NoSuchVolume.to_u32()))
			}
		}
	}
}

fn get_eof(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (ref_num, eof_ptr): (u16, u32) = reader.read2(uc)?;

	if let Some(handle) = state.file_handles.get(&ref_num) {
		let eof = handle.file.borrow().len(handle.fork);
		uc.write_u32(eof_ptr, eof as u32)?;
		Ok(Some(0))
	} else {
		Ok(Some(OSErr::RefNum.to_u32()))
	}
}

fn set_eof(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (ref_num, eof): (u16, u32) = reader.read2(uc)?;

	if let Some(handle) = state.file_handles.get_mut(&ref_num) {
		handle.file.borrow_mut().get_fork_mut(handle.fork).resize(eof as usize, 0);
		Ok(Some(0))
	} else {
		Ok(Some(OSErr::RefNum.to_u32()))
	}
}

fn get_f_pos(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (ref_num, pos_ptr): (u16, u32) = reader.read2(uc)?;

	if let Some(handle) = state.file_handles.get(&ref_num) {
		uc.write_u32(pos_ptr, handle.position as u32)?;
		Ok(Some(0))
	} else {
		Ok(Some(OSErr::RefNum.to_u32()))
	}
}

fn set_f_pos(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (ref_num, pos_mode, pos_offset): (u16, i16, i32) = reader.read3(uc)?;

	if let Some(handle) = state.file_handles.get_mut(&ref_num) {
		let eof = handle.file.borrow().len(handle.fork) as isize;

		let current_pos = handle.position as isize;
		let new_pos = match pos_mode {
			1 => pos_offset as isize,
			2 => eof + pos_offset as isize,
			3 => current_pos + pos_offset as isize,
			_ => current_pos
		};

		if new_pos < 0 {
			Ok(Some(OSErr::Position.to_u32()))
		} else if new_pos > eof {
			Ok(Some(OSErr::Eof.to_u32()))
		} else {
			handle.position = new_pos as usize;
			Ok(Some(0))
		}
	} else {
		Ok(Some(OSErr::RefNum.to_u32()))
	}
}

fn pb_get_cat_info_sync(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let pb: u32 = reader.read1(uc)?;

	let name_ptr = uc.read_u32(pb + 0x12)?;
	let volume_ref = uc.read_i16(pb + 0x16)?;
	let io_f_dir_index = uc.read_i16(pb + 0x1C)?;
	let dir_id = uc.read_i32(pb + 0x30)?;

	trace!(target: "files", "PBGetCatInfoSync(volume={volume_ref}, name_ptr={name_ptr:08X}, dir_id={dir_id}, io_f_dir_index={io_f_dir_index})");

	if io_f_dir_index > 0 {
		// this does a lookup by index in the dir
		panic!("not implemented lol");
	} else {
		// gets info about a dir
		let path = if io_f_dir_index == 0 {
			let name = uc.read_pascal_string(name_ptr)?;
			trace!(target: "files", "PBGetCatInfoSync(volume={volume_ref}, dir_id={dir_id}, name={name:?})");
			state.filesystem.resolve_path(volume_ref, dir_id, name.as_bytes()).unwrap()
		} else {
			trace!(target: "files", "PBGetCatInfoSync(volume={volume_ref}, dir_id={dir_id})");
			state.filesystem.resolve_path(volume_ref, dir_id, b"").unwrap()
		};

		if path.exists() {
			let info = state.filesystem.spec(&path).unwrap();
			if name_ptr != 0 {
				uc.write_pascal_string(name_ptr, &info.node_name)?;
			}
			uc.write_i16(pb + 0x16, info.volume_ref)?;
			uc.write_u8(pb + 0x1E, if path.is_dir() { ATTRIB_DIRECTORY } else { 0 })?;
			uc.write_i32(pb + 0x30, info.node_id)?;
			uc.write_i32(pb + 0x64, info.parent_id)?;
			Ok(Some(0))
		} else {
			Ok(Some(OSErr::FileNotFound.to_u32()))
		}
	}
}

fn pb_h_open_sync(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let pb: u32 = reader.read1(uc)?;

	// this is HParamBlockRec (HFileParam)
	let name = uc.read_pascal_string(uc.read_u32(pb + 0x12)?)?;
	let volume_ref = uc.read_i16(pb + 0x16)?;
	let dir_id = uc.read_i32(pb + 0x30)?;
	trace!(target: "files", "PBHOpenSync(vol={volume_ref}, dir={dir_id}, name={name:?})");

	let path = match state.filesystem.resolve_path(volume_ref, dir_id, name.as_bytes()) {
		Ok(p) => p,
		Err(e) => {
			error!(target: "files", "PBHOpenSync failed to resolve path: {e:?}");
			return Ok(Some(OSErr::BadName.to_u32()));
		}
	};

	let file = match state.filesystem.get_file(&path) {
		Ok(f) => f,
		Err(e) => {
			error!(target: "files", "PBHOpenSync failed to get file: {e:?}");
			return Ok(Some(OSErr::IOError.to_u32()))
		}
	};

	let handle = state.next_file_handle;
	state.next_file_handle += 1;
	state.file_handles.insert(handle, FileHandle {
		file,
		fork: Fork::Data,
		position: 0
	});

	info!(target: "files", "... returned handle {handle}");
	uc.write_u16(pb + 0x18, handle)?;

	Ok(Some(0))
}

fn pb_h_get_f_info_sync(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	// https://web.archive.org/web/20011122050640/http://developer.apple.com/techpubs/mac/Files/Files-240.html#HEADING240-0
	let pb: u32 = reader.read1(uc)?;

	// this is HParamBlockRec (HFileParam)
	let io_f_dir_index = uc.read_i16(pb + 0x1C)?;
	if io_f_dir_index > 0 {
		// this does a lookup by index in the dir
		panic!("not implemented lol");
	} else {
		// looks up info about a file or dir, by name
		let name = uc.read_pascal_string(uc.read_u32(pb + 0x12)?)?;
		let volume_ref = uc.read_i16(pb + 0x16)?;
		let dir_id = uc.read_i32(pb + 0x30)?;
		trace!(target: "files", "PBHGetFInfoSync is looking up dir {dir_id}, name {name:?}");

		let path = match state.filesystem.resolve_path(volume_ref, dir_id, name.as_bytes()) {
			Ok(p) => p,
			Err(e) => {
				error!(target: "files", "PBHGetFInfoSync failed to resolve path: {e:?}");
				return Ok(Some(nice_error(e)));
			}
		};

		let metadata = match std::fs::metadata(&path) {
			Ok(m) => m,
			Err(e) => {
				error!(target: "files", "PBHGetFInfoSync failed to get metadata: {e:?}");
				return Ok(Some(OSErr::IOError.to_u32()));
			}
		};

		let file = match state.filesystem.get_file(&path) {
			Ok(f) => f,
			Err(e) => {
				error!(target: "files", "PBHGetFInfoSync failed to get file: {e:?}");
				return Ok(Some(nice_error(e)))
			}
		};
		let file = file.borrow();

		// I should probably write more of this data...
		uc.write_u8(pb + 0x1E, if metadata.is_dir() { ATTRIB_DIRECTORY } else { 0 })?;
		uc.write_u32(pb + 0x20, file.file_info.file_type.0)?;
		uc.write_u32(pb + 0x24, file.file_info.file_creator.0)?;
		uc.write_u16(pb + 0x28, file.file_info.finder_flags)?;
		uc.write_i16(pb + 0x2A, file.file_info.location.0)?;
		uc.write_i16(pb + 0x2C, file.file_info.location.1)?;
		uc.write_u16(pb + 0x2E, file.file_info.reserved_field)?;

		if let Ok(time) = metadata.created() {
			uc.write_u32(pb + 0x48, system_time_to_mac_time(time))?;
		}
		if let Ok(time) = metadata.modified() {
			uc.write_u32(pb + 0x4C, system_time_to_mac_time(time))?;
		}

		Ok(Some(0))
	}
}

fn pb_h_set_f_info_sync(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let pb: u32 = reader.read1(uc)?;

	// this is HParamBlockRec (HFileParam)
	let name = uc.read_pascal_string(uc.read_u32(pb + 0x12)?)?;
	let volume_ref = uc.read_i16(pb + 0x16)?;
	let dir_id = uc.read_i32(pb + 0x30)?;
	trace!(target: "files", "PBHSetFInfoSync is updating dir {dir_id}, name {name:?}");

	let path = match state.filesystem.resolve_path(volume_ref, dir_id, name.as_bytes()) {
		Ok(p) => p,
		Err(e) => {
			error!(target: "files", "PBHSetFInfoSync failed to resolve path: {e:?}");
			return Ok(Some(nice_error(e)));
		}
	};

	let file = match state.filesystem.get_file(&path) {
		Ok(f) => f,
		Err(e) => {
			error!(target: "files", "PBHSetFInfoSync failed to get file: {e:?}");
			return Ok(Some(nice_error(e)))
		}
	};

	// we don't support setting the metadata times here
	// (use the 'filetime' crate?)
	let mut file_ref = file.borrow_mut();
	file_ref.file_info.file_type.0 = uc.read_u32(pb + 0x20)?;
	file_ref.file_info.file_creator.0 = uc.read_u32(pb + 0x24)?;
	file_ref.file_info.finder_flags = uc.read_u16(pb + 0x28)?;
	file_ref.file_info.location.0 = uc.read_i16(pb + 0x2A)?;
	file_ref.file_info.location.1 = uc.read_i16(pb + 0x2C)?;
	file_ref.file_info.reserved_field = uc.read_u16(pb + 0x2E)?;

	file_ref.set_dirty();
	match file_ref.save_if_dirty() {
		Ok(()) => Ok(Some(0)),
		Err(e) => {
			error!(target: "files", "PBHSetFInfoSync failed to save file: {e:?}");
			Ok(Some(nice_error(e)))
		}
	}
}

fn h_open(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (volume, dir_id, name, permission, ref_num_ptr): (i16, i32, CString, i8, u32) = reader.pstr().read5(uc)?;
	info!(target: "files", "HOpen(vol={volume}, dir={dir_id}, name={name:?}, permission={permission})");

	let path = match state.filesystem.resolve_path(volume, dir_id, name.as_bytes()) {
		Ok(p) => p,
		Err(e) => {
			error!(target: "files", "HOpen failed to resolve path: {e:?}");
			return Ok(Some(OSErr::BadName.to_u32()))
		}
	};

	let file = match state.filesystem.get_file(&path) {
		Ok(f) => f,
		Err(e) => {
			error!(target: "files", "HOpen failed to get file: {e:?}");
			return Ok(Some(OSErr::IOError.to_u32()))
		}
	};

	let handle = state.next_file_handle;
	state.next_file_handle += 1;
	state.file_handles.insert(handle, FileHandle {
		file,
		fork: Fork::Data,
		position: 0
	});

	info!(target: "files", "... returned handle {handle}");
	uc.write_u16(ref_num_ptr, handle)?;
	Ok(Some(0))
}

fn h_create(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (volume_ref, dir_id, name, creator, file_type): (i16, i32, CString, FourCC, FourCC) = reader.pstr().read5(uc)?;
	info!(target: "files", "HCreate(vol={volume_ref}, dir={dir_id}, name={name:?}, creator={creator:?}, type={file_type:?})");

	let path = match state.filesystem.resolve_path(volume_ref, dir_id, name.as_bytes()) {
		Ok(p) => p,
		Err(e) => {
			error!(target: "files", "HCreate failed to resolve path: {e:?}");
			return Ok(Some(OSErr::BadName.to_u32()));
		}
	};

	if path.exists() {
		error!(target: "files", "Path already exists for HCreate: {path:?}");
		return Ok(Some(OSErr::DuplicateFilename.to_u32()));
	}

	match state.filesystem.create_file(&path, creator, file_type) {
		Ok(()) => Ok(Some(0)),
		Err(e) => {
			error!(target: "files", "HCreate failed to create file: {e:?}");
			return Ok(Some(OSErr::IOError.to_u32()));
		}
	}
}

fn h_delete(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (volume_ref, dir_id, name): (i16, i32, CString) = reader.pstr().read3(uc)?;
	info!(target: "files", "HDelete(vol={volume_ref}, dir={dir_id}, name={name:?})");

	let path = match state.filesystem.resolve_path(volume_ref, dir_id, name.as_bytes()) {
		Ok(p) => p,
		Err(e) => {
			error!(target: "files", "HDelete failed to resolve path: {e:?}");
			return Ok(Some(OSErr::BadName.to_u32()));
		}
	};

	// temporary safety check
	let ok_to_delete = match path.extension() {
		Some(e) => e.to_string_lossy() == "o",
		None => false
	};

	if !ok_to_delete {
		error!(target: "files", "HDelete is protecting against a delete of: {path:?}");
		return Ok(Some(OSErr::FileLocked.to_u32()));
	}

	// is this file open at all?
	for handle in state.file_handles.values() {
		if handle.file.borrow().path == path {
			error!(target: "files", "HDelete cannot delete open file: {path:?}");
			return Ok(Some(OSErr::FileBusy.to_u32()));
		}
	}

	match state.filesystem.delete_file(&path) {
		Ok(()) => Ok(Some(0)),
		Err(e) => {
			error!(target: "files", "HDelete failed to delete file: {e:?}");
			Ok(Some(OSErr::IOError.to_u32()))
		}
	}
}

fn h_get_f_info(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (volume, dir_id, name, info_ptr): (i16, i32, CString, u32) = reader.pstr().read4(uc)?;
	info!(target: "files", "HGetFInfo(vol={volume}, dir={dir_id}, name={name:?})");

	let path = match state.filesystem.resolve_path(volume, dir_id, name.as_bytes()) {
		Ok(p) => p,
		Err(e) => {
			error!(target: "files", "HOpen failed to resolve path: {e:?}");
			return Ok(Some(OSErr::BadName.to_u32()))
		}
	};

	let file = match state.filesystem.get_file(&path) {
		Ok(f) => f,
		Err(e) => {
			error!(target: "files", "HOpen failed to get file: {e:?}");
			return Ok(Some(nice_error(e)))
		}
	};

	let f = file.borrow();
	uc.write_u32(info_ptr, f.file_info.file_type.0)?;
	uc.write_u32(info_ptr + 4, f.file_info.file_creator.0)?;
	uc.write_u16(info_ptr + 8, f.file_info.finder_flags)?;
	uc.write_i16(info_ptr + 10, f.file_info.location.0)?;
	uc.write_i16(info_ptr + 12, f.file_info.location.1)?;
	uc.write_u16(info_ptr + 14, 0)?; // fdFldr - do I need this? maybe.
	Ok(Some(0))
}

fn fs_make_fs_spec(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (volume, dir_id, name, spec_ptr): (i16, i32, CString, u32) = reader.pstr().read4(uc)?;
	info!(target: "files", "FSMakeFSSpec(vol={volume}, dir={dir_id}, name={name:?}, spec={spec_ptr:08X})");

	let name = match name.to_str() {
		Ok(n) => n,
		Err(_) => return Ok(Some(OSErr::BadName.to_u32()))
	};

	let path = state.filesystem.resolve_path(volume, dir_id, name.as_bytes()).unwrap();
	let info = state.filesystem.spec(&path).unwrap();

	uc.write_i16(spec_ptr, info.volume_ref)?;
	uc.write_i32(spec_ptr + 2, info.parent_id)?;
	uc.write_pascal_string(spec_ptr + 6, &info.node_name)?;

	if path.exists() {
		Ok(Some(0))
	} else {
		Ok(Some(OSErr::FileNotFound.to_u32()))
	}
}

fn fsp_open_df(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (spec_ptr, permission, ref_num_ptr): (u32, i8, u32) = reader.read3(uc)?;
	let volume = uc.read_i16(spec_ptr)?;
	let dir_id = uc.read_i32(spec_ptr + 2)?;
	let name = uc.read_pascal_string(spec_ptr + 6)?;

	info!(target: "files", "FSpOpenDF(vol={volume}, dir={dir_id}, name={name:?}, permission={permission})");

	let path = match state.filesystem.resolve_path(volume, dir_id, name.as_bytes()) {
		Ok(p) => p,
		Err(e) => {
			error!(target: "files", "FSpOpenDF failed to resolve path: {e:?}");
			return Ok(Some(OSErr::BadName.to_u32()))
		}
	};

	let file = match state.filesystem.get_file(&path) {
		Ok(f) => f,
		Err(e) => {
			error!(target: "files", "FSpOpenDF failed to get file: {e:?}");
			return Ok(Some(nice_error(e)))
		}
	};

	let handle = state.next_file_handle;
	state.next_file_handle += 1;
	state.file_handles.insert(handle, FileHandle {
		file,
		fork: Fork::Data,
		position: 0
	});

	info!(target: "files", "... returned handle {handle}");
	uc.write_u16(ref_num_ptr, handle)?;
	Ok(Some(0))
}

fn fsp_create(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (spec_ptr, creator, file_type, script_tag): (u32, FourCC, FourCC, i16) = reader.read4(uc)?;
	let volume = uc.read_i16(spec_ptr)?;
	let dir_id = uc.read_i32(spec_ptr + 2)?;
	let name = uc.read_pascal_string(spec_ptr + 6)?;

	info!(target: "files", "FSpCreate(vol={volume}, dir={dir_id}, name={name:?}, creator={creator:?}, file_type={file_type:?}, script_tag={script_tag})");

	let path = match state.filesystem.resolve_path(volume, dir_id, name.as_bytes()) {
		Ok(p) => p,
		Err(e) => {
			error!(target: "files", "FSpCreate failed to resolve path: {e:?}");
			return Ok(Some(OSErr::BadName.to_u32()))
		}
	};

	if path.exists() {
		error!(target: "files", "Path already exists for FSpCreate: {path:?}");
		return Ok(Some(OSErr::DuplicateFilename.to_u32()));
	}

	match state.filesystem.create_file(&path, creator, file_type) {
		Ok(()) => Ok(Some(0)),
		Err(e) => {
			error!(target: "files", "FSpCreate failed to create file: {e:?}");
			return Ok(Some(nice_error(e)))
		}
	}
}

fn fsp_delete(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let spec_ptr: u32 = reader.read1(uc)?;
	let volume = uc.read_i16(spec_ptr)?;
	let dir_id = uc.read_i32(spec_ptr + 2)?;
	let name = uc.read_pascal_string(spec_ptr + 6)?;

	info!(target: "files", "FSpDelete(vol={volume}, dir={dir_id}, name={name:?})");

	let path = match state.filesystem.resolve_path(volume, dir_id, name.as_bytes()) {
		Ok(p) => p,
		Err(e) => {
			error!(target: "files", "FSpDelete failed to resolve path: {e:?}");
			return Ok(Some(OSErr::BadName.to_u32()))
		}
	};

	// temporary safety check
	let ok_to_delete = match path.extension() {
		Some(e) => e.to_string_lossy() == "o",
		None => false
	};

	if !ok_to_delete {
		error!(target: "files", "FSpDelete is protecting against a delete of: {path:?}");
		return Ok(Some(OSErr::FileLocked.to_u32()));
	}

	// is this file open at all?
	for handle in state.file_handles.values() {
		if handle.file.borrow().path == path {
			error!(target: "files", "FSpDelete cannot delete open file: {path:?}");
			return Ok(Some(OSErr::FileBusy.to_u32()));
		}
	}

	match state.filesystem.delete_file(&path) {
		Ok(()) => Ok(Some(0)),
		Err(e) => {
			error!(target: "files", "FSpDelete failed to delete file: {e:?}");
			return Ok(Some(nice_error(e)))
		}
	}
}

fn fsp_get_f_info(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (spec_ptr, info_ptr): (u32, u32) = reader.read2(uc)?;
	let volume = uc.read_i16(spec_ptr)?;
	let dir_id = uc.read_i32(spec_ptr + 2)?;
	let name = uc.read_pascal_string(spec_ptr + 6)?;

	info!(target: "files", "FSpGetFInfo(vol={volume}, dir={dir_id}, name={name:?})");

	let path = match state.filesystem.resolve_path(volume, dir_id, name.as_bytes()) {
		Ok(p) => p,
		Err(e) => {
			error!(target: "files", "FSpGetFInfo failed to resolve path: {e:?}");
			return Ok(Some(OSErr::BadName.to_u32()))
		}
	};

	let file = match state.filesystem.get_file(&path) {
		Ok(f) => f,
		Err(e) => {
			error!(target: "files", "FSpGetFInfo failed to get file: {e:?}");
			return Ok(Some(nice_error(e)))
		}
	};

	let f = file.borrow();
	uc.write_u32(info_ptr, f.file_info.file_type.0)?;
	uc.write_u32(info_ptr + 4, f.file_info.file_creator.0)?;
	uc.write_u16(info_ptr + 8, f.file_info.finder_flags)?;
	uc.write_i16(info_ptr + 10, f.file_info.location.0)?;
	uc.write_i16(info_ptr + 12, f.file_info.location.1)?;
	uc.write_u16(info_ptr + 14, 0)?; // fdFldr - do I need this? maybe.
	Ok(Some(0))
}

fn mpw_make_resolved_path(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (volume, dir_id, path, resolve_leaf_name, buffer_ptr, is_folder_ptr, had_alias_ptr, leaf_is_alias_ptr): (i16, i32, CString, bool, u32, u32, u32, u32) = reader.pstr().read8(uc)?;
	trace!(target: "files", "MakeResolvedPath(vol={volume}, dir={dir_id}, path={path:?}, resolve_leaf_name={resolve_leaf_name}, buffer={buffer_ptr:08X}, ...)");

	let fs_path = state.filesystem.resolve_path(volume, dir_id, path.as_bytes()).unwrap();
	trace!(target: "files", "..resolved to {fs_path:?}");
	if fs_path.exists() {
		trace!(target: "files", "..exists!");
		// is this correct? not sure.
		uc.write_pascal_string(buffer_ptr, path.as_bytes())?;
		uc.write_u8(is_folder_ptr, if fs_path.is_dir() { 1 } else { 0 })?;
		uc.write_u8(had_alias_ptr, 0)?;
		uc.write_u8(leaf_is_alias_ptr, 0)?;
		Ok(Some(0))
	} else {
		Ok(Some(OSErr::FileNotFound.to_u32()))
	}
}

fn mpw_make_resolved_fs_spec(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (volume, dir_id, name, spec_ptr, is_folder_ptr, had_alias_ptr, leaf_is_alias_ptr): (i16, i32, CString, u32, u32, u32, u32) = reader.pstr().read7(uc)?;
	trace!(target: "files", "MakeResolvedFSSpec(vol={volume}, dir={dir_id}, name={name:?}, spec={spec_ptr:08X}, ...)");

	let path = state.filesystem.resolve_path(volume, dir_id, name.as_bytes()).unwrap();

	// Write a spec
	let info = state.filesystem.spec(&path).unwrap();
	uc.write_i16(spec_ptr, info.volume_ref)?;
	uc.write_i32(spec_ptr + 2, info.parent_id)?;
	uc.write_pascal_string(spec_ptr + 6, &info.node_name)?;
	uc.write_u8(is_folder_ptr, if path.is_dir() { 1 } else { 0 })?;
	uc.write_u8(had_alias_ptr, 0)?;
	uc.write_u8(leaf_is_alias_ptr, 0)?;
	Ok(Some(0))
}

fn resolve_alias_file(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (spec_ptr, resolve_alias_chains, target_is_folder_ptr, was_aliased_ptr): (u32, bool, u32, u32) = reader.read4(uc)?;
	let volume_ref = uc.read_i16(spec_ptr)?;
	let dir_id = uc.read_i32(spec_ptr + 2)?;
	let name = uc.read_pascal_string(spec_ptr + 6)?;
	trace!(target: "files", "ResolveAliasFile(spec={spec_ptr:08X}, resolve_alias_chains={resolve_alias_chains} - vol={volume_ref}, dir_id={dir_id}, name={name:?})");

	let path = match state.filesystem.resolve_path(volume_ref, dir_id, name.as_bytes()) {
		Ok(p) => p,
		Err(e) => {
			error!(target: "files", "ResolveAliasFile failed to resolve path: {e:?}");
			return Ok(Some(OSErr::DirNotFound.to_u32()));
		}
	};

	if path.exists() {
		// Yep
		let info = state.filesystem.spec(&path).unwrap();
		uc.write_i16(spec_ptr, info.volume_ref)?;
		uc.write_i32(spec_ptr + 2, info.parent_id)?;
		uc.write_pascal_string(spec_ptr + 6, &info.node_name)?;

		uc.write_u8(target_is_folder_ptr, if path.is_dir() { 1 } else { 0 })?;
		uc.write_u8(was_aliased_ptr, 0)?;
		Ok(Some(0))
	} else {
		Ok(Some(OSErr::FileNotFound.to_u32()))
	}
}

pub(super) fn install_shims(state: &mut EmuState) {
	state.install_shim_function("PBOpenRFSync", pb_open_rf_sync);
	// PBCloseSync
	// PBReadSync
	state.install_shim_function("PBWriteSync", pb_write_sync);
	state.install_shim_function("FSClose", fs_close);
	state.install_shim_function("FSRead", fs_read);
	state.install_shim_function("FSWrite", fs_write);
	state.install_shim_function("GetVInfo", get_v_info);
	state.install_shim_function("GetEOF", get_eof);
	state.install_shim_function("SetEOF", set_eof);
	state.install_shim_function("GetFPos", get_f_pos);
	state.install_shim_function("SetFPos", set_f_pos);
	state.install_shim_function("PBGetCatInfoSync", pb_get_cat_info_sync);
	state.install_shim_function("PBHOpenSync", pb_h_open_sync);
	state.install_shim_function("PBHGetFInfoSync", pb_h_get_f_info_sync);
	state.install_shim_function("PBHSetFInfoSync", pb_h_set_f_info_sync);
	state.install_shim_function("HOpen", h_open);
	state.install_shim_function("HCreate", h_create);
	state.install_shim_function("HDelete", h_delete);
	state.install_shim_function("HGetFInfo", h_get_f_info);
	state.install_shim_function("FSMakeFSSpec", fs_make_fs_spec);
	state.install_shim_function("FSpOpenDF", fsp_open_df);
	state.install_shim_function("FSpCreate", fsp_create);
	state.install_shim_function("FSpDelete", fsp_delete);
	state.install_shim_function("FSpGetFInfo", fsp_get_f_info);

	// not actually in Files.h but we'll let it slide.
	// Aliases.h
	state.install_shim_function("ResolveAliasFile", resolve_alias_file);

	// these are from MPW itself
	state.install_shim_function("MakeResolvedPath", mpw_make_resolved_path);
	state.install_shim_function("MakeResolvedFSSpec", mpw_make_resolved_fs_spec);
}
