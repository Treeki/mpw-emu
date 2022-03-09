use std::ffi::CString;

use crate::{common::{FourCC, OSErr, four_cc}, resources};

use super::{EmuState, EmuUC, FuncResult, helpers::{ArgReader, UnicornExtras}, UcResult};

fn update_res_file_internal(uc: &mut EmuUC, state: &mut EmuState, ref_num: u16) -> UcResult<bool> {
	let resources = state.resource_files.get_mut(&ref_num).unwrap();

	// restore all loaded resources
	for (cache_key, &handle) in state.loaded_resources.iter() {
		if cache_key.0 == ref_num {
			let res = resources.get(cache_key.1, cache_key.2).unwrap();
			let mut res = res.borrow_mut();

			let ptr = uc.read_u32(handle)?;
			let size = state.heap.get_handle_size(uc, handle)?.unwrap();
			res.data.resize(size as usize, 0);
			uc.mem_read(ptr.into(), &mut res.data)?;
		}
	}

	resources.save_to_file();
	let mut file = resources.file.borrow_mut();
	match file.save_if_dirty() {
		Ok(()) => Ok(true),
		Err(e) => {
			error!(target: "resources", "failed to save modified resources file {:?}: {:?}", file.path, e);
			Ok(false)
		}
	}
}

fn close_res_file(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let ref_num: u16 = reader.read1(uc)?;

	trace!(target: "resources", "CloseResFile({ref_num})");

	if state.resource_files.contains_key(&ref_num) {
		state.res_error = OSErr::NoError;

		// stage 1: update
		if !update_res_file_internal(uc, state, ref_num)? {
			state.res_error = OSErr::IOError;
			return Ok(None);
		}

		// stage 2: get rid of all loaded resources from this fork
		for (cache_key, &handle) in state.loaded_resources.iter() {
			if cache_key.0 == ref_num {
				state.heap.dispose_handle(uc, handle)?;
			}
		}
		state.loaded_resources.retain(|cache_key, _| cache_key.0 != ref_num);

		// stage 3: get rid of the resources
		state.resource_files.remove(&ref_num);

		if state.active_resource_file == ref_num {
			// find another one to put in
			state.active_resource_file = *state.resource_files.keys().max().unwrap();
			trace!(target: "resources", "Active resource file has been set to {}", state.active_resource_file);
		}
	} else {
		state.res_error = OSErr::ResFileNotFound;
	}

	Ok(None)
}

fn res_error(_uc: &mut EmuUC, state: &mut EmuState, _reader: &mut ArgReader) -> FuncResult {
	Ok(Some(state.res_error as u32))
}

fn cur_res_file(_uc: &mut EmuUC, state: &mut EmuState, _reader: &mut ArgReader) -> FuncResult {
	Ok(Some(state.active_resource_file as u32))
}

fn use_res_file(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let new_file: u16 = reader.read1(uc)?;

	if state.resource_files.contains_key(&new_file) {
		let old_file = state.active_resource_file;
		trace!(target: "resources", "changing resource files from {old_file} to {new_file}");
		state.active_resource_file = new_file;
		state.res_error = OSErr::NoError;
	} else {
		state.res_error = OSErr::ResFileNotFound;
	}

	Ok(None)
}

fn set_res_load(_uc: &mut EmuUC, _state: &mut EmuState, _reader: &mut ArgReader) -> FuncResult {
	// do nothing here...
	Ok(Some(0))
}

fn get_resource(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
    let (ty, id): (FourCC, i16) = reader.read2(uc)?;
    let cache_key = (state.active_resource_file, ty, id);

	trace!(target: "resources", "GetResource({ty:?}, {id}) [active file is {}]", state.active_resource_file);

	state.res_error = OSErr::NoError;

	if let Some(&handle) = state.loaded_resources.get_by_left(&cache_key) {
        // easy mode
        return Ok(Some(handle));
	}

	let resources = state.resource_files.get(&state.active_resource_file).unwrap();
	if let Some(res) = resources.get(ty, id) {
		let res = res.borrow();
		let handle = state.heap.new_handle(uc, res.data.len() as u32)?;
		if handle != 0 {
			let ptr = uc.read_u32(handle)?;
			uc.mem_write(ptr.into(), &res.data)?;
			state.loaded_resources.insert(cache_key, handle);
			return Ok(Some(handle));
		}
	}

	state.res_error = OSErr::ResNotFound;
	Ok(Some(0))
}

fn release_resource(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let handle: u32 = reader.read1(uc)?;
	match state.loaded_resources.get_by_right(&handle) {
		Some(_) => {
			state.res_error = OSErr::NoError;

			// lose it
			state.heap.dispose_handle(uc, handle)?;
			state.loaded_resources.remove_by_right(&handle);

			// might need to keep track of ResChanged on the resource...?
			// "Be aware that ReleaseResource won't release a resource whose resChanged attribute has been set, but ResError still returns the result code noErr."
		}
		None => {
			state.res_error = OSErr::ResNotFound;
		}
	}

	Ok(None)
}

fn detach_resource(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let handle: u32 = reader.read1(uc)?;
	match state.loaded_resources.get_by_right(&handle) {
		Some(_) => {
			state.res_error = OSErr::NoError;
			state.loaded_resources.remove_by_right(&handle);
		}
		None => {
			state.res_error = OSErr::ResNotFound;
		}
	}

	Ok(None)
}

fn add_resource(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (data_handle, type_id, res_id, name_ptr): (u32, FourCC, i16, u32) = reader.read4(uc)?;

	trace!(target: "resources", "AddResource(data={data_handle:08X}, type={type_id:?}, id={res_id}, name_ptr={name_ptr:08X})");
	state.res_error = OSErr::AddResFailed;

	if data_handle == 0 {
		error!(target: "resources", "AddResource called with nil handle");
		return Ok(None);
	}
	if state.loaded_resources.contains_right(&data_handle) {
		error!(target: "resources", "AddResource called with a handle that's already attached to a resource");
		return Ok(None);
	}

	let name = if name_ptr == 0 {
		None
	} else {
		Some(uc.read_pascal_string(name_ptr)?.into_bytes())
	};

	let resources = state.resource_files.get_mut(&state.active_resource_file).unwrap();
	match resources.add(type_id, res_id, name) {
		Some(res) => {
			let mut res = res.borrow_mut();

			let data_ptr = uc.read_u32(data_handle)?;
			let data_size = state.heap.get_handle_size(uc, data_handle)?.unwrap();
			res.data.resize(data_size as usize, 0);
			uc.mem_read(data_ptr.into(), &mut res.data)?;

			// we've taken ownership of the handle for this
			let cache_key = (state.active_resource_file, type_id, res_id);
			state.loaded_resources.insert(cache_key, data_handle);
		}
		None => {
			error!(target: "resources", "AddResource trying to add a duplicate resource ({type_id:?}, {res_id})");
			return Ok(None);
		}
	}

	state.res_error = OSErr::NoError;
	Ok(None)
}

fn remove_resource(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let handle: u32 = reader.read1(uc)?;

	match state.loaded_resources.get_by_right(&handle) {
		Some(&cache_key) => {
			// technically, we may be leaking a handle here, but that's probably fine for our needs
			state.res_error = OSErr::NoError;
			state.loaded_resources.remove_by_right(&handle);

			let rf = state.resource_files.get_mut(&cache_key.0).unwrap();
			rf.remove(cache_key.1, cache_key.2);
		}
		None => {
			state.res_error = OSErr::ResNotFound;
		}
	}

	Ok(None)
}

fn update_res_file(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let ref_num: u16 = reader.read1(uc)?;

	if state.resource_files.contains_key(&ref_num) {
		if update_res_file_internal(uc, state, ref_num)? {
			state.res_error = OSErr::NoError;
		} else {
			state.res_error = OSErr::IOError;
		}
	} else {
		state.res_error = OSErr::ResNotFound;
	}

	Ok(None)
}

fn h_create_res_file(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (volume_ref, dir_id, name): (i16, i32, CString) = reader.pstr().read3(uc)?;
	info!(target: "resources", "HCreateResFile(vol={volume_ref}, dir={dir_id}, name={name:?})");

	let path = match state.filesystem.resolve_path(volume_ref, dir_id, name.as_bytes()) {
		Ok(p) => p,
		Err(e) => {
			error!(target: "resources", "HCreateResFile failed to resolve path: {e:?}");
			state.res_error = OSErr::BadName;
			return Ok(None);
		}
	};

	if !path.exists() {
		// make an empty file
		match state.filesystem.create_file(&path, four_cc(*b"????"), four_cc(*b"????")) {
			Ok(()) => {}
			Err(e) => {
				error!(target: "resources", "HCreateResFile failed to create file: {e:?}");
				state.res_error = OSErr::IOError;
				return Ok(None);
			}
		}
	}

	match state.filesystem.get_file(&path) {
		Ok(f) => {
			let mut file = f.borrow_mut();
			if file.resource_fork.is_empty() {
				trace!(target: "resources", "creating empty resource map");

				// create an empty resource map
				file.resource_fork.resize(286, 0);

				// main header
				file.resource_fork[2] = 1; // set data offset to 0x100
				file.resource_fork[6] = 1; // set map offset to 0x100
				file.resource_fork[15] = 30; // set map size to 30 bytes

				// copy of header
				file.resource_fork[256 + 2] = 1; // set data offset to 0x100
				file.resource_fork[256 + 6] = 1; // set map offset to 0x100
				file.resource_fork[256 + 15] = 30; // set map size to 30 bytes
				file.resource_fork[256 + 25] = 28; // type list offset
				file.resource_fork[256 + 27] = 30; // name list offset
				file.resource_fork[256 + 28] = 0xFF; // number of types is -1
				file.resource_fork[256 + 29] = 0xFF; // number of types is -1

				file.set_dirty();
			} else {
				trace!(target: "resources", "resource map already existed");
			}

			state.res_error = OSErr::NoError;
		},
		Err(e) => {
			error!(target: "resources", "HCreateResFile failed to get file: {e:?}");
			state.res_error = OSErr::IOError;
		}
	}

	Ok(None)
}

fn f_sp_open_res_file(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (spec_ptr, permission): (u32, i8) = reader.read2(uc)?;
	let volume = uc.read_i16(spec_ptr)?;
	let dir_id = uc.read_i32(spec_ptr + 2)?;
	let name = uc.read_pascal_string(spec_ptr + 6)?;

	info!(target: "resources", "FSpOpenResFile(vol={volume}, dir={dir_id}, name={name:?}, permission={permission})");

	let path = match state.filesystem.resolve_path(volume, dir_id, name.as_bytes()) {
		Ok(p) => p,
		Err(e) => {
			error!(target: "resources", "FSpOpenResFile failed to resolve path: {e:?}");
			state.res_error = OSErr::BadName;
			return Ok(Some(0xFFFFFFFF));
		}
	};

	let file = match state.filesystem.get_file(&path) {
		Ok(f) => f,
		Err(e) => {
			error!(target: "resources", "FSpOpenResFile failed to get file: {e:?}");
			state.res_error = OSErr::FileNotFound;
			return Ok(Some(0xFFFFFFFF));
		}
	};

	// Oh boy this is fun...
	let resources = match resources::parse_resources(file) {
		Ok(r) => r,
		Err(e) => {
			error!(target: "resources", "FSpOpenResFile failed to parse resource fork: {e:?}");
			state.res_error = OSErr::MapRead;
			return Ok(Some(0xFFFFFFFF));
		}
	};

	let rf_id = state.next_resource_file;
	state.next_resource_file += 1;
	state.resource_files.insert(rf_id, resources);
	state.active_resource_file = rf_id;
	state.res_error = OSErr::NoError;

	info!(target: "resources", "... returned handle {rf_id}, made active");
	Ok(Some(rf_id as u32))
}

pub(super) fn install_shims(state: &mut EmuState) {
	state.install_shim_function("CloseResFile", close_res_file);
	state.install_shim_function("ResError", res_error);
	state.install_shim_function("CurResFile", cur_res_file);
	// short HomeResFile(Handle theResource)
	// void CreateResFile(ConstStr255Param fileName)
	// short OpenResFile(ConstStr255Param fileName)
	state.install_shim_function("UseResFile", use_res_file);
	// short CountTypes()
	// short Count1Types()
	// void GetIndType(ResType *theType, short index)
	// void Get1IndType(ResType *theType, short index)
	state.install_shim_function("SetResLoad", set_res_load);
	// short CountResources(ResType theType)
	// short Count1Resources(ResType theType)
	// Handle GetIndResource(ResType theType, short index)
	// Handle Get1IndResource(ResType theType, short index)
	state.install_shim_function("GetResource", get_resource);
	state.install_shim_function("Get1Resource", get_resource);
	// Handle GetNamedResource(ResType theType, ConstStr255Param name)
	// Handle Get1NamedResource(ResType theType, ConstStr255Param name)
	// void LoadResource(Handle theResource)
	state.install_shim_function("ReleaseResource", release_resource);
	state.install_shim_function("DetachResource", detach_resource);
	// short UniqueID(ResType theType)
	// short Unique1ID(ResType theType)
	// short GetResAttrs(Handle theResource)
	// void GetResInfo(Handle theResource, short *theID, ResType *theType, Str255 name)
	// void SetResInfo(Handle theResource, short theID, ConstStr255Param name)
	state.install_shim_function("AddResource", add_resource);
	// long GetResourceSizeOnDisk(Handle theResource)
	// long GetMaxResourceSize(Handle theResource)
	// long RsrcMapEntry(Handle theResource)
	// void SetResAttrs(Handle theResource, short attrs)
	// void ChangedResource(Handle theResource)
	state.install_shim_function("RemoveResource", remove_resource);
	state.install_shim_function("UpdateResFile", update_res_file);
	// void WriteResource(Handle theResource)
	state.install_shim_function("HCreateResFile", h_create_res_file);
	state.install_shim_function("FSpOpenResFile", f_sp_open_res_file);
}
