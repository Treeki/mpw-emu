use crate::common::four_cc;

use super::{EmuState, EmuUC, FuncResult, helpers::{ArgReader, UnicornExtras}};

fn get_ind_string(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (ptr, table_id, mut str_id): (u32, i16, i16) = reader.read3(uc)?;

	if let Some(res) = state.resources.get(four_cc(*b"STR#"), table_id) {
		let mut offset = 2;
		while offset < res.data.len() && str_id > 1 {
			str_id -= 1;
			offset += res.data[offset] as usize;
			offset += 1;
		}

		let len = res.data[offset] as usize;
		let s = &res.data[offset .. offset + len + 1];
		uc.mem_write(ptr.into(), s)?;
	} else {
		warn!(target: "text_utils", "GetIndString failed to find STR# table {table_id}");
	}

	Ok(None)
}

fn numtostring(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (num, ptr): (i32, u32) = reader.read2(uc)?;
	let s = format!("{}", num);
	uc.write_c_string(ptr, s.as_bytes())?;
	Ok(None)
}

fn c2pstr(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let ptr: u32 = reader.read1(uc)?;
	let mut i = 0;
	let mut c = uc.read_u8(ptr)?;
	while c != 0 {
		let next_c = uc.read_u8(ptr + i + 1)?;
		uc.write_u8(ptr + i + 1, c)?;
		c = next_c;
		i += 1;
	}
	uc.write_u8(ptr, i as u8)?;
	Ok(Some(ptr))
}

fn p2cstr(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let ptr: u32 = reader.read1(uc)?;
	let len = uc.read_u8(ptr)? as u32;
	for i in 0..len {
		uc.write_u8(ptr + i, uc.read_u8(ptr + i + 1)?)?;
	}
	uc.write_u8(ptr + len, 0)?;
	Ok(Some(ptr))
}

pub(super) fn install_shims(state: &mut EmuState) {
	state.install_shim_function("GetIndString", get_ind_string);
	state.install_shim_function("numtostring", numtostring);
	state.install_shim_function("c2pstr", c2pstr);
	state.install_shim_function("p2cstr", p2cstr);
}
