use std::{ffi::CString, io::{Write, Read}, rc::Rc, cell::RefCell};

use crate::{mac_roman, filesystem::MacFile};

use super::{EmuState, EmuUC, FuncResult, UcResult, helpers::{ArgReader, UnicornExtras}};

pub(super) struct FileHandle {
	file: Rc<RefCell<MacFile>>,
	position: usize
}

pub(super) enum CFile {
	StdIn,
	StdOut,
	StdErr,
	File(FileHandle)
}

impl CFile {
	fn is_terminal(&self) -> bool {
		match self {
			CFile::StdIn | CFile::StdOut | CFile::StdErr => true,
			_ => false
		}
	}

	fn generic_read(&mut self, buffer: &mut [u8]) -> u32 {
		let read_result = match self {
			CFile::StdIn => std::io::stdin().read(buffer),
			CFile::StdOut | CFile::StdErr => return 0,
			CFile::File(handle) => {
				let file = handle.file.borrow();
				let current_pos = handle.position;
				let new_pos = (handle.position + buffer.len()).min(file.data_fork.len());
				buffer[0..new_pos - current_pos].copy_from_slice(&file.data_fork[current_pos..new_pos]);
				handle.position = new_pos;
				return (new_pos - current_pos) as u32;
			}
		};

		match read_result {
			Ok(amount) => amount as u32,
			Err(e) => {
				error!(target: "stdio", "failed to read from file: {e:?}");
				0
			}
		}
	}

	fn generic_write(&mut self, buffer: &[u8]) -> u32 {
		let write_result = match self {
			CFile::StdIn => return 0,
			CFile::StdOut => std::io::stdout().write(buffer),
			CFile::StdErr => std::io::stderr().write(buffer),
			CFile::File(handle) => {
				let mut file = handle.file.borrow_mut();
				let current_pos = handle.position;
				let new_pos = handle.position + buffer.len();
				if new_pos > file.data_fork.len() {
					file.data_fork.resize(new_pos, 0);
				}
				file.data_fork[current_pos..new_pos].copy_from_slice(buffer);
				handle.position = new_pos;
				return buffer.len() as u32;
			}
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
			CFile::File(handle) => {
				handle.position as u32
			},
			_ => {
				warn!(target: "stdio", "running ftell() on stdin, stdout or stderr");
				0xFFFFFFFF
			}
		}
	}
}

#[allow(unused_variables)]
#[allow(unused_assignments)]
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
		if ch == b'%' {
			output.push(ch);
			continue;
		}

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
				let digit = (ch - b'0') as i32;
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
					let digit = (ch - b'0') as i32;
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
				format!("{:01$X}", num, precision.unwrap_or(0) as usize).into_bytes()
			}
			_ => {
				error!(target: "stdio", "Unimplemented format character: {}", ch as char);
				format!("?{}", ch as char).into_bytes()
			}
		};

		let what_len = what.len() as isize;
		let min_width = min_width.unwrap_or(0) as isize;
		let padding = min_width.max(what_len) - what_len;
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

fn setvbuf(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (file, buf_ptr, mode, size): (u32, u32, i32, u32) = reader.read4(uc)?;
	trace!(target: "stdio", "setvbuf(file={file:08X}, buf={buf_ptr:08x}, mode={mode}, size={size}");
	Ok(Some(0)) // pretend we did something. (we didn't)
}

fn fclose(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let file: u32 = reader.read1(uc)?;

	if state.stdio_files.contains_key(&file) {
		state.stdio_files.remove(&file);
		state.heap.dispose_ptr(uc, file)?;
		Ok(Some(0))
	} else {
		warn!(target: "stdio", "fclose() on invalid file {file:08X}");
		// TODO: this should be EOF, check what it is in MSL
		Ok(Some(0xFFFFFFFF))
	}
}

fn fflush(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let file: u32 = reader.read1(uc)?;

	if state.stdio_files.contains_key(&file) {
		// pretend we did something
		Ok(Some(0))
	} else {
		warn!(target: "stdio", "fflush() on invalid file {file:08X}");
		// TODO: this should be EOF, check what it is in MSL
		Ok(Some(0xFFFFFFFF))
	}
}

fn fopen(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (name, mode): (CString, CString) = reader.read2(uc)?;

	trace!(target: "stdio", "fopen({name:?}, {mode:?})");

	let path = match state.filesystem.resolve_path(0, 0, name.as_bytes()) {
		Ok(p) => p,
		Err(e) => {
			error!(target: "stdio", "fopen failed to resolve path {name:?}: {e:?}");
			return Ok(Some(0));
		}
	};

	let file = match state.filesystem.get_file(&path) {
		Ok(f) => f,
		Err(e) => {
			error!(target: "stdio", "fopen failed to get file {name:?}: {e:?}");
			return Ok(Some(0));
		}
	};

	let ptr = state.heap.new_ptr(uc, 0x18)?;
	// set _cnt to a negative value so getc() and putc() will always call a hooked function
	uc.write_u32(ptr, 0xFFFFFFFE)?;
	state.stdio_files.insert(ptr, CFile::File(FileHandle { file, position: 0 }));

	Ok(Some(ptr))
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

fn printf(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let format: CString = reader.read1(uc)?;
	trace!(target: "stdio", "printf({format:?}, ...)");
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

fn fgets(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (ptr, max_size, file): (u32, u32, u32) = reader.read3(uc)?;

	match state.stdio_files.get_mut(&file) {
		Some(f) => {
			let mut buffer = Vec::new();
			let mut byte = [0u8];

			while buffer.len() < (max_size - 1) as usize {
				if f.generic_read(&mut byte) == 0 {
					break;
				}
				buffer.push(byte[0]);
				if byte[0] == b'\r' || byte[0] == b'\n' {
					break;
				}
			}

			if buffer.len() == 0 {
				trace!(target: "stdio", "fgets() reached end");
				Ok(Some(0))
			} else {
				// do we need to handle MacRoman here? probably
				uc.write_c_string(ptr, &buffer)?;

				let s = CString::new(buffer).unwrap();
				trace!(target: "stdio", "fgets() returned: [{s:?}]");

				Ok(Some(ptr))
			}
		}
		None => {
			warn!(target: "stdio", "fgets() is getting from invalid file {file:08X}");
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

fn putchar(uc: &mut EmuUC, _state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let ch: u8 = reader.read1(uc)?;
	print!("{}", mac_roman::decode_char(ch, true));
	Ok(Some(ch.into()))
}

fn filbuf(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let file_ptr: u32 = reader.read1(uc)?;

	// set _cnt to a negative value so getc() and putc() will always call a hooked function
	uc.write_u32(file_ptr, 0xFFFFFFFE)?;

	// get a singular byte
	let mut byte = [0u8];
	if let Some(file) = state.stdio_files.get_mut(&file_ptr) {
		if file.generic_read(&mut byte) == 1 {
			Ok(Some(byte[0] as u32))
		} else {
			Ok(Some(0xFFFFFFFF))
		}
	} else {
		Ok(Some(0))
	}
}

fn flsbuf(uc: &mut EmuUC, state: &mut EmuState, reader: &mut ArgReader) -> FuncResult {
	let (ch, file_ptr): (u8, u32) = reader.read2(uc)?;

	// set _cnt to a negative value so getc() and putc() will always call a hooked function
	uc.write_u32(file_ptr, 0xFFFFFFFE)?;

	// write a singular byte
	let byte = [ch];
	if let Some(file) = state.stdio_files.get_mut(&file_ptr) {
		if file.generic_write(&byte) == 1 {
			return Ok(Some(byte[0] as u32));
		}
	}

	Ok(Some(0xFFFFFFFF))
}

pub(super) fn install_shims(state: &mut EmuState) {
	if let Some(iob) = state.get_shim_addr("_iob") {
		state.stdio_files.insert(iob, CFile::StdIn);
		state.stdio_files.insert(iob + 0x18, CFile::StdOut);
		state.stdio_files.insert(iob + 0x30, CFile::StdErr);
	}

	// remove
	// rename
	// tmpnam
	// tmpfile
	// setbuf
	state.install_shim_function("setvbuf", setvbuf);
	state.install_shim_function("fclose", fclose);
	state.install_shim_function("fflush", fflush);
	state.install_shim_function("fopen", fopen);
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
	state.install_shim_function("fgets", fgets);
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

	state.install_shim_function("_filbuf", filbuf);
	state.install_shim_function("_flsbuf", flsbuf);
}
