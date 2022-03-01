use std::fmt;
use binread::BinRead;
use chrono::{prelude::*, Duration};

const UNIX_TO_MAC_DELTA: i64 = 2082844800;

fn get_mac_epoch() -> DateTime<Local> {
	Local.ymd(1904, 1, 1).and_hms(0, 0, 0)
}

pub fn parse_mac_time(time: u32) -> DateTime<Local> {
	get_mac_epoch() + Duration::seconds(time.into())
}

pub fn get_mac_time(dt: DateTime<Local>) -> u32 {
	(dt - get_mac_epoch()).num_seconds() as u32
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
pub enum OSErr {
	IOError = -36,
	BadName = -37,
	Eof = -39,
	FileNotFound = -43,
	RefNum = -51,
	NotEnoughMemory = -108,
	DirNotFound = -120,
	GestaltUndefSelector = -5551
}

impl OSErr {
	pub fn to_u32(self) -> u32 {
		self as i16 as i32 as u32
	}
}
