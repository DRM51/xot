use std::borrow::Cow;
use std::fmt::Write;

use crate::error::ParseError;
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
fn is_xml11_line_end_or_control(c: char) -> bool {
    matches!(c, '\u{7F}'..='\u{9F}' | '\u{2028}')
}

fn push_char_ref(result: &mut String, c: char) {
    // writing to a string cannot fail
    let _ = write!(result, "&#x{:X};", u32::from(c));
}

pub(crate) fn parse_text(content: Cow<str>, base_position: usize) -> Result<Cow<str>, ParseError> {
    parse_content(content, false, base_position)
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

pub(crate) fn parse_attribute(
    content: Cow<str>,
    base_position: usize,
) -> Result<Cow<str>, ParseError> {
    parse_content(content, true, base_position)
}

fn parse_content(
    content: Cow<str>,
    attribute: bool,
    base_position: usize,
) -> Result<Cow<str>, ParseError> {
    let mut result = String::new();
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
                let first_char = entity.chars().next().ok_or_else(|| {
                    ParseError::InvalidEntity(
                        entity.to_string(),
                        Span::new(base_position + position, base_position + end_position),
                    )
                })?;
                let code = if first_char == 'x' {
                    u32::from_str_radix(&entity[1..], 16)
                } else {
                    entity.parse::<u32>()
                };
                let code = code.map_err(|_| {
                    ParseError::InvalidEntity(
                        entity.to_string(),
                        Span::new(base_position + position, base_position + end_position),
                    )
                })?;
                let c = std::char::from_u32(code).ok_or_else(|| {
                    ParseError::InvalidEntity(
                        entity.to_string(),
                        Span::new(base_position + position, base_position + end_position),
                    )
                })?;
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
        Ok(result.into())
    }
}

pub(crate) fn serialize_text<'a, N: Normalizer>(
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

pub(crate) fn serialize_cdata<'a, N: Normalizer>(
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

pub(crate) fn serialize_attribute<'a, N: Normalizer>(
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
