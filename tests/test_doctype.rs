//! A document type declaration is accepted and ignored: nothing external is
//! read and the internal subset is not applied, but a document that depends
//! on it gives `ParseError::DtdUnsupported`.

use xot::{Node, ParseError, Span, SpanInfo, SpanInfoKey, Xot};

/// `xml` with `part` replaced by as many spaces as it has bytes.
fn blank(xml: &str, part: &str) -> String {
    assert!(xml.contains(part), "{:?} is not in {:?}", part, xml);
    xml.replacen(part, &" ".repeat(part.len()), 1)
}

/// The spans of `node` and its descendants, in document order, with the
/// kind of each node.
fn spans(xot: &Xot, document: Node, span_info: &SpanInfo) -> Vec<(String, Option<Span>)> {
    let mut result = Vec::new();
    for node in xot.descendants(document) {
        let mut push = |kind: &str, key: SpanInfoKey| {
            result.push((kind.to_string(), span_info.get(key).copied()));
        };
        if xot.is_element(node) {
            push("start", SpanInfoKey::ElementStart(node));
            push("end", SpanInfoKey::ElementEnd(node));
            for name in xot.attributes(node).keys() {
                push("attribute name", SpanInfoKey::AttributeName(node, name));
                push("attribute value", SpanInfoKey::AttributeValue(node, name));
            }
        } else if xot.is_text(node) {
            push("text", SpanInfoKey::Text(node));
        } else if xot.is_comment(node) {
            push("comment", SpanInfoKey::Comment(node));
        } else if xot.is_processing_instruction(node) {
            push("pi target", SpanInfoKey::PiTarget(node));
            push("pi content", SpanInfoKey::PiContent(node));
        } else {
            push("document", SpanInfoKey::ElementStart(node));
        }
    }
    result
}

/// `xml` parses, and gives the tree and the spans that it gives with the
/// document type declaration `doctype` replaced by spaces.
fn accepted(xml: &str, doctype: &str) {
    let mut xot = Xot::new();
    let (document, span_info) = xot
        .parse_with_span_info(xml)
        .unwrap_or_else(|e| panic!("{:?} should parse: {:?}", xml, e));
    let without = blank(xml, doctype);
    let (expected, expected_span_info) = xot.parse_with_span_info(&without).unwrap();
    assert_eq!(
        xot.to_string(document).unwrap(),
        xot.to_string(expected).unwrap()
    );
    assert!(xot.deep_equal(document, expected));
    assert_eq!(
        spans(&xot, document, &span_info),
        spans(&xot, expected, &expected_span_info)
    );
}

/// The error of parsing `xml`.
fn error(xml: &str) -> ParseError {
    let mut xot = Xot::new();
    match xot.parse(xml) {
        Ok(_) => panic!("{:?} should not parse", xml),
        Err(e) => e,
    }
}

/// The span of the first `part` in `xml`.
fn span_of(xml: &str, part: &str) -> Span {
    let start = xml
        .find(part)
        .unwrap_or_else(|| panic!("{:?} is not in {:?}", part, xml));
    Span::new(start, start + part.len())
}

/// Parsing `xml` gives `DtdUnsupported` with the span of the first `part`.
fn unsupported_at(xml: &str, part: &str) {
    match error(xml) {
        ParseError::DtdUnsupported(span) => assert_eq!(span, span_of(xml, part), "{:?}", xml),
        other => panic!("{:?}: expected DtdUnsupported, got {:?}", xml, other),
    }
}

#[test]
fn test_doctype_with_a_name_only() {
    accepted("<!DOCTYPE doc><doc/>", "<!DOCTYPE doc>");
    accepted("<!DOCTYPE  doc ><doc/>", "<!DOCTYPE  doc >");
}

#[test]
fn test_doctype_with_an_external_identifier() {
    let doctype = r#"<!DOCTYPE doc SYSTEM "doc.dtd">"#;
    accepted(&format!("{}<doc/>", doctype), doctype);
    let doctype = r#"<!DOCTYPE doc PUBLIC "-//Example//DTD Doc//EN" 'doc.dtd'>"#;
    accepted(&format!("{}<doc/>", doctype), doctype);
    let doctype = r#"<!DOCTYPE doc SYSTEM "a]>b" [ ]>"#;
    accepted(&format!("{}<doc/>", doctype), doctype);
}

#[test]
fn test_doctype_name_other_than_the_document_element() {
    accepted("<!DOCTYPE other><doc/>", "<!DOCTYPE other>");
}

#[test]
fn test_doctype_with_an_empty_internal_subset() {
    for doctype in [
        "<!DOCTYPE doc []>",
        "<!DOCTYPE doc [ ]>",
        "<!DOCTYPE doc[\n]\n>",
        "<!DOCTYPE doc\tSYSTEM 'doc.dtd'\r\n[\t]  >",
    ] {
        accepted(&format!("{}\n<doc/>", doctype), doctype);
    }
}

#[test]
fn test_doctype_with_declarations() {
    let doctype = r#"<!DOCTYPE doc [
<!ELEMENT doc (item*)>
<!ELEMENT item (#PCDATA)>
<!ATTLIST item key ID #REQUIRED kind (a|b) #IMPLIED ref IDREF #IMPLIED note CDATA #IMPLIED>
<!NOTATION n SYSTEM "urn:example:n">
<!ENTITY e "value">
<!ENTITY % p "x">
<!ENTITY u SYSTEM "u.bin" NDATA n>
<!-- inside -->
<?pi inside?>
]>"#;
    let xml = format!(
        "<?xml version=\"1.0\"?>\n<!-- before -->\n{}\n<?after x?>\n<doc><item key=\"a1\" kind=\"a\" ref=\"a1\" note=\" n \">t&amp;&#65;</item></doc>",
        doctype
    );
    accepted(&xml, doctype);
    // the comment and the processing instruction of the prolog are kept, but
    // not those of the internal subset
    let mut xot = Xot::new();
    let document = xot.parse(&xml).unwrap();
    assert_eq!(
        xot.to_string(document).unwrap(),
        "<!-- before --><?after x?><doc><item key=\"a1\" kind=\"a\" ref=\"a1\" note=\" n \">t&amp;A</item></doc>"
    );
}

#[test]
fn test_doctype_brackets_and_closes_in_quotes() {
    let doctype = r#"<!DOCTYPE doc [<!ENTITY a "]>"><!ENTITY b SYSTEM ']>'><!ENTITY c PUBLIC "-//X//Y" "c]>">]>"#;
    accepted(&format!("{}<doc/>", doctype), doctype);
}

#[test]
fn test_doctype_typed_attributes_that_need_no_normalization() {
    let doctype = "<!DOCTYPE doc [<!ATTLIST doc key ID #IMPLIED keys NMTOKENS #IMPLIED>\
                   <!ATTLIST p:item p:ref IDREF #IMPLIED>]>";
    let xml = format!(
        "{}<doc key=\"a1\" keys=\"a b\"><p:item xmlns:p=\"urn:example:p\" p:ref=\"a1\"/></doc>",
        doctype
    );
    accepted(&xml, doctype);
    // a tab from a character reference is not a space
    let xml = format!("{}<doc key=\"&#9;a1\"/>", doctype);
    accepted(&xml, doctype);
}

#[test]
fn test_doctype_utf16_with_byte_order_mark() {
    let xml =
        "<?xml version=\"1.0\" encoding=\"UTF-16\"?><!DOCTYPE doc [<!ENTITY e \"v\">]><doc>é</doc>";
    let mut bytes = vec![0xFF, 0xFE];
    for u in xml.encode_utf16() {
        bytes.extend_from_slice(&u.to_le_bytes());
    }
    let mut xot = Xot::new();
    let document = xot.parse_bytes(&bytes).unwrap();
    assert_eq!(xot.to_string(document).unwrap(), "<doc>é</doc>");
}

#[test]
fn test_doctype_declared_entity_reference() {
    let doctype = r#"<!DOCTYPE doc [<!ENTITY e "v">]>"#;
    unsupported_at(&format!("{}<doc>a&e;b</doc>", doctype), "&e;");
    unsupported_at(&format!("{}<doc a=\"x&e;\"/>", doctype), "&e;");
    unsupported_at(
        &format!("{}<doc xmlns:p=\"urn:example:&e;\"/>", doctype),
        "&e;",
    );
    // an entity that is not declared stays unknown
    match error(&format!("{}<doc>&f;</doc>", doctype)) {
        ParseError::InvalidEntity(name, span) => {
            assert_eq!(name, "f");
            assert_eq!(span, Span::new(doctype.len() + 5, doctype.len() + 8));
        }
        other => panic!("expected InvalidEntity, got {:?}", other),
    }
}

#[test]
fn test_doctype_external_subset_entity_reference() {
    unsupported_at(
        r#"<!DOCTYPE doc SYSTEM "doc.dtd"><doc>&other;</doc>"#,
        "&other;",
    );
    unsupported_at(
        r#"<!DOCTYPE doc PUBLIC "-//X//Y" "doc.dtd" []><doc a="&other;"/>"#,
        "&other;",
    );
    // a character reference keeps its error
    let xml = r#"<!DOCTYPE doc SYSTEM "doc.dtd"><doc>&#xQQ;</doc>"#;
    match error(xml) {
        ParseError::InvalidEntity(name, span) => {
            assert_eq!(name, "xQQ");
            assert_eq!(span, span_of(xml, "&#xQQ;"));
        }
        other => panic!("expected InvalidEntity, got {:?}", other),
    }
}

#[test]
fn test_unknown_entity_without_doctype() {
    let xml = "<doc>&other;</doc>";
    match error(xml) {
        ParseError::InvalidEntity(name, span) => {
            assert_eq!(name, "other");
            assert_eq!(span, Span::new(5, 12));
        }
        other => panic!("expected InvalidEntity, got {:?}", other),
    }
}

#[test]
fn test_doctype_attribute_default_values() {
    for declaration in [
        r#"<!ATTLIST doc a CDATA "v">"#,
        "<!ATTLIST doc a CDATA 'v'>",
        "<!ATTLIST doc a CDATA #FIXED 'v'>",
        r#"<!ATTLIST doc xmlns:p CDATA #FIXED "urn:example:p">"#,
        r#"<!ATTLIST doc a ID #IMPLIED b (x|y) "x">"#,
    ] {
        let xml = format!("<!DOCTYPE doc [<!ELEMENT doc ANY>{}]><doc/>", declaration);
        unsupported_at(&xml, declaration);
    }
}

#[test]
fn test_doctype_parameter_entity_references() {
    unsupported_at(r#"<!DOCTYPE doc [<!ENTITY % p "x"> %p; ]><doc/>"#, "%p;");
    unsupported_at(r#"<!DOCTYPE doc [<!ENTITY a "x%p;">]><doc/>"#, "%p;");
    unsupported_at(r#"<!DOCTYPE doc [<!ELEMENT doc %p;>]><doc/>"#, "%p;");
}

#[test]
fn test_doctype_typed_attribute_values_that_need_normalization() {
    let doctype =
        "<!DOCTYPE doc [<!ATTLIST doc key ID #IMPLIED keys NMTOKENS #IMPLIED note CDATA #IMPLIED>\
                   <!ATTLIST p:item p:ref IDREF #IMPLIED>]>";
    unsupported_at(&format!("{}<doc key=\" x \"/>", doctype), " x ");
    unsupported_at(&format!("{}<doc keys=\"a  b\"/>", doctype), "a  b");
    unsupported_at(&format!("{}<doc keys=\"a&#10;b \"/>", doctype), "a&#10;b ");
    unsupported_at(
        &format!(
            "{}<doc><p:item xmlns:p=\"urn:example:p\" p:ref=\"r \"/></doc>",
            doctype
        ),
        "r ",
    );
    // a CDATA attribute is not normalized further
    accepted(&format!("{}<doc note=\" x \"/>", doctype), doctype);
    // the same names on another element are not declared
    accepted(
        &format!("{}<doc><key key=\" x \"/></doc>", doctype),
        doctype,
    );
}

#[test]
fn test_doctype_unreadable_attlist() {
    let declaration = "<!ATTLIST doc a TEXT #IMPLIED>";
    unsupported_at(
        &format!("<!DOCTYPE doc [{}]><doc/>", declaration),
        declaration,
    );
}

#[test]
fn test_doctype_xml_processing_instruction_in_internal_subset() {
    let xml = "<!DOCTYPE doc [<?XML x?>]><doc/>";
    match error(xml) {
        ParseError::InvalidPiTarget(target, span) => {
            assert_eq!(target, "XML");
            assert_eq!(span, span_of(xml, "XML"));
        }
        other => panic!("expected InvalidPiTarget, got {:?}", other),
    }
}

#[test]
fn test_doctype_errors_of_the_tokenizer() {
    // a second document type declaration
    assert!(matches!(
        error("<!DOCTYPE doc><!DOCTYPE doc><doc/>"),
        ParseError::XmlParser(..)
    ));
    // a document type declaration after the document element
    assert!(matches!(
        error("<doc/><!DOCTYPE doc>"),
        ParseError::XmlParser(..)
    ));
    // a conditional section, which an internal subset cannot have
    assert!(matches!(
        error("<!DOCTYPE doc [<![INCLUDE[<!ELEMENT doc ANY>]]>]><doc/>"),
        ParseError::XmlParser(..)
    ));
    // an entity declaration that is not well-formed
    assert!(matches!(
        error("<!DOCTYPE doc [<!ENTITY e>]><doc/>"),
        ParseError::XmlParser(..)
    ));
}

/// Parsing `xml` gives the tokenizer's error for a malformed external
/// identifier, at the first `part` in `xml`.
fn invalid_public_id_at(xml: &str, part: &str) {
    match error(xml) {
        ParseError::XmlParser(e, position) => {
            assert_eq!(position, span_of(xml, part).start, "{:?}", xml);
            assert!(
                matches!(
                    e,
                    xmlparser::Error::InvalidDoctype(xmlparser::StreamError::InvalidExternalID, _)
                        | xmlparser::Error::InvalidEntity(
                            xmlparser::StreamError::InvalidExternalID,
                            _
                        )
                ),
                "{:?}: {:?}",
                xml,
                e
            );
        }
        other => panic!("{:?}: expected XmlParser, got {:?}", xml, other),
    }
}

#[test]
fn test_doctype_public_identifier_characters() {
    // every PubidChar, and a quote of the other kind
    let doctype = "<!DOCTYPE doc PUBLIC \"-'()+,./:=?;!*#@$_% \r\nazAZ09\" 'doc.dtd'>";
    accepted(&format!("{}<doc/>", doctype), doctype);
    let doctype = r#"<!DOCTYPE doc PUBLIC "" "doc.dtd" []>"#;
    accepted(&format!("{}<doc/>", doctype), doctype);
    let doctype = r#"<!DOCTYPE doc [<!ENTITY e PUBLIC "-//X//Y" "e.xml"><!NOTATION n PUBLIC '-//X//N'><!NOTATION m PUBLIC "-//X//M" "m">]>"#;
    accepted(&format!("{}<doc/>", doctype), doctype);
    // characters that are not PubidChars
    invalid_public_id_at(r#"<!DOCTYPE doc PUBLIC "{" "doc.dtd"><doc/>"#, "{");
    invalid_public_id_at(r#"<!DOCTYPE doc PUBLIC "a~b" "doc.dtd" []><doc/>"#, "~");
    invalid_public_id_at("<!DOCTYPE doc PUBLIC \"-\t-\" \"doc.dtd\"><doc/>", "\t");
    invalid_public_id_at(r#"<!DOCTYPE doc PUBLIC 'a"b' "doc.dtd"><doc/>"#, "\"b'");
    invalid_public_id_at(r#"<!DOCTYPE doc PUBLIC "a&b" "doc.dtd"><doc/>"#, "&");
    invalid_public_id_at(
        "<!DOCTYPE doc PUBLIC \"\u{e9}\" \"doc.dtd\"><doc/>",
        "\u{e9}",
    );
    invalid_public_id_at(
        r#"<!DOCTYPE doc [<!ENTITY e PUBLIC "a<b" "e.xml">]><doc/>"#,
        "<b",
    );
    invalid_public_id_at(
        r#"<!DOCTYPE doc [<!ENTITY % p PUBLIC "a^" "p.dtd">]><doc/>"#,
        "^",
    );
    invalid_public_id_at(
        r#"<!DOCTYPE doc [<!ENTITY e PUBLIC "a|" "e" NDATA n>]><doc/>"#,
        "|",
    );
    invalid_public_id_at(r#"<!DOCTYPE doc [<!NOTATION n PUBLIC "{">]><doc/>"#, "{");
    invalid_public_id_at(
        "<?xml version='1.0'\n?>\n<!DOCTYPE doc [\n<!NOTATION n PUBLIC 'a\\b' 'n'>]><doc/>",
        "\\b",
    );
    // a system literal can have any character
    let doctype =
        r#"<!DOCTYPE doc SYSTEM "{~}" [<!ENTITY e SYSTEM "{~}"><!NOTATION n SYSTEM "{~}">]>"#;
    accepted(&format!("{}<doc/>", doctype), doctype);
}

#[test]
fn test_doctype_unterminated_internal_subset() {
    let xml = r#"<!DOCTYPE doc [<!ENTITY a "b">"#;
    match error(xml) {
        ParseError::DtdUnsupported(span) => assert_eq!(span, Span::new(0, 15)),
        other => panic!("expected DtdUnsupported, got {:?}", other),
    }
}

#[test]
fn test_doctype_in_fragment() {
    let mut xot = Xot::new();
    assert!(matches!(
        xot.parse_fragment("<!DOCTYPE doc><doc/>"),
        Err(ParseError::XmlParser(..))
    ));
}
