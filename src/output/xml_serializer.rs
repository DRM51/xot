use std::borrow::Cow;
use std::io::Write;

use crate::entity::Escapes;
use crate::error::Error;
use crate::id::{NameId, PrefixId};
use crate::output::{self, Normalizer};
use crate::parse::is_allowed_declaration;
use crate::xmlname::NameStrInfo;
use crate::xotdata::{Node, Xot, XMLNS_NAMESPACE};
use crate::Value;

use super::escape::{self, first_not_xml_char};
use super::fullname::FullnameSerializer;
use super::{output_walk, Output, Pretty, TokenSerializeParameters};

/// Renders the outputs of a node as XML text, one token at a time, into a
/// string that the caller gives.
pub(crate) struct XmlRenderer<'a, N: Normalizer> {
    xot: &'a Xot,
    fullname_serializer: FullnameSerializer<'a>,
    normalizer: N,
    parameters: TokenSerializeParameters,
    // for each open element: whether its start tag pushed namespace
    // declarations, and the prefix its name was written with
    open: Vec<(bool, Option<PrefixId>)>,
}

impl<'a, N: Normalizer> XmlRenderer<'a, N> {
    pub(crate) fn new(
        xot: &'a Xot,
        node: Node,
        parameters: TokenSerializeParameters,
        normalizer: N,
    ) -> Self {
        let extra_declarations = xot.namespaces_in_scope(node).collect();
        let fullname_serializer = FullnameSerializer::new(xot, extra_declarations);
        Self {
            xot,
            fullname_serializer,
            normalizer,
            parameters,
            open: Vec::new(),
        }
    }

    /// Append the text of the token for `output` to `out`, without the space
    /// in front of it, and say whether a space goes in front of it.
    ///
    /// With `checks`, the token is first checked for what XML 1.0 and
    /// Namespaces in XML do not allow; nothing is appended when a check
    /// fails.
    pub(crate) fn token(
        &mut self,
        node: Node,
        output: &Output<'a>,
        out: &mut String,
        checks: bool,
    ) -> Result<bool, Error> {
        // where escaping of the text as it is in the tree may start
        let start = if checks {
            let start = self.check_characters(node, output)?;
            self.check_attribute_name(node, output)?;
            self.check_namespace_declaration(node, output)?;
            self.check_processing_instruction(output)?;
            Some(start)
        } else {
            None
        };
        use Output::*;
        match output {
            StartTagOpen(element) => {
                let declarations = self.xot.namespace_declarations(node);
                let pushed = !declarations.is_empty();
                self.fullname_serializer.push(declarations);
                let prefix = self.fullname_serializer.element_prefix(element.name())?;
                self.open.push((pushed, prefix));
                out.push('<');
                self.push_name(out, prefix, element.name());
                Ok(false)
            }
            StartTagClose => {
                out.push_str(if self.xot.first_child(node).is_none() {
                    "/>"
                } else {
                    ">"
                });
                Ok(false)
            }
            EndTag(element) => {
                let (pushed, prefix) = match self.open.pop() {
                    Some(open) => open,
                    // an end tag always follows its start tag in the walk
                    None => (
                        self.xot.has_namespace_declarations(node),
                        self.fullname_serializer.element_prefix(element.name())?,
                    ),
                };
                // the namespaces in scope are those of the start tag
                if self.xot.first_child(node).is_some() {
                    out.push_str("</");
                    self.push_name(out, prefix, element.name());
                    out.push('>');
                }
                self.fullname_serializer.pop(pushed);
                Ok(false)
            }
            Prefix(prefix_id, namespace_id) => {
                // we don't want to output the xml prefix
                if *namespace_id == self.xot.xml_namespace() {
                    return Ok(false);
                }
                if *prefix_id == self.xot.empty_prefix_id {
                    out.push_str("xmlns=\"");
                } else {
                    out.push_str("xmlns:");
                    out.push_str(self.xot.prefix_str(*prefix_id));
                    out.push_str("=\"");
                }
                // a namespace declaration is an attribute, so the namespace
                // is escaped like an attribute value; it is not normalized,
                // as that would change the namespace
                escape::push_attribute(
                    out,
                    self.xot.namespace_str(*namespace_id),
                    start.unwrap_or(0),
                );
                out.push('"');
                Ok(true)
            }
            Attribute(name_id, value) => {
                let prefix = self.fullname_serializer.attribute_prefix(*name_id)?;
                self.push_name(out, prefix, *name_id);
                out.push_str("=\"");
                let normalized = self.normalizer.normalize(Cow::Borrowed(*value));
                let start = escape_start(value, &normalized, start);
                escape::push_attribute(out, &normalized, start);
                out.push('"');
                Ok(true)
            }
            Text(text) => {
                // a text node can be a child of an element or document, or it
                // can have no parent at all
                let cdata_section_elements = &self.parameters.cdata_section_elements;
                let is_cdata_element = !cdata_section_elements.is_empty()
                    && self
                        .xot
                        .parent(node)
                        .and_then(|parent| self.xot.element(parent))
                        .is_some_and(|element| cdata_section_elements.contains(&element.name()));
                let normalized = self.normalizer.normalize(Cow::Borrowed(*text));
                let unescaped_cr = self.parameters.unescaped_cr;
                if is_cdata_element {
                    escape::push_cdata(out, &normalized, Escapes::RoundTrip { unescaped_cr });
                } else {
                    let start = escape_start(text, &normalized, start);
                    escape::push_text(
                        out,
                        &normalized,
                        start,
                        self.parameters.unescaped_gt,
                        unescaped_cr,
                    );
                }
                Ok(false)
            }
            Comment(text) => {
                out.push_str("<!--");
                out.push_str(text);
                out.push_str("-->");
                Ok(false)
            }
            ProcessingInstruction(target, data) => {
                let (target, ns) = self.xot.name_ns_str(*target);
                if !ns.is_empty() {
                    return Err(Error::NamespaceInProcessingInstruction);
                }
                out.push_str("<?");
                out.push_str(target);
                if let Some(data) = data {
                    out.push(' ');
                    out.push_str(data);
                }
                out.push_str("?>");
                Ok(false)
            }
        }
    }

    /// Append a name with its prefix, if any.
    #[inline]
    fn push_name(&self, out: &mut String, prefix: Option<PrefixId>, name_id: NameId) {
        if let Some(prefix) = prefix {
            out.push_str(self.xot.prefix_str(prefix));
            out.push(':');
        }
        out.push_str(self.xot.local_name_str(name_id));
    }

    /// Check that the output has only characters that XML 1.0 allows (the
    /// `Char` production). Serialization 3.1
    /// (<https://www.w3.org/TR/xslt-xquery-serialization-31/>, section 5.1.1)
    /// makes any other character an error (SERE0006): it cannot be written
    /// as it is, nor as a character reference.
    ///
    /// Gives the first index of a text, or of an attribute value or a
    /// namespace, that escaping needs to look at, found on the way.
    fn check_characters(&self, node: Node, output: &Output<'a>) -> Result<usize, Error> {
        let (value, kinds) = match output {
            Output::Text(text) => (*text, escape::TEXT),
            Output::Comment(text) => (*text, 0),
            Output::Attribute(_, value) => (*value, escape::ATTRIBUTE),
            Output::ProcessingInstruction(_, Some(data)) => (*data, 0),
            Output::Prefix(_, namespace_id) => {
                (self.xot.namespace_str(*namespace_id), escape::ATTRIBUTE)
            }
            _ => return Ok(0),
        };
        let c = match first_not_xml_char(value, kinds) {
            Ok(start) => return Ok(start),
            Err(c) => c,
        };
        let place = match output {
            Output::Attribute(name_id, _) => {
                let name = self
                    .fullname_serializer
                    .attribute_fullname(*name_id)
                    .unwrap_or(Cow::Borrowed(self.xot.local_name_str(*name_id)));
                format!("attribute \"{}\"", name)
            }
            Output::Prefix(prefix_id, _) if *prefix_id == self.xot.empty_prefix_id => {
                "namespace declaration \"xmlns\"".to_string()
            }
            Output::Prefix(prefix_id, _) => format!(
                "namespace declaration \"xmlns:{}\"",
                self.xot.prefix_str(*prefix_id)
            ),
            Output::Comment(_) => "comment".to_string(),
            Output::ProcessingInstruction(target, _) => format!(
                "processing instruction \"{}\"",
                self.xot.local_name_str(*target)
            ),
            _ => "text".to_string(),
        };
        Err(Error::InvalidCharacter(c, node, place))
    }

    /// Check that an attribute would not read back as a namespace
    /// declaration (https://www.w3.org/TR/xml-names/#ns-decl): the attribute
    /// `xmlns` in no namespace would declare the default namespace, and an
    /// attribute in the namespace of the `xmlns` prefix cannot be written
    /// without declaring a prefix for that namespace, which is not allowed.
    fn check_attribute_name(&self, node: Node, output: &Output<'a>) -> Result<(), Error> {
        if let Output::Attribute(name_id, _) = output {
            let (local_name, namespace) = self.xot.name_ns_str(*name_id);
            if namespace == XMLNS_NAMESPACE || (namespace.is_empty() && local_name == "xmlns") {
                return Err(Error::XmlnsAttribute(node, local_name.to_string()));
            }
        }
        Ok(())
    }

    /// Check that a namespace declaration is one that Namespaces in XML 1.0
    /// allows, as the parser does (https://www.w3.org/TR/xml-names/#xmlReserved
    /// and https://www.w3.org/TR/xml-names/#nsc-NoPrefixUndecl): a prefix
    /// cannot be undeclared, and the prefixes `xml` and `xmlns` and their
    /// namespaces are reserved. The prefix `xml` bound to its own namespace
    /// is allowed; it is not written.
    fn check_namespace_declaration(&self, node: Node, output: &Output<'a>) -> Result<(), Error> {
        if let Output::Prefix(prefix_id, namespace_id) = output {
            let prefix = self.xot.prefix_str(*prefix_id);
            let namespace = self.xot.namespace_str(*namespace_id);
            if !is_allowed_declaration(prefix, namespace) {
                let name = if prefix.is_empty() {
                    "xmlns".to_string()
                } else {
                    format!("xmlns:{}", prefix)
                };
                return Err(Error::InvalidNamespaceDeclaration(
                    node,
                    name,
                    namespace.to_string(),
                ));
            }
        }
        Ok(())
    }

    /// Check that a processing instruction does not have the target `xml`
    /// in any case, which XML 1.0 does not allow (production [17] PITarget,
    /// https://www.w3.org/TR/xml/#sec-pi), as the parser does. At the start
    /// of a document it would be read as an XML declaration.
    fn check_processing_instruction(&self, output: &Output<'a>) -> Result<(), Error> {
        if let Output::ProcessingInstruction(target, _) = output {
            let target = self.xot.local_name_str(*target);
            if target.eq_ignore_ascii_case("xml") {
                return Err(Error::InvalidTarget(target.to_string()));
            }
        }
        Ok(())
    }
}

/// Where escaping of `normalized` may start: the index that the check found
/// in `raw` when the normalizer gave `raw` back as it is (the same pointer
/// and length; a string the normalizer made cannot have the pointer of
/// `raw`, except when both are empty), else 0.
#[inline]
fn escape_start(raw: &str, normalized: &str, start: Option<usize>) -> usize {
    match start {
        Some(start) if normalized.as_ptr() == raw.as_ptr() && normalized.len() == raw.len() => {
            start
        }
        _ => 0,
    }
}

/// The capacity of the string that tokens are rendered into.
pub(crate) const SCRATCH_CAPACITY: usize = 256;

/// After a token that made the string grow beyond this capacity, the string
/// is replaced by a new one, so that one large token does not keep its
/// memory for the remainder of the serialization.
const SCRATCH_KEEP: usize = 1 << 20;

/// Replace `scratch` by a new string when it has grown beyond
/// [`SCRATCH_KEEP`].
#[inline]
pub(crate) fn reset_large_scratch(scratch: &mut String) {
    if scratch.capacity() > SCRATCH_KEEP {
        *scratch = String::with_capacity(SCRATCH_CAPACITY);
    }
}

const SPACES: [u8; 128] = [b' '; 128];

/// Write the indentation of `level`, two spaces a level, in one call.
#[inline]
pub(crate) fn write_indentation(w: &mut dyn Write, level: usize) -> std::io::Result<()> {
    let count = level * 2;
    if count <= SPACES.len() {
        w.write_all(&SPACES[..count])
    } else {
        w.write_all(" ".repeat(count).as_bytes())
    }
}

/// Write `node` as XML to `w`.
///
/// Each token is rendered into one string that is used again for the next,
/// and `w` gets the calls it got when each token was a new string: the
/// indentation, the space in front of the token, the token (also when it is
/// empty) and the newline, each in one `write_all`. When a token fails a
/// check, `w` has had everything before it, its indentation included, and
/// nothing of it.
pub(crate) fn write_xml<N: Normalizer>(
    xot: &Xot,
    parameters: output::xml::Parameters,
    node: Node,
    w: &mut dyn Write,
    normalizer: N,
) -> Result<(), Error> {
    if let Some(declaration) = parameters.declaration {
        declaration.serialize(w)?;
    }
    if let Some(doctype) = parameters.doctype {
        // if we are in a document node, we look for the document_element,
        // otherwise we take the current element, if possible
        let node = match xot.value(node) {
            Value::Document => xot.document_element(node)?,
            Value::Element(_) => node,
            _ => return Err(Error::NotElement(node)),
        };
        // now take the full name of the element; we can unwrap as we
        // know it's an element now
        let name = xot.node_name_ref(node)?.unwrap();
        let name = name.full_name();
        doctype.serialize(name.as_ref(), w)?;
    }
    let outputs = output_walk(xot, node);
    let mut renderer = XmlRenderer::new(
        xot,
        node,
        TokenSerializeParameters {
            cdata_section_elements: parameters.cdata_section_elements,
            unescaped_gt: parameters.unescaped_gt,
            unescaped_cr: parameters.unescaped_cr,
        },
        normalizer,
    );
    let mut scratch = String::with_capacity(SCRATCH_CAPACITY);
    if let Some(indentation) = parameters.indentation {
        let suppress = indentation.suppress;
        let mut pretty = Pretty::new(xot, |name| suppress.contains(&name), |_| false);
        for (node, output) in outputs {
            let (level, newline) = pretty.prettify(node, &output);
            if level > 0 {
                write_indentation(w, level)?;
            }
            scratch.clear();
            if renderer.token(node, &output, &mut scratch, true)? {
                w.write_all(b" ")?;
            }
            w.write_all(scratch.as_bytes())?;
            if newline {
                w.write_all(b"\n")?;
            }
            reset_large_scratch(&mut scratch);
        }
    } else {
        for (node, output) in outputs {
            scratch.clear();
            if renderer.token(node, &output, &mut scratch, true)? {
                w.write_all(b" ")?;
            }
            w.write_all(scratch.as_bytes())?;
            reset_large_scratch(&mut scratch);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reset_large_scratch() {
        let mut scratch = String::with_capacity(SCRATCH_CAPACITY);
        scratch.push_str("abc");
        reset_large_scratch(&mut scratch);
        assert_eq!(scratch, "abc");

        let mut scratch = String::with_capacity(SCRATCH_KEEP);
        scratch.push_str("abc");
        reset_large_scratch(&mut scratch);
        assert_eq!(scratch, "abc");
        assert!(scratch.capacity() >= SCRATCH_KEEP);

        let mut scratch = "x".repeat(SCRATCH_KEEP + 1);
        reset_large_scratch(&mut scratch);
        assert!(scratch.is_empty());
        assert_eq!(scratch.capacity(), SCRATCH_CAPACITY);
    }

    #[test]
    fn test_write_indentation() {
        for level in [0, 1, 63, 64, 65, 70] {
            let mut w = Vec::new();
            write_indentation(&mut w, level).unwrap();
            assert_eq!(w, " ".repeat(level * 2).as_bytes());
        }
    }

    #[test]
    fn test_escape_start() {
        let raw = "a&b";
        assert_eq!(escape_start(raw, raw, Some(1)), 1);
        assert_eq!(escape_start(raw, raw, None), 0);
        assert_eq!(escape_start(raw, &raw[1..], Some(1)), 0);
        assert_eq!(escape_start(raw, &raw[..2], Some(1)), 0);
        let copy = raw.to_string();
        assert_eq!(escape_start(raw, &copy, Some(1)), 0);
    }
}
