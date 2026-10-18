// The outputs of a node, walked without a generator for each node.

use crate::access::NodeEdge;
use crate::arena::{self, NodeId};
use crate::id::{NamespaceId, PrefixId};
use crate::xmlvalue::Value;
use crate::xotdata::{Node, Xot};

use super::Output;

/// Where the walk is within the start of an element.
enum State<'a> {
    /// Between elements: the next edge comes from the traversal.
    Edges,
    /// The start tag of the top element is open; the prefixes in scope that
    /// it does not declare are still to be collected.
    TopCollect,
    /// The prefixes in scope for the top element, `i` of them yielded.
    TopExtra(usize),
    /// The namespace and attribute nodes at the start of the children of
    /// `element`; `next` is the child to look at, with its value.
    Leading {
        element: Node,
        next: Option<(NodeId, &'a Value)>,
        in_attributes: bool,
    },
    /// The start tag of the element is ready to be closed.
    Close(Node),
}

/// The outputs of a node, as [`Xot::outputs`] gives them.
///
/// This reads the values of the edges of [`Xot::traverse`], and the
/// namespace and attribute nodes of each element by following its children,
/// so it makes no allocation per node.
pub(crate) struct OutputWalk<'a, I> {
    xot: &'a Xot,
    top: Node,
    edges: I,
    state: State<'a>,
    // the prefixes in scope of the top element that it does not declare
    top_extra: Vec<(PrefixId, NamespaceId)>,
}

/// The outputs of `top` and its descendants.
pub(crate) fn output_walk<'a>(
    xot: &'a Xot,
    top: Node,
) -> OutputWalk<'a, impl Iterator<Item = (NodeEdge, &'a Value)> + 'a> {
    OutputWalk {
        xot,
        top,
        edges: xot.traverse_values(top),
        state: State::Edges,
        top_extra: Vec::new(),
    }
}

impl<'a, I> OutputWalk<'a, I> {
    /// The state after the start tag of `element` was opened (and the
    /// prefixes of the top element were yielded): at its first child.
    #[inline]
    fn leading(&self, element: Node) -> State<'a> {
        let next = self
            .xot
            .arena
            .children_data(element.get())
            .next()
            .map(|(id, value)| (id, arena::live(value)));
        State::Leading {
            element,
            next,
            in_attributes: false,
        }
    }
}

impl<'a, I> Iterator for OutputWalk<'a, I>
where
    I: Iterator<Item = (NodeEdge, &'a Value)>,
{
    type Item = (Node, Output<'a>);

    fn next(&mut self) -> Option<(Node, Output<'a>)> {
        loop {
            match &mut self.state {
                State::Edges => {
                    let (edge, value) = self.edges.next()?;
                    match edge {
                        NodeEdge::Start(node) => match value {
                            Value::Document => {}
                            Value::Element(element) => {
                                self.state = if node == self.top {
                                    State::TopCollect
                                } else {
                                    self.leading(node)
                                };
                                return Some((node, Output::StartTagOpen(*element)));
                            }
                            Value::Text(text) => return Some((node, Output::Text(text.get()))),
                            Value::Comment(comment) => {
                                return Some((node, Output::Comment(comment.get())))
                            }
                            Value::ProcessingInstruction(pi) => {
                                return Some((
                                    node,
                                    Output::ProcessingInstruction(pi.target(), pi.data()),
                                ))
                            }
                            // the traversal gives normal nodes only
                            Value::Attribute(_) | Value::Namespace(_) => {}
                        },
                        NodeEdge::End(node) => {
                            if let Value::Element(element) = value {
                                return Some((node, Output::EndTag(*element)));
                            }
                        }
                    }
                }
                State::TopCollect => {
                    // serialize any extra prefixes if this is the top element
                    // of an unattached tree and they aren't declared already
                    let top = self.top;
                    let namespaces = self.xot.namespaces(top);
                    self.top_extra.clear();
                    for (prefix_id, namespace_id) in self.xot.namespaces_in_scope(top) {
                        if !namespaces.contains_key(prefix_id) {
                            self.top_extra.push((prefix_id, namespace_id));
                        }
                    }
                    self.state = State::TopExtra(0);
                }
                State::TopExtra(i) => {
                    if let Some((prefix_id, namespace_id)) = self.top_extra.get(*i) {
                        let output = Output::Prefix(*prefix_id, *namespace_id);
                        *i += 1;
                        return Some((self.top, output));
                    }
                    self.state = self.leading(self.top);
                }
                State::Leading {
                    element,
                    next,
                    in_attributes,
                } => {
                    let element = *element;
                    let Some((id, value)) = *next else {
                        self.state = State::Close(element);
                        continue;
                    };
                    let output = match value {
                        Value::Namespace(namespace) if !*in_attributes => {
                            Output::Prefix(namespace.prefix_id, namespace.namespace_id)
                        }
                        Value::Attribute(attribute) => {
                            *in_attributes = true;
                            Output::Attribute(attribute.name_id, attribute.value.as_str())
                        }
                        // the namespaces and attributes end at any other
                        // child, and at a namespace after an attribute
                        _ => {
                            self.state = State::Close(element);
                            continue;
                        }
                    };
                    *next = self.xot.arena.get_with_next_sibling(id).1;
                    return Some((element, output));
                }
                State::Close(element) => {
                    let element = *element;
                    self.state = State::Edges;
                    return Some((element, Output::StartTagClose));
                }
            }
        }
    }
}

/// The generator based walk that `OutputWalk` replaced, as the oracle of the
/// tests.
#[cfg(test)]
pub(crate) mod oracle {
    use genawaiter::rc::gen;
    use genawaiter::yield_;

    use super::super::Output;
    use crate::access::NodeEdge;
    use crate::xmlvalue::Value;
    use crate::xotdata::{Node, Xot};

    pub(crate) fn gen_outputs(
        xot: &Xot,
        node: Node,
    ) -> impl Iterator<Item = (Node, Output<'_>)> + '_ {
        gen!({
            for edge in xot.traverse(node) {
                match edge {
                    NodeEdge::Start(current_node) => {
                        let gen = gen_edge_start(xot, node, current_node);
                        for output in gen {
                            yield_!((current_node, output));
                        }
                    }
                    NodeEdge::End(current_node) => {
                        let gen = gen_edge_end(xot, current_node);
                        for output in gen {
                            yield_!((current_node, output));
                        }
                    }
                }
            }
        })
        .into_iter()
    }

    fn gen_edge_start(
        xot: &Xot,
        top_node: Node,
        node: Node,
    ) -> impl Iterator<Item = Output<'_>> + '_ {
        gen!({
            let value = xot.value(node);

            match value {
                Value::Document => {}
                Value::Element(element) => {
                    yield_!(Output::StartTagOpen(*element));

                    // serialize any extra prefixes if this is the top element of
                    // an unattached tree and they aren't declared already
                    let namespaces = xot.namespaces(node);
                    if node == top_node {
                        for (prefix_id, namespace_id) in xot.namespaces_in_scope(node) {
                            if !namespaces.contains_key(prefix_id) {
                                yield_!(Output::Prefix(prefix_id, namespace_id,));
                            }
                        }
                    }

                    for (prefix_id, namespace_id) in namespaces.iter() {
                        yield_!(Output::Prefix(prefix_id, *namespace_id,));
                    }

                    for (name_id, value) in xot.attributes(node).iter() {
                        yield_!(Output::Attribute(name_id, value));
                    }

                    yield_!(Output::StartTagClose);
                }
                Value::Text(text) => {
                    yield_!(Output::Text(text.get()));
                }
                Value::Comment(comment) => {
                    yield_!(Output::Comment(comment.get()));
                }
                Value::ProcessingInstruction(pi) => {
                    yield_!(Output::ProcessingInstruction(pi.target(), pi.data()));
                }
                Value::Attribute(_) | Value::Namespace(_) => {
                    // handled in element
                }
            }
        })
        .into_iter()
    }

    fn gen_edge_end(xot: &Xot, node: Node) -> impl Iterator<Item = Output<'_>> + '_ {
        gen!({
            let value = xot.value(node);
            if let Value::Element(element) = value {
                yield_!(Output::EndTag(*element));
            }
        })
        .into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::oracle::gen_outputs;
    use super::*;

    // the items of the walk and of the oracle for `node`, which must agree
    fn same(xot: &Xot, node: Node) -> Vec<(Node, String)> {
        let walked: Vec<_> = output_walk(xot, node).collect();
        let oracle: Vec<_> = gen_outputs(xot, node).collect();
        assert_eq!(walked, oracle);
        let public: Vec<_> = xot.outputs(node).collect();
        assert_eq!(public, oracle);
        walked
            .into_iter()
            .map(|(node, output)| (node, format!("{:?}", output)))
            .collect()
    }

    #[test]
    fn test_middle_element_as_top() {
        let mut xot = Xot::new();
        let root = xot
            .parse(
                r#"<a xmlns:p="urn:p" xmlns:q="urn:q" xmlns="urn:d"><b xmlns:q="urn:other" xmlns:r="urn:r" x="1"><c/></b></a>"#,
            )
            .unwrap();
        let a = xot.document_element(root).unwrap();
        let b = xot.first_child(a).unwrap();
        let items = same(&xot, b);
        // the prefixes of b, then those of a that b does not declare
        let prefixes: Vec<(PrefixId, NamespaceId)> = output_walk(&xot, b)
            .filter_map(|(_, output)| match output {
                Output::Prefix(p, n) => Some((p, n)),
                _ => None,
            })
            .collect();
        let in_scope: Vec<_> = xot
            .namespaces_in_scope(b)
            .filter(|(p, _)| !xot.namespaces(b).contains_key(*p))
            .collect();
        let own: Vec<_> = xot.namespaces(b).iter().map(|(p, n)| (p, *n)).collect();
        let mut expected = in_scope;
        expected.extend(own);
        assert_eq!(prefixes, expected);
        // xml, p, d (the default) are extra, q is left out as b declares it
        assert_eq!(prefixes.len(), 3 + 2);
        assert!(items.len() > 5);
        // and the same as an element in the middle of a document
        same(&xot, a);
        same(&xot, root);
        let c = xot.first_child(b).unwrap();
        same(&xot, c);
    }

    #[test]
    fn test_other_tops() {
        let mut xot = Xot::new();
        let root = xot
            .parse(r#"<a xmlns:p="urn:p" x="1">T<![CDATA[cd]]><!--c--><?t d?><?u?></a>"#)
            .unwrap();
        let a = xot.document_element(root).unwrap();
        let attribute = xot.attribute_nodes(a).next().unwrap();
        let namespace = xot
            .all_children_values(a)
            .find(|(_, value)| matches!(value, Value::Namespace(_)))
            .map(|(node, _)| node)
            .unwrap();
        assert!(same(&xot, attribute).is_empty());
        assert!(same(&xot, namespace).is_empty());
        for child in xot.children(a).collect::<Vec<_>>() {
            let items = same(&xot, child);
            assert_eq!(items.len(), 1, "{:?}", items);
        }
    }

    #[test]
    fn test_text_top() {
        let mut xot = Xot::new();
        let root = xot.parse(r#"<a><b>x</b></a>"#).unwrap();
        let a = xot.document_element(root).unwrap();
        let b = xot.first_child(a).unwrap();
        let text = xot.first_child(b).unwrap();
        let items: Vec<_> = output_walk(&xot, text).collect();
        assert_eq!(items, vec![(text, Output::Text("x"))]);
    }

    #[test]
    fn test_abnormal_top_with_normal_children() {
        // normal nodes below a node whose value was turned into an attribute
        let mut xot = Xot::new();
        let root = xot.parse(r#"<a><b><c/>t</b></a>"#).unwrap();
        let a = xot.document_element(root).unwrap();
        let b = xot.first_child(a).unwrap();
        let name = xot.add_name("n");
        *xot.value_mut(b) = Value::Attribute(crate::xmlvalue::Attribute {
            name_id: name,
            value: "v".to_string(),
        });
        // as a top, b gives no start or end, but its children are visited
        let items = same(&xot, b);
        assert_eq!(items.len(), 4);
        same(&xot, a);
        same(&xot, root);
    }

    #[test]
    fn test_namespace_after_attribute() {
        let mut xot = Xot::new();
        let root = xot
            .parse(r#"<a xmlns:p="urn:p" x="1" y="2"><b/></a>"#)
            .unwrap();
        let a = xot.document_element(root).unwrap();
        let y = xot.attribute_nodes(a).nth(1).unwrap();
        // y becomes a namespace node after the attribute x
        let q = xot.add_prefix("q");
        let ns = xot.add_namespace("urn:q");
        *xot.value_mut(y) = Value::Namespace(crate::xmlvalue::Namespace {
            prefix_id: q,
            namespace_id: ns,
        });
        let outputs: Vec<_> = output_walk(&xot, a).collect();
        let kinds: Vec<String> = outputs
            .iter()
            .map(|(_, output)| format!("{:?}", output))
            .collect();
        assert!(kinds.iter().any(|k| k.starts_with("Attribute")));
        // neither the namespace node after it nor anything after it is output
        assert!(!kinds.iter().any(|k| k.contains(&format!("Prefix({:?}", q))));
        same(&xot, a);
        // an attribute after such a namespace node is not output either
        let mut xot = Xot::new();
        let root = xot
            .parse(r#"<a xmlns:p="urn:p" x="1" y="2" z="3"/>"#)
            .unwrap();
        let a = xot.document_element(root).unwrap();
        let y = xot.attribute_nodes(a).nth(1).unwrap();
        let q = xot.add_prefix("q");
        let ns = xot.add_namespace("urn:q");
        *xot.value_mut(y) = Value::Namespace(crate::xmlvalue::Namespace {
            prefix_id: q,
            namespace_id: ns,
        });
        let attributes = output_walk(&xot, a)
            .filter(|(_, output)| matches!(output, Output::Attribute(..)))
            .count();
        assert_eq!(attributes, 1);
        same(&xot, a);
    }

    #[cfg(feature = "proptest")]
    mod prop {
        use super::*;
        use crate::proptest::arb_xml_document;
        use proptest::prelude::*;

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(2000))]
            #[test]
            fn test_walk_matches_oracle(document in arb_xml_document()) {
                let mut xot = Xot::new();
                let root = document.xotify(&mut xot);
                let walked: Vec<_> = output_walk(&xot, root).collect();
                let oracle: Vec<_> = gen_outputs(&xot, root).collect();
                prop_assert_eq!(walked, oracle);
                let element = xot.document_element(root).unwrap();
                let walked: Vec<_> = output_walk(&xot, element).collect();
                let oracle: Vec<_> = gen_outputs(&xot, element).collect();
                prop_assert_eq!(walked, oracle);
            }
        }
    }
}
