use std::ffi::CString;

use unicorn_engine::{Unicorn, RegisterPPC};

use crate::common::FourCC;

use super::{EmuUC, UcResult};

pub(super) struct ArgReader {
	use_pascal_strings: bool,
	gpr_id: i32
}

impl ArgReader {
	pub fn new() -> Self {
		ArgReader { use_pascal_strings: false, gpr_id: RegisterPPC::GPR3.into() }
	}

	pub fn pstr(&mut self) -> &mut Self {
		self.use_pascal_strings = true;
		self
	}

	pub(super) fn read1<T: ReadableArg>(&mut self, uc: &EmuUC) -> UcResult<T> {
		T::get_from_reader(self, uc, self.use_pascal_strings)
	}
	pub(super) fn read2<T1: ReadableArg, T2: ReadableArg>(&mut self, uc: &EmuUC) -> UcResult<(T1, T2)> {
		let a = T1::get_from_reader(self, uc, self.use_pascal_strings)?;
		let b = T2::get_from_reader(self, uc, self.use_pascal_strings)?;
		Ok((a, b))
	}
	pub(super) fn read3<T1: ReadableArg, T2: ReadableArg, T3: ReadableArg>(&mut self, uc: &EmuUC) -> UcResult<(T1, T2, T3)> {
		let a = T1::get_from_reader(self, uc, self.use_pascal_strings)?;
		let b = T2::get_from_reader(self, uc, self.use_pascal_strings)?;
		let c = T3::get_from_reader(self, uc, self.use_pascal_strings)?;
		Ok((a, b, c))
	}
	pub(super) fn read4<T1: ReadableArg, T2: ReadableArg, T3: ReadableArg, T4: ReadableArg>(&mut self, uc: &EmuUC) -> UcResult<(T1, T2, T3, T4)> {
		let a = T1::get_from_reader(self, uc, self.use_pascal_strings)?;
		let b = T2::get_from_reader(self, uc, self.use_pascal_strings)?;
		let c = T3::get_from_reader(self, uc, self.use_pascal_strings)?;
		let d = T4::get_from_reader(self, uc, self.use_pascal_strings)?;
		Ok((a, b, c, d))
	}
	pub(super) fn read5<T1: ReadableArg, T2: ReadableArg, T3: ReadableArg, T4: ReadableArg, T5: ReadableArg>(&mut self, uc: &EmuUC) -> UcResult<(T1, T2, T3, T4, T5)> {
		let a = T1::get_from_reader(self, uc, self.use_pascal_strings)?;
		let b = T2::get_from_reader(self, uc, self.use_pascal_strings)?;
		let c = T3::get_from_reader(self, uc, self.use_pascal_strings)?;
		let d = T4::get_from_reader(self, uc, self.use_pascal_strings)?;
		let e = T5::get_from_reader(self, uc, self.use_pascal_strings)?;
		Ok((a, b, c, d, e))
	}
	pub(super) fn read6<T1: ReadableArg, T2: ReadableArg, T3: ReadableArg, T4: ReadableArg, T5: ReadableArg, T6: ReadableArg>(&mut self, uc: &EmuUC) -> UcResult<(T1, T2, T3, T4, T5, T6)> {
		let a = T1::get_from_reader(self, uc, self.use_pascal_strings)?;
		let b = T2::get_from_reader(self, uc, self.use_pascal_strings)?;
		let c = T3::get_from_reader(self, uc, self.use_pascal_strings)?;
		let d = T4::get_from_reader(self, uc, self.use_pascal_strings)?;
		let e = T5::get_from_reader(self, uc, self.use_pascal_strings)?;
		let f = T6::get_from_reader(self, uc, self.use_pascal_strings)?;
		Ok((a, b, c, d, e, f))
	}
	pub(super) fn read7<T1: ReadableArg, T2: ReadableArg, T3: ReadableArg, T4: ReadableArg, T5: ReadableArg, T6: ReadableArg, T7: ReadableArg>(&mut self, uc: &EmuUC) -> UcResult<(T1, T2, T3, T4, T5, T6, T7)> {
		let a = T1::get_from_reader(self, uc, self.use_pascal_strings)?;
		let b = T2::get_from_reader(self, uc, self.use_pascal_strings)?;
		let c = T3::get_from_reader(self, uc, self.use_pascal_strings)?;
		let d = T4::get_from_reader(self, uc, self.use_pascal_strings)?;
		let e = T5::get_from_reader(self, uc, self.use_pascal_strings)?;
		let f = T6::get_from_reader(self, uc, self.use_pascal_strings)?;
		let g = T7::get_from_reader(self, uc, self.use_pascal_strings)?;
		Ok((a, b, c, d, e, f, g))
	}

	pub fn read_gpr(&mut self, uc: &EmuUC) -> UcResult<u32> {
		let value = uc.reg_read(self.gpr_id)? as u32;
		self.gpr_id += 1;
		Ok(value)
	}
}

pub(super) trait ReadableArg {
	fn get_from_reader(reader: &mut ArgReader, uc: &EmuUC, pstr_flag: bool) -> UcResult<Self> where Self: Sized;
}

impl ReadableArg for u8 {
	fn get_from_reader(reader: &mut ArgReader, uc: &EmuUC, _pstr_flag: bool) -> UcResult<Self> {
		Ok((reader.read_gpr(uc)? & 0xFF) as u8)
	}
}
impl ReadableArg for u16 {
	fn get_from_reader(reader: &mut ArgReader, uc: &EmuUC, _pstr_flag: bool) -> UcResult<Self> {
		Ok((reader.read_gpr(uc)? & 0xFFFF) as u16)
	}
}
impl ReadableArg for u32 {
	fn get_from_reader(reader: &mut ArgReader, uc: &EmuUC, _pstr_flag: bool) -> UcResult<Self> {
		reader.read_gpr(uc)
	}
}
impl ReadableArg for i8 {
	fn get_from_reader(reader: &mut ArgReader, uc: &EmuUC, _pstr_flag: bool) -> UcResult<Self> {
		Ok((reader.read_gpr(uc)? & 0xFF) as u8 as i8)
	}
}
impl ReadableArg for i16 {
	fn get_from_reader(reader: &mut ArgReader, uc: &EmuUC, _pstr_flag: bool) -> UcResult<Self> {
		Ok((reader.read_gpr(uc)? & 0xFFFF) as u16 as i16)
	}
}
impl ReadableArg for i32 {
	fn get_from_reader(reader: &mut ArgReader, uc: &EmuUC, _pstr_flag: bool) -> UcResult<Self> {
		Ok(reader.read_gpr(uc)? as i32)
	}
}
impl ReadableArg for FourCC {
	fn get_from_reader(reader: &mut ArgReader, uc: &EmuUC, _pstr_flag: bool) -> UcResult<Self> {
		Ok(FourCC(reader.read_gpr(uc)?))
	}
}
impl ReadableArg for CString {
	fn get_from_reader(reader: &mut ArgReader, uc: &EmuUC, pstr_flag: bool) -> UcResult<Self> {
		let addr = reader.read_gpr(uc)?;
		if pstr_flag {
			uc.read_pascal_string(addr)
		} else {
			uc.read_c_string(addr)
		}
	}
}

pub trait UnicornExtras {
	fn read_u8(&self, addr: u32) -> UcResult<u8>;
	fn read_u16(&self, addr: u32) -> UcResult<u16>;
	fn read_u32(&self, addr: u32) -> UcResult<u32>;
	fn read_i8(&self, addr: u32) -> UcResult<i8>;
	fn read_i16(&self, addr: u32) -> UcResult<i16>;
	fn read_i32(&self, addr: u32) -> UcResult<i32>;
	fn read_c_string(&self, addr: u32) -> UcResult<CString>;
	fn read_pascal_string(&self, addr: u32) -> UcResult<CString>;

	fn write_u8(&mut self, addr: u32, value: u8) -> UcResult<()>;
	fn write_u16(&mut self, addr: u32, value: u16) -> UcResult<()>;
	fn write_u32(&mut self, addr: u32, value: u32) -> UcResult<()>;
	fn write_i8(&mut self, addr: u32, value: i8) -> UcResult<()>;
	fn write_i16(&mut self, addr: u32, value: i16) -> UcResult<()>;
	fn write_i32(&mut self, addr: u32, value: i32) -> UcResult<()>;
	fn write_c_string(&mut self, addr: u32, s: &[u8]) -> UcResult<()>;
	fn write_pascal_string(&mut self, addr: u32, s: &[u8]) -> UcResult<()>;
}
impl<D> UnicornExtras for Unicorn<'_, D> {
	fn read_u8(&self, addr: u32) -> UcResult<u8> {
		let mut bytes = [0u8];
		self.mem_read(addr as u64, &mut bytes)?;
		Ok(bytes[0])
	}

	fn read_u16(&self, addr: u32) -> UcResult<u16> {
		let mut bytes = [0u8; 2];
		self.mem_read(addr as u64, &mut bytes)?;
		Ok(u16::from_be_bytes(bytes))
	}

	fn read_u32(&self, addr: u32) -> UcResult<u32> {
		let mut bytes = [0u8; 4];
		self.mem_read(addr as u64, &mut bytes)?;
		Ok(u32::from_be_bytes(bytes))
	}

	fn read_i8(&self, addr: u32) -> UcResult<i8> {
		let mut bytes = [0u8];
		self.mem_read(addr as u64, &mut bytes)?;
		Ok(bytes[0] as i8)
	}

	fn read_i16(&self, addr: u32) -> UcResult<i16> {
		let mut bytes = [0u8; 2];
		self.mem_read(addr as u64, &mut bytes)?;
		Ok(i16::from_be_bytes(bytes))
	}

	fn read_i32(&self, addr: u32) -> UcResult<i32> {
		let mut bytes = [0u8; 4];
		self.mem_read(addr as u64, &mut bytes)?;
		Ok(i32::from_be_bytes(bytes))
	}

	fn read_c_string(&self, mut addr: u32) -> UcResult<CString> {
		let mut result = Vec::new();

		loop {
			let b = self.read_u8(addr)?;
			if b == 0 {
				break;
			} else {
				result.push(b);
				addr += 1;
			}
		}

		Ok(CString::new(result).unwrap())
	}

	fn read_pascal_string(&self, addr: u32) -> UcResult<CString> {
		let len = self.read_u8(addr)?;
		let mut result = Vec::new();

		for i in 0..len {
			result.push(self.read_u8(addr + 1 + (i as u32))?);
		}

		Ok(CString::new(result).unwrap())
	}

	fn write_u8(&mut self, addr: u32, value: u8) -> UcResult<()> {
		self.mem_write(addr as u64, &[value])
	}
	fn write_u16(&mut self, addr: u32, value: u16) -> UcResult<()> {
		self.mem_write(addr as u64, &value.to_be_bytes())
	}
	fn write_u32(&mut self, addr: u32, value: u32) -> UcResult<()> {
		self.mem_write(addr as u64, &value.to_be_bytes())
	}
	fn write_i8(&mut self, addr: u32, value: i8) -> UcResult<()> {
		self.mem_write(addr as u64, &value.to_be_bytes())
	}
	fn write_i16(&mut self, addr: u32, value: i16) -> UcResult<()> {
		self.mem_write(addr as u64, &value.to_be_bytes())
	}
	fn write_i32(&mut self, addr: u32, value: i32) -> UcResult<()> {
		self.mem_write(addr as u64, &value.to_be_bytes())
	}

	fn write_c_string(&mut self, addr: u32, s: &[u8]) -> UcResult<()> {
		self.mem_write(addr as u64, s)?;
		if !s.ends_with(b"\0") {
			self.mem_write((addr as usize + s.len()) as u64, b"\0")?;
		}
		Ok(())
	}

	fn write_pascal_string(&mut self, addr: u32, s: &[u8]) -> UcResult<()> {
		self.write_u8(addr, s.len() as u8)?;
		self.mem_write((addr + 1) as u64, s)
	}
}
