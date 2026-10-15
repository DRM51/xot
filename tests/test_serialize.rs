use std::io;

use xot::{output, Error, Node, Xot};

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

#[test]
fn test_serialize_prefixed_xmlns_attribute() {
    // `p:xmlns` is an attribute, written with its prefix, so the output
    // declares no default namespace and reads back the same
    let mut xot = Xot::new();
    let xml = r#"<a xmlns:p="urn:p" p:xmlns="urn:v"><b/></a>"#;
    let doc = xot.parse(xml).unwrap();
    assert_eq!(xot.to_string(doc).unwrap(), xml);
}

#[test]
fn test_serialize_attribute_named_xmlns_in_namespace() {
    // made without the parser: an attribute with the local name `xmlns` in a
    // namespace is written with a prefix like any attribute in a namespace
    let mut xot = Xot::new();
    let ns = xot.add_namespace("urn:p");
    let a_name = xot.add_name("a");
    let xmlns = xot.add_name_ns("xmlns", ns);
    let a = xot.new_element(a_name);
    xot.set_attribute(a, xmlns, "urn:v");
    let doc = xot.new_document_with_element(a).unwrap();
    // there is no prefix for the namespace yet
    assert!(matches!(xot.to_string(doc), Err(Error::MissingPrefix(ref ns)) if ns == "urn:p"));
    xot.create_missing_prefixes(doc).unwrap();
    let serialized = xot.to_string(doc).unwrap();
    assert_eq!(serialized, r#"<a xmlns:n0="urn:p" n0:xmlns="urn:v"/>"#);
    // which reads back as the same attribute, not as a declaration
    let doc = xot.parse(&serialized).unwrap();
    let a = xot.document_element(doc).unwrap();
    assert_eq!(xot.name_ns_str(xot.element(a).unwrap().name()), ("a", ""));
    assert_eq!(xot.get_attribute(a, xmlns), Some("urn:v"));
}

#[test]
fn test_html5_prefixed_xmlns_attribute() {
    let mut xot = Xot::new();
    let doc = xot
        .parse(r#"<html><body xmlns:p="urn:p" p:xmlns="urn:v"></body></html>"#)
        .unwrap();
    assert_eq!(
        xot.html5().to_string(doc).unwrap(),
        r#"<!DOCTYPE html><html><body xmlns:p="urn:p" p:xmlns="urn:v"></body></html>"#
    );
}

#[test]
fn test_serialize_unprefixed_xmlns_attribute() {
    // made through the API: written as `xmlns="..."`, this attribute would
    // read back as a default namespace declaration
    let mut xot = Xot::new();
    let a_name = xot.add_name("a");
    let xmlns = xot.add_name("xmlns");
    let a = xot.new_element(a_name);
    xot.set_attribute(a, xmlns, "urn:v");
    let doc = xot.new_document_with_element(a).unwrap();
    let err = xot.to_string(doc).unwrap_err();
    assert!(matches!(err, Error::XmlnsAttribute(node, ref name) if node == a && name == "xmlns"));
    assert_eq!(
        err.to_string(),
        "Attribute xmlns would read back as a namespace declaration"
    );
}

#[test]
fn test_serialize_attribute_in_xmlns_namespace() {
    // made through the API: an attribute in the namespace of `xmlns` cannot
    // be written, as no prefix can be declared for that namespace, nor can
    // `xmlns` itself
    let mut xot = Xot::new();
    let ns = xot.add_namespace("http://www.w3.org/2000/xmlns/");
    let a_name = xot.add_name("a");
    let p = xot.add_name_ns("p", ns);
    let a = xot.new_element(a_name);
    xot.set_attribute(a, p, "urn:v");
    let doc = xot.new_document_with_element(a).unwrap();
    assert!(matches!(
        xot.to_string(doc),
        Err(Error::XmlnsAttribute(node, ref name)) if node == a && name == "p"
    ));
    // a prefix bound to the namespace is not written either
    let xmlns_prefix = xot.add_prefix("xmlns");
    xot.set_namespace(a, xmlns_prefix, ns);
    assert!(matches!(
        xot.to_string(doc),
        Err(Error::InvalidNamespaceDeclaration(node, ref name, _))
            if node == a && name == "xmlns:xmlns"
    ));
}

const XML_NAMESPACE: &str = "http://www.w3.org/XML/1998/namespace";
const XMLNS_NAMESPACE: &str = "http://www.w3.org/2000/xmlns/";

/// A document with an element `a` that declares `prefix` for `namespace`
/// and has a child `b`, both in `namespace`, made through the API; and `a`.
fn document_with_declaration(xot: &mut Xot, prefix: &str, namespace: &str) -> (Node, Node) {
    let prefix = xot.add_prefix(prefix);
    let ns = xot.add_namespace(namespace);
    let a_name = xot.add_name_ns("a", ns);
    let b_name = xot.add_name_ns("b", ns);
    let a = xot.new_element(a_name);
    xot.set_namespace(a, prefix, ns);
    xot.append_element(a, b_name).unwrap();
    let doc = xot.new_document_with_element(a).unwrap();
    (doc, a)
}

#[test]
fn test_serialize_invalid_namespace_declaration() {
    // made through the API: the namespace declarations that Namespaces in
    // XML 1.0 does not allow, which the parser rejects, are not written
    for (prefix, namespace, name) in [
        // only XML 1.1 can undeclare a prefix
        ("p", "", "xmlns:p"),
        ("xml", "", "xmlns:xml"),
        ("xmlns", "", "xmlns:xmlns"),
        // `xml` is bound to its namespace, and no other prefix is
        ("xml", "urn:x", "xmlns:xml"),
        ("p", XML_NAMESPACE, "xmlns:p"),
        ("", XML_NAMESPACE, "xmlns"),
        // `xmlns` cannot be declared, and its namespace cannot be bound
        ("xmlns", "urn:x", "xmlns:xmlns"),
        ("xmlns", XMLNS_NAMESPACE, "xmlns:xmlns"),
        ("p", XMLNS_NAMESPACE, "xmlns:p"),
        ("", XMLNS_NAMESPACE, "xmlns"),
    ] {
        let mut xot = Xot::new();
        let (doc, a) = document_with_declaration(&mut xot, prefix, namespace);
        let err = xot.to_string(doc).unwrap_err();
        assert!(
            matches!(
                err,
                Error::InvalidNamespaceDeclaration(node, ref n, ref ns)
                    if node == a && n == name && ns == namespace
            ),
            "{}=\"{}\": {:?}",
            name,
            namespace,
            err
        );
    }
    let mut xot = Xot::new();
    let (doc, a) = document_with_declaration(&mut xot, "p", "");
    assert_eq!(
        xot.to_string(doc).unwrap_err().to_string(),
        "Invalid namespace declaration: xmlns:p=\"\""
    );
    // also when the declaration is in scope of a node serialized on its own
    let b = xot.first_child(a).unwrap();
    assert!(matches!(
        xot.to_string(b),
        Err(Error::InvalidNamespaceDeclaration(node, ref name, _)) if node == b && name == "xmlns:p"
    ));
}

#[test]
fn test_serialize_namespace_declaration_round_trip() {
    // whatever namespace declaration is made through the API, XML output
    // either fails or reads back with the same declaration and names
    let mut serialized = Vec::new();
    for prefix in ["", "p", "xml", "xmlns"] {
        for namespace in ["", "urn:x", XML_NAMESPACE, XMLNS_NAMESPACE] {
            let mut xot = Xot::new();
            let (doc, a) = document_with_declaration(&mut xot, prefix, namespace);
            let xml = match xot.to_string(doc) {
                Ok(xml) => xml,
                Err(err) => {
                    assert!(
                        matches!(err, Error::InvalidNamespaceDeclaration(..)),
                        "{}={:?}: {:?}",
                        prefix,
                        namespace,
                        err
                    );
                    continue;
                }
            };
            let doc2 = xot
                .parse(&xml)
                .unwrap_or_else(|err| panic!("{}: {}", xml, err));
            let a2 = xot.document_element(doc2).unwrap();
            let names = |xot: &Xot, a: Node| {
                let b = xot.first_child(a).unwrap();
                (xot.node_name(a), xot.node_name(b))
            };
            assert_eq!(names(&xot, a2), names(&xot, a), "{}", xml);
            let declarations = |xot: &Xot, a: Node| {
                xot.namespaces(a)
                    .iter()
                    // `xml` is bound without a declaration
                    .filter(|(prefix, _)| *prefix != xot.xml_prefix())
                    .map(|(prefix, ns)| (prefix, *ns))
                    .collect::<Vec<_>>()
            };
            assert_eq!(declarations(&xot, a2), declarations(&xot, a), "{}", xml);
            serialized.push(xml);
        }
    }
    assert_eq!(
        serialized,
        [
            r#"<a xmlns=""><b/></a>"#,
            r#"<a xmlns="urn:x"><b/></a>"#,
            r#"<p:a xmlns:p="urn:x"><p:b/></p:a>"#,
            r#"<xml:a><xml:b/></xml:a>"#,
        ]
    );
}
