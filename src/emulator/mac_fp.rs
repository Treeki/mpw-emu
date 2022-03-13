use unicorn_engine::RegisterPPC;

use super::{EmuState, EmuUC, FuncResult, helpers::{ArgReader, UnicornExtras}};

fn dec2num(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let ptr: u32 = reader.read1(uc)?;
	let sgn = uc.read_i8(ptr)?;
	let exp = uc.read_i16(ptr + 2)?;
	let text = uc.read_pascal_string(ptr + 4)?;
	trace!(target: "fp", "dec2num(sgn={sgn}, exp={exp}, text={text:?})");

	// let's rely on Rust to do it
	let sgn = if sgn == 1 { "-" } else { "" };
	let text = text.to_str().unwrap();
	let num_str = format!("{sgn}{text}E{exp}");
	match num_str.parse::<f64>() {
		Ok(n) => {
			// we should set overflow/underflow flags in the PPC FPSCR here
			// but not sure how to do that atm
			uc.reg_write(RegisterPPC::FPR1, n.to_bits())?;
		}
		Err(e) => {
			error!(target: "fp", "dec2num failed to parse: {num_str} - {e:?}");
		}
	}
	Ok(Some(0))
}

pub(super) fn install_shims(state: &mut EmuState) {
	state.install_shim_function("dec2num", dec2num);
}
