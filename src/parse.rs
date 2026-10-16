use std::borrow::Cow;

use ahash::{HashMap, HashMapExt, HashSet, HashSetExt};
use xmlparser::{ElementEnd, StrSpan, Stream, TextPos, Token, Tokenizer};

use crate::arena::NodeId;
use crate::encoding::decode;
use crate::entity::{normalize_line_ends, parse_attribute, parse_text};
use crate::error::ParseError;
use crate::id::{Name, NameId, PrefixId};
use crate::xmlvalue::{Attribute, Comment, Element, Namespace, ProcessingInstruction, Text, Value};
use crate::xotdata::{Node, Xot, XMLNS_NAMESPACE, XML_NAMESPACE};
use crate::NamespaceId;

type Namespaces = Vec<(PrefixId, NamespaceId)>;

struct AttributeBuilder {
    prefix: String,
    name: String,
    value: String,
    name_span: Span,
    value_span: Span,
    prefix_span: Span,
}

struct ElementBuilder {
    prefix: String,
    name: String,
    namespaces: Namespaces,
    attributes: Vec<AttributeBuilder>,
    prefix_span: Span,
    span: Span,
}

impl ElementBuilder {
    fn new(prefix: StrSpan<'_>, name: StrSpan<'_>) -> Self {
        ElementBuilder {
            prefix: prefix.to_string(),
            name: name.to_string(),
            namespaces: Namespaces::new(),
            attributes: Vec::new(),
            prefix_span: prefix.into(),
            span: Span::from_prefix_name(prefix, name),
        }
    }
}

struct DocumentBuilder {
    tree: NodeId,
    current_node_id: NodeId,
    name_id_builder: NameIdBuilder,
    element_builder: Option<ElementBuilder>,
    seen_ids: HashSet<String>,
    id_nodes: HashMap<String, NodeId>,
    xml_id_id: NameId,
}

impl DocumentBuilder {
    fn new(xot: &mut Xot) -> Self {
        let document = xot.arena.new_node(Value::Document);
        let mut name_id_builder = NameIdBuilder::new(xot.base_prefixes().into_iter().collect());
        let base_prefixes = vec![(xot.empty_prefix_id, xot.no_namespace_id)];
        name_id_builder.push(base_prefixes);
        DocumentBuilder {
            tree: document,
            current_node_id: document,
            name_id_builder,
            element_builder: None,
            seen_ids: HashSet::new(),
            id_nodes: HashMap::new(),
            xml_id_id: xot.xml_id_id,
        }
    }

    fn element(&mut self, prefix: StrSpan<'_>, name: StrSpan<'_>) {
        self.element_builder = Some(ElementBuilder::new(prefix, name));
    }

    /// A namespace declaration: the attribute `xmlns` declares the default
    /// namespace, and the attribute `xmlns:p` the prefix `p`.
    fn prefix(
        &mut self,
        prefix: StrSpan<'_>,
        local: StrSpan<'_>,
        value: StrSpan<'_>,
        xot: &mut Xot,
    ) -> Result<(), ParseError> {
        let (declared, name) = if prefix.is_empty() {
            ("", "xmlns".to_string())
        } else {
            (local.as_str(), format!("xmlns:{}", local))
        };
        let span = Span::from_prefix_name(prefix, local);
        // the namespace name is the normalized value of the namespace
        // declaration attribute, with its references expanded like in any
        // other attribute value (https://www.w3.org/TR/xml-names/#ns-decl)
        let namespace_uri = parse_attribute(value.as_str().into(), value.start())?;
        let prefix_id = xot.prefix_lookup.get_id_mut(declared);
        let namespaces = &mut self.element_builder.as_mut().unwrap().namespaces;
        // like any attribute, a declaration can be on an element only once
        if namespaces.iter().any(|(p, _)| *p == prefix_id) {
            return Err(ParseError::DuplicateAttribute(name, span));
        }
        if !is_allowed_declaration(declared, &namespace_uri) {
            return Err(ParseError::InvalidNamespaceDeclaration(
                name,
                namespace_uri.into_owned(),
                span,
            ));
        }
        let namespace_id = xot.namespace_lookup.get_id_mut(namespace_uri.as_ref());
        namespaces.push((prefix_id, namespace_id));
        Ok(())
    }

    fn attribute(
        &mut self,
        prefix: StrSpan<'_>,
        name: StrSpan<'_>,
        value: StrSpan<'_>,
    ) -> Result<(), ParseError> {
        let attributes = &mut self.element_builder.as_mut().unwrap().attributes;
        let is_duplicate = attributes.iter().any(|attribute_builder| {
            attribute_builder.prefix == prefix.as_str() && attribute_builder.name == name.as_str()
        });
        if is_duplicate {
            let attr_name = if prefix.is_empty() {
                name.to_string()
            } else {
                format!("{}:{}", prefix, name)
            };
            let span = Span::from_prefix_name(prefix, name);
            return Err(ParseError::DuplicateAttribute(attr_name, span));
        }
        let value_span = value.into();
        let value = parse_attribute(value.as_str().into(), value.start())?.to_string();
        // if this is an xml:id we want to apply xml:id normalization as described here
        // https://www.w3.org/TR/xml-id/#id-avn
        let value = if name == "id" && prefix == "xml" {
            normalize_xml_id(&value)
        } else {
            value
        };
        attributes.push(AttributeBuilder {
            prefix: prefix.to_string(),
            name: name.to_string(),
            value,
            name_span: Span::from_prefix_name(prefix, name),
            value_span,
            prefix_span: prefix.into(),
        });
        Ok(())
    }

    fn add(&mut self, value: Value, xot: &mut Xot) -> NodeId {
        let node_id = xot.arena.new_node(value);
        self.append_to_current(node_id, xot);
        node_id
    }

    /// Append `node_id`, a node just made, to the current node.
    fn append_to_current(&self, node_id: NodeId, xot: &mut Xot) {
        xot.arena
            .checked_append(self.current_node_id, node_id)
            .expect("Preconditions not met: invalid argument");
    }

    fn open_element(
        &mut self,
        xot: &mut Xot,
    ) -> Result<(NodeId, Span, AttributeSpans), ParseError> {
        let element_builder = self.element_builder.take().unwrap();
        let span = element_builder.span;

        self.name_id_builder
            .push(element_builder.namespaces.clone());

        let name_id = self.name_id_builder.element_name_id(
            &element_builder.prefix,
            &element_builder.name,
            element_builder.prefix_span,
            xot,
        )?;
        let element_value = Value::Element(Element { name_id });
        let node_id = self.add(element_value, xot);
        self.current_node_id = node_id;

        // add namespace nodes
        for (prefix_id, namespace_id) in &element_builder.namespaces {
            let namespace_node = xot.arena.new_node(Value::Namespace(Namespace {
                prefix_id: *prefix_id,
                namespace_id: *namespace_id,
            }));
            self.append_to_current(namespace_node, xot);
        }
        // add attribute nodes
        let mut attribute_spans: AttributeSpans = Vec::new();
        for attribute_builder in element_builder.attributes {
            let name_id = self.name_id_builder.attribute_name_id(
                &attribute_builder.prefix,
                &attribute_builder.name,
                attribute_builder.prefix_span,
                xot,
            )?;
            // two attributes cannot have the same expanded name, even with
            // different prefixes (https://www.w3.org/TR/xml-names/#uniqAttrs)
            if attribute_spans.iter().any(|(seen, _, _)| *seen == name_id) {
                let name = if attribute_builder.prefix.is_empty() {
                    attribute_builder.name
                } else {
                    format!("{}:{}", attribute_builder.prefix, attribute_builder.name)
                };
                return Err(ParseError::DuplicateAttribute(
                    name,
                    attribute_builder.name_span,
                ));
            }
            // if we see xml:id, check that they aren't a duplicate
            // and keep track of all node ids that have an xml:id
            if name_id == self.xml_id_id {
                if self.seen_ids.contains(&attribute_builder.value) {
                    return Err(ParseError::DuplicateId(
                        attribute_builder.value,
                        attribute_builder.value_span,
                    ));
                }
                self.seen_ids.insert(attribute_builder.value.clone());
                // use entry api to add node id for the value given
                self.id_nodes
                    .insert(attribute_builder.value.clone(), node_id);
            }

            let attribute_node = xot.arena.new_node(Value::Attribute(Attribute {
                name_id,
                value: attribute_builder.value,
            }));
            attribute_spans.push((
                name_id,
                attribute_builder.name_span,
                attribute_builder.value_span,
            ));
            self.append_to_current(attribute_node, xot);
        }

        Ok((node_id, span, attribute_spans))
    }

    // consolidates a text node with previous node if possible. If consolidation
    // took place returns the node id , otherwise none.
    fn consolidate_text(&mut self, content: &str, xot: &mut Xot) -> Option<NodeId> {
        // let's look at the last node we added
        let last = xot.arena.last_child(self.current_node_id);
        if let Some(last) = last {
            if let Value::Text(last_text) = xot.arena.get_mut(last) {
                last_text.get_mut().push_str(content);
                return Some(last);
            }
        }
        None
    }

    fn text(&mut self, content: &StrSpan, xot: &mut Xot) -> Result<NodeId, ParseError> {
        let content = parse_text(content.as_str().into(), content.start())?;
        if let Some(last) = self.consolidate_text(&content, xot) {
            return Ok(last);
        }
        Ok(self.add(Value::Text(Text::new(content.to_string())), xot))
    }

    fn cdata_text(&mut self, content: &str, xot: &mut Xot) -> Result<NodeId, ParseError> {
        // a CDATA section has no references, but its line ends are
        // normalized like everywhere else
        let content = normalize_line_ends(content);
        if let Some(last) = self.consolidate_text(&content, xot) {
            return Ok(last);
        }
        Ok(self.add(Value::Text(Text::new(content.into_owned())), xot))
    }

    fn close_element_immediate(&mut self, xot: &mut Xot) -> NodeId {
        if matches!(xot.arena.get(self.current_node_id), Value::Element(_)) {
            self.name_id_builder.pop();
        }
        let closed_node_id = self.current_node_id;
        self.current_node_id = xot
            .arena
            .parent(closed_node_id)
            .expect("Cannot close document node");
        closed_node_id
    }

    fn close_element(
        &mut self,
        prefix: StrSpan,
        name: StrSpan,
        xot: &mut Xot,
    ) -> Result<NodeId, ParseError> {
        // an end tag without a start tag, which the tokenizer reports in a
        // document, but takes in a fragment
        if self.is_current_node_document(xot) {
            return Err(ParseError::InvalidCloseTag(
                prefix.to_string(),
                name.to_string(),
                Span::from_prefix_name(prefix, name),
            ));
        }
        let name_id = self
            .name_id_builder
            .element_name_id(&prefix, &name, prefix.into(), xot)?;
        if let Value::Element(element) = xot.arena.get(self.current_node_id) {
            if element.name_id != name_id {
                return Err(ParseError::InvalidCloseTag(
                    prefix.to_string(),
                    name.to_string(),
                    Span::from_prefix_name(prefix, name),
                ));
            }
            self.name_id_builder.pop();
        }
        let closed_node_id = self.current_node_id;
        self.current_node_id = xot
            .arena
            .parent(closed_node_id)
            .expect("Cannot close document node");
        Ok(closed_node_id)
    }

    fn comment(&mut self, content: &str, xot: &mut Xot) -> Result<NodeId, ParseError> {
        // XXX are there illegal comments, like those with -- inside? or
        // won't they pass the parser?
        let content = normalize_line_ends(content).into_owned();
        Ok(self.add(Value::Comment(Comment::new(content)), xot))
    }

    fn processing_instruction(
        &mut self,
        target: &str,
        content: Option<&str>,
        xot: &mut Xot,
    ) -> Result<NodeId, ParseError> {
        // XXX are there illegal processing instructions, like those with
        // ?> inside? or won't they pass the parser? What about those with xml?
        let target = xot.add_name(target);
        Ok(self.add(
            Value::ProcessingInstruction(ProcessingInstruction::new(
                target,
                content.map(|s| normalize_line_ends(s).into_owned()),
            )),
            xot,
        ))
    }

    fn is_current_node_document(&self, xot: &Xot) -> bool {
        matches!(xot.arena.get(self.current_node_id), Value::Document)
    }
}

struct NameIdBuilder {
    namespace_stack: Vec<Namespaces>,
}

impl NameIdBuilder {
    fn new(prefixes: Namespaces) -> Self {
        let namespace_stack = vec![prefixes];
        Self { namespace_stack }
    }

    fn push(&mut self, namespaces: Namespaces) {
        self.namespace_stack.push(namespaces);
    }

    fn pop(&mut self) {
        // should always be able to pop as there's a bottom entry
        self.namespace_stack.pop();
    }

    fn element_name_id(
        &mut self,
        prefix: &str,
        name: &str,
        prefix_span: Span,
        xot: &mut Xot,
    ) -> Result<NameId, ParseError> {
        let prefix_id = xot.prefix_lookup.get_id_mut(prefix);
        if let Ok(name_id) = self.name_id_with_prefix_id(prefix_id, name, xot) {
            Ok(name_id)
        } else {
            Err(ParseError::UnknownPrefix(prefix.to_string(), prefix_span))
        }
    }

    fn attribute_name_id(
        &mut self,
        prefix: &str,
        name: &str,
        prefix_span: Span,
        xot: &mut Xot,
    ) -> Result<NameId, ParseError> {
        // an unprefixed attribute is in no namespace, not
        // in the default namespace
        // https://stackoverflow.com/questions/3312390/xml-default-namespaces-for-unqualified-attribute-names
        let prefix_id = xot.prefix_lookup.get_id_mut(prefix);
        if prefix_id == xot.empty_prefix_id {
            let name = Name::new(name.to_string(), xot.no_namespace_id);
            return Ok(xot.name_lookup.get_id_mut(&name));
        }
        if let Ok(name_id) = self.name_id_with_prefix_id(prefix_id, name, xot) {
            Ok(name_id)
        } else {
            Err(ParseError::UnknownPrefix(prefix.to_string(), prefix_span))
        }
    }

    fn name_id_with_prefix_id(
        &mut self,
        prefix_id: PrefixId,
        name: &str,
        xot: &mut Xot,
    ) -> Result<NameId, ()> {
        // go through namespace stack backwards, find the first namespace
        // that matches this prefix
        let namespace_id = self.namespace_stack.iter().rev().find_map(|ns| {
            ns.iter()
                .rev()
                .find_map(|(p, ns)| if *p == prefix_id { Some(*ns) } else { None })
        });
        let namespace_id = namespace_id.ok_or(())?;
        let name = Name::new(name.to_string(), namespace_id);
        Ok(xot.name_lookup.get_id_mut(&name))
    }
}

/// A span with a start and end position
///
/// Spans describe ranges in the source text, with the end point not inclusive,
/// like a range. It's not a `std::ops::Range` as it's handy for a span to be
/// `Copy`.
///
/// You can obtain these from a [`SpanInfo`](crate::SpanInfo). You create a
/// [`SpanInfo`] by using
/// [`Xot::parse_with_span_info`](crate::Xot::parse_with_span_info).
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct Span {
    /// the start position in the XML source
    pub start: usize,
    /// the end position in the XML source
    pub end: usize,
}

impl Span {
    /// Construct a new span
    pub fn new(start: usize, end: usize) -> Self {
        Span { start, end }
    }

    fn from_prefix_name(prefix: StrSpan<'_>, name: StrSpan<'_>) -> Self {
        if prefix.is_empty() {
            Self::new(name.start(), name.end())
        } else {
            Self::new(prefix.start(), name.end())
        }
    }

    /// Turn a span into a range
    pub fn range(&self) -> std::ops::Range<usize> {
        self.start..self.end
    }
}

impl From<xmlparser::StrSpan<'_>> for Span {
    fn from(span: xmlparser::StrSpan) -> Self {
        Span {
            start: span.start(),
            end: span.end(),
        }
    }
}

impl From<&xmlparser::StrSpan<'_>> for Span {
    fn from(span: &xmlparser::StrSpan) -> Self {
        Span {
            start: span.start(),
            end: span.end(),
        }
    }
}

impl From<Span> for std::ops::Range<usize> {
    fn from(span: Span) -> Self {
        span.range()
    }
}

impl From<std::ops::Range<usize>> for Span {
    fn from(range: std::ops::Range<usize>) -> Self {
        Span {
            start: range.start,
            end: range.end,
        }
    }
}

type AttributeSpans = Vec<(NameId, Span, Span)>;

/// A key to use to look up span information using
/// [`SpanInfo::get`](`crate::SpanInfo::get`)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpanInfoKey {
    /// The name part of an attribute.
    /// In `foo:name="value"`, the `foo:name` part
    AttributeName(Node, NameId),
    /// The value part of an attribute.
    /// In `foo:name="value"`, the `value` part
    AttributeValue(Node, NameId),
    /// The name part of a start element tag.
    /// In `<foo:name ..>`, the `foo:name` part
    ElementStart(Node),
    /// The closing part of the end element tag (or a self-closing element).
    /// In `</foo:name>`, the `</foo:name>` part, or if it is an empty element
    /// `<foo:name/>`, the `/>` part
    ElementEnd(Node),
    /// Text node.
    /// In `<foo>text</foo>`, the `text` part
    Text(Node),
    /// Comment node.
    /// In `<!--comment-->`, the `comment` part
    Comment(Node),
    /// The target part of a processing instruction.
    /// In `<?target content?>`, the `target` part
    PiTarget(Node),
    /// The content part of a processing instruction (if defined).
    /// In `<?target content?>`, the `content` part
    PiContent(Node),
}

/// Span information for a parsed XML document.
///
/// This span information is valid immediately after the parse. It becomes
/// invalid as soon as you mutate the parsed document.
///
/// You obtain this by using
/// [`Xot::parse_with_span_info`](`Xot::parse_with_span_info`).
///
/// You use a [`SpanInfoKey`](crate::SpanInfoKey) to look up the span
/// information.
///
/// The span of a CDATA section is only its text content:
///
/// ```text
/// <p><![CDATA[content]]></p>
///             ^^^^^^^
/// ```
///
/// There is an exception to this. During parsing, adjacent CDATA sections and
/// text nodes are consolidated into a single text node. This text node has the
/// span starting with the first adjacent CDATA section or text node and ending
/// with the last adjacent CDATA section or text node.
///
/// Example:
///
/// ```text
/// <p>text<![CDATA[content]]>text</p>
///    ^^^^^^^^^^^^^^^^^^^^^^^^^^^
/// ```
///
/// This can lead to the slightly odd situation where only part of the CDATA
/// marker is included in the span:
///
/// ```text
/// <p>text<![CDATA[content]]>foo</p>
///                 ^^^^^^^^^^^^^
///```
///
/// In every case all text content in the adjacent CDATA and text is included
/// in the span.
pub struct SpanInfo {
    map: HashMap<SpanInfoKey, Span>,
}

impl SpanInfo {
    fn new() -> Self {
        SpanInfo {
            map: HashMap::new(),
        }
    }

    /// Get span info by [`SpanInfoKey`](crate::SpanInfoKey)
    pub fn get(&self, key: SpanInfoKey) -> Option<&Span> {
        self.map.get(&key)
    }

    fn add(&mut self, key: SpanInfoKey, span: Span) {
        self.map.insert(key, span);
    }

    fn extend_text_span(&mut self, node: Node, span: Span) {
        // if we already have span for this (text) node it, we need to store the span with that
        // start and the given ending
        let key = SpanInfoKey::Text(node);
        if let Some(existing_span) = self.map.get(&key) {
            let start = existing_span.start;
            let end = span.end;
            self.map.insert(key, Span::new(start, end));
        } else {
            self.map.insert(key, span);
        }
    }
}

/// Where `Xot::_parse` puts the spans of the nodes it makes, and where the
/// checks after it find the spans of their errors.
///
/// [`SpanInfo`] keeps the spans of all nodes, for
/// [`Xot::parse_with_span_info`] and [`Xot::parse_fragment_with_span_info`].
/// [`ErrorSpans`] keeps only the spans that the checks can report, for
/// [`Xot::parse`] and [`Xot::parse_fragment`], so that these take no memory
/// for the spans of all nodes. The checks get the same spans from both.
trait SpanRecorder {
    /// An element was opened: `span` is its name in the start tag.
    fn element_start(&mut self, node_id: NodeId, span: Span);

    /// An attribute of the element that was just opened: `name_span` is its
    /// name and `value_span` its value.
    fn attribute(&mut self, node_id: NodeId, name_id: NameId, name_span: Span, value_span: Span);

    /// An element was closed: `span` is its end tag, or the `/>` of an empty
    /// element tag.
    fn element_end(&mut self, node_id: NodeId, span: Span);

    /// Text or the content of a CDATA section, which made the text node
    /// `node_id`, or was merged into it as that node was the last child of
    /// its parent already.
    fn text(&mut self, node_id: NodeId, span: Span);

    /// A comment: `span` is its text.
    fn comment(&mut self, node_id: NodeId, span: Span);

    /// A processing instruction: the spans of its target and its content.
    fn processing_instruction(&mut self, node_id: NodeId, target: Span, content: Option<Span>);

    /// The span of `node`, the first text node under the document node, for
    /// [`ParseError::TextAtTopLevel`].
    fn top_level_text(&self, node: Node) -> Span;

    /// The span of the name in the start tag of `node`, the second element
    /// under the document node, for
    /// [`ParseError::MultipleElementsAtTopLevel`].
    fn second_top_level_element(&self, node: Node) -> Span;

    /// The span of the name in the start tag of `node`, the innermost element
    /// that is still open at the end of the text, for
    /// [`ParseError::UnclosedTag`].
    fn unclosed_element(&self, node: Node) -> Span;
}

impl SpanRecorder for SpanInfo {
    fn element_start(&mut self, node_id: NodeId, span: Span) {
        self.add(SpanInfoKey::ElementStart(Node::new(node_id)), span);
    }

    fn attribute(&mut self, node_id: NodeId, name_id: NameId, name_span: Span, value_span: Span) {
        self.add(
            SpanInfoKey::AttributeName(Node::new(node_id), name_id),
            name_span,
        );
        self.add(
            SpanInfoKey::AttributeValue(Node::new(node_id), name_id),
            value_span,
        );
    }

    fn element_end(&mut self, node_id: NodeId, span: Span) {
        self.add(SpanInfoKey::ElementEnd(Node::new(node_id)), span);
    }

    fn text(&mut self, node_id: NodeId, span: Span) {
        self.extend_text_span(Node::new(node_id), span);
    }

    fn comment(&mut self, node_id: NodeId, span: Span) {
        self.add(SpanInfoKey::Comment(Node::new(node_id)), span);
    }

    fn processing_instruction(&mut self, node_id: NodeId, target: Span, content: Option<Span>) {
        self.add(SpanInfoKey::PiTarget(Node::new(node_id)), target);
        if let Some(content) = content {
            self.add(SpanInfoKey::PiContent(Node::new(node_id)), content);
        }
    }

    fn top_level_text(&self, node: Node) -> Span {
        *self.get(SpanInfoKey::Text(node)).unwrap()
    }

    fn second_top_level_element(&self, node: Node) -> Span {
        *self.get(SpanInfoKey::ElementStart(node)).unwrap()
    }

    fn unclosed_element(&self, node: Node) -> Span {
        *self.get(SpanInfoKey::ElementStart(node)).unwrap()
    }
}

/// The spans that the checks after a parse can report, the only spans that
/// [`Xot::parse`] and [`Xot::parse_fragment`] keep. Their memory grows with
/// the depth of the elements, where a [`SpanInfo`] holds a span for every
/// node. Each span is the one that a [`SpanInfo`] has for the same node.
///
/// The nodes under the document node are made in the order of the text, which
/// is the order of its children: the first text node made under it is its
/// first text child, and the second element made under it is its second
/// element child. An element or a text is made under the document node when
/// no element is open.
#[derive(Default)]
struct ErrorSpans {
    /// The first text node under the document node, and its span: from the
    /// start of the first text or CDATA section in it to the end of the last
    /// one merged into it, as `SpanInfo::extend_text_span` makes it.
    top_level_text: Option<(NodeId, Span)>,
    /// The number of elements opened under the document node.
    top_level_elements: usize,
    /// The second element under the document node, and the span of its name
    /// in its start tag.
    second_top_level_element: Option<(NodeId, Span)>,
    /// The elements that are open, the innermost last, and the spans of their
    /// names in their start tags.
    open_elements: Vec<(NodeId, Span)>,
}

impl SpanRecorder for ErrorSpans {
    #[inline]
    fn element_start(&mut self, node_id: NodeId, span: Span) {
        if self.open_elements.is_empty() {
            self.top_level_elements += 1;
            if self.top_level_elements == 2 {
                self.second_top_level_element = Some((node_id, span));
            }
        }
        self.open_elements.push((node_id, span));
    }

    #[inline]
    fn attribute(
        &mut self,
        _node_id: NodeId,
        _name_id: NameId,
        _name_span: Span,
        _value_span: Span,
    ) {
    }

    #[inline]
    fn element_end(&mut self, node_id: NodeId, _span: Span) {
        let closed = self.open_elements.pop();
        debug_assert_eq!(closed.map(|(closed, _)| closed), Some(node_id));
    }

    #[inline]
    fn text(&mut self, node_id: NodeId, span: Span) {
        if !self.open_elements.is_empty() {
            return;
        }
        match &mut self.top_level_text {
            None => self.top_level_text = Some((node_id, span)),
            // merged into the first text node: its span grows, as in
            // `SpanInfo::extend_text_span`
            Some((first, first_span)) if *first == node_id => first_span.end = span.end,
            Some(_) => {}
        }
    }

    #[inline]
    fn comment(&mut self, _node_id: NodeId, _span: Span) {}

    #[inline]
    fn processing_instruction(&mut self, _node_id: NodeId, _target: Span, _content: Option<Span>) {}

    fn top_level_text(&self, node: Node) -> Span {
        let (first, span) = self
            .top_level_text
            .expect("No text node under the document node");
        debug_assert_eq!(first, node.get());
        span
    }

    fn second_top_level_element(&self, node: Node) -> Span {
        let (second, span) = self
            .second_top_level_element
            .expect("No second element under the document node");
        debug_assert_eq!(second, node.get());
        span
    }

    fn unclosed_element(&self, node: Node) -> Span {
        let (innermost, span) = *self.open_elements.last().expect("No open element");
        debug_assert_eq!(innermost, node.get());
        span
    }
}

/// ## Parsing
impl Xot {
    /// Parse a string containing XML into a document node. Retain span information.
    ///
    /// This parses the XML source into a Xot tree, and also returns
    /// [`SpanInfo`](`crate::SpanInfo`) which describes where nodes in the
    /// tree are located in the source text.
    pub fn parse_with_span_info(&mut self, xml: &str) -> Result<(Node, SpanInfo), ParseError> {
        let mut span_info = SpanInfo::new();
        let document_node = self.parse_document(xml, &mut span_info)?;
        Ok((document_node, span_info))
    }

    /// Parse `xml` as a document, with the spans in `recorder`.
    fn parse_document<R: SpanRecorder>(
        &mut self,
        xml: &str,
        recorder: &mut R,
    ) -> Result<Node, ParseError> {
        let text = declaration_with_space(xml);
        let tokenizer = Tokenizer::from(text.as_ref());
        self.parse_tokens_as_document(tokenizer, xml, recorder)
    }

    /// Parse the tokens of `xml` into a document node, and check that it is
    /// a well-formed document: no element is left open, and the document node
    /// has one element and no text.
    ///
    /// The tokenizer of a document does not give text, or a second element,
    /// outside the document element, but stops with an error first; the
    /// tests also give this function the tokens of a fragment.
    fn parse_tokens_as_document<R: SpanRecorder>(
        &mut self,
        tokenizer: Tokenizer<'_>,
        xml: &str,
        recorder: &mut R,
    ) -> Result<Node, ParseError> {
        let builder = self._parse(tokenizer, xml, recorder)?;
        // we expect both a document as the current node (everything else being
        // closed) *and* the content of this node containing a single element
        // if not, we have a problem. We want to produce a parse error for
        // this, as this is the parser.
        if builder.is_current_node_document(self) {
            let document_node = Node::new(builder.tree);
            let mut element_nodes = Vec::new();

            for child in self.children(document_node) {
                match self.value(child) {
                    Value::Element(_) => element_nodes.push(child),
                    Value::Text(_) => {
                        return Err(ParseError::TextAtTopLevel(recorder.top_level_text(child)));
                    }
                    _ => {}
                }
            }
            if element_nodes.is_empty() {
                return Err(ParseError::NoElementAtTopLevel(xml.len()));
            }
            if element_nodes.len() > 1 {
                return Err(ParseError::MultipleElementsAtTopLevel(
                    recorder.second_top_level_element(element_nodes[1]),
                ));
            }
            self.id_nodes_map
                .insert(document_node.get(), builder.id_nodes);
            Ok(document_node)
        } else {
            let current_node = Node::new(builder.current_node_id);

            // the top level node's span is the problem
            Err(ParseError::UnclosedTag(
                recorder.unclosed_element(current_node),
            ))
        }
    }

    /// Parse a string containing an XML fragment into a document node.
    ///
    /// This is similar to [`Xot::parse``], but it relaxes the well-formedness
    /// requirements. Specifically, it allows text nodes at the top level, and
    /// does not require a document element, and allows multiple elements. In
    /// short, a fragment behaves like an element (without name, attributes or
    /// namespace definitions).
    ///
    /// This is to support
    /// <https://www.w3.org/TR/xpath-datamodel/#DocumentNode> which is more
    /// permissive than standard XML.
    ///
    /// This parses the XML source into a Xot tree, and also returns
    /// [`SpanInfo`](`crate::SpanInfo`) which describes where nodes in the
    /// tree are located in the source text.
    pub fn parse_fragment_with_span_info(
        &mut self,
        xml: &str,
    ) -> Result<(Node, SpanInfo), ParseError> {
        let mut span_info = SpanInfo::new();
        let document_node = self.parse_fragment_document(xml, &mut span_info)?;
        Ok((document_node, span_info))
    }

    /// Parse `xml` as a fragment, with the spans in `recorder`.
    fn parse_fragment_document<R: SpanRecorder>(
        &mut self,
        xml: &str,
        recorder: &mut R,
    ) -> Result<Node, ParseError> {
        let tokenizer = Tokenizer::from_fragment(xml, 0..xml.len());
        let builder = self._parse(tokenizer, xml, recorder)?;
        if builder.is_current_node_document(self) {
            let document_node = Node::new(builder.tree);
            self.id_nodes_map
                .insert(document_node.get(), builder.id_nodes);
            Ok(document_node)
        } else {
            let current_node = Node::new(builder.current_node_id);

            // the top level node's span is the problem
            Err(ParseError::UnclosedTag(
                recorder.unclosed_element(current_node),
            ))
        }
    }

    /// Parse the tokens of `xml`, which the tokenizer reads, maybe with a
    /// space in the place of one character (see `declaration_with_space`),
    /// and tell `recorder` the spans of the nodes made.
    fn _parse<R: SpanRecorder>(
        &mut self,
        mut tokenizer: Tokenizer<'_>,
        xml: &str,
        recorder: &mut R,
    ) -> Result<DocumentBuilder, ParseError> {
        use Token::*;

        let mut builder = DocumentBuilder::new(self);

        let mut position;
        loop {
            // getting the position unconditionally is required to get
            // the right one for the error handling, which is a bit unfortunate
            // https://github.com/RazrFalcon/xmlparser/issues/30
            position = tokenizer.stream().pos();
            if let Some(token) = tokenizer.next() {
                let token = match token {
                    Ok(token) => token,
                    Err(e) => {
                        // the lines and columns of the error count in the
                        // text that the tokenizer reads, where a newline in
                        // `xml` can be a space
                        let text = tokenizer.stream().span().as_str();
                        return Err(ParseError::XmlParser(
                            reposition_error(e, text, xml),
                            position,
                        ));
                    }
                };
                match token {
                    Attribute {
                        prefix,
                        local,
                        value,
                        span: _,
                    } => {
                        // only `xmlns` and `xmlns:NCName` declare namespaces
                        // (https://www.w3.org/TR/xml-names/#ns-decl);
                        // `p:xmlns` is an attribute like any other
                        if prefix.as_str() == "xmlns"
                            || (prefix.is_empty() && local.as_str() == "xmlns")
                        {
                            builder.prefix(prefix, local, value, self)?;
                        } else {
                            builder.attribute(prefix, local, value)?;
                        }
                    }
                    Text { text } => {
                        let node_id = builder.text(&text, self)?;
                        recorder.text(node_id, text.into());
                    }
                    Cdata { text, span: _ } => {
                        let node_id = builder.cdata_text(text.as_str(), self)?;
                        recorder.text(node_id, text.into());
                    }
                    ElementStart {
                        prefix,
                        local,
                        span: _,
                    } => {
                        builder.element(prefix, local);
                    }

                    ElementEnd {
                        end,
                        span: end_span,
                    } => {
                        use self::ElementEnd::*;

                        match end {
                            Open => {
                                let (node_id, span, attribute_spans) =
                                    builder.open_element(self)?;
                                record_element_start(recorder, node_id, span, attribute_spans);
                            }
                            Close(prefix, local) => {
                                let node_id = builder.close_element(prefix, local, self)?;
                                recorder.element_end(node_id, end_span.into());
                            }
                            Empty => {
                                let (node_id, span, attribute_spans) =
                                    builder.open_element(self)?;
                                record_element_start(recorder, node_id, span, attribute_spans);
                                let node_id = builder.close_element_immediate(self);
                                recorder.element_end(node_id, end_span.into());
                            }
                        }
                    }
                    Comment { text, span: _ } => {
                        let node_id = builder.comment(text.as_str(), self)?;
                        recorder.comment(node_id, text.into());
                    }
                    ProcessingInstruction {
                        target,
                        content,
                        span: _,
                    } => {
                        // `xml` in any case is not a target
                        // (https://www.w3.org/TR/xml/#sec-pi); only the XML
                        // declaration has it there
                        if target.as_str().eq_ignore_ascii_case("xml") {
                            return Err(ParseError::InvalidPiTarget(
                                target.to_string(),
                                target.into(),
                            ));
                        }
                        let node_id = builder.processing_instruction(
                            target.as_str(),
                            content.map(|s| s.as_str()),
                            self,
                        )?;
                        recorder.processing_instruction(
                            node_id,
                            target.into(),
                            content.map(Span::from),
                        );
                    }
                    Declaration { version, .. } => {
                        if version.as_str() != "1.0" {
                            return Err(ParseError::UnsupportedVersion(
                                version.to_string(),
                                version.into(),
                            ));
                        }
                    }
                    DtdStart { span, .. } => {
                        return Err(ParseError::DtdUnsupported(span.into()));
                    }
                    DtdEnd { span, .. } => {
                        return Err(ParseError::DtdUnsupported(span.into()));
                    }
                    EmptyDtd { span, .. } => {
                        return Err(ParseError::DtdUnsupported(span.into()));
                    }
                    EntityDeclaration { span, .. } => {
                        return Err(ParseError::DtdUnsupported(span.into()));
                    }
                }
            } else {
                return Ok(builder);
            }
        }
    }

    /// Parse a string containing XML into a document node.
    ///
    /// Even though the encoding in the XML declaration may indicate otherwise,
    /// the string is interpreted as a Rust string, i.e. UTF-8. If you need to
    /// decode the string first, use [`Xot::parse_bytes`].
    ///
    /// The returned node is the document node of the
    /// parsed XML document.
    ///
    /// To know where the nodes are in `xml`, use
    /// [`Xot::parse_with_span_info`].
    ///
    /// ```rust
    /// use xot::Xot;
    ///
    /// let mut xot = Xot::new();
    /// let document = xot.parse("<hello/>")?;
    ///
    /// # Ok::<(), xot::Error>(())
    /// ```
    pub fn parse(&mut self, xml: &str) -> Result<Node, ParseError> {
        self.parse_document(xml, &mut ErrorSpans::default())
    }

    /// Parse a string containing an XML fragment into a document node.
    ///
    /// This is similar to [`Xot::parse``], but it relaxes the well-formedness
    /// requirements. Specifically, it allows text nodes at the top level, and
    /// does not require a document element, and allows multiple elements. In
    /// short, a fragment behaves like an element (without name, attributes or
    /// namespace definitions).
    ///
    /// This is to support
    /// <https://www.w3.org/TR/xpath-datamodel/#DocumentNode> which is more
    /// permissive than standard XML.
    ///
    /// To know where the nodes are in `xml`, use
    /// [`Xot::parse_fragment_with_span_info`].
    pub fn parse_fragment(&mut self, xml: &str) -> Result<Node, ParseError> {
        self.parse_fragment_document(xml, &mut ErrorSpans::default())
    }

    /// Parse bytes containing XML into a node.
    ///
    /// This decodes the bytes into a Rust string (UTF-8) first, then parses
    /// this string.
    ///
    /// The encoding follows the rules of the XML specification: a byte
    /// order mark decides it; without one, the encoding declaration in the XML
    /// declaration does; without either, the document is UTF-8. If the
    /// encoding declaration names an encoding that is not known, or the
    /// document is in an encoding that cannot be decoded, such as UTF-32, you
    /// get [`ParseError::UnsupportedEncoding`]. A byte sequence that is not
    /// valid in the encoding is a fatal error, as the XML specification
    /// requires: you get [`ParseError::InvalidByteSequence`], with the byte
    /// offsets of the sequence.
    ///
    /// An encoding declaration has to match the byte order mark, and the way
    /// the first characters are written in bytes, or you get
    /// [`ParseError::EncodingConflict`]: a UTF-8 byte order mark goes only
    /// with a declaration of UTF-8, and a UTF-16 one with UTF-16, or another
    /// name of UTF-16 such as UCS-2, or the UTF-16LE or UTF-16BE of its byte
    /// order; a document in UTF-16 without a byte order mark has to declare
    /// it.
    ///
    /// The spans of these errors are byte offsets in `bytes`, byte order mark
    /// included. The spans of the errors in the decoded text are byte offsets
    /// in that text.
    ///
    /// Encoding names are those of the WHATWG Encoding Standard, ignoring
    /// case, except for the names that IANA registers for US-ASCII,
    /// ISO-8859-1, ISO-8859-9 and TIS-620 (ISO-8859-11), apart from those with
    /// `:` such as ISO_8859-1:1987, which an encoding declaration cannot have:
    /// US-ASCII, iso-ir-6, ANSI_X3.4-1968, ANSI_X3.4-1986, ISO646-US, us,
    /// IBM367, cp367 and csASCII, as well as ASCII; ISO-8859-1, ISO_8859-1,
    /// iso-ir-100, latin1, l1, IBM819, CP819 and csISOLatin1; ISO-8859-9,
    /// ISO_8859-9, iso-ir-148, latin5, l5 and csISOLatin5; TIS-620, csTIS620
    /// and ISO-8859-11. They stand for these encodings, not for windows-1252,
    /// windows-1254 and windows-874: in US-ASCII a byte of 0x80 or more is not
    /// valid, and in the others the bytes 0x80 to 0x9F are the control
    /// characters U+0080 to U+009F.
    ///
    /// If you already have a Rust string, use [`Xot::parse`].
    ///
    /// The returned node is the document node of the parsed XML document.
    ///
    /// ```rust
    /// use xot::Xot;
    ///
    /// let mut xot = Xot::new();
    /// let document = xot.parse_bytes(b"<hello/>")?;
    ///
    /// # Ok::<(), xot::Error>(())
    /// ```
    ///
    /// ```rust
    /// use xot::Xot;
    ///
    /// let mut xot = Xot::new();
    ///
    /// let document = xot.parse_bytes(b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><p>\xe9</p>").unwrap();
    ///
    /// let doc_el = xot.document_element(document)?;
    /// let txt_value = xot.text_content_str(doc_el).unwrap();
    /// assert_eq!(txt_value, "é");
    /// # Ok::<(), xot::Error>(())
    /// ```
    pub fn parse_bytes(&mut self, bytes: &[u8]) -> Result<Node, ParseError> {
        let xml = decode(bytes)?;
        self.parse(&xml)
    }
}

/// Tell `recorder` the span of the name of the element `node_id`, which was
/// just opened, and the spans of its attributes.
fn record_element_start<R: SpanRecorder>(
    recorder: &mut R,
    node_id: NodeId,
    span: Span,
    attribute_spans: AttributeSpans,
) {
    recorder.element_start(node_id, span);
    for (name_id, name_span, value_span) in attribute_spans {
        recorder.attribute(node_id, name_id, name_span, value_span);
    }
}

/// `xml`, with a space in the place of the white space after `<?xml` if it
/// starts with `<?xml` followed by a tab, a newline or a carriage return,
/// after a byte order mark if it has one.
///
/// An XML declaration is `<?xml` followed by white space
/// (https://www.w3.org/TR/xml/#sec-prolog-dtd), which can be a tab, a
/// newline or a carriage return, but the tokenizer only takes `<?xml`
/// followed by a space for one, and reads the others as a processing
/// instruction with the target `xml`. The tokenizer skips a byte order mark
/// before it. A space instead of this one character changes neither the
/// declaration nor the byte index of anything in the text, but a newline
/// replaced by a space moves the lines and columns that follow it, so
/// `_parse` counts those of an error in `xml` again.
fn declaration_with_space(xml: &str) -> Cow<'_, str> {
    let start = if xml.starts_with('\u{FEFF}') {
        '\u{FEFF}'.len_utf8()
    } else {
        0
    };
    match xml.as_bytes()[start..] {
        [b'<', b'?', b'x', b'm', b'l', b'\t' | b'\n' | b'\r', ..] => {
            let mut xml = xml.to_string();
            let white_space = start + "<?xml".len();
            xml.replace_range(white_space..white_space + 1, " ");
            Cow::Owned(xml)
        }
        _ => Cow::Borrowed(xml),
    }
}

/// `error` from the tokenizer, with its lines and columns, which count in
/// `text`, the text that the tokenizer reads, counted in `xml` instead.
///
/// The two texts have their characters at the same byte indices (see
/// `declaration_with_space`), so each position is found as a byte index in
/// `text`, and that index as a line and a column in `xml`, as the tokenizer
/// counts them.
fn reposition_error(error: xmlparser::Error, text: &str, xml: &str) -> xmlparser::Error {
    use xmlparser::Error as E;
    use xmlparser::StreamError as S;

    let at = |pos: TextPos| Stream::from(xml).gen_text_pos_from(text_pos_index(text, pos));
    let stream_error = |e: S| match e {
        S::NonXmlChar(c, pos) => S::NonXmlChar(c, at(pos)),
        S::InvalidChar(actual, expected, pos) => S::InvalidChar(actual, expected, at(pos)),
        S::InvalidCharMultiple(actual, expected, pos) => {
            S::InvalidCharMultiple(actual, expected, at(pos))
        }
        S::InvalidQuote(c, pos) => S::InvalidQuote(c, at(pos)),
        S::InvalidSpace(c, pos) => S::InvalidSpace(c, at(pos)),
        S::InvalidString(expected, pos) => S::InvalidString(expected, at(pos)),
        S::UnexpectedEndOfStream
        | S::InvalidName
        | S::InvalidReference
        | S::InvalidExternalID
        | S::InvalidCommentData
        | S::InvalidCommentEnd
        | S::InvalidCharacterData => e,
    };
    match error {
        E::InvalidDeclaration(e, pos) => E::InvalidDeclaration(stream_error(e), at(pos)),
        E::InvalidComment(e, pos) => E::InvalidComment(stream_error(e), at(pos)),
        E::InvalidPI(e, pos) => E::InvalidPI(stream_error(e), at(pos)),
        E::InvalidDoctype(e, pos) => E::InvalidDoctype(stream_error(e), at(pos)),
        E::InvalidEntity(e, pos) => E::InvalidEntity(stream_error(e), at(pos)),
        E::InvalidElement(e, pos) => E::InvalidElement(stream_error(e), at(pos)),
        E::InvalidAttribute(e, pos) => E::InvalidAttribute(stream_error(e), at(pos)),
        E::InvalidCdata(e, pos) => E::InvalidCdata(stream_error(e), at(pos)),
        E::InvalidCharData(e, pos) => E::InvalidCharData(stream_error(e), at(pos)),
        E::UnknownToken(pos) => E::UnknownToken(at(pos)),
    }
}

/// The byte index in `text` of a line and a column as the tokenizer counts
/// them, both from 1: the line is one more than the number of newlines
/// before the index, and the column one more than the number of characters
/// between the last of these newlines and the index.
fn text_pos_index(text: &str, pos: TextPos) -> usize {
    let line_start = match (pos.row as usize).checked_sub(2) {
        Some(newlines) => match text.match_indices('\n').nth(newlines) {
            Some((newline, _)) => newline + 1,
            None => return text.len(),
        },
        None => 0,
    };
    text[line_start..]
        .char_indices()
        .nth((pos.col as usize).saturating_sub(1))
        .map_or(text.len(), |(index, _)| line_start + index)
}

/// Whether Namespaces in XML 1.0 allows a declaration of `prefix`, or of the
/// default namespace if it is empty, for `namespace`
/// (https://www.w3.org/TR/xml-names/#xmlReserved): the prefix `xmlns` cannot
/// be declared; the prefix `xml` can only be bound to its own namespace, and
/// no other prefix, nor the default namespace, to that namespace or to the
/// namespace of `xmlns`; and only the default namespace can be undeclared
/// (https://www.w3.org/TR/xml-names/#nsc-NoPrefixUndecl).
pub(crate) fn is_allowed_declaration(prefix: &str, namespace: &str) -> bool {
    match prefix {
        "xmlns" => false,
        "xml" => namespace == XML_NAMESPACE,
        _ => {
            namespace != XML_NAMESPACE
                && namespace != XMLNS_NAMESPACE
                && (prefix.is_empty() || !namespace.is_empty())
        }
    }
}

fn normalize_xml_id(value: &str) -> String {
    // strip both leading and trailing space characters
    let value = value.strip_prefix(' ').unwrap_or(value);
    let value = value.strip_suffix(' ').unwrap_or(value);
    // now take any repeated sequences of the space character ' ' and normalize
    // it to a single space character
    let mut result = String::with_capacity(value.len());
    let mut last_char_space = false;

    for c in value.chars() {
        if c == ' ' {
            if !last_char_space {
                result.push(c);
                last_char_space = true;
            }
        } else {
            result.push(c);
            last_char_space = false;
        }
    }
    result
}

#[cfg(test)]
mod tests;
