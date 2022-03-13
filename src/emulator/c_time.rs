use chrono::Utc;

use super::{EmuState, EmuUC, FuncResult, helpers::{ArgReader, UnicornExtras}};

fn time(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let time_ptr: u32 = reader.read1(uc)?;
	let now = Utc::now().timestamp() as u32;
	if time_ptr != 0 {
		uc.write_u32(time_ptr, now)?;
	}
	Ok(Some(now))
}

pub(super) fn install_shims(state: &mut EmuState) {
	state.install_shim_function("time", time);
}
