use unicorn_engine::RegisterPPC;

use super::{EmuState, EmuUC, FuncResult, helpers::ArgReader};

fn feclearexcept(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let excepts: u32 = reader.read1(uc)?;
	let mut reg = uc.reg_read(RegisterPPC::FPSCR)?;
	reg &= !(excepts as u64);
	uc.reg_write(RegisterPPC::FPSCR, reg)?;
	Ok(Some(0))
}

fn fetestexcept(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let excepts: u32 = reader.read1(uc)?;
	let mut reg = uc.reg_read(RegisterPPC::FPSCR)?;
	reg &= excepts as u64;
	Ok(Some(reg as u32))
}

pub(super) fn install_shims(state: &mut EmuState) {
	state.install_shim_function("feclearexcept", feclearexcept);
	state.install_shim_function("fetestexcept", fetestexcept);
}
