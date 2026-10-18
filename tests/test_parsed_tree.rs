use xot::{Node, ParsedTree, Value, Xot};

/// The kept tree of `root`, once its places are found to be are the document
/// order of `root` and that every node of it gives it.
fn check_tree(xot: &Xot, root: Node) -> ParsedTree {
    let tree = xot.parsed_tree(root).expect("kept");
    assert_eq!(tree.root(), root);
    assert_eq!(xot.parent(root), None);
    let nodes: Vec<Node> = xot.all_descendants(root).collect();
    assert_eq!(nodes.len(), tree.node_count());
    for (i, n) in nodes.iter().enumerate() {
        assert_eq!(xot.parsed_tree(*n), Some(tree));
        assert_eq!(tree.index(*n), Some(i));
        assert_eq!(tree.node(i), Some(*n));
    }
    assert_eq!(tree.node(nodes.len()), None);
    assert_eq!(tree.node(usize::MAX), None);
    tree
}

const DOC: &str = r#"<r xmlns:p="urn:p" a="1"><p:b x="2" y="3">t<c/>u</p:b><!--c--><?pi d?><d xmlns="urn:d"><e/></d></r>"#;

#[test]
fn a_parse_keeps_its_tree_and_index_is_document_order() {
    let mut xot = Xot::new();
    let a = xot.parse(DOC).unwrap();
    let tree_a = check_tree(&xot, a);
    assert_eq!(tree_a.node_count(), 15);
    let b = xot.parse_fragment("t<s/>u<!--c-->").unwrap();
    let tree_b = check_tree(&xot, b);
    assert_ne!(tree_a, tree_b);
    assert!(tree_a < tree_b);
    assert_eq!(tree_a.index(b), None);
    assert_eq!(tree_b.index(a), None);
    let c = xot.parse("<k/>").unwrap();
    let tree_c = check_tree(&xot, c);
    // a node made through the API is in no parsed tree
    let name = xot.add_name("n");
    let n = xot.new_element(name);
    assert_eq!(xot.parsed_tree(n), None);
    assert!(xot.dropped_parsed_trees().is_empty());
    // a failed parse keeps nothing; the next parse comes after the made
    // node and the nodes of the failed parse, so it is not kept either
    assert!(xot.parse("<a><b></a>").is_err());
    let d = xot.parse("<z><y/></z>").unwrap();
    assert_eq!(xot.parsed_tree(d), None);
    assert_eq!(xot.parsed_tree(xot.document_element(d).unwrap()), None);
    // the trees kept before stay kept
    assert_eq!(check_tree(&xot, a), tree_a);
    assert_eq!(check_tree(&xot, b), tree_b);
    assert_eq!(check_tree(&xot, c), tree_c);
    assert!(xot.dropped_parsed_trees().is_empty());
}

#[test]
fn every_parse_that_gives_a_document_keeps_its_tree() {
    let mut xot = Xot::new();
    let a = xot.parse(DOC).unwrap();
    let b = xot.parse_fragment("x<y/>").unwrap();
    let (c, _) = xot.parse_with_span_info("<s><t/></s>").unwrap();
    let (d, _) = xot.parse_fragment_with_span_info("<s/><t/>").unwrap();
    let e = xot.parse_bytes(b"<u v='1'/>").unwrap();
    let trees: Vec<ParsedTree> = [a, b, c, d, e]
        .into_iter()
        .map(|doc| check_tree(&xot, doc))
        .collect();
    // each begins right after the one before
    for pair in trees.windows(2) {
        let last = pair[0].node(pair[0].node_count() - 1).unwrap();
        assert_eq!(pair[1].index(last), None);
        assert!(pair[0] < pair[1]);
    }
}

#[test]
fn a_free_slot_means_no_keep_until_it_is_taken() {
    let mut xot = Xot::new();
    let a = xot.parse("<r><s/></r>").unwrap();
    let tree_a = xot.parsed_tree(a).unwrap();
    let r = xot.document_element(a).unwrap();
    let s = xot.first_child(r).unwrap();
    xot.remove(s).unwrap();
    assert_eq!(xot.parsed_tree(a), None);
    assert_eq!(xot.dropped_parsed_trees(), &[tree_a]);
    let b = xot.parse("<q/>").unwrap();
    assert_eq!(xot.parsed_tree(b), None);
    // the free slot is taken now
    let c = xot.parse("<w/>").unwrap();
    check_tree(&xot, c);
    assert_eq!(xot.dropped_parsed_trees(), &[tree_a]);
}

#[test]
fn a_tree_over_several_chunks() {
    let mut xml = String::from("<r xmlns:p='urn:p'>");
    for i in 0..30_000 {
        xml.push_str(&format!("<p:e n='{i}'>t{i}</p:e>"));
    }
    xml.push_str("</r>");
    let mut xot = Xot::new();
    let small = xot.parse("<s/>").unwrap();
    let doc = xot.parse(&xml).unwrap();
    let tree = check_tree(&xot, doc);
    assert!(tree.node_count() > 65_536 + 2);
    check_tree(&xot, small);
}

#[test]
fn parses_one_after_another_are_kept() {
    let mut xot = Xot::new();
    let a = xot.parse(DOC).unwrap();
    let b = xot.parse(DOC).unwrap();
    check_tree(&xot, a);
    check_tree(&xot, b);

    // a node made between the two
    let mut xot = Xot::new();
    let a = xot.parse(DOC).unwrap();
    let name = xot.add_name("n");
    xot.new_element(name);
    let b = xot.parse(DOC).unwrap();
    check_tree(&xot, a);
    assert_eq!(xot.parsed_tree(b), None);
}

#[test]
fn a_parse_after_the_free_slots_are_taken_is_kept() {
    let mut xot = Xot::new();
    let a = xot.parse(DOC).unwrap();
    let tree_a = xot.parsed_tree(a).unwrap();
    xot.remove(a).unwrap();
    assert_eq!(xot.dropped_parsed_trees(), &[tree_a]);
    // a parse now takes free slots: not kept
    let name = xot.add_name("n");
    let b = xot.parse("<b/>").unwrap();
    assert_eq!(xot.parsed_tree(b), None);
    // take every free slot that is left
    for _ in 0..tree_a.node_count() - 2 {
        xot.new_element(name);
    }
    let c = xot.parse(DOC).unwrap();
    check_tree(&xot, c);
    assert_eq!(xot.dropped_parsed_trees(), &[tree_a]);
}

#[test]
fn a_change_in_a_later_tree_drops_it_and_the_trees_after_it() {
    let mut xot = Xot::new();
    let a = xot.parse(DOC).unwrap();
    let b = xot.parse(DOC).unwrap();
    let c = xot.parse(DOC).unwrap();
    let (tree_a, tree_b, tree_c) = (
        check_tree(&xot, a),
        check_tree(&xot, b),
        check_tree(&xot, c),
    );
    let root_b = xot.document_element(b).unwrap();
    xot.append_text(root_b, "x").unwrap();
    assert_eq!(xot.dropped_parsed_trees(), &[tree_b, tree_c]);
    assert_eq!(check_tree(&xot, a), tree_a);
    assert_eq!(xot.parsed_tree(b), None);
    assert_eq!(xot.parsed_tree(c), None);
    // a parse now comes after a dropped tree: not kept
    let d = xot.parse(DOC).unwrap();
    assert_eq!(xot.parsed_tree(d), None);
    assert_eq!(check_tree(&xot, a), tree_a);
}

#[test]
fn a_change_in_the_first_tree_drops_it_alone() {
    let mut xot = Xot::new();
    let a = xot.parse(DOC).unwrap();
    let b = xot.parse(DOC).unwrap();
    let c = xot.parse(DOC).unwrap();
    let (tree_a, tree_b, tree_c) = (
        check_tree(&xot, a),
        check_tree(&xot, b),
        check_tree(&xot, c),
    );
    let root_a = xot.document_element(a).unwrap();
    xot.detach(root_a).unwrap();
    assert_eq!(xot.dropped_parsed_trees(), &[tree_a]);
    assert_eq!(check_tree(&xot, b), tree_b);
    assert_eq!(check_tree(&xot, c), tree_c);
    // a parse right after the kept trees is kept
    let d = xot.parse(DOC).unwrap();
    check_tree(&xot, d);
    // so is a change in the last tree, which drops it alone
    let root_d = xot.document_element(d).unwrap();
    let tree_d = xot.parsed_tree(d).unwrap();
    xot.remove(root_d).unwrap();
    assert_eq!(xot.dropped_parsed_trees(), &[tree_a, tree_d]);
    assert_eq!(check_tree(&xot, b), tree_b);
    assert_eq!(check_tree(&xot, c), tree_c);
}

#[test]
fn removing_many_kept_documents_in_order() {
    let mut xot = Xot::new();
    let docs: Vec<Node> = (0..300)
        .map(|i| xot.parse(&format!("<r n='{i}'><a/></r>")).unwrap())
        .collect();
    let trees: Vec<ParsedTree> = docs.iter().map(|&d| check_tree(&xot, d)).collect();
    for (i, &doc) in docs.iter().enumerate() {
        xot.remove(doc).unwrap();
        assert_eq!(xot.dropped_parsed_trees(), &trees[..=i]);
        if let Some(&next) = docs.get(i + 1) {
            assert_eq!(check_tree(&xot, next), trees[i + 1]);
        }
    }
    // the slots are free: no keep now
    let doc = xot.parse("<r/>").unwrap();
    assert_eq!(xot.parsed_tree(doc), None);
}

#[test]
fn a_root_turned_into_an_element_and_moved_drops_its_tree() {
    type Op = fn(&mut Xot, Node, Node);
    let ops: [(&str, Op); 3] = [
        ("append", |xot, outer, doc| xot.append(outer, doc).unwrap()),
        ("insert_before", |xot, outer, doc| {
            let child = xot.first_child(outer).unwrap();
            xot.insert_before(child, doc).unwrap()
        }),
        ("insert_after", |xot, outer, doc| {
            let child = xot.first_child(outer).unwrap();
            xot.insert_after(child, doc).unwrap()
        }),
    ];
    for (what, op) in ops {
        let mut xot = Xot::new();
        let doc = xot.parse(DOC).unwrap();
        let other = xot.parse("<h><c/></h>").unwrap();
        let outer = xot.document_element(other).unwrap();
        let tree = xot.parsed_tree(doc).unwrap();
        let tree_of_outer = xot.parsed_tree(outer).unwrap();
        let name = xot.add_name("x");
        let element = xot.new_element(name);
        let value = xot.value(element).clone();
        assert!(matches!(value, Value::Element(_)));
        *xot.value_mut(doc) = value;
        // a change of a value: still kept, the root checked at keep time only
        assert_eq!(check_tree(&xot, doc), tree, "{what}");
        op(&mut xot, outer, doc);
        assert_eq!(xot.parsed_tree(doc), None, "{what}");
        assert_eq!(xot.parsed_tree(outer), None, "{what}");
        // both trees change
        let dropped = xot.dropped_parsed_trees();
        assert_eq!(dropped.len(), 2, "{what}");
        assert!(dropped.contains(&tree), "{what}");
        assert!(dropped.contains(&tree_of_outer), "{what}");
    }
}

#[test]
fn a_kept_root_moved_into_a_tree_that_is_not_kept_drops_its_tree() {
    let mut xot = Xot::new();
    let name = xot.add_name("outer");
    let outer = xot.new_element(name);
    let doc = xot.parse(DOC).unwrap();
    let tree = xot.parsed_tree(doc).unwrap();
    let r = xot.document_element(doc).unwrap();
    // the element moves out: only the check on the moved node sees it
    xot.append(outer, r).unwrap();
    assert_eq!(xot.dropped_parsed_trees(), &[tree]);
}

fn nth(xot: &Xot, doc: Node, i: usize) -> Node {
    xot.all_descendants(doc).nth(i).unwrap()
}

fn element_named(xot: &Xot, doc: Node, local: &str) -> Node {
    xot.all_descendants(doc)
        .find(|n| {
            xot.element(*n)
                .is_some_and(|e| xot.local_name_str(e.name()) == local)
        })
        .unwrap()
}

type Change = fn(&mut Xot, Node);

/// The changes of the structure, each applied to a tree parsed from `DOC`.
fn structural_changes() -> Vec<(&'static str, Change)> {
    vec![
        ("append element", |xot, doc| {
            let r = xot.document_element(doc).unwrap();
            let n = xot.add_name("n");
            let e = xot.new_element(n);
            xot.append(r, e).unwrap();
        }),
        ("append text not joined", |xot, doc| {
            let r = xot.document_element(doc).unwrap();
            xot.append_text(r, "x").unwrap();
        }),
        ("prepend", |xot, doc| {
            let r = xot.document_element(doc).unwrap();
            let n = xot.add_name("n");
            let e = xot.new_element(n);
            xot.prepend(r, e).unwrap();
        }),
        ("insert_before", |xot, doc| {
            let c = nth(xot, doc, 8);
            let n = xot.add_name("n");
            let e = xot.new_element(n);
            xot.insert_before(c, e).unwrap();
        }),
        ("insert_after", |xot, doc| {
            let c = nth(xot, doc, 8);
            let n = xot.add_name("n");
            let e = xot.new_element(n);
            xot.insert_after(c, e).unwrap();
        }),
        ("detach", |xot, doc| {
            let c = nth(xot, doc, 8);
            xot.detach(c).unwrap();
        }),
        ("remove", |xot, doc| {
            let c = nth(xot, doc, 8);
            xot.remove(c).unwrap();
        }),
        ("new attribute", |xot, doc| {
            let r = xot.document_element(doc).unwrap();
            let n = xot.add_name("new");
            xot.attributes_mut(r).insert(n, "v".to_string());
        }),
        ("first attribute", |xot, doc| {
            let c = element_named(xot, doc, "c");
            let n = xot.add_name("new");
            xot.attributes_mut(c).insert(n, "v".to_string());
        }),
        ("first namespace", |xot, doc| {
            let c = element_named(xot, doc, "c");
            let p = xot.add_prefix("q");
            let ns = xot.add_namespace("urn:q");
            xot.namespaces_mut(c).insert(p, ns);
        }),
        ("remove attribute", |xot, doc| {
            let r = xot.document_element(doc).unwrap();
            let n = xot.add_name("a");
            xot.remove_attribute(r, n);
        }),
        ("new namespace", |xot, doc| {
            let r = xot.document_element(doc).unwrap();
            let p = xot.add_prefix("q");
            let ns = xot.add_namespace("urn:q");
            xot.namespaces_mut(r).insert(p, ns);
        }),
        ("element_wrap", |xot, doc| {
            let c = nth(xot, doc, 8);
            let n = xot.add_name("w");
            xot.element_wrap(c, n).unwrap();
        }),
        ("element_unwrap", |xot, doc| {
            let c = nth(xot, doc, 8);
            xot.element_unwrap(c).unwrap();
        }),
        ("move into another tree", |xot, doc| {
            let c = nth(xot, doc, 8);
            let n = xot.add_name("n");
            let e = xot.new_element(n);
            xot.append(e, c).unwrap();
        }),
        ("move within the tree", |xot, doc| {
            let c = nth(xot, doc, 8);
            let e = nth(xot, doc, 14);
            xot.append(e, c).unwrap();
        }),
        ("remove the document", |xot, doc| {
            xot.remove(doc).unwrap();
        }),
        ("remove insignificant whitespace", |xot, doc| {
            let r = xot.document_element(doc).unwrap();
            xot.append_text(r, "  ").unwrap();
            xot.remove_insignificant_whitespace(doc);
        }),
    ]
}

#[test]
fn structural_changes_drop_the_tree_and_the_trees_after_it() {
    let changes = structural_changes();
    assert_eq!(changes.len(), 18);
    for (name, change) in changes {
        // the changed tree first, then one in the middle
        for head in [false, true] {
            let mut xot = Xot::new();
            let head_tree = head.then(|| {
                let h = xot.parse("<head/>").unwrap();
                xot.parsed_tree(h).unwrap()
            });
            let doc = xot.parse(DOC).unwrap();
            let tree = xot.parsed_tree(doc).unwrap();
            let other = xot.parse("<other><x/></other>").unwrap();
            let other = xot.parsed_tree(other).unwrap();
            let third = xot.parse("<third/>").unwrap();
            let third = xot.parsed_tree(third).unwrap();
            change(&mut xot, doc);
            if !xot.is_removed(doc) {
                assert_eq!(xot.parsed_tree(doc), None, "{name}");
            }
            if let Some(h) = head_tree {
                assert_eq!(xot.dropped_parsed_trees(), &[tree, other, third], "{name}");
                assert_eq!(check_tree(&xot, h.root()), h, "{name}");
            } else {
                assert_eq!(xot.dropped_parsed_trees(), &[tree], "{name}");
                assert_eq!(check_tree(&xot, other.root()), other, "{name}");
                assert_eq!(check_tree(&xot, third.root()), third, "{name}");
            }
        }
    }
}

#[test]
fn value_changes_and_reads_keep_the_tree() {
    let mut xot = Xot::new();
    let doc = xot.parse(DOC).unwrap();
    let third = xot.parse("<third/>").unwrap();
    let tree = check_tree(&xot, doc);
    let r = xot.document_element(doc).unwrap();
    // the value of an attribute that is there
    let a = xot.add_name("a");
    xot.attributes_mut(r).insert(a, "changed".to_string());
    *xot.attributes_mut(r).get_mut(a).unwrap() = "again".to_string();
    // the namespace of a prefix that is declared
    let p = xot.add_prefix("p");
    let ns = xot.add_namespace("urn:other");
    xot.namespaces_mut(r).insert(p, ns);
    // a text, a name, a comment, a processing instruction
    let t = nth(&xot, doc, 7);
    xot.text_mut(t).unwrap().set("new text");
    let n = xot.add_name("renamed");
    let c = nth(&xot, doc, 8);
    xot.set_element_name(c, n);
    xot.comment_mut(nth(&xot, doc, 10))
        .unwrap()
        .set("other")
        .unwrap();
    xot.processing_instruction_mut(nth(&xot, doc, 11))
        .unwrap()
        .set_data(Some("other"));
    // a text appended after a text joins it: the tree keeps its nodes
    let b = nth(&xot, doc, 4);
    xot.append_text(b, "more").unwrap();
    // copies, and their removal
    let copy = xot.clone_node(r);
    let _ = xot.clone_with_prefixes(c);
    xot.remove(copy).unwrap();
    // reads
    let _ = xot.to_string(doc);
    let _ = xot.string_value(doc);
    check_tree(&xot, doc);
    check_tree(&xot, third);
    assert_eq!(xot.parsed_tree(doc), Some(tree));
    assert!(xot.dropped_parsed_trees().is_empty());
}

#[test]
fn a_clone_of_the_xot_keeps_the_trees() {
    let mut xot = Xot::new();
    let a = xot.parse(DOC).unwrap();
    let b = xot.parse(DOC).unwrap();
    let tree_a = xot.parsed_tree(a).unwrap();
    let root_b = xot.document_element(b).unwrap();
    let tree_b = xot.parsed_tree(b).unwrap();
    xot.detach(root_b).unwrap();
    let mut copy = xot.clone();
    assert_eq!(check_tree(&copy, a), tree_a);
    assert_eq!(copy.parsed_tree(b), None);
    assert_eq!(copy.dropped_parsed_trees(), &[tree_b]);
    // the copy goes on by itself
    let root_a = copy.document_element(a).unwrap();
    copy.remove(root_a).unwrap();
    assert_eq!(copy.parsed_tree(a), None);
    assert_eq!(copy.dropped_parsed_trees(), &[tree_b, tree_a]);
    assert_eq!(check_tree(&xot, a), tree_a);
    assert_eq!(xot.dropped_parsed_trees(), &[tree_b]);
}

#[test]
fn a_dropped_tree_is_never_kept_again() {
    let mut xot = Xot::new();
    let a = xot.parse(DOC).unwrap();
    let tree_a = xot.parsed_tree(a).unwrap();
    let r = xot.document_element(a).unwrap();
    let detached = xot.first_child(r).unwrap();
    let index = xot.all_descendants(a).position(|n| n == detached);
    xot.detach(detached).unwrap();
    // put back as it was
    xot.prepend(r, detached).unwrap();
    assert_eq!(xot.parsed_tree(a), None);
    assert_eq!(xot.dropped_parsed_trees(), &[tree_a]);
    // indexes describe the tree as parsed
    assert_eq!(tree_a.index(detached), index);
    assert_eq!(tree_a.node(index.unwrap()), Some(detached));
}

// inserting an element after or before its own descendant panics after the
// element was detached; the tree it was detached from is dropped all the same
#[test]
fn an_insert_that_panics_after_its_detach_drops_the_tree() {
    for after in [true, false] {
        let mut xot = Xot::new();
        let doc = xot.parse("<r><a><b/></a></r>").unwrap();
        let r = xot.document_element(doc).unwrap();
        let a = xot.first_child(r).unwrap();
        let b = xot.first_child(a).unwrap();
        check_tree(&xot, doc);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if after {
                let _ = xot.insert_after(b, a);
            } else {
                let _ = xot.insert_before(b, a);
            }
        }));
        assert!(result.is_err());
        assert_eq!(xot.parent(a), None);
        assert_eq!(xot.parsed_tree(doc), None);
        assert_eq!(xot.parsed_tree(b), None);
        assert_eq!(xot.dropped_parsed_trees().len(), 1);
    }
}
