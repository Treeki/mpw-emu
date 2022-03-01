use std::ffi::CString;

use super::{EmuState, EmuUC, FuncResult, UcResult, helpers::{ArgReader, UnicornExtras}};

fn atoi(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let s: CString = reader.read1(uc)?;
	if let Ok(s) = s.into_string() {
		if let Ok(num) = s.parse::<i32>() {
			return Ok(Some(num as u32));
		}
	}
	Ok(Some(0))
}

fn is_digit_for_base(ch: u8, base: u32) -> bool {
	if base > 10 {
		let max_upper = b'A' + (base as u8) - 11;
		let max_lower = b'a' + (base as u8) - 11;
		if ch >= b'0' && ch <= b'9' { return true; }
		if ch >= b'A' && ch <= max_upper { return true; }
		if ch >= b'a' && ch <= max_lower { return true; }
	} else {
		let max_digit = b'0' + (base as u8) - 1;
		if ch >= b'0' && ch <= max_digit { return true; }
	}

	false
}

fn digit_to_num(ch: u8) -> u8 {
	match ch {
		b'0' ..= b'9' => (ch - b'0'),
		b'A' ..= b'Z' => (ch - b'A' + 11),
		b'a' ..= b'z' => (ch - b'a' + 11),
		_ => 0
	}
}

fn strtol(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (str_ptr, end_ptr, mut base): (u32, u32, u32) = reader.read3(uc)?;
	let s = uc.read_c_string(str_ptr)?;
	let s = s.as_bytes();
	let mut i = 0;
	let mut num = 0i64;
	let mut negative = false;

	while i < s.len() && s[i].is_ascii_whitespace() {
		i += 1;
	}

	if i < s.len() {
		if s[i] == b'-' {
			negative = true;
			i += 1;
		} else if s[i] == b'+' {
			i += 1;
		}
	}

	if (base == 0 || base == 16) && (s[i..].starts_with(b"0x") || s[i..].starts_with(b"0X")) {
		base = 16;
		i += 2;
	} else if (base == 0) && (s[i..].starts_with(b"0")) {
		base = 8;
	}

	while i < s.len() && is_digit_for_base(s[i], base) {
		num = num.wrapping_mul(base.into()).wrapping_add(digit_to_num(s[i]).into());
	}
	
	if negative {
		num = -num;
	}

	if end_ptr != 0 {
		uc.write_u32(end_ptr, str_ptr + i as u32)?;
	}

	Ok(Some(num as u32))
}

fn atexit(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let _func: u32 = reader.read1(uc)?;
	// Not implemented, but we pretend it is to satisfy the program
	Ok(Some(0))
}

fn exit(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let status: i32 = reader.read1(uc)?;

	info!(target: "stdlib", "exit({status})");
	state.exit_status = Some(status);
	uc.emu_stop()?;

	Ok(None)
}

fn getenv(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let name: CString = reader.read1(uc)?;
	if let Ok(name) = name.into_string() {
		if let Some(ptr) = state.env_var_map.get(&name) {
			return Ok(Some(*ptr));
		}
	}

	Ok(Some(0))
}

fn signal(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (sig, func): (i32, u32) = reader.read2(uc)?;

	info!(target: "stdlib", "signal({sig}, {func:08X})");

	// MSL defines SIG_DFL as 0, SIG_IGN as 1 and SIG_ERR as -1

	Ok(Some(0))
}

fn setjmp(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let env: u32 = reader.read1(uc)?;

	info!(target: "stdlib", "setjmp(env={env:08X})");

	Ok(Some(0))
}

pub(super) fn setup_environment(uc: &mut EmuUC, state: &mut EmuState, args: &[String], env_vars: &[(String, String)]) -> UcResult<()> {
	// Environment Variables
	for (name, value) in env_vars {
		let ptr = state.heap.new_ptr(uc, value.as_bytes().len() as u32 + 1)?;
		uc.write_c_string(ptr, value.as_bytes())?;

		state.env_var_map.insert(name.clone(), ptr);
	}

	// Arguments
	if let Some(int_env) = state.get_shim_addr("_IntEnv") {
		debug!(target: "stdlib", "_IntEnv ptr is at: {int_env:08X}");

		let argv = state.heap.new_ptr(uc, (args.len() * 4) as u32)?;
		uc.write_u32(int_env + 2, args.len() as u32)?;
		uc.write_u32(int_env + 6, argv)?;

		// _IntEnv also contains EnvP but I'm not sure what the format is for that yet

		for (i, arg) in args.iter().enumerate() {
			let arg_ptr = state.heap.new_ptr(uc, arg.as_bytes().len() as u32 + 1)?;

			uc.write_u32(argv + (i as u32) * 4, arg_ptr)?;
			uc.write_c_string(arg_ptr, arg.as_bytes())?;
		}
	}

	Ok(())
}

pub(super) fn install_shims(state: &mut EmuState) {
	// atof
	state.install_shim_function("atoi", atoi);
	state.install_shim_function("atol", atoi);
	// strtod
	state.install_shim_function("strtol", strtol);
	// strtoul
    // strtoll (?)
	// strtoull (?)
	// rand
	// srand
    // calloc
	// free
	// malloc
	// realloc
	// abort
	state.install_shim_function("atexit", atexit);
	state.install_shim_function("exit", exit);
	state.install_shim_function("getenv", getenv);
	// system
	// bsearch
	// qsort
	// abs
	// labs
	// div
	// ldiv
	// mblen
	// mbtowc
	// wctomb
	// mbstowcs
	// wcstombs

	// This isn't actually in stdlib.h, but I didn't feel like creating
	// c_signal.rs just for one stub. Fight me.
	state.install_shim_function("signal", signal);

	// ... Same for setjmp.h.
	state.install_shim_function("__setjmp", setjmp);
}
