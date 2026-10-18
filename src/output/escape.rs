//! Escaping by scanning: a table of byte kinds, and escapers that copy the
//! runs between the bytes that need something and write the replacement for
//! those.
//!
//! The characters that may need a reference or a replacement are found by
//! their first byte in UTF-8: `0xC2` leads U+0080 to U+00BF (the control
//! characters U+0080 to U+009F, and U+00A0 for HTML), `0xE2` leads U+2000 to
//! U+2FFF (U+2028) and `0xEF` leads U+F000 to U+FFFF (U+FFFE and U+FFFF,
//! which XML 1.0 does not allow). The bytes that continue a character are
//! never flagged, so a flagged byte is always the first byte of a character.
use crate::entity::Escapes;

/// XML text, round trip: `&`, `<`, `>`, carriage return, U+007F and the lead
/// bytes `0xC2` and `0xE2`.
pub(crate) const TEXT: u16 = 1;
/// XML attribute value, round trip: `&`, `<`, `"`, `'`, tab, newline,
/// carriage return, U+007F and the lead bytes `0xC2` and `0xE2`.
pub(crate) const ATTRIBUTE: u16 = 1 << 1;
/// Bytes of characters that XML 1.0 may not allow: the control characters
/// other than tab, newline and carriage return, and the lead byte `0xEF`.
pub(crate) const NOT_CHAR: u16 = 1 << 2;
/// CDATA sections: `>`, carriage return, U+007F and the lead bytes `0xC2`
/// and `0xE2`.
pub(crate) const CDATA: u16 = 1 << 3;
/// HTML text in an HTML element: `&`, `<` and the lead byte `0xC2`.
pub(crate) const HTML_TEXT: u16 = 1 << 4;
/// HTML attribute value in no namespace: `&`, `'`, `"` and the lead byte
/// `0xC2`.
pub(crate) const HTML_ATTRIBUTE: u16 = 1 << 5;
/// Text without references: `&`, `<` and `>`.
pub(crate) const MARKUP_TEXT: u16 = 1 << 6;
/// Attribute value without references: `&`, `<`, `'` and `"`.
pub(crate) const MARKUP_ATTRIBUTE: u16 = 1 << 7;

const fn byte_kinds() -> [u16; 256] {
    let mut t = [0u16; 256];
    let mut b = 0;
    while b < 0x20 {
        if b != 0x09 && b != 0x0A && b != 0x0D {
            t[b] |= NOT_CHAR;
        }
        b += 1;
    }
    t[0xEF] |= NOT_CHAR;
    t[b'&' as usize] |=
        TEXT | ATTRIBUTE | HTML_TEXT | HTML_ATTRIBUTE | MARKUP_TEXT | MARKUP_ATTRIBUTE;
    t[b'<' as usize] |= TEXT | ATTRIBUTE | HTML_TEXT | MARKUP_TEXT | MARKUP_ATTRIBUTE;
    t[b'>' as usize] |= TEXT | CDATA | MARKUP_TEXT;
    t[b'"' as usize] |= ATTRIBUTE | HTML_ATTRIBUTE | MARKUP_ATTRIBUTE;
    t[b'\'' as usize] |= ATTRIBUTE | HTML_ATTRIBUTE | MARKUP_ATTRIBUTE;
    t[b'\t' as usize] |= ATTRIBUTE;
    t[b'\n' as usize] |= ATTRIBUTE;
    t[b'\r' as usize] |= TEXT | ATTRIBUTE | CDATA;
    t[0x7F] |= TEXT | ATTRIBUTE | CDATA;
    t[0xC2] |= TEXT | ATTRIBUTE | CDATA | HTML_TEXT | HTML_ATTRIBUTE;
    t[0xE2] |= TEXT | ATTRIBUTE | CDATA;
    t
}

static BYTE_KINDS: [u16; 256] = byte_kinds();

/// The first index at or after `from` whose byte is flagged for one of
/// `kinds`, or the length of `b`.
#[inline]
pub(crate) fn first_special(b: &[u8], from: usize, kinds: u16) -> usize {
    match b[from..]
        .iter()
        .position(|&c| BYTE_KINDS[c as usize] & kinds != 0)
    {
        Some(i) => from + i,
        None => b.len(),
    }
}

/// The first character of `s` that XML 1.0 does not allow (the `Char`
/// production), as `s.chars().find(|c| !c.is_xml_char())` finds it. If there
/// is none, the first index whose byte is flagged for `kinds`, or the length
/// of `s`.
#[inline]
pub(crate) fn first_not_xml_char(s: &str, kinds: u16) -> Result<usize, char> {
    let b = s.as_bytes();
    let found = first_special(b, 0, kinds | NOT_CHAR);
    let mut i = found;
    while i < b.len() {
        i = first_special(b, i, NOT_CHAR);
        if i == b.len() {
            break;
        }
        let c = b[i];
        if c < 0x20 {
            return Err(c as char);
        }
        // 0xEF leads U+F000 to U+FFFF; U+FFFE is EF BF BE and U+FFFF is
        // EF BF BF
        if b[i + 1] == 0xBF && (b[i + 2] == 0xBE || b[i + 2] == 0xBF) {
            return Err(if b[i + 2] == 0xBE {
                '\u{FFFE}'
            } else {
                '\u{FFFF}'
            });
        }
        i += 1;
    }
    if found < b.len() && BYTE_KINDS[b[found] as usize] & kinds == 0 {
        // the byte at `found` is an `0xEF` that leads a character that is
        // allowed
        return Ok(first_special(b, found, kinds));
    }
    Ok(found)
}

/// Write the character reference `&#xH;` for `c`, with the digits in upper
/// case and no leading zeros, as `write!(out, "&#x{:X};", c)` does.
#[inline]
pub(crate) fn push_char_ref(out: &mut String, c: u32) {
    let mut buf = [0u8; 11];
    let mut i = buf.len();
    i -= 1;
    buf[i] = b';';
    let mut v = c;
    loop {
        let d = (v & 0xF) as u8;
        i -= 1;
        buf[i] = if d < 10 { b'0' + d } else { b'A' + d - 10 };
        v >>= 4;
        if v == 0 {
            break;
        }
    }
    i -= 3;
    buf[i..i + 3].copy_from_slice(b"&#x");
    // the bytes are all ASCII
    out.push_str(std::str::from_utf8(&buf[i..]).unwrap());
}

/// The control character or the line separator at `b[i]` (U+007F to U+009F,
/// U+2028) that round trip escaping writes as a reference, and its length in
/// bytes. `b[i]` is `0x7F`, `0xC2` or `0xE2`.
#[inline]
fn control_or_line_end(b: &[u8], i: usize) -> Option<(u32, usize)> {
    match b[i] {
        0x7F => Some((0x7F, 1)),
        0xC2 => {
            // C2 80 to C2 9F are U+0080 to U+009F
            let c = b[i + 1];
            if c < 0xA0 {
                Some((u32::from(c), 2))
            } else {
                None
            }
        }
        0xE2 => {
            if b[i + 1] == 0x80 && b[i + 2] == 0xA8 {
                Some((0x2028, 3))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Escape text for XML, so that it reads back as it is. Without `refs`, only
/// `&`, `<` and `>` are replaced.
///
/// `start` is an index before which no byte is flagged.
fn push_text_with(out: &mut String, s: &str, start: usize, unescaped_gt: bool, refs: Option<bool>) {
    let b = s.as_bytes();
    let kinds = if refs.is_some() { TEXT } else { MARKUP_TEXT };
    let mut run = 0;
    let mut i = start;
    loop {
        i = first_special(b, i, kinds);
        if i == b.len() {
            break;
        }
        match b[i] {
            b'&' => {
                out.push_str(&s[run..i]);
                out.push_str("&amp;");
                i += 1;
                run = i;
            }
            b'<' => {
                out.push_str(&s[run..i]);
                out.push_str("&lt;");
                i += 1;
                run = i;
            }
            b'>' => {
                // with `unescaped_gt`, only the `>` of `]]>` is written as
                // a reference; a `]` is never replaced, so the two bytes in
                // front of the `>` in the text are the two characters in
                // front of it in the output
                if !unescaped_gt || (i >= 2 && b[i - 1] == b']' && b[i - 2] == b']') {
                    out.push_str(&s[run..i]);
                    out.push_str("&gt;");
                    run = i + 1;
                }
                i += 1;
            }
            b'\r' => match refs {
                Some(false) => {
                    out.push_str(&s[run..i]);
                    out.push_str("&#xD;");
                    i += 1;
                    run = i;
                }
                _ => i += 1,
            },
            _ => {
                if let Some((c, len)) = control_or_line_end(b, i) {
                    out.push_str(&s[run..i]);
                    push_char_ref(out, c);
                    i += len;
                    run = i;
                } else {
                    i += 1;
                }
            }
        }
    }
    out.push_str(&s[run..]);
}

/// Escape the text `s` for XML with round trip escaping: `&` and `<` are
/// replaced, `>` too (with `unescaped_gt` only after `]]`), carriage return
/// unless `unescaped_cr`, and U+007F to U+009F and U+2028 are written as
/// character references. `start` is an index before which no byte is
/// flagged for [`TEXT`].
pub(crate) fn push_text(
    out: &mut String,
    s: &str,
    start: usize,
    unescaped_gt: bool,
    unescaped_cr: bool,
) {
    push_text_with(out, s, start, unescaped_gt, Some(unescaped_cr))
}

/// Escape the text `s` for markup without character references: `&` and `<`
/// are replaced, and `>` (with `unescaped_gt` only after `]]`). `start` is an
/// index before which no byte is flagged for [`MARKUP_TEXT`].
pub(crate) fn push_markup_text(out: &mut String, s: &str, start: usize, unescaped_gt: bool) {
    push_text_with(out, s, start, unescaped_gt, None)
}

/// Escape the attribute value `s` for XML with round trip escaping: `&`, `<`,
/// `"` and `'` are replaced, tab, newline and carriage return and U+007F to
/// U+009F and U+2028 are written as character references. `start` is an index
/// before which no byte is flagged for [`ATTRIBUTE`].
pub(crate) fn push_attribute(out: &mut String, s: &str, start: usize) {
    let b = s.as_bytes();
    let mut run = 0;
    let mut i = start;
    loop {
        i = first_special(b, i, ATTRIBUTE);
        if i == b.len() {
            break;
        }
        let replacement = match b[i] {
            b'&' => "&amp;",
            b'<' => "&lt;",
            b'"' => "&quot;",
            b'\'' => "&apos;",
            b'\t' => "&#x9;",
            b'\n' => "&#xA;",
            b'\r' => "&#xD;",
            _ => {
                if let Some((c, len)) = control_or_line_end(b, i) {
                    out.push_str(&s[run..i]);
                    push_char_ref(out, c);
                    i += len;
                    run = i;
                } else {
                    i += 1;
                }
                continue;
            }
        };
        out.push_str(&s[run..i]);
        out.push_str(replacement);
        i += 1;
        run = i;
    }
    out.push_str(&s[run..]);
}

/// Escape the attribute value `s` for markup without character references:
/// `&`, `<`, `'` and `"` are replaced. `start` is an index before which no
/// byte is flagged for [`MARKUP_ATTRIBUTE`].
pub(crate) fn push_markup_attribute(out: &mut String, s: &str, start: usize) {
    push_replaced(out, s, start, MARKUP_ATTRIBUTE, |b, i| match b[i] {
        b'&' => Some(("&amp;", 1)),
        b'<' => Some(("&lt;", 1)),
        b'\'' => Some(("&apos;", 1)),
        b'"' => Some(("&quot;", 1)),
        _ => None,
    })
}

/// Escape the text `s` in an HTML element: `&`, `<` and U+00A0. `start` is an
/// index before which no byte is flagged for [`HTML_TEXT`].
pub(crate) fn push_html_text(out: &mut String, s: &str, start: usize) {
    push_replaced(out, s, start, HTML_TEXT, |b, i| match b[i] {
        b'&' => Some(("&amp;", 1)),
        b'<' => Some(("&lt;", 1)),
        0xC2 if b[i + 1] == 0xA0 => Some(("&nbsp;", 2)),
        _ => None,
    })
}

/// Escape the value `s` of an attribute in no namespace in HTML: `&`, `'`,
/// `"` and U+00A0. `start` is an index before which no byte is flagged for
/// [`HTML_ATTRIBUTE`].
pub(crate) fn push_html_attribute(out: &mut String, s: &str, start: usize) {
    push_replaced(out, s, start, HTML_ATTRIBUTE, |b, i| match b[i] {
        b'&' => Some(("&amp;", 1)),
        b'\'' => Some(("&apos;", 1)),
        b'"' => Some(("&quot;", 1)),
        0xC2 if b[i + 1] == 0xA0 => Some(("&nbsp;", 2)),
        _ => None,
    })
}

/// Copy `s`, with each flagged byte for which `replacement` gives a text and
/// the length of what it replaces replaced by that text.
#[inline]
fn push_replaced(
    out: &mut String,
    s: &str,
    start: usize,
    kinds: u16,
    replacement: impl Fn(&[u8], usize) -> Option<(&'static str, usize)>,
) {
    let b = s.as_bytes();
    let mut run = 0;
    let mut i = start;
    loop {
        i = first_special(b, i, kinds);
        if i == b.len() {
            break;
        }
        match replacement(b, i) {
            Some((text, len)) => {
                out.push_str(&s[run..i]);
                out.push_str(text);
                i += len;
                run = i;
            }
            None => i += 1,
        }
    }
    out.push_str(&s[run..]);
}

/// Write `s` in CDATA sections. With round trip escapes, carriage return
/// (unless `unescaped_cr`), U+007F to U+009F and U+2028 are character
/// references, which a CDATA section cannot hold, so the section ends before
/// one and a new one starts after it if more follows. A `>` after `]]` also
/// ends the section, before the `>`. An empty `s` is an empty section.
pub(crate) fn push_cdata(out: &mut String, s: &str, escapes: Escapes) {
    if s.is_empty() {
        out.push_str("<![CDATA[]]>");
        return;
    }
    let refs = match escapes {
        Escapes::RoundTrip { unescaped_cr } => Some(unescaped_cr),
        Escapes::MarkupOnly => None,
    };
    let b = s.as_bytes();
    // `seg` is the start of the characters since the last reference, `run`
    // the start of what is not copied yet, and `opened` whether a section is
    // open and has been written before `run`
    let mut seg = 0;
    let mut run = 0;
    let mut opened = false;
    let mut i = 0;
    loop {
        i = first_special(b, i, CDATA);
        if i == b.len() {
            break;
        }
        let c = b[i];
        if c == b'>' {
            // at least two `]` of this section in front of it
            if i >= seg + 2 && b[i - 1] == b']' && b[i - 2] == b']' {
                if !opened {
                    out.push_str("<![CDATA[");
                    opened = true;
                }
                out.push_str(&s[run..i]);
                out.push_str("]]><![CDATA[");
                run = i;
            }
            i += 1;
            continue;
        }
        let Some(unescaped_cr) = refs else {
            i += 1;
            continue;
        };
        let reference = if c == b'\r' {
            (!unescaped_cr).then_some((0xD, 1))
        } else {
            control_or_line_end(b, i)
        };
        let Some((code, len)) = reference else {
            i += 1;
            continue;
        };
        if i > seg {
            if !opened {
                out.push_str("<![CDATA[");
            }
            out.push_str(&s[run..i]);
            out.push_str("]]>");
        }
        push_char_ref(out, code);
        i += len;
        seg = i;
        run = i;
        opened = false;
    }
    if b.len() > seg {
        if !opened {
            out.push_str("<![CDATA[");
        }
        out.push_str(&s[run..]);
        out.push_str("]]>");
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use xmlparser::XmlCharExt;

    use super::super::html5_serializer::{old_serialize_attribute_html, old_serialize_text_html};
    use super::*;
    use crate::entity::{old_serialize_attribute, old_serialize_cdata, old_serialize_text};
    use crate::output::NoopNormalizer;

    const ROUND_TRIP: Escapes = Escapes::RoundTrip {
        unescaped_cr: false,
    };
    const ROUND_TRIP_CR: Escapes = Escapes::RoundTrip { unescaped_cr: true };

    const ALPHABET: &[char] = &[
        '&', '<', '>', '"', '\'', ']', '\r', '\t', '\n', '\u{7F}', '\u{80}', '\u{85}', '\u{9F}',
        '\u{A0}', '\u{BF}', '\u{2027}', '\u{2028}', '\u{2029}', '\u{FFFD}', '\u{FFFE}', '\u{FFFF}',
        '\u{1}', 'é', '😀', 'Ａ', 'a', 'x',
    ];

    fn strings() -> impl Strategy<Value = String> {
        proptest::collection::vec(proptest::sample::select(ALPHABET), 0..=40)
            .prop_map(|chars| chars.into_iter().collect())
    }

    fn text(s: &str, unescaped_gt: bool, escapes: Escapes) -> String {
        let (kinds, start) = (
            match escapes {
                Escapes::RoundTrip { .. } => TEXT,
                Escapes::MarkupOnly => MARKUP_TEXT,
            },
            0,
        );
        let start = first_special(s.as_bytes(), start, kinds);
        let mut out = String::new();
        match escapes {
            Escapes::RoundTrip { unescaped_cr } => {
                push_text(&mut out, s, start, unescaped_gt, unescaped_cr)
            }
            Escapes::MarkupOnly => push_markup_text(&mut out, s, start, unescaped_gt),
        }
        out
    }

    fn check_all(s: &str) {
        for unescaped_gt in [false, true] {
            for escapes in [ROUND_TRIP, ROUND_TRIP_CR, Escapes::MarkupOnly] {
                assert_eq!(
                    text(s, unescaped_gt, escapes),
                    old_serialize_text(s.into(), &NoopNormalizer, unescaped_gt, escapes),
                    "text {s:?} {unescaped_gt} {escapes:?}"
                );
            }
        }
        for escapes in [ROUND_TRIP, Escapes::MarkupOnly] {
            let start = first_special(
                s.as_bytes(),
                0,
                match escapes {
                    Escapes::RoundTrip { .. } => ATTRIBUTE,
                    Escapes::MarkupOnly => MARKUP_ATTRIBUTE,
                },
            );
            let mut out = String::new();
            match escapes {
                Escapes::RoundTrip { .. } => push_attribute(&mut out, s, start),
                Escapes::MarkupOnly => push_markup_attribute(&mut out, s, start),
            }
            assert_eq!(
                out,
                old_serialize_attribute(s.into(), &NoopNormalizer, escapes),
                "attribute {s:?} {escapes:?}"
            );
        }
        for escapes in [ROUND_TRIP, ROUND_TRIP_CR, Escapes::MarkupOnly] {
            let mut out = String::new();
            push_cdata(&mut out, s, escapes);
            assert_eq!(
                out,
                old_serialize_cdata(s.into(), &NoopNormalizer, escapes),
                "cdata {s:?} {escapes:?}"
            );
        }
        let mut out = String::new();
        push_html_text(&mut out, s, first_special(s.as_bytes(), 0, HTML_TEXT));
        assert_eq!(
            out,
            old_serialize_text_html(s.into(), &NoopNormalizer),
            "html text {s:?}"
        );
        let mut out = String::new();
        push_html_attribute(&mut out, s, first_special(s.as_bytes(), 0, HTML_ATTRIBUTE));
        assert_eq!(
            out,
            old_serialize_attribute_html(s.into(), &NoopNormalizer),
            "html attribute {s:?}"
        );
        assert_eq!(
            first_not_xml_char(s, 0).err(),
            s.chars().find(|c| !c.is_xml_char()),
            "check {s:?}"
        );
        for kinds in [TEXT, ATTRIBUTE, HTML_TEXT, 0] {
            if let Ok(index) = first_not_xml_char(s, kinds) {
                assert_eq!(index, first_special(s.as_bytes(), 0, kinds), "{s:?}");
            }
        }
    }

    proptest! {
        #[test]
        fn escapers_equal_the_old_bodies(s in strings()) {
            check_all(&s);
        }

        #[test]
        fn escapers_equal_the_old_bodies_after_a_start(s in strings(), t in strings()) {
            // a start after a prefix that has nothing to escape
            let prefix: String = t.chars().filter(|c| matches!(c, 'a' | 'x' | 'é' | 'Ａ' | '😀')).collect();
            let full = format!("{prefix}{s}");
            let mut out = String::new();
            push_text(&mut out, &full, prefix.len(), false, false);
            prop_assert_eq!(
                out,
                old_serialize_text(full.as_str().into(), &NoopNormalizer, false, ROUND_TRIP)
            );
            let mut out = String::new();
            push_attribute(&mut out, &full, prefix.len());
            prop_assert_eq!(
                out,
                old_serialize_attribute(full.as_str().into(), &NoopNormalizer, ROUND_TRIP)
            );
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100_000))]

        #[test]
        #[ignore = "soak: run with --ignored"]
        fn escapers_equal_the_old_bodies_soak(s in strings()) {
            check_all(&s);
        }
    }

    fn cdata(s: &str, escapes: Escapes) -> String {
        let mut out = String::new();
        push_cdata(&mut out, s, escapes);
        out
    }

    #[test]
    fn text_unescaped_gt_after_square_brackets() {
        assert_eq!(text("]]&>", true, ROUND_TRIP), "]]&amp;>");
        assert_eq!(text("a]]>b", true, ROUND_TRIP), "a]]&gt;b");
        assert_eq!(text("a]>b>", true, ROUND_TRIP), "a]>b>");
        assert_eq!(text("a]]>b", false, ROUND_TRIP), "a]]&gt;b");
        assert_eq!(text("a]]>b", true, Escapes::MarkupOnly), "a]]&gt;b");
        assert_eq!(text("a>b", true, Escapes::MarkupOnly), "a>b");
    }

    #[test]
    fn cdata_splits_the_end_delimiter() {
        assert_eq!(cdata("a]]>b", ROUND_TRIP), "<![CDATA[a]]]]><![CDATA[>b]]>");
        assert_eq!(
            cdata("a]]>b", Escapes::MarkupOnly),
            "<![CDATA[a]]]]><![CDATA[>b]]>"
        );
        assert_eq!(cdata("]]>", ROUND_TRIP), "<![CDATA[]]]]><![CDATA[>]]>");
    }

    #[test]
    fn cdata_references() {
        assert_eq!(cdata("\r x", ROUND_TRIP), "&#xD;<![CDATA[ x]]>");
        assert_eq!(cdata("a\r", ROUND_TRIP), "<![CDATA[a]]>&#xD;");
        assert_eq!(cdata("\r\r", ROUND_TRIP), "&#xD;&#xD;");
        assert_eq!(
            cdata("a\rb\u{85}c", ROUND_TRIP),
            "<![CDATA[a]]>&#xD;<![CDATA[b]]>&#x85;<![CDATA[c]]>"
        );
        assert_eq!(cdata("\r", ROUND_TRIP_CR), "<![CDATA[\r]]>");
        assert_eq!(cdata("\r", Escapes::MarkupOnly), "<![CDATA[\r]]>");
        // the count of square brackets restarts after a reference
        assert_eq!(
            cdata("]]\r>", ROUND_TRIP),
            "<![CDATA[]]]]>&#xD;<![CDATA[>]]>"
        );
    }

    #[test]
    fn cdata_empty() {
        assert_eq!(cdata("", ROUND_TRIP), "<![CDATA[]]>");
        assert_eq!(cdata("", Escapes::MarkupOnly), "<![CDATA[]]>");
    }

    #[test]
    fn not_xml_chars() {
        assert_eq!(first_not_xml_char("a\u{FFFE}b", 0), Err('\u{FFFE}'));
        assert_eq!(first_not_xml_char("a\u{FFFF}b", 0), Err('\u{FFFF}'));
        assert_eq!(first_not_xml_char("a\u{FFFD}\u{FEFF}b", 0), Ok(8));
        assert_eq!(first_not_xml_char("a\u{FFFD}\u{FEFF}b", TEXT), Ok(8));
        assert_eq!(first_not_xml_char("a\u{1}", 0), Err('\u{1}'));
        assert_eq!(first_not_xml_char("\u{FEFF}a&", TEXT), Ok(4));
        assert_eq!(first_not_xml_char("a\t b\n c\r d", ATTRIBUTE), Ok(1));
        assert_eq!(first_not_xml_char("é\u{0}", TEXT), Err('\u{0}'));
        assert_eq!(first_not_xml_char("", TEXT), Ok(0));
    }

    #[test]
    fn html_nbsp() {
        let mut out = String::new();
        push_html_text(&mut out, "a\u{A0}b&c<d>e\u{80}", 0);
        assert_eq!(out, "a&nbsp;b&amp;c&lt;d>e\u{80}");
        let mut out = String::new();
        push_html_attribute(&mut out, "a\u{A0}b&c<d\"e'", 0);
        assert_eq!(out, "a&nbsp;b&amp;c<d&quot;e&apos;");
    }

    #[test]
    fn char_ref() {
        for c in [0u32, 9, 0xD, 0x7F, 0x85, 0x2028, 0x10FFFF] {
            let mut out = String::new();
            push_char_ref(&mut out, c);
            assert_eq!(out, format!("&#x{:X};", c));
        }
    }
}
