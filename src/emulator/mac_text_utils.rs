use crate::common::{four_cc, parse_mac_time};

use super::{EmuState, EmuUC, FuncResult, helpers::{ArgReader, UnicornExtras}};

fn generic_get_ind_string(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader, pascal: bool) -> FuncResult {
	let (ptr, table_id, mut str_id): (u32, i16, i16) = reader.read3(uc)?;

	if let Some(res) = state.resources.get(four_cc(*b"STR#"), table_id) {
		let mut offset = 2;
		while offset < res.data.len() && str_id > 1 {
			str_id -= 1;
			offset += res.data[offset] as usize;
			offset += 1;
		}

		let len = res.data[offset] as usize;
		let s = &res.data[offset + 1 .. offset + len + 1];
		if pascal {
			uc.write_pascal_string(ptr, s)?;
		} else {
			uc.write_c_string(ptr, s)?;
		}
	} else {
		warn!(target: "text_utils", "GetIndString failed to find STR# table {table_id}");
	}

	Ok(None)
}

fn pascal_get_ind_string(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	generic_get_ind_string(uc, state, reader, true)
}

fn c_get_ind_string(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	generic_get_ind_string(uc, state, reader, false)
}

fn numtostring(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (num, ptr): (i32, u32) = reader.read2(uc)?;
	let s = format!("{}", num);
	uc.write_c_string(ptr, s.as_bytes())?;
	Ok(None)
}

fn iudatestring(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (date, long_flag, ptr): (u32, u32, u32) = reader.read3(uc)?;
	let date = parse_mac_time(date);
	let s = match long_flag {
		2 => {
			// longDate: Friday, January 31, 1992
			date.format("%A, %B %d, %Y")
		}
		1 => {
			// abbrevDate: Fri, Jan 31, 1992
			date.format("%a, %m %d, %Y")
		}
		_ => {
			// shortDate: 1/31/92
			date.format("%m/%d/%y")
		}
	}.to_string();
	uc.write_c_string(ptr, s.as_bytes())?;
	Ok(None)
}

fn iutimestring(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (date, want_seconds, ptr): (u32, bool, u32) = reader.read3(uc)?;
	let date = parse_mac_time(date);
	let s = if want_seconds {
		date.format("%H:%M:%S")
	} else {
		date.format("%H:%M")
	}.to_string();
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
	state.install_shim_function("GetIndString", pascal_get_ind_string);
	state.install_shim_function("getindstring", c_get_ind_string);
	state.install_shim_function("numtostring", numtostring);
	state.install_shim_function("iudatestring", iudatestring);
	state.install_shim_function("iutimestring", iutimestring);
	state.install_shim_function("c2pstr", c2pstr);
	state.install_shim_function("p2cstr", p2cstr);
}
