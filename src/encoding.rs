//! Determine the encoding of an XML document given as bytes, and decode it.
//!
//! This follows the XML 1.0 specification, section 4.3.3 and appendix F: a
//! byte order mark determines the encoding; without one the encoding
//! declaration in the XML declaration does; without either the document is
//! UTF-8. An encoding declaration that does not match the byte order mark, or
//! the way the first characters are written in bytes, is a fatal error.
//!
//! An encoding name is looked up as a label of the WHATWG Encoding Standard,
//! which encoding_rs implements, except for the names in `IANA_NAMES`, which
//! IANA registers for US-ASCII, ISO-8859-1, ISO-8859-9 and TIS-620
//! (ISO-8859-11). The Encoding Standard makes many of these names stand for
//! windows-1252, windows-1254 and windows-874, which have characters such as
//! `€` for the bytes 0x80 to 0x9F, where the ISO 8859 encodings have the
//! control characters U+0080 to U+009F, and which have characters for the
//! bytes 0x80 and above, which are not US-ASCII. XML 1.0 asks for a name that
//! IANA registers to be taken as the encoding that IANA registers it for, so
//! these encodings are decoded as they are defined. `IANA_NAMES` has all the
//! names that IANA registers for them, except those with `:`, which an
//! encoding declaration cannot have.

use encoding_rs::{DecoderResult, Encoding, REPLACEMENT, UTF_16BE, UTF_16LE, UTF_8};

use crate::{error::ParseError, Span};

/// An encoding that a document can be decoded from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DocumentEncoding {
    /// An encoding of the WHATWG Encoding Standard, as encoding_rs decodes it.
    Whatwg(&'static Encoding),
    /// US-ASCII: each byte below 0x80 is the character with the same number;
    /// a byte of 0x80 or more is not valid.
    Ascii,
    /// ISO-8859-1: each byte is the character with the same number, U+0000 to
    /// U+00FF.
    Latin1,
    /// ISO-8859-9: ISO-8859-1 with six Turkish letters in place of Icelandic
    /// ones.
    Latin5,
    /// TIS-620, which IANA also registers as ISO-8859-11: ISO-8859-1 up to
    /// 0xA0, then Thai characters.
    Thai,
}

impl DocumentEncoding {
    /// The name of the encoding, as errors give it.
    pub(crate) fn name(self) -> &'static str {
        match self {
            DocumentEncoding::Whatwg(encoding) => encoding.name(),
            DocumentEncoding::Ascii => "US-ASCII",
            DocumentEncoding::Latin1 => "ISO-8859-1",
            DocumentEncoding::Latin5 => "ISO-8859-9",
            DocumentEncoding::Thai => "TIS-620",
        }
    }

    /// Whether this is UTF-16, in either byte order.
    fn is_utf16(self) -> bool {
        matches!(self, DocumentEncoding::Whatwg(encoding) if is_utf16(encoding))
    }
}

/// The encodings that are decoded as IANA registers them rather than as the
/// WHATWG Encoding Standard has them, with the names IANA registers for them
/// in its Character Sets registry, and `ASCII`, a common name of US-ASCII.
/// The names with `:` that IANA also registers are left out, as an encoding
/// declaration cannot have them (XML 1.0, production [81] EncName).
const IANA_NAMES: [(DocumentEncoding, &[&str]); 4] = [
    (
        DocumentEncoding::Ascii,
        &[
            "US-ASCII",
            "iso-ir-6",
            "ANSI_X3.4-1968",
            "ANSI_X3.4-1986",
            "ISO646-US",
            "us",
            "IBM367",
            "cp367",
            "csASCII",
            "ASCII",
        ],
    ),
    (
        DocumentEncoding::Latin1,
        &[
            "iso-ir-100",
            "ISO_8859-1",
            "ISO-8859-1",
            "latin1",
            "l1",
            "IBM819",
            "CP819",
            "csISOLatin1",
        ],
    ),
    (
        DocumentEncoding::Latin5,
        &[
            "iso-ir-148",
            "ISO_8859-9",
            "ISO-8859-9",
            "latin5",
            "l5",
            "csISOLatin5",
        ],
    ),
    (
        DocumentEncoding::Thai,
        &["TIS-620", "csTIS620", "ISO-8859-11"],
    ),
];

/// The family of encodings a document is in (XML 1.0 appendix F), as far as
/// it matters to read the XML declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    /// UTF-8 and other encodings with ASCII as a single byte per character.
    AsciiCompatible,
    /// Two bytes per character, little-endian.
    Utf16Le,
    /// Two bytes per character, big-endian.
    Utf16Be,
}

impl Family {
    /// The number of bytes of an ASCII character.
    fn width(self) -> usize {
        match self {
            Family::AsciiCompatible => 1,
            Family::Utf16Le | Family::Utf16Be => 2,
        }
    }
}

/// Decode `data` into a string, in the encoding [`encoding`] determines.
///
/// A byte order mark is removed. A byte sequence that is not valid in the
/// encoding is a fatal error (XML 1.0 section 4.3.3), reported with the
/// offsets of its bytes in `data`.
pub(crate) fn decode(data: &[u8]) -> Result<String, ParseError> {
    // the byte order mark, if any, is not part of the text
    let (encoding, start) = encoding(data)?;
    let char_for = match encoding {
        DocumentEncoding::Whatwg(encoding) => return decode_whatwg(data, start, encoding),
        DocumentEncoding::Ascii => ascii,
        DocumentEncoding::Latin1 => latin1,
        DocumentEncoding::Latin5 => latin5,
        DocumentEncoding::Thai => thai,
    };
    decode_single_byte(data, start, encoding.name(), char_for)
}

/// Decode `data` from the byte at `start` on, in an encoding with one byte
/// per character, the character that `char_for` gives; a byte that it gives
/// no character for is not valid.
fn decode_single_byte(
    data: &[u8],
    start: usize,
    name: &str,
    char_for: fn(u8) -> Option<char>,
) -> Result<String, ParseError> {
    let mut text = String::with_capacity(data.len() - start);
    for (position, byte) in data.iter().enumerate().skip(start) {
        match char_for(*byte) {
            Some(c) => text.push(c),
            None => {
                return Err(ParseError::InvalidByteSequence(
                    name.to_string(),
                    Span::new(position, position + 1),
                ))
            }
        }
    }
    Ok(text)
}

/// US-ASCII: the bytes below 0x80, each the character with the same number.
fn ascii(byte: u8) -> Option<char> {
    byte.is_ascii().then_some(char::from(byte))
}

/// ISO-8859-1: each byte is the character with the same number.
fn latin1(byte: u8) -> Option<char> {
    Some(char::from(byte))
}

/// ISO-8859-9: ISO-8859-1 with six Turkish letters in place of Icelandic
/// ones.
fn latin5(byte: u8) -> Option<char> {
    Some(match byte {
        0xD0 => '\u{11E}', // Ğ
        0xDD => '\u{130}', // İ
        0xDE => '\u{15E}', // Ş
        0xF0 => '\u{11F}', // ğ
        0xFD => '\u{131}', // ı
        0xFE => '\u{15F}', // ş
        _ => char::from(byte),
    })
}

/// TIS-620 and ISO-8859-11: ISO-8859-1 up to 0xA0, then the Thai characters
/// U+0E01 to U+0E3A and U+0E3F to U+0E5B in the same order; the bytes 0xDB to
/// 0xDE and 0xFC to 0xFF have no character.
///
/// TIS 620 itself has no character for 0xA0 either. IANA registers
/// ISO-8859-11 as a name of the same encoding, and ISO-8859-11 has NO-BREAK
/// SPACE there, as windows-874 has.
fn thai(byte: u8) -> Option<char> {
    match byte {
        0x00..=0xA0 => Some(char::from(byte)),
        0xA1..=0xDA | 0xDF..=0xFB => char::from_u32(u32::from(byte) - 0xA1 + 0x0E01),
        _ => None,
    }
}

/// Decode `data` from the byte at `start` on, in an encoding of the WHATWG
/// Encoding Standard.
fn decode_whatwg(
    data: &[u8],
    start: usize,
    encoding: &'static Encoding,
) -> Result<String, ParseError> {
    let mut read = start;
    let mut decoder = encoding.new_decoder_without_bom_handling();
    let capacity = decoder
        .max_utf8_buffer_length_without_replacement(data.len() - read)
        .unwrap_or(data.len());
    let mut text = String::with_capacity(capacity);
    loop {
        let (result, consumed) =
            decoder.decode_to_string_without_replacement(&data[read..], &mut text, true);
        read += consumed;
        match result {
            DecoderResult::InputEmpty => return Ok(text),
            DecoderResult::OutputFull => {
                // the capacity is the worst case, so this is not expected;
                // go on with twice the room
                let mut larger = String::with_capacity(text.capacity().max(16) * 2);
                larger.push_str(&text);
                text = larger;
            }
            DecoderResult::Malformed(length, consumed_after) => {
                // the sequence ends `consumed_after` bytes before what is read
                let end = read - usize::from(consumed_after);
                let start = end - usize::from(length);
                return Err(ParseError::InvalidByteSequence(
                    encoding.name().to_string(),
                    Span::new(start, end),
                ));
            }
        }
    }
}

/// Determine the encoding of the XML document in `data`, and the length of
/// its byte order mark, which is 0 if it has none.
///
/// This is an error if the encoding declaration names an encoding that is not
/// known, if the document is in UTF-32 or EBCDIC, which cannot be decoded, or
/// if the encoding declaration does not match the byte order mark or the way
/// the first characters are written in bytes (XML 1.0 section 4.3.3 and
/// appendix F). The spans of these errors are byte offsets in `data`.
pub(crate) fn encoding(data: &[u8]) -> Result<(DocumentEncoding, usize), ParseError> {
    use DocumentEncoding::Whatwg;
    if let Some(name) = undecodable_family(data) {
        return Err(ParseError::UnsupportedEncoding(
            name.to_string(),
            Span::new(0, 0),
        ));
    }
    let bom = Encoding::for_bom(data);
    let (family, start) = match bom {
        Some((encoding, length)) if encoding == UTF_8 => (Family::AsciiCompatible, length),
        Some((encoding, length)) if encoding == UTF_16LE => (Family::Utf16Le, length),
        Some((_, length)) => (Family::Utf16Be, length),
        // without a byte order mark, how `<?` is written shows the family
        None => match data {
            [0x3C, 0x00, 0x3F, 0x00, ..] => (Family::Utf16Le, 0),
            [0x00, 0x3C, 0x00, 0x3F, ..] => (Family::Utf16Be, 0),
            _ => (Family::AsciiCompatible, 0),
        },
    };
    let declared = match declared_encoding(&declaration(&data[start..], family)) {
        Some((name, span)) => {
            // the span counts the characters after the byte order mark
            let width = family.width();
            let span = Span::new(start + span.start * width, start + span.end * width);
            match for_name(&name) {
                Some(encoding) => Some((name, encoding, span)),
                None => return Err(ParseError::UnsupportedEncoding(name, span)),
            }
        }
        None => None,
    };
    // the encoding that the first bytes show, if they show one, and how
    // errors describe them
    let (shown, found) = match (bom, family) {
        (Some((encoding, _)), _) => (
            Some(encoding),
            format!("{} byte order mark", encoding.name()),
        ),
        (None, Family::Utf16Le) => (Some(UTF_16LE), "UTF-16LE without byte order mark".into()),
        (None, Family::Utf16Be) => (Some(UTF_16BE), "UTF-16BE without byte order mark".into()),
        (None, Family::AsciiCompatible) => (None, "ASCII-compatible encoding".into()),
    };
    match (shown, declared) {
        (None, None) => Ok((Whatwg(UTF_8), start)),
        // the declaration is written in an encoding that has ASCII characters
        // as single bytes, so it cannot be right about UTF-16
        (None, Some((name, encoding, span))) => {
            if encoding.is_utf16() {
                Err(ParseError::EncodingConflict(name, found, span))
            } else {
                Ok((encoding, start))
            }
        }
        // UTF-16 without a byte order mark has to be declared
        (Some(_), None) if bom.is_none() => Err(ParseError::EncodingConflict(
            String::new(),
            found,
            Span::new(0, 0),
        )),
        (Some(shown), None) => Ok((Whatwg(shown), start)),
        (Some(shown), Some((name, encoding, span))) => {
            if encoding == Whatwg(shown)
                || (is_utf16(shown) && allows_utf16(&name, encoding, shown))
            {
                Ok((Whatwg(shown), start))
            } else {
                Err(ParseError::EncodingConflict(name, found, span))
            }
        }
    }
}

/// The family of encodings `data` is in, if it is one that cannot be
/// decoded, as its byte order mark or how `<` is written shows (XML 1.0
/// appendix F).
fn undecodable_family(data: &[u8]) -> Option<&'static str> {
    match data {
        [0x00, 0x00, 0xFE, 0xFF, ..] | [0x00, 0x00, 0x00, 0x3C, ..] => Some("UTF-32BE"),
        [0xFF, 0xFE, 0x00, 0x00, ..] | [0x3C, 0x00, 0x00, 0x00, ..] => Some("UTF-32LE"),
        // UCS-4 in the unusual byte orders 2143 and 3412
        [0x00, 0x00, 0xFF, 0xFE, ..]
        | [0xFE, 0xFF, 0x00, 0x00, ..]
        | [0x00, 0x00, 0x3C, 0x00, ..]
        | [0x00, 0x3C, 0x00, 0x00, ..] => Some("UCS-4"),
        // `<?xm` in EBCDIC
        [0x4C, 0x6F, 0xA7, 0x94, ..] => Some("EBCDIC"),
        _ => None,
    }
}

fn is_utf16(encoding: &'static Encoding) -> bool {
    encoding == UTF_16LE || encoding == UTF_16BE
}

/// Whether an encoding declaration of `name`, which stands for `declared`,
/// allows a document in the UTF-16 encoding `shown`.
///
/// It has to name UTF-16. UTF-16LE and UTF-16BE name a byte order, as do the
/// other names that the WHATWG Encoding Standard has for UTF-16BE; UTF-16
/// and its other names, such as UCS-2 and csUnicode, leave the byte order to
/// the byte order mark or to the way the first characters are written, even
/// though the Encoding Standard makes them stand for UTF-16LE.
fn allows_utf16(name: &str, declared: DocumentEncoding, shown: &'static Encoding) -> bool {
    let byte_order = if declared == DocumentEncoding::Whatwg(UTF_16BE) {
        Some(UTF_16BE)
    } else if name.eq_ignore_ascii_case("UTF-16LE") {
        Some(UTF_16LE)
    } else {
        None
    };
    declared.is_utf16() && byte_order.is_none_or(|byte_order| byte_order == shown)
}

/// The encoding an encoding declaration names, if it is known and can be
/// decoded.
fn for_name(name: &str) -> Option<DocumentEncoding> {
    // XML 1.0 asks for names to be compared ignoring case
    let iana = IANA_NAMES
        .iter()
        .find(|(_, names)| names.iter().any(|n| n.eq_ignore_ascii_case(name)));
    if let Some((encoding, _)) = iana {
        return Some(*encoding);
    }
    // the replacement encoding decodes anything to a single U+FFFD
    Encoding::for_label(name.as_bytes())
        .filter(|encoding| *encoding != REPLACEMENT)
        .map(DocumentEncoding::Whatwg)
}

/// The characters at the start of `data`, read in `family`, up to and
/// including the first `>`, or up to the first character that is not ASCII.
///
/// This covers an XML declaration, which consists of ASCII characters and
/// contains `>` only at its end.
fn declaration(data: &[u8], family: Family) -> Vec<u8> {
    let mut chars = Vec::new();
    for chunk in data.chunks_exact(family.width()) {
        let c = match (family, chunk) {
            (Family::AsciiCompatible, [c]) => *c,
            (Family::Utf16Le, [c, 0]) => *c,
            (Family::Utf16Be, [0, c]) => *c,
            _ => break,
        };
        if !c.is_ascii() {
            break;
        }
        chars.push(c);
        if c == b'>' {
            break;
        }
    }
    chars
}

/// The name in the encoding declaration of the XML declaration that `text`
/// starts with, and its span, if there is one.
///
/// This reads the attributes of the XML declaration leniently; the parser
/// reports an XML declaration that is not well-formed.
fn declared_encoding(text: &[u8]) -> Option<(String, Span)> {
    let mut pos = b"<?xml".len();
    // `<?xml-stylesheet ...?>` and the like are processing instructions
    if !text.starts_with(b"<?xml") || !matches!(text.get(pos), Some(c) if is_space(*c)) {
        return None;
    }
    loop {
        pos = skip_spaces(text, pos);
        let name_start = pos;
        while matches!(text.get(pos), Some(c) if c.is_ascii_alphabetic()) {
            pos += 1;
        }
        let name = &text[name_start..pos];
        if name.is_empty() {
            // the end of the declaration, or it is not well-formed
            return None;
        }
        pos = skip_spaces(text, pos);
        if text.get(pos) != Some(&b'=') {
            return None;
        }
        pos = skip_spaces(text, pos + 1);
        let quote = *text.get(pos).filter(|c| **c == b'"' || **c == b'\'')?;
        let value_start = pos + 1;
        let value_end = value_start + text[value_start..].iter().position(|c| *c == quote)?;
        if name == b"encoding" {
            let value = String::from_utf8_lossy(&text[value_start..value_end]).into_owned();
            return Some((value, Span::new(value_start, value_end)));
        }
        pos = value_end + 1;
    }
}

fn skip_spaces(text: &[u8], mut pos: usize) -> usize {
    while matches!(text.get(pos), Some(c) if is_space(*c)) {
        pos += 1;
    }
    pos
}

/// `S` in the XML grammar.
fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\r' | b'\n')
}

#[cfg(test)]
mod tests {
    use encoding_rs::{WINDOWS_1254, WINDOWS_874};

    use super::DocumentEncoding::Whatwg;
    use super::*;

    fn utf16le(s: &str) -> Vec<u8> {
        s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect()
    }

    fn utf16be(s: &str) -> Vec<u8> {
        s.encode_utf16().flat_map(|u| u.to_be_bytes()).collect()
    }

    fn utf32le(s: &str) -> Vec<u8> {
        s.chars().flat_map(|c| (c as u32).to_le_bytes()).collect()
    }

    /// The encoding of `data`, which must be known.
    fn encoding_of(data: &[u8]) -> DocumentEncoding {
        encoding(data).unwrap().0
    }

    fn unsupported(data: &[u8]) -> (String, Span) {
        match encoding(data) {
            Err(ParseError::UnsupportedEncoding(name, span)) => (name, span),
            other => panic!("expected UnsupportedEncoding, got {:?}", other),
        }
    }

    fn conflict(data: &[u8]) -> (String, String, Span) {
        match encoding(data) {
            Err(ParseError::EncodingConflict(declared, found, span)) => (declared, found, span),
            other => panic!("expected EncodingConflict, got {:?}", other),
        }
    }

    #[test]
    fn test_utf8() {
        let data = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?><a/>";
        assert_eq!(encoding(data).unwrap(), (Whatwg(UTF_8), 0));
    }

    #[test]
    fn test_utf8_without_declaration() {
        let data = b"<a/>";
        assert_eq!(encoding(data).unwrap(), (Whatwg(UTF_8), 0));
    }

    #[test]
    fn test_us_ascii() {
        let data = b"<?xml version=\"1.0\" encoding=\"us-ascii\"?><a/>";
        let enc = encoding_of(data);
        // not windows-1252, which the WHATWG Encoding Standard makes of it
        assert_eq!(enc, DocumentEncoding::Ascii);
        assert_eq!(enc.name(), "US-ASCII");
    }

    #[test]
    fn test_iso8859_1() {
        let data = b"<?xml version=\"1.0\" encoding=\"iso-8859-1\"?><a/>";
        let enc = encoding_of(data);
        // not windows-1252, which the WHATWG Encoding Standard makes of it
        assert_eq!(enc, DocumentEncoding::Latin1);
        assert_eq!(enc.name(), "ISO-8859-1");
    }

    #[test]
    fn test_too_short_for_declaration() {
        // too short for a byte order mark or an XML declaration
        assert_eq!(encoding(b"").unwrap(), (Whatwg(UTF_8), 0));
        assert_eq!(encoding(b"<a>").unwrap(), (Whatwg(UTF_8), 0));
    }

    #[test]
    fn test_declaration_with_spaces() {
        let data = b"<?xml version = '1.0'\n  encoding = 'ISO-8859-2' ?><a/>";
        assert_eq!(encoding_of(data).name(), "ISO-8859-2");
    }

    #[test]
    fn test_declaration_without_encoding() {
        let data = b"<?xml version=\"1.0\" standalone=\"yes\"?><a/>";
        assert_eq!(encoding(data).unwrap(), (Whatwg(UTF_8), 0));
    }

    #[test]
    fn test_encoding_outside_declaration_ignored() {
        // only the XML declaration can declare the encoding
        assert_eq!(
            encoding_of(b"<a encoding=\"x-unknown\" charset=\"x-unknown\"/>"),
            Whatwg(UTF_8)
        );
        assert_eq!(encoding_of(b"<a>charset=x-unknown</a>"), Whatwg(UTF_8));
        assert_eq!(
            encoding_of(b"<?xml-stylesheet encoding=\"x-unknown\"?><a/>"),
            Whatwg(UTF_8)
        );
        assert_eq!(
            encoding_of(b"<?xml version=\"1.0\"?><a encoding=\"ISO-8859-2\"/>"),
            Whatwg(UTF_8)
        );
    }

    #[test]
    fn test_unknown_encoding() {
        let data = b"<?xml version=\"1.0\" encoding=\"x-unknown\"?><a/>";
        assert_eq!(
            unsupported(data),
            ("x-unknown".to_string(), Span::new(30, 39))
        );
    }

    #[test]
    fn test_empty_encoding() {
        let data = b"<?xml version=\"1.0\" encoding=\"\"?><a/>";
        assert_eq!(unsupported(data), (String::new(), Span::new(30, 30)));
    }

    #[test]
    fn test_replacement_encoding_unsupported() {
        // a WHATWG label for the replacement encoding
        let data = b"<?xml version=\"1.0\" encoding=\"ISO-2022-KR\"?><a/>";
        assert_eq!(unsupported(data).0, "ISO-2022-KR");
    }

    #[test]
    fn test_shift_jis_spelling() {
        let data = b"<?xml version=\"1.0\" encoding=\"Shift-JIS\"?><a/>";
        assert_eq!(encoding_of(data).name(), "Shift_JIS");
    }

    #[test]
    fn test_utf8_bom() {
        let data = b"\xEF\xBB\xBF<?xml version=\"1.0\"?><a/>";
        assert_eq!(encoding(data).unwrap(), (Whatwg(UTF_8), 3));
    }

    #[test]
    fn test_utf8_bom_with_utf8_declaration() {
        // a name of UTF-8, in any case
        for name in ["UTF-8", "utf-8", "utf8", "unicode-1-1-utf-8"] {
            let data = format!("\u{FEFF}<?xml version=\"1.0\" encoding=\"{}\"?><a/>", name);
            assert_eq!(
                encoding(data.as_bytes()).unwrap(),
                (Whatwg(UTF_8), 3),
                "{}",
                name
            );
        }
    }

    #[test]
    fn test_utf8_bom_with_other_declaration() {
        // the byte order mark and the declaration have to agree
        let data = b"\xEF\xBB\xBF<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><a/>";
        assert_eq!(
            conflict(data),
            (
                "ISO-8859-1".to_string(),
                "UTF-8 byte order mark".to_string(),
                Span::new(33, 43)
            )
        );
        let data = b"\xEF\xBB\xBF<?xml version=\"1.0\" encoding=\"UTF-16\"?><a/>";
        assert_eq!(conflict(data).0, "UTF-16");
        // US-ASCII is a subset of UTF-8, but a document with a UTF-8 byte
        // order mark is in UTF-8
        let data = b"\xEF\xBB\xBF<?xml version=\"1.0\" encoding=\"US-ASCII\"?><a/>";
        assert_eq!(conflict(data).0, "US-ASCII");
    }

    #[test]
    fn test_bom_with_unknown_encoding() {
        let data = b"\xEF\xBB\xBF<?xml version=\"1.0\" encoding=\"x-unknown\"?><a/>";
        // the span counts the byte order mark
        assert_eq!(
            unsupported(data),
            ("x-unknown".to_string(), Span::new(33, 42))
        );
    }

    #[test]
    fn test_utf16_bom() {
        let xml = "\u{FEFF}<?xml version=\"1.0\" encoding=\"UTF-16\"?><a/>";
        assert_eq!(encoding(&utf16le(xml)).unwrap(), (Whatwg(UTF_16LE), 2));
        assert_eq!(encoding(&utf16be(xml)).unwrap(), (Whatwg(UTF_16BE), 2));
        let xml = "\u{FEFF}<?xml version=\"1.0\"?><a/>";
        assert_eq!(encoding(&utf16le(xml)).unwrap(), (Whatwg(UTF_16LE), 2));
        assert_eq!(encoding(&utf16be(xml)).unwrap(), (Whatwg(UTF_16BE), 2));
    }

    #[test]
    fn test_utf16_bom_with_declaration() {
        // UTF-16 and its names that do not name a byte order go with either
        // byte order, UTF-16LE and UTF-16BE only with their own
        for (name, le, be) in [
            ("UTF-16", true, true),
            ("utf-16", true, true),
            ("UCS-2", true, true),
            ("csUnicode", true, true),
            ("UTF-16LE", true, false),
            ("utf-16le", true, false),
            ("UTF-16BE", false, true),
            ("utf-16be", false, true),
            ("UTF-8", false, false),
            ("ISO-8859-1", false, false),
            ("US-ASCII", false, false),
        ] {
            let xml = format!("\u{FEFF}<?xml version=\"1.0\" encoding=\"{}\"?><a/>", name);
            for (data, ok, encoding_name) in [
                (utf16le(&xml), le, "UTF-16LE"),
                (utf16be(&xml), be, "UTF-16BE"),
            ] {
                if ok {
                    assert_eq!(encoding_of(&data).name(), encoding_name, "{}", name);
                } else {
                    // the span is in bytes, and counts the byte order mark
                    assert_eq!(
                        conflict(&data),
                        (
                            name.to_string(),
                            format!("{} byte order mark", encoding_name),
                            Span::new(2 + 30 * 2, 2 + (30 + name.len()) * 2)
                        ),
                        "{} {}",
                        name,
                        encoding_name
                    );
                }
            }
        }
    }

    #[test]
    fn test_utf16_bom_with_unknown_encoding() {
        let xml = "\u{FEFF}<?xml version=\"1.0\" encoding=\"x-unknown\"?><a/>";
        // the span is in bytes, and counts the byte order mark
        assert_eq!(
            unsupported(&utf16le(xml)),
            ("x-unknown".to_string(), Span::new(62, 80))
        );
        assert_eq!(
            unsupported(&utf16be(xml)),
            ("x-unknown".to_string(), Span::new(62, 80))
        );
    }

    #[test]
    fn test_utf16_without_bom() {
        // the declaration shows the byte order
        let xml = "<?xml version=\"1.0\" encoding=\"UTF-16\"?><a/>";
        assert_eq!(encoding(&utf16le(xml)).unwrap(), (Whatwg(UTF_16LE), 0));
        assert_eq!(encoding(&utf16be(xml)).unwrap(), (Whatwg(UTF_16BE), 0));
        let xml = "<?xml version=\"1.0\" encoding=\"UTF-16LE\"?><a/>";
        assert_eq!(encoding(&utf16le(xml)).unwrap(), (Whatwg(UTF_16LE), 0));
        let xml = "<?xml version=\"1.0\" encoding=\"UTF-16BE\"?><a/>";
        assert_eq!(encoding(&utf16be(xml)).unwrap(), (Whatwg(UTF_16BE), 0));
    }

    #[test]
    fn test_utf16_without_bom_or_declaration() {
        // UTF-16 without a byte order mark has to be declared
        for xml in ["<?xml version=\"1.0\"?><a/>", "<?pi?><a/>"] {
            assert_eq!(
                conflict(&utf16le(xml)),
                (
                    String::new(),
                    "UTF-16LE without byte order mark".to_string(),
                    Span::new(0, 0)
                )
            );
            assert_eq!(
                conflict(&utf16be(xml)),
                (
                    String::new(),
                    "UTF-16BE without byte order mark".to_string(),
                    Span::new(0, 0)
                )
            );
        }
    }

    #[test]
    fn test_utf16_without_bom_with_other_declaration() {
        // a declaration of an encoding with single bytes for ASCII, or of the
        // other byte order
        let xml = "<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><a/>";
        assert_eq!(
            conflict(&utf16le(xml)),
            (
                "ISO-8859-1".to_string(),
                "UTF-16LE without byte order mark".to_string(),
                Span::new(60, 80)
            )
        );
        let xml = "<?xml version=\"1.0\" encoding=\"UTF-16LE\"?><a/>";
        assert_eq!(
            conflict(&utf16be(xml)),
            (
                "UTF-16LE".to_string(),
                "UTF-16BE without byte order mark".to_string(),
                Span::new(60, 76)
            )
        );
    }

    #[test]
    fn test_utf16_declared_in_ascii() {
        // a declaration that reads as one byte per character is not in UTF-16
        for name in ["UTF-16", "UTF-16LE", "utf-16be", "UCS-2"] {
            let data = format!("<?xml version=\"1.0\" encoding=\"{}\"?><a/>", name);
            assert_eq!(
                conflict(data.as_bytes()),
                (
                    name.to_string(),
                    "ASCII-compatible encoding".to_string(),
                    Span::new(30, 30 + name.len())
                )
            );
        }
    }

    #[test]
    fn test_utf32_unsupported() {
        assert_eq!(unsupported(&utf32le("\u{FEFF}<a/>")).0, "UTF-32LE");
        assert_eq!(unsupported(&utf32le("<a/>")).0, "UTF-32LE");
        assert_eq!(unsupported(b"\x00\x00\xFE\xFF\x00\x00\x00<").0, "UTF-32BE");
    }

    #[test]
    fn test_ebcdic_unsupported() {
        assert_eq!(unsupported(b"\x4C\x6F\xA7\x94\x93").0, "EBCDIC");
    }

    fn invalid(data: &[u8]) -> (String, Span) {
        match decode(data) {
            Err(ParseError::InvalidByteSequence(name, span)) => (name, span),
            other => panic!("expected InvalidByteSequence, got {:?}", other),
        }
    }

    #[test]
    fn test_decode() {
        assert_eq!(
            decode(b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><p>\xe9</p>").unwrap(),
            "<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><p>\u{e9}</p>"
        );
        assert_eq!(decode(b"<p>\xc3\xa9</p>").unwrap(), "<p>\u{e9}</p>");
        assert_eq!(decode(b"\xEF\xBB\xBF<p/>").unwrap(), "<p/>");
        assert_eq!(
            decode(&utf16be(
                "\u{FEFF}<?xml version=\"1.0\" encoding=\"UTF-16\"?><p/>"
            ))
            .unwrap(),
            "<?xml version=\"1.0\" encoding=\"UTF-16\"?><p/>"
        );
        assert_eq!(
            decode(&utf16le("\u{FEFF}<p>\u{10000}</p>")).unwrap(),
            "<p>\u{10000}</p>"
        );
    }

    #[test]
    fn test_decode_invalid_utf8() {
        // without a declaration the document is UTF-8, in which a lone 0xE9
        // is not valid
        assert_eq!(
            invalid(b"<p>\xe9</p>"),
            ("UTF-8".to_string(), Span::new(3, 4))
        );
        assert_eq!(
            invalid(b"<p>\xff</p>"),
            ("UTF-8".to_string(), Span::new(3, 4))
        );
        // a sequence cut short by a byte that cannot continue it
        assert_eq!(
            invalid(b"<p>\xe2\x82</p>"),
            ("UTF-8".to_string(), Span::new(3, 5))
        );
        // a sequence cut short by the end of the input
        assert_eq!(
            invalid(b"<p/>\xe2\x82"),
            ("UTF-8".to_string(), Span::new(4, 6))
        );
        // the UTF-8 form of a surrogate, which is not a character
        assert_eq!(
            invalid(b"<p>\xed\xa0\x80</p>"),
            ("UTF-8".to_string(), Span::new(3, 4))
        );
    }

    #[test]
    fn test_decode_invalid_after_bom() {
        // the offsets count the byte order mark
        assert_eq!(
            invalid(b"\xEF\xBB\xBF<p>\xff</p>"),
            ("UTF-8".to_string(), Span::new(6, 7))
        );
    }

    #[test]
    fn test_decode_invalid_utf16() {
        // a low surrogate without a high surrogate
        let mut data = utf16le("\u{FEFF}<p>");
        data.extend([0x00, 0xDC]);
        data.extend(utf16le("</p>"));
        assert_eq!(invalid(&data), ("UTF-16LE".to_string(), Span::new(8, 10)));
        // a high surrogate without a low surrogate
        let mut data = utf16be("\u{FEFF}<p>");
        data.extend([0xD8, 0x00]);
        data.extend(utf16be("</p>"));
        assert_eq!(invalid(&data), ("UTF-16BE".to_string(), Span::new(8, 10)));
        // an odd number of bytes
        let mut data = utf16le("\u{FEFF}<p/>");
        data.push(b'\n');
        assert_eq!(invalid(&data), ("UTF-16LE".to_string(), Span::new(10, 11)));
    }

    #[test]
    fn test_decode_invalid_declared_encoding() {
        // 0xA0 is not a character in Shift_JIS
        let data = b"<?xml version=\"1.0\" encoding=\"Shift_JIS\"?><p>\xa0</p>";
        assert_eq!(invalid(data), ("Shift_JIS".to_string(), Span::new(45, 46)));
    }

    /// The name of the encoding that an encoding declaration with `name`
    /// gives.
    fn declared(name: &str) -> &'static str {
        let data = format!("<?xml version=\"1.0\" encoding=\"{}\"?><a/>", name);
        encoding_of(data.as_bytes()).name()
    }

    #[test]
    fn test_latin1_names() {
        // the names IANA registers for ISO-8859-1, in any case
        for name in [
            "iso-ir-100",
            "ISO_8859-1",
            "ISO-8859-1",
            "latin1",
            "l1",
            "IBM819",
            "CP819",
            "csISOLatin1",
            "ISO-IR-100",
            "L1",
            "cp819",
            "iso_8859-1",
            "iso-8859-1",
            "Latin1",
            "ibm819",
            "CSISOLATIN1",
        ] {
            assert_eq!(declared(name), "ISO-8859-1", "{}", name);
        }
    }

    #[test]
    fn test_ascii_names() {
        // the names IANA registers for US-ASCII, and ASCII, in any case
        for name in [
            "US-ASCII",
            "iso-ir-6",
            "cp367",
            "ISO-IR-6",
            "CP367",
            "ANSI_X3.4-1968",
            "ANSI_X3.4-1986",
            "ISO646-US",
            "us",
            "IBM367",
            "csASCII",
            "ASCII",
            "us-ascii",
            "ansi_x3.4-1968",
            "ansi_x3.4-1986",
            "iso646-us",
            "US",
            "ibm367",
            "CSASCII",
            "Ascii",
        ] {
            assert_eq!(declared(name), "US-ASCII", "{}", name);
        }
    }

    #[test]
    fn test_latin5_names() {
        // the names IANA registers for ISO-8859-9, in any case
        for name in [
            "iso-ir-148",
            "l5",
            "ISO-IR-148",
            "L5",
            "ISO_8859-9",
            "ISO-8859-9",
            "latin5",
            "csISOLatin5",
            "iso-8859-9",
            "LATIN5",
            "csisolatin5",
        ] {
            assert_eq!(declared(name), "ISO-8859-9", "{}", name);
        }
    }

    #[test]
    fn test_thai_names() {
        // the names IANA registers for TIS-620, in any case
        for name in [
            "TIS-620",
            "csTIS620",
            "ISO-8859-11",
            "tis-620",
            "CSTIS620",
            "iso-8859-11",
        ] {
            assert_eq!(declared(name), "TIS-620", "{}", name);
        }
    }

    #[test]
    fn test_windows_1252_names() {
        // windows-1252 itself, and names the WHATWG Encoding Standard has for
        // it that IANA does not register for ISO-8859-1
        for name in [
            "windows-1252",
            "WINDOWS-1252",
            "cp1252",
            "x-cp1252",
            "iso8859-1",
            "iso88591",
        ] {
            assert_eq!(declared(name), "windows-1252", "{}", name);
        }
    }

    #[test]
    fn test_windows_1254_and_874_names() {
        // the same for windows-1254 and ISO-8859-9, and for windows-874 and
        // TIS-620
        for name in ["windows-1254", "iso8859-9", "iso88599"] {
            assert_eq!(declared(name), "windows-1254", "{}", name);
        }
        for name in ["windows-874", "iso8859-11", "iso885911"] {
            assert_eq!(declared(name), "windows-874", "{}", name);
        }
    }

    /// The character that `byte` decodes to after an encoding declaration
    /// with `name`, or the error.
    fn decode_byte(name: &str, byte: u8) -> Result<char, ParseError> {
        let mut data = format!("<?xml version=\"1.0\" encoding=\"{}\"?>", name).into_bytes();
        data.push(byte);
        decode(&data).map(|text| text.chars().last().unwrap())
    }

    /// The character that `byte` decodes to in an encoding of the WHATWG
    /// Encoding Standard, if it decodes to one.
    fn whatwg_char(encoding: &'static Encoding, byte: u8) -> Option<char> {
        encoding
            .decode_without_bom_handling_and_without_replacement(&[byte])
            .and_then(|text| text.chars().next())
    }

    #[test]
    fn test_decode_iso_8859_0x80_to_0x9f() {
        // in the ISO 8859 encodings the bytes 0x80 to 0x9F are the control
        // characters U+0080 to U+009F; windows-1252 and windows-1254 have
        // other characters for most of them, and windows-874 for some
        for name in ["ISO-8859-1", "ISO-8859-9", "ISO-8859-11", "TIS-620"] {
            for byte in 0x80..=0x9F {
                assert_eq!(
                    decode_byte(name, byte).unwrap(),
                    char::from(byte),
                    "{} {:#X}",
                    name,
                    byte
                );
            }
        }
        assert_eq!(decode_byte("ISO-8859-9", 0x80).unwrap(), '\u{80}');
        assert_eq!(decode_byte("latin1", 0x85).unwrap(), '\u{85}');
        assert_eq!(decode_byte("windows-1254", 0x80).unwrap(), '\u{20AC}');
        assert_eq!(decode_byte("windows-874", 0x85).unwrap(), '\u{2026}');
    }

    #[test]
    fn test_decode_latin5() {
        // below 0x80 ISO-8859-9 is US-ASCII, and above 0x9F it is the same as
        // windows-1254: ISO-8859-1 but for six Turkish letters
        for byte in 0..0x80 {
            assert_eq!(decode_byte("ISO-8859-9", byte).unwrap(), char::from(byte));
        }
        for byte in 0xA0..=0xFF {
            assert_eq!(
                decode_byte("ISO-8859-9", byte).ok(),
                whatwg_char(WINDOWS_1254, byte),
                "{:#X}",
                byte
            );
        }
        let turkish: Vec<u8> = (0xA0..=0xFF)
            .filter(|byte| decode_byte("ISO-8859-9", *byte).unwrap() != char::from(*byte))
            .collect();
        assert_eq!(turkish, [0xD0, 0xDD, 0xDE, 0xF0, 0xFD, 0xFE]);
        assert_eq!(decode_byte("ISO-8859-9", 0xD0).unwrap(), '\u{11E}');
        assert_eq!(decode_byte("ISO-8859-9", 0xFD).unwrap(), '\u{131}');
    }

    #[test]
    fn test_decode_thai() {
        // below 0x80 TIS-620 is US-ASCII, and above 0x9F it is the same as
        // windows-874, with NO-BREAK SPACE at 0xA0 and no character for the
        // same bytes
        for byte in 0..0x80 {
            assert_eq!(decode_byte("TIS-620", byte).unwrap(), char::from(byte));
        }
        for byte in 0xA0..=0xFF {
            assert_eq!(
                decode_byte("TIS-620", byte).ok(),
                whatwg_char(WINDOWS_874, byte),
                "{:#X}",
                byte
            );
        }
        assert_eq!(decode_byte("TIS-620", 0xA0).unwrap(), '\u{A0}');
        assert_eq!(decode_byte("TIS-620", 0xA1).unwrap(), '\u{E01}');
        assert_eq!(decode_byte("TIS-620", 0xDA).unwrap(), '\u{E3A}');
        assert_eq!(decode_byte("TIS-620", 0xDF).unwrap(), '\u{E3F}');
        assert_eq!(decode_byte("TIS-620", 0xFB).unwrap(), '\u{E5B}');
    }

    #[test]
    fn test_decode_invalid_thai() {
        // TIS-620 has no character for the bytes 0xDB to 0xDE and 0xFC to 0xFF
        let declaration = b"<?xml version=\"1.0\" encoding=\"ISO-8859-11\"?><p>";
        let at = declaration.len();
        for byte in [0xDB, 0xDC, 0xDD, 0xDE, 0xFC, 0xFD, 0xFE, 0xFF] {
            let mut data = declaration.to_vec();
            data.push(byte);
            data.extend(b"</p>");
            assert_eq!(
                invalid(&data),
                ("TIS-620".to_string(), Span::new(at, at + 1)),
                "{:#X}",
                byte
            );
        }
    }

    #[test]
    fn test_decode_latin1() {
        // each byte is the character with the same number, the bytes 0x80 to
        // 0x9F too, which are other characters in windows-1252
        let declaration = "<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?>";
        let mut data = declaration.as_bytes().to_vec();
        data.extend(0..=0xFF);
        let expected: String = declaration
            .chars()
            .chain((0..=0xFF).map(char::from))
            .collect();
        assert_eq!(decode(&data).unwrap(), expected);
    }

    #[test]
    fn test_decode_ascii() {
        let declaration = "<?xml version=\"1.0\" encoding=\"US-ASCII\"?>";
        let mut data = declaration.as_bytes().to_vec();
        data.extend(0..0x80);
        let expected: String = declaration
            .chars()
            .chain((0..0x80).map(char::from))
            .collect();
        assert_eq!(decode(&data).unwrap(), expected);
    }

    #[test]
    fn test_decode_invalid_ascii() {
        // US-ASCII has no characters for the bytes 0x80 to 0xFF
        let declaration = b"<?xml version=\"1.0\" encoding=\"US-ASCII\"?><p>";
        let at = declaration.len();
        for byte in 0x80..=0xFF {
            let mut data = declaration.to_vec();
            data.push(byte);
            data.extend(b"</p>");
            assert_eq!(
                invalid(&data),
                ("US-ASCII".to_string(), Span::new(at, at + 1)),
                "{:#X}",
                byte
            );
        }
        // the first such byte is reported
        assert_eq!(
            invalid(b"<?xml version=\"1.0\" encoding=\"us\"?><p>caf\xe9 \x80</p>"),
            ("US-ASCII".to_string(), Span::new(41, 42))
        );
    }

    #[test]
    fn test_decode_windows_1252() {
        // windows-1252 keeps its own characters for the bytes 0x80 to 0x9F
        assert_eq!(
            decode(b"<?xml version=\"1.0\" encoding=\"windows-1252\"?><p>\x80\x9f</p>").unwrap(),
            "<?xml version=\"1.0\" encoding=\"windows-1252\"?><p>\u{20AC}\u{178}</p>"
        );
    }

    #[test]
    fn test_decode_utf8_bom_with_single_byte_declaration() {
        // with a UTF-8 byte order mark the document is in UTF-8, so it cannot
        // declare US-ASCII or ISO-8859-1
        assert!(matches!(
            decode(b"\xEF\xBB\xBF<?xml version=\"1.0\" encoding=\"US-ASCII\"?><p>\xc3\xa9</p>"),
            Err(ParseError::EncodingConflict(..))
        ));
        assert!(matches!(
            decode(b"\xEF\xBB\xBF<?xml version=\"1.0\" encoding=\"latin1\"?><p>\xc3\xa9</p>"),
            Err(ParseError::EncodingConflict(..))
        ));
    }
}
