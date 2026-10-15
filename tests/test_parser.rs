use xot::{ParseError, Span, SpanInfoKey, Xot};

const US_ASCII: &str = include_str!("fixtures/us-ascii.xml");

#[test]
fn test_unclosed_tag() {
    let mut xot = Xot::new();
    let err = xot.parse(r#"<a><b></a>"#).unwrap_err();
    assert!(matches!(err, ParseError::InvalidCloseTag(_, _, _)));
}

#[test]
fn test_unclosed_tag_at_end() {
    let mut xot = Xot::new();
    let err = xot.parse(r#"<a>"#).unwrap_err();
    assert!(matches!(err, ParseError::UnclosedTag(_)));
}

#[test]
fn test_duplicate_attributes() {
    let mut xot = Xot::new();
    let err = xot.parse(r#"<a x="x" x="y"/>"#).unwrap_err();
    let s = match err {
        ParseError::DuplicateAttribute(s, _) => s,
        _ => unreachable!(),
    };
    assert_eq!(s, "x");
}

#[test]
fn test_duplicate_attributes_ns() {
    let mut xot = Xot::new();
    let err = xot
        .parse(r#"<a xmlns:foo="http://example.com" foo:x="x" foo:x="y"/>"#)
        .unwrap_err();
    let s = match err {
        ParseError::DuplicateAttribute(s, _) => s,
        _ => unreachable!(),
    };
    assert_eq!(s, "foo:x");
}

#[test]
fn test_parse_xml_declaration() {
    let mut xot = Xot::new();
    let doc = xot.parse(r#"<?xml version="1.0" encoding="UTF-8"?><a/>"#);
    assert!(doc.is_ok());
}

#[test]
fn test_parse_xml_standalone_yes() {
    let mut xot = Xot::new();
    let doc = xot.parse(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><a/>"#);
    assert!(doc.is_ok());
}

#[test]
fn test_parse_xml_standalone_no() {
    let mut xot = Xot::new();
    let doc = xot.parse(r#"<?xml version="1.0" encoding="UTF-8" standalone="no"?><a/>"#);
    assert!(doc.is_ok());
}

#[test]
fn test_encoding_lowercase_utf8() {
    let mut xot = Xot::new();
    let doc = xot.parse(r#"<?xml version="1.0" encoding="utf-8"?><a/>"#);
    assert!(doc.is_ok());
}

#[test]
fn test_encoding_us_ascii() {
    let mut xot = Xot::new();
    let doc = xot.parse(US_ASCII);
    assert!(doc.is_ok());
}

#[test]
fn test_unknown_prefix() {
    let mut xot = Xot::new();
    let err = xot.parse(r#"<a><foo:b></a>"#).unwrap_err();
    if let ParseError::UnknownPrefix(s, _) = err {
        assert_eq!(s, "foo");
    } else {
        unreachable!();
    }
}

#[test]
fn test_parse_non_static() -> Result<(), ParseError> {
    let mut xot = Xot::new();
    let mut xml = String::new();
    xml.push('<');
    xml.push('a');
    xml.push('>');
    xml.push('<');
    xml.push('/');
    xml.push('a');
    xml.push('>');
    let doc = xot.parse(&xml)?;
    drop(xml);
    let doc_el = xot.document_element(doc).unwrap();
    let el = xot.element(doc_el).unwrap();
    assert_eq!(xot.name_ns_str(el.name()), ("a", ""));
    Ok(())
}

#[test]
fn test_ampersand() -> Result<(), ParseError> {
    let mut xot = Xot::new();
    let err = xot.parse(r#"<a>&</a>"#).unwrap_err();
    assert!(matches!(err, ParseError::UnclosedEntity(_, _)));
    Ok(())
}

#[test]
fn test_ampersand_in_cdata() -> Result<(), ParseError> {
    let mut xot = Xot::new();
    let doc = xot.parse(r#"<a><![CDATA[&]]></a>"#)?;
    let doc_el = xot.document_element(doc).unwrap();
    let txt = xot.text_content_str(doc_el).unwrap();
    assert_eq!(txt, "&");
    Ok(())
}

#[test]
fn test_character_references_to_legal_characters() -> Result<(), ParseError> {
    // the characters at the ends of the ranges that the `Char` production of
    // XML 1.0 allows
    let mut xot = Xot::new();
    let doc = xot.parse(
        r#"<a b="&#x9;&#xD7FF;&#xE000;&#xFFFD;&#x10000;&#x10FFFF;">&#xD;&#x20;&#xFFFD;&#1114111;</a>"#,
    )?;
    let doc_el = xot.document_element(doc).unwrap();
    let b = xot.add_name("b");
    assert_eq!(
        xot.get_attribute(doc_el, b),
        Some("\u{9}\u{D7FF}\u{E000}\u{FFFD}\u{10000}\u{10FFFF}")
    );
    assert_eq!(
        xot.text_content_str(doc_el),
        Some("\u{D}\u{20}\u{FFFD}\u{10FFFF}")
    );
    Ok(())
}

#[test]
fn test_character_reference_in_cdata_is_text() -> Result<(), ParseError> {
    // a CDATA section has no references, so this is not a reference to a
    // character that XML 1.0 does not allow
    let mut xot = Xot::new();
    let doc = xot.parse(r#"<a><![CDATA[&#x1;]]></a>"#)?;
    let doc_el = xot.document_element(doc).unwrap();
    assert_eq!(xot.text_content_str(doc_el), Some("&#x1;"));
    Ok(())
}

#[test]
fn test_parse_with_span_info_element_start_unprefixed() {
    let mut xot = Xot::new();
    let (doc, span_info) = xot.parse_with_span_info(r#"<a></a>"#).unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    assert_eq!(
        span_info.get(SpanInfoKey::ElementStart(doc_el)).unwrap(),
        &Span::new(1, 2)
    );
}

#[test]
fn test_parse_with_span_info_element_start_prefixed() {
    let mut xot = Xot::new();
    let (doc, span_info) = xot
        .parse_with_span_info(r#"<foo:a xmlns:foo="http://example.com/foo"></foo:a>"#)
        .unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    assert_eq!(
        span_info.get(SpanInfoKey::ElementStart(doc_el)).unwrap(),
        &Span::new(1, 6)
    );
}

#[test]
fn test_parse_with_span_info_element_start_unprefixed_nested() {
    let mut xot = Xot::new();
    let (doc, span_info) = xot.parse_with_span_info(r#"<a><b></b></a>"#).unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    let b_el = xot.first_child(doc_el).unwrap();

    assert_eq!(
        span_info.get(SpanInfoKey::ElementStart(b_el)).unwrap(),
        &Span::new(4, 5)
    );
}

#[test]
fn test_parse_with_span_info_attribute_name_unprefixed() {
    let mut xot = Xot::new();
    let (doc, span_info) = xot.parse_with_span_info(r#"<a b="B"></a>"#).unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    let attribute_name = xot.name("b").unwrap();

    assert_eq!(
        span_info
            .get(SpanInfoKey::AttributeName(doc_el, attribute_name))
            .unwrap(),
        &Span::new(3, 4)
    );
}

#[test]
fn test_parse_with_span_info_attribute_value_unprefixed() {
    let mut xot = Xot::new();
    let (doc, span_info) = xot.parse_with_span_info(r#"<a b="B"></a>"#).unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    let attribute_name = xot.name("b").unwrap();

    assert_eq!(
        span_info
            .get(SpanInfoKey::AttributeValue(doc_el, attribute_name))
            .unwrap(),
        &Span::new(6, 7)
    );
}

#[test]
fn test_parse_with_span_info_attribute_name_prefixed() {
    let mut xot = Xot::new();
    let (doc, span_info) = xot
        .parse_with_span_info(r#"<a foo:b="B" xmlns:foo="http://example.com/foo"></a>"#)
        .unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    let ns = xot.namespace("http://example.com/foo").unwrap();
    let attribute_name = xot.name_ns("b", ns).unwrap();

    assert_eq!(
        span_info
            .get(SpanInfoKey::AttributeName(doc_el, attribute_name))
            .unwrap(),
        &Span::new(3, 8)
    );
}

#[test]
fn test_parse_with_span_info_attribute_value_prefixed() {
    let mut xot = Xot::new();
    let (doc, span_info) = xot
        .parse_with_span_info(r#"<a foo:b="B" xmlns:foo="http://example.com/foo"></a>"#)
        .unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    let ns = xot.namespace("http://example.com/foo").unwrap();
    let attribute_name = xot.name_ns("b", ns).unwrap();

    assert_eq!(
        span_info
            .get(SpanInfoKey::AttributeValue(doc_el, attribute_name))
            .unwrap(),
        &Span::new(10, 11)
    );
}

#[test]
fn test_parse_with_span_info_end_normal() {
    let mut xot = Xot::new();
    let (doc, span_info) = xot.parse_with_span_info(r#"<a></a>"#).unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    assert_eq!(
        span_info.get(SpanInfoKey::ElementEnd(doc_el)).unwrap(),
        &Span::new(3, 7)
    );
}

#[test]
fn test_parse_with_span_info_empty() {
    let mut xot = Xot::new();
    let (doc, span_info) = xot.parse_with_span_info(r#"<a/>"#).unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    assert_eq!(
        span_info.get(SpanInfoKey::ElementEnd(doc_el)).unwrap(),
        &Span::new(2, 4)
    );
}

#[test]
fn test_parse_with_span_info_text() {
    let mut xot = Xot::new();
    let (doc, span_info) = xot.parse_with_span_info(r#"<a>text</a>"#).unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    let text = xot.first_child(doc_el).unwrap();
    assert_eq!(
        span_info.get(SpanInfoKey::Text(text)).unwrap(),
        &Span::new(3, 7)
    );
}

#[test]
fn test_parse_with_span_info_comment() {
    let mut xot = Xot::new();
    let (doc, span_info) = xot
        .parse_with_span_info(r#"<a><!--comment--></a>"#)
        .unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    let comment = xot.first_child(doc_el).unwrap();
    assert_eq!(
        span_info.get(SpanInfoKey::Comment(comment)).unwrap(),
        &Span::new(7, 14)
    );
}

#[test]
fn test_parse_with_span_info_pi_target() {
    let mut xot = Xot::new();
    let (doc, span_info) = xot.parse_with_span_info(r#"<a><?pi?></a>"#).unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    let pi = xot.first_child(doc_el).unwrap();
    assert_eq!(
        span_info.get(SpanInfoKey::PiTarget(pi)).unwrap(),
        &Span::new(5, 7)
    );
}

#[test]
fn test_parse_with_span_info_pi_content() {
    let mut xot = Xot::new();
    let (doc, span_info) = xot
        .parse_with_span_info(r#"<a><?pi content?></a>"#)
        .unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    let pi = xot.first_child(doc_el).unwrap();
    assert_eq!(
        span_info.get(SpanInfoKey::PiContent(pi)).unwrap(),
        &Span::new(8, 15)
    );
}

#[test]
fn test_parse_consolidated_cdata_text() {
    let mut xot = Xot::new();
    let doc = xot.parse(r#"<a><![CDATA[foo]]>bar</a>"#).unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    assert_eq!(xot.children(doc_el).count(), 1);
    let txt = xot.text_content_str(doc_el).unwrap();
    assert_eq!(txt, "foobar");
}

#[test]
fn test_parse_consolidated_text_cdata() {
    let mut xot = Xot::new();
    let doc = xot.parse(r#"<a>foo<![CDATA[bar]]></a>"#).unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    assert_eq!(xot.children(doc_el).count(), 1);
    let txt = xot.text_content_str(doc_el).unwrap();
    assert_eq!(txt, "foobar");
}

#[test]
fn test_parse_consolidated_text_cdata_text() {
    let mut xot = Xot::new();
    let doc = xot.parse(r#"<a>foo<![CDATA[bar]]>baz</a>"#).unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    assert_eq!(xot.children(doc_el).count(), 1);
    let txt = xot.text_content_str(doc_el).unwrap();
    assert_eq!(txt, "foobarbaz");
}

#[test]
fn test_span_for_cdata() {
    let mut xot = Xot::new();
    let (doc, span_info) = xot
        .parse_with_span_info(r#"<a><![CDATA[foo]]></a>"#)
        .unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    let txt = xot.first_child(doc_el).unwrap();
    assert_eq!(
        span_info.get(SpanInfoKey::Text(txt)).unwrap(),
        &Span::new(12, 15)
    );
}

#[test]
fn test_span_for_cdata_text() {
    let mut xot = Xot::new();
    let (doc, span_info) = xot
        .parse_with_span_info(r#"<a><![CDATA[foo]]>bar</a>"#)
        .unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    let txt = xot.first_child(doc_el).unwrap();
    assert_eq!(
        span_info.get(SpanInfoKey::Text(txt)).unwrap(),
        &Span::new(12, 21)
    );
}

#[test]
fn test_span_for_text_cdata() {
    let mut xot = Xot::new();
    let (doc, span_info) = xot
        .parse_with_span_info(r#"<a>foo<![CDATA[bar]]></a>"#)
        .unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    let txt = xot.first_child(doc_el).unwrap();
    assert_eq!(
        span_info.get(SpanInfoKey::Text(txt)).unwrap(),
        &Span::new(3, 18)
    );
}

#[test]
fn test_span_for_cdata_cdata() {
    let mut xot = Xot::new();
    let (doc, span_info) = xot
        .parse_with_span_info(r#"<a><![CDATA[foo]]><![CDATA[bar]]></a>"#)
        .unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    let txt = xot.first_child(doc_el).unwrap();
    assert_eq!(
        span_info.get(SpanInfoKey::Text(txt)).unwrap(),
        &Span::new(12, 30)
    );
}

#[test]
fn test_parse_cdata_line_ends() {
    // XML 1.0 section 2.11: a carriage return followed by a newline, and a
    // carriage return that is not, become a newline, in a CDATA section too
    let mut xot = Xot::new();
    let doc = xot
        .parse("<a><![CDATA[x\u{D}\u{A}y\u{D}z\u{D}]]></a>")
        .unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    assert_eq!(xot.text_content_str(doc_el), Some("x\u{A}y\u{A}z\u{A}"));
}

#[test]
fn test_parse_cdata_line_ends_consolidated_with_text() {
    // a carriage return at the end of text or of a CDATA section is not
    // followed by the newline after the CDATA markup, so each becomes a
    // newline of its own
    let mut xot = Xot::new();
    let doc = xot
        .parse("<a>x\u{D}<![CDATA[\u{D}y\u{D}]]>\u{A}z<![CDATA[\u{D}\u{A}]]></a>")
        .unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    assert_eq!(xot.children(doc_el).count(), 1);
    assert_eq!(
        xot.text_content_str(doc_el),
        Some("x\u{A}\u{A}y\u{A}\u{A}z\u{A}")
    );
}

#[test]
fn test_parse_comment_line_ends() {
    let mut xot = Xot::new();
    let doc = xot.parse("<a><!--x\u{D}\u{A}y\u{D}z--></a>").unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    let comment = xot.first_child(doc_el).unwrap();
    assert_eq!(xot.comment_str(comment), Some("x\u{A}y\u{A}z"));
}

#[test]
fn test_parse_processing_instruction_line_ends() {
    let mut xot = Xot::new();
    let doc = xot.parse("<a><?target x\u{D}\u{A}y\u{D}z?></a>").unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    let pi = xot.first_child(doc_el).unwrap();
    assert_eq!(
        xot.processing_instruction(pi).unwrap().data(),
        Some("x\u{A}y\u{A}z")
    );
}

#[test]
fn test_span_for_cdata_with_line_ends() {
    // spans are positions in the source, before line ends are normalized
    let mut xot = Xot::new();
    let (doc, span_info) = xot
        .parse_with_span_info("<a><![CDATA[x\u{D}\u{A}y]]>z</a>")
        .unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    let txt = xot.first_child(doc_el).unwrap();
    assert_eq!(xot.text_str(txt), Some("x\u{A}yz"));
    assert_eq!(
        span_info.get(SpanInfoKey::Text(txt)).unwrap(),
        &Span::new(12, 20)
    );
}

#[test]
fn test_span_for_comment_and_processing_instruction_with_line_ends() {
    let mut xot = Xot::new();
    let (doc, span_info) = xot
        .parse_with_span_info("<a><!--x\u{D}\u{A}y--><?target x\u{D}\u{A}y?></a>")
        .unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    let comment = xot.first_child(doc_el).unwrap();
    let pi = xot.next_sibling(comment).unwrap();
    assert_eq!(
        span_info.get(SpanInfoKey::Comment(comment)).unwrap(),
        &Span::new(7, 11)
    );
    assert_eq!(
        span_info.get(SpanInfoKey::PiContent(pi)).unwrap(),
        &Span::new(23, 27)
    );
}

#[test]
fn test_parse_error_after_cdata_with_line_ends() {
    // the positions of errors after a CDATA section are in the source, and
    // do not shift when line ends in the section are normalized
    let mut xot = Xot::new();
    let err = xot
        .parse("<a><![CDATA[x\u{D}\u{A}y\u{D}\u{A}]]>&unknown;</a>")
        .unwrap_err();
    assert!(matches!(err, ParseError::InvalidEntity(_, _)));
    assert_eq!(err.span(), Span::new(21, 30));
    let err = xot
        .parse("<a><![CDATA[\u{D}\u{A}\u{D}\u{A}]]></b>")
        .unwrap_err();
    assert!(matches!(err, ParseError::InvalidCloseTag(_, _, _)));
    assert_eq!(err.span(), Span::new(21, 22));
}

#[test]
fn test_parse_should_reject_multiple_elements_in_document() {
    let mut xot = Xot::new();
    let err = xot.parse(r#"<a/><b/>"#).unwrap_err();
    assert!(matches!(
        err,
        ParseError::XmlParser(xmlparser::Error::UnknownToken(_), _)
    ));
}

#[test]
fn test_parse_should_reject_no_elements_in_document() {
    let mut xot = Xot::new();
    let err = xot.parse(r#""#).unwrap_err();
    assert!(matches!(err, ParseError::NoElementAtTopLevel(_)))
}

#[test]
fn test_parse_should_reject_text_in_document() {
    let mut xot = Xot::new();
    let err = xot.parse(r#"text"#).unwrap_err();
    assert!(matches!(
        err,
        ParseError::XmlParser(xmlparser::Error::UnknownToken(_), _)
    ));
}

#[test]
fn test_parse_should_reject_no_elements_in_document_with_comment() {
    let mut xot = Xot::new();
    let err = xot.parse(r#"<!-- comment -->"#).unwrap_err();
    assert!(matches!(err, ParseError::NoElementAtTopLevel(_)))
}

#[test]
fn test_parse_fragment_multiple_elements() {
    let mut xot = Xot::new();
    let doc = xot.parse_fragment(r#"<a/><b/>"#).unwrap();
    assert_eq!(xot.children(doc).count(), 2);
}

#[test]
fn test_parse_fragment_no_elements() {
    let mut xot = Xot::new();
    let doc = xot.parse_fragment(r#""#).unwrap();
    assert_eq!(xot.children(doc).count(), 0);
}

#[test]
fn test_parse_fragment_no_elements_just_comment() {
    let mut xot = Xot::new();
    let doc = xot.parse_fragment(r#"<!-- comment -->"#).unwrap();
    assert_eq!(xot.children(doc).count(), 1);
}

#[test]
fn test_parse_fragment_no_elements_just_text() {
    let mut xot = Xot::new();
    let doc = xot.parse_fragment(r#"text"#).unwrap();
    assert_eq!(xot.children(doc).count(), 1);
}

#[test]
fn test_parse_fragment_element_with_text_after() {
    let mut xot = Xot::new();
    let doc = xot.parse_fragment(r#"<a/>text"#).unwrap();
    assert_eq!(xot.children(doc).count(), 2);
}

#[test]
fn test_parse_fragment_error_span() {
    let mut xot = Xot::new();
    let err = xot.parse_fragment(r#"<a><b></a>"#).unwrap_err();
    assert!(matches!(err, ParseError::InvalidCloseTag(_, _, _)));
    let span = err.span();
    assert_eq!(span, (8..9).into());
}

#[test]
fn test_parse_fragment_with_span_info() {
    let mut xot = Xot::new();
    let (doc, span_info) = xot
        .parse_fragment_with_span_info(r#"<a><b></b></a>"#)
        .unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    let b_el = xot.first_child(doc_el).unwrap();
    assert_eq!(
        span_info.get(SpanInfoKey::ElementStart(b_el)).unwrap(),
        &Span::new(4, 5)
    );
}

#[test]
fn test_parse_bytes_too_short_for_declaration() {
    // too short for a byte order mark or an XML declaration
    let mut xot = Xot::new();
    let err = xot.parse_bytes(b"").unwrap_err();
    assert!(matches!(err, ParseError::NoElementAtTopLevel(_)));
    let err = xot.parse_bytes(b"<a>").unwrap_err();
    assert!(matches!(err, ParseError::UnclosedTag(_)));
}

#[test]
fn test_parse_bytes_encoding_attribute_without_declaration() {
    // without a byte order mark or an XML declaration the document is UTF-8,
    // whatever looks like an encoding elsewhere
    let mut xot = Xot::new();
    let doc = xot
        .parse_bytes("<doc encoding=\"x-unknown\">\u{e9}</doc>".as_bytes())
        .unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    assert_eq!(xot.text_content_str(doc_el), Some("\u{e9}"));
    let doc = xot.parse_bytes(b"<doc>charset=none</doc>").unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    assert_eq!(xot.text_content_str(doc_el), Some("charset=none"));
}

#[test]
fn test_parse_bytes_declaration_with_spaces() {
    let mut xot = Xot::new();
    let doc = xot
        .parse_bytes(b"<?xml version=\"1.0\" encoding = \"ISO-8859-1\"?><p>\xe9</p>")
        .unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    assert_eq!(xot.text_content_str(doc_el), Some("\u{e9}"));
}

#[test]
fn test_parse_bytes_utf16() {
    let xml = "\u{FEFF}<?xml version=\"1.0\" encoding=\"UTF-16\"?><p>\u{e9}</p>";
    let data: Vec<u8> = xml.encode_utf16().flat_map(|u| u.to_be_bytes()).collect();
    let mut xot = Xot::new();
    let doc = xot.parse_bytes(&data).unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    assert_eq!(xot.text_content_str(doc_el), Some("\u{e9}"));
}

#[test]
fn test_parse_bytes_bom_with_matching_declaration() {
    let mut xot = Xot::new();
    let doc = xot
        .parse_bytes("\u{FEFF}<?xml version=\"1.0\" encoding=\"UTF-8\"?><p>\u{e9}</p>".as_bytes())
        .unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    assert_eq!(xot.text_content_str(doc_el), Some("\u{e9}"));
    let xml = "\u{FEFF}<?xml version=\"1.0\" encoding=\"UTF-16LE\"?><p>\u{e9}</p>";
    let data: Vec<u8> = xml.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    let doc = xot.parse_bytes(&data).unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    assert_eq!(xot.text_content_str(doc_el), Some("\u{e9}"));
}

#[test]
fn test_parse_bytes_utf16_without_bom() {
    // the declaration and the way `<?` is written show the byte order
    let xml = "<?xml version=\"1.0\" encoding=\"UTF-16\"?><p>\u{e9}</p>";
    let data: Vec<u8> = xml.encode_utf16().flat_map(|u| u.to_be_bytes()).collect();
    let mut xot = Xot::new();
    let doc = xot.parse_bytes(&data).unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    assert_eq!(xot.text_content_str(doc_el), Some("\u{e9}"));
}

/// The namespace that `prefix` is declared for on the document element of
/// `xml`.
fn declared_namespace(xml: &str, prefix: &str) -> String {
    let mut xot = Xot::new();
    let doc = xot.parse(xml).unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    let prefix = xot.add_prefix(prefix);
    let namespace = xot.namespaces(doc_el).get(prefix).copied().unwrap();
    xot.namespace_str(namespace).to_string()
}

#[test]
fn test_parse_namespace_declaration_with_entity_references() {
    // the namespace is the value of the attribute, as with any attribute
    assert_eq!(
        declared_namespace(r#"<a xmlns:p="urn:x?a=1&amp;b=2"/>"#, "p"),
        "urn:x?a=1&b=2"
    );
    assert_eq!(
        declared_namespace(r#"<a xmlns:p="urn:&lt;&gt;&quot;&apos;"/>"#, "p"),
        "urn:<>\"'"
    );
    assert_eq!(
        declared_namespace(r#"<a xmlns="urn:a&amp;b"/>"#, ""),
        "urn:a&b"
    );
}

#[test]
fn test_parse_namespace_declaration_with_character_references() {
    assert_eq!(
        declared_namespace(r#"<a xmlns:p="urn:&#x41;&#66;"/>"#, "p"),
        "urn:AB"
    );
    // a character reference to a whitespace character is not normalized
    assert_eq!(
        declared_namespace(r#"<a xmlns="urn:a&#x9;b&#10;c"/>"#, ""),
        "urn:a\u{9}b\u{A}c"
    );
}

#[test]
fn test_parse_namespace_declaration_normalized() {
    // the value is normalized like that of any attribute: whitespace
    // characters become spaces
    assert_eq!(
        declared_namespace("<a xmlns:p=\"urn:a\u{9}b\u{D}\u{A}c\"/>", "p"),
        "urn:a b c"
    );
}

#[test]
fn test_parse_names_in_namespace_with_entity_references() {
    let mut xot = Xot::new();
    let doc = xot
        .parse(r#"<p:a xmlns:p="urn:&amp;" xmlns="urn:&lt;" p:b="1"><c/></p:a>"#)
        .unwrap();
    let a = xot.document_element(doc).unwrap();
    let c = xot.first_child(a).unwrap();
    let amp = xot.namespace("urn:&").unwrap();
    let lt = xot.namespace("urn:<").unwrap();
    assert_eq!(xot.namespace_for_name(xot.element(a).unwrap().name()), amp);
    assert_eq!(xot.namespace_for_name(xot.element(c).unwrap().name()), lt);
    let b = xot.name_ns("b", amp).unwrap();
    assert_eq!(xot.get_attribute(a, b), Some("1"));
}

#[test]
fn test_parse_namespace_declaration_with_invalid_entity() {
    let mut xot = Xot::new();
    let err = xot.parse(r#"<a xmlns:p="urn:&unknown;"/>"#).unwrap_err();
    assert!(matches!(err, ParseError::InvalidEntity(ref name, _) if name == "unknown"));
    assert_eq!(err.span(), Span::new(16, 25));
    let err = xot.parse(r#"<a xmlns="urn:&amp"/>"#).unwrap_err();
    assert!(matches!(err, ParseError::UnclosedEntity(_, 14)));
}
