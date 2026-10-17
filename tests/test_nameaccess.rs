use xot::Xot;

#[test]
fn test_deduplicate_namespace() {
    let mut xot = Xot::new();
    let root = xot
        .parse(r#"<doc xmlns="http://example.com"><a xmlns="http://example.com">Hello!</a></doc>"#)
        .unwrap();
    xot.deduplicate_namespaces(root);
    assert_eq!(
        xot.to_string(root).unwrap(),
        r#"<doc xmlns="http://example.com"><a>Hello!</a></doc>"#
    );
}

#[test]
fn test_deduplicate_named_namespace() {
    let mut xot = Xot::new();
    let root = xot
        .parse(r#"<doc xmlns="http://example.com"><foo:a xmlns:foo="http://example.com">Hello!</foo:a></doc>"#)
        .unwrap();
    xot.deduplicate_namespaces(root);
    assert_eq!(
        xot.to_string(root).unwrap(),
        r#"<doc xmlns="http://example.com"><a>Hello!</a></doc>"#
    );
}

#[test]
fn test_deduplicate_named_namespace_again() {
    let mut xot = Xot::new();
    let root = xot
        .parse(r#"<section xmlns="http://docbook.org/ns/docbook" xmlns:diff="http://paligo.net/nxd" version="5.0">
  <title>Title</title>
  <para diff:delete="">Para first old </para><para xmlns="http://docbook.org/ns/docbook" diff:insert="">Before emphasis <emphasis>emphasis</emphasis> After emphasis</para>
  <para diff:delete="">Para second old</para><warning xmlns="http://docbook.org/ns/docbook" diff:insert="">
    <title>I am new</title>
    <para>Warning here</para>
  </warning>
</section>"#)
        .unwrap();
    xot.deduplicate_namespaces(root);
    assert_eq!(
        xot.to_string(root).unwrap(),
        r#"<section xmlns="http://docbook.org/ns/docbook" xmlns:diff="http://paligo.net/nxd" version="5.0">
  <title>Title</title>
  <para diff:delete="">Para first old </para><para diff:insert="">Before emphasis <emphasis>emphasis</emphasis> After emphasis</para>
  <para diff:delete="">Para second old</para><warning diff:insert="">
    <title>I am new</title>
    <para>Warning here</para>
  </warning>
</section>"#
    );
}

#[test]
fn test_deduplicate_overlapping_namespace_attribute() {
    let mut xot = Xot::new();

    // we have a default namespace and an attribute in that namespace
    // deduplicate should not clean up the overlapping n prefix in this
    // case as it's in use by the attribute.
    let doc = xot
        .parse(r#"<a xmlns="N"><b xmlns:n="N" n:c="C"/></a>"#)
        .unwrap();

    xot.deduplicate_namespaces(doc);

    // now serialize the doc; we haven't removed prefix n as it's in use by an
    // attribute
    assert_eq!(
        xot.to_string(doc).unwrap(),
        r#"<a xmlns="N"><b xmlns:n="N" n:c="C"/></a>"#
    );
}

#[test]
fn test_deduplicate_overlapping_namespace_element() {
    let mut xot = Xot::new();

    let doc = xot
        .parse(r#"<a xmlns="N"><b xmlns:n="N"><n:c/></b></a>"#)
        .unwrap();

    xot.deduplicate_namespaces(doc);

    // now serialize the doc; we have removed prefix n as it's in use by
    // an element only
    assert_eq!(
        xot.to_string(doc).unwrap(),
        r#"<a xmlns="N"><b><c/></b></a>"#
    );
}

#[test]
fn test_deduplicate_overlapping_namespace_deeper() {
    let mut xot = Xot::new();

    // we have a default namespace and an attribute in that namespace
    // deduplicate should not clean up the overlapping n prefix in this
    // case as it's in use by the attribute. Here we test the case
    // where this attribute is deeper down the tree, away from where the
    // prefix is initialized.
    let doc = xot
        .parse(r#"<a xmlns="N"><b xmlns:n="N"><sub n:c="C"/></b></a>"#)
        .unwrap();

    xot.deduplicate_namespaces(doc);

    // now serialize the doc
    // we haven't removed prefix n as it's in use by an attribute
    assert_eq!(
        xot.to_string(doc).unwrap(),
        r#"<a xmlns="N"><b xmlns:n="N"><sub n:c="C"/></b></a>"#
    );
}

#[test]
fn test_name_ns_str_no_namespace() {
    let mut xot = Xot::new();
    let doc = xot.parse(r#"<a/>"#).unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    let name = xot.element(doc_el).unwrap().name();
    assert_eq!(xot.name_ns_str(name), ("a", ""));
}

#[test]
fn test_name_ns_str_namespace() {
    let mut xot = Xot::new();
    let doc = xot.parse(r#"<a xmlns="http://example.com" />"#).unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    let name = xot.element(doc_el).unwrap().name();
    assert_eq!(xot.name_ns_str(name), ("a", "http://example.com"));
}

#[test]
fn test_create_missing_prefixes() {
    let mut xot = Xot::new();
    let doc = xot.parse(r#"<doc></doc>"#).unwrap();
    let root_id = xot.document_element(doc).unwrap();
    let ns_id = xot.add_namespace("http://example.com");
    let name_id = xot.add_name_ns("a", ns_id);
    xot.append_element(root_id, name_id).unwrap();
    xot.create_missing_prefixes(root_id).unwrap();
    assert_eq!(
        xot.to_string(doc).unwrap(),
        r#"<doc xmlns:n0="http://example.com"><n0:a/></doc>"#
    );
}

#[test]
fn test_create_missing_prefixes_traversal_order() {
    // the prefixes are numbered and declared in the order in which the
    // traversal finds the namespaces, attributes with their element, and a
    // namespace found again keeps its prefix; a hash set gave another order
    // from one set to the next, so try a few times
    for _ in 0..10 {
        let mut xot = Xot::new();
        let doc = xot.parse(r#"<doc></doc>"#).unwrap();
        let root_id = xot.document_element(doc).unwrap();
        let c_ns = xot.add_namespace("urn:c");
        let a_ns = xot.add_namespace("urn:a");
        let d_ns = xot.add_namespace("urn:d");
        let c_name = xot.add_name_ns("e", c_ns);
        let a_name = xot.add_name_ns("e", a_ns);
        let attribute_name = xot.add_name_ns("x", d_ns);
        let c = xot.new_element(c_name);
        let a = xot.new_element(a_name);
        xot.append(root_id, c).unwrap();
        xot.append(c, a).unwrap();
        xot.append_element(a, c_name).unwrap();
        xot.set_attribute(a, attribute_name, "1");
        xot.create_missing_prefixes(root_id).unwrap();
        assert_eq!(
            xot.to_string(doc).unwrap(),
            concat!(
                r#"<doc xmlns:n0="urn:c" xmlns:n1="urn:a" xmlns:n2="urn:d">"#,
                r#"<n0:e><n1:e n2:x="1"><n0:e/></n1:e></n0:e></doc>"#
            )
        );
    }
}

#[test]
fn test_create_missing_prefixes_keeps_prefix_in_scope() {
    // `n0` is in scope, so the new namespace gets `n1`; declaring `n0` for
    // it would leave `urn:other`, the namespace of `a`, without a prefix
    let mut xot = Xot::new();
    let doc = xot
        .parse(r#"<doc xmlns:n0="urn:other"><n0:a/></doc>"#)
        .unwrap();
    let root_id = xot.document_element(doc).unwrap();
    let ns_id = xot.add_namespace("urn:new");
    let name_id = xot.add_name_ns("b", ns_id);
    xot.append_element(root_id, name_id).unwrap();
    xot.create_missing_prefixes(doc).unwrap();
    let xml = xot.to_string(doc).unwrap();
    assert_eq!(
        xml,
        r#"<doc xmlns:n0="urn:other" xmlns:n1="urn:new"><n0:a/><n1:b/></doc>"#
    );
    // which reads back with the same names
    let doc = xot.parse(&xml).unwrap();
    let root_id = xot.document_element(doc).unwrap();
    let a = xot.first_child(root_id).unwrap();
    let b = xot.next_sibling(a).unwrap();
    assert_eq!(
        xot.name_ns_str(xot.element(a).unwrap().name()),
        ("a", "urn:other")
    );
    assert_eq!(
        xot.name_ns_str(xot.element(b).unwrap().name()),
        ("b", "urn:new")
    );
}

#[test]
fn test_create_missing_prefixes_keeps_prefix_of_ancestor() {
    // `n0` is declared on an ancestor of the element the prefix goes on
    let mut xot = Xot::new();
    let doc = xot
        .parse(r#"<doc xmlns:n0="urn:other"><x/><n0:a/></doc>"#)
        .unwrap();
    let root_id = xot.document_element(doc).unwrap();
    let x = xot.first_child(root_id).unwrap();
    let ns_id = xot.add_namespace("urn:new");
    let name_id = xot.add_name_ns("b", ns_id);
    xot.append_element(x, name_id).unwrap();
    xot.create_missing_prefixes(x).unwrap();
    assert_eq!(
        xot.to_string(doc).unwrap(),
        r#"<doc xmlns:n0="urn:other"><x xmlns:n1="urn:new"><n1:b/></x><n0:a/></doc>"#
    );
}

#[test]
fn test_create_missing_prefixes_keeps_prefix_declared_inside() {
    // `n0` is declared inside the tree, where a new `n0` would not be in
    // scope
    let mut xot = Xot::new();
    let doc = xot
        .parse(r#"<doc><x xmlns:n0="urn:x"><n0:y/></x></doc>"#)
        .unwrap();
    let root_id = xot.document_element(doc).unwrap();
    let x = xot.first_child(root_id).unwrap();
    let ns_id = xot.add_namespace("urn:new");
    let name_id = xot.add_name_ns("b", ns_id);
    xot.append_element(x, name_id).unwrap();
    xot.create_missing_prefixes(doc).unwrap();
    assert_eq!(
        xot.to_string(doc).unwrap(),
        r#"<doc xmlns:n1="urn:new"><x xmlns:n0="urn:x"><n0:y/><n1:b/></x></doc>"#
    );
}

#[test]
fn test_create_missing_prefixes_uses_prefix_of_ancestor() {
    // `p` is declared on an ancestor of the node, so `urn:p` misses no
    // prefix, and `p:t` keeps it
    let mut xot = Xot::new();
    let doc = xot
        .parse(r#"<r xmlns:p="urn:p"><s><p:t/></s></r>"#)
        .unwrap();
    let r = xot.document_element(doc).unwrap();
    let s = xot.first_child(r).unwrap();
    let ns_id = xot.add_namespace("urn:q");
    let name_id = xot.add_name_ns("u", ns_id);
    xot.append_element(s, name_id).unwrap();
    xot.create_missing_prefixes(s).unwrap();
    assert_eq!(
        xot.to_string(doc).unwrap(),
        r#"<r xmlns:p="urn:p"><s xmlns:n0="urn:q"><p:t/><n0:u/></s></r>"#
    );
}

#[test]
fn test_create_missing_prefixes_xml_prefix() {
    // `xml` is in scope without a declaration
    let mut xot = Xot::new();
    let doc = xot.parse(r#"<doc xml:lang="en"><a/></doc>"#).unwrap();
    xot.create_missing_prefixes(doc).unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    assert_eq!(xot.namespaces(doc_el).len(), 0);
    assert_eq!(
        xot.to_string(doc).unwrap(),
        r#"<doc xml:lang="en"><a/></doc>"#
    );
    // also on an element without a parent
    let a_name = xot.add_name("a");
    let a = xot.new_element(a_name);
    let lang = xot.add_name_ns("lang", xot.xml_namespace());
    xot.set_attribute(a, lang, "en");
    xot.create_missing_prefixes(a).unwrap();
    assert_eq!(xot.namespaces(a).len(), 0);
    assert_eq!(xot.to_string(a).unwrap(), r#"<a xml:lang="en"/>"#);
}

#[test]
fn test_unresolved_namespaces() {
    let mut xot = Xot::new();
    let doc = xot
        .parse(r#"<doc xmlns:a="http://example.com/a"><a:p/></doc>"#)
        .unwrap();
    let root_id = xot.document_element(doc).unwrap();
    let p_el = xot.first_child(root_id).unwrap();
    let a_ns = xot.add_namespace("http://example.com/a");

    assert_eq!(xot.unresolved_namespaces(p_el), [a_ns]);
}

#[test]
fn test_unresolved_namespaces_resolved() {
    let mut xot = Xot::new();
    let doc = xot
        .parse(r#"<doc xmlns:a="http://example.com/a"><a:p xmlns:a="http://example.com/b"/></doc>"#)
        .unwrap();
    let root_id = xot.document_element(doc).unwrap();
    let p_el = xot.first_child(root_id).unwrap();

    assert_eq!(xot.unresolved_namespaces(p_el), []);
}

#[test]
fn test_unresolved_namespaces_resolved_deeper() {
    let mut xot = Xot::new();
    let doc = xot
        .parse(r#"<doc xmlns:a="http://example.com/a"><a:p><b:x xmlns:b="http://example.com/x"/></a:p></doc>"#)
        .unwrap();
    let root_id = xot.document_element(doc).unwrap();
    let p_el = xot.first_child(root_id).unwrap();
    let a_ns = xot.add_namespace("http://example.com/a");

    assert_eq!(xot.unresolved_namespaces(p_el), [a_ns]);
}

#[test]
fn test_is_prefix_defined() {
    let mut xot = Xot::new();
    let doc = xot
        .parse(r#"<doc xmlns:a="http://example.com/a"><a:p/></doc>"#)
        .unwrap();
    let root_id = xot.document_element(doc).unwrap();
    let a_prefix = xot.add_prefix("a");
    let b_prefix = xot.add_prefix("b");
    let xml_prefix = xot.add_prefix("xml");
    let p0 = xot.first_child(root_id).unwrap();
    // let b_ns = xot.add_namespace("http://example.com/b");
    // let p1_name = xot.add_name_ns("p", b_ns);
    // xot.append_element(root_id, p1_name).unwrap();

    assert!(xot.is_prefix_defined(root_id, a_prefix));
    assert!(xot.is_prefix_defined(p0, a_prefix));
    assert!(!xot.is_prefix_defined(p0, b_prefix));
    assert!(xot.is_prefix_defined(root_id, xml_prefix));
}

#[test]
fn test_prefix_for_namespace() {
    let mut xot = Xot::new();
    let doc = xot
        .parse(r#"<doc xmlns:a="http://example.com/a"><a:p/></doc>"#)
        .unwrap();
    let root_id = xot.document_element(doc).unwrap();
    let a_prefix = xot.add_prefix("a");
    let a_ns = xot.add_namespace("http://example.com/a");
    let b_ns = xot.add_namespace("http://example.com/b");
    let xml_prefix = xot.add_prefix("xml");
    let xml_ns = xot.add_namespace("http://www.w3.org/XML/1998/namespace");
    let p0 = xot.first_child(root_id).unwrap();
    // add a p1 that doesn't have a prefix
    let p1_name = xot.add_name_ns("p", b_ns);
    xot.append_element(root_id, p1_name).unwrap();
    let p1 = xot.next_sibling(p0).unwrap();

    assert_eq!(xot.prefix_for_namespace(root_id, a_ns), Some(a_prefix));
    assert_eq!(xot.prefix_for_namespace(p0, a_ns), Some(a_prefix));
    assert_eq!(xot.prefix_for_namespace(p0, b_ns), None);
    assert_eq!(xot.prefix_for_namespace(p1, b_ns), None);
    assert_eq!(xot.prefix_for_namespace(p0, xml_ns), Some(xml_prefix));
}

#[test]
fn test_prefix_for_namespace_overridden() {
    let mut xot = Xot::new();
    let doc = xot
        .parse(r#"<doc xmlns:a="http://example.com/a"><a:p xmlns:a="http://example.com/b"/></doc>"#)
        .unwrap();
    let root_id = xot.document_element(doc).unwrap();
    let a_prefix = xot.add_prefix("a");
    let a_ns = xot.add_namespace("http://example.com/a");
    let b_ns = xot.add_namespace("http://example.com/b");
    let p = xot.first_child(root_id).unwrap();

    assert_eq!(xot.prefix_for_namespace(root_id, a_ns), Some(a_prefix));
    assert_eq!(xot.prefix_for_namespace(root_id, b_ns), None);
    assert_eq!(xot.prefix_for_namespace(p, b_ns), Some(a_prefix));
    assert_eq!(xot.prefix_for_namespace(p, a_ns), None);
}

#[test]
fn test_prefix_for_namespace_other_prefix_after_shadowed_one() {
    // `p` is declared again on `b`, so the outer `p` is not in scope there,
    // but `q`, which comes after it on `a`, is
    let mut xot = Xot::new();
    let doc = xot
        .parse(r#"<a xmlns:p="urn:A" xmlns:q="urn:C"><b xmlns:p="urn:B"/></a>"#)
        .unwrap();
    let a = xot.document_element(doc).unwrap();
    let b = xot.first_child(a).unwrap();
    let p_prefix = xot.add_prefix("p");
    let q_prefix = xot.add_prefix("q");
    let urn_a = xot.add_namespace("urn:A");
    let urn_b = xot.add_namespace("urn:B");
    let urn_c = xot.add_namespace("urn:C");

    assert_eq!(xot.prefix_for_namespace(b, urn_c), Some(q_prefix));
    assert_eq!(xot.prefix_for_namespace(b, urn_b), Some(p_prefix));
    assert_eq!(xot.prefix_for_namespace(b, urn_a), None);
    assert_eq!(xot.prefix_for_namespace(a, urn_a), Some(p_prefix));
}

#[test]
fn test_prefix_for_namespace_xml_after_shadowed_prefix() {
    // the `xml` prefix is in scope without a declaration
    let mut xot = Xot::new();
    let doc = xot
        .parse(r#"<a xmlns:p="urn:A"><b xmlns:p="urn:B"/></a>"#)
        .unwrap();
    let a = xot.document_element(doc).unwrap();
    let b = xot.first_child(a).unwrap();
    let xml_prefix = xot.add_prefix("xml");
    assert_eq!(
        xot.prefix_for_namespace(b, xot.xml_namespace()),
        Some(xml_prefix)
    );
}

#[test]
fn test_prefix_for_namespace_default_and_undeclared() {
    let mut xot = Xot::new();
    let doc = xot
        .parse(
            r#"<doc xmlns="http://example.com/a"><p><inner xmlns=""><inside/></inner></p></doc>"#,
        )
        .unwrap();
    let root_id = xot.document_element(doc).unwrap();

    let a_ns = xot.add_namespace("http://example.com/a");

    let p = xot.first_child(root_id).unwrap();
    let inner = xot.first_child(p).unwrap();
    let inside = xot.first_child(inner).unwrap();

    assert_eq!(
        xot.prefix_for_namespace(root_id, a_ns),
        Some(xot.empty_prefix())
    );
    assert_eq!(xot.prefix_for_namespace(p, a_ns), Some(xot.empty_prefix()));
    assert_eq!(xot.prefix_for_namespace(inner, a_ns), None);
    assert_eq!(xot.prefix_for_namespace(inside, a_ns), None);
}

#[test]
fn test_namespace_for_prefix() {
    let mut xot = Xot::new();
    let doc = xot
        .parse(r#"<doc xmlns:a="http://example.com/a"><a:p/></doc>"#)
        .unwrap();
    let root_id = xot.document_element(doc).unwrap();
    let a_prefix = xot.add_prefix("a");
    let a_ns = xot.add_namespace("http://example.com/a");
    let b_ns = xot.add_namespace("http://example.com/b");
    let b_prefix = xot.add_prefix("b");
    let xml_prefix = xot.add_prefix("xml");
    let xml_ns = xot.add_namespace("http://www.w3.org/XML/1998/namespace");
    let p0 = xot.first_child(root_id).unwrap();
    // add a p1 that doesn't have a prefix
    let p1_name = xot.add_name_ns("p", b_ns);
    xot.append_element(root_id, p1_name).unwrap();
    let p1 = xot.next_sibling(p0).unwrap();

    assert_eq!(xot.namespace_for_prefix(root_id, a_prefix), Some(a_ns));
    assert_eq!(xot.namespace_for_prefix(p0, a_prefix), Some(a_ns));
    assert_eq!(xot.namespace_for_prefix(p0, b_prefix), None);
    assert_eq!(xot.namespace_for_prefix(p1, b_prefix), None);
    assert_eq!(xot.namespace_for_prefix(p0, xml_prefix), Some(xml_ns));
}

#[test]
fn test_namespace_for_prefix_default_and_undeclared() {
    let mut xot = Xot::new();
    let doc = xot
        .parse(
            r#"<doc xmlns="http://example.com/a"><p><inner xmlns=""><inside/></inner></p></doc>"#,
        )
        .unwrap();
    let root_id = xot.document_element(doc).unwrap();

    let a_ns = xot.add_namespace("http://example.com/a");

    let p = xot.first_child(root_id).unwrap();
    let inner = xot.first_child(p).unwrap();
    let inside = xot.first_child(inner).unwrap();

    assert_eq!(
        xot.namespace_for_prefix(root_id, xot.empty_prefix()),
        Some(a_ns)
    );
    assert_eq!(xot.namespace_for_prefix(p, xot.empty_prefix()), Some(a_ns));
    assert_eq!(xot.namespace_for_prefix(inner, xot.empty_prefix()), None);
    assert_eq!(xot.namespace_for_prefix(inside, xot.empty_prefix()), None);
}

#[test]
fn test_namespaces() {
    let mut xot = Xot::new();
    let doc = xot
        .parse(r#"<doc xmlns:a="http://example.com/a"><a:p/></doc>"#)
        .unwrap();
    let root_id = xot.document_element(doc).unwrap();
    let a_prefix = xot.add_prefix("a");
    let a_ns = xot.add_namespace("http://example.com/a");
    let b_ns = xot.add_namespace("http://example.com/b");
    let xml_prefix = xot.add_prefix("xml");
    let xml_ns = xot.add_namespace("http://www.w3.org/XML/1998/namespace");
    let p0 = xot.first_child(root_id).unwrap();
    // add a p1 that doesn't have a prefix
    let p1_name = xot.add_name_ns("p", b_ns);
    xot.append_element(root_id, p1_name).unwrap();
    let p1 = xot.next_sibling(p0).unwrap();

    assert_eq!(
        xot.namespaces_in_scope(p0).collect::<Vec<_>>(),
        [(a_prefix, a_ns), (xml_prefix, xml_ns)]
    );
    assert_eq!(
        xot.namespaces_in_scope(root_id).collect::<Vec<_>>(),
        [(a_prefix, a_ns), (xml_prefix, xml_ns)]
    );

    assert_eq!(
        xot.namespaces_in_scope(p1).collect::<Vec<_>>(),
        [(a_prefix, a_ns), (xml_prefix, xml_ns)]
    );
}

#[test]
fn test_namespaces2() {
    let mut xot = Xot::new();
    let doc = xot
        .parse(r#"<doc xmlns="http://example.com/a"><p/></doc>"#)
        .unwrap();
    let root_id = xot.document_element(doc).unwrap();
    let empty_prefix = xot.empty_prefix();
    let a_ns = xot.add_namespace("http://example.com/a");
    let b_ns = xot.add_namespace("http://example.com/b");
    let xml_prefix = xot.add_prefix("xml");
    let xml_ns = xot.add_namespace("http://www.w3.org/XML/1998/namespace");
    let p0 = xot.first_child(root_id).unwrap();
    // add a p1 that doesn't have a prefix
    let p1_name = xot.add_name_ns("p", b_ns);
    xot.append_element(root_id, p1_name).unwrap();
    let p1 = xot.next_sibling(p0).unwrap();

    assert_eq!(
        xot.namespaces_in_scope(p0).collect::<Vec<_>>(),
        [(empty_prefix, a_ns), (xml_prefix, xml_ns)]
    );
    assert_eq!(
        xot.namespaces_in_scope(root_id).collect::<Vec<_>>(),
        [(empty_prefix, a_ns), (xml_prefix, xml_ns)]
    );

    assert_eq!(
        xot.namespaces_in_scope(p1).collect::<Vec<_>>(),
        [(empty_prefix, a_ns), (xml_prefix, xml_ns)]
    );
}

#[test]
fn test_namespaces_overrides() {
    let mut xot = Xot::new();
    let doc = xot
        .parse(r#"<doc xmlns:a="http://example.com/a"><a:p xmlns:a="http://example.com/b"/></doc>"#)
        .unwrap();
    let root_id = xot.document_element(doc).unwrap();
    let a_prefix = xot.add_prefix("a");
    let a_ns = xot.add_namespace("http://example.com/a");
    let b_ns = xot.add_namespace("http://example.com/b");
    let xml_prefix = xot.add_prefix("xml");
    let xml_ns = xot.add_namespace("http://www.w3.org/XML/1998/namespace");
    let p0 = xot.first_child(root_id).unwrap();
    // add a p1 that doesn't have a prefix
    let p1_name = xot.add_name_ns("p", b_ns);
    xot.append_element(root_id, p1_name).unwrap();
    let p1 = xot.next_sibling(p0).unwrap();

    assert_eq!(
        xot.namespaces_in_scope(p0).collect::<Vec<_>>(),
        [(a_prefix, b_ns), (xml_prefix, xml_ns)]
    );
    assert_eq!(
        xot.namespaces_in_scope(root_id).collect::<Vec<_>>(),
        [(a_prefix, a_ns), (xml_prefix, xml_ns)]
    );
    assert_eq!(
        xot.namespaces_in_scope(p1).collect::<Vec<_>>(),
        [(a_prefix, a_ns), (xml_prefix, xml_ns)]
    );
}

#[test]
fn test_namespaces_overrides_xml_prefix() {
    let mut xot = Xot::new();
    let doc = xot.parse(r#"<doc><p/></doc>"#).unwrap();
    let root_id = xot.document_element(doc).unwrap();
    let a_ns = xot.add_namespace("http://example.com/a");
    let xml_prefix = xot.add_prefix("xml");
    let xml_ns = xot.add_namespace("http://www.w3.org/XML/1998/namespace");
    let p0 = xot.first_child(root_id).unwrap();
    // Namespaces in XML 1.0 does not allow `xmlns:xml="http://example.com/a"`,
    // so the parser rejects it, but it can be made through the API
    xot.set_namespace(p0, xml_prefix, a_ns);

    assert_eq!(
        xot.namespaces_in_scope(p0).collect::<Vec<_>>(),
        [(xml_prefix, a_ns)]
    );
    assert_eq!(
        xot.namespaces_in_scope(root_id).collect::<Vec<_>>(),
        [(xml_prefix, xml_ns)]
    );
}

#[test]
fn test_default_namespace_undeclared() {
    let mut xot = Xot::new();
    let doc = xot
        .parse(r#"<doc><p xmlns="http://example.com/a"><inner xmlns=""><a/></inner></p></doc>"#)
        .unwrap();
    let xml_prefix = xot.add_prefix("xml");
    let xml_ns = xot.add_namespace("http://www.w3.org/XML/1998/namespace");
    let root_id = xot.document_element(doc).unwrap();
    let p = xot.first_child(root_id).unwrap();
    let inner = xot.first_child(p).unwrap();
    let a = xot.first_child(inner).unwrap();

    assert_eq!(
        xot.namespaces_in_scope(a).collect::<Vec<_>>(),
        [(xml_prefix, xml_ns)]
    );
    assert_eq!(
        xot.namespaces_in_scope(inner).collect::<Vec<_>>(),
        [(xml_prefix, xml_ns)]
    );
}

// Ids of names, prefixes and namespaces were 16 bits, so from the 65,537th
// entry on, a new entry got the id of an earlier one and read back as it.
const MANY: usize = 70_000;

#[test]
fn test_many_names_read_back() {
    let mut xot = Xot::new();
    let ids: Vec<_> = (0..MANY).map(|i| xot.add_name(&format!("n{i}"))).collect();
    for (i, id) in ids.iter().enumerate() {
        assert_eq!(xot.local_name_str(*id), format!("n{i}"));
    }
}

#[test]
fn test_many_prefixes_and_namespaces_read_back() {
    let mut xot = Xot::new();
    let prefixes: Vec<_> = (0..MANY)
        .map(|i| xot.add_prefix(&format!("p{i}")))
        .collect();
    let namespaces: Vec<_> = (0..MANY)
        .map(|i| xot.add_namespace(&format!("urn:n{i}")))
        .collect();
    for (i, (prefix, namespace)) in prefixes.iter().zip(&namespaces).enumerate() {
        assert_eq!(xot.prefix_str(*prefix), format!("p{i}"));
        assert_eq!(xot.namespace_str(*namespace), format!("urn:n{i}"));
    }
}

#[test]
fn test_many_element_names_survive_parse_and_serialize() {
    use std::fmt::Write;
    let mut xml = String::from("<doc>");
    for i in 0..MANY {
        write!(xml, "<e{i}/>").unwrap();
    }
    xml.push_str("</doc>");
    let mut xot = Xot::new();
    let root = xot.parse(&xml).unwrap();
    assert_eq!(xot.to_string(root).unwrap(), xml);
}
