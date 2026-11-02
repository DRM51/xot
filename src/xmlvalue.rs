use std::borrow::Cow;
use std::fmt::Debug;

use ahash::AHashMap;

use crate::error::Error;
use crate::id::{NameId, NamespaceId, PrefixId};
use crate::textbuf::TextBuf;

/// The type of the XML node.
///
/// Access it using [`Value::value_type`] or
/// [`Xot::value_type`](crate::xotdata::Xot::value_type).
///
///    
/// The `ValueType` can be used if you are interested in
/// the type of the value without needing to match on it.
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum ValueType {
    /// Document that holds everything. Note that this not the same as the
    /// document element.
    Document,
    /// Element; it has a name, attributes and namespace information.
    Element,
    /// Text. You can get and set the text value.
    Text,
    /// Processing instruction
    ProcessingInstruction,
    /// Comment.
    Comment,
    /// Attribute
    Attribute,
    /// Namespace
    Namespace,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub(crate) enum ValueCategory {
    Normal,
    Attribute,
    Namespace,
}

/// An XML value.
///
/// Read it using [`Xot::view`](crate::xotdata::Xot::view), which gives a
/// [`NodeView`], or change it using
/// [`Xot::value_mut`](crate::xotdata::Xot::value_mut).
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum Value {
    /// Document that holds everything. Note that this not the same as the document
    /// element.
    Document,
    /// Element; it has a name, attributes and namespace information.
    Element(Element),
    /// Text. You can get and set the text value.
    Text(Text),
    /// Processing instruction.
    ProcessingInstruction(ProcessingInstruction),
    /// Comment.
    Comment(Comment),
    /// Attribute
    Attribute(Attribute),
    /// Namespace
    Namespace(Namespace),
}

impl Value {
    /// Returns the type of the XML value.
    pub fn value_type(&self) -> ValueType {
        match self {
            Value::Document => ValueType::Document,
            Value::Element(_) => ValueType::Element,
            Value::Text(_) => ValueType::Text,
            Value::Comment(_) => ValueType::Comment,
            Value::ProcessingInstruction(_) => ValueType::ProcessingInstruction,
            Value::Attribute(_) => ValueType::Attribute,
            Value::Namespace(_) => ValueType::Namespace,
        }
    }

    /// The value as a [`NodeView`]
    #[inline]
    pub(crate) fn view(&self) -> NodeView<'_> {
        match self {
            Value::Document => NodeView::Document,
            Value::Element(element) => NodeView::Element(element.name_id),
            Value::Text(text) => NodeView::Text(text.get()),
            Value::Comment(comment) => NodeView::Comment(comment.get()),
            Value::ProcessingInstruction(pi) => {
                NodeView::ProcessingInstruction(pi.target, pi.data.as_deref())
            }
            Value::Attribute(attribute) => {
                NodeView::Attribute(attribute.name_id, attribute.value.as_str())
            }
            Value::Namespace(namespace) => {
                NodeView::Namespace(namespace.prefix_id, namespace.namespace_id)
            }
        }
    }

    pub(crate) fn value_category(&self) -> ValueCategory {
        match self {
            Value::Document
            | Value::Element(_)
            | Value::Text(_)
            | Value::ProcessingInstruction(_)
            | Value::Comment(_) => ValueCategory::Normal,
            Value::Attribute(_) => ValueCategory::Attribute,
            Value::Namespace(_) => ValueCategory::Namespace,
        }
    }

    pub(crate) fn is_normal(&self) -> bool {
        matches!(
            self,
            Value::Document
                | Value::Element(_)
                | Value::Text(_)
                | Value::ProcessingInstruction(_)
                | Value::Comment(_)
        )
    }
}

/// A read of the value of a node, by value.
///
/// Obtained with [`Xot::view`](crate::Xot::view). The variants carry what a
/// node holds: names and ids by value, text, comment, processing instruction
/// data and attribute values as string slices that borrow from the tree. A
/// `NodeView` is `Copy`, so it can be matched, compared and handed on freely.
///
/// ```rust
/// use xot::{NodeView, Xot};
///
/// let mut xot = Xot::new();
/// let root = xot.parse(r#"<doc a="A">Example</doc>"#)?;
/// let doc_el = xot.document_element(root).unwrap();
/// let text_node = xot.first_child(doc_el).unwrap();
///
/// assert_eq!(xot.view(root), NodeView::Document);
/// assert_eq!(xot.view(text_node), NodeView::Text("Example"));
/// match xot.view(doc_el) {
///     NodeView::Element(name) => assert_eq!(xot.local_name_str(name), "doc"),
///     _ => unreachable!(),
/// }
/// # Ok::<(), xot::Error>(())
/// ```
///
/// To change a node, use [`Xot::value_mut`](crate::Xot::value_mut) or the
/// specific mutable accessors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NodeView<'a> {
    /// The document node.
    Document,
    /// An element, with its name.
    Element(NameId),
    /// Text.
    Text(&'a str),
    /// A comment, with its text.
    Comment(&'a str),
    /// A processing instruction, with its target and its data.
    ProcessingInstruction(NameId, Option<&'a str>),
    /// An attribute node, with its name and value.
    Attribute(NameId, &'a str),
    /// A namespace node, with its prefix and the namespace it binds.
    Namespace(PrefixId, NamespaceId),
}

impl NodeView<'_> {
    /// An owned [`Value`] with what this view holds.
    ///
    /// Use it to keep a copy of the value of a node, or to give another node
    /// the same value with [`Xot::value_mut`](crate::Xot::value_mut), which
    /// a read by value cannot do itself:
    ///
    /// ```rust
    /// use xot::{Value, Xot};
    ///
    /// let mut xot = Xot::new();
    /// let root = xot.parse("<doc><a/><b/></doc>")?;
    /// let doc_el = xot.document_element(root).unwrap();
    /// let a = xot.first_child(doc_el).unwrap();
    /// let b = xot.next_sibling(a).unwrap();
    ///
    /// let value: Value = xot.view(a).to_value();
    /// *xot.value_mut(b) = value;
    /// assert_eq!(xot.to_string(root)?, "<doc><a/><a/></doc>");
    /// # Ok::<(), xot::Error>(())
    /// ```
    pub fn to_value(&self) -> Value {
        match *self {
            NodeView::Document => Value::Document,
            NodeView::Element(name_id) => Value::Element(Element::new(name_id)),
            NodeView::Text(text) => Value::Text(Text::new_from_str(text)),
            NodeView::Comment(text) => Value::Comment(Comment::new(text.to_string())),
            NodeView::ProcessingInstruction(target, data) => Value::ProcessingInstruction(
                ProcessingInstruction::new(target, data.map(str::to_string)),
            ),
            NodeView::Attribute(name_id, value) => Value::Attribute(Attribute {
                name_id,
                value: value.to_string(),
            }),
            NodeView::Namespace(prefix_id, namespace_id) => Value::Namespace(Namespace {
                prefix_id,
                namespace_id,
            }),
        }
    }

    /// The [`ValueType`] of the node that was read.
    pub fn value_type(&self) -> ValueType {
        match self {
            NodeView::Document => ValueType::Document,
            NodeView::Element(_) => ValueType::Element,
            NodeView::Text(_) => ValueType::Text,
            NodeView::Comment(_) => ValueType::Comment,
            NodeView::ProcessingInstruction(..) => ValueType::ProcessingInstruction,
            NodeView::Attribute(..) => ValueType::Attribute,
            NodeView::Namespace(..) => ValueType::Namespace,
        }
    }
}

/// A map of PrefixId to NamespaceId for namespace tracking.
///
/// This is a real hash map, thus providing constant time access and does not
/// preserve order information.
///
/// It is used to return namespace information from various APIs.
pub type Prefixes = AHashMap<PrefixId, NamespaceId>;

/// XML element name.
///
/// Does not include namespace or attribute information;
/// this is kept in the tree.
///
/// Example: `<foo/>`.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct Element {
    pub(crate) name_id: NameId,
}

impl Element {
    pub(crate) fn new(name_id: NameId) -> Self {
        Self { name_id }
    }

    /// The name of the element.
    ///
    /// ```rust
    /// use xot::Xot;
    ///
    /// let mut xot = Xot::new();
    /// let name_doc = xot.add_name("doc");
    ///
    /// let root = xot.parse("<doc/>")?;
    /// let doc_el = xot.document_element(root).unwrap();
    /// let element = xot.element(doc_el).unwrap();
    /// assert_eq!(element.name(), name_doc);
    ///
    /// # Ok::<(), xot::Error>(())
    /// ```
    pub fn name(&self) -> NameId {
        self.name_id
    }

    pub(crate) fn set_name(&mut self, name_id: NameId) {
        self.name_id = name_id;
    }
}

/// A change of the value of an element node, from
/// [`Xot::element_mut`](crate::Xot::element_mut).
///
/// An [`Element`] read with [`Xot::element`](crate::Xot::element) is a copy
/// and has no setter, so that a change cannot go to the copy instead of the
/// tree:
///
/// ```rust,compile_fail
/// use xot::Xot;
///
/// let mut xot = Xot::new();
/// let changed = xot.add_name("changed");
/// let root = xot.parse("<doc/>").unwrap();
/// let doc_el = xot.document_element(root).unwrap();
/// // does not compile: an Element read by value cannot be changed
/// xot.element(doc_el).unwrap().set_name(changed);
/// ```
///
/// Change it through this type instead, or with
/// [`Xot::set_element_name`](crate::Xot::set_element_name).
#[derive(Debug)]
pub struct MutableElement<'a> {
    element: &'a mut Element,
}

impl<'a> MutableElement<'a> {
    pub(crate) fn new(element: &'a mut Element) -> Self {
        Self { element }
    }

    /// The name of the element.
    pub fn name(&self) -> NameId {
        self.element.name_id
    }

    /// Set the name of the element.
    pub fn set_name(&mut self, name_id: NameId) {
        self.element.set_name(name_id);
    }
}

/// XML text value.
///
/// Example: `Bar` in `<foo>Bar</foo>`, or `hello` and `world` in `<greeting>hello<sep/>world</greeting>`.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct Text {
    pub(crate) text: TextBuf,
}

impl Text {
    pub(crate) fn new(text: String) -> Self {
        Text {
            text: TextBuf::from_string(text),
        }
    }

    pub(crate) fn new_from_str(text: &str) -> Self {
        Text {
            text: TextBuf::new(text),
        }
    }

    /// The text `text`, in place if it fits, else in the `String` that
    /// `text` has or a new one.
    pub(crate) fn from_cow(text: Cow<'_, str>) -> Self {
        match text {
            Cow::Borrowed(text) => Self::new_from_str(text),
            Cow::Owned(text) => Self::new(text),
        }
    }

    /// Get the text value.
    ///
    /// See [`Xot::text_str`](`crate::Xot::text_str`) and [`Xot::text_content_str`](`crate::Xot::text_content_str`) for
    /// more convenient ways to get text values.
    #[inline]
    pub fn get(&self) -> &str {
        self.text.as_str()
    }

    /// Get the text value, as a mutable reference.
    ///
    /// A short text is kept in the node itself; this moves it to a `String`
    /// of its own first, which the node then keeps until the next
    /// [`Text::set`].
    pub fn get_mut(&mut self) -> &mut String {
        self.text.get_mut()
    }

    /// Set the text value.
    ///
    /// ```rust
    /// use xot::Xot;
    ///
    /// let mut xot = Xot::new();
    /// let root = xot.parse(r#"<doc>Example</doc>"#)?;
    /// let doc_el = xot.document_element(root).unwrap();
    /// let text_node = xot.first_child(doc_el).unwrap();
    ///
    /// let text = xot.text_mut(text_node).unwrap();
    /// text.set("New text");
    ///
    /// assert_eq!(xot.to_string(root).unwrap(), r#"<doc>New text</doc>"#);
    /// # Ok::<(), xot::Error>(())
    /// ```
    pub fn set<S: Into<String>>(&mut self, text: S) {
        self.text = TextBuf::from_string(text.into());
    }
}

/// XML comment.
///
/// Example: `<!-- foo -->`.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct Comment {
    pub(crate) text: String,
}

impl Comment {
    pub(crate) fn new(text: String) -> Self {
        Comment { text }
    }

    /// Get the comment text.
    pub fn get(&self) -> &str {
        &self.text
    }

    /// Set the comment text.
    ///
    /// Rejects comments that contain `--` as illegal.
    pub fn set<S: Into<String>>(&mut self, text: S) -> Result<(), Error> {
        let text = text.into();
        if text.contains("--") {
            return Err(Error::InvalidComment(text));
        }
        self.text = text;
        Ok(())
    }
}

/// XML processing instruction value.
///
/// Example: `<?foo?>` or `<?foo bar?>`.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct ProcessingInstruction {
    pub(crate) target: NameId,
    pub(crate) data: Option<String>,
}

impl ProcessingInstruction {
    pub(crate) fn new(target: NameId, data: Option<String>) -> Self {
        ProcessingInstruction { target, data }
    }

    /// Get processing instruction target.
    pub fn target(&self) -> NameId {
        self.target
    }

    /// Get processing instruction data.
    pub fn data(&self) -> Option<&str> {
        self.data.as_deref()
    }

    /// Set target
    pub fn set_target<S: Into<String>>(&mut self, target: NameId) -> Result<(), Error> {
        // XXX Ideally check that name follows XML spec
        self.target = target;
        Ok(())
    }

    /// Set data.
    pub fn set_data<S: Into<String>>(&mut self, data: Option<S>) {
        // XXX Ideally check that data follows XML spec, i.e. not contain
        // "?>".
        if let Some(data) = data {
            let data = data.into();
            if !data.is_empty() {
                self.data = Some(data);
                return;
            }
        }
        self.data = None;
    }
}

/// Represents a namespace node.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct Namespace {
    pub(crate) prefix_id: PrefixId,
    pub(crate) namespace_id: NamespaceId,
}

impl Namespace {
    /// Get prefix
    pub fn prefix(&self) -> PrefixId {
        self.prefix_id
    }

    /// Get namespace
    pub fn namespace(&self) -> NamespaceId {
        self.namespace_id
    }
}

/// A change of the value of a namespace node, from
/// [`Xot::namespace_node_mut`](crate::Xot::namespace_node_mut).
///
/// A [`Namespace`] read with
/// [`Xot::namespace_node`](crate::Xot::namespace_node) is a copy and has no
/// setter, so that a change cannot go to the copy instead of the tree:
///
/// ```rust,compile_fail
/// use xot::Xot;
///
/// let mut xot = Xot::new();
/// let other = xot.add_namespace("urn:example:other");
/// let root = xot.parse(r#"<doc xmlns:p="urn:example:p"/>"#).unwrap();
/// let doc_el = xot.document_element(root).unwrap();
/// let node = xot.namespaces(doc_el).nodes().next().unwrap();
/// // does not compile: a Namespace read by value cannot be changed
/// xot.namespace_node(node).unwrap().set_namespace(other);
/// ```
#[derive(Debug)]
pub struct MutableNamespace<'a> {
    namespace: &'a mut Namespace,
}

impl<'a> MutableNamespace<'a> {
    pub(crate) fn new(namespace: &'a mut Namespace) -> Self {
        Self { namespace }
    }

    /// Get prefix
    pub fn prefix(&self) -> PrefixId {
        self.namespace.prefix_id
    }

    /// Get namespace
    pub fn namespace(&self) -> NamespaceId {
        self.namespace.namespace_id
    }

    /// Set namespace id
    pub fn set_namespace(&mut self, namespace_id: NamespaceId) {
        self.namespace.namespace_id = namespace_id;
    }
}

/// Represents an attribute node.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct Attribute {
    pub(crate) name_id: NameId,
    pub(crate) value: String,
}

impl Attribute {
    /// Get name
    pub fn name(&self) -> NameId {
        self.name_id
    }

    /// Get value
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Set value
    pub fn set_value<S: Into<String>>(&mut self, value: S) {
        self.value = value.into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xotdata::Xot;

    #[test]
    fn test_element_hashable_name() {
        let mut xot = Xot::new();
        let a = xot.add_name("a");
        let b = xot.add_name("b");

        let alpha = Element { name_id: a };
        let beta = Element { name_id: a };
        let gamma = Element { name_id: b };

        let hash_builder = ahash::RandomState::with_seed(42);
        let alpha_hash = hash_builder.hash_one(alpha);
        let beta_hash = hash_builder.hash_one(beta);
        let gamma_hash = hash_builder.hash_one(gamma);
        assert_eq!(alpha_hash, beta_hash);
        assert_ne!(alpha_hash, gamma_hash);
    }
}
