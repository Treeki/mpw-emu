use super::{EmuState, EmuUC, FuncResult, helpers::{ArgReader, UnicornExtras}};

fn memset(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (ptr, byte, len): (u32, u8, u32) = reader.read3(uc)?;
	for i in 0..len {
		uc.write_u8(ptr + i, byte)?;
	}
	Ok(Some(ptr))
}

fn memcpy(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (dst, src, len): (u32, u32, u32) = reader.read3(uc)?;
	for i in 0..len {
		uc.write_u8(dst + i, uc.read_u8(src + i)?)?;
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

pub(super) fn install_shims(state: &mut EmuState) {
	state.install_shim_function("memset", memset);
	// memchr
	// memcmp
	state.install_shim_function("memcpy", memcpy);
	// memmove

	state.install_shim_function("strlen", strlen);
	state.install_shim_function("strcpy", strcpy);
	state.install_shim_function("strncpy", strncpy);
	// strcat
	// strncat
	state.install_shim_function("strcmp", strcmp);
	// strncmp
	// strcoll
	// strxfrm
	state.install_shim_function("strchr", strchr);
	state.install_shim_function("strrchr", strrchr);
	// strpbrk
	// strspn
	// strcspn
	// strtok
	// strstr
	// strerror
	// strcasecmp
	// strncasecmp
	// strdup
}
