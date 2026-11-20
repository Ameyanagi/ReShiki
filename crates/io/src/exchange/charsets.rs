//! Exact legacy codepage tables, generated from Python's standard codecs.
//! Each file starts with 256 little-endian u32 single-byte mappings, followed
//! by sorted (u16 byte-pair, u32 Unicode scalar) records. 0xffffffff is undefined.
//! These are data only; no Python process is involved in decoding.
use super::{Reader, Result};

pub(super) fn decode(bytes: &[u8], charset: u16) -> Result<String> {
    let table: &[u8] = match charset {
        10000 => include_bytes!("charsets/mac_roman.bin"),
        1252 => include_bytes!("charsets/cp1252.bin"),
        1251 => include_bytes!("charsets/cp1251.bin"),
        932 => include_bytes!("charsets/cp932.bin"),
        936 => include_bytes!("charsets/gbk.bin"),
        949 => include_bytes!("charsets/cp949.bin"),
        950 => include_bytes!("charsets/big5.bin"),
        _ => return Err(format!("Unsupported drawing text charset {charset}")),
    };
    let (single, pairs) = table
        .split_at_checked(1024)
        .ok_or("Invalid built-in charset table")?;
    let mut iter = bytes.iter().copied();
    let mut text = String::new();
    while let Some(byte) = iter.next() {
        let at = usize::from(byte) * 4;
        let record = single
            .get(at..at + 4)
            .ok_or("Invalid built-in charset entry")?;
        let point = Reader::new(record).u32()?;
        let point = if point == u32::MAX {
            let trail = iter.next().ok_or("Invalid binary drawing text encoding")?;
            lookup(pairs, u16::from_be_bytes([byte, trail]))?
        } else {
            point
        };
        text.push(char::from_u32(point).ok_or("Invalid binary drawing text encoding")?);
    }
    Ok(text)
}

// Adobe Symbol Encoding to Unicode, Unicode, Inc. (1991–2011).
// https://www.unicode.org/Public/MAPPINGS/VENDORS/ADOBE/symbol.txt
// Private-use glyphs use their standard Unicode equivalents where available.
// Entries start at byte 0x20. Zero keeps the original character.
#[rustfmt::skip]
const SYMBOL: [char; 224] = [
    '\u{0020}', '\u{0021}', '\u{2200}', '\u{0023}', '\u{2203}', '\u{0025}', '\u{0026}', '\u{220b}', // 20
    // `*` and `-` stay ASCII: atom labels read them as a wildcard and a bond
    // or charge sign.
    '\u{0028}', '\u{0029}', '\u{002a}', '\u{002b}', '\u{002c}', '\u{002d}', '\u{002e}', '\u{002f}', // 28
    '\u{0030}', '\u{0031}', '\u{0032}', '\u{0033}', '\u{0034}', '\u{0035}', '\u{0036}', '\u{0037}', // 30
    '\u{0038}', '\u{0039}', '\u{003a}', '\u{003b}', '\u{003c}', '\u{003d}', '\u{003e}', '\u{003f}', // 38
    '\u{2245}', '\u{0391}', '\u{0392}', '\u{03a7}', '\u{0394}', '\u{0395}', '\u{03a6}', '\u{0393}', // 40
    '\u{0397}', '\u{0399}', '\u{03d1}', '\u{039a}', '\u{039b}', '\u{039c}', '\u{039d}', '\u{039f}', // 48
    '\u{03a0}', '\u{0398}', '\u{03a1}', '\u{03a3}', '\u{03a4}', '\u{03a5}', '\u{03c2}', '\u{03a9}', // 50
    '\u{039e}', '\u{03a8}', '\u{0396}', '\u{005b}', '\u{2234}', '\u{005d}', '\u{22a5}', '\u{005f}', // 58
    '\u{203e}', '\u{03b1}', '\u{03b2}', '\u{03c7}', '\u{03b4}', '\u{03b5}', '\u{03c6}', '\u{03b3}', // 60
    '\u{03b7}', '\u{03b9}', '\u{03d5}', '\u{03ba}', '\u{03bb}', '\u{03bc}', '\u{03bd}', '\u{03bf}', // 68
    '\u{03c0}', '\u{03b8}', '\u{03c1}', '\u{03c3}', '\u{03c4}', '\u{03c5}', '\u{03d6}', '\u{03c9}', // 70
    '\u{03be}', '\u{03c8}', '\u{03b6}', '\u{007b}', '\u{007c}', '\u{007d}', '\u{223c}', '\0', // 78
    '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', // 80
    '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', // 88
    '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', // 90
    '\0', '\0', '\0', '\0', '\0', '\0', '\0', '\0', // 98
    '\u{20ac}', '\u{03d2}', '\u{2032}', '\u{2264}', '\u{2044}', '\u{221e}', '\u{0192}', '\u{2663}', // A0
    '\u{2666}', '\u{2665}', '\u{2660}', '\u{2194}', '\u{2190}', '\u{2191}', '\u{2192}', '\u{2193}', // A8
    '\u{00b0}', '\u{00b1}', '\u{2033}', '\u{2265}', '\u{00d7}', '\u{221d}', '\u{2202}', '\u{2022}', // B0
    '\u{00f7}', '\u{2260}', '\u{2261}', '\u{2248}', '\u{2026}', '\u{23d0}', '\u{23af}', '\u{21b5}', // B8
    '\u{2135}', '\u{2111}', '\u{211c}', '\u{2118}', '\u{2297}', '\u{2295}', '\u{2205}', '\u{2229}', // C0
    '\u{222a}', '\u{2283}', '\u{2287}', '\u{2284}', '\u{2282}', '\u{2286}', '\u{2208}', '\u{2209}', // C8
    '\u{2220}', '\u{2207}', '\u{00ae}', '\u{00a9}', '\u{2122}', '\u{220f}', '\u{221a}', '\u{22c5}', // D0
    '\u{00ac}', '\u{2227}', '\u{2228}', '\u{21d4}', '\u{21d0}', '\u{21d1}', '\u{21d2}', '\u{21d3}', // D8
    '\u{25ca}', '\u{2329}', '\u{00ae}', '\u{00a9}', '\u{2122}', '\u{2211}', '\u{239b}', '\u{239c}', // E0
    '\u{239d}', '\u{23a1}', '\u{23a2}', '\u{23a3}', '\u{23a7}', '\u{23a8}', '\u{23a9}', '\u{23aa}', // E8
    '\0', '\u{232a}', '\u{222b}', '\u{2320}', '\u{23ae}', '\u{2321}', '\u{239e}', '\u{239f}', // F0
    '\u{23a0}', '\u{23a4}', '\u{23a5}', '\u{23a6}', '\u{23ab}', '\u{23ac}', '\u{23ad}', '\0', // F8
];

pub(crate) fn is_symbol_font(name: &str) -> bool {
    name.eq_ignore_ascii_case("Symbol") || name.eq_ignore_ascii_case("Symbol MT")
}

pub(crate) fn symbol_char(ch: char) -> char {
    (ch as usize)
        .checked_sub(0x20)
        .and_then(|index| SYMBOL.get(index).copied())
        .filter(|&symbol| symbol != '\0')
        .unwrap_or(ch)
}

fn lookup(pairs: &[u8], code: u16) -> Result<u32> {
    let (mut low, mut high) = (0, pairs.len() / 6);
    while low < high {
        let mid = low + (high - low) / 2;
        let at = mid * 6;
        let mut r = Reader::new(
            pairs
                .get(at..at + 6)
                .ok_or("Invalid built-in charset pair")?,
        );
        match r.u16()?.cmp(&code) {
            std::cmp::Ordering::Equal => return r.u32(),
            std::cmp::Ordering::Less => low = mid + 1,
            std::cmp::Ordering::Greater => high = mid,
        }
    }
    Err("Invalid binary drawing text encoding".into())
}
