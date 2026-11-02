// Reads of node values by value: `Xot::view` and the accessors that return
// values or string slices.

use xot::{
    AttributeView, Element, Namespace, NodeView, ProcessingInstructionView, Value, ValueType, Xot,
};

const XML: &str = r#"<!--before--><?pi data?><doc xmlns="urn:example:d" xmlns:p="urn:example:p" a="A" p:b="B">text<child/><!--c--><?empty?></doc>"#;

#[test]
fn test_view_of_every_kind_of_node() {
    let mut xot = Xot::new();
    let root = xot.parse(XML).unwrap();
    let doc_el = xot.document_element(root).unwrap();

    assert_eq!(xot.view(root), NodeView::Document);

    // the comment and the processing instruction before the document element
    let comment = xot.first_child(root).unwrap();
    assert_eq!(xot.view(comment), NodeView::Comment("before"));
    let pi = xot.next_sibling(comment).unwrap();
    let pi_target = xot.name("pi").unwrap();
    assert_eq!(
        xot.view(pi),
        NodeView::ProcessingInstruction(pi_target, Some("data"))
    );

    let doc_name = xot.name_ns("doc", xot.namespace("urn:example:d").unwrap());
    assert_eq!(xot.view(doc_el), NodeView::Element(doc_name.unwrap()));

    let text = xot.first_child(doc_el).unwrap();
    assert_eq!(xot.view(text), NodeView::Text("text"));
    let child = xot.next_sibling(text).unwrap();
    assert!(matches!(xot.view(child), NodeView::Element(_)));
    let inner_comment = xot.next_sibling(child).unwrap();
    assert_eq!(xot.view(inner_comment), NodeView::Comment("c"));
    let empty_pi = xot.next_sibling(inner_comment).unwrap();
    let empty_target = xot.name("empty").unwrap();
    assert_eq!(
        xot.view(empty_pi),
        NodeView::ProcessingInstruction(empty_target, None)
    );

    // attribute and namespace nodes
    let a = xot.name("a").unwrap();
    let a_node = xot.attributes(doc_el).get_node(a).unwrap();
    assert_eq!(xot.view(a_node), NodeView::Attribute(a, "A"));
    let p = xot.prefix("p").unwrap();
    let urn_p = xot.namespace("urn:example:p").unwrap();
    let p_node = xot.namespaces(doc_el).get_node(p).unwrap();
    assert_eq!(xot.view(p_node), NodeView::Namespace(p, urn_p));
}

#[test]
fn test_view_value_type_matches_value_type() {
    let mut xot = Xot::new();
    let root = xot.parse(XML).unwrap();
    let mut nodes: Vec<_> = xot.all_descendants(root).collect();
    nodes.extend(xot.all_descendants(root).flat_map(|n| {
        xot.attribute_nodes(n)
            .chain(xot.namespaces(n).nodes())
            .collect::<Vec<_>>()
    }));
    let mut kinds = Vec::new();
    for node in nodes {
        let view = xot.view(node);
        assert_eq!(view.value_type(), xot.value_type(node));
        if !kinds.contains(&view.value_type()) {
            kinds.push(view.value_type());
        }
    }
    for kind in [
        ValueType::Document,
        ValueType::Element,
        ValueType::Text,
        ValueType::Comment,
        ValueType::ProcessingInstruction,
        ValueType::Attribute,
        ValueType::Namespace,
    ] {
        assert!(kinds.contains(&kind), "{kind:?} not seen");
    }
}

#[test]
fn test_view_is_copy_and_comparable() {
    let mut xot = Xot::new();
    let root = xot.parse("<doc>text</doc>").unwrap();
    let doc_el = xot.document_element(root).unwrap();
    let text = xot.first_child(doc_el).unwrap();
    let view = xot.view(text);
    let copy = view;
    assert_eq!(view, copy);
    assert_ne!(view, NodeView::Text("other"));
    assert_ne!(view, xot.view(doc_el));
}

#[test]
fn test_element_by_value() {
    let mut xot = Xot::new();
    let root = xot.parse("<doc><child/>text</doc>").unwrap();
    let doc_el = xot.document_element(root).unwrap();
    let child = xot.first_child(doc_el).unwrap();
    let text = xot.next_sibling(child).unwrap();

    let doc_name = xot.name("doc").unwrap();
    let child_name = xot.name("child").unwrap();
    // `Element` is `Copy`: the value outlives the borrow of the tree
    let element: Element = xot.element(doc_el).unwrap();
    let again = element;
    xot.add_name("another");
    assert_eq!(element.name(), doc_name);
    assert_eq!(again, element);
    assert_eq!(xot.element(child).unwrap().name(), child_name);
    assert!(xot.element(text).is_none());
    assert!(xot.element(root).is_none());
    assert_eq!(xot.get_element_name(child), child_name);
}

#[test]
fn test_to_value_of_every_kind_of_node() {
    let mut xot = Xot::new();
    let root = xot
        .parse(
            r#"<!--c--><?pi data?><?empty?><doc xmlns:p="urn:example:p" a="A">text<child/></doc>"#,
        )
        .unwrap();
    let doc_el = xot.document_element(root).unwrap();
    let mut nodes = vec![root];
    nodes.extend(xot.children(root));
    nodes.extend(xot.namespaces(doc_el).nodes());
    nodes.extend(xot.attributes(doc_el).nodes());
    nodes.extend(xot.children(doc_el));
    // a long text, which the tree does not keep in the node itself
    let long = xot.new_text(&"x".repeat(100));
    nodes.push(long);
    let kinds = nodes
        .iter()
        .map(|node| xot.value_type(*node))
        .collect::<Vec<_>>();
    for kind in [
        ValueType::Document,
        ValueType::Element,
        ValueType::Text,
        ValueType::Comment,
        ValueType::ProcessingInstruction,
        ValueType::Attribute,
        ValueType::Namespace,
    ] {
        assert!(kinds.contains(&kind), "{kind:?}");
    }
    for node in nodes {
        let value = xot.view(node).to_value();
        assert_eq!(value.value_type(), xot.view(node).value_type());
        // the owned value equals the stored one, and reads back as the view
        assert_eq!(&value, &*xot.value_mut(node));
        let copy = xot.new_text("");
        *xot.value_mut(copy) = value;
        assert_eq!(xot.view(copy), xot.view(node), "{node:?}");
    }
}

#[test]
fn test_element_mut_changes_the_name() {
    let mut xot = Xot::new();
    let changed = xot.add_name("changed");
    let root = xot.parse("<doc><child/></doc>").unwrap();
    let doc_el = xot.document_element(root).unwrap();
    let child = xot.first_child(doc_el).unwrap();
    let child_name = xot.name("child").unwrap();
    let before = xot.element(child).unwrap();
    let mut element = xot.element_mut(child).unwrap();
    assert_eq!(element.name(), child_name);
    element.set_name(changed);
    assert_eq!(element.name(), changed);
    assert_eq!(xot.element(child).unwrap().name(), changed);
    assert_eq!(xot.view(child), NodeView::Element(changed));
    // the copy read before the change keeps the name it had
    assert_eq!(before.name(), child_name);
    assert!(xot.element_mut(root).is_none());
}

#[test]
fn test_namespace_node_mut_changes_the_namespace() {
    let mut xot = Xot::new();
    let root = xot
        .parse(r#"<doc xmlns:p="urn:example:p"><child/></doc>"#)
        .unwrap();
    let doc_el = xot.document_element(root).unwrap();
    let p = xot.prefix("p").unwrap();
    let urn_p = xot.namespace("urn:example:p").unwrap();
    let other = xot.add_namespace("urn:example:other");
    let node = xot.namespaces(doc_el).get_node(p).unwrap();
    let before = xot.namespace_node(node).unwrap();
    let mut namespace = xot.namespace_node_mut(node).unwrap();
    assert_eq!(namespace.prefix(), p);
    assert_eq!(namespace.namespace(), urn_p);
    namespace.set_namespace(other);
    assert_eq!(namespace.namespace(), other);
    assert_eq!(xot.namespace_node(node).unwrap().namespace(), other);
    assert_eq!(xot.view(node), NodeView::Namespace(p, other));
    assert_eq!(xot.namespaces(doc_el).get(p), Some(other));
    // the copy read before the change keeps the namespace it had
    assert_eq!(before.namespace(), urn_p);
    assert!(xot.namespace_node_mut(doc_el).is_none());
    assert_eq!(
        xot.to_string(root).unwrap(),
        r#"<doc xmlns:p="urn:example:other"><child/></doc>"#
    );
}

#[test]
fn test_text_str_with_consolidation() {
    let mut xot = Xot::new();
    let root = xot.parse("<doc/>").unwrap();
    let doc_el = xot.document_element(root).unwrap();
    xot.append_text(doc_el, "Alpha").unwrap();
    xot.append_text(doc_el, "Beta").unwrap();
    // one text node
    let text = xot.first_child(doc_el).unwrap();
    assert!(xot.next_sibling(text).is_none());
    assert_eq!(xot.text_str(text), Some("AlphaBeta"));
    assert_eq!(xot.view(text), NodeView::Text("AlphaBeta"));
    assert_eq!(xot.text_content_str(doc_el), Some("AlphaBeta"));
}

#[test]
fn test_text_str_without_consolidation() {
    let mut xot = Xot::new();
    xot.set_text_consolidation(false);
    let root = xot.parse("<doc/>").unwrap();
    let doc_el = xot.document_element(root).unwrap();
    xot.append_text(doc_el, "Alpha").unwrap();
    xot.append_text(doc_el, "Beta").unwrap();
    let texts: Vec<_> = xot.children(doc_el).collect();
    assert_eq!(texts.len(), 2);
    assert_eq!(xot.text_str(texts[0]), Some("Alpha"));
    assert_eq!(xot.text_str(texts[1]), Some("Beta"));
    assert_eq!(xot.view(texts[1]), NodeView::Text("Beta"));
    // two children: no single text content
    assert_eq!(xot.text_content_str(doc_el), None);
}

#[test]
fn test_text_content_str() {
    let mut xot = Xot::new();
    let root = xot
        .parse("<doc><a>Example</a><b/><c><x/></c><d><!--c--></d></doc>")
        .unwrap();
    let doc_el = xot.document_element(root).unwrap();
    let children: Vec<_> = xot.children(doc_el).collect();
    assert_eq!(xot.text_content_str(children[0]), Some("Example"));
    assert_eq!(xot.text_content_str(children[1]), Some(""));
    assert_eq!(xot.text_content_str(children[2]), None);
    assert_eq!(xot.text_content_str(children[3]), None);
}

#[test]
fn test_text_str_of_other_nodes_is_none() {
    let mut xot = Xot::new();
    let root = xot.parse(XML).unwrap();
    let doc_el = xot.document_element(root).unwrap();
    assert!(xot.text_str(root).is_none());
    assert!(xot.text_str(doc_el).is_none());
    let comment = xot.first_child(root).unwrap();
    assert!(xot.text_str(comment).is_none());
    assert_eq!(xot.comment_str(comment), Some("before"));
    assert!(xot.comment_str(doc_el).is_none());
}

#[test]
fn test_text_str_after_text_mut() {
    let mut xot = Xot::new();
    let root = xot.parse("<doc>old</doc>").unwrap();
    let doc_el = xot.document_element(root).unwrap();
    let text = xot.first_child(doc_el).unwrap();
    xot.text_mut(text).unwrap().set("new");
    assert_eq!(xot.text_str(text), Some("new"));
    xot.text_mut(text).unwrap().get_mut().push('!');
    assert_eq!(xot.view(text), NodeView::Text("new!"));
}

#[test]
fn test_comment_str_after_comment_mut() {
    let mut xot = Xot::new();
    let root = xot.parse("<doc><!--old--></doc>").unwrap();
    let doc_el = xot.document_element(root).unwrap();
    let comment = xot.first_child(doc_el).unwrap();
    xot.comment_mut(comment).unwrap().set("new").unwrap();
    assert_eq!(xot.comment_str(comment), Some("new"));
    assert_eq!(xot.view(comment), NodeView::Comment("new"));
}

#[test]
fn test_processing_instruction_accessor() {
    let mut xot = Xot::new();
    let root = xot.parse("<doc><?one data here?><?two?></doc>").unwrap();
    let doc_el = xot.document_element(root).unwrap();
    let one = xot.first_child(doc_el).unwrap();
    let two = xot.next_sibling(one).unwrap();

    let pi: ProcessingInstructionView = xot.processing_instruction(one).unwrap();
    assert_eq!(xot.local_name_str(pi.target()), "one");
    assert_eq!(pi.data(), Some("data here"));
    // the same as the view of the node
    assert_eq!(
        xot.view(one),
        NodeView::ProcessingInstruction(pi.target(), Some("data here"))
    );
    // the data borrows from the tree, not from the view
    let data: Option<&str> = xot.processing_instruction(one).unwrap().data();
    assert_eq!(data, Some("data here"));
    let pi = xot.processing_instruction(two).unwrap();
    assert_eq!(xot.local_name_str(pi.target()), "two");
    assert_eq!(pi.data(), None);
    assert!(xot.processing_instruction(doc_el).is_none());

    // changed through the mutable accessor
    xot.processing_instruction_mut(two)
        .unwrap()
        .set_data(Some("changed"));
    assert_eq!(
        xot.processing_instruction(two).unwrap().data(),
        Some("changed")
    );
}

#[test]
fn test_attribute_values_by_value() {
    let mut xot = Xot::new();
    let root = xot.parse(r#"<doc a="A" b="x &lt; y" c=""></doc>"#).unwrap();
    let doc_el = xot.document_element(root).unwrap();
    let a = xot.name("a").unwrap();
    let b = xot.name("b").unwrap();
    let c = xot.name("c").unwrap();
    let d = xot.add_name("d");

    assert_eq!(xot.get_attribute(doc_el, a), Some("A"));
    assert_eq!(xot.get_attribute(doc_el, b), Some("x < y"));
    assert_eq!(xot.get_attribute(doc_el, c), Some(""));
    assert_eq!(xot.get_attribute(doc_el, d), None);

    let attributes = xot.attributes(doc_el);
    // the map yields string slices
    let value: Option<&str> = attributes.get(a);
    assert_eq!(value, Some("A"));
    assert_eq!(attributes.get(d), None);
    assert_eq!(
        attributes.iter().collect::<Vec<_>>(),
        vec![(a, "A"), (b, "x < y"), (c, "")]
    );
    assert_eq!(attributes.values().collect::<Vec<_>>(), ["A", "x < y", ""]);
    assert_eq!(attributes.keys().collect::<Vec<_>>(), [a, b, c]);
    assert_eq!(attributes.len(), 3);
    // owned copies
    assert_eq!(
        attributes.to_vec(),
        vec![
            (a, "A".to_string()),
            (b, "x < y".to_string()),
            (c, String::new())
        ]
    );
    assert_eq!(
        attributes.to_hashmap().get(&b).map(String::as_str),
        Some("x < y")
    );

    // the attribute node accessor
    let node = attributes.get_node(b).unwrap();
    let attribute: AttributeView = xot.attribute_node(node).unwrap();
    assert_eq!(attribute.name(), b);
    assert_eq!(attribute.value(), "x < y");
    // the value borrows from the tree, not from the view
    let value: &str = xot.attribute_node(node).unwrap().value();
    assert_eq!(value, "x < y");
    assert_eq!(xot.view(node), NodeView::Attribute(b, "x < y"));
    assert_eq!(xot.attribute_node(doc_el), None);
}

#[test]
fn test_attribute_values_after_changes() {
    let mut xot = Xot::new();
    let root = xot.parse(r#"<doc a="A"/>"#).unwrap();
    let doc_el = xot.document_element(root).unwrap();
    let a = xot.name("a").unwrap();
    let e = xot.add_name("e");
    xot.attributes_mut(doc_el).insert(a, "changed".to_string());
    xot.attributes_mut(doc_el).insert(e, "new".to_string());
    assert_eq!(xot.get_attribute(doc_el, a), Some("changed"));
    assert_eq!(xot.get_attribute(doc_el, e), Some("new"));
    // the value of an attribute node, replaced through the mutable accessor
    let node = xot.attributes(doc_el).get_node(e).unwrap();
    xot.attribute_node_mut(node).unwrap().set_value("again");
    assert_eq!(xot.view(node), NodeView::Attribute(e, "again"));
}

#[test]
fn test_namespace_nodes_by_value() {
    let mut xot = Xot::new();
    let root = xot
        .parse(r#"<doc xmlns="urn:example:d" xmlns:p="urn:example:p"><child xmlns:p="urn:example:q"/></doc>"#)
        .unwrap();
    let doc_el = xot.document_element(root).unwrap();
    let child = xot.first_child(doc_el).unwrap();
    let p = xot.prefix("p").unwrap();
    let empty = xot.empty_prefix();
    let urn_d = xot.namespace("urn:example:d").unwrap();
    let urn_p = xot.namespace("urn:example:p").unwrap();
    let urn_q = xot.namespace("urn:example:q").unwrap();

    // the namespace map yields ids by value
    let namespaces = xot.namespaces(doc_el);
    let found: Option<xot::NamespaceId> = namespaces.get(p);
    assert_eq!(found, Some(urn_p));
    assert_eq!(namespaces.get(empty), Some(urn_d));
    assert_eq!(
        namespaces.iter().collect::<Vec<_>>(),
        vec![(empty, urn_d), (p, urn_p)]
    );
    assert_eq!(namespaces.values().collect::<Vec<_>>(), [urn_d, urn_p]);
    assert_eq!(xot.get_namespace(child, p), Some(urn_q));
    assert_eq!(xot.get_namespace(child, empty), None);
    assert_eq!(xot.namespace_for_prefix(child, p), Some(urn_q));
    assert_eq!(xot.prefix_for_namespace(child, urn_q), Some(p));

    // the namespace node accessor returns the node's value
    let node = namespaces.get_node(p).unwrap();
    let namespace: Namespace = xot.namespace_node(node).unwrap();
    assert_eq!(namespace.prefix(), p);
    assert_eq!(namespace.namespace(), urn_p);
    assert_eq!(xot.namespace_node(doc_el), None);
    assert_eq!(xot.view(node), NodeView::Namespace(p, urn_p));

    // a change through the mutable map shows in the next read
    xot.namespaces_mut(doc_el).insert(p, urn_q);
    assert_eq!(xot.namespaces(doc_el).get(p), Some(urn_q));
    assert_eq!(xot.view(node), NodeView::Namespace(p, urn_q));
}

#[test]
fn test_view_of_created_nodes() {
    let mut xot = Xot::new();
    let a = xot.add_name("a");
    let p = xot.add_prefix("p");
    let ns = xot.add_namespace("urn:example:n");
    let text = xot.new_text("t");
    let comment = xot.new_comment("c");
    let pi = xot.new_processing_instruction(a, Some("data"));
    let attribute = xot.new_attribute_node(a, "v".to_string());
    let namespace = xot.new_namespace_node(p, ns);
    let element = xot.new_element(a);
    assert_eq!(xot.view(text), NodeView::Text("t"));
    assert_eq!(xot.view(comment), NodeView::Comment("c"));
    assert_eq!(
        xot.view(pi),
        NodeView::ProcessingInstruction(a, Some("data"))
    );
    assert_eq!(xot.view(attribute), NodeView::Attribute(a, "v"));
    assert_eq!(xot.view(namespace), NodeView::Namespace(p, ns));
    assert_eq!(xot.view(element), NodeView::Element(a));
}

#[test]
fn test_view_after_value_mut() {
    let mut xot = Xot::new();
    let root = xot.parse("<doc>text</doc>").unwrap();
    let doc_el = xot.document_element(root).unwrap();
    let text = xot.first_child(doc_el).unwrap();
    if let Value::Text(value) = xot.value_mut(text) {
        value.set("changed");
    }
    assert_eq!(xot.view(text), NodeView::Text("changed"));
}
