use crate::mac_roman;

use super::{EmuState, EmuUC, FuncResult, UcResult, helpers::{ArgReader, UnicornExtras}};

fn tolower(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let ch: u8 = reader.read1(uc)?;
	Ok(Some(mac_roman::to_lower(ch).into()))
}

fn toupper(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let ch: u8 = reader.read1(uc)?;
	Ok(Some(mac_roman::to_upper(ch).into()))
}

pub(super) fn install_shims(state: &mut EmuState) {
	state.install_shim_function("tolower", tolower);
	state.install_shim_function("toupper", toupper);
}
