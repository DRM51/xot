//! The text of a text node: up to 24 bytes in the node itself, a longer
//! text in a `String`.
//!
//! Most texts of a document are short (a number, a date, a code, a word or
//! two), and a `String` takes an allocation on the heap for each of them
//! besides its 24 bytes in the node. A `TextBuf` is 32 bytes, so a text node
//! is no larger than with a `String`, and keeps a text of up to 24 bytes
//! without an allocation.
//!
//! A text in place is an `arrayvec::ArrayString`, which gives its `str`
//! without validating its bytes again: texts are read far more often than
//! they are made, and validating that bytes are UTF-8 at every read would
//! cost more than the read itself.

use std::fmt;
use std::hash::{Hash, Hasher};

use arrayvec::ArrayString;

/// The most bytes that a text keeps in place: an `ArrayString` of this
/// many bytes and its length take 28 bytes, so that with the tag of
/// `TextBuf` it takes no more room than a `String` and a tag.
pub(crate) const INLINE: usize = 24;

/// A text, in place or in a `String`.
///
/// It compares, hashes and debug-prints as the `str` it holds, as the
/// `String` it replaces did: where the text is kept does not show.
pub(crate) enum TextBuf {
    /// A text of at most `INLINE` bytes.
    Inline(ArrayString<INLINE>),
    /// Any text: a longer one, or one that `get_mut` handed out.
    Heap(String),
}

impl TextBuf {
    /// The text `text`, in place if it fits.
    #[inline]
    pub(crate) fn new(text: &str) -> Self {
        match ArrayString::from(text) {
            Ok(inline) => TextBuf::Inline(inline),
            Err(_) => TextBuf::Heap(text.to_string()),
        }
    }

    /// The text `text`, in place if it fits, else in `text` itself.
    #[inline]
    pub(crate) fn from_string(text: String) -> Self {
        match ArrayString::from(&text) {
            Ok(inline) => TextBuf::Inline(inline),
            Err(_) => TextBuf::Heap(text),
        }
    }

    /// The text.
    #[inline]
    pub(crate) fn as_str(&self) -> &str {
        match self {
            TextBuf::Inline(text) => text.as_str(),
            TextBuf::Heap(text) => text,
        }
    }

    /// The text as a `String`, which it then keeps: a text in place moves
    /// to a `String` of its own first.
    #[inline]
    pub(crate) fn get_mut(&mut self) -> &mut String {
        if let TextBuf::Inline(text) = self {
            *self = TextBuf::Heap(text.as_str().to_string());
        }
        match self {
            TextBuf::Heap(text) => text,
            TextBuf::Inline(_) => unreachable!("the text was moved to a String"),
        }
    }

    /// Add `text` after the text. A text in place stays in place while it
    /// fits; a `String` grows as `push_str` makes it grow.
    pub(crate) fn push_str(&mut self, text: &str) {
        match self {
            TextBuf::Inline(inline) => {
                if inline.try_push_str(text).is_err() {
                    // as `push_str` grows a `String` that is just long
                    // enough for the text it has
                    let old = inline.len();
                    let mut joined = String::with_capacity((old + text.len()).max(2 * old));
                    joined.push_str(inline.as_str());
                    joined.push_str(text);
                    *self = TextBuf::Heap(joined);
                }
            }
            TextBuf::Heap(heap) => heap.push_str(text),
        }
    }

    /// Add `text` before the text. A text in place stays in place while it
    /// fits; a `String` with room for `text` takes it in place, and
    /// otherwise the joined text gets a `String` just long enough for it.
    pub(crate) fn insert_str_at_start(&mut self, text: &str) {
        match self {
            TextBuf::Inline(inline) => {
                if inline.len() + text.len() <= INLINE {
                    let mut joined = ArrayString::new();
                    joined.push_str(text);
                    joined.push_str(inline.as_str());
                    *inline = joined;
                } else {
                    let mut joined = String::with_capacity(text.len() + inline.len());
                    joined.push_str(text);
                    joined.push_str(inline.as_str());
                    *self = TextBuf::Heap(joined);
                }
            }
            TextBuf::Heap(heap) => {
                if heap.capacity() - heap.len() >= text.len() {
                    heap.insert_str(0, text);
                } else {
                    let mut joined = String::with_capacity(text.len() + heap.len());
                    joined.push_str(text);
                    joined.push_str(heap);
                    *heap = joined;
                }
            }
        }
    }
}

impl Clone for TextBuf {
    /// A copy is in place if it fits, also of a text in a `String`.
    fn clone(&self) -> Self {
        match self {
            TextBuf::Inline(text) => TextBuf::Inline(*text),
            TextBuf::Heap(text) => TextBuf::new(text),
        }
    }
}

impl PartialEq for TextBuf {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for TextBuf {}

impl Hash for TextBuf {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_str().hash(state)
    }
}

impl fmt::Debug for TextBuf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xmlvalue::Text;

    /// The hash of `value` with fixed seeds.
    fn hash_of<T: Hash + ?Sized>(value: &T) -> u64 {
        ahash::RandomState::with_seeds(1, 2, 3, 4).hash_one(value)
    }

    fn is_inline(text: &TextBuf) -> bool {
        matches!(text, TextBuf::Inline(_))
    }

    /// Texts of 0, 1, 23, 24, 25, 29, 30, 31 and 64 bytes, and with a
    /// character of 2, 3 and 4 bytes that ends at byte 23, 24, 25, 29, 30 or
    /// 31.
    fn samples() -> Vec<String> {
        let mut samples: Vec<String> = [0, 1, 23, 24, 25, 29, 30, 31, 64]
            .iter()
            .map(|&len| "a".repeat(len))
            .collect();
        for c in ['\u{e9}', '\u{65e5}', '\u{1f600}'] {
            for end in [23, 24, 25, 29, 30, 31] {
                samples.push(format!("{}{}", "a".repeat(end - c.len_utf8()), c));
            }
            // and ones that start before byte 24 or 30 and end after it
            samples.push(format!("{}{}b", "a".repeat(23), c));
            samples.push(format!("{}{}b", "a".repeat(29), c));
        }
        samples
    }

    #[test]
    fn texts_of_up_to_24_bytes_are_in_place() {
        for sample in samples() {
            for text in [TextBuf::new(&sample), TextBuf::from_string(sample.clone())] {
                assert_eq!(text.as_str(), sample);
                assert_eq!(is_inline(&text), sample.len() <= INLINE, "{:?}", sample);
            }
        }
    }

    #[test]
    fn get_mut_moves_a_text_to_a_string() {
        for sample in samples() {
            let mut text = Text::new(sample.clone());
            text.get_mut().push('x');
            assert!(!is_inline(&text.text));
            assert_eq!(text.get(), format!("{}x", sample));
            text.get_mut().pop();
            assert_eq!(text.get(), sample);
            // set puts a text that fits in place again
            text.set(sample.clone());
            assert_eq!(text.get(), sample);
            assert_eq!(is_inline(&text.text), sample.len() <= INLINE);
            text.set("a text that is longer than thirty bytes");
            assert!(!is_inline(&text.text));
            text.set(String::new());
            assert!(is_inline(&text.text));
            assert_eq!(text.get(), "");
        }
    }

    #[test]
    fn equality_hash_and_debug_as_a_string() {
        let samples = samples();
        for a in &samples {
            let text = Text::new(a.clone());
            let mut moved = Text::new(a.clone());
            moved.get_mut();
            assert_eq!(hash_of(&text), hash_of(a));
            assert_eq!(hash_of(&text.text), hash_of(a.as_str()));
            assert_eq!(hash_of(&moved), hash_of(&text));
            assert_eq!(format!("{:?}", text), format!("Text {{ text: {:?} }}", a));
            assert_eq!(format!("{:?}", moved), format!("{:?}", text));
            assert_eq!(text, moved);
            for b in &samples {
                assert_eq!(text == Text::new(b.clone()), a == b);
            }
        }
    }

    #[test]
    fn a_copy_is_in_place_if_it_fits() {
        for sample in samples() {
            let mut text = TextBuf::new(&sample);
            text.get_mut();
            let copy = text.clone();
            assert_eq!(copy, text);
            assert_eq!(is_inline(&copy), sample.len() <= INLINE);
        }
    }

    #[test]
    fn push_str_and_insert_str_at_start() {
        let pieces = [
            "",
            "a",
            "\u{e9}",
            "bcdefghij",
            "\u{65e5}\u{672c}",
            "0123456789012345678901234567",
        ];
        for a in pieces {
            for b in pieces {
                for c in pieces {
                    let mut pushed = TextBuf::new(a);
                    pushed.push_str(b);
                    pushed.push_str(c);
                    let joined = format!("{}{}{}", a, b, c);
                    assert_eq!(pushed.as_str(), joined);
                    assert_eq!(is_inline(&pushed), joined.len() <= INLINE);

                    let mut inserted = TextBuf::new(c);
                    inserted.insert_str_at_start(b);
                    inserted.insert_str_at_start(a);
                    assert_eq!(inserted.as_str(), joined);
                    assert_eq!(is_inline(&inserted), joined.len() <= INLINE);
                }
            }
        }
    }

    #[test]
    fn parsed_and_joined_texts() {
        // texts that the parser makes in one piece, from references, from
        // a CDATA section joined to the text before it, and across byte 24
        let mut xot = crate::Xot::new();
        let cases: Vec<(String, Option<String>)> = vec![
            ("<a></a>".to_string(), None),
            ("<a>b</a>".to_string(), Some("b".to_string())),
            (format!("<a>{}</a>", "c".repeat(24)), Some("c".repeat(24))),
            (format!("<a>{}</a>", "c".repeat(25)), Some("c".repeat(25))),
            (
                format!("<a>{}&amp;d</a>", "c".repeat(22)),
                Some(format!("{}&d", "c".repeat(22))),
            ),
            (
                format!("<a>{}&amp;d</a>", "c".repeat(23)),
                Some(format!("{}&d", "c".repeat(23))),
            ),
            (
                format!("<a>{}&amp;d</a>", "c".repeat(28)),
                Some(format!("{}&d", "c".repeat(28))),
            ),
            (
                format!("<a>{}<![CDATA[<\u{e9}>]]>x</a>", "c".repeat(19)),
                Some(format!("{}<\u{e9}>x", "c".repeat(19))),
            ),
            (
                format!("<a>{}<![CDATA[<\u{e9}>]]>x</a>", "c".repeat(26)),
                Some(format!("{}<\u{e9}>x", "c".repeat(26))),
            ),
            (
                format!(
                    "<a>{}<![CDATA[{}]]></a>",
                    "\u{65e5}\u{672c}",
                    "\u{65e5}\u{672c}".repeat(4)
                ),
                Some("\u{65e5}\u{672c}".repeat(5)),
            ),
            ("<a>a\r\nb\rc</a>".to_string(), Some("a\nb\nc".to_string())),
        ];
        for (xml, text) in cases {
            let root = xot.parse(&xml).unwrap();
            let a = xot.document_element(root).unwrap();
            let got = xot
                .first_child(a)
                .map(|t| xot.text_str(t).unwrap().to_string());
            assert_eq!(got, text, "{}", xml);
            let output = xot.to_string(root).unwrap();
            let again = xot.parse(&output).unwrap();
            assert!(xot.deep_equal(root, again), "{}", xml);
        }
    }

    #[test]
    fn new_and_appended_texts() {
        let mut xot = crate::Xot::new();
        let name = xot.add_name("p");
        let p = xot.new_element(name);
        let mut expected = String::new();
        for piece in ["ab", "\u{e9}", "0123456789", "0123456789012345", "x"] {
            xot.append_text(p, piece).unwrap();
            expected.push_str(piece);
            let text = xot.first_child(p).unwrap();
            assert_eq!(xot.text_str(text).unwrap(), expected);
            assert_eq!(xot.children(p).count(), 1);
        }
        let short = xot.new_text("short");
        let long = xot.new_text(&"l".repeat(40));
        assert_eq!(xot.text_str(short).unwrap(), "short");
        assert_eq!(xot.text_str(long).unwrap(), "l".repeat(40));
        // texts joined before an inline text and before a heap one
        xot.prepend(p, short).unwrap();
        assert_eq!(
            xot.text_str(xot.first_child(p).unwrap()).unwrap(),
            format!("short{}", expected)
        );
        let q = xot.new_element(name);
        let tail = xot.new_text("tail");
        xot.append(q, tail).unwrap();
        xot.prepend(q, long).unwrap();
        assert_eq!(
            xot.text_str(xot.first_child(q).unwrap()).unwrap(),
            format!("{}tail", "l".repeat(40))
        );
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn sizes() {
        use std::mem::size_of;
        assert_eq!(size_of::<TextBuf>(), 32);
        assert_eq!(size_of::<Text>(), 32);
        assert_eq!(size_of::<crate::Value>(), 40);
    }
}
