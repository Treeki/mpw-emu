use std::{ffi::CString, io::{Write, Seek}};

use crate::mac_roman;

use super::{EmuState, EmuUC, FuncResult, UcResult, helpers::{ArgReader, UnicornExtras}};

pub enum CFile {
	StdOut,
	StdErr,
	File(std::fs::File)
}

impl CFile {
	fn is_terminal(&self) -> bool {
		match self {
			CFile::StdOut | CFile::StdErr => true,
			_ => false
		}
	}
	
	fn generic_write(&mut self, buffer: &[u8]) -> u32 {
		let write_result = match self {
			CFile::StdOut => std::io::stdout().write(buffer),
			CFile::StdErr => std::io::stderr().write(buffer),
			CFile::File(f) => f.write(buffer)
		};

		match write_result {
			Ok(amount) => amount as u32,
			Err(e) => {
				error!(target: "stdio", "failed to write to file: {e:?}");
				0
			}
		}
	}

	fn tell(&mut self) -> u32 {
		match self {
			CFile::File(f) => {
				match f.stream_position() {
					Ok(pos) => pos as u32,
					Err(e) => {
						error!(target: "stdio", "failed to get position of file");
						0xFFFFFFFF
					}
				}
			},
			_ => {
				warn!(target: "stdio", "running ftell() on stdout or stderr");
				0xFFFFFFFF
			}
		}
	}
}

fn internal_printf(uc: &EmuUC, format: &[u8], arg_reader: &mut ArgReader) -> UcResult<Vec<u8>> {
	let mut output = Vec::new();
	let mut iter = format.iter();

	loop {
		let mut ch = match iter.next() {
			Some(c) => *c, None => break
		};

		if ch != b'%' {
			output.push(ch);
			continue;
		}

		let mut alternate = false;
		let mut zero_pad = false;
		let mut negative = false;
		let mut blank = false;
		let mut plus = false;

		ch = *iter.next().unwrap_or(&0);

		// part 1: flags
		loop {
			match ch {
				b'#' => alternate = true,
				b'0' => zero_pad = true,
				b'-' => negative = true,
				b' ' => blank = true,
				b'+' => plus = true,
				_ => break
			}
			ch = *iter.next().unwrap_or(&0);
		}
		
		// part 2: minimum width
		let mut min_width = None;
		if ch == b'*' {
			min_width = Some(arg_reader.read1(uc)?);
			ch = *iter.next().unwrap_or(&0);
		} else {
			while (b'1'..b'9').contains(&ch) || (ch == b'0' && min_width.is_some()) {
				let digit = (ch - b'0') as u32;
				min_width = Some(min_width.unwrap_or(0) * 10 + digit);

				ch = *iter.next().unwrap_or(&0);
			}
		}

		// part 3: precision
		let mut precision = None;
		if ch == b'.' {
			precision = Some(0);
			ch = *iter.next().unwrap_or(&0);

			if ch == b'*' {
				precision = Some(arg_reader.read1(uc)?);
				ch = *iter.next().unwrap_or(&0);
			} else {
				while (b'0'..b'9').contains(&ch) {
					let digit = (ch - b'0') as u32;
					precision = Some(precision.unwrap() * 10 + digit);
					ch = *iter.next().unwrap_or(&0);
				}
			}
		}

		// part 4: modifiers
		let mut modifier = None;
		if ch == b'h' || ch == b'l' || ch == b'j' || ch == b't' || ch == b'z' {
			modifier = Some(ch);
			ch = *iter.next().unwrap_or(&0);
		}

		// double h or l?
		let mut double_modifier = false;
		if (ch == b'h' || ch == b'l') && modifier == Some(ch) {
			double_modifier = true;
			ch = *iter.next().unwrap_or(&0);
		}

		// finally produce the actual thing
		let what = match ch {
			b's' => {
				let addr: u32 = arg_reader.read1(uc)?;
				let mut inner_string = if addr == 0 {
					b"(null)".to_vec()
				} else {
					uc.read_c_string(addr)?.into_bytes()
				};
				if let Some(prec) = precision {
					inner_string.truncate(prec as usize);
				}
				inner_string
			}
			b'd' => {
				let num: i32 = arg_reader.read1(uc)?;
				format!("{}", num).into_bytes()
			}
			b'X' => {
				let num: u32 = arg_reader.read1(uc)?;
				format!("{:X}", num).into_bytes()
			}
			_ => format!("UNIMPLEMENTED!! {}", ch as char).into_bytes()
		};

		let min_width = min_width.unwrap_or(0) as usize;
		let padding = min_width.max(what.len()) - what.len();
		if !negative {
			for _ in 0..padding {
				output.push(if zero_pad { b'0' } else { b' ' });
			}
		}
		output.extend(what);
		if negative {
			for _ in 0..padding {
				output.push(b' ');
			}
		}
	}

	Ok(output)
}

fn fclose(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let file: u32 = reader.read1(uc)?;

	if state.stdio_files.contains_key(&file) {
		state.stdio_files.remove(&file);
		Ok(Some(0))
	} else {
		warn!(target: "stdio", "fclose() on invalid file {file:08X}");
		// TODO: this should be EOF, check what it is in MSL
		Ok(Some(0xFFFFFFFF))
	}
}

fn fprintf(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (file, format): (u32, CString) = reader.read2(uc)?;
	trace!(target: "stdio", "fprintf({file:08X}, {format:?}, ...)");
	let output = internal_printf(uc, format.as_bytes(), reader)?;

	match state.stdio_files.get_mut(&file) {
		Some(f) => {
			if f.is_terminal() {
				Ok(Some(f.generic_write(&mac_roman::decode_buffer(&output, true))))
			} else {
				Ok(Some(f.generic_write(&output)))
			}
		}
		None => {
			warn!(target: "stdio", "fprintf() is writing to invalid file {file:08X}");
			// set errno later?
			Ok(Some(0))
		}
	}
}

fn printf(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (file, format): (u32, CString) = reader.read2(uc)?;
	trace!(target: "stdio", "printf({file:08X}, {format:?}, ...)");
	let output = internal_printf(uc, format.as_bytes(), reader)?;
	Ok(Some(CFile::StdOut.generic_write(&mac_roman::decode_buffer(&output, true))))
}

fn sprintf(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (buffer, format): (u32, CString) = reader.read2(uc)?;
	trace!(target: "stdio", "sprintf({buffer:08X}, {format:?}, ...)");
	let output = internal_printf(uc, format.as_bytes(), reader)?;

	uc.write_c_string(buffer, &output)?;
	Ok(Some(output.len() as u32))
}

fn vfprintf(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (file, format, va_list): (u32, CString, u32) = reader.read3(uc)?;
	trace!(target: "stdio", "vfprintf({file:08X}, {format:?}, va_list={va_list:08X})");
	let mut va_reader = ArgReader::new_with_va_list(va_list);
	let output = internal_printf(uc, format.as_bytes(), &mut va_reader)?;

	match state.stdio_files.get_mut(&file) {
		Some(f) => {
			if f.is_terminal() {
				Ok(Some(f.generic_write(&mac_roman::decode_buffer(&output, true))))
			} else {
				Ok(Some(f.generic_write(&output)))
			}
		}
		None => {
			warn!(target: "stdio", "vfprintf() is writing to invalid file {file:08X}");
			// set errno later?
			Ok(Some(0))
		}
	}
}

fn fputs(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (s, file): (CString, u32) = reader.read2(uc)?;
	let output = s.into_bytes();

	match state.stdio_files.get_mut(&file) {
		Some(f) => {
			if f.is_terminal() {
				Ok(Some(f.generic_write(&mac_roman::decode_buffer(&output, true))))
			} else {
				Ok(Some(f.generic_write(&output)))
			}
		}
		None => {
			warn!(target: "stdio", "fputs() is writing to invalid file {file:08X}");
			// set errno later?
			Ok(Some(0))
		}
	}
}

fn fwrite(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (ptr, size, count, file): (u32, u32, u32, u32) = reader.read4(uc)?;
	let output = uc.mem_read_as_vec(ptr.into(), (size * count) as usize)?;

	match state.stdio_files.get_mut(&file) {
		Some(f) => {
			if f.is_terminal() {
				Ok(Some(f.generic_write(&mac_roman::decode_buffer(&output, true)) / size))
			} else {
				Ok(Some(f.generic_write(&output) / size))
			}
		}
		None => {
			warn!(target: "stdio", "fwrite() is writing to invalid file {file:08X}");
			// set errno later?
			Ok(Some(0))
		}
	}
}

fn ftell(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let file: u32 = reader.read1(uc)?;

	match state.stdio_files.get_mut(&file) {
		Some(f) => {
			Ok(Some(f.tell()))
		}
		None => {
			warn!(target: "stdio", "ftell() is telling from invalid file {file:08X}");
			// set errno later?
			Ok(Some(0xFFFFFFFF))
		}
	}
}

fn putchar(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let ch: u8 = reader.read1(uc)?;
	print!("{}", mac_roman::decode_char(ch, true));
	Ok(Some(ch.into()))
}

pub(super) fn install_shims(state: &mut EmuState) {
	if let Some(iob) = state.get_shim_addr("_iob") {
		state.stdio_files.insert(iob + 0x18, CFile::StdOut);
		state.stdio_files.insert(iob + 0x30, CFile::StdErr);
	}

	// remove
	// rename
	// tmpnam
	// tmpfile
	// setbuf
	// setvbuf
	state.install_shim_function("fclose", fclose);
	// fflush
	// fopen
	// freopen
	state.install_shim_function("fprintf", fprintf);
	// fscanf
	state.install_shim_function("printf", printf);
	// scanf
	state.install_shim_function("sprintf", sprintf);
	// sscanf
	state.install_shim_function("vfprintf", vfprintf);
	// vprintf
	// vsprintf
	// fgetc
	// fgets
	// fputc
	state.install_shim_function("fputs", fputs);
	// gets
	// puts
	// ungetc
	// fread
	state.install_shim_function("fwrite", fwrite);
	// fgetpos
	state.install_shim_function("ftell", ftell);
	// fsetpos
	// fseek
	// rewind
	// clearerr
	// perror
	// getc
	// putc
	// getchar
	state.install_shim_function("putchar", putchar);
	// feof
	// ferror
}
