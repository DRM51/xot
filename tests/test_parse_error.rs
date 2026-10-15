use xot::Xot;

#[test]
fn test_parse_invalid_close_tag() {
    let xml = r#"<doc></a></doc>"#;
    let mut xot = Xot::new();
    let err = xot.parse(xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::InvalidCloseTag { .. }));
    assert_eq!(err.span(), (7..8).into());
}

#[test]
fn test_parse_invalid_close_tag_prefix() {
    let xml = r#"<doc xmlns:a="http://example.com"></a:doc></doc>"#;
    let mut xot = Xot::new();
    let err = xot.parse(xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::InvalidCloseTag { .. }));
    assert_eq!(err.span(), (36..41).into());
}

#[test]
fn test_unknown_prefix() {
    let xml = r#"<doc><a:p/></doc>"#;
    let mut xot = Xot::new();
    let err = xot.parse(xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::UnknownPrefix { .. }));
    assert_eq!(err.span(), (6..7).into());
}

#[test]
fn test_unsupported_version() {
    let xml = r#"<?xml version="1.1"?><doc/></doc>"#;
    let mut xot = Xot::new();
    let err = xot.parse(xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::UnsupportedVersion { .. }));
    assert_eq!(err.span(), (15..18).into());
}

// The order of the version, encoding and standalone attributes in the XML declaration is fixed by
// the standard.
#[test]
fn test_parse_invalid_xml_declaration() {
    let mut xot = Xot::new();
    let err = xot
        .parse(r#"<?xml version="1.0" standalone="yes" encoding="UTF-8"?><a/>"#)
        .unwrap_err();
    assert!(matches!(err, xot::ParseError::XmlParser { .. }));
    match err {
        xot::ParseError::XmlParser(e, _) => {
            assert!(matches!(e, xmlparser::Error::InvalidDeclaration { .. }))
        }
        _ => unreachable!(),
    }
}

#[test]
fn test_xmlparser_error() {
    let xml = r#"<doc><"#;
    let mut xot = Xot::new();
    let err = xot.parse(xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::XmlParser { .. }));
    assert_eq!(err.span(), (5..5).into());
}

#[test]
fn test_duplicate_attribute() {
    let xml = r#"<doc a="1" a="2"/>"#;
    let mut xot = Xot::new();
    let err = xot.parse(xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::DuplicateAttribute { .. }));
    assert_eq!(err.span(), (11..12).into());
}

#[test]
fn test_duplicate_attribute_with_prefix() {
    let xml = r#"<doc xmlns:a="http://example.com" a:b="1" a:b="2"/>"#;
    let mut xot = Xot::new();
    let err = xot.parse(xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::DuplicateAttribute { .. }));
    assert_eq!(err.span(), (42..45).into());
}

#[test]
fn test_invalid_dtd() {
    let xml = r#"<!DOCTYPE note><note></note>"#;
    let mut xot = Xot::new();
    let err = xot.parse(xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::DtdUnsupported { .. }));
    assert_eq!(err.span(), (0..15).into());
}

#[test]
fn test_dtd_unsupported() {
    let xml = r#"<!DOCTYPE note SYSTEM "Note.dtd"><note></note>"#;
    let mut xot = Xot::new();
    let err = xot.parse(xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::DtdUnsupported { .. }));
    assert_eq!(err.span(), (0..33).into());
}

#[test]
fn test_dtd_unsupported2() {
    let xml = r#"<?xml version="1.0"?><!DOCTYPE note SYSTEM "Note.dtd"><note></note>"#;
    let mut xot = Xot::new();
    let err = xot.parse(xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::DtdUnsupported { .. }));
    assert_eq!(err.span(), (21..54).into());
}

#[test]
fn test_unclosed_tag() {
    let xml = r#"<doc>"#;
    let mut xot = Xot::new();
    let err = xot.parse(xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::UnclosedTag(_)));
    assert_eq!(err.span(), (1..4).into());
}

#[test]
fn test_unclosed_tag_middle() {
    let xml = r#"<doc><a></doc>"#;
    let mut xot = Xot::new();
    let err = xot.parse(xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::InvalidCloseTag(_, _, _)));
    assert_eq!(err.span(), (10..13).into());
}

#[test]
fn test_unclosed_tag_with_prefix() {
    let xml = r#"<a:doc xmlns:a="http://example.com">"#;
    let mut xot = Xot::new();
    let err = xot.parse(xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::UnclosedTag(_)));
    assert_eq!(err.span(), (1..6).into());
}

#[test]
fn test_only_text_top_level() {
    let xml = r#"abc"#;
    let mut xot = Xot::new();
    let err = xot.parse(xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::XmlParser { .. }));
    assert_eq!(err.span(), (0..0).into());
}

#[test]
fn test_only_comment_top_level() {
    let xml = r#"<!-- foo -->"#;
    let mut xot = Xot::new();
    let err = xot.parse(xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::NoElementAtTopLevel { .. }));
    assert_eq!(err.span(), (12..12).into());
}

#[test]
fn test_unclosed_entity() {
    let xml = r#"<doc>&foo"#;
    let mut xot = Xot::new();
    let err = xot.parse(xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::UnclosedEntity(_, _)));
    assert_eq!(err.span(), (5..5).into());
}

#[test]
fn test_invalid_entity() {
    let xml = r#"<doc>&foo;</doc>"#;
    let mut xot = Xot::new();
    let err = xot.parse(xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::InvalidEntity(_, _)));
    assert_eq!(err.span(), (5..10).into());
}

// XML 1.0 allows a character reference only to a character that matches the
// `Char` production (section 4.1, "WFC: Legal Character"); the span is that
// of the reference

#[test]
fn test_character_reference_to_illegal_character_in_text() {
    let mut xot = Xot::new();
    let err = xot.parse("<doc>a&#x1;b</doc>").unwrap_err();
    assert!(matches!(err, xot::ParseError::InvalidEntity(ref s, _) if s == "x1"));
    assert_eq!(err.span(), (6..11).into());
    let err = xot.parse("<doc>&#0;</doc>").unwrap_err();
    assert!(matches!(err, xot::ParseError::InvalidEntity(ref s, _) if s == "0"));
    assert_eq!(err.span(), (5..9).into());
    let err = xot.parse("<doc>&#xFFFE;</doc>").unwrap_err();
    assert!(matches!(err, xot::ParseError::InvalidEntity(ref s, _) if s == "xFFFE"));
    assert_eq!(err.span(), (5..13).into());
}

#[test]
fn test_character_reference_to_illegal_character_in_attribute() {
    let mut xot = Xot::new();
    let err = xot.parse(r#"<doc a="x&#xB;"/>"#).unwrap_err();
    assert!(matches!(err, xot::ParseError::InvalidEntity(ref s, _) if s == "xB"));
    assert_eq!(err.span(), (9..14).into());
}

#[test]
fn test_character_reference_to_illegal_character_in_namespace_declaration() {
    let mut xot = Xot::new();
    let err = xot.parse(r#"<doc xmlns:p="urn:&#xFFFF;"/>"#).unwrap_err();
    assert!(matches!(err, xot::ParseError::InvalidEntity(ref s, _) if s == "xFFFF"));
    assert_eq!(err.span(), (18..26).into());
    let err = xot.parse(r#"<doc xmlns="&#31;"/>"#).unwrap_err();
    assert!(matches!(err, xot::ParseError::InvalidEntity(ref s, _) if s == "31"));
    assert_eq!(err.span(), (12..17).into());
}

#[test]
fn test_character_reference_to_illegal_character_in_fragment() {
    let mut xot = Xot::new();
    let err = xot.parse_fragment("a&#x8;").unwrap_err();
    assert!(matches!(err, xot::ParseError::InvalidEntity(ref s, _) if s == "x8"));
    assert_eq!(err.span(), (1..6).into());
}

#[test]
fn test_character_reference_with_sign() {
    // a character reference has only digits after `&#` or `&#x`, without a
    // sign (production `CharRef`)
    let mut xot = Xot::new();
    let err = xot.parse("<doc>&#+65;</doc>").unwrap_err();
    assert!(matches!(err, xot::ParseError::InvalidEntity(ref s, _) if s == "+65"));
    assert_eq!(err.span(), (5..11).into());
    let err = xot.parse(r#"<doc a="&#x+41;"/>"#).unwrap_err();
    assert!(matches!(err, xot::ParseError::InvalidEntity(ref s, _) if s == "x+41"));
    assert_eq!(err.span(), (8..15).into());
    let err = xot.parse(r#"<doc xmlns:p="urn:&#+65;"/>"#).unwrap_err();
    assert!(matches!(err, xot::ParseError::InvalidEntity(ref s, _) if s == "+65"));
    assert_eq!(err.span(), (18..24).into());
}

#[test]
fn test_parse_bytes_unknown_encoding() {
    let xml = br#"<?xml version="1.0" encoding="x-unknown"?><doc/>"#;
    let mut xot = Xot::new();
    let err = xot.parse_bytes(xml).unwrap_err();
    assert!(
        matches!(err, xot::ParseError::UnsupportedEncoding(ref name, _) if name == "x-unknown")
    );
    assert_eq!(err.span(), (30..39).into());
}

#[test]
fn test_parse_bytes_empty_encoding() {
    let xml = br#"<?xml version="1.0" encoding=""?><doc/>"#;
    let mut xot = Xot::new();
    let err = xot.parse_bytes(xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::UnsupportedEncoding(ref name, _) if name.is_empty()));
    assert_eq!(err.span(), (30..30).into());
}

#[test]
fn test_parse_bytes_unknown_encoding_after_bom() {
    // the span is in bytes, and counts the byte order mark
    let xml = b"\xEF\xBB\xBF<?xml version=\"1.0\" encoding=\"x-unknown\"?><doc/>";
    let mut xot = Xot::new();
    let err = xot.parse_bytes(xml).unwrap_err();
    assert!(
        matches!(err, xot::ParseError::UnsupportedEncoding(ref name, _) if name == "x-unknown")
    );
    assert_eq!(err.span(), (33..42).into());
}

#[test]
fn test_parse_bytes_unknown_encoding_utf16() {
    // the span is in bytes, and counts the byte order mark
    let xml: Vec<u8> = "\u{FEFF}<?xml version=\"1.0\" encoding=\"x-unknown\"?><doc/>"
        .encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .collect();
    let mut xot = Xot::new();
    let err = xot.parse_bytes(&xml).unwrap_err();
    assert!(
        matches!(err, xot::ParseError::UnsupportedEncoding(ref name, _) if name == "x-unknown")
    );
    assert_eq!(err.span(), (62..80).into());
}

#[test]
fn test_parse_bytes_encoding_conflict_with_bom() {
    // a byte order mark and an encoding declaration have to agree
    let xml = b"\xEF\xBB\xBF<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><doc/>";
    let mut xot = Xot::new();
    let err = xot.parse_bytes(xml).unwrap_err();
    assert!(matches!(
        err,
        xot::ParseError::EncodingConflict(ref declared, ref found, _)
            if declared == "ISO-8859-1" && found == "UTF-8 byte order mark"
    ));
    assert_eq!(err.span(), (33..43).into());
    assert_eq!(
        err.to_string(),
        "Encoding conflict: declared ISO-8859-1, found UTF-8 byte order mark"
    );
}

#[test]
fn test_parse_bytes_encoding_conflict_utf16_declared_in_ascii() {
    let xml = br#"<?xml version="1.0" encoding="UTF-16"?><doc/>"#;
    let mut xot = Xot::new();
    let err = xot.parse_bytes(xml).unwrap_err();
    assert!(matches!(
        err,
        xot::ParseError::EncodingConflict(ref declared, ref found, _)
            if declared == "UTF-16" && found == "ASCII-compatible encoding"
    ));
    assert_eq!(err.span(), (30..36).into());
}

#[test]
fn test_parse_bytes_encoding_conflict_utf16_without_declaration() {
    // UTF-16 without a byte order mark has to be declared
    let xml: Vec<u8> =
        r#"<?xml version="1.0"?><doc/>"#.encode_utf16().flat_map(|u| u.to_be_bytes()).collect();
    let mut xot = Xot::new();
    let err = xot.parse_bytes(&xml).unwrap_err();
    assert!(matches!(
        err,
        xot::ParseError::EncodingConflict(ref declared, ref found, _)
            if declared.is_empty() && found == "UTF-16BE without byte order mark"
    ));
    assert_eq!(err.span(), (0..0).into());
    assert_eq!(
        err.to_string(),
        "Encoding conflict: no encoding declaration, found UTF-16BE without byte order mark"
    );
}

#[test]
fn test_parse_bytes_utf32() {
    let xml: Vec<u8> = "\u{FEFF}<doc/>"
        .chars()
        .flat_map(|c| (c as u32).to_le_bytes())
        .collect();
    let mut xot = Xot::new();
    let err = xot.parse_bytes(&xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::UnsupportedEncoding(ref name, _) if name == "UTF-32LE"));
    assert_eq!(err.span(), (0..0).into());
}

#[test]
fn test_parse_bytes_invalid_byte_sequence() {
    // without a byte order mark or an encoding declaration the document is
    // UTF-8, in which 0xE9 is not a character on its own
    let xml = b"<doc>caf\xe9</doc>";
    let mut xot = Xot::new();
    let err = xot.parse_bytes(xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::InvalidByteSequence(ref name, _) if name == "UTF-8"));
    assert_eq!(err.span(), (8..9).into());
    assert_eq!(
        err.to_string(),
        "Invalid byte sequence for encoding UTF-8 at byte 8"
    );
}

#[test]
fn test_parse_bytes_charset_outside_declaration() {
    // `charset=` outside the XML declaration does not give the encoding: the
    // document is UTF-8, in which 0xE9 is not a character on its own
    let xml = b"<html><head><meta charset=\"iso-8859-1\"/></head><body>\xe9</body></html>";
    let mut xot = Xot::new();
    let err = xot.parse_bytes(xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::InvalidByteSequence(ref name, _) if name == "UTF-8"));
    assert_eq!(err.span(), (53..54).into());
}

#[test]
fn test_parse_bytes_invalid_byte_sequence_declared_encoding() {
    // 0xFD is not a character in Shift_JIS
    let xml = b"<?xml version=\"1.0\" encoding=\"Shift_JIS\"?><doc>\xfd</doc>";
    let mut xot = Xot::new();
    let err = xot.parse_bytes(xml).unwrap_err();
    assert!(
        matches!(err, xot::ParseError::InvalidByteSequence(ref name, _) if name == "Shift_JIS")
    );
    assert_eq!(err.span(), (47..48).into());
}

#[test]
fn test_parse_bytes_invalid_byte_sequence_us_ascii() {
    // US-ASCII has no characters for the bytes 0x80 to 0xFF
    let xml = b"<?xml version=\"1.0\" encoding=\"US-ASCII\"?><doc>caf\xe9</doc>";
    let mut xot = Xot::new();
    let err = xot.parse_bytes(xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::InvalidByteSequence(ref name, _) if name == "US-ASCII"));
    assert_eq!(err.span(), (49..50).into());
    assert_eq!(
        err.to_string(),
        "Invalid byte sequence for encoding US-ASCII at byte 49"
    );
}

#[test]
fn test_parse_bytes_invalid_byte_sequence_us_ascii_names() {
    // whichever name of US-ASCII the document uses
    let mut xot = Xot::new();
    for name in [
        "US-ASCII",
        "us-ascii",
        "ASCII",
        "ANSI_X3.4-1968",
        "ANSI_X3.4-1986",
        "ISO646-US",
        "us",
        "IBM367",
        "csASCII",
    ] {
        let mut xml = format!("<?xml version=\"1.0\" encoding=\"{}\"?><doc>", name).into_bytes();
        let at = xml.len();
        xml.extend(b"\x80</doc>");
        let err = xot.parse_bytes(&xml).unwrap_err();
        assert!(
            matches!(err, xot::ParseError::InvalidByteSequence(ref name, _) if name == "US-ASCII"),
            "{}: {:?}",
            name,
            err
        );
        assert_eq!(err.span(), (at..at + 1).into(), "{}", name);
    }
}

#[test]
fn test_parse_bytes_invalid_byte_sequence_tis_620() {
    // TIS-620, also named ISO-8859-11, has no character for the bytes 0xDB to
    // 0xDE and 0xFC to 0xFF
    let xml = b"<?xml version=\"1.0\" encoding=\"ISO-8859-11\"?><doc>ok\xfc</doc>";
    let mut xot = Xot::new();
    let err = xot.parse_bytes(xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::InvalidByteSequence(ref name, _) if name == "TIS-620"));
    assert_eq!(err.span(), (51..52).into());
}

#[test]
fn test_parse_bytes_invalid_byte_sequence_utf16() {
    // a high surrogate without a low surrogate; the offsets are in bytes and
    // count the byte order mark
    let mut xml: Vec<u8> = "\u{FEFF}<doc>"
        .encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .collect();
    xml.extend([0x00, 0xD8]);
    xml.extend("</doc>".encode_utf16().flat_map(|u| u.to_le_bytes()));
    let mut xot = Xot::new();
    let err = xot.parse_bytes(&xml).unwrap_err();
    assert!(matches!(err, xot::ParseError::InvalidByteSequence(ref name, _) if name == "UTF-16LE"));
    assert_eq!(err.span(), (12..14).into());
}
