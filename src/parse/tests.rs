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
use std::collections::BTreeMap;

use xmlparser::Tokenizer;

use super::{ErrorSpans, SpanRecorder};
use crate::{Node, ParseError, Span, SpanInfo, Xot};

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
