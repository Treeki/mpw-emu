use std::{fmt, time::SystemTime};
use binread::BinRead;
use chrono::{prelude::*, Duration};

pub fn lf_to_cr(buffer: &mut [u8]) {
	for ch in buffer {
		if *ch == b'\n' {
			*ch = b'\r';
		}
	}
}

fn get_mac_epoch() -> DateTime<Local> {
	Local.ymd(1904, 1, 1).and_hms(0, 0, 0)
}

pub fn parse_mac_time(time: u32) -> DateTime<Local> {
	get_mac_epoch() + Duration::seconds(time.into())
}

pub fn get_mac_time(dt: DateTime<Local>) -> u32 {
	(dt - get_mac_epoch()).num_seconds() as u32
}

pub fn system_time_to_mac_time(st: SystemTime) -> u32 {
	if let Ok(diff) = st.duration_since(SystemTime::UNIX_EPOCH) {
		(diff.as_secs() + 2082844800) as u32
	} else {
		0
	}
}

#[derive(BinRead, Clone, Copy, Hash, PartialEq, Eq)]
pub struct FourCC(pub u32);
impl fmt::Debug for FourCC {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(
			f, "{:08x} ({}{}{}{})",
			self.0,
			((self.0 & 0xFF000000) >> 24) as u8 as char,
			((self.0 & 0x00FF0000) >> 16) as u8 as char,
			((self.0 & 0x0000FF00) >> 8) as u8 as char,
			(self.0 & 0x000000FF) as u8 as char,
		)
	}
}

pub const fn four_cc(what: [u8; 4]) -> FourCC {
	FourCC(((what[0] as u32) << 24) | ((what[1] as u32) << 16) | ((what[2] as u32) << 8) | (what[3] as u32))
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(i16)]
#[allow(dead_code)]
pub enum OSErr {
	NoError = 0,
	NoSuchVolume = -35,
	IOError = -36,
	BadName = -37,
	Eof = -39,
	Position = -40,
	FileNotFound = -43,
	FileLocked = -45,
	FileBusy = -47,
	DuplicateFilename = -48,
	Param = -50,
	RefNum = -51,
	NotEnoughMemory = -108,
	NilHandle = -109,
	DirNotFound = -120,
	GestaltUndefSelector = -5551
}

impl OSErr {
	pub fn to_u32(self) -> u32 {
		self as i16 as i32 as u32
	}
}
