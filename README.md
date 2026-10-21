# Xot - XML Object Tree

This is a fork of [faassen/xot](https://github.com/faassen/xot) carrying fixes that are not released upstream yet; see CHANGES.md.

[![Crates.io](https://img.shields.io/crates/v/xot.svg)](https://crates.io/crates/xot)
[![Documentation](https://docs.rs/xot/badge.svg)](https://docs.rs/xot)

Xot is a full-featured XML tree access and manipulation library for Rust. With
Xot you can load XML data into an object tree which you can then manipulate,
and serialize back out to XML.

[API Documentation](https://docs.rs/xot)

## Features

- Extensive XML tree access and manipulation API.

- You can access the parent of a node! A lot of Rust XML tree libraries do not
  support this features: [xmltree-rs](https://crates.io/crates/xmltree),
 [RustyXML](https://crates.io/crates/RustyXML),
 [minidom](https://crates.io/crates/minidom) and
 [elementtree](https://crates.io/crates/elementtree) all lack this ability for
 instance. [roxmltree](https://github.com/RazrFalcon/roxmltree) does allow
 parent access, but is read-only.

- Move nodes out of trees, into trees, into other trees with ease.

- Access to the entire XML structure, including comments and processing
  instructions, and support for namespace and attribute nodes.

- Compact storage of XML data, with efficient traversal.

- Parse XML into a tree, and serialize back to XML.

- Pretty-printing. Removal of non-significant whitespace.

## Support

Xot has well-rounded XML support. Supported are namespaces for both elements
and attributes. You can also get comments and processing instructions.

Xot exposes the entire XML structure for access and programmatic manipulation.

## Limitations

There are the following limitations:

- A document type declaration is accepted and ignored: nothing external is
  read, and the declarations of the internal subset are not applied (no
  entity expansion, no attribute default values, no attribute types). A document
  that depends on them gives `ParseError::DtdUnsupported`: a reference to a
  declared entity, or to any entity other than the predefined ones when
  there is an external subset; an attribute default or a parameter-entity
  reference in the internal subset; or an attribute value that the declared
  type of the attribute would normalize. Only the first definition of an
  attribute of an element type counts, as in XML 1.0.
- Only XML 1.0 is supported. 

## Why another XML library for Rust?

Use Xot if you need to be able to access XML as a tree, mutate it where
necessary, and serialize it back out again. Various Rust XML tree libraries
exist but none seem particularly interested in a complete tree manipulation
API, including parent access, so Xot aims to fill that gap.

If you need to access XML as a tree, very high performance, and do not need
tree manipulation or serialization at all, I recommend
[roxmltree](https://github.com/RazrFalcon/roxmltree).

For extensive features such as Xpath as well as tree manipulation, you can use
[libxml](https://github.com/KWARC/rust-libxml) which wraps the libxml2 C
library. Unfortunately its tree access facilities aren't entirely complete, and
I've experienced odd errors and even crashes when using it; this was part of
the reason I created Xot.

If you need to process big XML data with high performance, you may not want to
use a tree library and instead use one of the various XML parser/serialization
libraries.

## Implementation notes

Xot keeps its nodes in an arena of its own, whose operations are adapted from
those of the [indextree](https://docs.rs/indextree/latest/indextree/) crate
(version 4.7.2), which Xot used before. Xot completely wraps this arena.
