use crate::id::{NameId, NamespaceId, PrefixId};
use crate::xmlvalue::Element;

/// Output of serialization
///
/// Given an [`OutputToken`] or
/// [`PrettyOutputToken`](`crate::output::PrettyOutputToken`), this enum
/// represents what the token represents in the XML tree.
///
/// You can use this information for customized serialization.
#[derive(Debug, PartialEq)]
pub enum Output<'a> {
    /// Start tag open, i.e `<foo` or `<ns:foo`
    StartTagOpen(Element),
    /// Start tag close, either `>` or `/>`
    StartTagClose,
    /// End tag, i.e. `</foo>` or `</ns:foo>`
    EndTag(Element),
    /// Namespace declaration, i.e. `xmlns:foo="http://example.com"`
    Prefix(PrefixId, NamespaceId),
    /// Attribute, i.e. `foo="bar"`
    Attribute(NameId, &'a str),
    /// Text, i.e. `foo`
    Text(&'a str),
    /// Comment, i.e. `<!-- foo -->`
    Comment(&'a str),
    /// Processing instruction, i.e. `<?foo bar?>`
    ProcessingInstruction(NameId, Option<&'a str>),
}

/// Output token
///
/// This represents an [`Output`] as a rendered output token.
pub struct OutputToken {
    /// Whether the token is prefixed by a space character.
    pub space: bool,
    /// The token.
    ///
    /// This is a fragment of XML like `<foo` or `a="A"` or `/>`, etc.
    pub text: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xotdata::Xot;

    #[test]
    fn test_iter_mkgen() {
        let mut xot = Xot::new();
        let root = xot.parse(r#"<doc a="A">Text</doc>"#).unwrap();
        let a_id = xot.add_name("a");
        let doc = xot.document_element(root).unwrap();
        let doc_el = xot.element(doc).unwrap();
        let mut iter = xot.outputs(doc);

        let v = iter.next().unwrap().1;
        assert_eq!(v, Output::StartTagOpen(*doc_el));
        let v = iter.next().unwrap().1;
        assert_eq!(v, Output::Prefix(xot.xml_prefix(), xot.xml_namespace()));
        let v = iter.next().unwrap().1;
        assert_eq!(v, Output::Attribute(a_id, "A"));
        let v = iter.next().unwrap().1;
        assert_eq!(v, Output::StartTagClose);
        let v = iter.next().unwrap().1;
        assert_eq!(v, Output::Text("Text"));
        let v = iter.next().unwrap().1;
        assert_eq!(v, Output::EndTag(*doc_el));
        assert!(iter.next().is_none());
    }
}
