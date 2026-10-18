use std::borrow::Cow;
#[cfg(test)]
use std::fmt::Write;

use xmlparser::XmlCharExt;

use crate::error::ParseError;
use crate::output::escape;
use crate::output::Normalizer;
use crate::Span;

/// Which characters other than markup characters are written as character
/// references in text and attribute values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Escapes {
    /// None.
    MarkupOnly,
    /// Those that would not survive parsing the output again, as the XML
    /// output method of Serialization 3.1
    /// (<https://www.w3.org/TR/xslt-xquery-serialization-31/>, section 5)
    /// requires: in text carriage return, NEL and LINE SEPARATOR, in
    /// attribute values also tab and newline, and in both the control
    /// characters #x7F to #x9F.
    ///
    /// The control characters #x1 to #x1F other than tab, newline and
    /// carriage return cannot appear in XML 1.0, not even as character
    /// references, so they are left as they are here. Writing XML output
    /// with them is an error, which the XML serializer reports before it
    /// writes the value (see `XmlSerializer::serialize_node`).
    RoundTrip {
        /// Write a carriage return in text as is.
        unescaped_cr: bool,
    },
}

#[cfg(test)]
impl Escapes {
    fn in_text(self, c: char) -> bool {
        match self {
            Escapes::MarkupOnly => false,
            Escapes::RoundTrip { unescaped_cr } => {
                (c == '\r' && !unescaped_cr) || is_xml11_line_end_or_control(c)
            }
        }
    }

    fn in_attribute(self, c: char) -> bool {
        match self {
            Escapes::MarkupOnly => false,
            Escapes::RoundTrip { .. } => {
                matches!(c, '\t' | '\n' | '\r') || is_xml11_line_end_or_control(c)
            }
        }
    }
}

/// NEL (#x85) and LINE SEPARATOR (#x2028) are line ends to an XML 1.1 parser,
/// and XML 1.1 requires the control characters #x7F to #x9F to be escaped.
/// The serialization specification requires escaping them whatever the XML
/// version, as an XML 1.1 document can include an XML 1.0 entity.
#[cfg(test)]
fn is_xml11_line_end_or_control(c: char) -> bool {
    matches!(c, '\u{7F}'..='\u{9F}' | '\u{2028}')
}

#[cfg(test)]
fn push_char_ref(result: &mut String, c: char) {
    // writing to a string cannot fail
    let _ = write!(result, "&#x{:X};", u32::from(c));
}

/// [`parse_text_in`] with a buffer of its own.
#[cfg(test)]
pub(crate) fn parse_text(content: Cow<str>, base_position: usize) -> Result<Cow<str>, ParseError> {
    parse_text_in(content, base_position, &mut String::new())
}

/// The content of a text, with its references expanded and its line ends
/// normalized (see [`parse_content`]). A text that changes is built in
/// `buffer`, which the caller keeps from one text to the next.
pub(crate) fn parse_text_in<'a>(
    content: Cow<'a, str>,
    base_position: usize,
    buffer: &mut String,
) -> Result<Cow<'a, str>, ParseError> {
    parse_content(content, false, base_position, buffer)
}

/// Normalize line ends in content without references, such as a CDATA
/// section, a comment or a processing instruction, as XML 1.0 requires
/// (<https://www.w3.org/TR/xml/#sec-line-ends>): a carriage return followed
/// by a newline, and a carriage return that is not, become a newline.
pub(crate) fn normalize_line_ends(content: &str) -> Cow<'_, str> {
    if content.contains('\r') {
        Cow::Owned(content.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        Cow::Borrowed(content)
    }
}

/// [`parse_attribute_in`] with a buffer of its own.
#[cfg(test)]
pub(crate) fn parse_attribute(
    content: Cow<str>,
    base_position: usize,
) -> Result<Cow<str>, ParseError> {
    parse_attribute_in(content, base_position, &mut String::new())
}

/// The value of an attribute, with its references expanded and its line
/// ends and white space normalized (see [`parse_content`]). A value that
/// changes is built in `buffer`, which the caller keeps from one value to the
/// next.
pub(crate) fn parse_attribute_in<'a>(
    content: Cow<'a, str>,
    base_position: usize,
    buffer: &mut String,
) -> Result<Cow<'a, str>, ParseError> {
    parse_content(content, true, base_position, buffer)
}

/// `content` with its references expanded and its line ends normalized, and
/// in an attribute value (`attribute`) its white space normalized
/// (<https://www.w3.org/TR/xml/#AVNormalize>), or `content` itself if that
/// changes nothing. `base_position` is the byte index of `content` in the
/// text, for the errors.
///
/// Only a reference (`&`), a carriage return, and in an attribute value a tab
/// or a newline, change the content, or make it an error, so without them
/// it is returned as it is, without going through it character by
/// character as [`parse_content_with_changes`] does, which builds the value
/// in `buffer`.
fn parse_content<'a>(
    content: Cow<'a, str>,
    attribute: bool,
    base_position: usize,
    buffer: &mut String,
) -> Result<Cow<'a, str>, ParseError> {
    // these characters are ASCII, and no byte of another character in
    // UTF-8 is, so the bytes tell
    let changes = if attribute {
        content
            .bytes()
            .any(|b| matches!(b, b'&' | b'\r' | b'\t' | b'\n'))
    } else {
        content.bytes().any(|b| matches!(b, b'&' | b'\r'))
    };
    if !changes {
        return Ok(content);
    }
    parse_content_with_changes(content, attribute, base_position, buffer)
}

/// [`parse_content`] for content that may change: it goes through the
/// content character by character, and builds the value in `buffer`, of
/// which it returns a copy of the length of the value. It returns `content`
/// itself if nothing changes after all.
fn parse_content_with_changes<'a>(
    content: Cow<'a, str>,
    attribute: bool,
    base_position: usize,
    buffer: &mut String,
) -> Result<Cow<'a, str>, ParseError> {
    // the buffer grows to the longest value, and keeps its room for the
    // values after it
    let result = buffer;
    result.clear();
    let mut chars = content.char_indices().peekable();
    let mut change = false;
    while let Some((position, c)) = chars.next() {
        // https://www.w3.org/TR/xml/#sec-line-ends
        if c == '\r' {
            if let Some((_, peeked)) = chars.peek() {
                if peeked == &'\n' {
                    // consume next char
                    chars.next();
                }
            }
            if !attribute {
                result.push('\n');
            } else {
                // https://www.w3.org/TR/xml/#AVNormalize
                result.push(' ');
            }
            change = true;
        } else if c == '&' {
            let mut entity = String::new();
            let mut is_complete = false;
            let mut end_position = 0;
            for (p, c) in chars.by_ref() {
                if c == ';' {
                    is_complete = true;
                    end_position = p + 1;
                    break;
                }
                entity.push(c);
            }
            if !is_complete {
                return Err(ParseError::UnclosedEntity(entity, base_position + position));
            }
            change = true;

            if let Some(entity) = entity.strip_prefix('#') {
                let invalid = || {
                    ParseError::InvalidEntity(
                        entity.to_string(),
                        Span::new(base_position + position, base_position + end_position),
                    )
                };
                // only digits, decimal or after `x` hexadecimal (production
                // `CharRef`, https://www.w3.org/TR/xml/#sec-references);
                // Rust's integer parsing would also take a sign
                let (digits, radix) = match entity.strip_prefix('x') {
                    Some(digits) => (digits, 16),
                    None => (entity, 10),
                };
                if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
                    return Err(invalid());
                }
                // the reference must be to a character that XML allows
                // (https://www.w3.org/TR/xml/#wf-Legalchar), which leaves
                // out most control characters, #xFFFE and #xFFFF as well as
                // surrogates and numbers that are not characters at all
                let c = u32::from_str_radix(digits, radix)
                    .ok()
                    .and_then(std::char::from_u32)
                    .filter(|c| c.is_xml_char())
                    .ok_or_else(invalid)?;
                result.push(c);
            } else {
                match entity.as_str() {
                    "amp" => result.push('&'),
                    "apos" => result.push('\''),
                    "gt" => result.push('>'),
                    "lt" => result.push('<'),
                    "quot" => result.push('"'),
                    _ => {
                        return Err(ParseError::InvalidEntity(
                            entity,
                            Span::new(base_position + position, base_position + end_position),
                        ))
                    }
                }
            }
        } else if attribute && (c == '\t' || c == '\n') {
            // https://www.w3.org/TR/xml/#AVNormalize
            // \r and \r\n already handled earlier
            result.push(' ');
            change = true;
        } else {
            result.push(c);
        }
    }

    if !change {
        Ok(content)
    } else {
        // the tree keeps the string, which takes no more room than the
        // value then: a string that gave back the room a reference or a line
        // end did not take, through `shrink_to_fit`, could keep all of the
        // memory that the allocator gave it for the length of the content
        Ok(Cow::Owned(result.as_str().to_owned()))
    }
}

pub(crate) fn serialize_text<'a, N: Normalizer>(
    content: Cow<'a, str>,
    normalizer: &N,
    unescaped_gt: bool,
    escapes: Escapes,
) -> Cow<'a, str> {
    let normalized_content = normalizer.normalize(content);
    let kinds = match escapes {
        Escapes::RoundTrip { .. } => escape::TEXT,
        Escapes::MarkupOnly => escape::MARKUP_TEXT,
    };
    let bytes = normalized_content.as_bytes();
    let start = escape::first_special(bytes, 0, kinds);
    if start == bytes.len() {
        return normalized_content;
    }
    let mut result = String::with_capacity(bytes.len() + bytes.len() / 8 + 16);
    match escapes {
        Escapes::RoundTrip { unescaped_cr } => escape::push_text(
            &mut result,
            &normalized_content,
            start,
            unescaped_gt,
            unescaped_cr,
        ),
        Escapes::MarkupOnly => {
            escape::push_markup_text(&mut result, &normalized_content, start, unescaped_gt)
        }
    }
    result.into()
}

pub(crate) fn serialize_cdata<'a, N: Normalizer>(
    content: Cow<'a, str>,
    normalizer: &N,
    escapes: Escapes,
) -> Cow<'a, str> {
    let normalized_content = normalizer.normalize(content);
    let mut result = String::with_capacity(normalized_content.len() + 16);
    escape::push_cdata(&mut result, &normalized_content, escapes);
    result.into()
}

pub(crate) fn serialize_attribute<'a, N: Normalizer>(
    content: Cow<'a, str>,
    normalizer: &N,
    escapes: Escapes,
) -> Cow<'a, str> {
    let normalized_content = normalizer.normalize(content);
    let kinds = match escapes {
        Escapes::RoundTrip { .. } => escape::ATTRIBUTE,
        Escapes::MarkupOnly => escape::MARKUP_ATTRIBUTE,
    };
    let bytes = normalized_content.as_bytes();
    let start = escape::first_special(bytes, 0, kinds);
    if start == bytes.len() {
        return normalized_content;
    }
    let mut result = String::with_capacity(bytes.len() + bytes.len() / 8 + 16);
    match escapes {
        Escapes::RoundTrip { .. } => {
            escape::push_attribute(&mut result, &normalized_content, start)
        }
        Escapes::MarkupOnly => {
            escape::push_markup_attribute(&mut result, &normalized_content, start)
        }
    }
    result.into()
}

/// The escaping that [`serialize_text`] and the others replaced, character by
/// character, as the oracle of their tests.
#[cfg(test)]
pub(crate) fn old_serialize_text<'a, N: Normalizer>(
    content: Cow<'a, str>,
    normalizer: &N,
    unescaped_gt: bool,
    escapes: Escapes,
) -> Cow<'a, str> {
    let mut result = String::new();
    let mut change = false;
    // if we had normalized_iter on the trait we avoid this string allocation
    let normalized_content = normalizer.normalize(content);

    for c in normalized_content.chars() {
        match c {
            '&' => {
                change = true;
                result.push_str("&amp;")
            }
            '<' => {
                change = true;
                result.push_str("&lt;")
            }
            '>' if !unescaped_gt => {
                change = true;
                result.push_str("&gt;")
            }
            '>' if unescaped_gt => {
                change = true;
                // take last two characters added to result
                let mut last_two = result.chars().rev().take(2);
                let last = last_two.next();
                if let Some(']') = last {
                    let last = last_two.next();
                    if let Some(']') = last {
                        // we are in the special ]]> sequence, so escape it
                        result.push_str("&gt;");
                        continue;
                    }
                }
                result.push('>');
            }
            c if escapes.in_text(c) => {
                change = true;
                push_char_ref(&mut result, c);
            }
            _ => result.push(c),
        }
    }

    if !change {
        normalized_content
    } else {
        result.into()
    }
}

#[cfg(test)]
pub(crate) fn old_serialize_cdata<'a, N: Normalizer>(
    content: Cow<'a, str>,
    normalizer: &N,
    escapes: Escapes,
) -> Cow<'a, str> {
    let mut result = String::new();
    let mut in_section = false;
    // the number of `]` that directly precede the current character in the
    // open CDATA section
    let mut square_brackets = 0;
    let normalized_content = normalizer.normalize(content);
    for c in normalized_content.chars() {
        if escapes.in_text(c) {
            // a CDATA section cannot contain a character reference, so the
            // section ends before it and a new one starts after it if needed
            if in_section {
                result.push_str("]]>");
                in_section = false;
            }
            push_char_ref(&mut result, c);
            square_brackets = 0;
            continue;
        }
        if !in_section {
            result.push_str("<![CDATA[");
            in_section = true;
        } else if c == '>' && square_brackets >= 2 {
            // a CDATA section cannot contain `]]>`, so we end it between `]]`
            // and `>` and start a new one
            result.push_str("]]><![CDATA[");
        }
        result.push(c);
        square_brackets = if c == ']' { square_brackets + 1 } else { 0 };
    }
    if in_section {
        result.push_str("]]>");
    } else if result.is_empty() {
        // empty text is an empty CDATA section
        result.push_str("<![CDATA[]]>");
    }
    result.into()
}

#[cfg(test)]
pub(crate) fn old_serialize_attribute<'a, N: Normalizer>(
    content: Cow<'a, str>,
    normalizer: &N,
    escapes: Escapes,
) -> Cow<'a, str> {
    let mut result = String::new();
    let mut change = false;
    let normalized_content = normalizer.normalize(content);
    for c in normalized_content.chars() {
        match c {
            '&' => {
                change = true;
                result.push_str("&amp;")
            }
            '<' => {
                change = true;
                result.push_str("&lt;")
            }
            '\'' => {
                change = true;
                result.push_str("&apos;")
            }
            '"' => {
                change = true;
                result.push_str("&quot;")
            }
            c if escapes.in_attribute(c) => {
                change = true;
                push_char_ref(&mut result, c);
            }
            _ => result.push(c),
        }
    }

    if !change {
        normalized_content
    } else {
        result.into()
    }
}

#[cfg(test)]
mod tests {

    use proptest::prelude::*;

    use crate::output::NoopNormalizer;

    use super::*;

    const XML: Escapes = Escapes::RoundTrip {
        unescaped_cr: false,
    };

    #[test]
    fn test_parse() {
        let text = "A &amp; B";
        assert_eq!(parse_text(text.into(), 0).unwrap(), "A & B");
    }

    #[test]
    fn test_parse_multiple() {
        let text = "&amp;&apos;&gt;&lt;&quot;";
        assert_eq!(parse_text(text.into(), 0).unwrap(), "&'><\"");
    }

    #[test]
    fn test_parse_unknown_entity() {
        let text = "&unknown;";
        let err = parse_text(text.into(), 0);
        if let Err(ParseError::InvalidEntity(entity, span)) = err {
            assert_eq!(entity, "unknown");
            assert_eq!(span, Span::new(0, 9));
        } else {
            unreachable!();
        }
    }

    #[test]
    fn test_parse_unfinished_entity() {
        let text = "&amp";
        let err = parse_text(text.into(), 0);
        if let Err(ParseError::UnclosedEntity(entity, position)) = err {
            assert_eq!(entity, "amp");
            assert_eq!(position, 0);
        } else {
            unreachable!();
        }
    }

    #[test]
    fn test_parse_no_entities() {
        let text = "hello";
        let result = parse_text(text.into(), 0).unwrap();
        // this is the same slice
        assert!(std::ptr::eq(text, result.as_ref()));
    }

    /// `parse_content` as it was before it returned content without
    /// references, line ends and, in attribute values, white space to
    /// normalize without going through it: the oracle of the tests below.
    fn parse_content_before(
        content: Cow<str>,
        attribute: bool,
        base_position: usize,
    ) -> Result<Cow<str>, ParseError> {
        let mut result = String::new();
        let mut chars = content.char_indices().peekable();
        let mut change = false;
        while let Some((position, c)) = chars.next() {
            if c == '\r' {
                if let Some((_, peeked)) = chars.peek() {
                    if peeked == &'\n' {
                        chars.next();
                    }
                }
                if !attribute {
                    result.push('\n');
                } else {
                    result.push(' ');
                }
                change = true;
            } else if c == '&' {
                let mut entity = String::new();
                let mut is_complete = false;
                let mut end_position = 0;
                for (p, c) in chars.by_ref() {
                    if c == ';' {
                        is_complete = true;
                        end_position = p + 1;
                        break;
                    }
                    entity.push(c);
                }
                if !is_complete {
                    return Err(ParseError::UnclosedEntity(entity, base_position + position));
                }
                change = true;

                if let Some(entity) = entity.strip_prefix('#') {
                    let invalid = || {
                        ParseError::InvalidEntity(
                            entity.to_string(),
                            Span::new(base_position + position, base_position + end_position),
                        )
                    };
                    let (digits, radix) = match entity.strip_prefix('x') {
                        Some(digits) => (digits, 16),
                        None => (entity, 10),
                    };
                    if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
                        return Err(invalid());
                    }
                    let c = u32::from_str_radix(digits, radix)
                        .ok()
                        .and_then(std::char::from_u32)
                        .filter(|c| c.is_xml_char())
                        .ok_or_else(invalid)?;
                    result.push(c);
                } else {
                    match entity.as_str() {
                        "amp" => result.push('&'),
                        "apos" => result.push('\''),
                        "gt" => result.push('>'),
                        "lt" => result.push('<'),
                        "quot" => result.push('"'),
                        _ => {
                            return Err(ParseError::InvalidEntity(
                                entity,
                                Span::new(base_position + position, base_position + end_position),
                            ))
                        }
                    }
                }
            } else if attribute && (c == '\t' || c == '\n') {
                result.push(' ');
                change = true;
            } else {
                result.push(c);
            }
        }

        if !change {
            Ok(content)
        } else {
            Ok(result.into())
        }
    }

    /// A result as text that compares the value, whether it is borrowed, and
    /// an error with its debug text, which has its position or span.
    fn described(result: Result<Cow<str>, ParseError>) -> String {
        match result {
            Ok(Cow::Borrowed(s)) => format!("borrowed {:?}", s),
            Ok(Cow::Owned(s)) => format!("owned {:?}", s),
            Err(e) => format!("error {:?}", e),
        }
    }

    /// Text made of pieces that `parse_content` treats in a particular way:
    /// references that are known, not known or not closed, and line ends
    /// and white space, with any other characters between them.
    fn content() -> impl Strategy<Value = String> {
        let piece = prop_oneof![
            4 => any::<char>().prop_map(String::from),
            1 => prop::sample::select(vec![
                "&amp;", "&lt;", "&gt;", "&apos;", "&quot;", "&#65;", "&#x41;", "&#x1F600;",
                "&#9;", "&#xD;", "&#0;", "&#x;", "&#;", "&#-1;", "&ampx;", "&;", "&", ";",
                "#", "x", "\r", "\n", "\r\n", "\n\r", "\t", " ", "a", "\u{E9}",
            ])
            .prop_map(String::from),
        ];
        prop::collection::vec(piece, 0..12).prop_map(|pieces| pieces.concat())
    }

    proptest! {
        #[test]
        fn parse_content_skips_content_that_does_not_change(
            content in content(),
            attribute in any::<bool>(),
            base_position in 0usize..1000,
        ) {
            // a buffer with something in it already, as the parser keeps it
            let mut buffer = String::from("left over &amp;");
            let fast = parse_content(content.as_str().into(), attribute, base_position, &mut buffer);
            // a borrowed result is the content itself
            if let Ok(Cow::Borrowed(s)) = &fast {
                prop_assert!(std::ptr::eq(*s, content.as_str()));
            }
            let fast = described(fast);
            let slow = described(parse_content_with_changes(
                content.as_str().into(),
                attribute,
                base_position,
                &mut buffer,
            ));
            let before = described(parse_content_before(
                content.as_str().into(),
                attribute,
                base_position,
            ));
            prop_assert_eq!(&fast, &before);
            prop_assert_eq!(&slow, &before);
        }
    }

    #[test]
    fn parse_content_gives_a_changed_value_no_more_room_than_it_takes() {
        // a copy of the value, made for its length, and not the buffer, which
        // the parser keeps from one value to the next: the memory that the
        // allocator gives a string does not shrink with `shrink_to_fit` when
        // the string keeps more than about half of it
        let mut buffer = String::with_capacity(100);
        for content in [
            "a &amp; b",
            "&lt;&lt;&lt;&lt;&lt;&lt;&lt;&lt;",
            "a\r\nb\rc",
            "&#x1F600;",
        ] {
            for attribute in [false, true] {
                let Ok(Cow::Owned(value)) =
                    parse_content(content.into(), attribute, 0, &mut buffer)
                else {
                    panic!("{:?} changes", content);
                };
                assert_eq!(value.capacity(), value.len(), "{:?}", content);
                assert_eq!(value, buffer);
                assert_ne!(value.as_ptr(), buffer.as_ptr());
            }
        }
        let Ok(Cow::Owned(value)) = parse_attribute("one\u{9}two".into(), 0) else {
            panic!("a tab changes an attribute value");
        };
        assert_eq!(value.capacity(), value.len());
    }

    #[test]
    fn parse_content_returns_content_without_changes_as_it_is() {
        for (content, text, attribute) in [
            ("", true, true),
            ("text \u{E9} \u{1F600} ;#x", true, true),
            // a tab and a newline change attribute values only
            ("a\u{9}b\u{A}c", true, false),
            ("a\r\nb", false, false),
            ("a&amp;b", false, false),
        ] {
            assert_eq!(
                matches!(parse_text(content.into(), 0), Ok(Cow::Borrowed(_))),
                text,
                "{:?}",
                content
            );
            assert_eq!(
                matches!(parse_attribute(content.into(), 0), Ok(Cow::Borrowed(_))),
                attribute,
                "{:?}",
                content
            );
        }
    }

    #[test]
    fn test_parse_newline_r() {
        let text = "A \r B";
        assert_eq!(parse_text(text.into(), 0).unwrap(), "A \n B");
    }

    #[test]
    fn test_parse_newline_rn() {
        let text = "A \r\n B";
        assert_eq!(parse_text(text.into(), 0).unwrap(), "A \n B");
    }

    #[test]
    fn test_normalize_line_ends() {
        assert_eq!(
            normalize_line_ends("a\u{D}\u{A}b\u{D}c\u{A}\u{D}d\u{D}\u{D}\u{A}e\u{D}"),
            "a\u{A}b\u{A}c\u{A}\u{A}d\u{A}\u{A}e\u{A}"
        );
        // `&` is not a reference here
        assert_eq!(normalize_line_ends("&amp;\u{D}"), "&amp;\u{A}");
    }

    #[test]
    fn test_normalize_line_ends_without_carriage_return() {
        let text = "a\u{A}b\u{9}c";
        let result = normalize_line_ends(text);
        // this is the same slice
        assert!(std::ptr::eq(text, result.as_ref()));
    }

    #[test]
    fn test_do_not_normalize_text_tab() {
        let text = "A \t B";
        assert_eq!(parse_text(text.into(), 0).unwrap(), "A \t B");
    }

    #[test]
    fn test_do_not_normalize_text_newline() {
        let text = "A \n B";
        assert_eq!(parse_text(text.into(), 0).unwrap(), "A \n B");
    }

    #[test]
    fn test_normalize_attribute_tab() {
        let text = "A \t B";
        assert_eq!(parse_attribute(text.into(), 0).unwrap(), "A   B");
    }

    #[test]
    fn test_normalize_attribute_r_newline() {
        let text = "A \r B";
        assert_eq!(parse_attribute(text.into(), 0).unwrap(), "A   B");
    }

    #[test]
    fn test_normalize_attribute_rn_newline() {
        let text = "A \r\n B";
        assert_eq!(parse_attribute(text.into(), 0).unwrap(), "A   B");
    }

    #[test]
    fn test_normalize_attribute_newline() {
        let text = "A \n B";
        assert_eq!(parse_attribute(text.into(), 0).unwrap(), "A   B");
    }

    #[test]
    fn test_serialize_text() {
        let text = "A & B";
        assert_eq!(
            serialize_text(text.into(), &NoopNormalizer, false, XML),
            "A &amp; B"
        );
    }

    #[test]
    fn test_serialize_text_multiple() {
        let text = "&<'\">";
        assert_eq!(
            serialize_text(text.into(), &NoopNormalizer, false, XML),
            "&amp;&lt;'\"&gt;"
        );
    }

    #[test]
    fn test_serialize_text_gt_escaped() {
        let text = ">";
        assert_eq!(
            serialize_text(text.into(), &NoopNormalizer, false, XML),
            "&gt;"
        );
    }

    #[test]
    fn test_serialize_text_gt_unescaped() {
        let text = ">";
        assert_eq!(serialize_text(text.into(), &NoopNormalizer, true, XML), ">");
    }

    #[test]
    fn test_serialize_text_like_cdata_section_close_delimiter() {
        let text = "]]>";
        assert_eq!(
            serialize_text(text.into(), &NoopNormalizer, false, XML),
            "]]&gt;"
        );
    }

    #[test]
    fn test_serialize_text_like_cdata_section_close_delimiter_gt_unescaped() {
        let text = "]]>";
        // this needs to happen even if the gt is asked to be unescaped
        assert_eq!(
            serialize_text(text.into(), &NoopNormalizer, true, XML),
            "]]&gt;"
        );
    }

    #[test]
    fn test_serialize_text_like_cdata_section_close_delimiter_split() {
        let text = "]]extra>";
        assert_eq!(
            serialize_text(text.into(), &NoopNormalizer, true, XML),
            "]]extra>"
        );
    }
    #[test]
    fn test_serialize_text_like_cdata_section_close_delimiter_prefix() {
        let text = "extra]]>";
        assert_eq!(
            serialize_text(text.into(), &NoopNormalizer, true, XML),
            "extra]]&gt;"
        );
    }

    #[test]
    fn test_serialize_text_like_cdata_section_close_delimiter_postfix() {
        let text = "]]>extra";
        assert_eq!(
            serialize_text(text.into(), &NoopNormalizer, true, XML),
            "]]&gt;extra"
        );
    }

    #[test]
    fn test_serialize_text_no_entities() {
        let text = "hello";
        let result = serialize_text(text.into(), &NoopNormalizer, false, XML);
        // this is the same slice
        assert!(std::ptr::eq(text, result.as_ref()));
    }

    #[test]
    fn test_serialize_attribute() {
        let text = "A & B";
        assert_eq!(
            serialize_attribute(text.into(), &NoopNormalizer, XML),
            "A &amp; B"
        );
    }

    #[test]
    fn test_serialize_attribute_multiple_single() {
        let text = "&<'";
        assert_eq!(
            serialize_attribute(text.into(), &NoopNormalizer, XML),
            "&amp;&lt;&apos;"
        );
    }

    #[test]
    fn test_serialize_attribute_multiple_double() {
        let text = "&<\"";
        assert_eq!(
            serialize_attribute(text.into(), &NoopNormalizer, XML),
            "&amp;&lt;&quot;"
        );
    }

    #[test]
    fn test_serialize_attribute_no_entities() {
        let text = "hello";
        let result = serialize_attribute(text.into(), &NoopNormalizer, XML);
        // this is the same slice
        assert!(std::ptr::eq(text, result.as_ref()));
    }

    #[test]
    fn test_parse_character_hex_entity() {
        let text = "A &#x26; B";
        assert_eq!(parse_text(text.into(), 0).unwrap(), "A & B");
    }

    #[test]
    fn test_parse_character_decimal_entity() {
        let text = "A &#38; B";
        assert_eq!(parse_text(text.into(), 0).unwrap(), "A & B");
    }

    #[test]
    fn test_parse_character_empty_entity() {
        let text = "A &#; B";
        assert!(parse_text(text.into(), 0).is_err());
    }

    #[test]
    fn test_parse_character_empty_hex_entity() {
        let text = "A &x#; B";
        assert!(parse_text(text.into(), 0).is_err());
    }

    #[test]
    fn test_parse_character_broken_hex_entity() {
        let text = "A &xflub#; B";
        assert!(parse_text(text.into(), 0).is_err());
    }

    /// Check that each reference, with text around it, is an invalid entity
    /// with the given string and the span of the reference, in text and in
    /// attribute values alike.
    fn assert_invalid_references(references: &[(&str, &str)]) {
        for (reference, entity) in references {
            let content = format!("ab{}cd", reference);
            let span = Span::new(12, 12 + reference.len());
            for result in [
                parse_text(content.as_str().into(), 10),
                parse_attribute(content.as_str().into(), 10),
            ] {
                match result {
                    Err(ParseError::InvalidEntity(e, s)) => {
                        assert_eq!((e.as_str(), s), (*entity, span), "{}", reference)
                    }
                    other => panic!("{}: expected InvalidEntity, got {:?}", reference, other),
                }
            }
        }
    }

    #[test]
    fn test_parse_character_reference_to_illegal_character() {
        // a character reference must refer to a character that matches the
        // `Char` production (https://www.w3.org/TR/xml/#wf-Legalchar)
        assert_invalid_references(&[
            ("&#0;", "0"),
            ("&#x0;", "x0"),
            ("&#x1;", "x1"),
            ("&#8;", "8"),
            ("&#xB;", "xB"),
            ("&#xc;", "xc"),
            ("&#xE;", "xE"),
            ("&#x1F;", "x1F"),
            ("&#31;", "31"),
            ("&#xFFFE;", "xFFFE"),
            ("&#xFFFF;", "xFFFF"),
            ("&#65535;", "65535"),
            // surrogates and numbers beyond Unicode are not characters at all
            ("&#xD800;", "xD800"),
            ("&#xDFFF;", "xDFFF"),
            ("&#x110000;", "x110000"),
            ("&#99999999999;", "99999999999"),
        ]);
    }

    #[test]
    fn test_parse_character_reference_not_digits() {
        // a character reference has decimal digits after `&#` and hexadecimal
        // digits after `&#x`, and nothing else (production `CharRef`,
        // https://www.w3.org/TR/xml/#sec-references); Rust's integer parsing
        // would also take a sign
        assert_invalid_references(&[
            ("&#+65;", "+65"),
            ("&#x+41;", "x+41"),
            ("&#-65;", "-65"),
            ("&#x-41;", "x-41"),
            ("&#;", ""),
            ("&#x;", "x"),
            ("&#X41;", "X41"),
            ("&#x41x;", "x41x"),
            ("&# 65;", " 65"),
            ("&#6 5;", "6 5"),
            ("&#6A;", "6A"),
            ("&#x4G;", "x4G"),
            ("&#65.0;", "65.0"),
        ]);
    }

    #[test]
    fn test_parse_character_reference_digits() {
        // leading zeros, and hexadecimal digits in either case
        for (reference, c) in [
            ("&#0065;", 'A'),
            ("&#x0041;", 'A'),
            ("&#x4a;", 'J'),
            ("&#x4A;", 'J'),
            ("&#xe9;", '\u{E9}'),
        ] {
            assert_eq!(parse_text(reference.into(), 0).unwrap(), c.to_string());
            assert_eq!(parse_attribute(reference.into(), 0).unwrap(), c.to_string());
        }
    }

    #[test]
    fn test_parse_character_reference_to_legal_character() {
        // the characters at the ends of the ranges that `Char` allows
        for (reference, c) in [
            ("&#x9;", '\u{9}'),
            ("&#xA;", '\u{A}'),
            ("&#xD;", '\u{D}'),
            ("&#x20;", '\u{20}'),
            ("&#xD7FF;", '\u{D7FF}'),
            ("&#xE000;", '\u{E000}'),
            ("&#xFFFD;", '\u{FFFD}'),
            ("&#x10000;", '\u{10000}'),
            ("&#x10FFFF;", '\u{10FFFF}'),
            ("&#1114111;", '\u{10FFFF}'),
        ] {
            assert_eq!(parse_text(reference.into(), 0).unwrap(), c.to_string());
            assert_eq!(parse_attribute(reference.into(), 0).unwrap(), c.to_string());
        }
    }

    #[test]
    fn test_serialize_cdata_simple() {
        let text = "hello";
        assert_eq!(
            serialize_cdata(text.into(), &NoopNormalizer, XML),
            "<![CDATA[hello]]>"
        );
    }

    #[test]
    fn test_serialize_cdata_end_sequence() {
        let text = "hello]]>world";
        assert_eq!(
            serialize_cdata(text.into(), &NoopNormalizer, XML),
            "<![CDATA[hello]]]]><![CDATA[>world]]>"
        );
    }

    #[test]
    fn test_serialize_cdata_two_square_brackets() {
        let text = "hello]]world";
        assert_eq!(
            serialize_cdata(text.into(), &NoopNormalizer, XML),
            "<![CDATA[hello]]world]]>"
        );
    }

    // two square brackets at the end
    #[test]
    fn test_serialize_cdata_two_square_brackets_end() {
        let text = "hello]]";
        assert_eq!(
            serialize_cdata(text.into(), &NoopNormalizer, XML),
            "<![CDATA[hello]]]]>"
        );
    }

    // greater than sign by itself
    #[test]
    fn test_serialize_cdata_greater_than() {
        let text = ">";
        assert_eq!(
            serialize_cdata(text.into(), &NoopNormalizer, XML),
            "<![CDATA[>]]>"
        );
    }

    // special sequence by itself
    #[test]
    fn test_serialize_cdata_special_sequence() {
        let text = "]]>";
        assert_eq!(
            serialize_cdata(text.into(), &NoopNormalizer, XML),
            "<![CDATA[]]]]><![CDATA[>]]>"
        );
    }

    // three square brackets
    #[test]
    fn test_serialize_cdata_three_square_brackets() {
        let text = "hello]]]world";
        assert_eq!(
            serialize_cdata(text.into(), &NoopNormalizer, XML),
            "<![CDATA[hello]]]world]]>"
        );
    }

    // three square brackets ending in special sequence
    #[test]
    fn test_serialize_cdata_three_square_brackets_end_sequence() {
        let text = "hello]]]>world";
        assert_eq!(
            serialize_cdata(text.into(), &NoopNormalizer, XML),
            "<![CDATA[hello]]]]]><![CDATA[>world]]>"
        );
    }

    const XML_UNESCAPED_CR: Escapes = Escapes::RoundTrip { unescaped_cr: true };

    #[test]
    fn test_serialize_text_cr() {
        let text = "A\rB\r\nC";
        assert_eq!(
            serialize_text(text.into(), &NoopNormalizer, false, XML),
            "A&#xD;B&#xD;\nC"
        );
    }

    #[test]
    fn test_serialize_text_unescaped_cr() {
        let text = "A\rB";
        assert_eq!(
            serialize_text(text.into(), &NoopNormalizer, false, XML_UNESCAPED_CR),
            "A\rB"
        );
    }

    #[test]
    fn test_serialize_text_tab_newline_unescaped() {
        let text = "A\tB\nC";
        let result = serialize_text(text.into(), &NoopNormalizer, false, XML);
        assert!(std::ptr::eq(text, result.as_ref()));
    }

    #[test]
    fn test_serialize_text_line_ends_and_control_characters() {
        // NEL, LINE SEPARATOR and the control characters #x7F to #x9F, but
        // not the characters around that range
        let text = "\u{7E}\u{7F}\u{85}\u{9F}\u{A0}\u{2028}\u{2029}";
        assert_eq!(
            serialize_text(text.into(), &NoopNormalizer, false, XML),
            "\u{7E}&#x7F;&#x85;&#x9F;\u{A0}&#x2028;\u{2029}"
        );
        assert_eq!(
            serialize_text(text.into(), &NoopNormalizer, false, XML_UNESCAPED_CR),
            "\u{7E}&#x7F;&#x85;&#x9F;\u{A0}&#x2028;\u{2029}"
        );
    }

    #[test]
    fn test_serialize_text_markup_only() {
        let text = "A\r\u{85}\u{2028}&";
        assert_eq!(
            serialize_text(text.into(), &NoopNormalizer, false, Escapes::MarkupOnly),
            "A\r\u{85}\u{2028}&amp;"
        );
    }

    #[test]
    fn test_serialize_text_gt_unescaped_with_char_ref() {
        let text = "]]\u{85}>";
        assert_eq!(
            serialize_text(text.into(), &NoopNormalizer, true, XML),
            "]]&#x85;>"
        );
    }

    #[test]
    fn test_serialize_attribute_whitespace() {
        let text = "A\tB\nC\rD\r\nE F";
        assert_eq!(
            serialize_attribute(text.into(), &NoopNormalizer, XML),
            "A&#x9;B&#xA;C&#xD;D&#xD;&#xA;E F"
        );
        // a carriage return is escaped in attribute values even when it is
        // not in text
        assert_eq!(
            serialize_attribute(text.into(), &NoopNormalizer, XML_UNESCAPED_CR),
            "A&#x9;B&#xA;C&#xD;D&#xD;&#xA;E F"
        );
    }

    #[test]
    fn test_serialize_attribute_line_ends_and_control_characters() {
        let text = "\u{7E}\u{7F}\u{85}\u{9F}\u{A0}\u{2028}";
        assert_eq!(
            serialize_attribute(text.into(), &NoopNormalizer, XML),
            "\u{7E}&#x7F;&#x85;&#x9F;\u{A0}&#x2028;"
        );
    }

    #[test]
    fn test_serialize_attribute_markup_only() {
        let text = "A\tB\nC\rD";
        let result = serialize_attribute(text.into(), &NoopNormalizer, Escapes::MarkupOnly);
        assert!(std::ptr::eq(text, result.as_ref()));
    }

    #[test]
    fn test_serialize_cdata_empty() {
        assert_eq!(
            serialize_cdata("".into(), &NoopNormalizer, XML),
            "<![CDATA[]]>"
        );
    }

    #[test]
    fn test_serialize_cdata_cr() {
        let text = "hello\rworld";
        assert_eq!(
            serialize_cdata(text.into(), &NoopNormalizer, XML),
            "<![CDATA[hello]]>&#xD;<![CDATA[world]]>"
        );
    }

    #[test]
    fn test_serialize_cdata_char_refs_at_start_and_end() {
        let text = "\rhello\u{2028}";
        assert_eq!(
            serialize_cdata(text.into(), &NoopNormalizer, XML),
            "&#xD;<![CDATA[hello]]>&#x2028;"
        );
        assert_eq!(
            serialize_cdata("\r\n".into(), &NoopNormalizer, XML),
            "&#xD;<![CDATA[\n]]>"
        );
    }

    #[test]
    fn test_serialize_cdata_unescaped_cr() {
        let text = "hello\rworld\u{85}";
        assert_eq!(
            serialize_cdata(text.into(), &NoopNormalizer, XML_UNESCAPED_CR),
            "<![CDATA[hello\rworld]]>&#x85;"
        );
    }

    #[test]
    fn test_serialize_cdata_char_ref_between_square_brackets_and_gt() {
        // the char reference ends the section, so `]]` and `>` are no longer
        // adjacent
        let text = "]]\r>";
        assert_eq!(
            serialize_cdata(text.into(), &NoopNormalizer, XML),
            "<![CDATA[]]]]>&#xD;<![CDATA[>]]>"
        );
    }

    #[test]
    fn test_serialize_cdata_markup_only() {
        let text = "hello\rworld";
        assert_eq!(
            serialize_cdata(text.into(), &NoopNormalizer, Escapes::MarkupOnly),
            "<![CDATA[hello\rworld]]>"
        );
    }
}
