use chrono::prelude::*;

use crate::common::get_mac_time;

use super::{EmuState, EmuUC, FuncResult, helpers::ArgReader};

fn get_date_time(_uc: &mut EmuUC, _state: &mut EmuState, _reader: &mut ArgReader) -> FuncResult {
	Ok(Some(get_mac_time(Local::now())))
}

fn tick_count(_uc: &mut EmuUC, state: &mut EmuState, _reader: &mut ArgReader) -> FuncResult {
	let duration = (state.start_time.elapsed().as_millis() * 60) / 1000;
	Ok(Some(duration as u32))
}

fn trap_available(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let trap: u16 = reader.read1(uc)?;
	if trap == 0xA1AD {
		// Gestalt is available
		Ok(Some(1))
	} else {
		// nothing else is available, according to us
		warn!(target: "os_utils", "Unknown trap for TrapAvailable: 0x{trap:04X} (reporting unavailable)");
		Ok(Some(0))
	}
}

pub(super) fn install_shims(state: &mut EmuState) {
	state.install_shim_function("GetDateTime", get_date_time);

	// this is actually in Events.h but shhh
	state.install_shim_function("TickCount", tick_count);

	// not sure where this lies...?
	state.install_shim_function("TrapAvailable", trap_available);
}
