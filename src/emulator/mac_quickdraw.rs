use super::{EmuState, EmuUC, FuncResult, helpers::ArgReader};

fn get_cursor(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let id: i16 = reader.read1(uc)?;
	info!(target: "quickdraw", "GetCursor({id}) - unimplemented");
	Ok(Some(0))
}

fn init_graf(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let global_ptr: u32 = reader.read1(uc)?;
	info!(target: "quickdraw", "InitGraf(globalPtr = {global_ptr:08X}) - unimplemented");
	Ok(None)
}

fn set_cursor(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let crsr: u32 = reader.read1(uc)?;
	info!(target: "quickdraw", "SetCursor(crsr = {crsr:08X}) - unimplemented");
	Ok(None)
}

pub(super) fn install_shims(state: &mut EmuState) {
	state.install_shim_function("GetCursor", get_cursor);
	state.install_shim_function("InitGraf", init_graf);
	state.install_shim_function("SetCursor", set_cursor);
}
