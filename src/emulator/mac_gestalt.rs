use crate::common::{FourCC, OSErr, four_cc};

use super::{EmuState, EmuUC, FuncResult, helpers::{ArgReader, UnicornExtras}};

fn build_response(selector: FourCC) -> Option<u32> {
	let response = if selector == four_cc(*b"alis") {
		// alias manager present, without remote appletalk
		1
	} else if selector == four_cc(*b"os  ") {
		// no special OS features
		0
	} else if selector == four_cc(*b"fold") {
		// FindFolder not available, whatever it is
		0
	} else {
		return None;
	};
	Some(response)
}

fn gestalt(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (selector, response_ptr): (FourCC, u32) = reader.read2(uc)?;

	if let Some(response) = build_response(selector) {
		uc.write_u32(response_ptr, response)?;
		Ok(Some(0))
	} else {
		Ok(Some(OSErr::GestaltUndefSelector.to_u32()))
	}
}

pub(super) fn install_shims(state: &mut EmuState) {
	state.install_shim_function("Gestalt", gestalt);
}
