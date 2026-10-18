// What the HTML 5 writer gives for its rules and quirks: the forced default
// namespace of SVG, MathML and XHTML elements, prefixes that are not written,
// boolean attributes, void elements, text that is not escaped, CDATA sections,
// non-breaking spaces, processing instructions, errors and indentation. The
// expected strings are what the writer gave before it rendered its tokens into
// one string, with the bytes written before an error.

use xot::output::html5::Parameters;
use xot::output::Indentation;
use xot::{Node, Xot};

const SVG: &str = "http://www.w3.org/2000/svg";
const XHTML: &str = "https://www.w3.org/1999/xhtml";

fn indented() -> Parameters {
    Parameters {
        indentation: Some(Indentation::default()),
        ..Default::default()
    }
}

fn case(xot: &mut Xot, name: &str) -> (Node, Parameters) {
    let parse = |xot: &mut Xot, s: &str| xot.parse(s).unwrap();
    match name {
        "svg_twice_under_declaring_ancestor" => {
            let s = format!(
                r#"<html xmlns:s="{SVG}"><body><s:svg><s:g/></s:svg><s:svg/></body></html>"#
            );
            (parse(xot, &s), Parameters::default())
        }
        "svg_twice_each_declaring" => {
            let s = format!(
                r#"<html><body><s:svg xmlns:s="{SVG}"/><s:svg xmlns:s="{SVG}"/></body></html>"#
            );
            (parse(xot, &s), Parameters::default())
        }
        "svg_twice_from_the_base" => {
            let s = format!(r#"<html xmlns:s="{SVG}"><body><s:svg/><s:svg/></body></html>"#);
            let doc = parse(xot, &s);
            let html = xot.document_element(doc).unwrap();
            let body = xot.first_child(html).unwrap();
            (body, Parameters::default())
        }
        "svg_default_namespace_then_prefixed" => {
            let s = format!(
                r#"<html><body><svg xmlns="{SVG}" xmlns:s="{SVG}"><s:g/></svg><s:svg xmlns:s="{SVG}"/></body></html>"#
            );
            (parse(xot, &s), Parameters::default())
        }
        "prefixed_xhtml_without_attribute" => {
            let s = format!(r#"<x:html xmlns:x="{XHTML}"><x:body><x:p>t</x:p></x:body></x:html>"#);
            (parse(xot, &s), Parameters::default())
        }
        "prefixed_xhtml_with_attribute" => {
            let s = format!(
                r#"<x:html xmlns:x="{XHTML}"><x:body x:lang="c"><x:p x:id="i">t</x:p></x:body></x:html>"#
            );
            (parse(xot, &s), Parameters::default())
        }
        "default_namespace_not_of_the_element" => {
            let s = r#"<html><body><x:p xmlns="urn:d" xmlns:x="urn:x"><q/></x:p></body></html>"#;
            (parse(xot, s), Parameters::default())
        }
        "boolean_attributes" => {
            let s = format!(
                r#"<html><body><input CHECKED="checked" disabled="DISABLED" x="y" xmlns:p="urn:p" p:checked="checked" xmlns:h="{XHTML}" h:selected="selected"/></body></html>"#
            );
            (parse(xot, &s), Parameters::default())
        }
        "void_elements" => {
            let s = r#"<html><head><META charset="utf-8"/><link/></head><body><br/><BR/><Hr></Hr><img src="a"/><br>t</br><x:br xmlns:x="urn:x"/></body></html>"#;
            (parse(xot, s), Parameters::default())
        }
        "script_and_style_raw" => {
            let s = r#"<html><head><script>a &amp;&amp; b &lt; c ]]&gt;</script><SCRIPT>x &lt; y</SCRIPT><Style>p &gt; q &amp; r</Style><sCrIpT>&lt;</sCrIpT></head><body></body></html>"#;
            (parse(xot, s), Parameters::default())
        }
        "cdata_markup_only" => {
            let s = "<html><body><m>a]]&gt;b &amp; &#xD; \u{85} \u{a0} c&lt;</m><m/><n>a&amp;b</n></body></html>";
            let doc = parse(xot, s);
            let m = xot.add_name("m");
            (
                doc,
                Parameters {
                    cdata_section_elements: vec![m],
                    ..Default::default()
                },
            )
        }
        "nbsp" => {
            let s = "<html><BODY title=\"a\u{a0}b\">foo\u{a0}bar<Weird>\u{a0}</Weird><i xmlns=\"urn:i\">\u{a0}&amp;&gt;</i></BODY></html>";
            (parse(xot, s), Parameters::default())
        }
        "pi_with_gt" => {
            let s = r#"<html><head><?ok data?><?lone?><?bad a>b?></head></html>"#;
            (parse(xot, s), Parameters::default())
        }
        "pi_in_namespace" => {
            let doc = parse(xot, "<html><body>t</body></html>");
            let html = xot.document_element(doc).unwrap();
            let body = xot.first_child(html).unwrap();
            let ns = xot.add_namespace("urn:pi");
            let target = xot.add_name_ns("t", ns);
            let pi = xot.new_processing_instruction(target, Some("d"));
            xot.append(body, pi).unwrap();
            (doc, Parameters::default())
        }
        "attribute_in_default_namespace_only" => {
            let doc = parse(xot, r#"<html><body xmlns="urn:d"><e/></body></html>"#);
            let html = xot.document_element(doc).unwrap();
            let body = xot.first_child(html).unwrap();
            let e = xot.first_child(body).unwrap();
            let ns = xot.add_namespace("urn:d");
            let a = xot.add_name_ns("a", ns);
            xot.set_attribute(e, a, "v");
            (doc, Parameters::default())
        }
        "element_without_prefix" => {
            let doc = parse(xot, r#"<html><body/></html>"#);
            let html = xot.document_element(doc).unwrap();
            let body = xot.first_child(html).unwrap();
            let ns = xot.add_namespace("urn:n");
            let name = xot.add_name_ns("e", ns);
            let e = xot.new_element(name);
            xot.append(body, e).unwrap();
            (doc, Parameters::default())
        }
        "indentation" => {
            let s = r#"<html><head><title>T</title></head><body><DIV><PRE><b>x</b>
 y</PRE><foo><p/></foo><Section><p>t</p></Section><span><p/></span><TextArea><p/></TextArea><svg xmlns="http://www.w3.org/2000/svg"><g/></svg></DIV></body></html>"#;
            let doc = parse(xot, s);
            let section = xot.add_name("section");
            (
                doc,
                Parameters {
                    indentation: Some(Indentation {
                        suppress: vec![section],
                    }),
                    ..Default::default()
                },
            )
        }
        "indentation_default" => {
            let s = r#"<html><body><DIV><Section><p>t</p></Section><x:a xmlns:x="urn:x"><x:b/></x:a></DIV></body></html>"#;
            (parse(xot, s), indented())
        }
        "long_names" => {
            let s = r#"<html><body><averyveryverylongname><SCRIPTSCRIPTSCRIPTSCRIPT>a&amp;b</SCRIPTSCRIPTSCRIPTSCRIPT></averyveryverylongname><blockquote>q</blockquote><FIGCAPTION>c</FIGCAPTION></body></html>"#;
            (parse(xot, s), indented())
        }
        "text_outside_elements" => {
            let doc = xot.new_document();
            let t = xot.new_text("a<b&c\u{a0}");
            xot.append(doc, t).unwrap();
            (doc, Parameters::default())
        }
        _ => panic!("unknown case {name}"),
    }
}

const CASES: &[&str] = &[
    "svg_twice_under_declaring_ancestor",
    "svg_twice_each_declaring",
    "svg_twice_from_the_base",
    "svg_default_namespace_then_prefixed",
    "prefixed_xhtml_without_attribute",
    "prefixed_xhtml_with_attribute",
    "default_namespace_not_of_the_element",
    "boolean_attributes",
    "void_elements",
    "script_and_style_raw",
    "cdata_markup_only",
    "nbsp",
    "pi_with_gt",
    "pi_in_namespace",
    "attribute_in_default_namespace_only",
    "element_without_prefix",
    "indentation",
    "indentation_default",
    "long_names",
    "text_outside_elements",
];

fn run(name: &str, indent: bool) -> String {
    let mut xot = Xot::new();
    let (node, mut parameters) = case(&mut xot, name);
    if indent {
        parameters.indentation = Some(Indentation::default());
    }
    let mut out = Vec::new();
    let result = xot.html5().serialize_write(parameters, node, &mut out);
    let out = String::from_utf8(out).unwrap();
    match result {
        Ok(()) => out,
        Err(e) => format!("{out}\nerror: {e} / {e:?}"),
    }
}

// (case, indented, output)
const EXPECTED: &[(&str, bool, &str)] = &[
    ("svg_twice_under_declaring_ancestor", false, "<!DOCTYPE html><html><body><svg xmlns=\"http://www.w3.org/2000/svg\"><g></g></svg><svg></svg></body></html>"),
    ("svg_twice_under_declaring_ancestor", true, "<!DOCTYPE html><html>\n  <body>\n    <svg xmlns=\"http://www.w3.org/2000/svg\">\n      <g></g>\n    </svg>\n    <svg></svg>\n  </body>\n</html>\n"),
    ("svg_twice_each_declaring", false, "<!DOCTYPE html><html><body><svg xmlns=\"http://www.w3.org/2000/svg\"></svg><svg xmlns=\"http://www.w3.org/2000/svg\"></svg></body></html>"),
    ("svg_twice_each_declaring", true, "<!DOCTYPE html><html>\n  <body>\n    <svg xmlns=\"http://www.w3.org/2000/svg\"></svg>\n    <svg xmlns=\"http://www.w3.org/2000/svg\"></svg>\n  </body>\n</html>\n"),
    ("svg_twice_from_the_base", false, "<!DOCTYPE html><body><svg xmlns=\"http://www.w3.org/2000/svg\"></svg><svg></svg></body>"),
    ("svg_twice_from_the_base", true, "<!DOCTYPE html><body>\n  <svg xmlns=\"http://www.w3.org/2000/svg\"></svg>\n  <svg></svg>\n</body>\n"),
    ("svg_default_namespace_then_prefixed", false, "<!DOCTYPE html><html><body><svg xmlns=\"http://www.w3.org/2000/svg\"><g></g></svg><svg xmlns=\"http://www.w3.org/2000/svg\"></svg></body></html>"),
    ("svg_default_namespace_then_prefixed", true, "<!DOCTYPE html><html>\n  <body>\n    <svg xmlns=\"http://www.w3.org/2000/svg\">\n      <g></g>\n    </svg>\n    <svg xmlns=\"http://www.w3.org/2000/svg\"></svg>\n  </body>\n</html>\n"),
    ("prefixed_xhtml_without_attribute", false, "<!DOCTYPE html><html xmlns=\"https://www.w3.org/1999/xhtml\"><body><p>t</p></body></html>"),
    ("prefixed_xhtml_without_attribute", true, "<!DOCTYPE html><html xmlns=\"https://www.w3.org/1999/xhtml\">\n  <body>\n    <p>t</p>\n  </body>\n</html>\n"),
    ("prefixed_xhtml_with_attribute", false, "<!DOCTYPE html><html xmlns=\"https://www.w3.org/1999/xhtml\"><body x:lang=\"c\"><p x:id=\"i\">t</p></body></html>"),
    ("prefixed_xhtml_with_attribute", true, "<!DOCTYPE html><html xmlns=\"https://www.w3.org/1999/xhtml\">\n  <body x:lang=\"c\">\n    <p x:id=\"i\">t</p>\n  </body>\n</html>\n"),
    ("default_namespace_not_of_the_element", false, "<!DOCTYPE html><html><body><x:p xmlns:x=\"urn:x\"><q></q></x:p></body></html>"),
    ("default_namespace_not_of_the_element", true, "<!DOCTYPE html><html>\n  <body>\n    <x:p xmlns:x=\"urn:x\">\n      <q></q>\n    </x:p>\n  </body>\n</html>\n"),
    ("boolean_attributes", false, "<!DOCTYPE html><html><body><input xmlns:p=\"urn:p\" xmlns:h=\"https://www.w3.org/1999/xhtml\" CHECKED disabled x=\"y\" p:checked=\"checked\" h:selected=\"selected\"></body></html>"),
    ("boolean_attributes", true, "<!DOCTYPE html><html>\n  <body><input xmlns:p=\"urn:p\" xmlns:h=\"https://www.w3.org/1999/xhtml\" CHECKED disabled x=\"y\" p:checked=\"checked\" h:selected=\"selected\"></body>\n</html>\n"),
    ("void_elements", false, "<!DOCTYPE html><html><head><META charset=\"utf-8\"><link></head><body><br><BR><Hr><img src=\"a\"><br>t<x:br xmlns:x=\"urn:x\"></x:br></body></html>"),
    ("void_elements", true, "<!DOCTYPE html><html>\n  <head>\n    <META charset=\"utf-8\">\n    <link>\n  </head>\n  <body><br><BR><Hr><img src=\"a\"><br>t<x:br xmlns:x=\"urn:x\"></x:br></body>\n</html>\n"),
    ("script_and_style_raw", false, "<!DOCTYPE html><html><head><script>a && b < c ]]></script><SCRIPT>x < y</SCRIPT><Style>p > q & r</Style><sCrIpT><</sCrIpT></head><body></body></html>"),
    ("script_and_style_raw", true, "<!DOCTYPE html><html>\n  <head><script>a && b < c ]]></script><SCRIPT>x < y</SCRIPT><Style>p > q & r</Style><sCrIpT><</sCrIpT></head>\n  <body></body>\n</html>\n"),
    ("cdata_markup_only", false, "<!DOCTYPE html><html><body><m><![CDATA[a]]]]><![CDATA[>b & \r \u{85} \u{a0} c<]]></m><m></m><n>a&amp;b</n></body></html>"),
    ("cdata_markup_only", true, "<!DOCTYPE html><html>\n  <body><m><![CDATA[a]]]]><![CDATA[>b & \r \u{85} \u{a0} c<]]></m><m></m><n>a&amp;b</n></body>\n</html>\n"),
    ("nbsp", false, "<!DOCTYPE html><html><BODY title=\"a&nbsp;b\">foo&nbsp;bar<Weird>&nbsp;</Weird><i xmlns=\"urn:i\">\u{a0}&amp;&gt;</i></BODY></html>"),
    ("nbsp", true, "<!DOCTYPE html><html>\n  <BODY title=\"a&nbsp;b\">foo&nbsp;bar<Weird>&nbsp;</Weird><i xmlns=\"urn:i\">\u{a0}&amp;&gt;</i></BODY>\n</html>\n"),
    ("pi_with_gt", false, "<!DOCTYPE html><html><head><?ok data><?lone>\nerror: Processing instruction with > in HTML: a>b / ProcessingInstructionGtInHtml(\"a>b\")"),
    ("pi_with_gt", true, "<!DOCTYPE html><html>\n  <head>\n    <?ok data>\n    <?lone>\n    \nerror: Processing instruction with > in HTML: a>b / ProcessingInstructionGtInHtml(\"a>b\")"),
    ("pi_in_namespace", false, "<!DOCTYPE html><html><body>t\nerror: Namespace in processing instruction target / NamespaceInProcessingInstruction"),
    ("pi_in_namespace", true, "<!DOCTYPE html><html>\n  <body>t\nerror: Namespace in processing instruction target / NamespaceInProcessingInstruction"),
    ("attribute_in_default_namespace_only", false, "<!DOCTYPE html><html><body xmlns=\"urn:d\"><e\nerror: Missing prefix / MissingPrefix(\"urn:d\")"),
    ("attribute_in_default_namespace_only", true, "<!DOCTYPE html><html>\n  <body xmlns=\"urn:d\">\n    <e\nerror: Missing prefix / MissingPrefix(\"urn:d\")"),
    ("element_without_prefix", false, "<!DOCTYPE html><html><body>\nerror: Missing prefix / MissingPrefix(\"urn:n\")"),
    ("element_without_prefix", true, "<!DOCTYPE html><html>\n  <body>\n    \nerror: Missing prefix / MissingPrefix(\"urn:n\")"),
    ("indentation", false, "<!DOCTYPE html><html>\n  <head>\n    <title>T</title>\n  </head>\n  <body>\n    <DIV><PRE><b>x</b>\n y</PRE><foo><p></p></foo><Section><p>t</p></Section><span><p></p></span><TextArea><p></p></TextArea><svg xmlns=\"http://www.w3.org/2000/svg\"><g></g></svg></DIV>\n  </body>\n</html>\n"),
    ("indentation", true, "<!DOCTYPE html><html>\n  <head>\n    <title>T</title>\n  </head>\n  <body>\n    <DIV><PRE><b>x</b>\n y</PRE><foo><p></p></foo><Section><p>t</p></Section><span><p></p></span><TextArea><p></p></TextArea><svg xmlns=\"http://www.w3.org/2000/svg\"><g></g></svg></DIV>\n  </body>\n</html>\n"),
    ("indentation_default", false, "<!DOCTYPE html><html>\n  <body>\n    <DIV>\n      <Section>\n        <p>t</p>\n      </Section>\n      <x:a xmlns:x=\"urn:x\">\n        <x:b></x:b>\n      </x:a>\n    </DIV>\n  </body>\n</html>\n"),
    ("indentation_default", true, "<!DOCTYPE html><html>\n  <body>\n    <DIV>\n      <Section>\n        <p>t</p>\n      </Section>\n      <x:a xmlns:x=\"urn:x\">\n        <x:b></x:b>\n      </x:a>\n    </DIV>\n  </body>\n</html>\n"),
    ("long_names", false, "<!DOCTYPE html><html>\n  <body><averyveryverylongname><SCRIPTSCRIPTSCRIPTSCRIPT>a&amp;b</SCRIPTSCRIPTSCRIPTSCRIPT></averyveryverylongname><blockquote>q</blockquote><FIGCAPTION>c</FIGCAPTION></body>\n</html>\n"),
    ("long_names", true, "<!DOCTYPE html><html>\n  <body><averyveryverylongname><SCRIPTSCRIPTSCRIPTSCRIPT>a&amp;b</SCRIPTSCRIPTSCRIPTSCRIPT></averyveryverylongname><blockquote>q</blockquote><FIGCAPTION>c</FIGCAPTION></body>\n</html>\n"),
    ("text_outside_elements", false, "<!DOCTYPE html>a&lt;b&amp;c&nbsp;"),
    ("text_outside_elements", true, "<!DOCTYPE html>a&lt;b&amp;c&nbsp;"),
];

#[test]
fn test_html5_writer_cases() {
    assert_eq!(EXPECTED.len(), CASES.len() * 2);
    for (name, indent, expected) in EXPECTED {
        assert_eq!(run(name, *indent), *expected, "{name} {indent}");
    }
}

#[test]
fn test_html5_to_string_equals_write() {
    for name in CASES {
        let mut xot = Xot::new();
        let (node, parameters) = case(&mut xot, name);
        let mut out = Vec::new();
        let written = xot
            .html5()
            .serialize_write(parameters.clone(), node, &mut out)
            .map(|_| String::from_utf8(out).unwrap());
        let string = xot.html5().serialize_string(parameters, node);
        assert_eq!(
            written.map_err(|e| e.to_string()),
            string.map_err(|e| e.to_string()),
            "{name}"
        );
    }
}
