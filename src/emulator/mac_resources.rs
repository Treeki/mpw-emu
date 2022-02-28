use crate::common::FourCC;

use super::{EmuState, EmuUC, FuncResult, helpers::{ArgReader, UnicornExtras}};

fn get_resource(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
    let (ty, id): (FourCC, i16) = reader.read2(uc)?;
    let cache_key = (ty, id);

	if let Some(&handle) = state.loaded_resources.get(&cache_key) {
        // easy mode
        return Ok(Some(handle));
	}

	if let Some(res) = state.resources.get(ty, id) {
		let handle = state.heap.new_handle(uc, res.data.len() as u32)?;
		if handle != 0 {
			let ptr = uc.read_u32(handle)?;
			uc.mem_write(ptr.into(), &res.data)?;
			state.loaded_resources.insert(cache_key, handle);
			return Ok(Some(handle));
		}
	}

	// should set ResError here
	Ok(Some(0))
}

pub(super) fn install_shims(state: &mut EmuState) {
	state.install_shim_function("GetResource", get_resource);
	state.install_shim_function("Get1Resource", get_resource);
}
