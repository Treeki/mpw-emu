use std::time::SystemTime;

use super::{EmuState, EmuUC, FuncResult, helpers::ArgReader};

fn get_date_time(_uc: &mut EmuUC, _state: &mut EmuState, _reader: &mut ArgReader) -> FuncResult {
	let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap();
	let mac_time = now.as_secs() + 2082844800;
	Ok(Some((mac_time & 0xFFFFFFFF) as u32))
}

pub(super) fn install_shims(state: &mut EmuState) {
	state.install_shim_function("GetDateTime", get_date_time);
}
