use std::ffi::CString;

use unicorn_engine::RegisterPPC;

use super::{EmuState, EmuUC, FuncResult, UcResult, helpers::{ArgReader, UnicornExtras}};

const QSORT_CODE: &[u32] = &[
	// Offset 0
	0x7C0802A6, // mflr     r0
	0x90010008, // stw      r0,8(SP)
	0x9421FFC0, // stwu     SP,-64(SP)
	0x7C882378, // mr       r8,r4
	0x7CA42B78, // mr       r4,r5
	0x7CC73378, // mr       r7,r6
	0x38C8FFFF, // subi     r6,r8,1
	0x38A00000, // li       r5,0
	0x48000015, // bl       ._qsort (+0x14)
	0x80010048, // lwz      r0,72(SP)
	0x38210040, // addi     SP,SP,64
	0x7C0803A6, // mtlr     r0
	0x4E800020, // blr
	// Offset 0x34
	0x7C0802A6, // mflr     r0
	0xBEE1FFDC, // stmw     r23,-36(SP)
	0x90010008, // stw      r0,8(SP)
	0x9421FFA0, // stwu     SP,-96(SP)
	0x7C7B1B78, // mr       r27,r3
	0x7C9C2378, // mr       r28,r4
	0x7CBD2B78, // mr       r29,r5
	0x7CDE3378, // mr       r30,r6
	0x7CFF3B78, // mr       r31,r7
	0x2C1D0000, // cmpwi    r29,0
	0x3B1DFFFF, // subi     r24,r29,1
	0x418000E8, // blt      *+232
	0x7C1DF000, // cmpw     r29,r30
	0x408000E0, // bge      *+224
	0x7FB7EB78, // mr       r23,r29
	0x7F5EE1D6, // mullw    r26,r30,r28
	0x48000060, // b        *+96
	0x7F37E1D6, // mullw    r25,r23,r28
	0x7FECFB78, // mr       r12,r31
	0x7C7BCA14, // add      r3,r27,r25
	0x7C9BD214, // add      r4,r27,r26
	0x480000D5, // bl       .__ptr_glue (+0xD4)
	0x80410014, // lwz      r2,20(SP)
	0x2C030000, // cmpwi    r3,0
	0x4181003C, // bgt      *+60
	0x3B180001, // addi     r24,r24,1
	0x7C18E1D6, // mullw    r0,r24,r28
	0x7CDB0214, // add      r6,r27,r0
	0x7CBBCA14, // add      r5,r27,r25
	0x38600000, // li       r3,0
	0x7F8903A6, // mtctr    r28
	0x281C0000, // cmplwi   r28,$0000
	0x4081001C, // ble      *+28
	0x7C8618AE, // lbzx     r4,r6,r3
	0x7C0518AE, // lbzx     r0,r5,r3
	0x7C0619AE, // stbx     r0,r6,r3
	0x7C8519AE, // stbx     r4,r5,r3
	0x38630001, // addi     r3,r3,1
	0x4200FFEC, // bdnz     *-20
	0x3AF70001, // addi     r23,r23,1
	0x7C17F040, // cmplw    r23,r30
	0x4180FFA0, // blt      *-96
	0x3B180001, // addi     r24,r24,1
	0x7C78E1D6, // mullw    r3,r24,r28
	0x7C1EE1D6, // mullw    r0,r30,r28
	0x7CDB1A14, // add      r6,r27,r3
	0x7CBB0214, // add      r5,r27,r0
	0x38600000, // li       r3,0
	0x7F8903A6, // mtctr    r28
	0x281C0000, // cmplwi   r28,$0000
	0x4081001C, // ble      *+28
	0x7C8618AE, // lbzx     r4,r6,r3
	0x7C0518AE, // lbzx     r0,r5,r3
	0x7C0619AE, // stbx     r0,r6,r3
	0x7C8519AE, // stbx     r4,r5,r3
	0x38630001, // addi     r3,r3,1
	0x4200FFEC, // bdnz     *-20
	0x7F63DB78, // mr       r3,r27
	0x7F84E378, // mr       r4,r28
	0x7FA5EB78, // mr       r5,r29
	0x7FE7FB78, // mr       r7,r31
	0x38D8FFFF, // subi     r6,r24,1
	0x4BFFFF09, // bl       ._qsort (-0xF8)
	0x7F63DB78, // mr       r3,r27
	0x7F84E378, // mr       r4,r28
	0x7FC6F378, // mr       r6,r30
	0x7FE7FB78, // mr       r7,r31
	0x38B80001, // addi     r5,r24,1
	0x4BFFFEF1, // bl       ._qsort (-0x110)
	0x80010068, // lwz      r0,104(SP)
	0x38210060, // addi     SP,SP,96
	0x7C0803A6, // mtlr     r0
	0xBAE1FFDC, // lmw      r23,-36(SP)
	0x4E800020, // blr
	// Offset 0x15C
	0x800C0000, // lwz      r0,0(r12)
	0x90410014, // stw      r2,20(SP)
	0x7C0903A6, // mtctr    r0
	0x804C0004, // lwz      r2,0(r12)
	0x4E800420, // bctr
];

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

fn calloc(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (count, size): (u32, u32) = reader.read2(uc)?;
	let ptr = state.heap.new_ptr(uc, count * size)?;
	Ok(Some(ptr))
}

fn free(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let ptr: u32 = reader.read1(uc)?;
	if ptr != 0 {
		state.heap.dispose_ptr(uc, ptr)?;
	}
	Ok(None)
}

fn malloc(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let size: u32 = reader.read1(uc)?;
	let ptr = state.heap.new_ptr(uc, size)?;
	Ok(Some(ptr))
}

fn realloc(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (ptr, new_size): (u32, u32) = reader.read2(uc)?;
	if ptr != 0 {
		if state.heap.set_ptr_size(uc, ptr, new_size)? {
			// resized successfully
			Ok(Some(ptr))
		} else {
			// no dice, we need to allocate a new buffer
			let new_ptr = state.heap.new_ptr(uc, new_size)?;
			if new_ptr != 0 {
				let old_size = state.heap.get_ptr_size(uc, ptr)?;
				let to_copy = old_size.min(new_size);
				for i in 0..to_copy {
					uc.write_u8(new_ptr + i, uc.read_u8(ptr + i)?)?;
				}
				state.heap.dispose_ptr(uc, ptr)?;
				Ok(Some(new_ptr))
			} else {
				// failed
				Ok(Some(0))
			}
		}
	} else {
		let ptr = state.heap.new_ptr(uc, new_size)?;
		Ok(Some(ptr))
	}
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

fn abs(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let value: u32 = reader.read1(uc)?;
	Ok(Some((value as i32).unsigned_abs()))
}

fn signal(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (sig, func): (i32, u32) = reader.read2(uc)?;

	trace!(target: "stdlib", "signal({sig}, {func:08X})");

	// MSL defines SIG_DFL as 0, SIG_IGN as 1 and SIG_ERR as -1

	Ok(Some(0))
}

fn setjmp(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let env: u32 = reader.read1(uc)?;

	trace!(target: "stdlib", "setjmp(env={env:08X})");

	// hateful, absolutely hateful
	uc.write_u32(env, uc.reg_read(RegisterPPC::LR)? as u32)?;
	uc.write_u32(env + 4, uc.reg_read(RegisterPPC::CR)? as u32)?;
	uc.write_u32(env + 8, uc.reg_read(RegisterPPC::R1)? as u32)?;
	uc.write_u32(env + 12, uc.reg_read(RegisterPPC::R2)? as u32)?;

	for i in 0..19 {
		uc.write_u32(env + 20 + (i as u32) * 4, uc.reg_read(RegisterPPC::R13 as i32 + i)? as u32)?;
	}
	for i in 0..18 {
		uc.write_u64(env + 96 + (i as u32) * 8, uc.reg_read(RegisterPPC::FPR14 as i32 + i)?)?;
	}

	uc.write_u64(env + 240, uc.reg_read(RegisterPPC::FPSCR)?)?;

	Ok(Some(0))
}

fn longjmp(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (env, val): (u32, i32) = reader.read2(uc)?;

	trace!(target: "stdlib", "longjmp(env={env:08X}, val={val})");

	uc.reg_write(RegisterPPC::LR, uc.read_u32(env)? as u64)?; // LR
	uc.reg_write(RegisterPPC::CR, uc.read_u32(env + 4)? as u64)?; // CR
	uc.reg_write(RegisterPPC::R1, uc.read_u32(env + 8)? as u64)?;
	uc.reg_write(RegisterPPC::R2, uc.read_u32(env + 12)? as u64)?;

	for i in 0..19 {
		uc.reg_write(RegisterPPC::R13 as i32 + i, uc.read_u32(env + 20 + (i as u32) * 4)? as u64)?;
	}
	for i in 0i32..18 {
		uc.reg_write(RegisterPPC::FPR14 as i32 + i, uc.read_u64(env + 96 + (i as u32) * 8)?)?;
	}

	uc.reg_write(RegisterPPC::FPSCR, uc.read_u64(env + 240)?)?;

	Ok(Some(if val == 0 { 1 } else { val as u32 }))
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

pub(super) fn install_shims(uc: &mut EmuUC, state: &mut EmuState) -> UcResult<()> {
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
	state.install_shim_function("calloc", calloc);
	state.install_shim_function("free", free);
	state.install_shim_function("malloc", malloc);
	state.install_shim_function("realloc", realloc);
	// abort
	state.install_shim_function("atexit", atexit);
	state.install_shim_function("exit", exit);
	state.install_shim_function("getenv", getenv);
	// system
	// bsearch
	state.install_shim_function("abs", abs);
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
	state.install_shim_function("longjmp", longjmp);

	// qsort() needs special handling.
	// We implement it in PowerPC so it can invoke a callback function
	if let Some(qsort) = state.get_shim_addr("qsort") {
		let qsort_code = state.heap.new_ptr(uc, (QSORT_CODE.len() * 4) as u32)?;
		uc.write_u32(qsort, qsort_code)?;
		for (i, insn) in QSORT_CODE.iter().enumerate() {
			uc.write_u32(qsort_code + 4 * (i as u32), *insn)?;
		}
	}

	Ok(())
}
