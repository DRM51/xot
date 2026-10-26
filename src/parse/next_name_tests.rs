//! Tests of the table of next names (`NameIdBuilder::next_names`): a parse
//! makes the same name, prefix and namespace ids, the same tree and the same
//! error with the table as without it.
use super::{NameIdBuilder, NEXT_NAMES_AFTER, NEXT_NAMES_AT_ONCE, WITHOUT_NEXT_NAMES};
use crate::{NameId, Xot};

/// What a parse in a new `Xot` made: its result (the serialization, or
/// the error) and the names of all ids in id order.
fn parse_report(xml: &str, fragment: bool, without_table: bool) -> (String, Vec<(String, String)>) {
    WITHOUT_NEXT_NAMES.with(|w| w.set(without_table));
    let mut xot = Xot::new();
    let parsed = if fragment {
        xot.parse_fragment(xml)
    } else {
        xot.parse(xml)
    };
    WITHOUT_NEXT_NAMES.with(|w| w.set(false));
    let result = match parsed {
        Ok(root) => {
            let mut nodes = String::new();
            for node in xot.all_descendants(root) {
                nodes.push_str(&format!("{:?}\n", xot.value(node)));
            }
            format!("{}\n{}", xot.to_string(root).unwrap(), nodes)
        }
        Err(error) => format!("{:?} {}", error, error),
    };
    // a name that no parse makes gets the next id: the ids below it are
    // the names made so far
    let end = xot.add_name("\u{0}end");
    let names = (0..end.0)
        .map(|id| {
            let (local, namespace) = xot.name_ns_str(NameId(id));
            (local.to_string(), namespace.to_string())
        })
        .collect();
    (result, names)
}

fn assert_same_ids(xml: &str) {
    for fragment in [false, true] {
        let with_table = parse_report(xml, fragment, false);
        let without_table = parse_report(xml, fragment, true);
        assert_eq!(with_table, without_table, "{}", xml);
    }
}

#[test]
fn record_shaped_repeats() {
    let mut xml = String::from(r#"<root xmlns:x="urn:example:x">"#);
    for i in 0..500 {
        xml.push_str(&format!(
            r#"<record id="{i}" x:kind="k"><item>{i}</item><tag a="1">t</tag><x:f0/><row/></record>"#
        ));
        if i % 7 == 0 {
            // a record with its fields in another order now and then
            xml.push_str(r#"<record><tag/><item/><row x:kind="j"/></record>"#);
        }
    }
    xml.push_str("</root>");
    assert_same_ids(&xml);
    let (_, names) = parse_report(&xml, false, false);
    let locals: Vec<&str> = names.iter().map(|(local, _)| local.as_str()).collect();
    assert_eq!(
        &locals[2..],
        ["root", "record", "id", "kind", "item", "tag", "a", "f0", "row"]
    );
}

#[test]
fn names_whose_ids_are_256_apart() {
    // names n0 .. n599 first in order, so that n0, n256 and n512 share a
    // slot of the table, then in an order that makes their slots collide
    let mut xml = String::from("<root>");
    for i in 0..600 {
        xml.push_str(&format!("<n{i}/>"));
    }
    let mut state: u64 = 7;
    for _ in 0..5000 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let base = (state % 256) as usize;
        let i = base + 256 * ((state >> 20) % 3) as usize;
        let j = ((state >> 32) % 600) as usize;
        xml.push_str(&format!("<n{i}/><n{j}/>"));
    }
    // the same predecessor id modulo 256, followed by other names
    xml.push_str("<n0/><n1/><n256/><n1/><n256/><n2/><n0/><n2/><n512/><n1/><n512/><n2/>");
    xml.push_str("</root>");
    assert_same_ids(&xml);
}

#[test]
fn prefix_bound_again_in_a_nested_scope() {
    assert_same_ids(
        r#"<r xmlns:p="urn:example:a"><p:x/><p:y/><p:x/><p:y xmlns:p="urn:example:b"/><p:x/><p:y/><p:x xmlns:p="urn:example:b"><p:y/><p:x xmlns:p="urn:example:a"><p:y/></p:x><p:y/></p:x><p:y/></r>"#,
    );
    assert_same_ids(
        r#"<r xmlns="urn:example:a"><x/><y/><x xmlns="urn:example:b"><y/></x><x/><y/><x xmlns=""><y/></x><y/></r>"#,
    );
}

#[test]
fn attributes_in_no_namespace_beside_elements_in_a_default_namespace() {
    assert_same_ids(
        r#"<a xmlns="urn:example:a" a="1"><a a="2"/><a a="3"><b a="4" b="5"><a b="6"/></b></a><b b="7" a="8"/></a>"#,
    );
    assert_same_ids(
        r#"<p:a xmlns:p="urn:example:a" xmlns="urn:example:a" a="1" p:a="2"><a a="3" p:a="4"/><p:a p:a="5" a="6"/></p:a>"#,
    );
}

#[test]
fn end_tag_with_another_prefix() {
    assert_same_ids(
        r#"<r xmlns:p="urn:example:a" xmlns:q="urn:example:a"><p:x></q:x><p:x><p:y/></q:x><q:x></p:x></r>"#,
    );
    // an end tag of another name, or with a prefix that is not declared
    assert_same_ids(r#"<r xmlns:p="urn:example:a" xmlns:q="urn:example:b"><p:x></q:x></r>"#);
    assert_same_ids(r#"<r xmlns:p="urn:example:a"><p:x></z:x></r>"#);
    assert_same_ids(r#"<r xmlns:p="urn:example:a"><p:x><p:y/></p:z></r>"#);
}

#[test]
fn processing_instructions_and_failed_parses() {
    assert_same_ids(r#"<?pi a?><r><?pi b?><pi/><?r?><r/></r><?pi?>"#);
    assert_same_ids(r#"<r><a/><b/><a/><c:d/></r>"#);
    assert_same_ids(r#"<r><a/><b/><a/><b c="1" c="2"/></r>"#);
    assert_same_ids(r#"<r><a/><b/><a/><b"#);
}

#[test]
fn table_made_only_after_the_first_names() {
    let mut xot = Xot::new();
    let no_namespace = xot.no_namespace_id;
    NEXT_NAMES_AT_ONCE.with(|at_once| at_once.set(false));
    let mut builder = NameIdBuilder::new(&xot);
    let names = ["record", "item", "tag"];
    // no table for the first names
    for i in 0..NEXT_NAMES_AFTER {
        builder.name_id(no_namespace, names[i % 3], &mut xot);
        assert!(builder.next_names.is_none(), "{i}");
    }
    // the next lookup makes it, with that name as the one asked for last
    let id = builder.name_id(no_namespace, names[NEXT_NAMES_AFTER % 3], &mut xot);
    NEXT_NAMES_AT_ONCE.with(|at_once| at_once.set(true));
    let next_names = builder.next_names.as_ref().unwrap();
    assert_eq!(next_names.last, id);
    assert!(next_names.table.iter().all(|next| next.is_none()));
    // a name after it goes into the table, and is found there the next time
    let after = builder.name_id(no_namespace, "row", &mut xot);
    let next_names = builder.next_names.as_mut().unwrap();
    assert_eq!(
        next_names
            .table
            .iter()
            .filter(|next| next.is_some())
            .count(),
        1
    );
    next_names.last = id;
    assert_eq!(builder.name_id(no_namespace, "row", &mut xot), after);
    assert_eq!(builder.next_names.as_ref().unwrap().last, after);
}

#[test]
fn same_ids_on_either_side_of_the_threshold() {
    for count in [
        0,
        1,
        NEXT_NAMES_AFTER - 1,
        NEXT_NAMES_AFTER,
        NEXT_NAMES_AFTER + 1,
        200,
    ] {
        let mut xml = String::from(r#"<root xmlns:x="urn:example:x">"#);
        for i in 0..count {
            // names repeat in groups of three, with one in another order now
            // and then
            match i % 5 {
                4 => xml.push_str(r#"<tag a="1"/>"#),
                _ => xml.push_str(&format!(r#"<row x:f{}="1"/>"#, i % 2)),
            }
        }
        xml.push_str("</root>");
        for fragment in [false, true] {
            let without_table = parse_report(&xml, fragment, true);
            NEXT_NAMES_AT_ONCE.with(|at_once| at_once.set(false));
            let lazy_table = parse_report(&xml, fragment, false);
            NEXT_NAMES_AT_ONCE.with(|at_once| at_once.set(true));
            let table_at_once = parse_report(&xml, fragment, false);
            assert_eq!(lazy_table, without_table, "{count}");
            assert_eq!(table_at_once, without_table, "{count}");
        }
    }
}

#[cfg(feature = "proptest")]
mod generated {
    use proptest::prelude::*;

    use super::*;
    use crate::proptest::arb_xml_document;

    proptest! {
        #[test]
        fn generated_documents(document in arb_xml_document()) {
            let mut xot = Xot::new();
            let node = document.xotify(&mut xot);
            xot.create_missing_prefixes(node).unwrap();
            let xml = xot.to_string(node).unwrap();
            for fragment in [false, true] {
                let with_table = parse_report(&xml, fragment, false);
                let without_table = parse_report(&xml, fragment, true);
                prop_assert_eq!(with_table, without_table);
            }
        }
    }
}
