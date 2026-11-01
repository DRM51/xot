// The incremental XML writer against the tree serializer: a walk of a tree
// that feeds the writer must give the bytes of `serialize_xml_write`, or the
// same error.

use std::cell::Cell;

use proptest::prelude::*;
use proptest::test_runner::{Config, TestRunner};

use xot::output::xml::{Declaration, DocType, Parameters, Writer};
use xot::output::{Indentation, NoopNormalizer};
use xot::{NameId, NamespaceId, Node, NodeEdge, PrefixId, ValueType, Xot};

/// Write `root` (a document or an element) with the writer, as a caller
/// that walks the tree would: the namespace nodes of each element as its
/// declarations, `has_child` from its first child, CDATA from the name of
/// the parent of a text.
fn write_by_walk(xot: &Xot, parameters: &Parameters, root: Node) -> Result<Vec<u8>, xot::Error> {
    let mut writer = Writer::new(xot, parameters, Vec::new(), NoopNormalizer)?;
    let mut declarations: Vec<(PrefixId, NamespaceId)> = Vec::new();
    for edge in xot.traverse(root) {
        match edge {
            NodeEdge::Start(node) => match xot.value_type(node) {
                ValueType::Element => {
                    declarations.clear();
                    declarations.extend(xot.namespaces(node).iter().map(|(p, n)| (p, *n)));
                    let has_child = xot.first_child(node).is_some();
                    writer.start_element(xot, node, &declarations, has_child)?;
                }
                ValueType::Text => {
                    let cdata = xot
                        .parent(node)
                        .filter(|parent| xot.is_element(*parent))
                        .is_some_and(|parent| {
                            writer.is_cdata_section_element(xot.get_element_name(parent))
                        });
                    writer.text(xot, xot.text_str(node).unwrap(), cdata, node)?;
                }
                ValueType::Comment => writer.comment(xot, xot.comment_str(node).unwrap(), node)?,
                ValueType::ProcessingInstruction => {
                    let pi = xot.processing_instruction(node).unwrap();
                    writer.processing_instruction(xot, pi.target(), pi.data(), node)?;
                }
                _ => {}
            },
            NodeEdge::End(node) => {
                if xot.is_element(node) {
                    writer.end_element(xot, xot.first_child(node).is_some())?;
                }
            }
        }
    }
    Ok(writer.into_inner())
}

/// What the tree serializer writes.
fn write_tree(xot: &Xot, parameters: &Parameters, root: Node) -> Result<Vec<u8>, xot::Error> {
    let mut buf = Vec::new();
    xot.serialize_xml_write(parameters.clone(), root, &mut buf)?;
    Ok(buf)
}

#[derive(Debug, PartialEq)]
enum Serialized {
    Bytes(Vec<u8>),
    Error(String, String),
}

fn serialized(result: Result<Vec<u8>, xot::Error>) -> Serialized {
    match result {
        Ok(bytes) => Serialized::Bytes(bytes),
        Err(e) => Serialized::Error(format!("{:?}", e), e.to_string()),
    }
}

/// Compare the writer with the tree serializer; true when both wrote bytes.
fn same(xot: &Xot, parameters: &Parameters, root: Node) -> Result<bool, TestCaseError> {
    let tree = serialized(write_tree(xot, parameters, root));
    let writer = serialized(write_by_walk(xot, parameters, root));
    prop_assert_eq!(&writer, &tree);
    Ok(matches!(tree, Serialized::Bytes(_)))
}

// A random tree, written from scratch: names in a few namespaces, the
// reserved prefixes and namespaces among the declarations, texts with
// characters that need escapes or that XML does not allow, comments and
// processing instructions (with the target `xml` among them).

const LOCALS: &[&str] = &["a", "b", "c", "xmlns", "xml"];
const NAMESPACES: &[&str] = &[
    "",
    "urn:example:a",
    "urn:example:b",
    "urn:example:c\"&<",
    "http://www.w3.org/XML/1998/namespace",
    "http://www.w3.org/2000/xmlns/",
    "urn:example:\u{1}",
];
// the prefixes p, q and r go with the namespaces 1, 2 and 3 when an element
// declares the namespaces of its names
const PREFIXES: &[&str] = &["", "p", "q", "r", "xml", "xmlns"];

#[derive(Debug, Clone)]
enum Content {
    Element {
        name: (usize, usize),
        // declare the namespaces of the names of the element first, the
        // element's own with the empty prefix or not
        declare_names: Option<bool>,
        declarations: Vec<(usize, usize)>,
        attributes: Vec<((usize, usize), String)>,
        children: Vec<Content>,
    },
    Text(String),
    Comment(String),
    Pi(usize, Option<String>),
}

fn arb_string() -> impl Strategy<Value = String> {
    // simple characters and ones that need escaping; rarely a string with
    // characters that XML 1.0 does not allow
    let simple = || prop::sample::select(vec!['x', 'y', ' ', '1', 'é', '中']);
    let escaped = || {
        prop::sample::select(vec![
            '<', '>', '&', '"', '\'', ']', '\t', '\n', '\r', '\u{85}', '\u{2028}', '\u{7f}',
            '\u{9f}',
        ])
    };
    let clean = prop_oneof![3 => simple(), 2 => escaped()];
    let any = prop_oneof![
        6 => simple(),
        3 => escaped(),
        1 => prop::sample::select(vec!['\u{1}', '\u{b}', '\u{fffe}', '\u{ffff}']),
    ];
    prop_oneof![
        40 => prop::collection::vec(clean, 0..8),
        1 => prop::collection::vec(any, 1..8),
    ]
    .prop_map(|cs| cs.into_iter().collect())
}

fn arb_name() -> impl Strategy<Value = (usize, usize)> {
    // mostly no namespace or a usual one; the reserved ones are rare
    let ns = prop_oneof![
        60 => 0..4usize,
        1 => 4..NAMESPACES.len(),
    ];
    (ns, prop_oneof![60 => 0..3usize, 1 => 3..LOCALS.len()])
}

fn arb_declaration() -> impl Strategy<Value = (usize, usize)> {
    // a prefix for a usual namespace, the default namespace undeclared, and
    // rarely anything else (reserved prefixes and namespaces, a prefix
    // undeclared)
    prop_oneof![
        50 => (0..4usize, 1..4usize),
        10 => Just((0, 0)),
        1 => (0..PREFIXES.len(), 0..NAMESPACES.len()),
    ]
}

fn arb_element(children: impl Strategy<Value = Vec<Content>>) -> impl Strategy<Value = Content> {
    (
        arb_name(),
        prop_oneof![
            12 => any::<bool>().prop_map(Some),
            1 => Just(None),
        ],
        prop::collection::vec(arb_declaration(), 0..3),
        prop::collection::vec((arb_name(), arb_string()), 0..3),
        children,
    )
        .prop_map(
            |(name, declare_names, declarations, attributes, children)| Content::Element {
                name,
                declare_names,
                declarations,
                attributes,
                children,
            },
        )
}

fn arb_content() -> impl Strategy<Value = Content> {
    let leaf = prop_oneof![
        4 => arb_string().prop_map(Content::Text),
        1 => arb_string()
            .prop_filter("a comment cannot hold --", |s| !s.contains("--") && !s.ends_with('-'))
            .prop_map(Content::Comment),
        1 => (prop_oneof![60 => 0..1usize, 1 => 1..3usize], prop::option::of(arb_string()))
            .prop_map(|(t, d)| Content::Pi(t, d)),
    ];
    leaf.prop_recursive(5, 48, 5, |inner| {
        arb_element(prop::collection::vec(inner, 0..5))
    })
}

fn arb_root() -> impl Strategy<Value = (Content, Vec<Content>)> {
    (
        arb_element(prop::collection::vec(arb_content(), 0..5)),
        // comments and processing instructions after the document element
        prop::collection::vec(
            prop_oneof![
                arb_string()
                    .prop_filter("no --", |s| !s.contains("--") && !s.ends_with('-'))
                    .prop_map(Content::Comment),
                (0..3usize, prop::option::of(arb_string())).prop_map(|(t, d)| Content::Pi(t, d)),
            ],
            0..2,
        ),
    )
}

#[derive(Debug, Clone)]
struct Params {
    declaration: Option<(Option<String>, Option<bool>)>,
    cdata: Vec<(usize, usize)>,
    unescaped_gt: bool,
    unescaped_cr: bool,
}

fn arb_params() -> impl Strategy<Value = Params> {
    (
        prop::option::of((
            prop::option::of(prop::sample::select(vec![
                "UTF-8".to_string(),
                "x".to_string(),
            ])),
            prop::option::of(any::<bool>()),
        )),
        prop::collection::vec(arb_name(), 0..3),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(|(declaration, cdata, unescaped_gt, unescaped_cr)| Params {
            declaration,
            cdata,
            unescaped_gt,
            unescaped_cr,
        })
}

struct Ids {
    namespaces: Vec<NamespaceId>,
    prefixes: Vec<PrefixId>,
}

fn ids(xot: &mut Xot) -> Ids {
    Ids {
        namespaces: NAMESPACES.iter().map(|ns| xot.add_namespace(ns)).collect(),
        prefixes: PREFIXES.iter().map(|p| xot.add_prefix(p)).collect(),
    }
}

fn name(xot: &mut Xot, ids: &Ids, (ns, local): (usize, usize)) -> NameId {
    xot.add_name_ns(LOCALS[local], ids.namespaces[ns])
}

fn make_node(xot: &mut Xot, ids: &Ids, content: &Content) -> Node {
    match content {
        Content::Element {
            name: n,
            declare_names,
            declarations,
            attributes,
            children,
        } => {
            let element_name = name(xot, ids, *n);
            let element = xot.new_element(element_name);
            for (prefix, ns) in declarations {
                xot.namespaces_mut(element)
                    .insert(ids.prefixes[*prefix], ids.namespaces[*ns]);
            }
            if let Some(unprefixed) = declare_names {
                // the usual namespaces of the names, each with its prefix
                for (ns, _) in std::iter::once(n).chain(attributes.iter().map(|(n, _)| n)) {
                    if (1..=3).contains(ns) {
                        xot.namespaces_mut(element)
                            .insert(ids.prefixes[*ns], ids.namespaces[*ns]);
                    }
                }
                if *unprefixed && (1..=3).contains(&n.0) {
                    xot.namespaces_mut(element)
                        .insert(ids.prefixes[0], ids.namespaces[n.0]);
                }
            }
            for (n, value) in attributes {
                let attribute_name = name(xot, ids, *n);
                xot.attributes_mut(element)
                    .insert(attribute_name, value.clone());
            }
            for child in children {
                let child = make_node(xot, ids, child);
                xot.append(element, child).unwrap();
            }
            element
        }
        Content::Text(text) => xot.new_text(text),
        Content::Comment(text) => xot.new_comment(text),
        Content::Pi(target, data) => {
            // `x`, `xml` (not allowed) and `x` in a namespace (not allowed)
            let target = match target {
                0 => xot.add_name("x"),
                1 => xot.add_name("xml"),
                _ => xot.add_name_ns("x", ids.namespaces[1]),
            };
            xot.new_processing_instruction(target, data.as_deref())
        }
    }
}

fn parameters(xot: &mut Xot, ids: &Ids, params: &Params) -> Parameters {
    Parameters {
        declaration: params
            .declaration
            .clone()
            .map(|(encoding, standalone)| Declaration {
                encoding,
                standalone,
            }),
        cdata_section_elements: params.cdata.iter().map(|n| name(xot, ids, *n)).collect(),
        unescaped_gt: params.unescaped_gt,
        unescaped_cr: params.unescaped_cr,
        ..Default::default()
    }
}

#[test]
fn test_writer_matches_tree_serializer_on_random_trees() {
    let cases = 10_000;
    // a fixed seed, so that each run checks the same cases
    let mut runner = TestRunner::new_with_rng(
        Config {
            cases,
            failure_persistence: None,
            ..Config::default()
        },
        TestRunner::deterministic().new_rng(),
    );
    let bytes = Cell::new(0u32);
    let errors = Cell::new(0u32);
    runner
        .run(&(arb_root(), arb_params()), |((element, after), params)| {
            let mut xot = Xot::new();
            let ids = ids(&mut xot);
            let element = make_node(&mut xot, &ids, &element);
            let root = xot.new_document_with_element(element).unwrap();
            for content in &after {
                let node = make_node(&mut xot, &ids, content);
                xot.append(root, node).unwrap();
            }
            let parameters = parameters(&mut xot, &ids, &params);
            if same(&xot, &parameters, root)? {
                bytes.set(bytes.get() + 1);
            } else {
                errors.set(errors.get() + 1);
            }
            Ok(())
        })
        .unwrap();
    println!(
        "writer against the tree serializer: {} cases, {} equal bytes, {} equal errors",
        bytes.get() + errors.get(),
        bytes.get(),
        errors.get()
    );
    assert_eq!(bytes.get() + errors.get(), cases);
    // both kinds of result are well represented
    assert!(bytes.get() > cases / 10, "{} byte results", bytes.get());
    assert!(errors.get() > cases / 10, "{} error results", errors.get());
}

#[cfg(feature = "proptest")]
mod arbitrary_documents {
    use super::*;
    use xot::proptest::arb_xml_document;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(2000))]
        #[test]
        fn test_writer_matches_tree_serializer_on_arbitrary_documents(
            document in arb_xml_document(),
            params in arb_params(),
        ) {
            let mut xot = Xot::new();
            let root = document.xotify(&mut xot);
            let ids = ids(&mut xot);
            let parameters = parameters(&mut xot, &ids, &params);
            same(&xot, &parameters, root)?;
        }
    }
}

#[test]
fn test_scope_of_a_document_is_the_xml_prefix() {
    // the writer starts with the namespaces in scope of a document node
    let mut xot = Xot::new();
    let root = xot.parse("<a/>").unwrap();
    let in_scope: Vec<_> = xot.namespaces_in_scope(root).collect();
    assert_eq!(in_scope, vec![(xot.xml_prefix(), xot.xml_namespace())]);
}

#[test]
fn test_xml_attribute_without_declaration() {
    let mut xot = Xot::new();
    let root = xot
        .parse(r#"<a xml:lang="en" xmlns:p="urn:example:p"><p:b p:x="1">t</p:b><c/></a>"#)
        .unwrap();
    let parameters = Parameters::default();
    assert!(same(&xot, &parameters, root).unwrap());
    assert_eq!(
        write_by_walk(&xot, &parameters, root).unwrap(),
        br#"<a xmlns:p="urn:example:p" xml:lang="en"><p:b p:x="1">t</p:b><c/></a>"#
    );
}

#[test]
fn test_declarations_given_by_the_caller() {
    // the declarations need not be the element's namespace nodes, and the
    // caller may change the Xot between calls
    let mut xot = Xot::new();
    let ns = xot.add_namespace("urn:example:a");
    let p = xot.add_prefix("p");
    let a_name = xot.add_name_ns("a", ns);
    let a = xot.new_element(a_name);
    let mut writer = Writer::new(&xot, &Parameters::default(), Vec::new(), NoopNormalizer).unwrap();
    writer.start_element(&xot, a, &[(p, ns)], true).unwrap();
    let b_name = xot.add_name_ns("b", ns);
    let b = xot.new_element(b_name);
    let x = xot.add_name_ns("x", ns);
    xot.attributes_mut(b).insert(x, "1".to_string());
    writer.start_element(&xot, b, &[], false).unwrap();
    writer.end_element(&xot, false).unwrap();
    let text = xot.new_text("a<b");
    writer.text(&xot, "a<b", false, text).unwrap();
    writer.end_element(&xot, true).unwrap();
    assert_eq!(
        writer.into_inner(),
        br#"<p:a xmlns:p="urn:example:a"><p:b p:x="1"/>a&lt;b</p:a>"#
    );
}

#[test]
fn test_cdata_and_declaration() {
    let mut xot = Xot::new();
    let root = xot.parse(r#"<a><b>x]]&gt;y</b><c>z</c></a>"#).unwrap();
    let b = xot.add_name("b");
    let parameters = Parameters {
        declaration: Some(Declaration {
            encoding: Some("UTF-8".to_string()),
            standalone: None,
        }),
        cdata_section_elements: vec![b],
        ..Default::default()
    };
    assert!(same(&xot, &parameters, root).unwrap());
    let written = String::from_utf8(write_by_walk(&xot, &parameters, root).unwrap()).unwrap();
    assert!(
        written.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<a><b><![CDATA[x]]"),
        "{written}"
    );
    assert!(written.ends_with("]]></b><c>z</c></a>"), "{written}");
}

#[test]
fn test_errors() {
    let mut xot = Xot::new();
    let root = xot.parse("<a><b/></a>").unwrap();
    let a = xot.document_element(root).unwrap();
    let b = xot.first_child(a).unwrap();
    // a text that XML 1.0 does not allow
    let mut writer = Writer::new(&xot, &Parameters::default(), Vec::new(), NoopNormalizer).unwrap();
    writer.start_element(&xot, a, &[], true).unwrap();
    let e = writer.text(&xot, "x\u{1}", false, b).unwrap_err();
    assert!(matches!(e, xot::Error::InvalidCharacter('\u{1}', node, _) if node == b));
    // nothing of the failed token was written
    assert_eq!(writer.into_inner(), b"<a>");

    // a name in a namespace without a declaration
    let ns = xot.add_namespace("urn:example:a");
    let n = xot.add_name_ns("n", ns);
    let element = xot.new_element(n);
    let mut writer = Writer::new(&xot, &Parameters::default(), Vec::new(), NoopNormalizer).unwrap();
    let e = writer.start_element(&xot, element, &[], false).unwrap_err();
    assert!(matches!(e, xot::Error::MissingPrefix(ref s) if s == "urn:example:a"));

    // an end without a start, and a start of a node that is not an element
    let mut writer = Writer::new(&xot, &Parameters::default(), Vec::new(), NoopNormalizer).unwrap();
    assert!(matches!(
        writer.end_element(&xot, false),
        Err(xot::Error::InvalidOperation(_))
    ));
    assert!(matches!(
        writer.start_element(&xot, root, &[], false),
        Err(xot::Error::NotElement(node)) if node == root
    ));
}

#[test]
fn test_parameters_that_need_the_tree() {
    let xot = Xot::new();
    let indent = Parameters {
        indentation: Some(Indentation::default()),
        ..Default::default()
    };
    assert!(matches!(
        Writer::new(&xot, &indent, Vec::new(), NoopNormalizer),
        Err(xot::Error::InvalidOperation(_))
    ));
    let doctype = Parameters {
        doctype: Some(DocType::System {
            system: "a.dtd".to_string(),
        }),
        ..Default::default()
    };
    assert!(matches!(
        Writer::new(&xot, &doctype, Vec::new(), NoopNormalizer),
        Err(xot::Error::InvalidOperation(_))
    ));
}

#[test]
fn test_debug_shows_the_open_elements() {
    let mut xot = Xot::new();
    let root = xot.parse("<a><b/></a>").unwrap();
    let a = xot.document_element(root).unwrap();
    let mut writer = Writer::new(&xot, &Parameters::default(), Vec::new(), NoopNormalizer).unwrap();
    assert_eq!(format!("{writer:?}"), "Writer { open_elements: 0, .. }");
    writer.start_element(&xot, a, &[], true).unwrap();
    assert_eq!(format!("{writer:?}"), "Writer { open_elements: 1, .. }");
    writer.end_element(&xot, true).unwrap();
    assert_eq!(format!("{writer:?}"), "Writer { open_elements: 0, .. }");
}
