use crate::access::NodeEdge;
use crate::xmlvalue::{
    Attribute, Comment, Element, MutableElement, MutableNamespace, Namespace, NodeView,
    ProcessingInstruction, Text, Value, ValueType,
};
use crate::xotdata::{Node, Xot};
use crate::NameId;

/// ## Value and type access
impl Xot {
    /// Read the value of this node, by value.
    ///
    /// The result is a [`NodeView`]: the kind of the node with what it holds,
    /// names and ids by value and text, comments, processing instruction data
    /// and attribute values as string slices.
    ///
    /// ```rust
    /// use xot::{NodeView, Xot};
    ///
    /// let mut xot = Xot::new();
    ///
    /// let root = xot.parse("<doc>Example</doc>")?;
    /// let doc_el = xot.document_element(root).unwrap();
    /// let doc_name = xot.name("doc").unwrap();
    ///
    /// match xot.view(doc_el) {
    ///    NodeView::Element(name) => {
    ///       assert_eq!(name, doc_name);
    ///   }
    ///   _ => { }
    /// }
    /// # Ok::<(), xot::Error>(())
    /// ```
    ///
    /// Note that if you already know the type of a node value or are
    /// only interested in a single type, you can use the convenience
    /// methods like [`Xot::element`].
    #[inline]
    pub fn view(&self, node: Node) -> NodeView<'_> {
        self.value(node).view()
    }

    /// The stored value of this node, for use inside xot.
    #[inline]
    pub(crate) fn value(&self, node_id: Node) -> &Value {
        self.arena.get(node_id.get())
    }

    /// Mutable access to the XML value for this node.
    ///
    /// ```rust
    /// use xot::{Xot, Value};
    ///
    /// let mut xot = Xot::new();
    ///
    /// let root = xot.parse("<doc>Example</doc>")?;
    /// let doc_el = xot.document_element(root).unwrap();
    /// let text_node = xot.first_child(doc_el).unwrap();
    ///
    /// match xot.value_mut(doc_el) {
    ///    Value::Text(text) => {
    ///       text.set("Changed");
    ///   }
    ///   _ => { }
    /// }
    ///
    /// # Ok::<(), xot::Error>(())
    /// ```
    ///
    /// Note that if you already know the type of a node value or are
    /// only interested in a single type, you can use the convenience
    /// methods like [`Xot::text_mut`]
    #[inline]
    pub fn value_mut(&mut self, node_id: Node) -> &mut Value {
        self.arena.get_mut(node_id.get())
    }

    /// Get the [`ValueType`](crate::xmlvalue::ValueType) of a node.
    #[inline]
    pub fn value_type(&self, node: Node) -> ValueType {
        self.value(node).value_type()
    }

    /// Return true if node is directly under the document node.
    /// This means it's either the document element or a comment or
    /// processing instruction.
    ///
    /// ```rust
    /// use xot::Xot;
    ///
    /// let mut xot = Xot::new();
    /// let root = xot.parse("<doc>Example</doc>")?;
    /// let doc_el = xot.document_element(root).unwrap();
    /// let text_node = xot.first_child(doc_el).unwrap();
    ///
    /// assert!(xot.has_document_parent(doc_el));
    /// assert!(!xot.has_document_parent(text_node));
    ///
    /// # Ok::<(), xot::Error>(())
    /// ```
    pub fn has_document_parent(&self, node: Node) -> bool {
        if let Some(parent_id) = self.parent(node) {
            self.value_type(parent_id) == ValueType::Document
        } else {
            false
        }
    }

    /// Return true if the node is the document element.
    ///
    /// ```rust
    /// use xot::Xot;
    ///
    /// let mut xot = Xot::new();
    /// let root = xot.parse("<doc>Example</doc>")?;
    /// let doc_el = xot.document_element(root).unwrap();
    /// let text_node = xot.first_child(doc_el).unwrap();
    ///
    /// assert!(xot.is_document_element(doc_el));
    /// assert!(!xot.is_document_element(text_node));
    /// # Ok::<(), xot::Error>(())
    /// ```
    pub fn is_document_element(&self, node: Node) -> bool {
        if let Some(parent_id) = self.parent(node) {
            self.value_type(parent_id) == ValueType::Document
                && self.value_type(node) == ValueType::Element
        } else {
            false
        }
    }

    /// Return true if node is the document node.
    ///
    /// ```rust
    /// use xot::Xot;
    ///
    /// let mut xot = Xot::new();
    /// let root = xot.parse("<doc>Example</doc>")?;
    /// let doc_el = xot.document_element(root).unwrap();
    ///
    /// assert!(xot.is_document(root));
    /// assert!(!xot.is_document(doc_el));
    /// # Ok::<(), xot::Error>(())
    /// ```
    #[inline]
    pub fn is_document(&self, node: Node) -> bool {
        self.value_type(node) == ValueType::Document
    }

    /// Return true if node is an element.
    ///
    /// ```rust
    /// use xot::Xot;
    ///
    /// let mut xot = Xot::new();
    /// let root = xot.parse("<doc>Example</doc>")?;
    /// let doc_el = xot.document_element(root).unwrap();
    /// let text_node = xot.first_child(doc_el).unwrap();
    ///
    /// assert!(xot.is_element(doc_el));
    /// assert!(!xot.is_element(text_node));
    /// # Ok::<(), xot::Error>(())
    /// ```
    #[inline]
    pub fn is_element(&self, node: Node) -> bool {
        self.value_type(node) == ValueType::Element
    }

    /// Return true if node is text.
    ///
    /// ```rust
    /// use xot::Xot;
    ///
    /// let mut xot = Xot::new();
    /// let root = xot.parse("<doc>Example</doc>")?;
    /// let doc_el = xot.document_element(root).unwrap();
    /// let text_node = xot.first_child(doc_el).unwrap();
    ///
    /// assert!(xot.is_text(text_node));
    /// assert!(!xot.is_text(doc_el));
    ///
    /// # Ok::<(), xot::Error>(())
    /// ```
    #[inline]
    pub fn is_text(&self, node: Node) -> bool {
        self.value_type(node) == ValueType::Text
    }

    /// Return true if node is a comment.
    #[inline]
    pub fn is_comment(&self, node: Node) -> bool {
        self.value_type(node) == ValueType::Comment
    }

    /// Return true if node is a processing instruction.
    #[inline]
    pub fn is_processing_instruction(&self, node: Node) -> bool {
        self.value_type(node) == ValueType::ProcessingInstruction
    }

    /// Return true if node is a namespace node.
    #[inline]
    pub fn is_namespace_node(&self, node: Node) -> bool {
        self.value_type(node) == ValueType::Namespace
    }

    /// Return true if node is an attribute node.
    #[inline]
    pub fn is_attribute_node(&self, node: Node) -> bool {
        self.value_type(node) == ValueType::Attribute
    }

    /// If this node's value is text, return the string.
    ///
    /// See also [`Xot::text_mut()`] if you want to manipulate
    /// a text value.
    ///
    /// ```rust
    /// use xot::Xot;
    ///
    /// let mut xot = Xot::new();
    /// let root = xot.parse("<doc>Example</doc>")?;
    /// let doc_el = xot.document_element(root).unwrap();
    /// let text_node = xot.first_child(doc_el).unwrap();
    ///
    /// assert_eq!(xot.text_str(text_node).unwrap(), "Example");
    /// assert!(xot.text_str(doc_el).is_none());
    ///
    /// # Ok::<(), xot::Error>(())
    /// ```
    #[inline]
    pub fn text_str(&self, node: Node) -> Option<&str> {
        match self.value(node) {
            Value::Text(text) => Some(text.get()),
            _ => None,
        }
    }

    /// If this node's value is a text, return a mutable reference to it.
    ///
    /// This can be used to manipulate the text content of a
    /// document.
    ///
    /// ```rust
    ///
    /// use xot::Xot;
    ///
    /// let mut xot = Xot::new();
    /// let root = xot.parse("<doc>Example</doc>")?;
    /// let doc_el = xot.document_element(root)?;
    /// let text_node = xot.first_child(doc_el).unwrap();
    ///
    /// let text = xot.text_mut(text_node).unwrap();
    ///
    /// text.set("New text");
    ///
    /// assert_eq!(xot.text_str(text_node).unwrap(), "New text");
    /// assert_eq!(xot.to_string(root)?, "<doc>New text</doc>");
    /// # Ok::<(), xot::Error>(())
    /// ```
    #[inline]
    pub fn text_mut(&mut self, node: Node) -> Option<&mut Text> {
        let xml_node = self.value_mut(node);
        if let Value::Text(text) = xml_node {
            Some(text)
        } else {
            None
        }
    }

    /// Get the name of a node that's an element.
    ///
    /// If the node does not exist, then panic.
    pub fn get_element_name(&self, node: Node) -> NameId {
        match self.value(node) {
            Value::Element(element) => element.name(),
            _ => panic!("Node is not an element"),
        }
    }

    /// If this node's value is an element, return it.
    ///
    /// The [`Element`] is returned by value; it holds the name of the
    /// element. It is a copy, without a setter: to change the name, use
    /// [`Xot::element_mut()`] or [`Xot::set_element_name()`].
    ///
    /// ```rust
    /// use xot::Xot;
    ///
    /// let mut xot = Xot::new();
    /// let root = xot.parse(r#"<doc><child a="A"/></doc>"#)?;
    /// let doc_el = xot.document_element(root).unwrap();
    /// let child_el = xot.first_child(doc_el).unwrap();
    ///
    /// let element = xot.element(child_el).unwrap();
    ///
    /// let child_name = xot.name("child").unwrap();
    /// assert_eq!(element.name(), child_name);
    ///
    /// # Ok::<(), xot::Error>(())
    /// ```
    #[inline]
    pub fn element(&self, node: Node) -> Option<Element> {
        match self.value(node) {
            Value::Element(element) => Some(*element),
            _ => None,
        }
    }

    /// If this node's value is an element, return a [`MutableElement`] to
    /// change it.
    ///
    /// You can use this to change an element's name.
    ///
    /// ```rust
    /// use xot::Xot;
    ///
    /// let mut xot = Xot::new();
    /// let changed = xot.add_name("changed");
    /// let root = xot.parse(r#"<doc><child/></doc>"#)?;
    /// let doc_el = xot.document_element(root)?;
    /// let child_el = xot.first_child(doc_el).unwrap();
    ///
    /// let mut element = xot.element_mut(child_el).unwrap();
    /// element.set_name(changed);
    ///
    /// assert_eq!(xot.to_string(root)?, r#"<doc><changed/></doc>"#);
    ///
    /// # Ok::<(), xot::Error>(())
    /// ```
    #[inline]
    pub fn element_mut(&mut self, node: Node) -> Option<MutableElement<'_>> {
        let xml_node = self.value_mut(node);
        if let Value::Element(element) = xml_node {
            Some(MutableElement::new(element))
        } else {
            None
        }
    }

    /// If this element has only a single text child, return a mutable reference to it.
    ///
    /// If the element has no children, create a text child and return multiple reference
    /// to its value.
    ///
    /// If the element more than one child, return `None`.
    ///
    /// ```rust
    /// use xot::Xot;
    ///
    /// let mut xot = Xot::new();
    /// let root = xot.parse("<doc><a>Example</a><b/></doc>")?;
    /// let doc_el = xot.document_element(root).unwrap();
    /// let a_el = xot.first_child(doc_el).unwrap();
    /// let b_el = xot.next_sibling(a_el).unwrap();
    ///
    /// let text = xot.text_content_mut(a_el).unwrap();
    /// text.set("New value");
    ///
    /// assert_eq!(xot.to_string(root)?, "<doc><a>New value</a><b/></doc>");
    ///
    ///
    /// let text = xot.text_content_mut(b_el).unwrap();
    /// text.set("New value 2");
    ///
    /// assert_eq!(xot.to_string(root)?, "<doc><a>New value</a><b>New value 2</b></doc>");
    ///  
    ///
    /// # Ok::<(), xot::Error>(())
    /// ```
    pub fn text_content_mut(&mut self, node: Node) -> Option<&mut Text> {
        if let Some(child) = self.first_child(node) {
            if self.next_sibling(child).is_some() {
                return None;
            }
            if let Some(text) = self.text_mut(child) {
                return Some(text);
            }
        } else if self.value_type(node) == ValueType::Element {
            self.append_text(node, "").unwrap();
            let child = self.first_child(node).unwrap();
            return self.text_mut(child);
        }
        None
    }

    /// If this element only has a single text child, return str reference to it.
    ///
    /// If the element has no content, return `Some("")`.
    ///
    /// If the element has more than one child, return `None`.
    ///
    /// ```rust
    /// use xot::Xot;
    ///
    /// let mut xot = Xot::new();
    /// let root = xot.parse("<doc><a>Example</a><b/><c><x/></c></doc>")?;
    /// let doc_el = xot.document_element(root).unwrap();
    /// let a_el = xot.first_child(doc_el).unwrap();
    /// let b_el = xot.next_sibling(a_el).unwrap();
    /// let c_el = xot.next_sibling(b_el).unwrap();
    ///
    /// let text = xot.text_content_str(a_el).unwrap();
    /// assert_eq!(text, "Example");
    /// let text = xot.text_content_str(b_el).unwrap();
    /// assert_eq!(text, "");
    /// assert!(xot.text_content_str(c_el).is_none());
    ///
    /// # Ok::<(), xot::Error>(())
    /// ```
    pub fn text_content_str(&self, node: Node) -> Option<&str> {
        let Some(child) = self.first_child(node) else {
            return Some("");
        };
        if self.next_sibling(child).is_some() {
            return None;
        }
        self.text_str(child)
    }

    /// If this node's value is a comment, return the string.
    #[inline]
    pub fn comment_str(&self, node: Node) -> Option<&str> {
        match self.value(node) {
            Value::Comment(comment) => Some(comment.get()),
            _ => None,
        }
    }

    /// If this node's value is a comment, return a mutable reference to it.
    #[inline]
    pub fn comment_mut(&mut self, node: Node) -> Option<&mut Comment> {
        let xml_node = self.value_mut(node);
        if let Value::Comment(comment) = xml_node {
            Some(comment)
        } else {
            None
        }
    }

    /// If this node's value is a processing instruction, return its target
    /// and its data.
    ///
    /// ```rust
    /// use xot::Xot;
    ///
    /// let mut xot = Xot::new();
    /// let root = xot.parse("<doc><?pi data?></doc>")?;
    /// let doc_el = xot.document_element(root).unwrap();
    /// let pi_node = xot.first_child(doc_el).unwrap();
    ///
    /// let (target, data) = xot.processing_instruction(pi_node).unwrap();
    /// assert_eq!(xot.local_name_str(target), "pi");
    /// assert_eq!(data, Some("data"));
    /// assert!(xot.processing_instruction(doc_el).is_none());
    /// # Ok::<(), xot::Error>(())
    /// ```
    #[inline]
    pub fn processing_instruction(&self, node: Node) -> Option<(NameId, Option<&str>)> {
        match self.value(node) {
            Value::ProcessingInstruction(pi) => Some((pi.target(), pi.data())),
            _ => None,
        }
    }

    /// If this node's value is a processing instruction, return a mutable reference to it.
    #[inline]
    pub fn processing_instruction_mut(&mut self, node: Node) -> Option<&mut ProcessingInstruction> {
        let xml_node = self.value_mut(node);
        if let Value::ProcessingInstruction(pi) = xml_node {
            Some(pi)
        } else {
            None
        }
    }

    /// If this node's value is a namespace node, return it.
    ///
    /// The [`Namespace`] is returned by value; it holds the prefix and the
    /// namespace. It is a copy, without a setter: to change the namespace,
    /// use [`Xot::namespace_node_mut()`].
    #[inline]
    pub fn namespace_node(&self, node: Node) -> Option<Namespace> {
        match self.value(node) {
            Value::Namespace(namespace) => Some(*namespace),
            _ => None,
        }
    }

    /// If this node's value is a namespace node, return a
    /// [`MutableNamespace`] to change it.
    #[inline]
    pub fn namespace_node_mut(&mut self, node: Node) -> Option<MutableNamespace<'_>> {
        let xml_node = self.value_mut(node);
        if let Value::Namespace(namespace) = xml_node {
            Some(MutableNamespace::new(namespace))
        } else {
            None
        }
    }

    /// If this node's value is an attribute node, return its name and its
    /// value.
    ///
    /// ```rust
    /// use xot::Xot;
    ///
    /// let mut xot = Xot::new();
    /// let root = xot.parse(r#"<doc a="A"/>"#)?;
    /// let doc_el = xot.document_element(root).unwrap();
    /// let a = xot.name("a").unwrap();
    /// let attribute_node = xot.attributes(doc_el).get_node(a).unwrap();
    ///
    /// assert_eq!(xot.attribute_node(attribute_node), Some((a, "A")));
    /// assert!(xot.attribute_node(doc_el).is_none());
    /// # Ok::<(), xot::Error>(())
    /// ```
    #[inline]
    pub fn attribute_node(&self, node: Node) -> Option<(NameId, &str)> {
        match self.value(node) {
            Value::Attribute(attribute) => Some((attribute.name(), attribute.value())),
            _ => None,
        }
    }

    /// Manipulate attribute node value
    #[inline]
    pub fn attribute_node_mut(&mut self, node: Node) -> Option<&mut Attribute> {
        let xml_node = self.value_mut(node);
        if let Value::Attribute(attribute) = xml_node {
            Some(attribute)
        } else {
            None
        }
    }

    /// Given a node, give back a string representation.
    ///
    /// For the root node and element nodes this gives back all text node
    /// descendant content, concatenated.
    ///
    /// For text nodes, it gives back the text.
    ///
    /// For comments, it gives back the comment text.
    ///
    /// For processing instructions, it gives back their content (data).
    ///
    /// For attribute nodes, it gives back the attribute value.
    ///
    /// For namespace nodes, it gives back the namespace URI.
    ///
    /// This is defined by the `string-value` property in
    /// <https://www.w3.org/TR/xpath-datamodel-31>
    pub fn string_value(&self, node: Node) -> String {
        match self.value(node) {
            Value::Document | Value::Element(_) => descendants_to_string(self, node),
            Value::Text(text) => text.get().to_string(),
            Value::ProcessingInstruction(pi) => pi.data().unwrap_or("").to_string(),
            Value::Comment(comment) => comment.get().to_string(),
            Value::Attribute(attribute) => attribute.value().to_string(),
            Value::Namespace(namespace) => {
                let namespace_id = namespace.namespace();
                self.namespace_str(namespace_id).to_string()
            }
        }
    }

    /// Check two nodes for semantic equality.
    ///
    /// This is a deep comparison of the nodes and their children.
    /// The trees have to have the same structure.
    ///
    /// A name is considered to be semantically equal to another name if
    /// they have the same namespace and local name. Prefixes are ignored.
    ///
    /// Two elements are the same if their name and attributes are the same. Namespace
    /// declarations are ignored.
    ///
    /// Text nodes, comments and processing instructions are considered to be the
    /// same if their values are the same.
    ///
    /// Compare two documents:
    ///
    /// ```rust
    /// use xot::Xot;
    ///
    /// let mut xot = Xot::new();
    /// let root0 = xot.parse("<doc><a>Example</a><b/></doc>")?;
    /// let root1 = xot.parse("<doc><a>Example</a><b/></doc>")?;
    ///
    /// assert!(xot.deep_equal(root0, root1));
    ///
    /// # Ok::<(), xot::Error>(())
    /// ```
    ///
    /// Different prefixes are ignored; the namespace URI is
    /// what is compared:
    ///
    /// ```rust
    /// use xot::Xot;
    ///
    /// let mut xot = Xot::new();
    /// let root0 = xot.parse("<doc xmlns:foo='http://example.com'><foo:a/></doc>")?;
    /// let root1 = xot.parse("<doc xmlns:bar='http://example.com'><bar:a/></doc>")?;
    ///
    /// assert!(xot.deep_equal(root0, root1));
    /// # Ok::<(), xot::Error>(())
    /// ```
    ///
    /// But different text is a real difference:
    ///
    /// ```rust
    /// use xot::Xot;
    ///
    /// let mut xot = Xot::new();
    /// let root0 = xot.parse("<doc>Example</doc>")?;
    /// let root1 = xot.parse("<doc>Changed</doc>")?;
    ///
    /// assert!(!xot.deep_equal(root0, root1));
    /// # Ok::<(), xot::Error>(())
    /// ```
    ///
    /// You can compare any nodes, not just documents:
    ///
    /// ```rust
    /// use xot::Xot;
    ///
    /// let mut xot = Xot::new();
    /// let root = xot.parse(r#"<doc><a f="F"/><b/><a f="F"/></doc>"#)?;
    /// let doc_el = xot.first_child(root).unwrap();
    /// let a_el = xot.first_child(doc_el).unwrap();
    /// let b_el = xot.next_sibling(a_el).unwrap();
    /// let a2_el = xot.next_sibling(b_el).unwrap();
    ///
    /// assert!(xot.deep_equal(a_el, a2_el));
    /// assert!(!xot.deep_equal(a_el, b_el));
    ///
    /// # Ok::<(), xot::Error>(())
    /// ```
    pub fn deep_equal(&self, a: Node, b: Node) -> bool {
        self.advanced_deep_equal(a, b, |_| true, |a, b| a == b)
    }

    /// Compare the children of two nodes
    ///
    /// If the children are the same semantically, return true. It ignores
    /// the name and attributes of the `a` and `b` nodes themselves.
    pub fn deep_equal_children(&self, a: Node, b: Node) -> bool {
        let mut b_children = self.children(b);
        for a_child in self.children(a) {
            if let Some(b_child) = b_children.next() {
                // if the child is different, the element is different
                if !self.deep_equal(a_child, b_child) {
                    return false;
                }
            } else {
                // we cannot find a b child for an a child
                return false;
            }
        }
        b_children.next().is_none()
    }

    /// XPath deep equal
    /// Comparison of two nodes as defined by the XPath deep-equal function:
    ///
    /// <https://www.w3.org/TR/xpath-functions-31/#func-deep-equal>
    ///
    /// We ignore anything about typed content in that definition.
    pub fn deep_equal_xpath(
        &self,
        a: Node,
        b: Node,
        text_compare: impl Fn(&str, &str) -> bool,
    ) -> bool {
        // the top level comparison needs to compare the node, even if
        // processing instruction or a comment, though for elements, we want to
        // compare the structure and filter comments and processing
        // instructions out.
        use Value::*;
        match (self.value(a), self.value(b)) {
            (Element(_), Element(_)) | (Document, Document) => self.advanced_deep_equal(
                a,
                b,
                // we need to only consider elements and text nodes for
                // root/element content comparison
                |node| self.is_element(node) || self.is_text(node),
                text_compare,
            ),
            _ => self.advanced_compare_value(a, b, text_compare),
        }
    }

    /// Compare two nodes for semantic equality with custom text compare and
    /// filtering.
    ///
    /// This is a deep comparison of the nodes and their children. The trees
    /// have to have the same structure.
    ///
    /// A name is considered to be semantically equal to another name if they
    /// have the same namespace and local name. Prefixes are ignored.
    ///
    /// Two elements are the same if their name and attributes are the same.
    /// Namespace declarations are ignored.
    ///
    /// You can include only the nodes that are relevant to the comparison
    /// using the filter function.
    ///
    /// Text nodes and attributes are compared using the provided comparison function.
    pub fn advanced_deep_equal<F, C>(&self, a: Node, b: Node, filter: F, text_compare: C) -> bool
    where
        F: Fn(Node) -> bool,
        C: Fn(&str, &str) -> bool,
    {
        // the edges of `Xot::traverse`, each with the value of its node,
        // which the comparison of two starts takes
        let mut edges_a = self.traverse_values(a);
        let mut edges_b = self.traverse_values(b);
        // the pairs that `zip` of the two traversals with the filter gives,
        // in a loop of its own, so that the steps of both are taken in it
        while let Some(edge_a) = next_passing(&mut edges_a, &filter) {
            let Some(edge_b) = next_passing(&mut edges_b, &filter) else {
                break;
            };
            match (edge_a, edge_b) {
                ((NodeEdge::Start(a), a_value), (NodeEdge::Start(b), b_value)) => {
                    if !self.advanced_compare_values(a, a_value, b, b_value, &text_compare) {
                        return false;
                    }
                }
                ((NodeEdge::End(_a), _), (NodeEdge::End(_b), _)) => {
                    // If there is only a difference in structure, not value,
                    // the default case will fire
                }
                _ => {
                    return false;
                }
            }
        }
        // if we have leftover elements in the iterators, the trees are not equal
        if next_passing(&mut edges_a, &filter).is_some()
            || next_passing(&mut edges_b, &filter).is_some()
        {
            return false;
        }
        true
    }

    pub(crate) fn advanced_compare_value<C>(&self, a: Node, b: Node, text_compare: C) -> bool
    where
        C: Fn(&str, &str) -> bool,
    {
        self.advanced_compare_values(a, self.value(a), b, self.value(b), text_compare)
    }

    /// [`Xot::advanced_compare_value`] of `a` and `b`, whose values are
    /// `a_value` and `b_value`.
    #[inline]
    fn advanced_compare_values<C>(
        &self,
        a: Node,
        a_value: &Value,
        b: Node,
        b_value: &Value,
        text_compare: C,
    ) -> bool
    where
        C: Fn(&str, &str) -> bool,
    {
        match (a_value, b_value) {
            (Value::Document, Value::Document) => true,
            (Value::Element(a_element), Value::Element(b_element)) => {
                a_element.name() == b_element.name()
                    && self.advanced_compare_attributes(a, b, text_compare)
            }
            (Value::Text(a), Value::Text(b)) => text_compare(a.get(), b.get()),
            (Value::Comment(a), Value::Comment(b)) => a.get() == b.get(),
            (Value::ProcessingInstruction(a), Value::ProcessingInstruction(b)) => {
                if a.target() != b.target() {
                    return false;
                }
                match (a.data(), b.data()) {
                    (Some(a_content), Some(b_content)) => text_compare(a_content, b_content),
                    (None, None) => true,
                    _ => false,
                }
            }
            (Value::Attribute(a), Value::Attribute(b)) => {
                a.name() == b.name() && text_compare(a.value(), b.value())
            }
            (Value::Namespace(a), Value::Namespace(b)) => {
                a.prefix() == b.prefix() && a.namespace() == b.namespace()
            }
            _ => false,
        }
    }

    fn advanced_compare_attributes<C>(&self, a: Node, b: Node, text_compare: C) -> bool
    where
        C: Fn(&str, &str) -> bool,
    {
        let a_attributes = self.attributes(a);
        let b_attributes = self.attributes(b);
        if a_attributes.len() != b_attributes.len() {
            return false;
        }
        // if we can't find a value for a key in a in b, then we
        // know they aren't the same, given we already compared the length
        for (key, value_a) in a_attributes.iter() {
            let value_b = b_attributes.get(key);
            if let Some(value_b) = value_b {
                if !text_compare(value_a, value_b) {
                    return false;
                }
            } else {
                return false;
            }
        }
        true
    }

    /// Shallow compare two nodes.
    ///
    /// Does not consider content of any nodes, but does compare
    /// attributes.
    pub fn shallow_equal(&self, a: Node, b: Node) -> bool {
        self.shallow_equal_ignore_attributes(a, b, &[])
    }

    /// Shallow compare two nodes, with possibility to ignore attributes.
    ///
    /// Attributes of elements are compared, except those listed to ignore.
    ///
    /// Child content of the root or elements is not considered.
    pub fn shallow_equal_ignore_attributes(
        &self,
        a: Node,
        b: Node,
        ignore_attributes: &[NameId],
    ) -> bool {
        if let (Some(a_element), Some(b_element)) = (self.element(a), self.element(b)) {
            if a_element.name() != b_element.name() {
                return false;
            }

            // count the amount of attributes we compare
            let mut compare_attributes_count = 0;

            let a_attributes = self.attributes(a);
            let b_attributes = self.attributes(b);

            for (key, value_a) in a_attributes.iter() {
                if ignore_attributes.contains(&key) {
                    continue;
                }
                let value_b = b_attributes.get(key);
                if Some(value_a) != value_b {
                    return false;
                }
                compare_attributes_count += 1;
            }

            let mut b_ignore_attributes = 0;
            for ignore_attribute in ignore_attributes {
                if b_attributes.get(*ignore_attribute).is_some() {
                    b_ignore_attributes += 1;
                }
            }
            // we expect the amount of non-ignored attributes in a to
            // be the same as the amount of non-ignored attributes in b
            compare_attributes_count == b_attributes.len() - b_ignore_attributes
        } else {
            self.advanced_compare_value(a, b, |a, b| a == b)
        }
    }
}

fn descendants_to_string(xot: &Xot, node: Node) -> String {
    let texts = xot.descendants(node).filter_map(|n| xot.text_str(n));
    let (lower_bound, _) = texts.size_hint();
    let mut r = String::with_capacity(lower_bound);
    for text in texts {
        r.push_str(text);
    }
    r
}

/// The next edge of `edges` whose node passes `filter`, with its value.
#[inline(always)]
fn next_passing<'a>(
    edges: &mut impl Iterator<Item = (NodeEdge, &'a Value)>,
    filter: &impl Fn(Node) -> bool,
) -> Option<(NodeEdge, &'a Value)> {
    loop {
        let (edge, value) = edges.next()?;
        let (NodeEdge::Start(node) | NodeEdge::End(node)) = edge;
        if filter(node) {
            return Some((edge, value));
        }
    }
}
