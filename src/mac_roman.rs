use std::borrow::Cow;

const CONVERSIONS: [&str; 128] = [
	"Ä", "Å", "Ç", "É", "Ñ", "Ö", "Ü", "á", "à", "â", "ä", "ã", "å", "ç", "é", "è",
	"ê", "ë", "í", "ì", "î", "ï", "ñ", "ó", "ò", "ô", "ö", "õ", "ú", "ù", "û", "ü",
	"†", "°", "¢", "£", "§", "•", "¶", "ß", "®", "©", "™", "´", "¨", "≠", "Æ", "Ø",
	"∞", "±", "≤", "≥", "¥", "µ", "∂", "∑", "∏", "π", "∫", "ª", "º", "Ω", "æ", "ø",
	"¿", "¡", "¬", "√", "ƒ", "≈", "∆", "«", "»", "…", "\u{A0}", "À", "Ã", "Õ", "Œ", "œ",
	"–", "—", "“", "”", "‘", "’", "÷", "◊", "ÿ", "Ÿ", "⁄", "€", "‹", "›", "ﬁ", "ﬂ",
	"‡", "·", "‚", "„", "‰", "Â", "Ê", "Á", "Ë", "È", "Í", "Î", "Ï", "Ì", "Ó", "Ô",
	"\u{F8FF}", "Ò", "Ú", "Û", "Ù", "ı", "ˆ", "˜", "¯", "˘", "˙", "˚", "¸", "˝", "˛", "ˇ"
];

pub fn decode_mac_roman(buf: &[u8], cr_to_lf: bool) -> Cow<[u8]> {
	let mut safe = true;
	for &c in buf {
		if !c.is_ascii() || (cr_to_lf && c == b'\r') {
			safe = false;
			break;
		}
	}

	if safe {
		Cow::Borrowed(buf)
	} else {
		let mut s = String::with_capacity(buf.len() + (buf.len() / 2));

		for &c in buf {
			if cr_to_lf && c == b'\r' {
				s.push('\n');
			} else if c.is_ascii() {
				s.push(c as char);
			} else {
				s.push_str(CONVERSIONS[(c - 0x80) as usize]);
			}
		}

		Cow::Owned(s.into_bytes())
	}
}
