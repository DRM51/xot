//! The XML name scanner of the tokenizer that xot carries (the copy in `xmlparser/` of this repository,
//! xmlparser 0.13.6 with a faster scan of names) against xmlparser 0.13.6
//! itself: the same tokens, the same errors and the same positions for
//! every input.
//!
//! The test of every Unicode scalar value in every name position takes a
//! few minutes without optimizations; it is ignored there. Run it with
//! `cargo test --release --test test_xmlparser_names -- --include-ignored`.

use original_xmlparser as original;
use xmlparser as vendored;

/// The tokens of `xml` as debug text, up to the first error, which is
/// given as its debug and display text.
macro_rules! tokens {
    ($module:ident, $xml:tt, $fragment:tt) => {{
        let xml: &str = $xml;
        let tokenizer = if $fragment {
            $module::Tokenizer::from_fragment(xml, 0..xml.len())
        } else {
            $module::Tokenizer::from(xml)
        };
        let mut out = Vec::new();
        for token in tokenizer {
            match token {
                Ok(token) => out.push(format!("{:?}", token)),
                Err(error) => {
                    out.push(format!("error {:?} | {}", error, error));
                    break;
                }
            }
        }
        out
    }};
}

/// What a stream makes of a name at `skip` bytes into `text`: the name
/// with a prefix, the name without one, and the name gone over by skip_name, each
/// with the position after it.
macro_rules! names {
    ($module:ident, $text:tt, $skip:tt) => {{
        let text: &str = $text;
        let mut qname = $module::Stream::from(text);
        qname.advance($skip);
        let q = format!("{:?} {}", qname.consume_qname(), qname.pos());
        let mut name = $module::Stream::from(text);
        name.advance($skip);
        let n = format!("{:?} {}", name.consume_name(), name.pos());
        let mut over = $module::Stream::from(text);
        over.advance($skip);
        let s = format!("{:?} {}", over.skip_name(), over.pos());
        (q, n, s)
    }};
}

/// The differences between the two tokenizers on `xml`, as text.
fn differences(xml: &str) -> Vec<String> {
    let mut found = Vec::new();
    for fragment in [false, true] {
        let a = tokens!(original, xml, fragment);
        let b = tokens!(vendored, xml, fragment);
        if a != b {
            found.push(format!(
                "{:?} (fragment {}):\n  original {:?}\n  vendored {:?}",
                xml, fragment, a, b
            ));
        }
    }
    for skip in 0..xml.len().min(4) {
        if xml.is_char_boundary(skip) {
            let a = names!(original, xml, skip);
            let b = names!(vendored, xml, skip);
            if a != b {
                found.push(format!(
                    "{:?} at {}:\n  original {:?}\n  vendored {:?}",
                    xml, skip, a, b
                ));
            }
        }
    }
    found
}

/// Every place of a name that the scanner reads, with `{c}` for the
/// character that is tried there.
const POSITIONS: &[&str] = &[
    // element names: first, inner, last character, prefix and local name
    "<{c}/>",
    "<a{c}/>",
    "<a{c}b/>",
    "<{c}:a/>",
    "<a:{c}/>",
    "<a:b{c}/>",
    "<p{c}:x/>",
    // attribute names
    "<a {c}='1'/>",
    "<a b{c}='1'/>",
    "<a x:{c}='1'/>",
    "<a {c}:x='1'/>",
    // end tags
    "<a>t</a{c}>",
    "<a{c}>t</a{c}>",
    "<a:b>t</a:b{c}>",
    // processing instruction targets
    "<?x{c} d?><a/>",
    "<?{c}?><a/>",
    // entity references
    "<a>&e{c};</a>",
    "<a b='&{c};'/>",
    // DOCTYPE and ENTITY names
    "<!DOCTYPE d{c} [<!ENTITY e{c} 'v'>]><a/>",
    "<!DOCTYPE {c}><a/>",
    "{c}",
];

/// Check the characters `chars` in every position, and panic with the
/// first differences.
fn check_chars(chars: impl Iterator<Item = char>) -> usize {
    let mut count = 0;
    let mut found = Vec::new();
    for c in chars {
        let c = c.to_string();
        for position in POSITIONS {
            let xml = position.replace("{c}", &c);
            found.extend(differences(&xml));
            count += 1;
        }
        assert!(found.len() < 20, "{}", found.join("\n"));
    }
    assert!(found.is_empty(), "{}", found.join("\n"));
    count
}

/// The characters where the classes of XML 1.0 name characters change
/// (production 4 and 4a), and their neighbours.
fn boundary_chars() -> Vec<char> {
    let edges: &[u32] = &[
        0x00, 0x09, 0x0A, 0x0D, 0x20, 0x2D, 0x2E, 0x2F, 0x30, 0x39, 0x3A, 0x3B, 0x40, 0x41, 0x005A,
        0x005B, 0x005F, 0x60, 0x61, 0x7A, 0x7B, 0x7F, 0x80, 0xB7, 0xC0, 0xD6, 0xD7, 0xD8, 0xF6,
        0xF7, 0xF8, 0x2FF, 0x300, 0x36F, 0x370, 0x37D, 0x37E, 0x37F, 0x1FFF, 0x2000, 0x200B,
        0x200C, 0x200D, 0x200E, 0x203F, 0x2040, 0x2041, 0x206F, 0x2070, 0x218F, 0x2190, 0x2BFF,
        0x2C00, 0x2FEF, 0x2FF0, 0x3000, 0x3001, 0xD7FF, 0xE000, 0xF8FF, 0xF900, 0xFDCF, 0xFDD0,
        0xFDEF, 0xFDF0, 0xFFFD, 0xFFFE, 0xFFFF, 0x10000, 0xEFFFF, 0xF0000, 0x10FFFF,
    ];
    let mut chars = Vec::new();
    for &edge in edges {
        for code in edge.saturating_sub(1)..=edge + 1 {
            if let Some(c) = char::from_u32(code) {
                if !chars.contains(&c) {
                    chars.push(c);
                }
            }
        }
    }
    chars
}

#[test]
fn ascii_and_latin_1_in_every_name_position() {
    let count = check_chars((0u32..0x300).filter_map(char::from_u32));
    assert_eq!(count, 0x300 * POSITIONS.len());
}

#[test]
fn edges_of_name_characters_in_every_name_position() {
    check_chars(boundary_chars().into_iter());
}

#[test]
#[ignore = "every Unicode scalar value: run with optimizations"]
fn every_unicode_scalar_value_in_every_name_position() {
    let count = check_chars((0u32..=0x10FFFF).filter_map(char::from_u32));
    assert_eq!(count, (0x110000 - 0x800) * POSITIONS.len());
}

/// A seeded xorshift64 generator.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

fn random_names(seed: u64, count: usize) {
    let mut pool = boundary_chars();
    pool.extend("aZ_-.:09:: \t>/=\"'<&;!?".chars());
    let mut rng = Rng(seed);
    let mut found = Vec::new();
    for _ in 0..count {
        let len = (rng.next() % 12) as usize;
        let name: String = (0..len)
            .map(|_| pool[(rng.next() % pool.len() as u64) as usize])
            .collect();
        let xml = match rng.next() % 6 {
            0 => format!("<{}/>", name),
            1 => format!("<a {}='v'/>", name),
            2 => format!("<a>x</{}>", name),
            3 => format!("<?{} d?><a/>", name),
            4 => format!("<a>&{};</a>", name),
            _ => format!("<{n}>x</{n}>", n = name),
        };
        found.extend(differences(&xml));
        assert!(found.len() < 20, "{}", found.join("\n"));
    }
    assert!(found.is_empty(), "{}", found.join("\n"));
}

#[test]
fn random_names_from_boundary_characters() {
    random_names(0x9E37_79B9_7F4A_7C15, 50_000);
}

#[test]
#[ignore = "three million names: run with optimizations"]
fn many_random_names_from_boundary_characters() {
    random_names(0x2545_F491_4F6C_DD1D, 3_000_000);
}

/// The name scanner's quirks, as xmlparser 0.13.6 has them.
#[test]
fn quirks_of_xmlparser() {
    // an empty prefix is accepted
    let tokens = tokens!(vendored, "<:a/>", false);
    assert_eq!(
        tokens[0],
        r#"ElementStart { prefix: StrSpan("" 1..1), local: StrSpan("a" 2..3), span: StrSpan("<:a" 0..3) }"#
    );
    for xml in [
        "<:a/>",
        "<a:b:c/>",
        "<a:/>",
        "<a:1b/>",
        "<1a/>",
        "<a\u{300}/>",
        "<\u{300}a/>",
        "<a b:c:d='1'/>",
        "<a>t</a:b:c>",
    ] {
        assert!(
            differences(xml).is_empty(),
            "{}",
            differences(xml).join("\n")
        );
    }
    // the errors, with their text and position: a second ':', a prefix
    // without a local name, a local name that starts with a digit and a
    // name that starts with one are all errors at the start of the element
    let error = |xml: &str| tokens!(vendored, xml, false).pop().unwrap();
    for xml in ["<a:b:c/>", "<a:/>", "<a:1b/>", "<1a/>"] {
        assert_eq!(
            error(xml),
            "error InvalidElement(InvalidName, TextPos { row: 1, col: 1 }) | invalid element at 1:1 cause invalid name token",
            "{}",
            xml
        );
    }
    // a combining character is accepted after the first character of a
    // name, but not as the first
    let tokens = tokens!(vendored, "<a\u{300}/>", false);
    assert_eq!(
        tokens[0],
        "ElementStart { prefix: StrSpan(\"\" 0..0), local: StrSpan(\"a\\u{300}\" 1..4), span: StrSpan(\"<a\\u{300}\" 0..4) }"
    );
    assert_eq!(
        error("<\u{300}a/>"),
        "error InvalidElement(InvalidName, TextPos { row: 1, col: 1 }) | invalid element at 1:1 cause invalid name token"
    );
}
