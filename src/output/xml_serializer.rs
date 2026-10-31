use std::borrow::Cow;
use std::io::Write;

use crate::entity::Escapes;
use crate::error::Error;
use crate::id::{NameId, NamespaceId, PrefixId};
use crate::output::{self, Normalizer};
use crate::parse::is_allowed_declaration;
use crate::xmlname::NameStrInfo;
use crate::xotdata::{Node, Xot, XMLNS_NAMESPACE};
use crate::Value;

use super::escape::{self, first_not_xml_char};
use super::{output_walk, NameScope, Output, Pretty, TokenSerializeParameters};

/// Renders XML tokens, one at a time, into a string that the caller gives.
///
/// It keeps what lasts from one token to the next (the namespaces in scope,
/// the open elements, the normalizer and the parameters) and no borrow of the
/// [`Xot`]: every method takes the `Xot`, and the facts of its token that the
/// tree would otherwise give (the declarations of a start tag, whether an
/// element has a child, whether a text is in a CDATA section element, the node
/// that an error names). [`XmlRenderer`] reads these facts from the tree;
/// [`Writer`](output::xml::Writer) gets them from its caller.
pub(crate) struct TokenRenderer<N: Normalizer> {
    scope: NameScope,
    normalizer: N,
    parameters: TokenSerializeParameters,
    // for each open element: whether its start tag pushed namespace
    // declarations, and the prefix its name was written with
    open: Vec<(bool, Option<PrefixId>)>,
}

impl<N: Normalizer> TokenRenderer<N> {
    pub(crate) fn new(
        scope: NameScope,
        parameters: TokenSerializeParameters,
        normalizer: N,
    ) -> Self {
        Self {
            scope,
            normalizer,
            parameters,
            open: Vec::new(),
        }
    }

    /// Check `output` for what XML 1.0 and Namespaces in XML do not allow,
    /// naming `node` in an error.
    ///
    /// Gives the first index of a text, or of an attribute value or a
    /// namespace, that escaping needs to look at, found on the way.
    #[inline(always)]
    pub(crate) fn check(&self, xot: &Xot, node: Node, output: &Output<'_>) -> Result<usize, Error> {
        let start = self.check_characters(xot, node, output)?;
        check_attribute_name(xot, node, output)?;
        check_namespace_declaration(xot, node, output)?;
        check_processing_instruction(xot, output)?;
        Ok(start)
    }

    /// The start of a start tag: `<` and the name, written with the prefix
    /// that the namespaces in scope give once `declarations`, the namespace
    /// declarations of the element, are added to them.
    #[inline(always)]
    pub(crate) fn start_tag_open(
        &mut self,
        xot: &Xot,
        name: NameId,
        declarations: impl Iterator<Item = (PrefixId, NamespaceId)>,
        out: &mut String,
    ) -> Result<bool, Error> {
        let pushed = self.scope.push(declarations);
        let prefix = self.scope.element_prefix(xot, name)?;
        self.open.push((pushed, prefix));
        out.push('<');
        push_name(xot, out, prefix, name);
        Ok(false)
    }

    /// The end of a start tag: `>` for an element with a child, `/>` for one
    /// without.
    #[inline(always)]
    pub(crate) fn start_tag_close(&self, has_child: bool, out: &mut String) -> bool {
        out.push_str(if has_child { ">" } else { "/>" });
        false
    }

    /// The end tag of the innermost open element, if it has a child.
    ///
    /// `unopened` is called only when no element is open, which only the
    /// walk of a tree that goes on after the node it serializes gives: it
    /// says whether the element has namespace declarations.
    #[inline(always)]
    pub(crate) fn end_tag(
        &mut self,
        xot: &Xot,
        name: NameId,
        has_child: bool,
        unopened: impl FnOnce() -> bool,
        out: &mut String,
    ) -> Result<bool, Error> {
        let (pushed, prefix) = match self.open.pop() {
            Some(open) => open,
            // an end tag always follows its start tag in the walk
            None => (unopened(), self.scope.element_prefix(xot, name)?),
        };
        // the namespaces in scope are those of the start tag
        if has_child {
            out.push_str("</");
            push_name(xot, out, prefix, name);
            out.push('>');
        }
        if pushed {
            self.scope.pop();
        }
        Ok(false)
    }

    /// A namespace declaration. Nothing for a declaration of the namespace
    /// of the `xml` prefix.
    #[inline(always)]
    pub(crate) fn prefix(
        &self,
        xot: &Xot,
        prefix_id: PrefixId,
        namespace_id: NamespaceId,
        start: Option<usize>,
        out: &mut String,
    ) -> bool {
        // we don't want to output the xml prefix
        if namespace_id == xot.xml_namespace() {
            return false;
        }
        if prefix_id == xot.empty_prefix_id {
            out.push_str("xmlns=\"");
        } else {
            out.push_str("xmlns:");
            out.push_str(xot.prefix_str(prefix_id));
            out.push_str("=\"");
        }
        // a namespace declaration is an attribute, so the namespace
        // is escaped like an attribute value; it is not normalized,
        // as that would change the namespace
        escape::push_attribute(out, xot.namespace_str(namespace_id), start.unwrap_or(0));
        out.push('"');
        true
    }

    /// An attribute.
    #[inline(always)]
    pub(crate) fn attribute(
        &self,
        xot: &Xot,
        name_id: NameId,
        value: &str,
        start: Option<usize>,
        out: &mut String,
    ) -> Result<bool, Error> {
        let prefix = self.scope.attribute_prefix(xot, name_id)?;
        push_name(xot, out, prefix, name_id);
        out.push_str("=\"");
        let normalized = self.normalizer.normalize(Cow::Borrowed(value));
        let start = escape_start(value, &normalized, start);
        escape::push_attribute(out, &normalized, start);
        out.push('"');
        Ok(true)
    }

    /// Whether text in an element named `name` is written as CDATA
    /// sections.
    #[inline(always)]
    pub(crate) fn is_cdata_section_element(&self, name: NameId) -> bool {
        self.parameters.cdata_section_elements.contains(&name)
    }

    /// Whether any element is a CDATA section element.
    #[inline(always)]
    pub(crate) fn has_cdata_section_elements(&self) -> bool {
        !self.parameters.cdata_section_elements.is_empty()
    }

    /// A text, as CDATA sections when `cdata`.
    #[inline(always)]
    pub(crate) fn text(
        &self,
        text: &str,
        cdata: bool,
        start: Option<usize>,
        out: &mut String,
    ) -> bool {
        let normalized = self.normalizer.normalize(Cow::Borrowed(text));
        let unescaped_cr = self.parameters.unescaped_cr;
        if cdata {
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
        false
    }

    /// A comment.
    #[inline(always)]
    pub(crate) fn comment(&self, text: &str, out: &mut String) -> bool {
        out.push_str("<!--");
        out.push_str(text);
        out.push_str("-->");
        false
    }

    /// A processing instruction.
    #[inline(always)]
    pub(crate) fn processing_instruction(
        &self,
        xot: &Xot,
        target: NameId,
        data: Option<&str>,
        out: &mut String,
    ) -> Result<bool, Error> {
        let (target, ns) = xot.name_ns_str(target);
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

    /// Check that the output has only characters that XML 1.0 allows (the
    /// `Char` production). Serialization 3.1
    /// (<https://www.w3.org/TR/xslt-xquery-serialization-31/>, section 5.1.1)
    /// makes any other character an error (SERE0006): it cannot be written
    /// as it is, nor as a character reference.
    ///
    /// Gives the first index of a text, or of an attribute value or a
    /// namespace, that escaping needs to look at, found on the way.
    #[inline(always)]
    fn check_characters(&self, xot: &Xot, node: Node, output: &Output<'_>) -> Result<usize, Error> {
        let (value, kinds) = match output {
            Output::Text(text) => (*text, escape::TEXT),
            Output::Comment(text) => (*text, 0),
            Output::Attribute(_, value) => (*value, escape::ATTRIBUTE),
            Output::ProcessingInstruction(_, Some(data)) => (*data, 0),
            Output::Prefix(_, namespace_id) => {
                (xot.namespace_str(*namespace_id), escape::ATTRIBUTE)
            }
            _ => return Ok(0),
        };
        match first_not_xml_char(value, kinds) {
            Ok(start) => Ok(start),
            Err(c) => Err(self.invalid_character(xot, node, output, c)),
        }
    }

    /// The error for the character `c` of `output`, which XML 1.0 does not
    /// allow, naming the place it is in.
    #[cold]
    #[inline(never)]
    fn invalid_character(&self, xot: &Xot, node: Node, output: &Output<'_>, c: char) -> Error {
        let place = match output {
            Output::Attribute(name_id, _) => {
                let local_name = xot.local_name_str(*name_id);
                match self.scope.attribute_prefix(xot, *name_id) {
                    Ok(Some(prefix)) => {
                        format!("attribute \"{}:{}\"", xot.prefix_str(prefix), local_name)
                    }
                    _ => format!("attribute \"{}\"", local_name),
                }
            }
            Output::Prefix(prefix_id, _) if *prefix_id == xot.empty_prefix_id => {
                "namespace declaration \"xmlns\"".to_string()
            }
            Output::Prefix(prefix_id, _) => format!(
                "namespace declaration \"xmlns:{}\"",
                xot.prefix_str(*prefix_id)
            ),
            Output::Comment(_) => "comment".to_string(),
            Output::ProcessingInstruction(target, _) => {
                format!("processing instruction \"{}\"", xot.local_name_str(*target))
            }
            _ => "text".to_string(),
        };
        Error::InvalidCharacter(c, node, place)
    }
}

/// Append a name with its prefix, if any.
#[inline(always)]
fn push_name(xot: &Xot, out: &mut String, prefix: Option<PrefixId>, name_id: NameId) {
    if let Some(prefix) = prefix {
        out.push_str(xot.prefix_str(prefix));
        out.push(':');
    }
    out.push_str(xot.local_name_str(name_id));
}

/// Check that an attribute would not read back as a namespace
/// declaration (https://www.w3.org/TR/xml-names/#ns-decl): the attribute
/// `xmlns` in no namespace would declare the default namespace, and an
/// attribute in the namespace of the `xmlns` prefix cannot be written
/// without declaring a prefix for that namespace, which is not allowed.
#[inline(always)]
fn check_attribute_name(xot: &Xot, node: Node, output: &Output<'_>) -> Result<(), Error> {
    if let Output::Attribute(name_id, _) = output {
        let (local_name, namespace) = xot.name_ns_str(*name_id);
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
#[inline(always)]
fn check_namespace_declaration(xot: &Xot, node: Node, output: &Output<'_>) -> Result<(), Error> {
    if let Output::Prefix(prefix_id, namespace_id) = output {
        let prefix = xot.prefix_str(*prefix_id);
        let namespace = xot.namespace_str(*namespace_id);
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
#[inline(always)]
fn check_processing_instruction(xot: &Xot, output: &Output<'_>) -> Result<(), Error> {
    if let Output::ProcessingInstruction(target, _) = output {
        let target = xot.local_name_str(*target);
        if target.eq_ignore_ascii_case("xml") {
            return Err(Error::InvalidTarget(target.to_string()));
        }
    }
    Ok(())
}

/// Renders the outputs of a node as XML text, one token at a time, into a
/// string that the caller gives.
pub(crate) struct XmlRenderer<'a, N: Normalizer> {
    xot: &'a Xot,
    tokens: TokenRenderer<N>,
}

impl<'a, N: Normalizer> XmlRenderer<'a, N> {
    pub(crate) fn new(
        xot: &'a Xot,
        node: Node,
        parameters: TokenSerializeParameters,
        normalizer: N,
    ) -> Self {
        let base = xot.namespaces_in_scope(node).collect::<Vec<_>>();
        let scope = NameScope::new(xot, &base);
        Self {
            xot,
            tokens: TokenRenderer::new(scope, parameters, normalizer),
        }
    }

    /// Append the text of the token for `output` to `out`, without the space
    /// in front of it, and say whether a space goes in front of it.
    ///
    /// With `checks`, the token is first checked for what XML 1.0 and
    /// Namespaces in XML do not allow; nothing is appended when a check
    /// fails.
    ///
    /// The facts of the token that are not in `output` are read from the
    /// tree: the declarations of `node` for a start tag, whether it has a
    /// child for the end of a start tag and for an end tag, and whether the
    /// parent of a text is a CDATA section element.
    pub(crate) fn token(
        &mut self,
        node: Node,
        output: &Output<'a>,
        out: &mut String,
        checks: bool,
    ) -> Result<bool, Error> {
        let xot = self.xot;
        // where escaping of the text as it is in the tree may start
        let start = if checks {
            Some(self.tokens.check(xot, node, output)?)
        } else {
            None
        };
        use Output::*;
        match output {
            StartTagOpen(element) => self.tokens.start_tag_open(
                xot,
                element.name(),
                xot.namespaces(node)
                    .iter()
                    .map(|(prefix, namespace)| (prefix, *namespace)),
                out,
            ),
            StartTagClose => Ok(self
                .tokens
                .start_tag_close(xot.first_child(node).is_some(), out)),
            EndTag(element) => self.tokens.end_tag(
                xot,
                element.name(),
                xot.first_child(node).is_some(),
                || xot.has_namespace_declarations(node),
                out,
            ),
            Prefix(prefix_id, namespace_id) => {
                Ok(self
                    .tokens
                    .prefix(xot, *prefix_id, *namespace_id, start, out))
            }
            Attribute(name_id, value) => self.tokens.attribute(xot, *name_id, value, start, out),
            Text(text) => {
                // a text node can be a child of an element or document, or it
                // can have no parent at all
                let is_cdata_element = self.tokens.has_cdata_section_elements()
                    && xot
                        .parent(node)
                        .and_then(|parent| xot.element(parent))
                        .is_some_and(|element| {
                            self.tokens.is_cdata_section_element(element.name())
                        });
                Ok(self.tokens.text(text, is_cdata_element, start, out))
            }
            Comment(text) => Ok(self.tokens.comment(text, out)),
            ProcessingInstruction(target, data) => {
                self.tokens.processing_instruction(xot, *target, *data, out)
            }
        }
    }
}

/// Writes XML incrementally: a start tag, a text, a comment, a processing
/// instruction or an end tag at a time, as the caller produces them, instead
/// of serializing a tree that exists already.
///
/// It renders each token as [`Xot::serialize_xml_write`] renders the token
/// of a tree node, with the same escapes, the same checks and the same
/// errors, and chooses the prefixes of names from the namespace declarations
/// it has written. Fed the outputs of a document node, element by element,
/// it writes the bytes that `serialize_xml_write` writes for that document,
/// or fails with the same error.
///
/// The writer keeps no borrow of the [`Xot`] between calls: every method
/// takes it, so the caller can change the `Xot` in between. The nodes it is
/// given are read at the call; a start tag reads the name and the attributes
/// of its element then.
///
/// Facts that a tree would give are given by the caller: the namespace
/// declarations of each start tag, whether an element has a child (`>` or
/// `/>`, and whether an end tag is written), and whether a text is written
/// as CDATA sections ([`Writer::is_cdata_section_element`] says it for the
/// name of the element that contains it).
///
/// Each call writes its token with one `write_all` to `W`; give it a
/// buffered writer. When a call returns an error, nothing of its token was
/// written, and the bytes written before are not a result: the writer should
/// not be used further, apart from [`Writer::into_inner`].
///
/// ```rust
/// use xot::Xot;
/// use xot::output::xml::{Parameters, Writer};
/// use xot::output::NoopNormalizer;
///
/// let mut xot = Xot::new();
/// let root = xot.parse(r#"<a xmlns="urn:example:a" x="1"/>"#).unwrap();
/// let a = xot.document_element(root).unwrap();
/// let declarations: Vec<_> = xot.namespaces(a).iter().map(|(p, n)| (p, *n)).collect();
///
/// let mut writer = Writer::new(&xot, &Parameters::default(), Vec::new(), NoopNormalizer).unwrap();
/// writer.start_element(&xot, a, &declarations, true).unwrap();
/// // the caller may change the Xot between calls
/// let text = xot.new_text("t");
/// writer.text(&xot, xot.text_str(text).unwrap(), false, text).unwrap();
/// writer.end_element(&xot, true).unwrap();
/// let bytes = writer.into_inner();
/// assert_eq!(bytes, br#"<a xmlns="urn:example:a" x="1">t</a>"#);
/// ```
pub struct Writer<W: Write, N: Normalizer> {
    tokens: TokenRenderer<N>,
    // the names of the open elements
    names: Vec<NameId>,
    scratch: String,
    w: W,
}

impl<W: Write, N: Normalizer> Writer<W, N> {
    /// A writer into `w`, which first gets the XML declaration of
    /// `parameters`, if any, as [`Xot::serialize_xml_write`] writes it.
    ///
    /// The namespace of the `xml` prefix is in scope from the start, as it is
    /// for a document node.
    ///
    /// Indentation and a doctype declaration need the tree; parameters with
    /// either are refused with [`Error::InvalidOperation`].
    pub fn new(
        xot: &Xot,
        parameters: &output::xml::Parameters,
        mut w: W,
        normalizer: N,
    ) -> Result<Self, Error> {
        if parameters.indentation.is_some() {
            return Err(Error::InvalidOperation(
                "the XML writer does not indent".to_string(),
            ));
        }
        if parameters.doctype.is_some() {
            return Err(Error::InvalidOperation(
                "the XML writer does not write a doctype declaration".to_string(),
            ));
        }
        if let Some(declaration) = &parameters.declaration {
            declaration.serialize(&mut w)?;
        }
        let scope = NameScope::new(xot, &[(xot.xml_prefix(), xot.xml_namespace())]);
        let tokens = TokenRenderer::new(
            scope,
            TokenSerializeParameters {
                cdata_section_elements: parameters.cdata_section_elements.clone(),
                unescaped_gt: parameters.unescaped_gt,
                unescaped_cr: parameters.unescaped_cr,
            },
            normalizer,
        );
        Ok(Self {
            tokens,
            names: Vec::new(),
            scratch: String::with_capacity(SCRATCH_CAPACITY),
            w,
        })
    }

    /// Whether a text in an element named `name` is written as CDATA
    /// sections: whether the parameters name it among the CDATA section
    /// elements.
    #[inline]
    pub fn is_cdata_section_element(&self, name: NameId) -> bool {
        self.tokens.is_cdata_section_element(name)
    }

    /// Write the start tag of `element`: its name, the namespace
    /// declarations `declarations` in their order, then the attributes of
    /// the element in their order, and `>` if `has_child`, else `/>`.
    ///
    /// The declarations are in scope from this element on, until its
    /// [`end_element`](Writer::end_element); the name and the attributes are
    /// written with the prefixes they give. They need not be the namespace
    /// nodes of `element`; the namespace nodes of `element` are not read.
    ///
    /// Every element given to `start_element` is closed with
    /// [`end_element`](Writer::end_element), also one without a child.
    pub fn start_element(
        &mut self,
        xot: &Xot,
        element: Node,
        declarations: &[(PrefixId, NamespaceId)],
        has_child: bool,
    ) -> Result<(), Error> {
        if !xot.is_element(element) {
            return Err(Error::NotElement(element));
        }
        let name = xot.get_element_name(element);
        self.scratch.clear();
        let result = self.render_start_tag(xot, element, name, declarations, has_child);
        self.write_scratch(result)
    }

    fn render_start_tag(
        &mut self,
        xot: &Xot,
        element: Node,
        name: NameId,
        declarations: &[(PrefixId, NamespaceId)],
        has_child: bool,
    ) -> Result<(), Error> {
        let out = &mut self.scratch;
        let tokens = &mut self.tokens;
        tokens.start_tag_open(xot, name, declarations.iter().copied(), out)?;
        self.names.push(name);
        for &(prefix, namespace) in declarations {
            let start = tokens.check(xot, element, &Output::Prefix(prefix, namespace))?;
            let at = out.len();
            out.push(' ');
            if !tokens.prefix(xot, prefix, namespace, Some(start), out) {
                out.truncate(at);
            }
        }
        for (name_id, value) in xot.attributes(element).iter() {
            let start = tokens.check(xot, element, &Output::Attribute(name_id, value))?;
            out.push(' ');
            tokens.attribute(xot, name_id, value, Some(start), out)?;
        }
        tokens.start_tag_close(has_child, out);
        Ok(())
    }

    /// Write a text, as CDATA sections if `cdata` (see
    /// [`is_cdata_section_element`](Writer::is_cdata_section_element)). An
    /// error names `error_node`.
    pub fn text(
        &mut self,
        xot: &Xot,
        text: &str,
        cdata: bool,
        error_node: Node,
    ) -> Result<(), Error> {
        self.scratch.clear();
        let result = self
            .tokens
            .check(xot, error_node, &Output::Text(text))
            .map(|start| {
                self.tokens
                    .text(text, cdata, Some(start), &mut self.scratch);
            });
        self.write_scratch(result)
    }

    /// Write a comment. An error names `error_node`.
    pub fn comment(&mut self, xot: &Xot, text: &str, error_node: Node) -> Result<(), Error> {
        self.scratch.clear();
        let result = self
            .tokens
            .check(xot, error_node, &Output::Comment(text))
            .map(|_| {
                self.tokens.comment(text, &mut self.scratch);
            });
        self.write_scratch(result)
    }

    /// Write a processing instruction. An error names `error_node`.
    pub fn processing_instruction(
        &mut self,
        xot: &Xot,
        target: NameId,
        data: Option<&str>,
        error_node: Node,
    ) -> Result<(), Error> {
        self.scratch.clear();
        let output = Output::ProcessingInstruction(target, data);
        let result = self.tokens.check(xot, error_node, &output).and_then(|_| {
            self.tokens
                .processing_instruction(xot, target, data, &mut self.scratch)
                .map(|_| ())
        });
        self.write_scratch(result)
    }

    /// Write the end tag of the innermost open element if `has_child`, as
    /// its [`start_element`](Writer::start_element) said, and take its
    /// declarations out of scope.
    pub fn end_element(&mut self, xot: &Xot, has_child: bool) -> Result<(), Error> {
        let Some(name) = self.names.pop() else {
            return Err(Error::InvalidOperation(
                "end_element without an open element".to_string(),
            ));
        };
        self.scratch.clear();
        let result = self
            .tokens
            .end_tag(xot, name, has_child, || false, &mut self.scratch)
            .map(|_| ());
        self.write_scratch(result)
    }

    /// The writer that was given to [`new`](Writer::new).
    pub fn into_inner(self) -> W {
        self.w
    }

    /// Write what the call rendered, unless it gave an error.
    #[inline]
    fn write_scratch(&mut self, result: Result<(), Error>) -> Result<(), Error> {
        result?;
        if !self.scratch.is_empty() {
            self.w.write_all(self.scratch.as_bytes())?;
        }
        reset_large_scratch(&mut self.scratch);
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

    // The walk of a node can go on after the node, and give the end tag of an
    // element without its start tag. The namespaces in scope are then gone, as
    // they were in the serializer with a list for each element.
    #[test]
    fn test_end_tag_without_start_tag() {
        let mut xot = Xot::new();
        let doc = xot
            .parse(r#"<s:a xmlns:s="urn:s"><b/><s:c/></s:a>"#)
            .unwrap();
        let a = xot.document_element(doc).unwrap();
        let b = xot.first_child(a).unwrap();
        let c = xot.next_sibling(b).unwrap();
        let mut renderer = XmlRenderer::new(
            &xot,
            b,
            TokenSerializeParameters::default(),
            crate::output::NoopNormalizer,
        );
        let element = |node| *xot.element(node).unwrap();
        let mut token = |node, output: Output<'static>| {
            let mut out = String::new();
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                renderer.token(node, &output, &mut out, true).unwrap();
            }))
            .map(|_| out)
            .map_err(|cause| cause.downcast_ref::<String>().cloned().unwrap_or_default())
        };
        assert_eq!(token(b, Output::StartTagOpen(element(b))).unwrap(), "<b");
        assert_eq!(token(b, Output::StartTagClose).unwrap(), "/>");
        assert_eq!(token(b, Output::EndTag(element(b))).unwrap(), "");
        // the end tag of the parent, which has declarations
        assert_eq!(token(a, Output::EndTag(element(a))).unwrap(), "</s:a>");
        // a name in no namespace is still written
        assert_eq!(token(b, Output::StartTagOpen(element(b))).unwrap(), "<b");
        // a name in a namespace, and an element with declarations, are not
        let message = "called `Option::unwrap()` on a `None` value";
        assert_eq!(
            token(c, Output::StartTagOpen(element(c))).unwrap_err(),
            message
        );
        assert_eq!(
            token(a, Output::StartTagOpen(element(a))).unwrap_err(),
            message
        );
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
