//! Exact legacy codepage tables, generated from Python's standard codecs.
//! Each file starts with 256 little-endian u32 single-byte mappings, followed
//! by sorted (u16 byte-pair, u32 Unicode scalar) records. 0xffffffff is undefined.
//! These are data only; no Python process is involved in decoding.
use super::{Reader, Result};

fn codepage(charset: u16) -> Option<&'static [u8]> {
    Some(match charset {
        10000 => include_bytes!("charsets/mac_roman.bin"),
        1252 => include_bytes!("charsets/cp1252.bin"),
        1251 => include_bytes!("charsets/cp1251.bin"),
        932 => include_bytes!("charsets/cp932.bin"),
        936 => include_bytes!("charsets/gbk.bin"),
        949 => include_bytes!("charsets/cp949.bin"),
        950 => include_bytes!("charsets/big5.bin"),
        _ => return None,
    })
}

pub(super) fn decode(bytes: &[u8], charset: u16) -> Result<String> {
    let table =
        codepage(charset).ok_or_else(|| format!("Unsupported drawing text charset {charset}"))?;
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
// Entries start at byte 0x20. Zero denotes an unassigned byte.
#[rustfmt::skip]
const SYMBOL: [char; 224] = [
    '\u{0020}', '\u{0021}', '\u{2200}', '\u{0023}', '\u{2203}', '\u{0025}', '\u{0026}', '\u{220b}', // 20
    '\u{0028}', '\u{0029}', '\u{2217}', '\u{002b}', '\u{002c}', '\u{2212}', '\u{002e}', '\u{002f}', // 28
    '\u{0030}', '\u{0031}', '\u{0032}', '\u{0033}', '\u{0034}', '\u{0035}', '\u{0036}', '\u{0037}', // 30
    '\u{0038}', '\u{0039}', '\u{003a}', '\u{003b}', '\u{003c}', '\u{003d}', '\u{003e}', '\u{003f}', // 38
    '\u{2245}', '\u{0391}', '\u{0392}', '\u{03a7}', '\u{0394}', '\u{0395}', '\u{03a6}', '\u{0393}', // 40
    '\u{0397}', '\u{0399}', '\u{03d1}', '\u{039a}', '\u{039b}', '\u{039c}', '\u{039d}', '\u{039f}', // 48
    '\u{03a0}', '\u{0398}', '\u{03a1}', '\u{03a3}', '\u{03a4}', '\u{03a5}', '\u{03c2}', '\u{03a9}', // 50
    '\u{039e}', '\u{03a8}', '\u{0396}', '\u{005b}', '\u{2234}', '\u{005d}', '\u{22a5}', '\u{005f}', // 58
    '\u{f8e5}', '\u{03b1}', '\u{03b2}', '\u{03c7}', '\u{03b4}', '\u{03b5}', '\u{03c6}', '\u{03b3}', // 60
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
    '\u{00f7}', '\u{2260}', '\u{2261}', '\u{2248}', '\u{2026}', '\u{f8e6}', '\u{f8e7}', '\u{21b5}', // B8
    '\u{2135}', '\u{2111}', '\u{211c}', '\u{2118}', '\u{2297}', '\u{2295}', '\u{2205}', '\u{2229}', // C0
    '\u{222a}', '\u{2283}', '\u{2287}', '\u{2284}', '\u{2282}', '\u{2286}', '\u{2208}', '\u{2209}', // C8
    '\u{2220}', '\u{2207}', '\u{f6da}', '\u{f6d9}', '\u{f6db}', '\u{220f}', '\u{221a}', '\u{22c5}', // D0
    '\u{00ac}', '\u{2227}', '\u{2228}', '\u{21d4}', '\u{21d0}', '\u{21d1}', '\u{21d2}', '\u{21d3}', // D8
    '\u{25ca}', '\u{2329}', '\u{f8e8}', '\u{f8e9}', '\u{f8ea}', '\u{2211}', '\u{f8eb}', '\u{f8ec}', // E0
    '\u{f8ed}', '\u{f8ee}', '\u{f8ef}', '\u{f8f0}', '\u{f8f1}', '\u{f8f2}', '\u{f8f3}', '\u{f8f4}', // E8
    '\0', '\u{232a}', '\u{222b}', '\u{2320}', '\u{f8f5}', '\u{2321}', '\u{f8f6}', '\u{f8f7}', // F0
    '\u{f8f8}', '\u{f8f9}', '\u{f8fa}', '\u{f8fb}', '\u{f8fc}', '\u{f8fd}', '\u{f8fe}', '\0', // F8
];

pub(crate) fn symbol_text(text: &str, charset: u16) -> String {
    // CDX decodes legacy runs through a codepage. CDXML carries Symbol
    // bytes as code points, alongside any Unicode text already present.
    let upper = codepage(charset).and_then(|table| table.get(512..1024));
    text.chars()
        .map(|ch| {
            let point = u32::from(ch);
            let byte = match upper {
                Some(upper) if !ch.is_ascii() => {
                    let record = point.to_le_bytes();
                    upper
                        .chunks_exact(4)
                        .position(|entry| entry == record)
                        .map(|index| index + 0x80)
                }
                _ => Some(point as usize),
            };
            byte.and_then(|byte| byte.checked_sub(0x20))
                .and_then(|index| SYMBOL.get(index).copied())
                .filter(|&symbol| symbol != '\0')
                .unwrap_or(ch)
        })
        .collect()
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
