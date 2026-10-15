use proptest::prelude::*;
use xot::{output, output::TokenSerializeParameters, Error, Node, Value, Xot};

#[test]
fn test_escape_in_text() {
    let mut xot = Xot::new();
    let doc = xot.parse(r#"<a>&lt;</a>"#).unwrap();
    let text_id = xot.first_child(xot.document_element(doc).unwrap()).unwrap();
    assert!(matches!(xot.value(text_id), Value::Text(_)));
    match xot.value(text_id) {
        Value::Text(text) => {
            assert_eq!(text.get(), "<");
        }
        _ => unreachable!(),
    }
}

#[test]
fn test_add_attribute_entities() {
    let mut xot = Xot::new();
    let doc = xot.parse(r#"<doc/>"#).unwrap();
    let el_id = xot.document_element(doc).unwrap();
    assert!(xot.name("z").is_none());
    let a = xot.add_name("z");

    let mut attributes = xot.attributes_mut(el_id);
    attributes.insert(a, "Created & set".to_string());
    assert_eq!(
        xot.to_string(doc).unwrap(),
        r#"<doc z="Created &amp; set"/>"#
    );
}

// a value with all the characters that need care when serialized
const AWKWARD: &str =
    "a\u{9}b\u{A}c\u{D}d\u{D}\u{A}e\u{85}f\u{2028}g\u{7F}h\u{9F}i\u{A0}j&<>\"']]>k";

/// A document `<doc a="{value}">{value}</doc>`.
fn document(xot: &mut Xot, value: &str) -> (Node, Node) {
    let doc_name = xot.add_name("doc");
    let a = xot.add_name("a");
    let doc_el = xot.new_element(doc_name);
    xot.set_attribute(doc_el, a, value);
    xot.append_text(doc_el, value).unwrap();
    let doc = xot.new_document_with_element(doc_el).unwrap();
    (doc, doc_el)
}

/// The text content and attribute value of the document element after
/// parsing `xml`.
fn parse_values(xot: &mut Xot, xml: &str) -> (String, String) {
    let doc = xot.parse(xml).unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    let a = xot.add_name("a");
    let text = xot.text_content_str(doc_el).unwrap().to_string();
    let attribute = xot.get_attribute(doc_el, a).unwrap().to_string();
    (text, attribute)
}

#[test]
fn test_round_trip_text_and_attribute() {
    let mut xot = Xot::new();
    let (doc, _) = document(&mut xot, AWKWARD);
    let serialized = xot.to_string(doc).unwrap();
    assert_eq!(
        serialized,
        concat!(
            r#"<doc a="a&#x9;b&#xA;c&#xD;d&#xD;&#xA;e&#x85;f&#x2028;g&#x7F;h&#x9F;i"#,
            "\u{A0}",
            r#"j&amp;&lt;>&quot;&apos;]]>k">"#,
            "a\u{9}b\u{A}c&#xD;d&#xD;\u{A}e&#x85;f&#x2028;g&#x7F;h&#x9F;i\u{A0}j&amp;&lt;&gt;\"']]&gt;k",
            "</doc>"
        )
    );
    assert_eq!(
        parse_values(&mut xot, &serialized),
        (AWKWARD.to_string(), AWKWARD.to_string())
    );
}

#[test]
fn test_round_trip_cdata_section() {
    let mut xot = Xot::new();
    let (doc, _) = document(&mut xot, AWKWARD);
    let doc_name = xot.add_name("doc");
    let serialized = xot
        .serialize_xml_string(
            output::xml::Parameters {
                cdata_section_elements: vec![doc_name],
                ..Default::default()
            },
            doc,
        )
        .unwrap();
    assert!(serialized.ends_with(concat!(
        "><![CDATA[a\u{9}b\u{A}c]]>&#xD;<![CDATA[d]]>&#xD;<![CDATA[\u{A}e]]>&#x85;<![CDATA[f]]>",
        "&#x2028;<![CDATA[g]]>&#x7F;<![CDATA[h]]>&#x9F;<![CDATA[i\u{A0}j&<>\"']]]]><![CDATA[>k]]>",
        "</doc>"
    )));
    assert_eq!(
        parse_values(&mut xot, &serialized),
        (AWKWARD.to_string(), AWKWARD.to_string())
    );
}

#[test]
fn test_unescaped_cr() {
    let mut xot = Xot::new();
    let (doc, _) = document(&mut xot, "a\rb\r\nc\u{85}");
    let serialized = xot
        .serialize_xml_string(
            output::xml::Parameters {
                unescaped_cr: true,
                ..Default::default()
            },
            doc,
        )
        .unwrap();
    // a carriage return is still escaped in attribute values
    assert_eq!(
        serialized,
        "<doc a=\"a&#xD;b&#xD;&#xA;c&#x85;\">a\rb\r\nc&#x85;</doc>"
    );
    // so it survives there, but not in text
    assert_eq!(
        parse_values(&mut xot, &serialized),
        ("a\nb\nc\u{85}".to_string(), "a\rb\r\nc\u{85}".to_string())
    );
}

#[test]
fn test_unescaped_cr_in_cdata_section() {
    let mut xot = Xot::new();
    let (doc, _) = document(&mut xot, "a\rb\u{85}");
    let doc_name = xot.add_name("doc");
    let mut buf = Vec::new();
    xot.serialize_xml_write(
        output::xml::Parameters {
            cdata_section_elements: vec![doc_name],
            unescaped_cr: true,
            ..Default::default()
        },
        doc,
        &mut buf,
    )
    .unwrap();
    assert_eq!(
        String::from_utf8(buf).unwrap(),
        "<doc a=\"a&#xD;b&#x85;\"><![CDATA[a\rb]]>&#x85;</doc>"
    );
}

#[test]
fn test_unescaped_cr_with_normalizer() {
    let mut xot = Xot::new();
    let (doc, _) = document(&mut xot, "a\rb");
    let serialized = xot
        .serialize_xml_string_with_normalizer(
            output::xml::Parameters {
                unescaped_cr: true,
                ..Default::default()
            },
            doc,
            output::NoopNormalizer,
        )
        .unwrap();
    assert_eq!(serialized, "<doc a=\"a&#xD;b\">a\rb</doc>");
}

/// The text of the tokens of `doc`, with `parameters`.
fn token_text(xot: &Xot, doc: Node, parameters: TokenSerializeParameters) -> String {
    xot.tokens(doc, parameters, output::NoopNormalizer)
        .map(|(_, _, token)| {
            if token.space {
                format!(" {}", token.text)
            } else {
                token.text
            }
        })
        .collect()
}

/// The text of the pretty printed tokens of `doc`, with `parameters`.
fn pretty_token_text(xot: &Xot, doc: Node, parameters: TokenSerializeParameters) -> String {
    xot.pretty_tokens(doc, parameters, &[], output::NoopNormalizer)
        .map(|(_, _, token)| {
            if token.space {
                format!(" {}", token.text)
            } else {
                token.text
            }
        })
        .collect()
}

#[test]
fn test_tokens_escape_as_serialization() {
    let mut xot = Xot::new();
    let (doc, _) = document(&mut xot, AWKWARD);
    let serialized = xot.to_string(doc).unwrap();
    assert_eq!(token_text(&xot, doc, Default::default()), serialized);
    assert_eq!(pretty_token_text(&xot, doc, Default::default()), serialized);
}

#[test]
fn test_tokens_unescaped_cr() {
    let mut xot = Xot::new();
    let (doc, _) = document(&mut xot, "a\rb");
    let parameters = TokenSerializeParameters {
        unescaped_cr: true,
        ..Default::default()
    };
    let expected = "<doc a=\"a&#xD;b\">a\rb</doc>";
    assert_eq!(token_text(&xot, doc, parameters.clone()), expected);
    assert_eq!(pretty_token_text(&xot, doc, parameters), expected);
}

/// Characters that XML 1.0 does not allow, not even as character references:
/// the control characters other than tab, newline and carriage return, and
/// #x0, #xFFFE and #xFFFF.
const NOT_IN_XML_10: &[char] = &[
    '\u{0}', '\u{1}', '\u{8}', '\u{B}', '\u{C}', '\u{E}', '\u{1F}', '\u{FFFE}', '\u{FFFF}',
];

/// The character, node and place of an `Error::InvalidCharacter`.
fn invalid_character<T: std::fmt::Debug>(result: Result<T, Error>) -> (char, Node, String) {
    match result {
        Err(Error::InvalidCharacter(c, node, place)) => (c, node, place),
        other => panic!("expected Error::InvalidCharacter, got {:?}", other),
    }
}

#[test]
fn test_invalid_character_in_text() {
    for c in NOT_IN_XML_10 {
        let mut xot = Xot::new();
        let doc_name = xot.add_name("doc");
        let doc_el = xot.new_element(doc_name);
        xot.append_text(doc_el, &format!("a{}b", c)).unwrap();
        let text = xot.first_child(doc_el).unwrap();
        let doc = xot.new_document_with_element(doc_el).unwrap();
        assert_eq!(
            invalid_character(xot.to_string(doc)),
            (*c, text, "text".to_string())
        );
    }
}

#[test]
fn test_invalid_character_in_cdata_section_element() {
    let mut xot = Xot::new();
    let doc_name = xot.add_name("doc");
    let doc_el = xot.new_element(doc_name);
    xot.append_text(doc_el, "a\u{1}b").unwrap();
    let text = xot.first_child(doc_el).unwrap();
    let doc = xot.new_document_with_element(doc_el).unwrap();
    let parameters = output::xml::Parameters {
        cdata_section_elements: vec![doc_name],
        ..Default::default()
    };
    assert_eq!(
        invalid_character(xot.serialize_xml_string(parameters, doc)),
        ('\u{1}', text, "text".to_string())
    );
}

#[test]
fn test_invalid_character_in_attribute() {
    for c in NOT_IN_XML_10 {
        let mut xot = Xot::new();
        let (doc, doc_el) = document(&mut xot, "text");
        let a = xot.add_name("a");
        xot.set_attribute(doc_el, a, format!("a{}b", c));
        assert_eq!(
            invalid_character(xot.to_string(doc)),
            (*c, doc_el, "attribute \"a\"".to_string())
        );
    }
}

#[test]
fn test_invalid_character_in_prefixed_attribute() {
    let mut xot = Xot::new();
    let doc = xot
        .parse(r#"<doc xmlns:p="http://example.com/p" p:a="value"/>"#)
        .unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    let ns = xot.add_namespace("http://example.com/p");
    let a = xot.add_name_ns("a", ns);
    xot.set_attribute(doc_el, a, "\u{2}");
    assert_eq!(
        invalid_character(xot.to_string(doc)),
        ('\u{2}', doc_el, "attribute \"p:a\"".to_string())
    );
}

#[test]
fn test_invalid_character_in_comment() {
    let mut xot = Xot::new();
    let (doc, doc_el) = document(&mut xot, "text");
    let comment = xot.new_comment("a\u{3}b");
    xot.append(doc_el, comment).unwrap();
    assert_eq!(
        invalid_character(xot.to_string(doc)),
        ('\u{3}', comment, "comment".to_string())
    );
}

#[test]
fn test_invalid_character_in_processing_instruction() {
    let mut xot = Xot::new();
    let (doc, doc_el) = document(&mut xot, "text");
    let target = xot.add_name("target");
    let pi = xot.new_processing_instruction(target, Some("a\u{4}b"));
    xot.append(doc_el, pi).unwrap();
    assert_eq!(
        invalid_character(xot.to_string(doc)),
        ('\u{4}', pi, "processing instruction \"target\"".to_string())
    );
}

#[test]
fn test_invalid_character_in_namespace_declaration() {
    let mut xot = Xot::new();
    let (doc, doc_el) = document(&mut xot, "text");
    let p = xot.add_prefix("p");
    let ns = xot.add_namespace("http://example.com/\u{5}");
    xot.set_namespace(doc_el, p, ns);
    assert_eq!(
        invalid_character(xot.to_string(doc)),
        (
            '\u{5}',
            doc_el,
            "namespace declaration \"xmlns:p\"".to_string()
        )
    );

    let mut xot = Xot::new();
    let ns = xot.add_namespace("http://example.com/\u{6}");
    let doc_name = xot.add_name_ns("doc", ns);
    let doc_el = xot.new_element(doc_name);
    let empty = xot.empty_prefix();
    xot.set_namespace(doc_el, empty, ns);
    let doc = xot.new_document_with_element(doc_el).unwrap();
    assert_eq!(
        invalid_character(xot.to_string(doc)),
        (
            '\u{6}',
            doc_el,
            "namespace declaration \"xmlns\"".to_string()
        )
    );
}

#[test]
fn test_invalid_character_with_options() {
    let mut xot = Xot::new();
    let (doc, doc_el) = document(&mut xot, "a\u{7}b");
    let a = xot.add_name("a");
    xot.set_attribute(doc_el, a, "value");
    let text = xot.first_child(doc_el).unwrap();
    let expected = ('\u{7}', text, "text".to_string());
    let unescaped_cr = output::xml::Parameters {
        unescaped_cr: true,
        ..Default::default()
    };
    assert_eq!(
        invalid_character(xot.serialize_xml_string(unescaped_cr, doc)),
        expected
    );
    let pretty = output::xml::Parameters {
        indentation: Some(Default::default()),
        ..Default::default()
    };
    assert_eq!(
        invalid_character(xot.serialize_xml_string(pretty, doc)),
        expected
    );
}

#[test]
fn test_invalid_character_is_not_written() {
    let mut xot = Xot::new();
    let (doc, doc_el) = document(&mut xot, "a\u{1}b");
    let a = xot.add_name("a");
    xot.set_attribute(doc_el, a, "value");
    let mut buf = Vec::new();
    let err = xot.write(doc, &mut buf).unwrap_err();
    assert!(matches!(err, Error::InvalidCharacter('\u{1}', _, _)));
    // the start tag is written, but nothing of the text
    assert_eq!(String::from_utf8(buf).unwrap(), "<doc a=\"value\">");
}

#[test]
fn test_invalid_character_message() {
    let mut xot = Xot::new();
    let (doc, _) = document(&mut xot, "\u{1F}");
    let err = xot.to_string(doc).unwrap_err();
    assert_eq!(
        err.to_string(),
        "Character U+001F is not allowed in XML 1.0, found in attribute \"a\""
    );
}

#[test]
fn test_characters_allowed_in_xml_10() {
    // the edges of the ranges of the `Char` production
    let value = "\u{9}\u{A}\u{D}\u{20}\u{D7FF}\u{E000}\u{FFFD}\u{10000}\u{10FFFF}";
    let mut xot = Xot::new();
    let (doc, doc_el) = document(&mut xot, value);
    let serialized = xot.to_string(doc).unwrap();
    assert_eq!(
        parse_values(&mut xot, &serialized),
        (value.to_string(), value.to_string())
    );

    let comment = xot.new_comment(value);
    xot.append(doc_el, comment).unwrap();
    let target = xot.add_name("target");
    let pi = xot.new_processing_instruction(target, Some(value));
    xot.append(doc_el, pi).unwrap();
    assert_eq!(xot.to_string(comment).unwrap(), format!("<!--{}-->", value));
    assert_eq!(xot.to_string(pi).unwrap(), format!("<?target {}?>", value));
}

/// A character that may appear in an XML 1.0 document, more often one that
/// needs care when serialized.
fn xml_char() -> impl Strategy<Value = char> {
    prop_oneof![
        prop::sample::select(vec![
            '\t', '\n', '\r', ' ', '&', '<', '>', '"', '\'', ']', '\u{7F}', '\u{85}', '\u{9F}',
            '\u{A0}', '\u{2028}', '\u{2029}',
        ]),
        prop::char::ranges(
            vec![
                '\u{20}'..='\u{D7FF}',
                '\u{E000}'..='\u{FFFD}',
                '\u{10000}'..='\u{10FFFF}',
            ]
            .into()
        ),
    ]
}

fn xml_string() -> impl Strategy<Value = String> {
    prop::collection::vec(xml_char(), 0..20).prop_map(|chars| chars.into_iter().collect())
}

proptest! {
    #[test]
    fn test_round_trip_any_value(value in xml_string(), cdata in any::<bool>()) {
        let mut xot = Xot::new();
        let (doc, _) = document(&mut xot, &value);
        let doc_name = xot.add_name("doc");
        let parameters = output::xml::Parameters {
            cdata_section_elements: if cdata { vec![doc_name] } else { vec![] },
            ..Default::default()
        };
        let serialized = xot.serialize_xml_string(parameters, doc).unwrap();
        prop_assert_eq!(parse_values(&mut xot, &serialized), (value.clone(), value));
    }
}
