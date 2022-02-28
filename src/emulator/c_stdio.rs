use std::{ffi::CString, io::Write};

use super::{EmuState, EmuUC, FuncResult, UcResult, helpers::{ArgReader, UnicornExtras}};

pub enum CFile {
	StdOut,
	StdErr,
	File(std::fs::File)
}

impl CFile {
	fn needs_line_ending_conversion(&self) -> bool {
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
				let mut inner_string = arg_reader.read1::<CString>(uc)?.into_bytes();
				if let Some(prec) = precision {
					inner_string.truncate(prec as usize);
				}
				inner_string
			}
			b'd' => {
				let num: u32 = arg_reader.read1(uc)?;
				format!("{}", num).into_bytes()
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

fn cr_to_lf(data: &mut [u8]) {
	for c in data.iter_mut() {
		if *c == b'\r' {
			*c = b'\n';
		} else if *c > 127 {
			*c = b'?'; // for windows stdio
		}
	}
}

fn fprintf(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (file, format): (u32, CString) = reader.read2(uc)?;
	let mut output = internal_printf(uc, format.as_bytes(), reader)?;

	match state.stdio_files.get_mut(&file) {
		Some(f) => {
			if f.needs_line_ending_conversion() {
				cr_to_lf(&mut output);
			}
			Ok(Some(f.generic_write(&output)))
		}
		None => {
			// set errno later?
			Ok(Some(0))
		}
	}
}

fn sprintf(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (buffer, format): (u32, CString) = reader.read2(uc)?;
	let output = internal_printf(uc, format.as_bytes(), reader)?;

	uc.write_c_string(buffer, &output)?;
	Ok(Some(output.len() as u32))
}

fn fwrite(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (ptr, size, count, file): (u32, u32, u32, u32) = reader.read4(uc)?;
	let mut output = uc.mem_read_as_vec(ptr.into(), (size * count) as usize)?;

	match state.stdio_files.get_mut(&file) {
		Some(f) => {
			if f.needs_line_ending_conversion() {
				cr_to_lf(&mut output);
			}
			Ok(Some(f.generic_write(&output) / size))
		}
		None => {
			// set errno later?
			Ok(Some(0))
		}
	}
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
	// fclose
	// fflush
	// fopen
	// freopen
	state.install_shim_function("fprintf", fprintf);
	// fscanf
	// printf
	// scanf
	state.install_shim_function("sprintf", sprintf);
	// sscanf
	// vfprintf
	// vprintf
	// vsprintf
	// fgetc
	// fgets
	// fputc
	// fputs
	// gets
	// puts
	// ungetc
	// fread
	state.install_shim_function("fwrite", fwrite);
	// fgetpos
	// ftell
	// fsetpos
	// fseek
	// rewind
	// clearerr
	// perror
	// getc
	// putc
	// getchar
	// putchar
	// feof
	// ferror
}
