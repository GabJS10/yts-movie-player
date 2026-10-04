//! Subtitle text handling: decoding to UTF-8 and SRT → WebVTT conversion.
//!
//! Real-world SRT files are messy: UTF-8 with BOM, UTF-16, or windows-1252/latin-1
//! (common for Spanish), `\r\n`, timestamps with `,` or `.`, missing or broken cue
//! numbers, extra blank lines and ASS override tags such as `{\an8}`. The output is
//! always a clean WebVTT document with cues sorted by start time.

/// Windows-1252 code points for bytes 0x80–0x9F (the rest is latin-1). Undefined bytes
/// map to U+FFFD.
const CP1252_HIGH: [char; 32] = [
    '€', '\u{FFFD}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{FFFD}', 'Ž',
    '\u{FFFD}', '\u{FFFD}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{FFFD}',
    'ž', 'Ÿ',
];

/// Decodes subtitle bytes: UTF-8 (BOM stripped), UTF-16 with BOM, else windows-1252.
pub fn decode(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8_lossy(rest).into_owned();
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return decode_utf16(rest, u16::from_le_bytes);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return decode_utf16(rest, u16::from_be_bytes);
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_owned(),
        Err(_) => bytes
            .iter()
            .map(|&b| match b {
                0x80..=0x9F => CP1252_HIGH[usize::from(b - 0x80)],
                _ => char::from(b),
            })
            .collect(),
    }
}

fn decode_utf16(bytes: &[u8], word: fn([u8; 2]) -> u16) -> String {
    let words: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|c| word(*c)).collect();
    String::from_utf16_lossy(&words)
}

/// Milliseconds from `HH:MM:SS,mmm`, `H:MM:SS.mmm` or `MM:SS,mmm` (spaces tolerated).
pub fn parse_timestamp(s: &str) -> Option<u64> {
    let s = s.trim();
    let (clock, frac) = match s.rfind([',', '.']) {
        Some(i) => (&s[..i], &s[i + 1..]),
        None => (s, ""),
    };
    let parts: Vec<&str> = clock.split(':').map(str::trim).collect();
    let num = |p: &str| -> Option<u64> {
        (!p.is_empty() && p.len() <= 3 && p.bytes().all(|b| b.is_ascii_digit()))
            .then(|| p.parse().ok())
            .flatten()
    };
    let (h, m, sec) = match parts.as_slice() {
        [h, m, s] => (num(h)?, num(m)?, num(s)?),
        [m, s] => (0, num(m)?, num(s)?),
        _ => return None,
    };
    if m >= 60 || sec >= 60 {
        return None;
    }
    let frac = frac.trim();
    let ms = if frac.is_empty() {
        0
    } else {
        if frac.len() > 3 || !frac.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        // "5" is half a second: right-pad to milliseconds.
        format!("{frac:0<3}").parse::<u64>().ok()?
    };
    Some(((h * 60 + m) * 60 + sec) * 1000 + ms)
}

/// `start --> end` (anything after the end time, like SRT positions, is ignored).
fn parse_timing(line: &str) -> Option<(u64, u64)> {
    let (a, b) = line.split_once("-->")?;
    let start = parse_timestamp(a)?;
    let end = parse_timestamp(b.split_whitespace().next()?)?;
    Some((start, end))
}

fn format_timestamp(ms: u64) -> String {
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1000 % 60,
        ms % 1000
    )
}

/// Cleans one line of cue text: drops `{\…}` override tags and `<font>`, keeps
/// `<i>`, `<b>` and `<u>`, and escapes what WebVTT would misread.
fn clean_line(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(c) = rest.chars().next() {
        if c == '{' {
            if let Some(end) = rest.find('}') {
                if rest[1..].starts_with('\\') {
                    rest = &rest[end + 1..];
                    continue;
                }
            }
        }
        if c == '<' {
            if let Some(end) = rest.find('>') {
                let tag = rest[1..end].trim().to_ascii_lowercase();
                let name = tag.trim_start_matches('/');
                if matches!(name, "i" | "b" | "u") {
                    out.push_str(&rest[..=end].to_ascii_lowercase().replace(' ', ""));
                    rest = &rest[end + 1..];
                    continue;
                }
                if name == "font" || name.starts_with("font ") {
                    rest = &rest[end + 1..];
                    continue;
                }
            }
            out.push_str("&lt;");
            rest = &rest[1..];
            continue;
        }
        match c {
            '&' => {
                // Keep entities that are already escaped.
                let entity = rest.find(';').filter(|&i| i <= 8).is_some_and(|i| {
                    rest[1..i]
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '#')
                });
                out.push_str(if entity { "&" } else { "&amp;" });
            }
            _ => out.push(c),
        }
        rest = &rest[c.len_utf8()..];
    }
    // "-->" is the cue timing separator in WebVTT.
    out.replace("-->", "->").trim_end().to_owned()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cue {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

fn is_number(line: &str) -> bool {
    let t = line.trim();
    !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit())
}

/// Parses SRT text into cues. Tolerates missing/broken numbering and extra blank lines;
/// cues without text or with `end <= start` are dropped.
pub fn parse_srt(text: &str) -> Vec<Cue> {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = text.lines().collect();
    let mut cues: Vec<Cue> = Vec::new();
    let mut current: Option<(u64, u64, Vec<String>)> = None;
    let flush = |cur: Option<(u64, u64, Vec<String>)>, cues: &mut Vec<Cue>| {
        if let Some((start, end, text)) = cur {
            let text = text.join("\n").trim().to_owned();
            if end > start && !text.is_empty() {
                cues.push(Cue {
                    start_ms: start,
                    end_ms: end,
                    text,
                });
            }
        }
    };
    let next_non_empty = |from: usize| lines[from..].iter().find(|l| !l.trim().is_empty());
    for (i, line) in lines.iter().enumerate() {
        let line = line.trim_start_matches('\u{FEFF}');
        if let Some((start, end)) = parse_timing(line) {
            flush(current.take(), &mut cues);
            current = Some((start, end, Vec::new()));
            continue;
        }
        // A cue number (right before a timing line) is dropped.
        if is_number(line) && next_non_empty(i + 1).is_some_and(|l| parse_timing(l).is_some()) {
            continue;
        }
        if let Some((_, _, text)) = current.as_mut() {
            if line.trim().is_empty() {
                continue;
            }
            let cleaned = clean_line(line.trim());
            if !cleaned.is_empty() {
                text.push(cleaned);
            }
        }
    }
    flush(current, &mut cues);
    cues.sort_by_key(|c| c.start_ms);
    cues
}

pub fn cues_to_vtt(cues: &[Cue]) -> String {
    let mut out = String::from("WEBVTT\n\n");
    for cue in cues {
        out.push_str(&format_timestamp(cue.start_ms));
        out.push_str(" --> ");
        out.push_str(&format_timestamp(cue.end_ms));
        out.push('\n');
        out.push_str(&cue.text);
        out.push_str("\n\n");
    }
    out
}

/// Converts subtitle bytes (SRT or WebVTT, any common encoding) to clean WebVTT.
/// `None` if no cue could be read.
pub fn to_vtt(bytes: &[u8]) -> Option<String> {
    let text = decode(bytes);
    let trimmed = text.trim_start_matches('\u{FEFF}').trim_start();
    // WebVTT input: its timing lines use the same syntax, so the same parser rebuilds a
    // clean file (dropping the header, NOTE/STYLE blocks and cue settings).
    let body = if trimmed.starts_with("WEBVTT") {
        trimmed.split_once('\n').map(|(_, rest)| rest).unwrap_or("")
    } else {
        trimmed
    };
    let cues = parse_srt(body);
    (!cues.is_empty()).then(|| cues_to_vtt(&cues))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Vec<u8> {
        let path = format!("{}/tests/fixtures/subs/{name}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    #[test]
    fn timestamps() {
        assert_eq!(parse_timestamp("00:00:01,500"), Some(1_500));
        assert_eq!(parse_timestamp("01:02:03.004"), Some(3_723_004));
        assert_eq!(parse_timestamp(" 1:02:03,4 "), Some(3_723_400));
        assert_eq!(parse_timestamp("02:03,040"), Some(123_040));
        assert_eq!(parse_timestamp("00:00:05"), Some(5_000));
        assert_eq!(parse_timestamp("00:61:00,000"), None);
        assert_eq!(parse_timestamp("aa:00:00,000"), None);
        assert_eq!(parse_timestamp("00:00:00,1234"), None);
        assert_eq!(parse_timestamp(""), None);
        assert_eq!(format_timestamp(3_723_004), "01:02:03.004");
    }

    #[test]
    fn decodes_bom_utf16_and_windows_1252() {
        assert_eq!(decode(b"\xEF\xBB\xBFhola"), "hola");
        assert_eq!(decode("¿Qué?".as_bytes()), "¿Qué?");
        // "¿Qué pasó…?" in windows-1252 (… is 0x85, only in cp1252).
        assert_eq!(decode(b"\xBFQu\xE9 pas\xF3\x85?"), "¿Qué pasó…?");
        let utf16le: Vec<u8> = [0xFF, 0xFE]
            .into_iter()
            .chain("añ".encode_utf16().flat_map(|w| w.to_le_bytes()))
            .collect();
        assert_eq!(decode(&utf16le), "añ");
        let utf16be: Vec<u8> = [0xFE, 0xFF]
            .into_iter()
            .chain("añ".encode_utf16().flat_map(|w| w.to_be_bytes()))
            .collect();
        assert_eq!(decode(&utf16be), "añ");
    }

    #[test]
    fn cleans_tags() {
        assert_eq!(clean_line("{\\an8}<i>Arriba</i>"), "<i>Arriba</i>");
        assert_eq!(clean_line("<I>x</I> <B >y</B>"), "<i>x</i> <b>y</b>");
        assert_eq!(
            clean_line("<font color=\"#ffff00\">Amarillo</font>"),
            "Amarillo"
        );
        assert_eq!(clean_line("a < b && c"), "a &lt; b &amp;&amp; c");
        assert_eq!(clean_line("Tom &amp; Jerry"), "Tom &amp; Jerry");
        assert_eq!(clean_line("{not a tag} ok"), "{not a tag} ok");
        assert_eq!(clean_line("go --> there"), "go -> there");
    }

    #[test]
    fn simple_srt() {
        let srt = "1\r\n00:00:01,000 --> 00:00:02,500\r\nHola\r\n\r\n2\r\n00:00:03,000 --> 00:00:04,000\r\n<i>Dos</i>\r\nlíneas\r\n";
        assert_eq!(
            to_vtt(srt.as_bytes()).unwrap(),
            "WEBVTT\n\n00:00:01.000 --> 00:00:02.500\nHola\n\n\
             00:00:03.000 --> 00:00:04.000\n<i>Dos</i>\nlíneas\n\n"
        );
    }

    #[test]
    fn messy_srt_fixture() {
        // windows-1252, CRLF, broken and missing numbers, extra blank lines, {\an8},
        // <font>, positions after the timing, a cue out of order, an empty cue, a cue
        // with end before start and a number-only line of dialogue.
        let vtt = to_vtt(&fixture("messy-cp1252.srt")).unwrap();
        assert_eq!(
            vtt,
            String::from_utf8(fixture("messy-cp1252.expected.vtt")).unwrap()
        );
    }

    #[test]
    fn utf8_bom_fixture() {
        let vtt = to_vtt(&fixture("bom-utf8.srt")).unwrap();
        assert!(vtt.starts_with("WEBVTT\n\n00:00:00.500 --> 00:00:02.000\n"));
        assert!(vtt.contains("¡Buenos días, señor Muñoz!"));
        assert!(!vtt.contains('\u{FEFF}'));
        assert!(!vtt.contains('\r'));
    }

    #[test]
    fn webvtt_input_is_normalized() {
        let input = "\u{FEFF}WEBVTT - title\r\n\r\nNOTE a comment\r\n\r\n1\r\n00:01.000 --> 00:02.000 align:start\r\n{\\an8}Hola\r\n";
        assert_eq!(
            to_vtt(input.as_bytes()).unwrap(),
            "WEBVTT\n\n00:00:01.000 --> 00:00:02.000\nHola\n\n"
        );
    }

    #[test]
    fn not_a_subtitle() {
        assert_eq!(to_vtt(b""), None);
        assert_eq!(to_vtt(b"just some text\nwithout timings"), None);
        assert_eq!(to_vtt(&[0xFF, 0xD8, 0xFF, 0xE0, 0, 1, 2]), None);
    }
}
