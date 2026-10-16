use xot::{Node, Xot};

#[test]
fn test_text_content_str() {
    let mut xot = Xot::new();
    let doc = xot.parse(r#"<a>text</a>"#).unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    assert_eq!(xot.text_content_str(doc_el), Some("text"));
}

#[test]
fn test_text_content_str_no_text() {
    let mut xot = Xot::new();
    let doc = xot.parse(r#"<a/>"#).unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    assert_eq!(xot.text_content_str(doc_el), Some(""));
}

#[test]
fn test_text_content_str_mixed_content() {
    let mut xot = Xot::new();
    let doc = xot.parse(r#"<a>text<b/></a>"#).unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    assert_eq!(xot.text_content_str(doc_el), None);
}

#[test]
fn test_compare() {
    let mut xot = Xot::new();
    let doc1 = xot.parse(r#"<a>text</a>"#).unwrap();
    let doc2 = xot.parse(r#"<a>text</a>"#).unwrap();

    assert!(xot.deep_equal(doc1, doc2));
}

#[test]
fn test_compare_different_text() {
    let mut xot = Xot::new();
    let doc1 = xot.parse(r#"<a>text A</a>"#).unwrap();
    let doc2 = xot.parse(r#"<a>text B</a>"#).unwrap();

    assert!(!xot.deep_equal(doc1, doc2));
}

#[test]
fn test_compare_different_structure() {
    let mut xot = Xot::new();
    let doc1 = xot.parse(r#"<a></a>"#).unwrap();
    let doc2 = xot.parse(r#"<a><b/></a>"#).unwrap();

    assert!(!xot.deep_equal(doc1, doc2));
}

#[test]
fn test_compare_different_namespace() {
    let mut xot = Xot::new();
    let doc1 = xot
        .parse(r#"<a xmlns="http://example.com/a"></a>"#)
        .unwrap();
    let doc2 = xot
        .parse(r#"<a xmlns="http://example.com/b"></a>"#)
        .unwrap();

    assert!(!xot.deep_equal(doc1, doc2));
}

#[test]
fn test_compare_same_attributes() {
    let mut xot = Xot::new();
    let doc1 = xot.parse(r#"<a x="X"></a>"#).unwrap();
    let doc2 = xot.parse(r#"<a x="X"></a>"#).unwrap();

    assert!(xot.deep_equal(doc1, doc2));
}

#[test]
fn test_compare_different_attributes() {
    let mut xot = Xot::new();
    let doc1 = xot.parse(r#"<a x="X"></a>"#).unwrap();
    let doc2 = xot.parse(r#"<a x="Y"></a>"#).unwrap();

    assert!(!xot.deep_equal(doc1, doc2));
}

#[test]
fn test_compare_different_attributes_extra() {
    let mut xot = Xot::new();
    let doc1 = xot.parse(r#"<a x="X"></a>"#).unwrap();
    let doc2 = xot.parse(r#"<a x="X" y="Y"></a>"#).unwrap();

    assert!(!xot.deep_equal(doc1, doc2));
}

#[test]
fn test_compare_value_the_same_structure_different() {
    let mut xot = Xot::new();
    let doc1 = xot.parse(r#"<article><body><sec id="1"><sec id="2"><title>T1</title><p>P1</p><p>P2</p></sec></sec><sec id="3"><title>T2</title><p>P3</p></sec></body></article>"#
    ).unwrap();
    let doc2 = xot.parse(r#"<article><body><sec id="1"><sec id="2"><title>T1</title><p>P1</p><p>P2</p></sec><sec id="3"><title>T2</title><p>P3</p></sec></sec></body></article>"#).unwrap();
    assert!(!xot.deep_equal(doc1, doc2));
}

#[test]
fn test_compare_children() {
    let mut xot = Xot::new();
    let root1 = xot
        .parse(r#"<a><b z="Y">Alpha<x/>Gamma</b><b>Alpha<x/></b></a>"#)
        .unwrap();
    let root2 = xot.parse(r#"<a><b z="Z">Alpha<x/>Gamma</b></a>"#).unwrap();
    let doc1 = xot.document_element(root1).unwrap();
    let doc2 = xot.document_element(root2).unwrap();
    let doc1_b0 = xot.first_child(doc1).unwrap();
    let doc1_b1 = xot.next_sibling(doc1_b0).unwrap();
    let doc2_b0 = xot.first_child(doc2).unwrap();

    assert!(xot.deep_equal_children(doc1_b0, doc2_b0));
    assert!(!xot.deep_equal_children(doc1_b1, doc2_b0));
}

#[test]
fn test_advanced_compare_text() {
    let mut xot = Xot::new();
    let doc1 = xot.parse(r#"<a>text</a>"#).unwrap();
    let doc2 = xot.parse(r#"<a>TEXT</a>"#).unwrap();
    // case insensitive compare
    assert!(xot.advanced_deep_equal(
        doc1,
        doc2,
        |_| true,
        |a, b| a.to_lowercase() == b.to_lowercase()
    ));
}

#[test]
fn test_advanced_compare_text2() {
    let mut xot = Xot::new();
    let doc1 = xot.parse(r#"<a>text</a>"#).unwrap();
    let doc2 = xot.parse(r#"<a>different</a>"#).unwrap();

    // case insensitive compare doesn't matter, it's still different
    assert!(!xot.advanced_deep_equal(
        doc1,
        doc2,
        |_| true,
        |a, b| a.to_lowercase() == b.to_lowercase()
    ));
}

#[test]
fn test_advanced_compare_attribute_text() {
    let mut xot = Xot::new();
    let doc1 = xot.parse(r#"<a alpha="alpha">text</a>"#).unwrap();
    let doc2 = xot.parse(r#"<a alpha="ALPHA">text</a>"#).unwrap();
    // no text compares as equal, so this is not equal
    assert!(xot.advanced_deep_equal(
        doc1,
        doc2,
        |_| true,
        |a, b| a.to_lowercase() == b.to_lowercase()
    ));
}

#[test]
fn test_advanced_compare_filter() {
    let mut xot = Xot::new();
    let doc1 = xot.parse(r#"<a>text<!--comment--></a>"#).unwrap();
    let doc2 = xot.parse(r#"<a>text</a>"#).unwrap();
    // compare, disregarding comments
    assert!(xot.advanced_deep_equal(doc1, doc2, |node| !xot.is_comment(node), |a, b| a == b));
}

#[test]
fn test_first_child_with_namespace() {
    let mut xot = Xot::new();
    let doc = xot.parse(r#"<doc xmlns:foo="FOO">bar</doc>"#).unwrap();
    let element = xot.document_element(doc).unwrap();
    let value = xot.value(xot.first_child(element).unwrap());
    if let xot::Value::Text(text) = value {
        assert_eq!(text.get(), "bar");
    } else {
        unreachable!();
    }
}

#[test]
fn test_first_child_with_attribute() {
    let mut xot = Xot::new();
    let doc = xot.parse(r#"<doc foo="FOO">bar</doc>"#).unwrap();
    let element = xot.document_element(doc).unwrap();
    let value = xot.value(xot.first_child(element).unwrap());
    if let xot::Value::Text(text) = value {
        assert_eq!(text.get(), "bar");
    } else {
        unreachable!();
    }
}

#[test]
fn test_first_child_missing_with_attribute() {
    let mut xot = Xot::new();
    let doc = xot.parse(r#"<doc foo="FOO"/>"#).unwrap();
    let element = xot.document_element(doc).unwrap();
    assert_eq!(xot.first_child(element), None);
}

#[test]
fn test_last_child_with_attribute() {
    let mut xot = Xot::new();
    let doc = xot.parse(r#"<doc foo="FOO">bar</doc>"#).unwrap();
    let element = xot.document_element(doc).unwrap();
    let value = xot.value(xot.last_child(element).unwrap());
    if let xot::Value::Text(text) = value {
        assert_eq!(text.get(), "bar");
    } else {
        unreachable!();
    }
}

#[test]
fn test_last_child_missing_with_attribute() {
    let mut xot = Xot::new();
    let doc = xot.parse(r#"<doc foo="FOO"/>"#).unwrap();
    let element = xot.document_element(doc).unwrap();
    assert_eq!(xot.last_child(element), None);
}

#[test]
fn test_previous_sibling_with_attribute() {
    let mut xot = Xot::new();
    let doc = xot.parse(r#"<doc foo="FOO">bar</doc>"#).unwrap();
    let doc = xot.document_element(doc).unwrap();
    let bar = xot.first_child(doc).unwrap();
    assert_eq!(xot.previous_sibling(bar), None);
}

#[test]
fn test_next_sibling_with_attribute() {
    let mut xot = Xot::new();
    let doc = xot.parse(r#"<doc foo="FOO">bar</doc>"#).unwrap();
    let doc = xot.document_element(doc).unwrap();
    let bar = xot.first_child(doc).unwrap();
    assert_eq!(xot.next_sibling(bar), None);
}

#[test]
fn test_children() {
    let mut xot = Xot::new();
    let doc = xot.parse(r#"<doc foo="FOO">bar</doc>"#).unwrap();
    let doc = xot.document_element(doc).unwrap();
    let children = xot.children(doc).collect::<Vec<_>>();
    assert_eq!(children.len(), 1);
}

/// `reverse_children`, cut off one node after the number of children, so
/// that an iterator that repeats a child fails the test instead of never
/// ending.
fn reverse_children(xot: &Xot, node: Node) -> Vec<Node> {
    let count = xot.children(node).count();
    xot.reverse_children(node).take(count + 1).collect()
}

#[test]
fn test_reverse_children_none() {
    let mut xot = Xot::new();
    let doc = xot.parse(r#"<doc/>"#).unwrap();
    let doc = xot.document_element(doc).unwrap();
    assert_eq!(reverse_children(&xot, doc), vec![]);
}

#[test]
fn test_reverse_children_one() {
    let mut xot = Xot::new();
    let doc = xot.parse(r#"<doc><a/></doc>"#).unwrap();
    let doc = xot.document_element(doc).unwrap();
    let a = xot.first_child(doc).unwrap();
    assert_eq!(reverse_children(&xot, doc), vec![a]);
}

#[test]
fn test_reverse_children_two() {
    let mut xot = Xot::new();
    let doc = xot.parse(r#"<doc><a/>text</doc>"#).unwrap();
    let doc = xot.document_element(doc).unwrap();
    let a = xot.first_child(doc).unwrap();
    let text = xot.next_sibling(a).unwrap();
    assert_eq!(reverse_children(&xot, doc), vec![text, a]);
}

#[test]
fn test_reverse_children_three() {
    let mut xot = Xot::new();
    let doc = xot.parse(r#"<doc><a/>text<!--comment--></doc>"#).unwrap();
    let doc = xot.document_element(doc).unwrap();
    let a = xot.first_child(doc).unwrap();
    let text = xot.next_sibling(a).unwrap();
    let comment = xot.next_sibling(text).unwrap();
    assert_eq!(reverse_children(&xot, doc), vec![comment, text, a]);
}

#[test]
fn test_reverse_children_with_namespace_and_attribute() {
    let mut xot = Xot::new();
    let doc = xot
        .parse(r#"<doc xmlns:p="urn:p" p:x="1" y="2"><a/>text<?pi data?></doc>"#)
        .unwrap();
    let doc = xot.document_element(doc).unwrap();
    let a = xot.first_child(doc).unwrap();
    let text = xot.next_sibling(a).unwrap();
    let pi = xot.next_sibling(text).unwrap();
    assert_eq!(reverse_children(&xot, doc), vec![pi, text, a]);
}

#[test]
fn test_reverse_children_one_with_attribute() {
    let mut xot = Xot::new();
    let doc = xot.parse(r#"<doc foo="FOO">bar</doc>"#).unwrap();
    let doc = xot.document_element(doc).unwrap();
    let bar = xot.first_child(doc).unwrap();
    assert_eq!(reverse_children(&xot, doc), vec![bar]);
}

#[test]
fn test_reverse_children_one_with_namespace() {
    let mut xot = Xot::new();
    let doc = xot.parse(r#"<doc xmlns:foo="FOO">bar</doc>"#).unwrap();
    let doc = xot.document_element(doc).unwrap();
    let bar = xot.first_child(doc).unwrap();
    assert_eq!(reverse_children(&xot, doc), vec![bar]);
}

#[test]
fn test_reverse_children_none_with_namespace_and_attribute() {
    let mut xot = Xot::new();
    let doc = xot
        .parse(r#"<doc xmlns:p="urn:p" p:x="1" y="2"/>"#)
        .unwrap();
    let doc = xot.document_element(doc).unwrap();
    assert_eq!(reverse_children(&xot, doc), vec![]);
    for attribute in xot.attribute_nodes(doc).collect::<Vec<_>>() {
        assert_eq!(reverse_children(&xot, attribute), vec![]);
    }
}

#[test]
fn test_reverse_children_document() {
    let mut xot = Xot::new();
    let root = xot
        .parse(r#"<?pi data?><!--before--><doc><a/></doc><!--after-->"#)
        .unwrap();
    let pi = xot.first_child(root).unwrap();
    let before = xot.next_sibling(pi).unwrap();
    let doc = xot.next_sibling(before).unwrap();
    let after = xot.next_sibling(doc).unwrap();
    assert_eq!(reverse_children(&xot, root), vec![after, doc, before, pi]);
}

#[test]
fn test_reverse_children_is_children_reversed() {
    let mut xot = Xot::new();
    let root = xot
        .parse(concat!(
            r#"<?pi?><doc xmlns="urn:d" xmlns:p="urn:p" a="1">"#,
            r#"<p:a p:b="2">x<b/>y</p:a><!--c--><c xmlns:q="urn:q"><d/></c>z"#,
            r#"</doc><!--after-->"#
        ))
        .unwrap();
    for node in xot.all_descendants(root) {
        let mut children = xot.children(node).collect::<Vec<_>>();
        children.reverse();
        assert_eq!(reverse_children(&xot, node), children);
    }
}

#[test]
fn test_reverse_children_after_changes() {
    let mut xot = Xot::new();
    let doc = xot.add_name("doc");
    let a = xot.add_name("a");
    let x = xot.add_name("x");
    let p = xot.add_prefix("p");
    let ns = xot.add_namespace("urn:p");
    let doc = xot.new_element(doc);
    let first = xot.new_text("first");
    let second = xot.new_element(a);
    let third = xot.new_comment("third");
    xot.append(doc, second).unwrap();
    xot.prepend(doc, first).unwrap();
    xot.insert_after(second, third).unwrap();
    // the attribute and namespace nodes go before the children
    xot.set_attribute(doc, x, "1");
    xot.set_namespace(doc, p, ns);
    assert_eq!(reverse_children(&xot, doc), vec![third, second, first]);
    xot.remove(second).unwrap();
    assert_eq!(reverse_children(&xot, doc), vec![third, first]);
    xot.detach(third).unwrap();
    assert_eq!(reverse_children(&xot, doc), vec![first]);
    xot.remove(first).unwrap();
    assert_eq!(reverse_children(&xot, doc), vec![]);
}

#[test]
fn test_document_element_for_fragment_with_single_element() {
    let mut xot = Xot::new();
    let doc = xot.parse_fragment(r#"<a>text</a>"#).unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    assert_eq!(xot.text_content_str(doc_el), Some("text"));
}

#[test]
fn test_document_element_for_fragment_with_multiple_elements() {
    let mut xot = Xot::new();
    let doc = xot.parse_fragment(r#"<a>text</a><b/>"#).unwrap();
    let doc_el = xot.document_element(doc).unwrap();
    assert_eq!(xot.text_content_str(doc_el), Some("text"));
}

#[test]
fn test_document_element_for_fragment_without_elements() {
    let mut xot = Xot::new();
    let doc = xot.parse_fragment(r#""#).unwrap();
    assert!(matches!(
        xot.document_element(doc),
        Err(xot::Error::NoElementAtTopLevel)
    ));
}
