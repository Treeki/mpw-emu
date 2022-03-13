use std::ffi::CString;

use super::{EmuState, EmuUC, FuncResult, helpers::{ArgReader, UnicornExtras}};

fn memset(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (ptr, byte, len): (u32, u8, u32) = reader.read3(uc)?;
	for i in 0..len {
		uc.write_u8(ptr + i, byte)?;
	}
	Ok(Some(ptr))
}

fn memcmp(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (s1, s2, len): (u32, u32, u32) = reader.read3(uc)?;
	for i in 0..len {
		let b1 = uc.read_u8(s1 + i)? as i32;
		let b2 = uc.read_u8(s2 + i)? as i32;
		if b1 != b2 {
			return Ok(Some((b1 - b2) as u32));
		}
	}
	Ok(Some(0))
}

fn memcpy(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (dst, src, len): (u32, u32, u32) = reader.read3(uc)?;
	for i in 0..len {
		uc.write_u8(dst + i, uc.read_u8(src + i)?)?;
	}
	Ok(Some(dst))
}

fn memmove(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (dst, src, len): (u32, u32, u32) = reader.read3(uc)?;
	if (src + len) <= dst || src >= (dst + len) {
		for i in 0..len {
			uc.write_u8(dst + i, uc.read_u8(src + i)?)?;
		}
	} else {
		// backwards
		for i in 0..len {
			let offset = len - 1 - i;
			uc.write_u8(dst + offset, uc.read_u8(src + offset)?)?;
		}
	}
	Ok(Some(dst))
}

fn strlen(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let addr: u32 = reader.read1(uc)?;
	let mut len = 0;
	while uc.read_u8(addr + len)? != 0 {
		len += 1;
	}
	Ok(Some(len))
}

fn strcpy(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (dst, src): (u32, u32) = reader.read2(uc)?;
	for i in 0..u32::MAX {
		let byte = uc.read_u8(src + i)?;
		uc.write_u8(dst + i, byte)?;
		if byte == 0 { break; }
	}
	Ok(Some(dst))
}

fn strncpy(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (dst, src, len): (u32, u32, u32) = reader.read3(uc)?;
	let mut reached_null = false;
	for i in 0..len {
		let byte = if reached_null { 0 } else { uc.read_u8(src + i)? };
		uc.write_u8(dst + i, byte)?;
		reached_null = byte == 0;
	}
	Ok(Some(dst))
}

fn strcat(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (dst, src): (u32, u32) = reader.read2(uc)?;
	// find the length of dest
	let mut len = 0;
	while uc.read_u8(dst + len)? != 0 {
		len += 1;
	}
	// now copy src over
	for i in 0..u32::MAX {
		let byte = uc.read_u8(src + i)?;
		uc.write_u8(dst + len + i, byte)?;
		if byte == 0 { break; }
	}
	Ok(Some(dst))
}

fn strcmp(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (s1, s2): (u32, u32) = reader.read2(uc)?;
	for i in 0..u32::MAX {
		let b1 = uc.read_u8(s1 + i)?;
		let b2 = uc.read_u8(s2 + i)?;
		if b1 > b2 {
			return Ok(Some(1));
		} else if b1 < b2 {
			return Ok(Some(0xFFFFFFFF));
		} else if b1 == 0 {
			return Ok(Some(0));
		}
	}
	unreachable!()
}

fn strncmp(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (s1, s2, n): (u32, u32, u32) = reader.read3(uc)?;
	for i in 0..n {
		let b1 = uc.read_u8(s1 + i)?;
		let b2 = uc.read_u8(s2 + i)?;
		if b1 > b2 {
			return Ok(Some(1));
		} else if b1 < b2 {
			return Ok(Some(0xFFFFFFFF));
		} else if b1 == 0 {
			break;
		}
	}
	Ok(Some(0))
}

fn strchr(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (addr, needle): (u32, u8) = reader.read2(uc)?;
	for i in 0..u32::MAX {
		let ch = uc.read_u8(addr + i)?;
		if ch == needle {
			return Ok(Some(addr + i));
		} else if ch == 0 {
			return Ok(Some(0));
		}
	}
	unreachable!()
}

fn strrchr(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (addr, needle): (u32, u8) = reader.read2(uc)?;
	let mut best = 0;
	for i in 0..u32::MAX {
		let ch = uc.read_u8(addr + i)?;
		if ch == needle {
			best = addr + i;
		} else if ch == 0 {
			break;
		}
	}
	Ok(Some(best))
}

fn strspn(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (ptr, sep): (u32, CString) = reader.read2(uc)?;
	if ptr == 0 { return Ok(Some(0)); }

	for i in 0..u32::MAX {
		let ch = uc.read_u8(ptr + i)?;
		if ch == 0 || !sep.as_bytes().contains(&ch) {
			return Ok(Some(i));
		}
	}

	unreachable!()
}

fn strtok(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (mut ptr, sep): (u32, CString) = reader.read2(uc)?;
	if ptr == 0 {
		ptr = state.strtok_state;
		if ptr == 0 {
			// no more tokens remain
			return Ok(Some(0));
		}
	}

	// find first separator
	for i in 0..u32::MAX {
		let ch = uc.read_u8(ptr + i)?;
		if sep.as_bytes().contains(&ch) {
			// delimiter found
			state.strtok_state = ptr + i + 1;
			uc.write_u8(ptr + i, 0)?;
			return Ok(Some(ptr));
		} else if ch == 0 {
			// we reached the end
			state.strtok_state = 0;
			if i == 0 {
				// token was empty
				return Ok(Some(0));
			} else {
				return Ok(Some(ptr));
			}
		}
	}

	unreachable!()
}

pub(super) fn install_shims(state: &mut EmuState) {
	state.install_shim_function("memset", memset);
	// memchr
	state.install_shim_function("memcmp", memcmp);
	state.install_shim_function("memcpy", memcpy);
	state.install_shim_function("memmove", memmove);

	state.install_shim_function("strlen", strlen);
	state.install_shim_function("strcpy", strcpy);
	state.install_shim_function("strncpy", strncpy);
	state.install_shim_function("strcat", strcat);
	// strncat
	state.install_shim_function("strcmp", strcmp);
	state.install_shim_function("strncmp", strncmp);
	// strcoll
	// strxfrm
	state.install_shim_function("strchr", strchr);
	state.install_shim_function("strrchr", strrchr);
	// strpbrk
	state.install_shim_function("strspn", strspn);
	// strcspn
	state.install_shim_function("strtok", strtok);
	// strstr
	// strerror
	// strcasecmp
	// strncasecmp
	// strdup
}
