//! Tests of the spans of the errors that the checks after a parse report:
//! `parse` and `parse_fragment` keep only these spans (`ErrorSpans`), and
//! have to report the same spans as `parse_with_span_info` and
//! `parse_fragment_with_span_info`, which keep the spans of all nodes
//! (`SpanInfo`).
//!
//! The tokenizer of a document stops with an error at text or a second
//! element outside the document element, so `TextAtTopLevel` and
//! `MultipleElementsAtTopLevel` come up here only through the checks of a
//! document after the tokens of a fragment
//! (`Mode::FragmentTokensAsDocument`).
//!
//! The same texts check the seam between the builder and its sink
//! (`TreeSink`): a sink that keeps a log of what it gets, in the order it gets it,
//! against the tree that the sink of `Xot`'s trees makes.
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use xmlparser::Tokenizer;

use super::{ErrorSpans, MutableSink, SpanRecorder, TreeSink};
use crate::arena::NodeId;
use crate::id::{NameId, PrefixId};
use crate::{NamespaceId, Node, NodeView, ParseError, Span, SpanInfo, Xot};

/// How a text is parsed.
#[derive(Clone, Copy, Debug)]
enum Mode {
    /// As `parse` and `parse_with_span_info` do.
    Document,
    /// As `parse_fragment` and `parse_fragment_with_span_info` do.
    Fragment,
    /// The tokens of a fragment, then the checks of a document.
    FragmentTokensAsDocument,
}

const MODES: [Mode; 3] = [
    Mode::Document,
    Mode::Fragment,
    Mode::FragmentTokensAsDocument,
];

/// Parse `xml` in a new `Xot`, with the spans in `recorder`.
fn parse_with<R: SpanRecorder>(
    xml: &str,
    mode: Mode,
    recorder: &mut R,
) -> Result<Node, ParseError> {
    let mut xot = Xot::new();
    match mode {
        Mode::Document => xot.parse_document(xml, recorder),
        Mode::Fragment => xot.parse_fragment_document(xml, recorder),
        Mode::FragmentTokensAsDocument => {
            let tokenizer = Tokenizer::from_fragment(xml, 0..xml.len());
            xot.parse_tokens_as_document(tokenizer, xml, recorder)
        }
    }
}

/// Parse `xml` with all spans and with the spans of the errors only, and
/// check that both give the same result, which is returned.
fn parse_both(xml: &str, mode: Mode) -> Result<Node, ParseError> {
    let with_span_info = parse_with(xml, mode, &mut SpanInfo::new());
    let with_error_spans = parse_with(xml, mode, &mut ErrorSpans::default());
    assert_eq!(
        format!("{:?}", with_error_spans),
        format!("{:?}", with_span_info),
        "{:?} {:?}",
        mode,
        xml
    );
    with_error_spans
}

/// The error of the checks of a document after the tokens of `xml` as a
/// fragment.
fn checked_as_document(xml: &str) -> ParseError {
    parse_both(xml, Mode::FragmentTokensAsDocument).unwrap_err()
}

#[test]
fn text_at_top_level_spans_the_first_text_node() {
    for (xml, span) in [
        ("t<x/>", 0..1),
        // text with references, CDATA sections and line ends merge into one
        // text node, which spans them all
        ("a&amp;b<![CDATA[c]]>d<x/>", 0..21),
        ("<x/><![CDATA[c]]>&lt;", 13..21),
        ("<![CDATA[c]]><![CDATA[d]]><x/>", 9..23),
        ("<![CDATA[]]>t<x/>", 9..13),
        ("a\r\nb<x/>", 0..4),
        ("<x/>\r\n<![CDATA[\r\n]]>", 4..17),
        // the first text node, not a later one
        ("<x/>t<!--c-->u", 4..5),
        ("<!--c-->t<x/>", 8..9),
        ("<?p q?>t<x/>u", 7..8),
        ("<x/>t<y/>u", 4..5),
        // text comes first, also after a second element
        ("<x/><y/>t", 8..9),
        ("t<x/><y/>", 0..1),
        // without an element
        ("t", 0..1),
        (" ", 0..1),
    ] {
        let err = checked_as_document(xml);
        assert!(
            matches!(err, ParseError::TextAtTopLevel(s) if s == Span::from(span.clone())),
            "{:?}: {:?}",
            xml,
            err
        );
    }
}

#[test]
fn multiple_elements_at_top_level_spans_the_name_of_the_second_element() {
    for (xml, span) in [
        ("<x/><y/>", 5..6),
        ("<x/><y/><z/>", 5..6),
        // the second element under the document node, not the second element
        ("<x><y/></x><z/>", 12..13),
        ("<x></x><!--c--><p:y xmlns:p=\"urn:p\"><z/></p:y>", 16..19),
        ("<?p q?><x a=\"1\"/><yy b=\"2\">\r\n</yy>", 18..20),
    ] {
        let err = checked_as_document(xml);
        assert!(
            matches!(err, ParseError::MultipleElementsAtTopLevel(s) if s == Span::from(span.clone())),
            "{:?}: {:?}",
            xml,
            err
        );
    }
}

#[test]
fn unclosed_tag_spans_the_name_of_the_innermost_open_element() {
    for (xml, span) in [
        ("<x>", 1..2),
        ("t<x>", 2..3),
        ("<x/><y><z>", 8..9),
        ("<x><y></y>", 1..2),
        ("<x></x>t<p:y xmlns:p=\"urn:p\">a<![CDATA[b]]>", 9..12),
    ] {
        for mode in [Mode::Fragment, Mode::FragmentTokensAsDocument] {
            let err = parse_both(xml, mode).unwrap_err();
            assert!(
                matches!(err, ParseError::UnclosedTag(s) if s == Span::from(span.clone())),
                "{:?} {:?}: {:?}",
                mode,
                xml,
                err
            );
        }
    }
}

/// Pieces of text, from which the texts of `error_spans_are_those_of_span_info`
/// are made: one after the other as they come, or inside the start and end
/// tags of `ELEMENTS`.
const PIECES: [&str; 20] = [
    "t",
    " ",
    "\r\n",
    "&amp;",
    "&#x41;",
    "&",
    "<![CDATA[c]]>",
    "<![CDATA[]]>",
    "<!--c-->",
    "<?p q?>",
    "<a>",
    "</a>",
    "<b x=\"1\" y=\"&lt;\">",
    "</b>",
    "<c/>",
    "<p:d xmlns:p=\"urn:p\">",
    "</p:d>",
    "<p:e/>",
    "<f xml:id=\"i\"/>",
    "<?xml version=\"1.0\"?>",
];

/// Start and end tags around content.
const ELEMENTS: [(&str, &str); 3] = [
    ("<a>", "</a>"),
    ("<b x=\"1\">", "</b>"),
    ("<p:d xmlns:p=\"urn:p\">", "</p:d>"),
];

/// A seeded xorshift64* generator.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) % n as u64) as usize
    }
}

/// Add up to three random pieces or elements to `text`, with content in the
/// elements as far as `depth` allows.
fn random_content(rng: &mut Rng, depth: usize, text: &mut String) {
    for _ in 0..rng.below(4) {
        if depth > 0 && rng.below(2) == 0 {
            let (start, end) = ELEMENTS[rng.below(ELEMENTS.len())];
            text.push_str(start);
            random_content(rng, depth - 1, text);
            text.push_str(end);
        } else {
            text.push_str(PIECES[rng.below(PIECES.len())]);
        }
    }
}

/// The kind of a result, for the tally.
fn kind(result: &Result<Node, ParseError>) -> String {
    match result {
        Ok(_) => "Ok".to_string(),
        Err(err) => {
            let text = format!("{:?}", err);
            text.split('(').next().unwrap_or_default().to_string()
        }
    }
}

#[test]
fn error_spans_are_those_of_span_info() {
    // every text of up to three pieces, texts of 4 to 12 random pieces, and
    // random texts with elements around content
    let mut texts: Vec<String> = vec![String::new()];
    let mut shorter = vec![String::new()];
    for _ in 0..3 {
        let longer: Vec<String> = shorter
            .iter()
            .flat_map(|text| PIECES.iter().map(move |piece| format!("{}{}", text, piece)))
            .collect();
        texts.extend(longer.iter().cloned());
        shorter = longer;
    }
    let mut rng = Rng::new(1);
    for _ in 0..2000 {
        let len = 4 + rng.below(9);
        texts.push((0..len).map(|_| PIECES[rng.below(PIECES.len())]).collect());
    }
    for _ in 0..2000 {
        let mut text = String::new();
        random_content(&mut rng, 3, &mut text);
        texts.push(text);
    }
    let mut tally: BTreeMap<(String, String), usize> = BTreeMap::new();
    for text in &texts {
        for mode in MODES {
            let result = parse_both(text, mode);
            *tally
                .entry((format!("{:?}", mode), kind(&result)))
                .or_default() += 1;
        }
    }
    // the checks after the parse give each of their errors
    for (mode, kind) in [
        ("Document", "Ok"),
        ("Document", "UnclosedTag"),
        ("Document", "NoElementAtTopLevel"),
        ("Fragment", "Ok"),
        ("Fragment", "UnclosedTag"),
        ("FragmentTokensAsDocument", "Ok"),
        ("FragmentTokensAsDocument", "UnclosedTag"),
        ("FragmentTokensAsDocument", "NoElementAtTopLevel"),
        ("FragmentTokensAsDocument", "TextAtTopLevel"),
        ("FragmentTokensAsDocument", "MultipleElementsAtTopLevel"),
    ] {
        assert!(
            tally.get(&(mode.to_string(), kind.to_string())).copied() >= Some(100),
            "{} {}: {:?}",
            mode,
            kind,
            tally
        );
    }
}

// The seam between the builder and the sink: a sink that keeps a log of what
// it gets, and hands it on to the sink of `Xot`'s trees.

/// What a sink got.
#[derive(Debug, Clone, PartialEq)]
enum Event {
    Document(NodeId),
    Element(NodeId, NodeId, NameId),
    Namespace(NodeId, PrefixId, NamespaceId),
    Attribute(NodeId, NameId, String),
    Text(NodeId, NodeId, String),
    AppendText(NodeId, String),
    Comment(NodeId, NodeId, String),
    ProcessingInstruction(NodeId, NodeId, NameId, Option<String>),
    Close(NodeId),
    Abandon,
}

/// A sink that keeps each call in `events`, and passes it on to
/// `MutableSink`, so that the parse makes its tree as before.
struct RecordingSink {
    events: Rc<RefCell<Vec<Event>>>,
}

impl RecordingSink {
    fn record(&self, event: Event) {
        self.events.borrow_mut().push(event);
    }
}

impl TreeSink for RecordingSink {
    fn document(&mut self, xot: &mut Xot) -> NodeId {
        let node = MutableSink.document(xot);
        self.record(Event::Document(node));
        node
    }

    fn element(&mut self, xot: &mut Xot, parent: NodeId, name_id: NameId) -> NodeId {
        let node = MutableSink.element(xot, parent, name_id);
        self.record(Event::Element(parent, node, name_id));
        node
    }

    fn namespace(
        &mut self,
        xot: &mut Xot,
        element: NodeId,
        prefix_id: PrefixId,
        namespace_id: NamespaceId,
    ) {
        MutableSink.namespace(xot, element, prefix_id, namespace_id);
        self.record(Event::Namespace(element, prefix_id, namespace_id));
    }

    fn attribute(&mut self, xot: &mut Xot, element: NodeId, name_id: NameId, value: Cow<'_, str>) {
        self.record(Event::Attribute(element, name_id, value.to_string()));
        MutableSink.attribute(xot, element, name_id, value);
    }

    fn text(&mut self, xot: &mut Xot, parent: NodeId, content: Cow<'_, str>) -> NodeId {
        let text = content.to_string();
        let node = MutableSink.text(xot, parent, content);
        self.record(Event::Text(parent, node, text));
        node
    }

    fn append_text(&mut self, xot: &mut Xot, text: NodeId, content: &str) {
        MutableSink.append_text(xot, text, content);
        self.record(Event::AppendText(text, content.to_string()));
    }

    fn comment(&mut self, xot: &mut Xot, parent: NodeId, content: Cow<'_, str>) -> NodeId {
        let text = content.to_string();
        let node = MutableSink.comment(xot, parent, content);
        self.record(Event::Comment(parent, node, text));
        node
    }

    fn processing_instruction(
        &mut self,
        xot: &mut Xot,
        parent: NodeId,
        target: NameId,
        content: Option<Cow<'_, str>>,
    ) -> NodeId {
        let text = content.as_ref().map(|c| c.to_string());
        let node = MutableSink.processing_instruction(xot, parent, target, content);
        self.record(Event::ProcessingInstruction(parent, node, target, text));
        node
    }

    fn close_element(&mut self, xot: &mut Xot, element: NodeId) {
        MutableSink.close_element(xot, element);
        self.record(Event::Close(element));
    }

    fn element_name(&self, xot: &Xot, node: NodeId) -> Option<NameId> {
        MutableSink.element_name(xot, node)
    }

    fn abandon(&mut self, xot: &mut Xot) {
        MutableSink.abandon(xot);
        self.record(Event::Abandon);
    }
}

/// A tree as text: each node as `NodeView` shows it, then its children, all
/// of them, namespace and attribute nodes first, as the arena has them.
fn dump(xot: &Xot, node: NodeId, depth: usize, out: &mut String) {
    out.push_str(&format!(
        "{}{:?}\n",
        "  ".repeat(depth),
        xot.view(Node::new(node))
    ));
    for child in xot.arena.children(node) {
        dump(xot, child, depth + 1, out);
    }
}

/// The tree that `events` describe, as `dump` shows a tree; and a check
/// that they come in the order a sink is promised.
fn replay(events: &[Event]) -> String {
    let mut views: HashMap<NodeId, (Event, Vec<NodeId>, String)> = HashMap::new();
    let mut open: Vec<NodeId> = Vec::new();
    // the node that came last, and whether attributes may still come for it
    let mut last: Option<NodeId> = None;
    let mut in_start_tag: Option<(NodeId, bool)> = None;
    let add = |views: &mut HashMap<NodeId, (Event, Vec<NodeId>, String)>,
               parent: Option<NodeId>,
               node: NodeId,
               event: &Event| {
        assert!(!views.contains_key(&node), "{:?} came twice", node);
        if let Some(parent) = parent {
            views
                .get_mut(&parent)
                .expect("the parent came first")
                .1
                .push(node);
        }
        views.insert(node, (event.clone(), Vec::new(), String::new()));
    };
    let Some((Event::Document(document), later)) = events.split_first() else {
        panic!("the document node comes first: {:?}", events);
    };
    add(&mut views, None, *document, &events[0]);
    open.push(*document);
    for event in later {
        let current = *open
            .last()
            .expect("an event after the document node was closed");
        match event {
            Event::Element(parent, node, _) => {
                assert_eq!(*parent, current, "an element in the current node");
                add(&mut views, Some(*parent), *node, event);
                open.push(*node);
                in_start_tag = Some((*node, true));
                last = Some(*node);
            }
            Event::Namespace(element, _, _) => {
                assert_eq!(
                    in_start_tag,
                    Some((*element, true)),
                    "a namespace node right after its element"
                );
                let node = NodeId::first_in(
                    std::num::NonZeroU32::new(u32::MAX - views.len() as u32).unwrap(),
                );
                add(&mut views, Some(*element), node, event);
                last = None;
            }
            Event::Attribute(element, _, _) => {
                assert!(
                    matches!(in_start_tag, Some((e, _)) if e == *element),
                    "an attribute node in its start tag"
                );
                in_start_tag = Some((*element, false));
                let node = NodeId::first_in(
                    std::num::NonZeroU32::new(u32::MAX - views.len() as u32).unwrap(),
                );
                add(&mut views, Some(*element), node, event);
                last = None;
            }
            Event::Text(parent, node, _) => {
                assert_eq!(*parent, current, "text in the current node");
                in_start_tag = None;
                add(&mut views, Some(*parent), *node, event);
                last = Some(*node);
            }
            Event::AppendText(node, content) => {
                assert_eq!(last, Some(*node), "more text for the node that came last");
                let entry = views.get_mut(node).unwrap();
                assert!(
                    matches!(entry.0, Event::Text(..)),
                    "more text for a text node"
                );
                entry.2.push_str(content);
            }
            Event::Comment(parent, node, _) | Event::ProcessingInstruction(parent, node, _, _) => {
                assert_eq!(*parent, current, "a node in the current node");
                in_start_tag = None;
                add(&mut views, Some(*parent), *node, event);
                last = Some(*node);
            }
            Event::Close(element) => {
                assert_eq!(*element, current, "the innermost open element closes");
                assert_ne!(*element, *document, "the document node is not closed");
                open.pop();
                in_start_tag = None;
                last = Some(*element);
            }
            Event::Document(_) => panic!("a second document node"),
            Event::Abandon => panic!("abandon in a parse that did not fail"),
        }
    }
    assert_eq!(open, [*document], "every element is closed");
    fn show(
        views: &HashMap<NodeId, (Event, Vec<NodeId>, String)>,
        node: NodeId,
        depth: usize,
        out: &mut String,
    ) {
        let (event, children, more) = &views[&node];
        let text = match event {
            Event::Text(_, _, text) => format!("{}{}", text, more),
            _ => String::new(),
        };
        let view = match event {
            Event::Document(_) => NodeView::Document,
            Event::Element(_, _, name) => NodeView::Element(*name),
            Event::Namespace(_, prefix, namespace) => NodeView::Namespace(*prefix, *namespace),
            Event::Attribute(_, name, value) => NodeView::Attribute(*name, value),
            Event::Text(..) => NodeView::Text(&text),
            Event::Comment(_, _, text) => NodeView::Comment(text),
            Event::ProcessingInstruction(_, _, target, content) => {
                NodeView::ProcessingInstruction(*target, content.as_deref())
            }
            _ => unreachable!(),
        };
        let line = format!("{:?}", view);
        out.push_str(&format!("{}{}\n", "  ".repeat(depth), line));
        for &child in children {
            show(views, child, depth + 1, out);
        }
    }
    let mut out = String::new();
    show(&views, *document, 0, &mut out);
    out
}

/// Parse `xml` in `mode` through a recording sink, check what the sink got
/// against the tree, and check that the result is that of the parse
/// through the sink of `Xot`'s trees.
fn check_sink(xml: &str, mode: Mode) -> bool {
    let expected = parse_with(xml, mode, &mut ErrorSpans::default()).map(|_| ());
    let mut xot = Xot::new();
    let events = Rc::new(RefCell::new(Vec::new()));
    let sink = RecordingSink {
        events: events.clone(),
    };
    let tokenizer = match mode {
        Mode::Document => Tokenizer::from(xml),
        _ => Tokenizer::from_fragment(xml, 0..xml.len()),
    };
    let mut spans = ErrorSpans::default();
    let result = xot
        ._parse(tokenizer, xml, &mut spans, sink)
        .and_then(|builder| {
            let checked = match mode {
                Mode::Fragment if builder.is_current_node_document() => Ok(()),
                Mode::Fragment => Err(ParseError::UnclosedTag(
                    spans.unclosed_element(Node::new(builder.current_node_id)),
                )),
                _ => xot.check_document(&builder, xml, &mut spans),
            };
            match checked {
                Ok(()) => Ok(builder.tree),
                Err(error) => {
                    builder.abandon(&mut xot);
                    Err(error)
                }
            }
        });
    assert_eq!(
        format!("{:?}", result.as_ref().map(|_| ())),
        format!("{:?}", expected),
        "{:?} {:?}",
        mode,
        xml
    );
    let events = events.borrow();
    match result {
        Ok(document) => {
            let mut tree = String::new();
            dump(&xot, document, 0, &mut tree);
            assert_eq!(replay(&events), tree, "{:?} {:?}", mode, xml);
            true
        }
        Err(_) => {
            assert_eq!(events.last(), Some(&Event::Abandon), "{:?} {:?}", mode, xml);
            assert_eq!(
                events.iter().filter(|e| **e == Event::Abandon).count(),
                1,
                "{:?} {:?}",
                mode,
                xml
            );
            false
        }
    }
}

/// Documents with what the builder does on its side of the sink: namespaces,
/// attributes in namespaces, `xml:id`, references, CDATA sections next to
/// text, line ends, comments, processing instructions and a document type
/// declaration, and documents that fail at each of the builder's checks.
const SINK_SAMPLES: [&str; 24] = [
    "<a/>",
    "<?xml version=\"1.0\"?>\n<!-- c --><?p d?><a b=\"1\" c='2'/><!--e-->",
    "<a xmlns=\"urn:example:a\" xmlns:p=\"urn:example:p\" p:b=\"&lt;&#x41;\"><p:c xmlns:p=\"urn:example:q\"/><d xmlns=\"\"/></a>",
    "<a>t<![CDATA[c]]>u&amp;v<![CDATA[]]>\r\nw<b/>x\ry<!--z-->1<?p?>2</a>",
    "<a xml:id=\" i  j \"><b xml:id=\"k\"/></a>",
    "<a xml:id=\"i\"><b xml:id=\"i\"/></a>",
    "<a b=\"1\" b=\"2\"/>",
    "<a xmlns:p=\"urn:example:p\" xmlns:q=\"urn:example:p\" p:b=\"1\" q:b=\"2\"/>",
    "<a xmlns:p=\"urn:example:p\" xmlns:p=\"urn:example:q\"/>",
    "<p:a xmlns:p=\"urn:example:p\" xmlns:q=\"urn:example:p\"></q:a>",
    "<a></b>",
    "<a><b></a>",
    "<a>&unknown;</a>",
    "<a b=\"&#0;\"/>",
    "<p:a/>",
    "<a xmlns:xml=\"urn:example:x\"/>",
    "<!DOCTYPE a [<!ATTLIST a b ID #IMPLIED>]><a b=\" x \"/>",
    "<!DOCTYPE a [<!ATTLIST a b CDATA #IMPLIED>]><a b=\" x \"/>",
    "<!DOCTYPE a [<!ENTITY e \"v\">]><a>&e;</a>",
    "<!DOCTYPE a SYSTEM \"a.dtd\"><a>t</a>",
    "<?xml version=\"1.1\"?><a/>",
    "<a/><b/>",
    "t<a/>",
    "<a><?xml x?></a>",
];

#[test]
fn the_sink_gets_each_node_in_document_order() {
    let mut texts: Vec<String> = SINK_SAMPLES.iter().map(|s| s.to_string()).collect();
    let mut rng = Rng::new(2);
    for _ in 0..2000 {
        let mut text = String::new();
        random_content(&mut rng, 3, &mut text);
        texts.push(text);
    }
    for _ in 0..1000 {
        let len = 4 + rng.below(9);
        texts.push((0..len).map(|_| PIECES[rng.below(PIECES.len())]).collect());
    }
    let (mut parsed, mut failed) = (0, 0);
    for text in &texts {
        for mode in MODES {
            if check_sink(text, mode) {
                parsed += 1;
            } else {
                failed += 1;
            }
        }
    }
    assert!(
        parsed >= 1000 && failed >= 1000,
        "{} parsed, {} failed",
        parsed,
        failed
    );
}
