use std::borrow::Cow;

const CONVERSIONS: [char; 128] = [
	'Ä', 'Å', 'Ç', 'É', 'Ñ', 'Ö', 'Ü', 'á', 'à', 'â', 'ä', 'ã', 'å', 'ç', 'é', 'è',
	'ê', 'ë', 'í', 'ì', 'î', 'ï', 'ñ', 'ó', 'ò', 'ô', 'ö', 'õ', 'ú', 'ù', 'û', 'ü',
	'†', '°', '¢', '£', '§', '•', '¶', 'ß', '®', '©', '™', '´', '¨', '≠', 'Æ', 'Ø',
	'∞', '±', '≤', '≥', '¥', 'µ', '∂', '∑', '∏', 'π', '∫', 'ª', 'º', 'Ω', 'æ', 'ø',
	'¿', '¡', '¬', '√', 'ƒ', '≈', '∆', '«', '»', '…', '\u{A0}', 'À', 'Ã', 'Õ', 'Œ', 'œ',
	'–', '—', '“', '”', '‘', '’', '÷', '◊', 'ÿ', 'Ÿ', '⁄', '€', '‹', '›', 'ﬁ', 'ﬂ',
	'‡', '·', '‚', '„', '‰', 'Â', 'Ê', 'Á', 'Ë', 'È', 'Í', 'Î', 'Ï', 'Ì', 'Ó', 'Ô',
	'\u{F8FF}', 'Ò', 'Ú', 'Û', 'Ù', 'ı', 'ˆ', '˜', '¯', '˘', '˙', '˚', '¸', '˝', '˛', 'ˇ'
];

pub fn to_lower(ch: u8) -> u8 {
	match ch {
		b'A'..=b'Z' => ch + (b'a' - b'A'),
		0x80 => 0x8A,
		0x81 => 0x8C,
		0x82 => 0x8D,
		0x83 => 0x8E,
		0x84 => 0x96,
		0x85 => 0x9A,
		0x86 => 0x9F,
		0xAE => 0xBE,
		0xAF => 0xBF,
		0xCB => 0x88,
		0xCC => 0x8B,
		0xCD => 0x9B,
		0xCE => 0xCF,
		0xD9 => 0xD8,
		0xE5 => 0x89,
		0xE6 => 0x90,
		0xE7 => 0x87,
		0xE8 => 0x91,
		0xE9 => 0x8F,
		0xEA => 0x92,
		0xEB => 0x94,
		0xEC => 0x95,
		0xED => 0x93,
		0xEE => 0x97,
		0xEF => 0x99,
		0xF1 => 0x98,
		0xF2 => 0x9C,
		0xF3 => 0x9E,
		0xF4 => 0x9D,
		_ => ch
	}
}

pub fn to_upper(ch: u8) -> u8 {
	match ch {
		b'a'..=b'z' => ch - (b'a' - b'A'),
		0x87 => 0xE7,
		0x88 => 0xCB,
		0x89 => 0xE5,
		0x8A => 0x80,
		0x8B => 0xCC,
		0x8C => 0x81,
		0x8D => 0x82,
		0x8E => 0x83,
		0x8F => 0xE9,
		0x90 => 0xE6,
		0x91 => 0xE8,
		0x92 => 0xEA,
		0x93 => 0xED,
		0x94 => 0xEB,
		0x95 => 0xEC,
		0x96 => 0x84,
		0x97 => 0xEE,
		0x98 => 0xF1,
		0x99 => 0xEF,
		0x9A => 0x85,
		0x9B => 0xCD,
		0x9C => 0xF2,
		0x9D => 0xF4,
		0x9E => 0xF3,
		0x9F => 0x86,
		0xBE => 0xAE,
		0xBF => 0xAF,
		0xCF => 0xCE,
		0xD8 => 0xD9,
		_ => ch
	}
}

pub fn decode_string(buf: &[u8], cr_to_lf: bool) -> Cow<str> {
	let mut safe = true;
	for &c in buf {
		if !c.is_ascii() || (cr_to_lf && c == b'\r') {
			safe = false;
			break;
		}
	}

	if safe {
		Cow::Borrowed(unsafe { std::str::from_utf8_unchecked(buf) })
	} else {
		let mut s = String::with_capacity(buf.len() + (buf.len() / 2));

		for &c in buf {
			s.push(decode_char(c, cr_to_lf));
		}

		Cow::Owned(s)
	}
}

pub fn decode_buffer(buf: &[u8], cr_to_lf: bool) -> Cow<[u8]> {
	match decode_string(buf, cr_to_lf) {
		Cow::Borrowed(s) => Cow::Borrowed(s.as_bytes()),
		Cow::Owned(s) => Cow::Owned(s.into_bytes())
	}
}

pub fn decode_char(ch: u8, cr_to_lf: bool) -> char {
	if cr_to_lf && ch == b'\r' {
		'\n'
	} else if ch < 0x80 {
		ch as char
	} else {
		CONVERSIONS[(ch - 0x80) as usize]
	}
}
