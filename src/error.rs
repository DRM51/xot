use crate::{xotdata::Node, Span};

/// An error that occurred during parsing.
///
/// Each error has a span or a position, which [`ParseError::span`] gives.
/// These are byte offsets in the text that is parsed: the string given to
/// [`Xot::parse`](crate::Xot::parse) or another function that parses a
/// string, or for [`Xot::parse_bytes`](crate::Xot::parse_bytes) the text
/// decoded from the bytes, which leaves out a byte order mark.
///
/// The errors that `parse_bytes` reports while it decodes the bytes, before
/// it parses, are the exception: the spans of
/// [`ParseError::UnsupportedEncoding`], [`ParseError::EncodingConflict`] and
/// [`ParseError::InvalidByteSequence`] are byte offsets in the bytes given to
/// `parse_bytes`, byte order mark included.
#[derive(Debug, Clone)]
pub enum ParseError {
    /// The XML is not well-formed - a tag is opened and never closed.
    ///
    /// The span is the name in the start tag, in the parsed text.
    UnclosedTag(Span),
    /// The XML is not well-formed - a tag is closed that was never opened.
    ///
    /// This holds the prefix and the local name in the end tag, and the span
    /// of the name in the parsed text.
    InvalidCloseTag(String, String, Span),
    /// The XML is not well-formed - you use `&` to open an entity without
    /// closing it with `;`.
    ///
    /// This holds what follows the `&`, and the position of the `&` in the
    /// parsed text.
    UnclosedEntity(String, usize),
    /// The entity is not known. Only the basic entities are supported
    /// right now, not any user defined ones.
    ///
    /// This holds what is between `&` and `;`, and the span of the reference,
    /// from `&` to `;`, in the parsed text.
    ///
    /// This is also the error for a character reference that is not
    /// well-formed, such as `&#xG;` or `&#+65;`, or that refers to a
    /// character that XML 1.0 does not allow, such as `&#x1;` (XML 1.0 section
    /// 4.1, "WFC: Legal Character"). The string is then what is between `&#`
    /// and `;`, such as `x1`.
    InvalidEntity(String, Span),
    /// You used a namespace prefix that is not declared during parsing.
    ///
    /// This holds the prefix and its span in the parsed text.
    UnknownPrefix(String, Span),
    /// You declared an attribute of the same name twice.
    ///
    /// This holds the name and the span of its second occurrence in the
    /// parsed text.
    DuplicateAttribute(String, Span),
    /// Unsupported XML version. Only 1.0 is supported.
    ///
    /// This holds the version and its span in the parsed text.
    UnsupportedVersion(String, Span),
    /// Unsupported encoding, reported by [`Xot::parse_bytes`](crate::Xot::parse_bytes):
    /// the encoding declaration names an encoding that is not known, or the
    /// document is in an encoding that cannot be decoded, such as UTF-32.
    ///
    /// This holds the name in the encoding declaration and its span, or the
    /// encoding that the first bytes show, such as `UTF-32LE`, and an empty
    /// span at 0. Unlike the spans of most errors, this span is a range of
    /// byte offsets in the bytes given to `parse_bytes`, byte order mark
    /// included.
    UnsupportedEncoding(String, Span),
    /// The encoding declaration does not match the encoding that the first
    /// bytes show, reported by [`Xot::parse_bytes`](crate::Xot::parse_bytes).
    /// XML 1.0 (section 4.3.3 and appendix F) makes this a fatal error.
    ///
    /// With a byte order mark, the declaration has to name the encoding of
    /// the byte order mark: UTF-8 for a UTF-8 one, and for a UTF-16 one
    /// UTF-16, another name of UTF-16 such as UCS-2, or the UTF-16LE or
    /// UTF-16BE of its byte order. Without a byte order mark, a document
    /// whose first characters are single bytes cannot declare UTF-16, and a
    /// document that starts with `<?` in UTF-16 has to declare UTF-16 in that
    /// byte order.
    ///
    /// This holds the name in the encoding declaration, which is empty if
    /// there is none; what the first bytes show, such as
    /// `UTF-8 byte order mark`, `UTF-16LE without byte order mark` or
    /// `ASCII-compatible encoding`; and the span of the name in the
    /// declaration, or an empty span at 0 if there is none. Unlike the spans
    /// of most errors, this span is a range of byte offsets in the bytes given
    /// to `parse_bytes`, byte order mark included.
    EncodingConflict(String, String, Span),
    /// A byte sequence that is not valid in the encoding of the document,
    /// reported by [`Xot::parse_bytes`](crate::Xot::parse_bytes). XML 1.0
    /// (section 4.3.3) makes this a fatal error.
    ///
    /// This holds the name of the encoding, such as `UTF-8`, and the span of
    /// the byte sequence. Unlike the spans of most errors, this span is a
    /// range of byte offsets in the bytes given to `parse_bytes`, byte order
    /// mark included: there is no decoded text to point into.
    InvalidByteSequence(String, Span),
    /// Unsupported standalone declaration. This error is deprecated since version 0.29, and both
    /// "yes" and "no" values are accepted for the standalone declaration.
    #[deprecated(
        since = "0.2.9",
        note = "The value of the standalone declaration is now ignored"
    )]
    UnsupportedNotStandalone(Span),
    /// XML DTD is not supported.
    ///
    /// The span is the part of the DTD that is found, in the parsed text.
    DtdUnsupported(Span),
    /// No top-level element in the document.
    ///
    /// The position is the end of the parsed text.
    NoElementAtTopLevel(usize),
    /// Multiple top-level elements in the document.
    ///
    /// The span is the name in the start tag of the second element, in the
    /// parsed text.
    MultipleElementsAtTopLevel(Span),
    /// Text at top level is not allowed in a well-formed document.
    ///
    /// The span is the text, in the parsed text.
    TextAtTopLevel(Span),
    /// Duplicate xml:id is not allowed
    ///
    /// This holds the value, and the span of the second `xml:id` attribute
    /// value with it in the parsed text.
    DuplicateId(String, Span),
    /// xmlparser error
    ///
    /// This holds the error, and the position in the parsed text where the
    /// part that xmlparser could not read starts.
    XmlParser(xmlparser::Error, usize),
}

impl ParseError {
    /// Obtain the span for a ParseError.
    ///
    /// For an error that has a position, this is an empty span at that
    /// position. The span is a range of byte offsets in the parsed text,
    /// except for the errors that [`Xot::parse_bytes`](crate::Xot::parse_bytes)
    /// reports while it decodes the bytes, which are byte offsets in its
    /// input, byte order mark included. See [`ParseError`].
    pub fn span(&self) -> Span {
        match self {
            ParseError::UnclosedTag(span) => *span,
            ParseError::InvalidCloseTag(_, _, span) => *span,
            ParseError::UnclosedEntity(_, position) => Span::new(*position, *position),
            ParseError::InvalidEntity(_, span) => *span,
            ParseError::UnknownPrefix(_, span) => *span,
            ParseError::DuplicateAttribute(_, span) => *span,
            ParseError::UnsupportedVersion(_, span) => *span,
            ParseError::UnsupportedEncoding(_, span) => *span,
            ParseError::EncodingConflict(_, _, span) => *span,
            ParseError::InvalidByteSequence(_, span) => *span,
            #[allow(deprecated)]
            ParseError::UnsupportedNotStandalone(span) => *span,
            ParseError::DtdUnsupported(span) => *span,
            ParseError::NoElementAtTopLevel(position) => Span::new(*position, *position),
            ParseError::MultipleElementsAtTopLevel(span) => *span,
            ParseError::TextAtTopLevel(span) => *span,
            ParseError::DuplicateId(_, span) => *span,
            ParseError::XmlParser(_, position) => Span::new(*position, *position),
        }
    }
}

/// Xot errors
#[derive(Debug, Clone)]
pub enum Error {
    // access errors
    /// The node is not a Document node.
    NotDocument(Node),

    // manipulation errors
    /// Invalid operation on XML. You get this when
    /// trying to remove the document, or trying to
    /// insert something under a text node, for instance.
    InvalidOperation(String),

    /// You aren't allowed to use this string as a comment.
    /// Happens if you include `--` in a comment.
    InvalidComment(String),
    /// You aren't allowed to use this string as a processing instruction
    /// target. Happens if you use `XML` or any case variation of this.
    InvalidTarget(String),
    /// The node you tried to act on is not an element.
    NotElement(Node),
    /// Indextree error that can happen during manipulation.
    NodeError(indextree::NodeError),

    // serializer
    /// Missing prefix for namespace.
    /// Can occur during serialization if a namespace is used that has no
    /// prefix is declared. Use [`Xot::create_missing_prefixes`](crate::xotdata::Xot::create_missing_prefixes)
    /// to fix this.
    MissingPrefix(String),
    /// It's not allowed to serialize a processing instruction to HTML with a > in it.
    ProcessingInstructionGtInHtml(String),
    /// A character that XML 1.0 does not allow would be in XML output.
    ///
    /// These are the characters that the `Char` production of XML 1.0 leaves
    /// out: the control characters #x1 to #x1F other than tab, newline and
    /// carriage return, and #x0, #xFFFE and #xFFFF. They are not allowed even
    /// as character references, so output that contains them would not be
    /// well-formed. This is serialization error SERE0006 of
    /// [Serialization 3.1](https://www.w3.org/TR/xslt-xquery-serialization-31/),
    /// section 5.1.1.
    ///
    /// This holds the character, the node where it is found and a
    /// description of where it is, such as `attribute "a"`. The node is the
    /// text, comment or processing instruction node, or for an attribute
    /// value or a namespace declaration the element it is written on.
    InvalidCharacter(char, Node, String),

    /// It's not allowed to include a namespace prefix in a processing instruction
    /// target name.
    NamespaceInProcessingInstruction,

    /// An error during parsing
    Parse(ParseError),

    /// You used a namespace prefix that is not declared.
    ///
    /// Note that this error does not occur during parsing but during
    /// name creation.
    UnknownPrefix(String),

    /// Illegal content that can never appear under a document node, such as an
    /// attribute or a namespace node
    IllegalAtTopLevel(Node),
    /// A text node at top level is not allowed in a well formed document,
    /// but it is accepted as a fragment.
    TextAtTopLevel(Node),
    /// Missing document element at top level
    NoElementAtTopLevel,
    /// Multiple document elements at top level
    MultipleElementsAtTopLevel,

    /// IO error
    ///
    /// We take the string version of the IO error so as to keep errors comparable,
    /// which is more important than the exact error object in this case (serialization)
    Io(String),
}

impl From<indextree::NodeError> for Error {
    #[inline]
    fn from(e: indextree::NodeError) -> Self {
        Error::NodeError(e)
    }
}

impl From<std::io::Error> for Error {
    #[inline]
    fn from(e: std::io::Error) -> Self {
        Error::Io(e.to_string())
    }
}

impl From<ParseError> for Error {
    #[inline]
    fn from(e: ParseError) -> Self {
        Error::Parse(e)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::NotDocument(_) => write!(f, "Not a document node"),
            Error::InvalidOperation(s) => write!(f, "Invalid operation: {}", s),
            Error::InvalidComment(s) => write!(f, "Invalid comment: {}", s),
            Error::InvalidTarget(s) => write!(f, "Invalid target: {}", s),
            Error::NotElement(_) => write!(f, "Not an element"),
            Error::NodeError(e) => write!(f, "Node error: {}", e),
            Error::MissingPrefix(_) => write!(f, "Missing prefix"),
            Error::ProcessingInstructionGtInHtml(s) => {
                write!(f, "Processing instruction with > in HTML: {}", s)
            }
            Error::InvalidCharacter(c, _, place) => write!(
                f,
                "Character U+{:04X} is not allowed in XML 1.0, found in {}",
                u32::from(*c),
                place
            ),
            Error::NamespaceInProcessingInstruction => {
                write!(f, "Namespace in processing instruction target")
            }
            Error::Parse(e) => write!(f, "Parse error: {:?}", e),
            Error::UnknownPrefix(s) => write!(f, "Unknown prefix: {}", s),
            Error::IllegalAtTopLevel(_) => write!(f, "Illegal content under document node (attribute, namespace or document node"),
            Error::TextAtTopLevel(_) => write!(f, "Text node under document not. Not allowed in a well-formed document, but allowed in a fragment"),
            Error::NoElementAtTopLevel => write!(f, "No element under document root. Not allowed in a well-formed document, but allowed in a fragment"),
            Error::MultipleElementsAtTopLevel => write!(f, "Multiple elements under document root. Not allowed in a well-formed document, but allowed in a fragment"),
            Error::Io(s) => write!(f, "IO error: {}", s),
        }
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            ParseError::UnclosedTag(_) => write!(f, "Unclosed tag"),
            ParseError::InvalidCloseTag(s, s2, _) => write!(f, "Invalid close tag: {} {}", s, s2),
            ParseError::UnclosedEntity(s, _) => write!(f, "Unclosed entity: {}", s),
            ParseError::InvalidEntity(s, _) => write!(f, "Invalid entity: {}", s),
            ParseError::UnknownPrefix(s, _) => write!(f, "Unknown prefix: {}", s),
            ParseError::DuplicateAttribute(s, _) => write!(f, "Duplicate attribute: {}", s),
            ParseError::UnsupportedVersion(s, _) => write!(f, "Unsupported version: {}", s),
            ParseError::UnsupportedEncoding(s, _) => write!(f, "Unsupported encoding: {}", s),
            ParseError::EncodingConflict(declared, found, _) if declared.is_empty() => write!(
                f,
                "Encoding conflict: no encoding declaration, found {}",
                found
            ),
            ParseError::EncodingConflict(declared, found, _) => write!(
                f,
                "Encoding conflict: declared {}, found {}",
                declared, found
            ),
            ParseError::InvalidByteSequence(s, span) => write!(
                f,
                "Invalid byte sequence for encoding {} at byte {}",
                s, span.start
            ),
            #[allow(deprecated)]
            ParseError::UnsupportedNotStandalone(_) => write!(f, "Unsupported standalone"),
            ParseError::DtdUnsupported(_) => write!(f, "DTD is not supported"),
            ParseError::NoElementAtTopLevel(_) => write!(f, "No element at top level"),
            ParseError::MultipleElementsAtTopLevel(_) => {
                write!(f, "Multiple elements at top level")
            }
            ParseError::TextAtTopLevel(_) => write!(f, "Text at top level"),
            ParseError::DuplicateId(s, _) => write!(f, "Duplicate xml:id: {}", s),
            ParseError::XmlParser(e, _position) => write!(f, "Parser error: {}", e),
        }
    }
}

impl std::error::Error for ParseError {
    fn description(&self) -> &str {
        "XML parsing error"
    }
}

impl std::error::Error for Error {
    fn description(&self) -> &str {
        "Xot error"
    }
}
