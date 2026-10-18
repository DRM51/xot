use std::borrow::Cow;
use std::io::Write;

use crate::entity::Escapes;
use crate::error::Error;
use crate::id::{NameId, PrefixId};
use crate::output::{self, escape, Normalizer};
use crate::xotdata::{Node, Xot};

use super::html5elements::Html5Elements;
use super::xml_serializer::write_indentation;
use super::{output_walk, reset_large_scratch, NameScope, Output, Pretty, SCRATCH_CAPACITY};

/// Renders the outputs of a node as HTML 5 text, one token at a time, into a
/// string that the caller gives.
pub(crate) struct Html5Renderer<'a, N: Normalizer> {
    xot: &'a Xot,
    html5_elements: &'a Html5Elements,
    cdata_section_names: &'a [NameId],
    scope: NameScope,
    normalizer: N,
    // for each open element: whether its start tag pushed namespace
    // declarations
    pushed: Vec<bool>,
}

pub(crate) fn html_matches_suppress(
    xot: &Xot,
    html5_elements: &Html5Elements,
    names: &[NameId],
    name_id: NameId,
) -> bool {
    for suppress_name in names {
        if name_id == *suppress_name {
            return true;
        }
        let suppress_name_ns = xot.namespace_for_name(*suppress_name);
        // if it's not in the xhtml namespace or in no namespace, they
        // can't possibly compare anymore
        if !html5_elements.is_html_namespace(xot, suppress_name_ns) {
            return false;
        }
        let name_ns = xot.namespace_for_name(name_id);
        if !html5_elements.is_html_namespace(xot, name_ns) {
            return false;
        }
        // now we can do a case insensitive compare of the local name
        if xot
            .local_name_str(*suppress_name)
            .eq_ignore_ascii_case(xot.local_name_str(name_id))
        {
            return true;
        }
    }
    false
}

impl<'a, N: Normalizer> Html5Renderer<'a, N> {
    pub(crate) fn new(
        xot: &'a Xot,
        html5_elements: &'a Html5Elements,
        node: Node,
        cdata_section_names: &'a [NameId],
        normalizer: N,
    ) -> Self {
        let base = xot.namespaces_in_scope(node).collect::<Vec<_>>();
        let scope = NameScope::new(xot, &base);
        Self {
            xot,
            html5_elements,
            cdata_section_names,
            scope,
            normalizer,
            pushed: Vec::new(),
        }
    }

    /// Append the text of the token for `output` to `out`, without the space
    /// in front of it, and say whether a space goes in front of it.
    pub(crate) fn token(
        &mut self,
        node: Node,
        output: &Output<'a>,
        out: &mut String,
    ) -> Result<bool, Error> {
        use Output::*;
        match output {
            StartTagOpen(element) => {
                let pushed = self.scope.push(
                    self.xot
                        .namespaces(node)
                        .iter()
                        .map(|(prefix, namespace)| (prefix, *namespace)),
                );
                self.pushed.push(pushed);
                let namespace_id = self.xot.namespace_for_name(element.name());
                if self
                    .html5_elements
                    .must_be_serialized_unprefixed(namespace_id)
                    && !self.scope.has_unprefixed(namespace_id)
                {
                    // add the empty prefix for the namespace
                    self.scope.add_unprefixed(namespace_id);
                    // we also need to serialize the additional xmlns, as it
                    // is
                    out.push('<');
                    out.push_str(self.xot.local_name_str(element.name()));
                    out.push_str(" xmlns=\"");
                    out.push_str(self.xot.namespace_str(namespace_id));
                    out.push('"');
                    return Ok(false);
                }
                let prefix = self.scope.element_prefix(self.xot, element.name())?;
                out.push('<');
                self.push_name(out, prefix, element.name());
                Ok(false)
            }
            StartTagClose => {
                out.push('>');
                Ok(false)
            }
            EndTag(element) => {
                let pushed = match self.pushed.pop() {
                    Some(pushed) => pushed,
                    // an end tag always follows its start tag in the walk
                    None => self.xot.has_namespace_declarations(node),
                };
                // void elements don't get their end tag, so their text is
                // empty
                if !self
                    .html5_elements
                    .void_names
                    .matches(self.xot, element.name())
                {
                    let prefix = self.scope.element_prefix(self.xot, element.name())?;
                    out.push_str("</");
                    self.push_name(out, prefix, element.name());
                    out.push('>');
                }
                if pushed {
                    self.scope.pop();
                }
                Ok(false)
            }
            Prefix(prefix_id, namespace_id) => {
                let element_name = self.xot.element(node).unwrap().name();
                // we don't want to output non-empty prefixes unless the
                // element has an attribute with the same prefix
                if *namespace_id == self.xot.xml_namespace()
                    || (*prefix_id == self.xot.empty_prefix()
                        && self.xot.namespace_for_name(element_name) != *namespace_id)
                    || (*prefix_id != self.xot.empty_prefix()
                        && self
                            .html5_elements
                            .must_be_serialized_unprefixed(*namespace_id)
                        && !self
                            .xot
                            .attributes(node)
                            .keys()
                            .any(|name| self.xot.namespace_for_name(name) == *namespace_id))
                {
                    return Ok(false);
                }
                if *prefix_id == self.xot.empty_prefix_id {
                    out.push_str("xmlns=\"");
                } else {
                    out.push_str("xmlns:");
                    out.push_str(self.xot.prefix_str(*prefix_id));
                    out.push_str("=\"");
                }
                // escaped like the value of an attribute in a namespace, and
                // not normalized, as that would change the namespace
                escape::push_markup_attribute(out, self.xot.namespace_str(*namespace_id), 0);
                out.push('"');
                Ok(true)
            }
            Attribute(name_id, value) => {
                let prefix = self.scope.attribute_prefix(self.xot, *name_id)?;
                let namespace = self.xot.namespace_for_name(*name_id);
                // boolean attribute: no prefix, and the local name is the
                // same as the value
                if prefix.is_none()
                    && self.html5_elements.is_html_namespace(self.xot, namespace)
                    && self
                        .xot
                        .local_name_str(*name_id)
                        .eq_ignore_ascii_case(value)
                {
                    out.push_str(self.xot.local_name_str(*name_id));
                    return Ok(true);
                }
                self.push_name(out, prefix, *name_id);
                out.push_str("=\"");
                let normalized = self.normalizer.normalize(Cow::Borrowed(*value));
                if namespace != self.xot.no_namespace() {
                    escape::push_markup_attribute(out, &normalized, 0);
                } else {
                    escape::push_html_attribute(out, &normalized, 0);
                }
                out.push('"');
                Ok(true)
            }
            Text(text) => {
                // a text node is usually a child of an element, but it can also
                // be a child of a document or have no parent at all; text
                // outside any element is escaped as in an HTML element
                let element_name = self
                    .xot
                    .parent(node)
                    .and_then(|parent| self.xot.element(parent))
                    .map(|element| element.name());
                let normalized = self.normalizer.normalize(Cow::Borrowed(*text));
                match element_name {
                    Some(name) if self.html5_elements.no_escape_names.matches(self.xot, name) => {
                        out.push_str(&normalized);
                    }
                    Some(name) if self.cdata_section_names.contains(&name) => {
                        escape::push_cdata(out, &normalized, Escapes::MarkupOnly);
                    }
                    Some(name) if !self.html5_elements.is_html_element(self.xot, name) => {
                        escape::push_markup_text(out, &normalized, 0, false);
                    }
                    _ => escape::push_html_text(out, &normalized, 0),
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
                // for some reason the HTML output method does allow processing
                // instructions, but they don't end with ?>
                if let Some(data) = data {
                    if data.contains('>') {
                        return Err(Error::ProcessingInstructionGtInHtml(data.to_string()));
                    }
                }
                out.push_str("<?");
                out.push_str(target);
                if let Some(data) = data {
                    out.push(' ');
                    out.push_str(data);
                }
                out.push('>');
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
}

/// Write `node` as HTML 5 to `w`.
///
/// As for XML (see `write_xml`), each token is rendered into one string that
/// is used again for the next, and `w` gets the calls it got when each token
/// was a new string: first the doctype, then for each token the indentation,
/// the space in front of it, the token (also when it is empty) and the
/// newline, each in one `write_all`. When a token cannot be written, `w` has
/// had everything before it, its indentation included, and nothing of it.
pub(crate) fn write_html<N: Normalizer>(
    xot: &Xot,
    html5_elements: &Html5Elements,
    parameters: output::html5::Parameters,
    node: Node,
    w: &mut dyn Write,
    normalizer: N,
) -> Result<(), Error> {
    w.write_all(b"<!DOCTYPE html>")?;
    let outputs = output_walk(xot, node);
    let mut renderer = Html5Renderer::new(
        xot,
        html5_elements,
        node,
        &parameters.cdata_section_elements,
        normalizer,
    );
    let mut scratch = String::with_capacity(SCRATCH_CAPACITY);
    if let Some(indentation) = &parameters.indentation {
        let suppress = &indentation.suppress;
        // we have to do the relatively slow html_matches_suppress call here,
        // as we cannot make an efficient HtmlNames at this point (as this
        // needs a mutable Xot)
        let is_suppressed = |name_id| {
            html5_elements.formatted_names.matches(xot, name_id)
                || html_matches_suppress(xot, html5_elements, suppress, name_id)
        };
        let is_inline = |name_id| html5_elements.is_inline(xot, name_id);
        let mut pretty = Pretty::new(xot, is_suppressed, is_inline);
        for (node, output) in outputs {
            let (level, newline) = pretty.prettify(node, &output);
            if level > 0 {
                write_indentation(w, level)?;
            }
            scratch.clear();
            if renderer.token(node, &output, &mut scratch)? {
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
            if renderer.token(node, &output, &mut scratch)? {
                w.write_all(b" ")?;
            }
            w.write_all(scratch.as_bytes())?;
            reset_large_scratch(&mut scratch);
        }
    }
    Ok(())
}

/// The text of an HTML element, escaped, as the HTML 5 writer escapes it.
#[cfg(test)]
pub(crate) fn serialize_text_html<'a, N: Normalizer>(
    content: Cow<'a, str>,
    normalizer: &N,
) -> Cow<'a, str> {
    let normalized_content = normalizer.normalize(content);
    let bytes = normalized_content.as_bytes();
    let start = escape::first_special(bytes, 0, escape::HTML_TEXT);
    if start == bytes.len() {
        return normalized_content;
    }
    let mut result = String::with_capacity(bytes.len() + bytes.len() / 8 + 16);
    escape::push_html_text(&mut result, &normalized_content, start);
    result.into()
}

/// The value of an attribute in no namespace, escaped, as the HTML 5 writer
/// escapes it.
#[cfg(test)]
pub(crate) fn serialize_attribute_html<'a, N: Normalizer>(
    content: Cow<'a, str>,
    normalizer: &N,
) -> Cow<'a, str> {
    let normalized_content = normalizer.normalize(content);
    let bytes = normalized_content.as_bytes();
    let start = escape::first_special(bytes, 0, escape::HTML_ATTRIBUTE);
    if start == bytes.len() {
        return normalized_content;
    }
    let mut result = String::with_capacity(bytes.len() + bytes.len() / 8 + 16);
    escape::push_html_attribute(&mut result, &normalized_content, start);
    result.into()
}

#[cfg(test)]
pub(crate) fn old_serialize_text_html<'a, N: Normalizer>(
    content: Cow<'a, str>,
    normalizer: &N,
) -> Cow<'a, str> {
    let mut result = String::new();
    let mut change = false;
    // if we had normalized_iter on the trait we avoid this string allocation
    let normalized_content = normalizer.normalize(content);
    for c in normalized_content.chars() {
        match c {
            '&' => {
                change = true;
                result.push_str("&amp;")
            }
            '<' => {
                change = true;
                result.push_str("&lt;")
            }
            // non-breaking space
            '\u{a0}' => {
                change = true;
                result.push_str("&nbsp;")
            }
            _ => result.push(c),
        }
    }

    if !change {
        normalized_content
    } else {
        result.into()
    }
}

#[cfg(test)]
pub(crate) fn old_serialize_attribute_html<'a, N: Normalizer>(
    content: Cow<'a, str>,
    normalizer: &N,
) -> Cow<'a, str> {
    let mut result = String::new();
    let mut change = false;
    let normalized_content = normalizer.normalize(content);
    for c in normalized_content.chars() {
        match c {
            '&' => {
                change = true;
                result.push_str("&amp;")
            }
            '\'' => {
                change = true;
                result.push_str("&apos;")
            }
            '"' => {
                change = true;
                result.push_str("&quot;")
            }
            // non-breaking space
            '\u{a0}' => {
                change = true;
                result.push_str("&nbsp;")
            }
            _ => result.push(c),
        }
    }

    if !change {
        normalized_content
    } else {
        result.into()
    }
}

#[cfg(test)]
mod tests {
    use crate::output::{html5::Parameters, html5elements::XHTML_NS, Indentation, NoopNormalizer};

    use super::*;

    // turns x into a non-breaking space and y into &
    struct Replacing;

    impl Normalizer for Replacing {
        fn normalize<'a>(&self, content: Cow<'a, str>) -> Cow<'a, str> {
            if content.contains(['x', 'y']) {
                Cow::Owned(content.replace('x', "\u{a0}").replace('y', "&"))
            } else {
                content
            }
        }
    }

    #[test]
    fn test_html_escapers_equal_the_old_bodies() {
        for s in [
            "",
            "a",
            "x",
            "y",
            "a<b",
            "a&b",
            "'\"",
            "\u{a0}",
            "\u{a1}\u{80}",
            "ax'y\u{a0}<>",
            "z",
        ] {
            assert_eq!(
                serialize_text_html(s.into(), &NoopNormalizer),
                old_serialize_text_html(s.into(), &NoopNormalizer)
            );
            assert_eq!(
                serialize_text_html(s.into(), &Replacing),
                old_serialize_text_html(s.into(), &Replacing)
            );
            assert_eq!(
                serialize_attribute_html(s.into(), &NoopNormalizer),
                old_serialize_attribute_html(s.into(), &NoopNormalizer)
            );
            assert_eq!(
                serialize_attribute_html(s.into(), &Replacing),
                old_serialize_attribute_html(s.into(), &Replacing)
            );
        }
        assert!(matches!(
            serialize_text_html("a'\"".into(), &NoopNormalizer),
            Cow::Borrowed(_)
        ));
        assert!(matches!(
            serialize_attribute_html("a<>".into(), &NoopNormalizer),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn test_normalizer_in_html() {
        let mut xot = Xot::new();
        let root = xot
            .parse(r#"<html><body a="x'y" xmlns:p="urn:p" p:b="xy"><script>xy</script><p>x&lt;y</p><i xmlns="urn:i">xy</i></body></html>"#)
            .unwrap();
        let s = xot
            .html5()
            .serialize_string_with_normalizer(Default::default(), root, Replacing)
            .unwrap();
        assert_eq!(
            s,
            "<!DOCTYPE html><html><body xmlns:p=\"urn:p\" a=\"&nbsp;&apos;&amp;\" p:b=\"\u{a0}&amp;\"><script>\u{a0}&</script><p>&nbsp;&lt;&amp;</p><i xmlns=\"urn:i\">\u{a0}&amp;</i></body></html>"
        );
    }

    #[test]
    fn test_never_empty_html() {
        let mut xot = Xot::new();
        let root = xot
            .parse("<html><head></head><body></body></html>")
            .unwrap();
        let s = xot.html5().to_string(root).unwrap();
        assert_eq!(s, "<!DOCTYPE html><html><head></head><body></body></html>");
    }

    #[test]
    fn test_never_empty_xml_element() {
        let mut xot = Xot::new();
        let root = xot
            .parse(r#"<html><head><foo xmlns="foo"><bar></bar></foo></head><body></body></html>"#)
            .unwrap();
        let s = xot.html5().to_string(root).unwrap();
        assert_eq!(
            s,
            r#"<!DOCTYPE html><html><head><foo xmlns="foo"><bar></bar></foo></head><body></body></html>"#
        );
    }

    #[test]
    fn test_void_element() {
        let mut xot = Xot::new();
        let root = xot.parse("<html><body>foo<br/>bar</body></html>").unwrap();
        let s = xot.html5().to_string(root).unwrap();
        assert_eq!(s, "<!DOCTYPE html><html><body>foo<br>bar</body></html>");
    }

    #[test]
    fn test_escaping_for_normal_content() {
        let mut xot = Xot::new();
        let root = xot
            .parse(r#"<html><head><title>foo &amp; bar</title></head><body>foo &amp; bar</body></html>"#)
            .unwrap();
        let s = xot.html5().to_string(root).unwrap();
        assert_eq!(
            s,
            r#"<!DOCTYPE html><html><head><title>foo &amp; bar</title></head><body>foo &amp; bar</body></html>"#
        );
    }
    #[test]
    fn test_no_escaping_for_script_and_style() {
        let mut xot = Xot::new();
        let root = xot
            .parse(r#"<html><head><script>if (a &lt; b) foo()</script><style>a &lt; b</style></head><body></body></html>"#)
            .unwrap();
        let s = xot.html5().to_string(root).unwrap();
        assert_eq!(
            s,
            r#"<!DOCTYPE html><html><head><script>if (a < b) foo()</script><style>a < b</style></head><body></body></html>"#
        );
    }

    #[test]
    fn test_processing_instruction() {
        let mut xot = Xot::new();
        let root = xot
            .parse(r#"<html><head><?foo bar?></head><body></body></html>"#)
            .unwrap();
        let s = xot.html5().to_string(root).unwrap();
        assert_eq!(
            s,
            r#"<!DOCTYPE html><html><head><?foo bar></head><body></body></html>"#
        );
    }

    #[test]
    fn test_processing_instruction_no_gt() {
        let mut xot = Xot::new();
        let root = xot
            .parse(r#"<html><head><?foo >bar?></head><body></body></html>"#)
            .unwrap();
        let e = xot.html5().to_string(root).unwrap_err();

        assert!(matches!(e, Error::ProcessingInstructionGtInHtml(_)));
        match e {
            Error::ProcessingInstructionGtInHtml(s) => {
                assert_eq!(s, ">bar");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn test_serialize_text_nbsp() {
        let mut xot = Xot::new();
        let root = xot
            .parse("<html><body>foo\u{00a0}bar</body></html>")
            .unwrap();
        let s = xot.html5().to_string(root).unwrap();
        assert_eq!(s, "<!DOCTYPE html><html><body>foo&nbsp;bar</body></html>");
    }

    #[test]
    fn test_serialize_text_no_nbsp_for_xml_island() {
        let mut xot = Xot::new();
        let root = xot
            .parse("<html><body><island xmlns=\"island\">\u{00a0}</island></body></html>")
            .unwrap();
        let s = xot.html5().to_string(root).unwrap();
        assert_eq!(
            s,
            "<!DOCTYPE html><html><body><island xmlns=\"island\">\u{00a0}</island></body></html>"
        );
    }

    #[test]
    fn test_serialize_attribute_nbsp() {
        let mut xot = Xot::new();
        let root = xot
            .parse("<html><body foo='\u{00a0}'>bar</body></html>")
            .unwrap();
        let s = xot.html5().to_string(root).unwrap();
        assert_eq!(
            s,
            r#"<!DOCTYPE html><html><body foo="&nbsp;">bar</body></html>"#
        );
    }

    #[test]
    fn test_serialize_attribute_nbsp_not_in_prefixed_attribute() {
        let mut xot = Xot::new();
        let root = xot
            .parse("<html><body xmlns:prefix='ns' prefix:foo='\u{00a0}'>bar</body></html>")
            .unwrap();
        let s = xot.html5().to_string(root).unwrap();
        assert_eq!(
            s,
            "<!DOCTYPE html><html><body xmlns:prefix=\"ns\" prefix:foo=\"\u{00a0}\">bar</body></html>".to_string()
        );
    }

    #[test]
    fn test_serialize_attribute_dont_escape_lt() {
        let mut xot = Xot::new();
        let root = xot
            .parse("<html><body foo='&lt;'>bar</body></html>")
            .unwrap();
        let s = xot.html5().to_string(root).unwrap();
        assert_eq!(s, r#"<!DOCTYPE html><html><body foo="<">bar</body></html>"#);
    }

    #[test]
    fn test_serialize_attribute_boolean() {
        let mut xot = Xot::new();
        let root = xot
            .parse(r#"<html><body><option selected="selected"/></body></html>"#)
            .unwrap();
        let s = xot.html5().to_string(root).unwrap();
        assert_eq!(
            s,
            r#"<!DOCTYPE html><html><body><option selected></option></body></html>"#
        );
    }

    #[test]
    fn test_serialize_attribute_boolean_not_when_prefixed() {
        let mut xot = Xot::new();
        let root = xot
            .parse(r#"<html><body><option xmlns:prefix="ns" prefix:selected="prefix:selected"/></body></html>"#)
            .unwrap();
        let s = xot.html5().to_string(root).unwrap();
        assert_eq!(
            s,
            r#"<!DOCTYPE html><html><body><option xmlns:prefix="ns" prefix:selected="prefix:selected"></option></body></html>"#
        );
    }

    #[test]
    fn test_serialize_attribute_boolean_with_prefix() {
        let mut xot = Xot::new();
        let root = xot
            .parse(r#"<html><body><option xmlns:foo="foo" foo:selected="selected"/></body></html>"#)
            .unwrap();
        let s = xot.html5().to_string(root).unwrap();
        assert_eq!(
            s,
            r#"<!DOCTYPE html><html><body><option xmlns:foo="foo" foo:selected="selected"></option></body></html>"#
        );
    }

    #[test]
    fn test_serialize_attribute_boolean_with_xhtml_prefix() {
        let mut xot = Xot::new();
        let root = xot
            .parse(r#"<html><body><option xmlns:foo="https://www.w3.org/1999/xhtml" foo:selected="selected"/></body></html>"#)
            .unwrap();
        let s = xot.html5().to_string(root).unwrap();
        assert_eq!(
            s,
            r#"<!DOCTYPE html><html><body><option xmlns:foo="https://www.w3.org/1999/xhtml" foo:selected="selected"></option></body></html>"#
        );
    }

    #[test]
    fn test_serialize_attribute_boolean_case_insensitive() {
        let mut xot = Xot::new();
        let root = xot
            .parse(r#"<html><body><option selected="SeLecTed"/></body></html>"#)
            .unwrap();
        let s = xot.html5().to_string(root).unwrap();
        assert_eq!(
            s,
            r#"<!DOCTYPE html><html><body><option selected></option></body></html>"#
        );
    }

    #[test]
    fn test_html_no_xml_namespace() {
        let mut xot = Xot::new();
        // note that the serialization spec illegitimately places the XML namespace in
        // https, but it's in http
        let root = xot
            .parse(r#"<html xmlns:xml="http://www.w3.org/XML/1998/namespace"></html>"#)
            .unwrap();
        let s = xot.html5().to_string(root).unwrap();
        assert_eq!(s, "<!DOCTYPE html><html></html>");
    }

    #[test]
    fn test_xhtml_namespace_without_prefix() {
        let mut xot = Xot::new();
        let root = xot
            .parse(r#"<prefix:html xmlns:prefix="https://www.w3.org/1999/xhtml"></prefix:html>"#)
            .unwrap();
        let s = xot.html5().to_string(root).unwrap();
        assert_eq!(
            s,
            r#"<!DOCTYPE html><html xmlns="https://www.w3.org/1999/xhtml"></html>"#
        );
    }

    #[test]
    fn test_xhtml_namespace_without_prefix_but_with_attribute() {
        let mut xot = Xot::new();
        let root = xot
            .parse(r#"<prefix:html xmlns:prefix="https://www.w3.org/1999/xhtml" prefix:a="A"></prefix:html>"#)
            .unwrap();
        let s = xot.html5().to_string(root).unwrap();
        assert_eq!(
            s,
            r#"<!DOCTYPE html><html xmlns="https://www.w3.org/1999/xhtml" xmlns:prefix="https://www.w3.org/1999/xhtml" prefix:a="A"></html>"#
        );
    }

    #[test]
    fn test_xhtml_namespace_without_prefix_dont_redeclare() {
        let mut xot = Xot::new();
        let root = xot
            .parse(r#"<prefix:html xmlns:prefix="https://www.w3.org/1999/xhtml"><prefix:body></prefix:body></prefix:html>"#)
            .unwrap();
        let s = xot.html5().to_string(root).unwrap();
        assert_eq!(
            s,
            r#"<!DOCTYPE html><html xmlns="https://www.w3.org/1999/xhtml"><body></body></html>"#
        );
    }

    #[test]
    fn test_default_namespace_different_from_element_is_ignored_xhtml() {
        let mut xot = Xot::new();
        let root = xot
            .parse(r#"<prefix:html xmlns="different" xmlns:prefix="https://www.w3.org/1999/xhtml"><prefix:body></prefix:body></prefix:html>"#)
            .unwrap();
        let s = xot.html5().to_string(root).unwrap();
        assert_eq!(
            s,
            r#"<!DOCTYPE html><html xmlns="https://www.w3.org/1999/xhtml"><body></body></html>"#
        );
    }

    #[test]
    fn test_default_namespace_different_from_element_is_ignored() {
        let mut xot = Xot::new();
        let root = xot
            .parse(r#"<prefix:html xmlns="different" xmlns:prefix="main"><prefix:body></prefix:body></prefix:html>"#)
            .unwrap();
        let s = xot.html5().to_string(root).unwrap();
        assert_eq!(
            s,
            r#"<!DOCTYPE html><prefix:html xmlns:prefix="main"><prefix:body></prefix:body></prefix:html>"#
        );
    }

    // #[test]
    // fn test_xhtml_namespace_without_prefix_redeclare_if_intervening() {
    //     let mut xot = Xot::new();
    //     let root = xot
    //         .parse(r#"<prefix:html xmlns:prefix="https://www.w3.org/1999/xhtml"><prefix:body xmlns="different"><prefix:p></prefix:p></prefix:body></prefix:html>"#)
    //         .unwrap();
    //     let s = xot.html5().to_string(root).unwrap();
    //     // TODO: this is probably wrong; we don't expect an additional namespace declaration. On
    //     // the other hand, there was an intervening prefix, but it should have been ignored.
    //     assert_eq!(
    //         s,
    //         r#"<!DOCTYPE html><html xmlns="https://www.w3.org/1999/xhtml"><body></body></html>"#
    //     );
    // }

    #[test]
    fn test_pretty_with_xml_island() {
        let mut xot = Xot::new();
        let root = xot
            .parse(r#"<html><body><island xmlns="island"><foo><bar/></foo></island></body></html>"#)
            .unwrap();
        let s = xot
            .html5()
            .serialize_string(
                Parameters {
                    indentation: Some(Default::default()),
                    ..Default::default()
                },
                root,
            )
            .unwrap();
        assert_eq!(
            s,
            r#"<!DOCTYPE html><html>
  <body>
    <island xmlns="island">
      <foo>
        <bar></bar>
      </foo>
    </island>
  </body>
</html>
"#
        );
    }

    #[test]
    fn test_pretty_with_non_phrasing_element() {
        let mut xot = Xot::new();
        let root = xot
            .parse(r#"<html><body><p>Hello</p><p>World</p></body></html>"#)
            .unwrap();
        let s = xot
            .html5()
            .serialize_string(
                Parameters {
                    indentation: Some(Default::default()),
                    ..Default::default()
                },
                root,
            )
            .unwrap();
        assert_eq!(
            s,
            r#"<!DOCTYPE html><html>
  <body>
    <p>Hello</p>
    <p>World</p>
  </body>
</html>
"#
        );
    }

    #[test]
    fn test_pretty_with_phrasing_element() {
        let mut xot = Xot::new();
        let root = xot
            .parse(r#"<html><body><p><span>Foo</span></p></body></html>"#)
            .unwrap();
        let s = xot
            .html5()
            .serialize_string(
                Parameters {
                    indentation: Some(Default::default()),
                    ..Default::default()
                },
                root,
            )
            .unwrap();
        assert_eq!(
            s,
            r#"<!DOCTYPE html><html>
  <body>
    <p><span>Foo</span></p>
  </body>
</html>
"#
        );
    }

    #[test]
    fn test_pretty_with_formatted_element() {
        let mut xot = Xot::new();
        // the HTML is nonsense, but we can verify p isn't indented as no
        // formatting may be removed or added inside of a formatted element like pre
        let root = xot
            .parse(r#"<html><body><pre><p></p></pre></body></html>"#)
            .unwrap();
        let s = xot
            .html5()
            .serialize_string(
                Parameters {
                    indentation: Some(Default::default()),
                    ..Default::default()
                },
                root,
            )
            .unwrap();
        assert_eq!(
            s,
            r#"<!DOCTYPE html><html>
  <body>
    <pre><p></p></pre>
  </body>
</html>
"#
        );
    }

    #[test]
    fn test_pretty_with_suppressed_element_exact_match() {
        let mut xot = Xot::new();
        let body = xot.add_name("body");
        let root = xot.parse(r#"<html><body><p></p></body></html>"#).unwrap();
        let s = xot
            .html5()
            .serialize_string(
                Parameters {
                    indentation: Some(Indentation {
                        suppress: vec![body],
                    }),
                    ..Default::default()
                },
                root,
            )
            .unwrap();
        assert_eq!(
            s,
            r#"<!DOCTYPE html><html>
  <body><p></p></body>
</html>
"#
        );
    }

    #[test]
    fn test_pretty_with_suppressed_element_case_insensitive_match() {
        let mut xot = Xot::new();
        let body = xot.add_name("body");
        let root = xot.parse(r#"<html><BODY><p></p></BODY></html>"#).unwrap();
        let s = xot
            .html5()
            .serialize_string(
                Parameters {
                    indentation: Some(Indentation {
                        suppress: vec![body],
                    }),
                    ..Default::default()
                },
                root,
            )
            .unwrap();
        assert_eq!(
            s,
            r#"<!DOCTYPE html><html>
  <BODY><p></p></BODY>
</html>
"#
        );
    }

    #[test]
    fn test_pretty_with_suppressed_element_case_no_case_insensitive_match_for_non_xhtml_namespace()
    {
        let mut xot = Xot::new();
        let foo = xot.add_name("foo");
        let root = xot
            .parse(
                r#"<html><body><prefix:FOO xmlns:prefix="ns"><p></p></prefix:FOO></body></html>"#,
            )
            .unwrap();
        let s = xot
            .html5()
            .serialize_string(
                Parameters {
                    indentation: Some(Indentation {
                        suppress: vec![foo],
                    }),
                    ..Default::default()
                },
                root,
            )
            .unwrap();
        assert_eq!(
            s,
            r#"<!DOCTYPE html><html>
  <body>
    <prefix:FOO xmlns:prefix="ns">
      <p></p>
    </prefix:FOO>
  </body>
</html>
"#
        );
    }

    #[test]
    fn test_pretty_with_unknown_xhtml_element_treated_as_span() {
        let mut xot = Xot::new();
        let root = xot
            .parse(&format!(
                r#"<html><body><FOO xmlns:xhtml="{}"><p></p></FOO></body></html>"#,
                XHTML_NS
            ))
            .unwrap();
        let s = xot
            .html5()
            .serialize_string(
                Parameters {
                    indentation: Some(Default::default()),
                    ..Default::default()
                },
                root,
            )
            .unwrap();
        assert_eq!(
            s,
            r#"<!DOCTYPE html><html>
  <body><FOO><p></p></FOO></body>
</html>
"#
        );
    }

    #[test]
    fn test_pretty_with_suppressed_element_case_insensitive_match_no_ns_xhtml() {
        let mut xot = Xot::new();
        let xhtml_ns = xot.add_namespace(XHTML_NS);
        let body = xot.add_name_ns("body", xhtml_ns);
        let root = xot.parse(r#"<html><body><p></p></body></html>"#).unwrap();
        let s = xot
            .html5()
            .serialize_string(
                Parameters {
                    indentation: Some(Indentation {
                        suppress: vec![body],
                    }),
                    ..Default::default()
                },
                root,
            )
            .unwrap();
        assert_eq!(
            s,
            r#"<!DOCTYPE html><html>
  <body><p></p></body>
</html>
"#
        );
    }

    #[test]
    fn test_unrecognized_html_element_is_inline() {
        let mut xot = Xot::new();
        let root = xot
            .parse(r#"<html><body><FOO><p></p></FOO></body></html>"#)
            .unwrap();
        let s = xot
            .html5()
            .serialize_string(
                Parameters {
                    indentation: Some(Indentation { suppress: vec![] }),
                    ..Default::default()
                },
                root,
            )
            .unwrap();
        assert_eq!(
            s,
            r#"<!DOCTYPE html><html>
  <body><FOO><p></p></FOO></body>
</html>
"#
        );
    }
}
