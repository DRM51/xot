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
//! The scan is one pass over the bytes of the internal subset, without
//! recursion; it keeps only the names that it notes.
//!
//! The tokenizer reads the internal subset after the scan, and reports what
//! is not well-formed. The scan stops without an error at anything it does
//! not know, such as a conditional section, and leaves it to the tokenizer:
//! the tokenizer rejects these in an internal subset. It ends an `ELEMENT` or
//! a `NOTATION` declaration at the first `>`, as the tokenizer does, so that
//! both see the same declarations.

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
pub(crate) fn scan(text: &str, start: usize) -> Result<InternalSubset, ParseError> {
    let mut scanner = Scanner {
        bytes: text.as_bytes(),
        pos: start,
    };
    let mut subset = InternalSubset::default();
    scanner.subset(&mut subset)?;
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

struct Scanner<'t> {
    bytes: &'t [u8],
    pos: usize,
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
    fn subset(&mut self, subset: &mut InternalSubset) -> Result<(), ParseError> {
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
                self.attlist(subset)?;
            } else if self.starts_with(b"<!ELEMENT") || self.starts_with(b"<!NOTATION") {
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
    /// `DtdUnsupported`.
    fn attlist(&mut self, subset: &mut InternalSubset) -> Result<(), ParseError> {
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
            // DefaultDecl ::= '#REQUIRED' | '#IMPLIED' | (('#FIXED' S)? AttValue)
            if self.starts_with(b"#REQUIRED") {
                self.pos += b"#REQUIRED".len();
            } else if self.starts_with(b"#IMPLIED") {
                self.pos += b"#IMPLIED".len();
            } else if self.starts_with(b"#FIXED") || matches!(self.peek(), Some(b'"' | b'\'')) {
                // an attribute default, which the parser would have to add
                return Err(unreadable(self));
            } else {
                return Err(self.reference_or(unreadable(self)));
            }
            if typed {
                if let (Ok(element), Ok(attribute)) =
                    (std::str::from_utf8(element), std::str::from_utf8(attribute))
                {
                    subset.typed.push((element.into(), attribute.into()));
                }
            }
        }
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
        scan(&text, "<!DOCTYPE doc [".len())
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
        assert!(scan(text, "<!DOCTYPE doc [".len()).is_ok());
    }
}
