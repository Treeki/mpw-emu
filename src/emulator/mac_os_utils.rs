use chrono::prelude::*;

use crate::common::get_mac_time;

use super::{EmuState, EmuUC, FuncResult, helpers::ArgReader};

fn get_date_time(_uc: &mut EmuUC, _state: &mut EmuState, _reader: &mut ArgReader) -> FuncResult {
	Ok(Some(get_mac_time(Local::now())))
}

pub(super) fn install_shims(state: &mut EmuState) {
	state.install_shim_function("GetDateTime", get_date_time);
}
