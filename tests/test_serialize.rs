use std::io;

use xot::{output, Error, Xot};

#[test]
fn test_serialize_node() {
    let mut xot = Xot::new();
    let doc = xot
        .parse(r#"<doc xmlns:foo="http://example.com"><foo:a/></doc>"#)
        .unwrap();
    let node = xot.first_child(xot.document_element(doc).unwrap()).unwrap();
    assert_eq!(
        xot.to_string(node).unwrap(),
        r#"<foo:a xmlns:foo="http://example.com"/>"#
    );
}

#[test]
fn test_serialize_node_default_ns() {
    let mut xot = Xot::new();
    let doc = xot
        .parse(r#"<doc xmlns="http://example.com"><a/></doc>"#)
        .unwrap();
    let node = xot.first_child(xot.document_element(doc).unwrap()).unwrap();
    assert_eq!(
        xot.to_string(node).unwrap(),
        r#"<a xmlns="http://example.com"/>"#
    );
}

#[test]
fn test_serialize_node_default_ns_nested() {
    let mut xot = Xot::new();
    let doc = xot
        .parse(r#"<doc xmlns="http://example.com"><a><b/></a></doc>"#)
        .unwrap();
    let node = xot.first_child(xot.document_element(doc).unwrap()).unwrap();
    assert_eq!(
        xot.to_string(node).unwrap(),
        r#"<a xmlns="http://example.com"><b/></a>"#
    );
}

#[test]
fn test_prefix_ambiguous() {
    let mut xot = Xot::new();
    let doc = xot
        .parse(r#"<doc xmlns:x="http://example.com/x"><a xmlns:x="http://example.com/y"/></doc>"#)
        .unwrap();
    let root_id = xot.document_element(doc).unwrap();
    let a_id = xot.first_child(root_id).unwrap();

    let ns_x_id = xot.add_namespace("http://example.com/x");
    let ns_y_id = xot.add_namespace("http://example.com/y");
    let name_x_id = xot.add_name_ns("b", ns_x_id);
    let name_y_id = xot.add_name_ns("b", ns_y_id);

    let mut attributes = xot.attributes_mut(a_id);
    attributes.insert(name_x_id, "X".to_string());
    attributes.insert(name_y_id, "Y".to_string());

    xot.create_missing_prefixes(doc).unwrap();

    assert_eq!(
        xot.to_string(doc).unwrap(),
        r#"<doc xmlns:x="http://example.com/x" xmlns:n0="http://example.com/x"><a xmlns:x="http://example.com/y" n0:b="X" x:b="Y"/></doc>"#
    );
}

#[test]
fn test_prefix_ambiguous_no_ns() {
    let mut xot = Xot::new();
    let doc = xot
        .parse(
            r#"<a><a xmlns:x="http://example.com/y"><a xmlns:x="http://example.com/x"/></a></a>"#,
        )
        .unwrap();
    let root_id = xot.document_element(doc).unwrap();
    let a_id = xot.first_child(root_id).unwrap();
    let a_id = xot.first_child(a_id).unwrap();

    let ns_y_id = xot.add_namespace("http://example.com/y");
    let name_q_id = xot.add_name_ns("q", ns_y_id);

    let mut attributes = xot.attributes_mut(a_id);
    attributes.insert(name_q_id, "Q".to_string());

    xot.create_missing_prefixes(doc).unwrap();
    assert_eq!(
        xot.to_string(doc).unwrap(),
        r#"<a xmlns:n0="http://example.com/y"><a xmlns:x="http://example.com/y"><a xmlns:x="http://example.com/x" n0:q="Q"/></a></a>"#
    );
}

#[test]
fn test_prefix_ambiguous_default_ns() {
    let mut xot = Xot::new();
    let doc = xot
        .parse(r#"<a xmlns="http://example.com/y"><a><a/></a></a>"#)
        .unwrap();
    let root_id = xot.document_element(doc).unwrap();
    let a_id = xot.first_child(root_id).unwrap();
    let a_id = xot.first_child(a_id).unwrap();

    let ns_y_id = xot.add_namespace("http://example.com/y");
    let ns_empty_id = xot.add_namespace("");

    let name_r_id = xot.add_name_ns("r", ns_y_id);
    let name_empty_id = xot.add_name_ns("r", ns_empty_id);

    let mut attributes = xot.attributes_mut(a_id);
    attributes.insert(name_r_id, "R".to_string());
    attributes.insert(name_empty_id, "R2".to_string());
    xot.create_missing_prefixes(doc).unwrap();
    assert_eq!(
        xot.to_string(doc).unwrap(),
        r#"<a xmlns="http://example.com/y" xmlns:n0="http://example.com/y"><a><a n0:r="R" r="R2"/></a></a>"#
    );
}

#[test]
fn test_prefix_ambiguous_default_ns2() {
    let mut xot = Xot::new();
    let doc = xot
        .parse(r#"<a><a><a xmlns="http://example.com/y"/></a></a>"#)
        .unwrap();
    let root_id = xot.document_element(doc).unwrap();
    let a_id = xot.first_child(root_id).unwrap();
    let a_id = xot.first_child(a_id).unwrap();

    let ns_y_id = xot.add_namespace("http://example.com/y");

    let name_r_id = xot.add_name_ns("r", ns_y_id);
    let mut attributes = xot.attributes_mut(a_id);
    attributes.insert(name_r_id, "R".to_string());

    xot.create_missing_prefixes(doc).unwrap();
    assert_eq!(
        xot.to_string(doc).unwrap(),
        r#"<a xmlns:n0="http://example.com/y"><a><a xmlns="http://example.com/y" n0:r="R"/></a></a>"#
    );
}

#[test]
fn test_serialize_lt() {
    let mut xot = Xot::new();
    let doc = xot.parse(r#"<a>&lt;</a>"#).unwrap();
    let serialized = xot.to_string(doc).unwrap();
    assert_eq!(serialized, r#"<a>&lt;</a>"#);
}

#[test]
fn test_serialize_gt_by_default() {
    let mut xot = Xot::new();
    let doc = xot.parse(r#"<a>&gt;</a>"#).unwrap();
    let serialized = xot.to_string(doc).unwrap();
    assert_eq!(serialized, r#"<a>&gt;</a>"#);
}

#[test]
fn test_do_not_serialize_gt() {
    let mut xot = Xot::new();
    let doc = xot.parse(r#"<a>&gt;</a>"#).unwrap();
    let serialized = xot
        .serialize_xml_string(
            xot::output::xml::Parameters {
                unescaped_gt: true,
                ..Default::default()
            },
            doc,
        )
        .unwrap();
    assert_eq!(serialized, r#"<a>></a>"#);
}

#[test]
fn test_weird_delimiter() {
    let mut xot = Xot::new();
    let doc = xot.parse(r#"<a>]]&gt;</a>"#).unwrap();
    let serialized = xot.to_string(doc).unwrap();
    assert_eq!(serialized, r#"<a>]]&gt;</a>"#);
}

#[test]
fn test_weird_delimiter_unescaped() {
    let mut xot = Xot::new();
    let doc = xot.parse(r#"<a>]]&gt;</a>"#).unwrap();
    let serialized = xot
        .serialize_xml_string(
            xot::output::xml::Parameters {
                unescaped_gt: true,
                ..Default::default()
            },
            doc,
        )
        .unwrap();
    assert_eq!(serialized, r#"<a>]]&gt;</a>"#);
}

#[test]
fn test_serialize_fragment() {
    let mut xot = Xot::new();
    let fragment = xot.parse_fragment(r#"<a/><b/>text"#).unwrap();
    let serialized = xot.to_string(fragment).unwrap();
    assert_eq!(serialized, r#"<a/><b/>text"#);
}

#[test]
fn test_serialize_empty_fragment() {
    let mut xot = Xot::new();
    let fragment = xot.parse_fragment(r#""#).unwrap();
    let serialized = xot.to_string(fragment).unwrap();
    assert_eq!(serialized, r#""#);
}

#[test]
fn test_serialize_text_fragment() {
    let mut xot = Xot::new();
    let fragment = xot.parse_fragment(r#"text"#).unwrap();
    let serialized = xot.to_string(fragment).unwrap();
    assert_eq!(serialized, r#"text"#);
}

#[test]
fn test_serialize_text_without_parent() {
    let mut xot = Xot::new();
    let text = xot.new_text("a<b");
    assert_eq!(xot.to_string(text).unwrap(), "a&lt;b");
}

#[test]
fn test_serialize_text_without_parent_cdata_section_elements() {
    let mut xot = Xot::new();
    let a = xot.add_name("a");
    let text = xot.new_text("a<b");
    let serialized = xot
        .serialize_xml_string(
            output::xml::Parameters {
                cdata_section_elements: vec![a],
                ..Default::default()
            },
            text,
        )
        .unwrap();
    assert_eq!(serialized, "a&lt;b");
}

#[test]
fn test_serialize_text_without_parent_pretty() {
    let mut xot = Xot::new();
    let text = xot.new_text("a<b");
    let serialized = xot
        .serialize_xml_string(
            output::xml::Parameters {
                indentation: Some(Default::default()),
                ..Default::default()
            },
            text,
        )
        .unwrap();
    assert_eq!(serialized, "a&lt;b");
}

#[test]
fn test_tokens_text_without_parent() {
    let mut xot = Xot::new();
    let text = xot.new_text("a<b");
    let tokens = xot
        .tokens(text, Default::default(), output::NoopNormalizer)
        .map(|(_, _, token)| token.text)
        .collect::<Vec<_>>();
    assert_eq!(tokens, vec!["a&lt;b"]);
}

#[test]
fn test_html5_text_without_parent() {
    let mut xot = Xot::new();
    let text = xot.new_text("a<b");
    assert_eq!(
        xot.html5().to_string(text).unwrap(),
        "<!DOCTYPE html>a&lt;b"
    );
}

#[test]
fn test_html5_text_under_document() {
    let mut xot = Xot::new();
    let fragment = xot.parse_fragment("a&lt;b<p>c</p>").unwrap();
    assert_eq!(
        xot.html5().to_string(fragment).unwrap(),
        "<!DOCTYPE html>a&lt;b<p>c</p>"
    );
}

struct FailingWriter;

impl io::Write for FailingWriter {
    fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("refused"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn test_write_error() {
    let mut xot = Xot::new();
    let doc = xot.parse("<p>a</p>").unwrap();
    let err = xot.write(doc, &mut FailingWriter).unwrap_err();
    assert!(matches!(err, Error::Io(_)));
}

#[test]
fn test_html5_write_error() {
    let mut xot = Xot::new();
    let doc = xot.parse("<p>a</p>").unwrap();
    let err = xot.html5().write(doc, &mut FailingWriter).unwrap_err();
    assert!(matches!(err, Error::Io(_)));
}

/// A document `<p:doc xmlns:p="{namespace}"/>`.
fn document_in_namespace(xot: &mut Xot, namespace: &str) -> xot::Node {
    let ns = xot.add_namespace(namespace);
    let p = xot.add_prefix("p");
    let doc_name = xot.add_name_ns("doc", ns);
    let doc_el = xot.new_element(doc_name);
    xot.set_namespace(doc_el, p, ns);
    xot.new_document_with_element(doc_el).unwrap()
}

#[test]
fn test_serialize_namespace_declaration_escaped() {
    let mut xot = Xot::new();
    let doc = document_in_namespace(&mut xot, "urn:x?a=1&b=2<>\"'");
    assert_eq!(
        xot.to_string(doc).unwrap(),
        r#"<p:doc xmlns:p="urn:x?a=1&amp;b=2&lt;>&quot;&apos;"/>"#
    );
}

#[test]
fn test_serialize_default_namespace_declaration_escaped() {
    let mut xot = Xot::new();
    let ns = xot.add_namespace("urn:a&b");
    let doc_name = xot.add_name_ns("doc", ns);
    let doc_el = xot.new_element(doc_name);
    let empty = xot.empty_prefix();
    xot.set_namespace(doc_el, empty, ns);
    let doc = xot.new_document_with_element(doc_el).unwrap();
    assert_eq!(xot.to_string(doc).unwrap(), r#"<doc xmlns="urn:a&amp;b"/>"#);
}

#[test]
fn test_serialize_namespace_declaration_whitespace() {
    // whitespace characters are written as character references, so that
    // attribute-value normalization does not turn them into spaces
    let mut xot = Xot::new();
    let doc = document_in_namespace(&mut xot, "urn:a\u{9}b\u{A}c\u{D}d");
    assert_eq!(
        xot.to_string(doc).unwrap(),
        r#"<p:doc xmlns:p="urn:a&#x9;b&#xA;c&#xD;d"/>"#
    );
}

#[test]
fn test_namespace_declaration_round_trip() {
    let namespace = "urn:x?a=1&b=2<>\"'\u{9}\u{A}\u{D}\u{85}\u{2028}";
    let mut xot = Xot::new();
    let doc = document_in_namespace(&mut xot, namespace);
    let serialized = xot.to_string(doc).unwrap();
    let doc = xot.parse(&serialized).unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    let ns = xot.namespace_for_name(xot.element(doc_el).unwrap().name());
    assert_eq!(xot.namespace_str(ns), namespace);
    assert_eq!(xot.to_string(doc).unwrap(), serialized);
}

#[test]
fn test_serialize_parsed_namespace_declaration() {
    let mut xot = Xot::new();
    let doc = xot
        .parse(r#"<p:doc xmlns:p="urn:a&#38;b&#60;c&#34;"><p:e/></p:doc>"#)
        .unwrap();
    assert_eq!(
        xot.to_string(doc).unwrap(),
        r#"<p:doc xmlns:p="urn:a&amp;b&lt;c&quot;"><p:e/></p:doc>"#
    );
    // a node serialized on its own declares the namespace it inherits
    let doc_el = xot.document_element(doc).unwrap();
    let e = xot.first_child(doc_el).unwrap();
    assert_eq!(
        xot.to_string(e).unwrap(),
        r#"<p:e xmlns:p="urn:a&amp;b&lt;c&quot;"/>"#
    );
}

#[test]
fn test_html5_namespace_declaration_escaped() {
    let mut xot = Xot::new();
    let doc = xot
        .parse(r#"<html><body><island xmlns="urn:a&#38;b&#34;c">text</island></body></html>"#)
        .unwrap();
    assert_eq!(
        xot.html5().to_string(doc).unwrap(),
        r#"<!DOCTYPE html><html><body><island xmlns="urn:a&amp;b&quot;c">text</island></body></html>"#
    );
}

#[test]
fn test_html5_namespace_declaration_from_api_escaped() {
    let mut xot = Xot::new();
    let doc = xot.parse("<html><body/></html>").unwrap();
    let html = xot.document_element(doc).unwrap();
    let body = xot.first_child(html).unwrap();
    let ns = xot.add_namespace("urn:a&b\"c");
    let island_name = xot.add_name_ns("island", ns);
    let island = xot.new_element(island_name);
    let empty = xot.empty_prefix();
    xot.set_namespace(island, empty, ns);
    xot.append(body, island).unwrap();
    assert_eq!(
        xot.html5().to_string(doc).unwrap(),
        r#"<!DOCTYPE html><html><body><island xmlns="urn:a&amp;b&quot;c"></island></body></html>"#
    );
}
