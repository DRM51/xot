//! A scan of the internal subset of a document type declaration.
//!
//! The parser accepts a document type declaration and skips it: nothing
//! external is read, and the declarations of the internal subset are not
//! applied. So that a document that depends on its DTD fails instead of
//! giving another tree, this scan finds what the parser would have to apply:
//!
//! - the general entities that are declared, so that a reference to one of
//!   them gives [`ParseError::DtdUnsupported`] instead of being taken for an
//!   unknown entity;
//! - the attributes that are declared with a type other than `CDATA`, whose
//!   values DTD attribute-value normalization would change;
//! - attribute default values (a literal or `#FIXED`) and parameter-entity
//!   references, which give [`ParseError::DtdUnsupported`] right away.
//!
//! As XML 1.0 (section 3.3) says, the first declaration of an attribute of
//! an element type is the one that counts and later ones are ignored: a later
//! declaration neither makes the attribute typed nor gives it a default.
//!
//! It also checks the public identifier of a `NOTATION` declaration, which
//! the tokenizer skips: a character other than a `PubidChar` is not
//! well-formed.
//!
//! The scan is one pass over the bytes of the internal subset, without
//! recursion; it keeps only the names that it notes.
//!
//! The tokenizer reads the internal subset after the scan, and reports what
//! is not well-formed. The scan stops without an error at anything it does
//! not know, such as a conditional section, and leaves it to the tokenizer:
//! the tokenizer rejects these in an internal subset. It ends an `ELEMENT` or
//! a `NOTATION` declaration at the first `>`, as the tokenizer does, so that
//! both see the same declarations.

use ahash::{HashSet, HashSetExt};

use crate::error::ParseError;
use crate::Span;

/// What the internal subset declares that the parser cannot apply, but has
/// to know about.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct InternalSubset {
    /// The names of the general entities that are declared.
    pub(crate) entities: Vec<Box<str>>,
    /// The element name and the attribute name, as written, of each
    /// attribute declared with a type other than `CDATA`.
    pub(crate) typed: Vec<(Box<str>, Box<str>)>,
}

/// Scan the internal subset of a document type declaration in `text`, which
/// starts at the byte index `start`, right after its `[`.
///
/// The spans of the errors are byte ranges in `text`.
pub(crate) fn scan(text: &str, start: usize, xml: &str) -> Result<InternalSubset, ParseError> {
    let mut scanner = Scanner {
        bytes: text.as_bytes(),
        pos: start,
        xml,
    };
    let mut subset = InternalSubset::default();
    let mut declared = Declared::new();
    scanner.subset(&mut subset, &mut declared)?;
    Ok(subset)
}

/// The attribute types of XML 1.0 (production `AttType`) that are a keyword,
/// apart from `NOTATION`.
const KEYWORD_TYPES: [&[u8]; 8] = [
    b"CDATA",
    b"ID",
    b"IDREF",
    b"IDREFS",
    b"ENTITY",
    b"ENTITIES",
    b"NMTOKEN",
    b"NMTOKENS",
];

/// The attributes declared so far, of any type, each as its element name and
/// its attribute name as written, separated by a space (which no name
/// contains): only the first declaration of each one counts.
struct Declared {
    names: HashSet<Box<str>>,
    /// The buffer in which a name is written to look it up.
    key: String,
}

impl Declared {
    fn new() -> Self {
        Declared {
            names: HashSet::new(),
            key: String::new(),
        }
    }

    /// Note that the attribute `attribute` of the element `element` is
    /// declared; whether this is its first declaration.
    fn first(&mut self, element: &str, attribute: &str) -> bool {
        self.key.clear();
        self.key.push_str(element);
        self.key.push(' ');
        self.key.push_str(attribute);
        if self.names.contains(self.key.as_str()) {
            return false;
        }
        self.names.insert(self.key.as_str().into());
        true
    }
}

struct Scanner<'t> {
    bytes: &'t [u8],
    pos: usize,
    /// The parsed text, for the positions of errors.
    xml: &'t str,
}

/// Whether `b` is a character that a public identifier can have (production
/// `PubidChar`).
fn is_pubid_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b" \r\n-'()+,./:=?;!*#@$_%".contains(&b)
}

/// Check the public identifier `literal`, without its quotes, which starts
/// at the byte index `start` of `xml`: a character other than a `PubidChar`
/// is not well-formed. `error` makes the tokenizer's error of the
/// declaration.
pub(crate) fn check_public_id(
    literal: &str,
    start: usize,
    xml: &str,
    error: fn(xmlparser::StreamError, xmlparser::TextPos) -> xmlparser::Error,
) -> Result<(), ParseError> {
    match literal.bytes().position(|b| !is_pubid_char(b)) {
        None => Ok(()),
        Some(i) => Err(public_id_error(start + i, xml, error)),
    }
}

#[cold]
fn public_id_error(
    at: usize,
    xml: &str,
    error: fn(xmlparser::StreamError, xmlparser::TextPos) -> xmlparser::Error,
) -> ParseError {
    let pos = xmlparser::Stream::from(xml).gen_text_pos_from(at);
    ParseError::XmlParser(error(xmlparser::StreamError::InvalidExternalID, pos), at)
}

/// Whether `b` is white space (production `S`).
fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

/// Whether `b` can be in a name or a name token. Bytes of characters
/// outside ASCII all can: the tokenizer checks the names it reads, and the
/// scan only has to find where they end.
fn is_name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b':' | b'-' | b'.') || b >= 0x80
}

impl<'t> Scanner<'t> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn starts_with(&self, prefix: &[u8]) -> bool {
        self.bytes[self.pos..].starts_with(prefix)
    }

    /// Skip white space; whether there was any.
    fn skip_space(&mut self) -> bool {
        let start = self.pos;
        while self.peek().is_some_and(is_space) {
            self.pos += 1;
        }
        self.pos > start
    }

    /// The name or name token at the position, which it moves past; empty
    /// if there is none.
    fn name(&mut self) -> &'t [u8] {
        let start = self.pos;
        while self.peek().is_some_and(is_name_byte) {
            self.pos += 1;
        }
        &self.bytes[start..self.pos]
    }

    /// The index right after the first `needle` from the position, if there
    /// is one.
    fn find_after(&self, needle: &[u8]) -> Option<usize> {
        self.bytes[self.pos..]
            .windows(needle.len())
            .position(|w| w == needle)
            .map(|i| self.pos + i + needle.len())
    }

    /// The span of the parameter-entity reference `%Name;` at `at`, or of the
    /// `%` alone if no name and `;` follow it.
    fn reference_span(&self, at: usize) -> Span {
        let mut end = at + 1;
        while self.bytes.get(end).copied().is_some_and(is_name_byte) {
            end += 1;
        }
        if end > at + 1 && self.bytes.get(end) == Some(&b';') {
            Span::new(at, end + 1)
        } else {
            Span::new(at, at + 1)
        }
    }

    fn reference_error(&self, at: usize) -> ParseError {
        ParseError::DtdUnsupported(self.reference_span(at))
    }

    /// The span of the declaration that starts at `start`: up to and with the
    /// first `>` outside quotes, or the first `>` at all if quotes are not
    /// closed, or the end of the text.
    fn declaration_span(&self, start: usize) -> Span {
        let mut quote = None;
        for (i, &b) in self.bytes[start..].iter().enumerate() {
            match quote {
                Some(q) if b == q => quote = None,
                Some(_) => {}
                None if b == b'"' || b == b'\'' => quote = Some(b),
                None if b == b'>' => return Span::new(start, start + i + 1),
                None => {}
            }
        }
        match self.bytes[start..].iter().position(|&b| b == b'>') {
            Some(i) => Span::new(start, start + i + 1),
            None => Span::new(start, self.bytes.len()),
        }
    }

    /// The declarations of the internal subset, up to the `]` that ends it.
    fn subset(
        &mut self,
        subset: &mut InternalSubset,
        declared: &mut Declared,
    ) -> Result<(), ParseError> {
        loop {
            self.skip_space();
            let Some(b) = self.peek() else {
                return Ok(());
            };
            if b == b']' {
                return Ok(());
            } else if self.starts_with(b"<!--") {
                self.pos += 4;
                match self.find_after(b"-->") {
                    Some(end) => self.pos = end,
                    None => return Ok(()),
                }
            } else if self.starts_with(b"<?") {
                match self.find_after(b"?>") {
                    Some(end) => self.pos = end,
                    None => return Ok(()),
                }
            } else if self.starts_with(b"<!ENTITY") {
                if !self.entity(subset)? {
                    return Ok(());
                }
            } else if self.starts_with(b"<!ATTLIST") {
                self.attlist(subset, declared)?;
            } else if self.starts_with(b"<!ELEMENT") || self.starts_with(b"<!NOTATION") {
                if self.starts_with(b"<!NOTATION") {
                    self.check_notation_public_id()?;
                }
                if !self.skip_to_first_close()? {
                    return Ok(());
                }
            } else if b == b'%' {
                let span = self.reference_span(self.pos);
                if span.end - span.start > 1 {
                    return Err(ParseError::DtdUnsupported(span));
                }
                return Ok(());
            } else {
                return Ok(());
            }
        }
    }

    /// The public identifier of the `NOTATION` declaration at the position,
    /// `'<!NOTATION' S Name S 'PUBLIC' S PubidLiteral`, must have only
    /// `PubidChar`s; the tokenizer skips the declaration without a check.
    /// The position does not move.
    fn check_notation_public_id(&self) -> Result<(), ParseError> {
        let mut scanner = Scanner {
            bytes: self.bytes,
            pos: self.pos + b"<!NOTATION".len(),
            xml: self.xml,
        };
        if !scanner.skip_space() || scanner.name().is_empty() || !scanner.skip_space() {
            return Ok(());
        }
        if !scanner.starts_with(b"PUBLIC") {
            return Ok(());
        }
        scanner.pos += b"PUBLIC".len();
        if !scanner.skip_space() {
            return Ok(());
        }
        let Some(quote @ (b'"' | b'\'')) = scanner.peek() else {
            return Ok(());
        };
        let start = scanner.pos + 1;
        let Some(length) = scanner.bytes[start..].iter().position(|&b| b == quote) else {
            return Ok(());
        };
        // the literal ends at a quote, so it is whole characters
        match std::str::from_utf8(&scanner.bytes[start..start + length]) {
            Ok(literal) => check_public_id(
                literal,
                start,
                scanner.xml,
                xmlparser::Error::InvalidDoctype,
            ),
            Err(_) => Ok(()),
        }
    }

    /// Skip an `ELEMENT` or a `NOTATION` declaration up to the first `>`, as
    /// the tokenizer does. A `%` outside quotes is a parameter-entity
    /// reference. Whether there was a `>`.
    fn skip_to_first_close(&mut self) -> Result<bool, ParseError> {
        let mut quote = None;
        while let Some(b) = self.peek() {
            match quote {
                _ if b == b'>' => {
                    self.pos += 1;
                    return Ok(true);
                }
                Some(q) if b == q => quote = None,
                Some(_) => {}
                None if b == b'"' || b == b'\'' => quote = Some(b),
                None if b == b'%' => return Err(self.reference_error(self.pos)),
                None => {}
            }
            self.pos += 1;
        }
        Ok(false)
    }

    /// Skip the remainder of an `ENTITY` declaration up to the first `>` outside
    /// quotes, as the tokenizer reads it. A `%` outside quotes is a
    /// parameter-entity reference. Whether there was a `>`.
    fn skip_literals_to_close(&mut self) -> Result<bool, ParseError> {
        let mut quote = None;
        while let Some(b) = self.peek() {
            match quote {
                Some(q) if b == q => quote = None,
                Some(_) => {}
                None if b == b'"' || b == b'\'' => quote = Some(b),
                None if b == b'>' => {
                    self.pos += 1;
                    return Ok(true);
                }
                None if b == b'%' => return Err(self.reference_error(self.pos)),
                None => {}
            }
            self.pos += 1;
        }
        Ok(false)
    }

    /// `'<!ENTITY' S ('%' S)? Name S (EntityValue | ExternalID NDataDecl?)
    /// S? '>'`. Notes the name of a general entity. A `%` in an entity
    /// value is a parameter-entity reference. Whether the declaration could
    /// be read; if not, the tokenizer reports it.
    fn entity(&mut self, subset: &mut InternalSubset) -> Result<bool, ParseError> {
        self.pos += b"<!ENTITY".len();
        if !self.skip_space() {
            return Ok(false);
        }
        let parameter = self.peek() == Some(b'%');
        if parameter {
            self.pos += 1;
            if !self.skip_space() {
                return Ok(false);
            }
        }
        let name = self.name();
        if name.is_empty() || !self.skip_space() {
            return Ok(false);
        }
        let name = std::str::from_utf8(name).ok();
        if let Some(quote @ (b'"' | b'\'')) = self.peek() {
            self.pos += 1;
            loop {
                match self.peek() {
                    None => return Ok(false),
                    Some(b) if b == quote => {
                        self.pos += 1;
                        break;
                    }
                    Some(b'%') => return Err(self.reference_error(self.pos)),
                    Some(_) => self.pos += 1,
                }
            }
        }
        if !self.skip_literals_to_close()? {
            return Ok(false);
        }
        if !parameter {
            if let Some(name) = name {
                subset.entities.push(name.into());
            }
        }
        Ok(true)
    }

    /// `'<!ATTLIST' S Name (S AttDef)* S? '>'`. Notes the attributes with
    /// a type other than `CDATA`; a default value, a parameter-entity
    /// reference or anything else that cannot be read gives
    /// `DtdUnsupported`. An attribute that an earlier definition declares
    /// keeps that definition: the later one is read, but neither noted nor
    /// rejected for its default.
    fn attlist(
        &mut self,
        subset: &mut InternalSubset,
        declared: &mut Declared,
    ) -> Result<(), ParseError> {
        let start = self.pos;
        let unreadable =
            |scanner: &Self| ParseError::DtdUnsupported(scanner.declaration_span(start));
        self.pos += b"<!ATTLIST".len();
        if !self.skip_space() {
            return Err(self.reference_or(unreadable(self)));
        }
        let element = self.name();
        if element.is_empty() {
            return Err(self.reference_or(unreadable(self)));
        }
        loop {
            let space = self.skip_space();
            match self.peek() {
                Some(b'>') => {
                    self.pos += 1;
                    return Ok(());
                }
                Some(_) if space => {}
                _ => return Err(self.reference_or(unreadable(self))),
            }
            // AttDef ::= S Name S AttType S DefaultDecl
            let attribute = self.name();
            if attribute.is_empty() || !self.skip_space() {
                return Err(self.reference_or(unreadable(self)));
            }
            let typed = match self.peek() {
                Some(b'(') => {
                    if !self.token_group() {
                        return Err(self.reference_or(unreadable(self)));
                    }
                    true
                }
                _ => {
                    let keyword = self.name();
                    if keyword == b"NOTATION" {
                        self.skip_space();
                        if self.peek() != Some(b'(') || !self.token_group() {
                            return Err(self.reference_or(unreadable(self)));
                        }
                        true
                    } else if KEYWORD_TYPES.contains(&keyword) {
                        keyword != b"CDATA"
                    } else {
                        return Err(self.reference_or(unreadable(self)));
                    }
                }
            };
            if !self.skip_space() {
                return Err(self.reference_or(unreadable(self)));
            }
            // names end at an ASCII byte, so they are whole characters
            let (Ok(element), Ok(attribute)) =
                (std::str::from_utf8(element), std::str::from_utf8(attribute))
            else {
                return Err(unreadable(self));
            };
            let first = declared.first(element, attribute);
            // DefaultDecl ::= '#REQUIRED' | '#IMPLIED' | (('#FIXED' S)? AttValue)
            if self.starts_with(b"#REQUIRED") {
                self.pos += b"#REQUIRED".len();
            } else if self.starts_with(b"#IMPLIED") {
                self.pos += b"#IMPLIED".len();
            } else if self.starts_with(b"#FIXED") || matches!(self.peek(), Some(b'"' | b'\'')) {
                // an attribute default, which the parser would have to add,
                // unless an earlier definition binds the attribute
                if first || !self.ignored_default() {
                    return Err(unreadable(self));
                }
            } else {
                return Err(self.reference_or(unreadable(self)));
            }
            if typed && first {
                subset.typed.push((element.into(), attribute.into()));
            }
        }
    }

    /// Skip the default `('#FIXED' S)? AttValue` at the position, of a
    /// definition that is ignored. Whether it could be read: a value with a
    /// `<`, `>` or `&` cannot, since the tokenizer ends the declaration at
    /// the first `>` and references in a default are not checked here.
    fn ignored_default(&mut self) -> bool {
        if self.starts_with(b"#FIXED") {
            self.pos += b"#FIXED".len();
            if !self.skip_space() {
                return false;
            }
        }
        let Some(quote @ (b'"' | b'\'')) = self.peek() else {
            return false;
        };
        let start = self.pos + 1;
        let Some(length) = self.bytes[start..].iter().position(|&b| b == quote) else {
            return false;
        };
        if self.bytes[start..start + length]
            .iter()
            .any(|b| matches!(b, b'<' | b'>' | b'&'))
        {
            return false;
        }
        self.pos = start + length + 1;
        true
    }

    /// The error for a parameter-entity reference at the position, if there
    /// is one there, or else `other`.
    fn reference_or(&self, other: ParseError) -> ParseError {
        if self.peek() == Some(b'%') {
            self.reference_error(self.pos)
        } else {
            other
        }
    }

    /// `'(' S? Nmtoken (S? '|' S? Nmtoken)* S? ')'`, the names of a
    /// `NOTATION` type or the tokens of an enumerated type. Whether it could be
    /// read; the position is then right after the `)`.
    fn token_group(&mut self) -> bool {
        self.pos += 1;
        loop {
            self.skip_space();
            if self.name().is_empty() {
                return false;
            }
            self.skip_space();
            match self.peek() {
                Some(b'|') => self.pos += 1,
                Some(b')') => {
                    self.pos += 1;
                    return true;
                }
                _ => return false,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan_subset(subset: &str) -> Result<InternalSubset, ParseError> {
        let text = format!("<!DOCTYPE doc [{}]><doc/>", subset);
        scan(&text, "<!DOCTYPE doc [".len(), &text)
    }

    fn err_span(subset: &str) -> (usize, usize) {
        let lead = "<!DOCTYPE doc [".len();
        match scan_subset(subset) {
            Err(ParseError::DtdUnsupported(span)) => (span.start - lead, span.end - lead),
            other => panic!("{:?}: expected DtdUnsupported, got {:?}", subset, other),
        }
    }

    #[test]
    fn empty() {
        assert_eq!(scan_subset("").unwrap(), InternalSubset::default());
        assert_eq!(scan_subset(" \n\t ").unwrap(), InternalSubset::default());
    }

    #[test]
    fn notes_general_entities_only() {
        let subset = scan_subset(
            r#"<!ENTITY a "x"> <!ENTITY % p "y"> <!ENTITY b SYSTEM "b.xml"> <!ENTITY c PUBLIC "-//P//X" "c.xml" NDATA n>"#,
        )
        .unwrap();
        let names: Vec<&str> = subset.entities.iter().map(|n| n.as_ref()).collect();
        assert_eq!(names, ["a", "b", "c"]);
        assert!(subset.typed.is_empty());
    }

    #[test]
    fn quotes_hide_brackets_and_closes() {
        let subset = scan_subset(r#"<!ENTITY a "]>"><!ENTITY b SYSTEM ']>'>"#).unwrap();
        assert_eq!(subset.entities.len(), 2);
    }

    #[test]
    fn notes_typed_attributes() {
        let subset = scan_subset(
            "<!ATTLIST doc a ID #REQUIRED b (x|y) #IMPLIED c CDATA #IMPLIED d IDREF #IMPLIED>\
             <!ATTLIST p:item n NMTOKENS #IMPLIED e NOTATION (n1 | n2) #IMPLIED>",
        )
        .unwrap();
        let typed: Vec<(&str, &str)> = subset
            .typed
            .iter()
            .map(|(e, a)| (e.as_ref(), a.as_ref()))
            .collect();
        assert_eq!(
            typed,
            [
                ("doc", "a"),
                ("doc", "b"),
                ("doc", "d"),
                ("p:item", "n"),
                ("p:item", "e")
            ]
        );
    }

    #[test]
    fn skips_other_declarations() {
        let subset =
            scan_subset("<!ELEMENT doc (a|b)*><!NOTATION n SYSTEM 'n'><!-- ]> --><?p ]>?>")
                .unwrap();
        assert_eq!(subset, InternalSubset::default());
    }

    #[test]
    fn default_values() {
        let decl = r#"<!ATTLIST doc a CDATA "v">"#;
        assert_eq!(err_span(decl), (0, decl.len()));
        let decl = "<!ATTLIST doc a CDATA 'v'>";
        assert_eq!(err_span(decl), (0, decl.len()));
        let decl = "<!ATTLIST doc a CDATA #FIXED 'v'>";
        assert_eq!(err_span(decl), (0, decl.len()));
        let decl = r#"<!ATTLIST doc a ID #IMPLIED b CDATA "v>w">"#;
        assert_eq!(
            err_span(&format!("<!-- c -->{}", decl)),
            (10, 10 + decl.len())
        );
    }

    #[test]
    fn first_definition_counts() {
        let typed = |subset: &str| -> Vec<(String, String)> {
            scan_subset(subset)
                .unwrap()
                .typed
                .iter()
                .map(|(e, a)| (e.to_string(), a.to_string()))
                .collect()
        };
        let pair = |e: &str, a: &str| vec![(e.to_string(), a.to_string())];
        // a later definition of the same attribute does not make it typed
        assert!(typed("<!ATTLIST doc a CDATA #IMPLIED><!ATTLIST doc a ID #IMPLIED>").is_empty());
        assert!(typed("<!ATTLIST doc a CDATA #IMPLIED a NMTOKEN #IMPLIED>").is_empty());
        // nor does it make an attribute that the first declares typed untyped
        assert_eq!(
            typed("<!ATTLIST doc a ID #IMPLIED><!ATTLIST doc a CDATA #IMPLIED>"),
            pair("doc", "a")
        );
        // a repeated definition is noted once
        assert_eq!(
            typed("<!ATTLIST doc a ID #IMPLIED a ID #IMPLIED><!ATTLIST doc a ID #IMPLIED>"),
            pair("doc", "a")
        );
        // the same attribute name on another element is another attribute
        assert_eq!(
            typed("<!ATTLIST doc a CDATA #IMPLIED><!ATTLIST item a ID #IMPLIED>"),
            pair("item", "a")
        );
        // a later default is ignored
        assert!(typed(
            "<!ATTLIST doc a CDATA #IMPLIED><!ATTLIST doc a CDATA 'v' b CDATA #IMPLIED a ID #FIXED \"w\">"
        )
        .is_empty());
        // but a first one is not, after an ignored one
        let lead = "<!ATTLIST doc a ID #IMPLIED>";
        let decl = "<!ATTLIST doc a CDATA 'v' b CDATA 'w'>";
        assert_eq!(
            err_span(&format!("{}{}", lead, decl)),
            (lead.len(), lead.len() + decl.len())
        );
        // a later default that cannot be read as the tokenizer reads it
        for value in ["'v>w'", "'v<w'", "'&#38;'", "#FIXED", "#FIXEDv", "'v"] {
            let decl = format!("<!ATTLIST doc a CDATA {}>", value);
            let lead = "<!ATTLIST doc a CDATA #IMPLIED>";
            let span = err_span(&format!("{}{}", lead, decl));
            assert_eq!(span.0, lead.len(), "{}", decl);
        }
    }

    #[test]
    fn parameter_entity_references() {
        assert_eq!(err_span(" %p; "), (1, 4));
        assert_eq!(err_span(r#"<!ENTITY a "x%p;y">"#), (13, 16));
        assert_eq!(err_span("<!ELEMENT doc %p;>"), (14, 17));
        assert_eq!(err_span("<!ATTLIST doc %p;>"), (14, 17));
        assert_eq!(err_span("<!ATTLIST doc a %t; #IMPLIED>"), (16, 19));
        assert_eq!(err_span("<!ENTITY a %p;>"), (11, 14));
    }

    #[test]
    fn unreadable_attlist() {
        let decl = "<!ATTLIST doc a TEXT #IMPLIED>";
        assert_eq!(err_span(decl), (0, decl.len()));
        let decl = "<!ATTLIST doc a ID>";
        assert_eq!(err_span(decl), (0, decl.len()));
        let decl = "<!ATTLIST doc a (x|) #IMPLIED>";
        assert_eq!(err_span(decl), (0, decl.len()));
        let decl = "<!ATTLIST doc a ID #IMPLIEDb CDATA #IMPLIED>";
        assert_eq!(err_span(decl), (0, decl.len()));
        let decl = "<!ATTLIST doc a ID #DEFAULT>";
        assert_eq!(err_span(decl), (0, decl.len()));
    }

    #[test]
    fn stops_at_unknown() {
        assert!(scan_subset("<![INCLUDE[<!ATTLIST doc a CDATA 'v'>]]>").is_ok());
        assert!(scan_subset("text <!ATTLIST doc a CDATA 'v'>").is_ok());
        // unterminated: the tokenizer reports it
        let text = "<!DOCTYPE doc [<!ENTITY a \"x";
        assert!(scan(text, "<!DOCTYPE doc [".len(), text).is_ok());
    }
}
