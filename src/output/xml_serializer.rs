use std::borrow::Cow;
use std::io;

use crate::entity::{serialize_attribute, serialize_cdata, serialize_text, Escapes};
use crate::error::Error;
use crate::id::NameId;
use crate::output::{NoopNormalizer, Normalizer};
use crate::parse::is_allowed_declaration;
use crate::xotdata::{Node, Xot, XMLNS_NAMESPACE};

use super::escape::first_not_xml_char;
use super::fullname::FullnameSerializer;
use super::{Output, OutputToken, Pretty, TokenSerializeParameters};

pub(crate) struct XmlSerializer<'a, N: Normalizer> {
    xot: &'a Xot,
    fullname_serializer: FullnameSerializer<'a>,
    normalizer: N,
    parameters: TokenSerializeParameters,
    escapes: Escapes,
}

impl<'a, N: Normalizer> XmlSerializer<'a, N> {
    pub(crate) fn new(
        xot: &'a Xot,
        node: Node,
        parameters: TokenSerializeParameters,
        normalizer: N,
    ) -> Self {
        let extra_declarations = xot.namespaces_in_scope(node).collect();
        let fullname_serializer = FullnameSerializer::new(xot, extra_declarations);
        let escapes = Escapes::RoundTrip {
            unescaped_cr: parameters.unescaped_cr,
        };
        Self {
            xot,
            fullname_serializer,
            normalizer,
            parameters,
            escapes,
        }
    }

    pub(crate) fn serialize<W: io::Write>(
        &mut self,
        w: &mut W,
        outputs: impl Iterator<Item = (Node, Output<'a>)>,
    ) -> Result<(), Error> {
        for (node, output) in outputs {
            self.serialize_node(w, node, output)?;
        }
        Ok(())
    }

    pub(crate) fn serialize_pretty<W: io::Write>(
        &mut self,
        w: &mut W,
        outputs: impl Iterator<Item = (Node, Output<'a>)>,
        suppress: &[NameId],
    ) -> Result<(), Error> {
        let is_suppressed = |name_id| suppress.contains(&name_id);
        let mut pretty = Pretty::new(self.xot, is_suppressed, |_| false);
        for (node, output) in outputs {
            let (indentation, newline) = pretty.prettify(node, &output);
            if indentation > 0 {
                w.write_all(" ".repeat(indentation * 2).as_bytes())?;
            }
            self.serialize_node(w, node, output)?;
            if newline {
                w.write_all(b"\n")?;
            }
        }
        Ok(())
    }

    pub(crate) fn serialize_node<W: io::Write>(
        &mut self,
        w: &mut W,
        node: Node,
        output: Output<'a>,
    ) -> Result<(), Error> {
        self.check_characters(node, &output)?;
        self.check_attribute_name(node, &output)?;
        self.check_namespace_declaration(node, &output)?;
        self.check_processing_instruction(&output)?;
        let data = self.render_output(node, &output)?;
        if data.space {
            w.write_all(b" ")?;
        }
        w.write_all(data.text.as_bytes())?;
        Ok(())
    }

    /// Check that the output has only characters that XML 1.0 allows (the
    /// `Char` production). Serialization 3.1
    /// (<https://www.w3.org/TR/xslt-xquery-serialization-31/>, section 5.1.1)
    /// makes any other character an error (SERE0006): it cannot be written
    /// as it is, nor as a character reference.
    fn check_characters(&self, node: Node, output: &Output<'a>) -> Result<(), Error> {
        let value = match output {
            Output::Text(text) | Output::Comment(text) => *text,
            Output::Attribute(_, value) => *value,
            Output::ProcessingInstruction(_, Some(data)) => *data,
            Output::Prefix(_, namespace_id) => self.xot.namespace_str(*namespace_id),
            _ => return Ok(()),
        };
        let Err(c) = first_not_xml_char(value, 0) else {
            return Ok(());
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

    pub(crate) fn render_output(
        &mut self,
        node: Node,
        output: &Output<'a>,
    ) -> Result<OutputToken, Error> {
        use Output::*;
        let r = match output {
            StartTagOpen(element) => {
                self.fullname_serializer
                    .push(self.xot.namespace_declarations(node));
                OutputToken {
                    space: false,
                    text: format!(
                        "<{}",
                        self.fullname_serializer.element_fullname(element.name_id)?
                    ),
                }
            }
            StartTagClose => {
                if self.xot.first_child(node).is_none() {
                    OutputToken {
                        space: false,
                        text: "/>".to_string(),
                    }
                } else {
                    OutputToken {
                        space: false,
                        text: ">".to_string(),
                    }
                }
            }
            EndTag(element) => {
                let r = if self.xot.first_child(node).is_some() {
                    OutputToken {
                        space: false,
                        text: format!(
                            "</{}>",
                            self.fullname_serializer.element_fullname(element.name_id)?
                        ),
                    }
                } else {
                    OutputToken {
                        space: false,
                        text: "".to_string(),
                    }
                };
                self.fullname_serializer
                    .pop(self.xot.has_namespace_declarations(node));
                r
            }
            Prefix(prefix_id, namespace_id) => {
                // we don't want to output the xml prefix
                if *namespace_id == self.xot.xml_namespace() {
                    return Ok(OutputToken {
                        space: false,
                        text: "".to_string(),
                    });
                }
                // a namespace declaration is an attribute, so the namespace
                // is escaped like an attribute value; it is not normalized,
                // as that would change the namespace
                let namespace = serialize_attribute(
                    self.xot.namespace_str(*namespace_id).into(),
                    &NoopNormalizer,
                    self.escapes,
                );
                if *prefix_id == self.xot.empty_prefix_id {
                    OutputToken {
                        space: true,
                        text: format!("xmlns=\"{}\"", namespace),
                    }
                } else {
                    let prefix = self.xot.prefix_str(*prefix_id);
                    OutputToken {
                        space: true,
                        text: format!("xmlns:{}=\"{}\"", prefix, namespace),
                    }
                }
            }
            Attribute(name_id, value) => {
                let fullname = self.fullname_serializer.attribute_fullname(*name_id)?;
                OutputToken {
                    space: true,
                    text: format!(
                        "{}=\"{}\"",
                        fullname,
                        serialize_attribute((*value).into(), &self.normalizer, self.escapes)
                    ),
                }
            }
            Text(text) => {
                // a text node can be a child of an element or document, or it
                // can have no parent at all
                let parent_element = self
                    .xot
                    .parent(node)
                    .and_then(|parent| self.xot.element(parent));

                let is_cdata_element = if let Some(element) = parent_element {
                    self.parameters
                        .cdata_section_elements
                        .contains(&element.name())
                } else {
                    false
                };

                if is_cdata_element {
                    OutputToken {
                        space: false,
                        text: serialize_cdata((*text).into(), &self.normalizer, self.escapes)
                            .to_string(),
                    }
                } else {
                    OutputToken {
                        space: false,
                        text: serialize_text(
                            (*text).into(),
                            &self.normalizer,
                            self.parameters.unescaped_gt,
                            self.escapes,
                        )
                        .to_string(),
                    }
                }
            }
            Comment(text) => OutputToken {
                space: false,
                text: format!("<!--{}-->", text),
            },
            ProcessingInstruction(target, data) => {
                let (target, ns) = self.xot.name_ns_str(*target);
                if !ns.is_empty() {
                    return Err(Error::NamespaceInProcessingInstruction);
                }
                if let Some(data) = data {
                    OutputToken {
                        space: false,
                        text: format!("<?{} {}?>", target, data),
                    }
                } else {
                    OutputToken {
                        space: false,
                        text: format!("<?{}?>", target),
                    }
                }
            }
        };
        Ok(r)
    }
}
