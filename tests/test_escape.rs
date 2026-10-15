use proptest::prelude::*;
use xot::{output, output::TokenSerializeParameters, Node, Value, Xot};

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
