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

/// An XML document with an encoding declaration for `encoding`, and `bytes`
/// as the content of its document element.
fn declared_encoding_document(encoding: &str, bytes: &[u8]) -> Vec<u8> {
    let mut xml = format!("<?xml version=\"1.0\" encoding=\"{}\"?><p>", encoding).into_bytes();
    xml.extend(bytes);
    xml.extend(b"</p>");
    xml
}

#[test]
fn test_parse_bytes_iso_8859_1() {
    // each byte is the character with the same number; the bytes 0x80 to
    // 0x9F are the C1 control characters, not the characters that
    // windows-1252 has for them
    let mut xot = Xot::new();
    let doc = xot
        .parse_bytes(
            b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><p a=\"\x80\x9f\">\x80\x9f\xa0\xe9\xff</p>",
        )
        .unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    assert_eq!(
        xot.text_content_str(doc_el),
        Some("\u{80}\u{9f}\u{a0}\u{e9}\u{ff}")
    );
    let a = xot.add_name("a");
    assert_eq!(xot.get_attribute(doc_el, a), Some("\u{80}\u{9f}"));
}

#[test]
fn test_parse_bytes_iso_8859_1_names() {
    // names IANA registers for ISO-8859-1, in any case; ISO_8859-1:1987 is
    // one too, but `:` cannot be in an encoding declaration
    let mut xot = Xot::new();
    for name in [
        "ISO-8859-1",
        "iso-8859-1",
        "ISO_8859-1",
        "latin1",
        "IBM819",
        "csISOLatin1",
    ] {
        let doc = xot
            .parse_bytes(&declared_encoding_document(name, b"\x80"))
            .unwrap();
        let doc_el = xot.document_element(doc).unwrap();
        assert_eq!(xot.text_content_str(doc_el), Some("\u{80}"), "{}", name);
    }
    // an encoding declaration with `:` in the name (production [81] EncName
    // does not allow it) is not well-formed
    for name in ["ISO_8859-1:1987", "ISO_646.irv:1991"] {
        assert!(
            xot.parse_bytes(&declared_encoding_document(name, b"\x80"))
                .is_err(),
            "{}",
            name
        );
    }
}

/// The text of the document element of a document with an encoding
/// declaration for `encoding`, which has `bytes` as its content.
fn declared_encoding_text(xot: &mut Xot, encoding: &str, bytes: &[u8]) -> String {
    let doc = xot
        .parse_bytes(&declared_encoding_document(encoding, bytes))
        .unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    xot.text_content_str(doc_el).unwrap().to_string()
}

#[test]
fn test_parse_bytes_iso_8859_0x80_to_0x9f() {
    // in ISO 8859 the bytes 0x80 to 0x9F are the control characters U+0080
    // to U+009F, not the characters such as `€` that windows-1252,
    // windows-1254 and windows-874 have for most of them
    let mut xot = Xot::new();
    assert_eq!(
        declared_encoding_text(&mut xot, "ISO-8859-9", b"\x80"),
        "\u{80}"
    );
    assert_eq!(
        declared_encoding_text(&mut xot, "latin1", b"\x85"),
        "\u{85}"
    );
    assert_eq!(
        declared_encoding_text(&mut xot, "ISO-8859-11", b"\x80\x85\x91\x9f"),
        "\u{80}\u{85}\u{91}\u{9f}"
    );
    assert_eq!(
        declared_encoding_text(&mut xot, "windows-1254", b"\x80"),
        "\u{20AC}"
    );
}

#[test]
fn test_parse_bytes_iso_8859_9() {
    // ISO-8859-1 with six Turkish letters; names IANA registers for
    // ISO-8859-9, in any case
    let mut xot = Xot::new();
    for name in [
        "ISO-8859-9",
        "iso-8859-9",
        "ISO_8859-9",
        "latin5",
        "csISOLatin5",
    ] {
        assert_eq!(
            declared_encoding_text(
                &mut xot,
                name,
                &[0x80, 0xD0, 0xDD, 0xDE, 0xE9, 0xF0, 0xFD, 0xFE]
            ),
            "\u{80}\u{11E}\u{130}\u{15E}\u{E9}\u{11F}\u{131}\u{15F}",
            "{}",
            name
        );
    }
}

#[test]
fn test_parse_bytes_tis_620() {
    // the names IANA registers for TIS-620, in any case
    let mut xot = Xot::new();
    for name in ["TIS-620", "tis-620", "csTIS620", "ISO-8859-11"] {
        assert_eq!(
            declared_encoding_text(&mut xot, name, b"\x80\xa0\xa1\xda\xdf\xfb"),
            "\u{80}\u{A0}\u{E01}\u{E3A}\u{E3F}\u{E5B}",
            "{}",
            name
        );
    }
}

#[test]
fn test_parse_bytes_us_ascii() {
    // names IANA registers for US-ASCII, and ASCII, in any case;
    // ISO_646.irv:1991 is one too, but `:` cannot be in an encoding
    // declaration
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
        let doc = xot
            .parse_bytes(&declared_encoding_document(name, b"caf\x65 \x7f"))
            .unwrap();
        let doc_el = xot.document_element(doc).unwrap();
        assert_eq!(
            xot.text_content_str(doc_el),
            Some("cafe \u{7f}"),
            "{}",
            name
        );
    }
}

#[test]
fn test_parse_bytes_windows_1252() {
    // windows-1252 has the euro sign at 0x80
    let mut xot = Xot::new();
    for name in ["windows-1252", "Windows-1252"] {
        let doc = xot
            .parse_bytes(&declared_encoding_document(name, b"\x80"))
            .unwrap();
        let doc_el = xot.document_element(doc).unwrap();
        assert_eq!(xot.text_content_str(doc_el), Some("\u{20AC}"), "{}", name);
    }
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

// Only `xmlns` and `xmlns:NCName` declare namespaces (Namespaces in XML 1.0,
// section 3); `p:xmlns` is an attribute with the local name `xmlns` in the
// namespace bound to `p`

#[test]
fn test_parse_prefixed_xmlns_is_attribute() {
    let mut xot = Xot::new();
    let doc = xot
        .parse(r#"<a xmlns:p="urn:p" p:xmlns="urn:v"><b/></a>"#)
        .unwrap();
    let a = xot.document_element(doc).unwrap();
    let b = xot.first_child(a).unwrap();
    // there is no default namespace, so the elements are in no namespace
    assert_eq!(xot.name_ns_str(xot.element(a).unwrap().name()), ("a", ""));
    assert_eq!(xot.name_ns_str(xot.element(b).unwrap().name()), ("b", ""));
    // `p` is the only prefix declared
    let p = xot.prefix("p").unwrap();
    let urn_p = xot.namespace("urn:p").unwrap();
    let declarations: Vec<_> = xot.namespaces(a).iter().map(|(p, n)| (p, *n)).collect();
    assert_eq!(declarations, vec![(p, urn_p)]);
    // `p:xmlns` is the only attribute
    let xmlns = xot.name_ns("xmlns", urn_p).unwrap();
    let attributes: Vec<_> = xot
        .attributes(a)
        .iter()
        .map(|(name, value)| (name, value.clone()))
        .collect();
    assert_eq!(attributes, vec![(xmlns, "urn:v".to_string())]);
}

#[test]
fn test_parse_prefixed_xmlns_with_default_namespace() {
    // `p:xmlns` does not take the place of the default namespace declaration
    let mut xot = Xot::new();
    let doc = xot
        .parse(r#"<a xmlns="urn:d" xmlns:p="urn:p" p:xmlns="urn:v"/>"#)
        .unwrap();
    let a = xot.document_element(doc).unwrap();
    assert_eq!(
        xot.name_ns_str(xot.element(a).unwrap().name()),
        ("a", "urn:d")
    );
    let urn_p = xot.namespace("urn:p").unwrap();
    let xmlns = xot.name_ns("xmlns", urn_p).unwrap();
    assert_eq!(xot.get_attribute(a, xmlns), Some("urn:v"));
}

#[test]
fn test_parse_xml_xmlns_is_attribute() {
    // the `xml` prefix is bound without a declaration
    let mut xot = Xot::new();
    let doc = xot.parse(r#"<a xml:xmlns="urn:v"/>"#).unwrap();
    let a = xot.document_element(doc).unwrap();
    assert_eq!(xot.name_ns_str(xot.element(a).unwrap().name()), ("a", ""));
    assert_eq!(xot.namespaces(a).iter().count(), 0);
    let xmlns = xot.name_ns("xmlns", xot.xml_namespace()).unwrap();
    assert_eq!(xot.get_attribute(a, xmlns), Some("urn:v"));
}

#[test]
fn test_parse_prefixed_xmlns_with_unknown_prefix() {
    // as for any other attribute, the prefix must be declared
    let mut xot = Xot::new();
    let err = xot.parse(r#"<a q:xmlns="urn:v"/>"#).unwrap_err();
    assert!(matches!(err, ParseError::UnknownPrefix(ref prefix, _) if prefix == "q"));
    assert_eq!(err.span(), Span::new(3, 4));
}

#[test]
fn test_parse_prefixed_xmlns_duplicate() {
    let mut xot = Xot::new();
    let err = xot
        .parse(r#"<a xmlns:p="urn:p" p:xmlns="1" p:xmlns="2"/>"#)
        .unwrap_err();
    assert!(matches!(err, ParseError::DuplicateAttribute(ref name, _) if name == "p:xmlns"));
    assert_eq!(err.span(), Span::new(31, 38));
}

#[test]
fn test_parse_allowed_namespace_declarations() {
    // `xml` may be declared for its own namespace, and the default namespace
    // may be undeclared
    let mut xot = Xot::new();
    let doc = xot
        .parse(concat!(
            r#"<a xmlns="urn:d" xmlns:xml="http://www.w3.org/XML/1998/namespace">"#,
            r#"<b xmlns="" xml:lang="en"/></a>"#
        ))
        .unwrap();
    let a = xot.document_element(doc).unwrap();
    let b = xot.first_child(a).unwrap();
    assert_eq!(xot.name_ns_str(xot.element(b).unwrap().name()), ("b", ""));
    let lang = xot.name_ns("lang", xot.xml_namespace()).unwrap();
    assert_eq!(xot.get_attribute(b, lang), Some("en"));
}

#[test]
fn test_parse_same_local_name_in_default_namespace() {
    // an unprefixed attribute is in no namespace, so it does not have the
    // same expanded name as a prefixed one (Namespaces in XML 1.0, section
    // 6.3)
    let mut xot = Xot::new();
    let doc = xot
        .parse(concat!(
            r#"<x xmlns:n1="http://www.w3.org" xmlns="http://www.w3.org">"#,
            r#"<good a="1" n1:a="2"/></x>"#
        ))
        .unwrap();
    let x = xot.document_element(doc).unwrap();
    let good = xot.first_child(x).unwrap();
    assert_eq!(xot.attributes(good).len(), 2);
}

#[test]
fn test_parse_processing_instruction_targets_starting_with_xml() {
    // only `xml` itself is not a target; other names starting with `xml`
    // are
    let mut xot = Xot::new();
    let doc = xot
        .parse(r#"<?xml-stylesheet href="a.css"?><?xmlfoo?><doc/>"#)
        .unwrap();
    let targets: Vec<_> = xot
        .children(doc)
        .filter_map(|node| xot.processing_instruction(node))
        .map(|pi| xot.local_name_str(pi.target()).to_string())
        .collect();
    assert_eq!(targets, ["xml-stylesheet", "xmlfoo"]);
}

#[test]
fn test_parse_declaration_with_other_white_space() {
    // `<?xml` can be followed by any white space (XML 1.0, production [23]
    // XMLDecl), not just a space
    let mut xot = Xot::new();
    for xml in [
        "<?xml\tversion=\"1.0\"?><doc/>",
        "<?xml\nversion=\"1.0\"?><doc/>",
        "<?xml\r\n  version=\"1.0\"\tencoding=\"UTF-8\"?><doc/>",
    ] {
        let doc = xot.parse(xml).unwrap();
        // the declaration is not a node
        assert_eq!(xot.children(doc).count(), 1, "{:?}", xml);
        assert_eq!(xot.to_string(doc).unwrap(), "<doc/>");
    }
    // it is read as a declaration
    let err = xot.parse("<?xml\tversion=\"1.1\"?><doc/>").unwrap_err();
    assert!(matches!(err, ParseError::UnsupportedVersion(ref v, _) if v == "1.1"));
    assert_eq!(err.span(), Span::new(15, 18));
    // in bytes too, with the encoding it declares
    let doc = xot
        .parse_bytes(b"<?xml\tversion=\"1.0\"\tencoding=\"ISO-8859-1\"?><p>\xe9</p>")
        .unwrap();
    let p = xot.document_element(doc).unwrap();
    assert_eq!(xot.text_content_str(p), Some("\u{e9}"));
}

#[test]
fn test_parse_declaration_with_other_white_space_after_byte_order_mark() {
    // the tokenizer skips a byte order mark before the declaration
    let mut xot = Xot::new();
    for xml in [
        "\u{FEFF}<?xml\tversion=\"1.0\"?><doc/>",
        "\u{FEFF}<?xml\nversion=\"1.0\"?><doc/>",
        "\u{FEFF}<?xml\r\nversion=\"1.0\"?><doc/>",
    ] {
        let doc = xot.parse(xml).unwrap();
        assert_eq!(xot.children(doc).count(), 1, "{:?}", xml);
        assert_eq!(xot.to_string(doc).unwrap(), "<doc/>");
    }
    let err = xot
        .parse("\u{FEFF}<?xml\nversion=\"1.1\"?><doc/>")
        .unwrap_err();
    assert!(matches!(err, ParseError::UnsupportedVersion(ref v, _) if v == "1.1"));
    assert_eq!(err.span(), Span::new(18, 21));
    // in bytes, where the byte order mark is not part of the decoded text
    for bytes in [
        &b"\xEF\xBB\xBF<?xml\tversion=\"1.0\" encoding=\"UTF-8\"?><p>\xc3\xa9</p>"[..],
        &b"\xEF\xBB\xBF<?xml\nversion=\"1.0\" encoding=\"UTF-8\"?><p>\xc3\xa9</p>"[..],
    ] {
        let doc = xot.parse_bytes(bytes).unwrap();
        assert_eq!(xot.children(doc).count(), 1);
        let p = xot.document_element(doc).unwrap();
        assert_eq!(xot.text_content_str(p), Some("\u{e9}"));
    }
}
