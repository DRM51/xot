// Which prefix the XML serializer writes a name with, where the namespace
// declarations of the elements shadow each other: the strings are what the
// serializer with a copied list of declarations for each element wrote.

use xot::{Node, Xot};

fn write(xot: &Xot, node: Node) -> String {
    let mut out = Vec::new();
    xot.serialize_xml_write(Default::default(), node, &mut out)
        .map(|_| String::from_utf8(out).unwrap())
        .unwrap_or_else(|e| format!("error: {}", e))
}

// two declarations of the prefix p on one element, made by changing the
// prefix of a namespace node
fn duplicate_declaration(xot: &mut Xot) -> Node {
    let doc = xot
        .parse(r#"<r xmlns:p="urn:p" xmlns:q="urn:q"></r>"#)
        .unwrap();
    let r = xot.document_element(doc).unwrap();
    let nodes = xot.namespaces(r).nodes().collect::<Vec<_>>();
    let p = xot.add_prefix("p");
    let urn_q = xot.add_namespace("urn:q");
    let other = xot.new_namespace_node(p, urn_q);
    let value = xot.value(other).clone();
    *xot.value_mut(nodes[1]) = value;
    for (name, ns) in [("a", "urn:p"), ("b", "urn:q")] {
        let ns = xot.add_namespace(ns);
        let name = xot.add_name_ns(name, ns);
        let e = xot.new_element(name);
        xot.append(r, e).unwrap();
    }
    doc
}

#[test]
fn test_duplicate_declaration_on_one_element() {
    let mut xot = Xot::new();
    let doc = duplicate_declaration(&mut xot);
    assert_eq!(
        write(&xot, doc),
        r#"<r xmlns:p="urn:p" xmlns:p="urn:q"><p:a/><p:b/></r>"#
    );
}

// serializing an inner element: the namespaces in scope of the top are the
// base, and for one namespace the prefix of a farther ancestor wins
#[test]
fn test_farther_ancestor_prefix_wins_in_the_base() {
    let mut xot = Xot::new();
    let doc = xot
        .parse(r#"<r xmlns:far="urn:n"><m xmlns:near="urn:n"><x><near:e/></x></m></r>"#)
        .unwrap();
    let r = xot.document_element(doc).unwrap();
    let m = xot.first_child(r).unwrap();
    let x = xot.first_child(m).unwrap();
    assert_eq!(
        write(&xot, x),
        r#"<x xmlns:near="urn:n" xmlns:far="urn:n"><far:e/></x>"#
    );
    // an element that declares namespaces uses its own
    assert_eq!(
        write(&xot, m),
        r#"<m xmlns:far="urn:n" xmlns:near="urn:n"><x><near:e/></x></m>"#
    );
}

// an attribute written with p2 after p2 is declared for its namespace, and
// the name of an attribute in the message of an error
#[test]
fn test_attribute_with_the_prefix_declared_later() {
    let mut xot = Xot::new();
    let doc = xot
        .parse(r#"<r xmlns:p="urn:p" p:l="1"><c xmlns:p2="urn:p" p:l="2" p2:m="3"/></r>"#)
        .unwrap();
    assert_eq!(
        write(&xot, doc),
        r#"<r xmlns:p="urn:p" p:l="1"><c xmlns:p2="urn:p" p2:l="2" p2:m="3"/></r>"#
    );
    let r = xot.document_element(doc).unwrap();
    let c = xot.first_child(r).unwrap();
    let ns = xot.add_namespace("urn:p");
    let pl = xot.add_name_ns("l", ns);
    xot.attributes_mut(c).insert(pl, "\u{1}".to_string());
    assert_eq!(
        write(&xot, doc),
        "error: Character U+0001 is not allowed in XML 1.0, found in attribute \"p2:l\""
    );
    xot.attributes_mut(c).insert(pl, "2".to_string());
    // no prefix for the namespace: the local name is in the message
    let ns = xot.add_namespace("urn:z");
    let l = xot.add_name_ns("l", ns);
    xot.attributes_mut(c).insert(l, "\u{1}".to_string());
    assert_eq!(
        write(&xot, doc),
        "error: Character U+0001 is not allowed in XML 1.0, found in attribute \"l\""
    );
}

#[test]
fn test_element_in_no_namespace_inside_a_default_namespace() {
    let mut xot = Xot::new();
    let doc = xot
        .parse(r#"<r xmlns="urn:d"><c xmlns=""><d xmlns="urn:d"/></c></r>"#)
        .unwrap();
    let xml = r#"<r xmlns="urn:d"><c xmlns=""><d xmlns="urn:d"/></c></r>"#;
    assert_eq!(write(&xot, doc), xml);
    let r = xot.document_element(doc).unwrap();
    let n = xot.add_name("lone");
    let e = xot.new_element(n);
    xot.append(r, e).unwrap();
    assert_eq!(
        write(&xot, doc),
        r#"<r xmlns="urn:d"><c xmlns=""><d xmlns="urn:d"/></c><lone/></r>"#
    );
}

// the empty prefix is preferred to p for the namespace, and xmlns="" gives
// no name a prefix
#[test]
fn test_undeclared_default_namespace() {
    let mut xot = Xot::new();
    let doc = xot
        .parse(r#"<r xmlns="urn:d" xmlns:p="urn:d"><c xmlns=""><p:d/></c><p:e/></r>"#)
        .unwrap();
    assert_eq!(
        write(&xot, doc),
        r#"<r xmlns="urn:d" xmlns:p="urn:d"><c xmlns=""><p:d/></c><e/></r>"#
    );
    let r = xot.document_element(doc).unwrap();
    let c = xot.first_child(r).unwrap();
    assert_eq!(write(&xot, c), r#"<c xmlns:p="urn:d" xmlns=""><p:d/></c>"#);
}
