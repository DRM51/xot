// What a writer receives from the XML serializer: the calls, what it holds
// when serialization fails, and the panics of the token APIs.

use std::io::{self, Write};

use xot::output::{self, Indentation, NoopNormalizer, TokenSerializeParameters};
use xot::{Node, Xot};

type MakeTree = fn(&mut Xot) -> Node;

fn doc_with(xot: &mut Xot, f: impl FnOnce(&mut Xot, Node)) -> Node {
    let doc = xot.parse("<r><a>t</a></r>").unwrap();
    let r = xot.document_element(doc).unwrap();
    let a = xot.first_child(r).unwrap();
    f(xot, a);
    doc
}

fn control_in_text(x: &mut Xot) -> Node {
    doc_with(x, |x, a| {
        let t = x.new_text("-\u{1}-");
        x.append(a, t).unwrap();
    })
}

fn control_in_attribute(x: &mut Xot) -> Node {
    doc_with(x, |x, a| {
        let n = x.add_name("k");
        x.attributes_mut(a).insert(n, "v\u{2}".to_string());
    })
}

fn not_char_in_comment(x: &mut Xot) -> Node {
    doc_with(x, |x, a| {
        let c = x.new_comment("c\u{FFFE}");
        x.append(a, c).unwrap();
    })
}

fn control_in_pi_data(x: &mut Xot) -> Node {
    doc_with(x, |x, a| {
        let t = x.add_name("pi");
        let p = x.new_processing_instruction(t, Some("d\u{3}"));
        x.append(a, p).unwrap();
    })
}

fn control_in_namespace(x: &mut Xot) -> Node {
    doc_with(x, |x, a| {
        let p = x.add_prefix("p");
        let ns = x.add_namespace("urn:\u{4}");
        x.namespaces_mut(a).insert(p, ns);
    })
}

fn xmlns_attribute(x: &mut Xot) -> Node {
    doc_with(x, |x, a| {
        let n = x.add_name("xmlns");
        x.attributes_mut(a).insert(n, "urn:x".to_string());
    })
}

fn prefix_undeclared(x: &mut Xot) -> Node {
    doc_with(x, |x, a| {
        let p = x.add_prefix("p");
        let ns = x.add_namespace("");
        x.namespaces_mut(a).insert(p, ns);
    })
}

fn pi_target_xml(x: &mut Xot) -> Node {
    doc_with(x, |x, a| {
        let t = x.add_name("XmL");
        let p = x.new_processing_instruction(t, None);
        x.append(a, p).unwrap();
    })
}

fn pi_target_in_namespace(x: &mut Xot) -> Node {
    doc_with(x, |x, a| {
        let ns = x.add_namespace("urn:pi");
        let t = x.add_name_ns("pi", ns);
        let p = x.new_processing_instruction(t, None);
        x.append(a, p).unwrap();
    })
}

fn element_without_prefix(x: &mut Xot) -> Node {
    doc_with(x, |x, a| {
        let ns = x.add_namespace("urn:e");
        let n = x.add_name_ns("e", ns);
        let e = x.new_element(n);
        x.append(a, e).unwrap();
    })
}

fn attribute_without_prefix(x: &mut Xot) -> Node {
    doc_with(x, |x, a| {
        let ns = x.add_namespace("urn:at");
        let n = x.add_name_ns("k", ns);
        x.attributes_mut(a).insert(n, "v".to_string());
    })
}

fn attribute_in_default_namespace(x: &mut Xot) -> Node {
    doc_with(x, |x, a| {
        let ns = x.add_namespace("urn:d");
        let e = x.empty_prefix();
        x.namespaces_mut(a).insert(e, ns);
        let n = x.add_name_ns("k", ns);
        x.attributes_mut(a).insert(n, "v".to_string());
    })
}

fn pi_data_with_gt(x: &mut Xot) -> Node {
    doc_with(x, |x, a| {
        let t = x.add_name("pi");
        let p = x.new_processing_instruction(t, Some("a > b"));
        x.append(a, p).unwrap();
    })
}

fn line_ends(x: &mut Xot) -> Node {
    doc_with(x, |x, a| {
        let t = x.new_text("1\r.2\u{85}3\u{2028}4\u{9F}5\t.6\n.7");
        x.append(a, t).unwrap();
        let n = x.add_name("k");
        x.attributes_mut(a)
            .insert(n, "1\r.2\u{85}3\u{2028}4\t.5\n.6<&\"'>".to_string());
    })
}

fn text_child_of_document(x: &mut Xot) -> Node {
    let doc = x.new_document();
    let t = x.new_text("a<b]]>c\r");
    x.append(doc, t).unwrap();
    doc
}

fn show(b: &[u8]) -> String {
    String::from_utf8_lossy(b).escape_debug().to_string()
}

/// What a Vec<u8> holds after serializing, and the error, if any.
fn run(make: MakeTree, method: &str) -> String {
    let mut xot = Xot::new();
    let doc = make(&mut xot);
    let mut buf = Vec::new();
    let parameters = match method {
        "xml" => output::xml::Parameters::default(),
        "xml-indent" => output::xml::Parameters {
            indentation: Some(Default::default()),
            ..Default::default()
        },
        "xml-unescaped-cr" => output::xml::Parameters {
            unescaped_cr: true,
            ..Default::default()
        },
        _ => unreachable!(),
    };
    match xot.serialize_xml_write(parameters, doc, &mut buf) {
        Ok(()) => format!("ok \"{}\"", show(&buf)),
        Err(e) => format!("error \"{}\" then {e:?}: {e}", show(&buf)),
    }
}

// The texts are those of xot 0.31.2 at f250789: the writer holds every token
// before the one that fails, with the indentation of the failing token, and
// nothing of the failing token.
#[test]
fn test_what_the_writer_holds_after_an_error() {
    let cases: &[(MakeTree, [&str; 3])] = &[
        (
            control_in_text,
            [
                r#"error "<r><a>" then InvalidCharacter('\u{1}', Node(NodeId { index1: 4, generation: 0 }), "text"): Character U+0001 is not allowed in XML 1.0, found in text"#,
                r#"error "<r>\n  <a>" then InvalidCharacter('\u{1}', Node(NodeId { index1: 4, generation: 0 }), "text"): Character U+0001 is not allowed in XML 1.0, found in text"#,
                r#"error "<r><a>" then InvalidCharacter('\u{1}', Node(NodeId { index1: 4, generation: 0 }), "text"): Character U+0001 is not allowed in XML 1.0, found in text"#,
            ],
        ),
        (
            control_in_attribute,
            [
                r#"error "<r><a" then InvalidCharacter('\u{2}', Node(NodeId { index1: 3, generation: 0 }), "attribute \"k\""): Character U+0002 is not allowed in XML 1.0, found in attribute "k""#,
                r#"error "<r>\n  <a" then InvalidCharacter('\u{2}', Node(NodeId { index1: 3, generation: 0 }), "attribute \"k\""): Character U+0002 is not allowed in XML 1.0, found in attribute "k""#,
                r#"error "<r><a" then InvalidCharacter('\u{2}', Node(NodeId { index1: 3, generation: 0 }), "attribute \"k\""): Character U+0002 is not allowed in XML 1.0, found in attribute "k""#,
            ],
        ),
        (
            not_char_in_comment,
            [
                r#"error "<r><a>t" then InvalidCharacter('\u{fffe}', Node(NodeId { index1: 5, generation: 0 }), "comment"): Character U+FFFE is not allowed in XML 1.0, found in comment"#,
                r#"error "<r>\n  <a>t" then InvalidCharacter('\u{fffe}', Node(NodeId { index1: 5, generation: 0 }), "comment"): Character U+FFFE is not allowed in XML 1.0, found in comment"#,
                r#"error "<r><a>t" then InvalidCharacter('\u{fffe}', Node(NodeId { index1: 5, generation: 0 }), "comment"): Character U+FFFE is not allowed in XML 1.0, found in comment"#,
            ],
        ),
        (
            control_in_pi_data,
            [
                r#"error "<r><a>t" then InvalidCharacter('\u{3}', Node(NodeId { index1: 5, generation: 0 }), "processing instruction \"pi\""): Character U+0003 is not allowed in XML 1.0, found in processing instruction "pi""#,
                r#"error "<r>\n  <a>t" then InvalidCharacter('\u{3}', Node(NodeId { index1: 5, generation: 0 }), "processing instruction \"pi\""): Character U+0003 is not allowed in XML 1.0, found in processing instruction "pi""#,
                r#"error "<r><a>t" then InvalidCharacter('\u{3}', Node(NodeId { index1: 5, generation: 0 }), "processing instruction \"pi\""): Character U+0003 is not allowed in XML 1.0, found in processing instruction "pi""#,
            ],
        ),
        (
            control_in_namespace,
            [
                r#"error "<r><a" then InvalidCharacter('\u{4}', Node(NodeId { index1: 3, generation: 0 }), "namespace declaration \"xmlns:p\""): Character U+0004 is not allowed in XML 1.0, found in namespace declaration "xmlns:p""#,
                r#"error "<r>\n  <a" then InvalidCharacter('\u{4}', Node(NodeId { index1: 3, generation: 0 }), "namespace declaration \"xmlns:p\""): Character U+0004 is not allowed in XML 1.0, found in namespace declaration "xmlns:p""#,
                r#"error "<r><a" then InvalidCharacter('\u{4}', Node(NodeId { index1: 3, generation: 0 }), "namespace declaration \"xmlns:p\""): Character U+0004 is not allowed in XML 1.0, found in namespace declaration "xmlns:p""#,
            ],
        ),
        (
            xmlns_attribute,
            [
                r#"error "<r><a" then XmlnsAttribute(Node(NodeId { index1: 3, generation: 0 }), "xmlns"): Attribute xmlns would read back as a namespace declaration"#,
                r#"error "<r>\n  <a" then XmlnsAttribute(Node(NodeId { index1: 3, generation: 0 }), "xmlns"): Attribute xmlns would read back as a namespace declaration"#,
                r#"error "<r><a" then XmlnsAttribute(Node(NodeId { index1: 3, generation: 0 }), "xmlns"): Attribute xmlns would read back as a namespace declaration"#,
            ],
        ),
        (
            prefix_undeclared,
            [
                r#"error "<r><a" then InvalidNamespaceDeclaration(Node(NodeId { index1: 3, generation: 0 }), "xmlns:p", ""): Invalid namespace declaration: xmlns:p="""#,
                r#"error "<r>\n  <a" then InvalidNamespaceDeclaration(Node(NodeId { index1: 3, generation: 0 }), "xmlns:p", ""): Invalid namespace declaration: xmlns:p="""#,
                r#"error "<r><a" then InvalidNamespaceDeclaration(Node(NodeId { index1: 3, generation: 0 }), "xmlns:p", ""): Invalid namespace declaration: xmlns:p="""#,
            ],
        ),
        (
            pi_target_xml,
            [
                r#"error "<r><a>t" then InvalidTarget("XmL"): Invalid target: XmL"#,
                r#"error "<r>\n  <a>t" then InvalidTarget("XmL"): Invalid target: XmL"#,
                r#"error "<r><a>t" then InvalidTarget("XmL"): Invalid target: XmL"#,
            ],
        ),
        (
            pi_target_in_namespace,
            [
                r#"error "<r><a>t" then NamespaceInProcessingInstruction: Namespace in processing instruction target"#,
                r#"error "<r>\n  <a>t" then NamespaceInProcessingInstruction: Namespace in processing instruction target"#,
                r#"error "<r><a>t" then NamespaceInProcessingInstruction: Namespace in processing instruction target"#,
            ],
        ),
        (
            element_without_prefix,
            [
                r#"error "<r><a>t" then MissingPrefix("urn:e"): Missing prefix"#,
                r#"error "<r>\n  <a>t" then MissingPrefix("urn:e"): Missing prefix"#,
                r#"error "<r><a>t" then MissingPrefix("urn:e"): Missing prefix"#,
            ],
        ),
        (
            attribute_without_prefix,
            [
                r#"error "<r><a" then MissingPrefix("urn:at"): Missing prefix"#,
                r#"error "<r>\n  <a" then MissingPrefix("urn:at"): Missing prefix"#,
                r#"error "<r><a" then MissingPrefix("urn:at"): Missing prefix"#,
            ],
        ),
        (
            attribute_in_default_namespace,
            [
                r#"error "<r><a xmlns=\"urn:d\"" then MissingPrefix("urn:d"): Missing prefix"#,
                r#"error "<r>\n  <a xmlns=\"urn:d\"" then MissingPrefix("urn:d"): Missing prefix"#,
                r#"error "<r><a xmlns=\"urn:d\"" then MissingPrefix("urn:d"): Missing prefix"#,
            ],
        ),
        (
            pi_data_with_gt,
            [
                r#"ok "<r><a>t<?pi a > b?></a></r>""#,
                r#"ok "<r>\n  <a>t<?pi a > b?></a>\n</r>\n""#,
                r#"ok "<r><a>t<?pi a > b?></a></r>""#,
            ],
        ),
        (
            line_ends,
            [
                r#"ok "<r><a k=\"1&#xD;.2&#x85;3&#x2028;4&#x9;.5&#xA;.6&lt;&amp;&quot;&apos;>\">t1&#xD;.2&#x85;3&#x2028;4&#x9F;5\t.6\n.7</a></r>""#,
                r#"ok "<r>\n  <a k=\"1&#xD;.2&#x85;3&#x2028;4&#x9;.5&#xA;.6&lt;&amp;&quot;&apos;>\">t1&#xD;.2&#x85;3&#x2028;4&#x9F;5\t.6\n.7</a>\n</r>\n""#,
                r#"ok "<r><a k=\"1&#xD;.2&#x85;3&#x2028;4&#x9;.5&#xA;.6&lt;&amp;&quot;&apos;>\">t1\r.2&#x85;3&#x2028;4&#x9F;5\t.6\n.7</a></r>""#,
            ],
        ),
        (
            text_child_of_document,
            [
                r#"ok "a&lt;b]]&gt;c&#xD;""#,
                r#"ok "a&lt;b]]&gt;c&#xD;""#,
                r#"ok "a&lt;b]]&gt;c\r""#,
            ],
        ),
    ];
    for (i, (make, expected)) in cases.iter().enumerate() {
        for (method, expected) in ["xml", "xml-indent", "xml-unescaped-cr"]
            .iter()
            .zip(expected.iter())
        {
            assert_eq!(&run(*make, method), expected, "case {} {}", i, method);
        }
    }
}

fn tokens_text(make: MakeTree) -> String {
    let mut xot = Xot::new();
    let doc = make(&mut xot);
    let mut s = String::new();
    for (_, _, token) in xot.tokens(doc, TokenSerializeParameters::default(), NoopNormalizer) {
        if token.space {
            s.push(' ');
        }
        s.push_str(&token.text);
    }
    s
}

#[test]
fn test_tokens_run_no_checks() {
    assert_eq!(tokens_text(control_in_text), "<r><a>t-\u{1}-</a></r>");
    assert_eq!(
        tokens_text(control_in_namespace),
        "<r><a xmlns:p=\"urn:\u{4}\">t</a></r>"
    );
    assert_eq!(tokens_text(pi_target_xml), "<r><a>t<?XmL?></a></r>");
}

#[test]
#[should_panic(expected = "called `Result::unwrap()` on an `Err` value: MissingPrefix(\"urn:e\")")]
fn test_tokens_panic_on_missing_prefix() {
    tokens_text(element_without_prefix);
}

#[test]
#[should_panic(expected = "called `Result::unwrap()` on an `Err` value: MissingPrefix(\"urn:d\")")]
fn test_tokens_panic_on_missing_attribute_prefix() {
    tokens_text(attribute_in_default_namespace);
}

#[test]
#[should_panic(
    expected = "called `Result::unwrap()` on an `Err` value: NamespaceInProcessingInstruction"
)]
fn test_tokens_panic_on_namespace_in_processing_instruction() {
    tokens_text(pi_target_in_namespace);
}

#[test]
#[should_panic(
    expected = "called `Result::unwrap()` on an `Err` value: NamespaceInProcessingInstruction"
)]
fn test_pretty_tokens_panic_on_namespace_in_processing_instruction() {
    let mut xot = Xot::new();
    let doc = pi_target_in_namespace(&mut xot);
    for _ in xot.pretty_tokens(doc, Default::default(), &[], NoopNormalizer) {}
}

/// Keeps each call, with `write` and `write_all` told apart.
#[derive(Default)]
struct Recorder {
    calls: Vec<(&'static str, String)>,
}

impl Write for Recorder {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.calls
            .push(("write", String::from_utf8(buf.to_vec()).unwrap()));
        Ok(buf.len())
    }
    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        self.calls
            .push(("write_all", String::from_utf8(buf.to_vec()).unwrap()));
        Ok(())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.calls.push(("flush", String::new()));
        Ok(())
    }
}

#[test]
fn test_calls_on_the_writer() {
    let mut xot = Xot::new();
    let doc = xot
        .parse(r#"<r xmlns:p="urn:p" a="1"><p:b>t</p:b><c/></r>"#)
        .unwrap();
    let mut w = Recorder::default();
    xot.serialize_xml_write(
        output::xml::Parameters {
            indentation: Some(Indentation::default()),
            declaration: Some(Default::default()),
            ..Default::default()
        },
        doc,
        &mut w,
    )
    .unwrap();
    let calls: Vec<(&str, &str)> = w.calls.iter().map(|(k, s)| (*k, s.as_str())).collect();
    assert_eq!(
        calls,
        [
            ("write_all", "<?xml version=\"1.0\""),
            ("write_all", "?>\n"),
            ("write_all", "<r"),
            ("write_all", " "),
            ("write_all", "xmlns:p=\"urn:p\""),
            ("write_all", " "),
            ("write_all", "a=\"1\""),
            ("write_all", ">"),
            ("write_all", "\n"),
            ("write_all", "  "),
            ("write_all", "<p:b"),
            ("write_all", ">"),
            ("write_all", "t"),
            ("write_all", "</p:b>"),
            ("write_all", "\n"),
            ("write_all", "  "),
            ("write_all", "<c"),
            ("write_all", "/>"),
            ("write_all", ""),
            ("write_all", "\n"),
            ("write_all", "</r>"),
            ("write_all", "\n"),
        ]
    );
}

#[test]
fn test_calls_on_the_writer_without_indentation() {
    let mut xot = Xot::new();
    let doc = xot
        .parse(r#"<r xmlns:p="urn:p" a="1"><p:b>t</p:b><c/></r>"#)
        .unwrap();
    let p = xot.document_element(doc).unwrap();
    let b = xot.first_child(p).unwrap();
    let mut w = Recorder::default();
    xot.serialize_xml_write(Default::default(), b, &mut w)
        .unwrap();
    let calls: Vec<(&str, &str)> = w.calls.iter().map(|(k, s)| (*k, s.as_str())).collect();
    assert_eq!(
        calls,
        [
            ("write_all", "<p:b"),
            ("write_all", " "),
            ("write_all", "xmlns:p=\"urn:p\""),
            // the xml prefix, in scope for every element, writes nothing
            ("write_all", ""),
            ("write_all", ">"),
            ("write_all", "t"),
            ("write_all", "</p:b>"),
        ]
    );
}

#[test]
fn test_text_of_two_megabytes() {
    let mut xot = Xot::new();
    let piece = "abc&<>\u{85}\r";
    let text = piece.repeat(2 * 1024 * 1024 / piece.len());
    let r = xot.add_name("r");
    let e = xot.new_element(r);
    let t = xot.new_text(&text);
    xot.append(e, t).unwrap();
    let c = xot.new_element(r);
    xot.append(e, c).unwrap();
    let escaped = "abc&amp;&lt;&gt;&#x85;&#xD;".repeat(2 * 1024 * 1024 / piece.len());
    let expected = format!("<r>{}<r/></r>", escaped);
    let mut w = Recorder::default();
    xot.serialize_xml_write(Default::default(), e, &mut w)
        .unwrap();
    let calls: Vec<(&str, &str)> = w.calls.iter().map(|(k, s)| (*k, s.as_str())).collect();
    assert_eq!(
        calls,
        [
            ("write_all", "<r"),
            ("write_all", ""),
            ("write_all", ">"),
            ("write_all", escaped.as_str()),
            ("write_all", "<r"),
            ("write_all", "/>"),
            ("write_all", ""),
            ("write_all", "</r>"),
        ]
    );
    assert_eq!(xot.to_string(e).unwrap(), expected);
}

#[test]
fn test_indentation_deeper_than_64_levels() {
    let mut xot = Xot::new();
    let name = xot.add_name("e");
    let top = xot.new_element(name);
    let mut current = top;
    for _ in 1..70 {
        let e = xot.new_element(name);
        xot.append(current, e).unwrap();
        current = e;
    }
    let xml = xot
        .serialize_xml_string(
            output::xml::Parameters {
                indentation: Some(Indentation::default()),
                ..Default::default()
            },
            top,
        )
        .unwrap();
    let mut expected = String::new();
    for level in 0..69 {
        expected.push_str(&" ".repeat(level * 2));
        expected.push_str("<e>\n");
    }
    expected.push_str(&" ".repeat(69 * 2));
    expected.push_str("<e/>\n");
    for level in (0..69).rev() {
        expected.push_str(&" ".repeat(level * 2));
        expected.push_str("</e>\n");
    }
    assert_eq!(xml, expected);

    // each indentation is one call
    let mut w = Recorder::default();
    xot.serialize_xml_write(
        output::xml::Parameters {
            indentation: Some(Indentation::default()),
            ..Default::default()
        },
        top,
        &mut w,
    )
    .unwrap();
    let deepest = " ".repeat(69 * 2);
    let position = w
        .calls
        .iter()
        .position(|(_, s)| *s == deepest)
        .expect("one call with the deepest indentation");
    assert_eq!(w.calls[position + 1].1, "<e");
    assert_eq!(w.calls[position + 2].1, "/>");
}
