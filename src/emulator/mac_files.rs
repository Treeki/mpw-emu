use std::{ffi::CString, io::{Read, Write, Seek, SeekFrom}};

use crate::{filesystem::{FSResult, Info}, common::{OSErr, FourCC, four_cc}};

use super::{EmuState, EmuUC, FuncResult, helpers::{ArgReader, UnicornExtras}};

fn get_assumed_type_and_creator_id(name: &str) -> (FourCC, FourCC) {
	if name.ends_with(".o") {
		(four_cc(*b"MPLF"), four_cc(*b"CWIE"))
	} else {
		(four_cc(*b"TEXT"), four_cc(*b"ttxt"))
	}
}

fn fs_close(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let ref_num: u16 = reader.read1(uc)?;

	match state.file_handles.remove(&ref_num) {
		Some(_) => Ok(Some(0)),
		None => Ok(Some(OSErr::RefNum.to_u32())) // file not opened
	}
}

fn fs_read(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (ref_num, count_ptr, buf_ptr): (u16, u32, u32) = reader.read3(uc)?;

	let file = match state.file_handles.get_mut(&ref_num) {
		Some(f) => f,
		None => return Ok(Some(OSErr::RefNum.to_u32()))
	};

	let count = uc.read_u32(count_ptr)? as usize;
	let mut buf = Vec::new();
	buf.resize(count, 0);

	let read_amount = match file.read(&mut buf) {
		Ok(amt) => amt,
		Err(e) => {
			error!(target: "files", "FSRead internal error: {e:?}");
			return Ok(Some(OSErr::IOError.to_u32()));
		}
	};

	uc.mem_write(buf_ptr.into(), &buf[0..read_amount])?;
	uc.write_u32(count_ptr, read_amount as u32)?;

	if read_amount < count {
		Ok(Some(OSErr::Eof.to_u32()))
	} else {
		Ok(Some(0))
	}
}

fn fs_write(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (ref_num, count_ptr, buf_ptr): (u16, u32, u32) = reader.read3(uc)?;

	let file = match state.file_handles.get_mut(&ref_num) {
		Some(f) => f,
		None => return Ok(Some(OSErr::RefNum.to_u32()))
	};

	let count = uc.read_u32(count_ptr)? as usize;
	let buf = uc.mem_read_as_vec(buf_ptr.into(), count)?;

	let written_amount = match file.write(&buf) {
		Ok(amt) => amt,
		Err(e) => {
			error!(target: "files", "FSWrite internal error: {e:?}");
			return Ok(Some(OSErr::IOError.to_u32()));
		}
	};

	uc.write_u32(count_ptr, written_amount as u32)?;

	if written_amount < count {
		Ok(Some(OSErr::Eof.to_u32()))
	} else {
		Ok(Some(0))
	}
}

fn get_v_info(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (_volume, name_ptr, ref_num_ptr, free_bytes_ptr): (i16, u32, u32, u32) = reader.read4(uc)?;

	// give them placeholder info for now
	uc.write_pascal_string(name_ptr, b"Root")?;
	uc.write_u16(ref_num_ptr, 0)?;
	uc.write_u32(free_bytes_ptr, 0x100000)?;

	Ok(Some(0))
}

fn length_of_file(file: &mut std::fs::File) -> std::io::Result<u64> {
	let pos = file.stream_position()?;
	let eof = file.seek(SeekFrom::End(0))?;
	file.seek(SeekFrom::Start(pos))?;
	Ok(eof)
}

fn get_eof(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (ref_num, eof_ptr): (u16, u32) = reader.read2(uc)?;

	if let Some(file) = state.file_handles.get_mut(&ref_num) {
		match length_of_file(file) {
			Ok(eof) => {
				uc.write_u32(eof_ptr, eof as u32)?;
				Ok(Some(0))
			}
			Err(_) => Ok(Some(OSErr::IOError.to_u32()))
		}
	} else {
		Ok(Some(OSErr::RefNum.to_u32()))
	}
}

fn get_f_pos(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (ref_num, pos_ptr): (u16, u32) = reader.read2(uc)?;

	if let Some(file) = state.file_handles.get_mut(&ref_num) {
		match file.stream_position() {
			Ok(pos) => {
				uc.write_u32(pos_ptr, pos as u32)?;
				Ok(Some(0))
			}
			Err(_) => Ok(Some(OSErr::IOError.to_u32()))
		}
	} else {
		Ok(Some(OSErr::RefNum.to_u32()))
	}
}

fn set_f_pos(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (ref_num, pos_mode, pos_offset): (u16, i16, i32) = reader.read3(uc)?;

	if let Some(file) = state.file_handles.get_mut(&ref_num) {
		let seek = match pos_mode {
			1 => SeekFrom::Start(pos_offset as u64),
			2 => SeekFrom::End(pos_offset as i64),
			3 => SeekFrom::Current(pos_offset as i64),
			_ => SeekFrom::Current(0)
		};
		match file.seek(seek) {
			Ok(_) => Ok(Some(0)),
			Err(_) => Ok(Some(OSErr::IOError.to_u32()))
		}
	} else {
		Ok(Some(OSErr::RefNum.to_u32()))
	}
}

fn pb_get_cat_info_sync(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let pb: u32 = reader.read1(uc)?;

	let io_f_dir_index = uc.read_i16(pb + 0x1C)?;
	if io_f_dir_index > 0 {
		// this does a lookup by index in the dir
		panic!("not implemented lol");
	} else if io_f_dir_index == 0 {
		// looks up info about a file or dir, by name
		let dir_id = uc.read_u32(pb + 0x30)?;
		let name = uc.read_pascal_string(uc.read_u32(pb + 0x12)?)?;
		let name = name.to_str().expect("Failed to decode filename");
		info!(target: "files", "PBGetCatInfoSync is looking up dir {dir_id}, name {name}");

		match state.filesystem.get_subnode_info(dir_id, &name) {
			FSResult::Ok(Info::Directory { id, .. }) => {
				uc.write_u8(pb + 0x1E, 0x10)?; // ioFlAttrib
				uc.write_u32(pb + 0x30, id)?; // ioDrDirID
				Ok(Some(0))
			}
			FSResult::Ok(Info::File { .. }) => {
				uc.write_u8(pb + 0x1E, 0)?; // ioFlAttrib
				uc.write_u32(pb + 0x30, 0)?; // ioDirID
				Ok(Some(0))
			}
			FSResult::Err(e) => Ok(Some(e.to_u32()))
		}
	} else {
		// gets info about a dir
		let dir_id = uc.read_u32(pb + 0x30)?;
		let name_ptr = uc.read_u32(pb + 0x12)?;
		info!(target: "files", "PBGetCatInfoSync is looking up dir {dir_id}");

		match state.filesystem.get_info_by_id(dir_id) {
			FSResult::Ok(Info::Directory { id, parent_id, name, .. }) => {
				if name_ptr != 0 {
					let name = name.unwrap();
					uc.write_pascal_string(name_ptr, name.as_bytes())?;
				}
				uc.write_u8(pb + 0x1E, 0x10)?; // ioFlAttrib
				uc.write_u32(pb + 0x30, id)?; // ioDrDirID
				uc.write_u32(pb + 0x64, parent_id)?; // ioDrParID
				uc.write_u8(pb + 0x1E, 0x10)?; // ioFlAttrib
				Ok(Some(0))
			}
			FSResult::Ok(Info::File { parent_id, name }) => {
				if name_ptr != 0 {
					let name = name.unwrap();
					uc.write_pascal_string(name_ptr, name.as_bytes())?;
				}
				uc.write_u8(pb + 0x1E, 0)?; // ioFlAttrib
				uc.write_u32(pb + 0x30, 0)?; // ioDirID
				uc.write_u32(pb + 0x64, parent_id)?; // ioFlParID
				Ok(Some(0))
			}
			FSResult::Err(e) => Ok(Some(e.to_u32()))
		}
	}
}

fn h_open(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (volume, dir_id, name, permission, ref_num_ptr): (i16, u32, CString, i8, u32) = reader.pstr().read5(uc)?;
	info!(target: "files", "HOpen(vol={volume}, dir={dir_id}, name={name:?}, permission={permission})");

	let name = match name.to_str() {
		Ok(n) => n,
		Err(_) => return Ok(Some(OSErr::BadName.to_u32()))
	};

	match state.filesystem.h_open(dir_id, name, permission) {
		FSResult::Ok(file) => {
			let handle = state.next_file_handle;
			state.next_file_handle += 1;
			state.file_handles.insert(handle, file);
			uc.write_u16(ref_num_ptr, handle)?;
			Ok(Some(0))
		}
		FSResult::Err(e) => Ok(Some(e.to_u32()))
	}
}

fn h_create(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (volume, dir_id, name, creator, file_type): (i16, u32, CString, FourCC, FourCC) = reader.pstr().read5(uc)?;
	info!(target: "files", "HCreate(vol={volume}, dir={dir_id}, name={name:?}, creator={creator:?}, type={file_type:?})");

	let name = match name.to_str() {
		Ok(n) => n,
		Err(_) => return Ok(Some(OSErr::BadName.to_u32()))
	};

	match state.filesystem.h_create(dir_id, name) {
		FSResult::Ok(()) => Ok(Some(0)),
		FSResult::Err(e) => Ok(Some(e.to_u32()))
	}
}

fn h_delete(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (volume, dir_id, name): (i16, u32, CString) = reader.pstr().read3(uc)?;
	info!(target: "files", "HDelete(vol={volume}, dir={dir_id}, name={name:?}) -- currently ignored");

	Ok(Some(0))
}

fn h_get_f_info(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (volume, dir_id, name, info_ptr): (i16, u32, CString, u32) = reader.pstr().read4(uc)?;
	info!(target: "files", "HGetFInfo(vol={volume}, dir={dir_id}, name={name:?})");

	let name = match name.to_str() {
		Ok(n) => n,
		Err(_) => return Ok(Some(OSErr::BadName.to_u32()))
	};

	match state.filesystem.get_subnode_info(dir_id, name) {
		FSResult::Ok(_) => {
			let (ty, creator) = get_assumed_type_and_creator_id(name);
			uc.write_u32(info_ptr, ty.0)?;
			uc.write_u32(info_ptr + 4, creator.0)?;
			uc.write_u32(info_ptr + 8, 0)?;
			uc.write_u32(info_ptr + 12, 0)?;
			Ok(Some(0))
		}
		FSResult::Err(e) => Ok(Some(e.to_u32()))
	}
}

fn fs_make_fs_spec(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (volume, dir_id, name, spec_ptr): (i16, u32, CString, u32) = reader.pstr().read4(uc)?;
	info!(target: "files", "FSMakeFSSpec(vol={volume}, dir={dir_id}, name={name:?}, spec={spec_ptr:08X})");

	let name = match name.to_str() {
		Ok(n) => n,
		Err(_) => return Ok(Some(OSErr::BadName.to_u32()))
	};

	match state.filesystem.make_fs_spec(dir_id, name) {
		FSResult::Ok(spec) => {
			uc.write_i16(spec_ptr, 0)?;
			uc.write_u32(spec_ptr + 2, spec.parent_id)?;
			uc.write_pascal_string(spec_ptr + 6, spec.name.as_bytes())?;

			if spec.exists {
				Ok(Some(0))
			} else {
				Ok(Some(OSErr::FileNotFound.to_u32()))
			}
		}
		FSResult::Err(e) => Ok(Some(e.to_u32()))
	}
}

fn fsp_open_df(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (spec_ptr, permission, ref_num_ptr): (u32, i8, u32) = reader.read3(uc)?;
	let volume = uc.read_u16(spec_ptr)?;
	let dir_id = uc.read_u32(spec_ptr + 2)?;
	let name = uc.read_pascal_string(spec_ptr + 6)?;

	info!(target: "files", "FSpOpenDF(vol={volume}, dir={dir_id}, name={name:?}, permission={permission})");

	let name = match name.to_str() {
		Ok(n) => n,
		Err(_) => return Ok(Some(OSErr::BadName.to_u32()))
	};

	match state.filesystem.h_open(dir_id, name, permission) {
		FSResult::Ok(file) => {
			let handle = state.next_file_handle;
			state.next_file_handle += 1;
			state.file_handles.insert(handle, file);
			uc.write_u16(ref_num_ptr, handle)?;
			Ok(Some(0))
		}
		FSResult::Err(e) => Ok(Some(e.to_u32()))
	}
}

fn make_resolved_fs_spec(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (volume, dir_id, name, spec_ptr, is_folder_ptr, had_alias_ptr, leaf_is_alias_ptr): (i16, u32, CString, u32, u32, u32, u32) = reader.pstr().read7(uc)?;
	info!(target: "files", "MakeResolvedFSSpec(vol={volume}, dir={dir_id}, name={name:?}, spec={spec_ptr:08X}, ...)");

	let name = match name.to_str() {
		Ok(n) => n,
		Err(_) => return Ok(Some(OSErr::BadName.to_u32()))
	};

	match state.filesystem.make_resolved_fs_spec(dir_id, name) {
		Some(spec) => {
			uc.write_i16(spec_ptr, 0)?;
			uc.write_u32(spec_ptr + 2, spec.parent_id)?;
			uc.write_pascal_string(spec_ptr + 6, spec.name.as_bytes())?;
			uc.write_u8(is_folder_ptr, if spec.is_folder { 1 } else { 0 })?;
			uc.write_u8(had_alias_ptr, 0)?;
			uc.write_u8(leaf_is_alias_ptr, 0)?;

			Ok(Some(0))
		}
		None => Ok(Some(OSErr::FileNotFound.to_u32()))
	}
}

pub(super) fn install_shims(state: &mut EmuState) {
	// PBCloseSync
	// PBReadSync
	state.install_shim_function("FSClose", fs_close);
	state.install_shim_function("FSRead", fs_read);
	state.install_shim_function("FSWrite", fs_write);
	state.install_shim_function("GetVInfo", get_v_info);
	state.install_shim_function("GetEOF", get_eof);
	state.install_shim_function("GetFPos", get_f_pos);
	state.install_shim_function("SetFPos", set_f_pos);
	state.install_shim_function("PBGetCatInfoSync", pb_get_cat_info_sync);
	// PBHOpenSync
	// PBHGetFInfoSync
	state.install_shim_function("HOpen", h_open);
	state.install_shim_function("HCreate", h_create);
	state.install_shim_function("HDelete", h_delete);
	state.install_shim_function("HGetFInfo", h_get_f_info);
	state.install_shim_function("FSMakeFSSpec", fs_make_fs_spec);
	state.install_shim_function("FSpOpenDF", fsp_open_df);

	// not actually in Files.h but we'll let it slide.
	state.install_shim_function("MakeResolvedFSSpec", make_resolved_fs_spec);
}
