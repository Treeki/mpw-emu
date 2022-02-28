use std::time::SystemTime;

use super::{EmuState, EmuUC, FuncResult, helpers::ArgReader};

fn lm_get_ticks(_uc: &mut EmuUC, state: &mut EmuState, _reader: &mut ArgReader) -> FuncResult {
	let duration = (state.start_time.elapsed().as_millis() * 60) / 1000;
	Ok(Some(duration as u32))
}

fn lm_get_time(_uc: &mut EmuUC, _state: &mut EmuState, _reader: &mut ArgReader) -> FuncResult {
	// Assuming that this is the same as GetDateTime... hopefully?
	let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap();
	let mac_time = now.as_secs() + 2082844800;
	Ok(Some((mac_time & 0xFFFFFFFF) as u32))
}

fn lm_get_boot_drive(_uc: &mut EmuUC, _state: &mut EmuState, _reader: &mut ArgReader) -> FuncResult {
	Ok(Some(0))
}

fn lm_get_mem_err(_uc: &mut EmuUC, _state: &mut EmuState, _reader: &mut ArgReader) -> FuncResult {
	Ok(Some(0))
}

pub(super) fn install_shims(state: &mut EmuState) {
	state.install_shim_function("LMGetTicks", lm_get_ticks);
	state.install_shim_function("LMGetTime", lm_get_time);
	state.install_shim_function("LMGetBootDrive", lm_get_boot_drive);
	state.install_shim_function("LMGetMemErr", lm_get_mem_err);
}

