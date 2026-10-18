<!-- next-header -->

## [Unreleased] - ReleaseDate

### Breaking changes

- `ParseError` has new variants `UnsupportedEncoding` and
  `EncodingConflict`. `Xot::parse_bytes` returns `UnsupportedEncoding` when
  the encoding declaration names an encoding that is not known, or when the
  document is in an encoding that cannot be decoded (UTF-32 or EBCDIC), and
  `EncodingConflict` when the encoding declaration does not match the byte
  order mark or the way the first characters are written (see below).

- `Xot::parse_bytes` rejects a document whose encoding declaration does not
  match its byte order mark, where the byte order mark used to decide the
  encoding, and a document in UTF-16 without a byte order mark that does not
  declare UTF-16 (see below).

- `Xot::parse_bytes` no longer takes the encoding from `charset=` or
  `encoding=` outside the XML declaration (see below): a document without a
  byte order mark or an encoding declaration is UTF-8. For example, an
  HTML-like document with `<meta charset="iso-8859-1"/>` and the byte 0xE9
  in its text used to give "é", and now gives
  `ParseError::InvalidByteSequence`, as a lone 0xE9 is not valid UTF-8.

- XML output escapes more characters, so that text and attribute values
  survive parsing the output again (see below). Output that contains these
  characters changes.

- `output::xml::Parameters` and `output::TokenSerializeParameters` have a new
  member, `unescaped_cr`. Code that creates them with a full struct literal,
  without `..Default::default()`, no longer compiles.

- `Error` has a new variant `InvalidCharacter`. XML serialization returns it
  instead of writing a character that XML 1.0 does not allow (see below).

- `ParseError` has a new variant `InvalidByteSequence`. `Xot::parse_bytes`
  returns it for a byte sequence that is not valid in the encoding of the
  document, which it used to replace by U+FFFD REPLACEMENT CHARACTER (see
  below).

- The parser reads the value of a namespace declaration like any attribute
  value (see below): a namespace with an entity or character reference in
  it changes, and an unknown or unclosed entity reference in it is now a
  parse error.

- A character reference to a character that XML 1.0 does not allow, such as
  `&#x1;`, or with a sign, such as `&#+65;`, is now a parse error (see
  below).

- `Xot::parse_bytes` decodes documents in US-ASCII, ISO-8859-1, ISO-8859-9
  and TIS-620 (ISO-8859-11) as these encodings are defined, not as
  windows-1252, windows-1254 and windows-874 (see below). In the ISO 8859
  encodings the bytes 0x80 to 0x9F are now the control characters U+0080 to
  U+009F, and in US-ASCII a byte of 0x80 or more is now an error.

- The parser reads `p:xmlns` as an attribute in the namespace bound to `p`,
  not as a default namespace declaration (see below), so an undeclared
  prefix there is now a parse error.

- `ParseError` has a new variant `InvalidNamespaceDeclaration`, and `Error`
  new variants `XmlnsAttribute` and `InvalidNamespaceDeclaration`. The
  parser rejects namespace declarations and attributes that Namespaces in
  XML 1.0 does not allow, and XML serialization rejects such a namespace
  declaration and an attribute that would read back as a namespace
  declaration (see below).

- `ParseError` has a new variant `InvalidPiTarget`: the parser rejects a
  processing instruction with the target `xml` in any case, and XML
  serialization rejects one made through the API with `Error::InvalidTarget`,
  which was not returned before (see below).

- `Xot::create_missing_prefixes` no longer declares a namespace that already
  has a prefix in scope on an ancestor of the node (see below). A node that
  is serialized on its own after it is detached may now give
  `Error::MissingPrefix`: call `create_missing_prefixes` after detaching it,
  or use `Xot::clone_with_prefixes`.

### Features added

- `unescaped_cr` in `output::xml::Parameters` and
  `output::TokenSerializeParameters` writes a carriage return in text as is,
  while it is still escaped in attribute values, as some other serializers
  do. It applies to all the XML serialization functions, those with a
  normalizer too, and to `Xot::tokens` and `Xot::pretty_tokens`.

- `MutableNodeMap` implements `Extend<(K, V)>`, so that the maps that
  `Xot::namespaces_mut` and `Xot::attributes_mut` give take several pairs at
  once. `extend` gives the same tree as `insert` of each pair in turn, but
  looks through the nodes of the map once instead of once for each pair, and
  again only for a key that may be in the map already. Adding 14 namespaces
  to an element with `extend` takes 0.3 to 0.5 times as long as adding them
  one at a time with `insert`, and 56 namespaces 0.2 times as long.

- `Xot::parsed_tree` tells whether a node is in a tree that a parse put in
  storage in document order and whose structure has not changed since, and
  gives that tree as a `ParsedTree`. `ParsedTree::index` is the place of a
  node in document order (the order of `Xot::all_descendants` of the
  document node, namespace and attribute nodes included), `ParsedTree::node`
  the node at a place, `ParsedTree::root` the document node and
  `ParsedTree::node_count` the number of nodes; these are arithmetic on the
  `Node`, so that the nodes of such a tree can be put in document order
  without a walk of the tree. A `Xot` keeps the tree of a parse that
  succeeds when the parse puts its nodes in new slots right after the trees
  it keeps already: no slot of a removed node is free when the parse
  starts, and no node was made after the last kept tree. It drops a tree
  when its structure changes: a node of it is moved, detached or removed,
  or a node is added to it. A change of a value keeps the tree, and so does
  a text that joins a text of the tree. `Xot::dropped_parsed_trees` lists
  the trees no longer kept, in the order of the drops; a dropped tree is
  never kept again.

### Bugs fixed

- `NameId`, `PrefixId` and `NamespaceId` were 16 bits, and a `Xot` that had
  seen more than 65,536 distinct names (or prefixes, or namespaces) gave the
  next one the id of an earlier one, with no error: the new name then read
  back, and was serialized, as the earlier name. The ids are now 32 bits,
  and going past 2^32 entries of a kind panics instead of reusing an id.
  Nodes stay the same size.

- XML output now escapes characters as the XML output method of
  Serialization 3.1 (<https://www.w3.org/TR/xslt-xquery-serialization-31/>)
  requires, so that they survive parsing the output again. Tab, newline and
  carriage return in attribute values are written as `&#x9;`, `&#xA;` and
  `&#xD;`; before, parsing turned them into spaces. A carriage return in
  text is written as `&#xD;`; before, parsing turned it into a newline. NEL
  (#x85), LINE SEPARATOR (#x2028) and the control characters #x7F to #x9F
  are written as character references in both. Text in a CDATA section
  element is split into CDATA sections around these characters. HTML 5
  output is unchanged.

- XML output wrote characters that XML 1.0 does not allow as they are, so
  the output was not well-formed: the control characters #x1 to #x1F other
  than tab, newline and carriage return, and #x0, #xFFFE and #xFFFF. XML 1.0
  does not allow them as character references either. Serialization now
  fails with `Error::InvalidCharacter` when one of them is in text, an
  attribute value, a comment, a processing instruction or a namespace
  declaration, as Serialization 3.1 requires (section 5.1.1, error
  SERE0006). The error names the character, the node and where in the node
  the character is. `Xot::tokens`, `Xot::pretty_tokens` and HTML 5 output
  are unchanged.

- `Xot::parse_bytes` could panic while determining the encoding: on input
  shorter than 4 bytes, on a UTF-32 byte order mark, on an encoding
  declaration naming an unknown encoding, and on a document without an
  encoding declaration where `encoding=` or `charset=` appears in the first
  512 bytes (in an attribute or in text) followed by something that is not
  an encoding name. The encoding now follows the rules of the XML
  specification: a byte order mark decides it; without one the encoding
  declaration in the XML declaration does; without either the document is
  UTF-8. Text outside the XML declaration is no longer taken for an encoding
  declaration, and an encoding declaration with spaces around `=` is now
  recognized. A name that the WHATWG Encoding Standard has for its
  replacement encoding, such as `ISO-2022-KR`, which only decodes to U+FFFD
  REPLACEMENT CHARACTER, is now `ParseError::UnsupportedEncoding` too. The
  `xhtmlchardet` dependency is no longer used.

- `Xot::parse_bytes` decoded a document whose encoding declaration names
  another encoding than its byte order mark as the byte order mark says.
  XML 1.0 (section 4.3.3 and appendix F) makes it a fatal error for a
  document to be in another encoding than the one it declares, so this is
  now `ParseError::EncodingConflict`: a UTF-8 byte order mark goes only with
  a declaration of UTF-8, and a UTF-16 one with UTF-16, another name of
  UTF-16 such as UCS-2, or the UTF-16LE or UTF-16BE of its byte order.
  Without a byte order mark, a document whose first characters are single
  bytes cannot declare UTF-16, and a document that starts with `<?` in
  UTF-16 has to declare UTF-16 in that byte order. The spans of
  `UnsupportedEncoding` and `EncodingConflict` are byte offsets in the bytes
  given to `parse_bytes`, byte order mark included.

- The parser took the value of a namespace declaration as it was written,
  so `xmlns:p="a&amp;b"` declared the namespace `a&amp;b` instead of `a&b`,
  and character references were not expanded either. The namespace is the
  value of the declaration after attribute-value normalization, as Namespaces
  in XML 1.0 (section 3) requires, so the parser now reads it like any other
  attribute value.

- Namespace declarations were written without escaping, so a namespace with
  `&`, `<` or `"` gave output that is not well-formed. The XML serializer now
  escapes the namespace like an attribute value, including tab, newline and
  carriage return, so that it survives parsing the output again; the HTML 5
  serializer escapes it like the value of an attribute in a namespace.

- The parser took any attribute with the local name `xmlns` for a default
  namespace declaration, so `p:xmlns="urn:v"` put the element and its
  descendants in the namespace `urn:v`, even when the prefix `p` was not
  declared. Namespaces in XML 1.0 (section 3) makes only `xmlns` and
  `xmlns:` followed by a prefix declare namespaces. `p:xmlns` is now an
  attribute with the local name `xmlns` in the namespace bound to `p`, which
  must be declared as for any other attribute. The serializers write such an
  attribute with its prefix, so the output reads back the same.

- The parser left carriage returns in CDATA sections, comments and
  processing instructions as they were. XML 1.0 (section 2.11) makes a
  carriage return followed by a newline, and a carriage return that is not,
  a newline everywhere in the document, as the parser already did in text
  and attribute values. Spans and the positions in parse errors are still
  positions in the source text.

- The parser accepted character references to characters that XML 1.0 does
  not allow, such as `&#x1;`, `&#0;` and `&#xFFFE;`, and put these characters
  in the text or attribute value. XML 1.0 allows a reference only to a
  character that matches the `Char` production (section 4.1, "WFC: Legal
  Character"), so such a reference in text, in an attribute value or in a
  namespace declaration is now a parse error, `ParseError::InvalidEntity`
  with the span of the reference, as a reference to a surrogate such as
  `&#xD800;` already was. The same goes for a reference with a sign, such as
  `&#+65;` or `&#x+41;`, which the parser took for `A`: the `CharRef`
  production allows only digits.

- `Xot::parse_bytes` replaced a byte sequence that is not valid in the
  encoding of the document by U+FFFD REPLACEMENT CHARACTER and parsed the
  result. The XML specification makes this a fatal error (section 4.3.3), so
  it now gives `ParseError::InvalidByteSequence`, with the name of the
  encoding and the byte offsets of the sequence in the input, byte order
  mark included.

- `Xot::parse_bytes` decoded a document in US-ASCII, ISO-8859-1, ISO-8859-9
  or TIS-620 (ISO-8859-11) as windows-1252, windows-1254 or windows-874, as
  the WHATWG Encoding Standard, which `encoding_rs` follows, makes these
  names stand for those encodings. So in the ISO 8859 encodings the bytes
  0x80 to 0x9F became characters such as `€` instead of the control
  characters U+0080 to U+009F, and a US-ASCII document could have bytes of
  0x80 and more. XML 1.0 (section 4.3.3) asks for a name that IANA registers
  to be taken as the encoding IANA registers it for, so these are now decoded
  as defined: US-ASCII gives `ParseError::InvalidByteSequence` for a byte of
  0x80 or more; ISO-8859-1 maps each byte to the character with the same
  number; ISO-8859-9 does the same but for six Turkish letters; TIS-620 has
  the Thai characters from 0xA1 on, and gives
  `ParseError::InvalidByteSequence` for the bytes it has no character for.
  TIS 620 itself has no character for 0xA0, but as IANA registers
  ISO-8859-11 as another name for it, 0xA0 is NO-BREAK SPACE, as in
  ISO-8859-11. This holds for the names IANA registers for these encodings,
  in any case, apart from those with `:` such as ISO_8859-1:1987, which an
  encoding declaration cannot have: US-ASCII, iso-ir-6, ANSI_X3.4-1968,
  ANSI_X3.4-1986, ISO646-US, us, IBM367, cp367 and csASCII, as well as
  ASCII; ISO-8859-1, ISO_8859-1, iso-ir-100, latin1, l1, IBM819, CP819 and
  csISOLatin1; ISO-8859-9, ISO_8859-9, iso-ir-148, latin5, l5 and
  csISOLatin5; TIS-620, csTIS620 and ISO-8859-11. Several of these were not
  known before. windows-1252, windows-1254 and windows-874 themselves, and
  the other WHATWG names for them such as `iso8859-1`, still stand for
  those encodings.

- Serializing a text node without a parent, such as one made with
  `Xot::new_text`, panicked: with `Xot::to_string` and the other XML
  serialization functions, with `Xot::tokens` and `Xot::pretty_tokens`, and
  with the HTML 5 serializer. The HTML 5 serializer also panicked on a text
  node directly under a document node, as `Xot::parse_fragment` can produce.

- A writer that fails during serialization now gives `Error::Io` instead of a
  panic.

- `Xot::clone_with_prefixes` added the namespace declarations it needs from
  outer scope in an order that varied from one run to the next, as it took
  them from a hash map. It now adds them in the document order of the
  declarations that bind them: those of the outermost ancestor first, and
  those of one element in the order they are declared. Thanks to Frank
  Arensmeier for the fix that made the order the same in every run!

- The parser accepted namespace declarations that Namespaces in XML 1.0
  does not allow: two declarations of one prefix, or of the default
  namespace, on one element; a declaration that undeclares a prefix, such
  as `xmlns:p=""`, which only XML 1.1 allows; a declaration of the prefix
  `xmlns`; a declaration that binds `xml` to another namespace than
  `http://www.w3.org/XML/1998/namespace`, or another prefix, or the default
  namespace, to that namespace; and a declaration that binds a prefix, or
  the default namespace, to `http://www.w3.org/2000/xmlns/`. It also
  accepted two attributes with the same expanded name, such as `a:x` and
  `b:x` with `a` and `b` bound to one namespace (section 6.3). Each of these
  is now a parse error with the span of the name of the declaration or of
  the second attribute: `ParseError::DuplicateAttribute` for the duplicates,
  and the new `ParseError::InvalidNamespaceDeclaration` for the others. The
  generator of the `proptest` feature no longer makes such declarations.

- XML serialization wrote an attribute made through the API that reads back
  as a namespace declaration: an attribute with the local name `xmlns` in no
  namespace became a default namespace declaration, and an attribute in the
  namespace `http://www.w3.org/2000/xmlns/` needed a prefix for that
  namespace, which cannot be declared. Serialization now fails with the new
  `Error::XmlnsAttribute` instead. `Xot::tokens`, `Xot::pretty_tokens` and
  HTML 5 output are unchanged.

- XML serialization wrote the namespace declarations made through the API
  that the parser now rejects (see above): a declaration that undeclares a
  prefix, such as `xmlns:p=""`, which XML 1.0 output cannot have
  (Serialization 3.1, error SEPM0010); a declaration of the prefix `xmlns`;
  one that binds `xml` to another namespace than its own; and one that binds
  another prefix, or the default namespace, to the namespace of `xml` or of
  `xmlns`. Such a declaration for the namespace of `xml` was left out, so
  that the names that used it read back in another namespace or not at all.
  Serialization now fails with the new `Error::InvalidNamespaceDeclaration`
  instead. `Xot::tokens`, `Xot::pretty_tokens` and HTML 5 output are
  unchanged.

- The parser accepted a processing instruction with the target `xml` in any
  case, such as `<?XML x?>` or `<?xml?>`, which XML 1.0 does not allow
  (production [17] PITarget). It is now `ParseError::InvalidPiTarget`, with
  the span of the target. An XML declaration with a tab, newline or carriage
  return after `<?xml`, which XML 1.0 allows, was read as such a processing
  instruction, as the tokenizer takes only `<?xml` followed by a space for
  an XML declaration; the parser now reads it as the XML declaration. XML
  serialization wrote a processing instruction with the target `xml` made
  through the API, which at the start of a document read back as an XML
  declaration; it now fails with `Error::InvalidTarget`. `Xot::tokens`,
  `Xot::pretty_tokens` and HTML 5 output are unchanged.

- `Xot::prefix_for_namespace` gave `None` as soon as it came to a
  declaration of a prefix that an inner element declares again, so it
  missed the prefixes declared after it: with
  `<a xmlns:p="urn:A" xmlns:q="urn:C"><b xmlns:p="urn:B"/></a>` it gave no
  prefix for `urn:C` on `b`. It now skips the shadowed declaration and gives
  `q`.

- `Xot::create_missing_prefixes` numbered the prefixes it creates, and
  declared them, in an order that varied from one run to the next, as it
  took the namespaces from a hash set. It now numbers and declares them in
  the order in which the namespaces are found in the tree. It could also
  declare a prefix such as `n0` that was in scope already for another
  namespace, so that this namespace had no prefix any more and serializing
  gave `Error::MissingPrefix`; it now leaves out the prefixes that are in
  scope on the node or declared in the tree. And it took the namespaces
  declared on the ancestors of the node for missing ones: for `s` in
  `<r xmlns:p="urn:p"><s><p:t/></s></r>` it declared `n0` for `urn:p`, so
  `p:t` was written `n0:t`, and for an attribute such as `xml:lang` it
  declared a prefix for the namespace of `xml`, which serialization left
  out, so the output had an undeclared prefix. It now starts from the
  declarations in scope on the parent of the node.

- `Xot::parse_fragment` panicked with "Cannot close document node" on an end
  tag without a start tag, such as `</a>`. This is now
  `ParseError::InvalidCloseTag` with the span of the name in the end tag,
  the error for an end tag that does not match its start tag.

- `Xot::reverse_children` repeated the last child without end when the node
  had two or more children, its namespace and attribute nodes counted: for
  `<a><b/><c/></a>` it gave `c`, `c`, `c` and so on, and for
  `<a x="1"><b/></a>` it gave `b`, `b` and so on. It took the reverse of
  indextree's `Children` iterator, whose `next_back` (indextree 4.7.2) moves
  the front of the iterator instead of the back. It now walks back from the
  last child and gives the children in reverse order: `c` and then `b` for
  the first element, and `b` for the second.

- A `Node` of a removed node compared equal to every later node in its slot
  once the slot had been taken 32,767 times, as indextree's stamp stops
  changing there. A slot now has a `u32` generation, one more for each node
  that takes it, and a slot whose generation reaches `u32::MAX` is not used
  again, so a removed node never compares equal to a later one.

### Optimizations

- Xot keeps its nodes in an arena of its own instead of indextree's. A node
  takes 64 bytes instead of 128: its value, the positions of its parent, its
  siblings and its first and last child as `u32`, and a `u32` generation. The
  nodes are kept in chunks of 65,536 (4 MiB): the first chunk grows like a
  vector, and each later one is made with room for all its nodes and never
  moves, so that growing the arena copies at most the first chunk, where the
  vector that doubled copied all the nodes and held two copies of them for a
  moment. A `Node` takes 8 bytes instead of 16. On a generated document of 25
  megabytes with 2.0M nodes, the heap after parsing takes 130.9 MiB instead of
  262.9 MiB, the heap peaks at 302.0 MiB instead of 530.0 MiB while parsing,
  parsing peaks at 429.5 MiB of resident memory instead of 617.6 MiB, and it
  takes 389 ms instead of 453 ms. Two copies of the document with `clone_node`
  and their serialization peak at 800.4 MiB of resident memory instead of
  1306.4 MiB, with 436.7 MiB of heap instead of 1140.7 MiB. Calls of
  `Xot::parent`, `Xot::first_child`, `Xot::last_child`, `Xot::next_sibling`
  and `Xot::previous_sibling` take up to 1.6 times as long, as each looks up
  the chunk of its node before it reads the node: a walk up through
  `Xot::parent` 1.8 ns a step instead of 1.2 ns, one through the whole
  document by `first_child`, `next_sibling` and `parent` 5.4 ns a node instead
  of 4.3 ns, and `last_child` of each of 100,000 elements 16 to 17 ns instead
  of 13 ns; other calls that look up a node do too, such as `Xot::text_mut`,
  with which `Text::set` takes 15.1 ns instead of 13.2 ns. Detaching an
  element of a parsed document takes 1.1 to 1.35 times as long (76 ns instead
  of 67 ns for one between two elements, and 71 ns instead of 53 ns for one
  without children), and removing an element with a text 1.1 times (158 ns
  instead of 141 ns), as `Xot::detach` and `Xot::remove` look up the chunks of
  the node, of its siblings and of its parent. A loop that takes the last
  child of each of 100,000 elements, an element appended to it, and removes it
  takes 1.4 to 1.6 times as long (88 to 108 ns an element instead of 63 to 67
  ns), where removing these children alone takes 1.2 times as long (56 to 58
  ns instead of 47 ns). The iterators, `Xot::ancestors`, `Xot::children`,
  `Xot::descendants` and the others, take 0.4 to 0.9 times as long as before,
  but for two of them taken in step (see below).

- Attribute and namespace maps, and `first_child`, read each node once as
  they go through children, and the small accessors are inline, so that a
  caller in another crate reads a node once for several of them.

- `Xot::following` and `Xot::all_following` go through the nodes with an
  iterator of the arena that reads each node once, where they took the first
  child, the next sibling and the parent of each node in calls of their own.
  `Xot::descendants`, `Xot::traverse` and `Xot::reverse_traverse` take the
  value of each node from the step that reads the node, where the step and
  then the check for namespace and attribute nodes each read it, and the end
  of a node without children takes the slot that its start read.
  `Xot::next_sibling` and `Xot::previous_sibling` read the node and its
  sibling once each, and a sibling in the chunk of the node without looking
  up that chunk again. On a generated document of 25 megabytes with 2.0M
  nodes, `following` of the first item takes 1.9 ns a node instead of 4.9 ns,
  `traverse` 1.1 to 2.2 ns an edge instead of 1.8 to 3.3 ns, `descendants`
  2.1 to 2.3 ns a node instead of 2.3 to 2.5 ns, and `next_sibling` and
  `previous_sibling` 1.3 ns instead of 2.4 ns.

- `Xot::reverse_preorder` and `Xot::all_reverse_preorder` go through the
  nodes with an iterator of the arena that reads each node once, where they
  took the previous sibling, the last child and the parent of each node in
  calls of their own, and `reverse_preorder` read each node again to leave
  out namespace and attribute nodes. On a generated document of 25 megabytes
  with 2.0M nodes, `all_reverse_preorder` from the last node takes 1.9 ns a
  node instead of 2.3 ns, and `reverse_preorder` 2.1 ns instead of 3.8 ns.

- `Xot::traverse`, `Xot::all_traverse`, `Xot::descendants` and
  `Xot::all_descendants` give iterators of their own, whose steps the compiler
  puts inline into the loops that take them, and through which `count`,
  `for_each` and the other methods that go through all the edges of a
  traversal take the end of a node without children together with its start.
  From a node that is not removed, and whose slot no later node took, they
  read only the positions of the nodes, as no node of its tree is removed
  then, and take the generation of each node from its slot; from another node
  they take the steps of before. `Xot::deep_equal`, `Xot::deep_equal_xpath`
  and `Xot::advanced_deep_equal` go through the two trees in one loop of their
  own, and compare the values that the traversals read. On a generated
  document of 25 megabytes with 2.0M nodes (ns a node): a loop over `traverse`
  that reads the text of each node takes 4.1 instead of 6.2,
  `traverse(..).count()` 2.1 instead of 3.4, `all_traverse(..).count()` 1.8
  instead of 3.4, `descendants(..).count()` 1.9 instead of 2.3, a loop over
  `descendants` that looks for elements 1.9 instead of 3.6, two `traverse`
  taken in step with `zip` 6.9 instead of 8.1, `deep_equal` of the document
  and a copy of it 16.7 instead of 16.9, `deep_equal_xpath` 19.8 instead of
  40.2, and `traverse` and `descendants` taken one step at a time through `dyn
  Iterator` 6.0 instead of 9.6 and 4.0 instead of 8.4. Two `all_traverse` or
  two `descendants` taken in step with `zip` take longer than before: 5.8
  instead of 4.3, and 6.2 instead of 5.4 (8.4 instead of 7.7 where the loop
  compares the values of the nodes too).

- Removing a node no longer reads the nodes it refers to. A removed node
  keeps the generations of its parent and its previous sibling, which
  `Xot::remove` knows as it removes a tree in document order, so that their
  slots are not read again; the generations of the other nodes it refers to
  are read from their slots when they are needed, as no later node takes
  these slots before one takes the slot of the removed node. `Xot::remove`
  and `Xot::detach` of a namespace or attribute node no longer look at its
  siblings, which are of its kind and so never texts that join. A new node
  takes a free slot with one look at the slot instead of two. On a generated
  document of 25 megabytes with 2.0M nodes, removing the document takes 7.6
  ns a node instead of 8.2 ns, removing a copy of it 7.5 ns instead of 8.3
  ns, and removing 14 namespace nodes of each of 100,000 elements 18.7 ns a
  node instead of 19.0 ns; removing a tree of elements without texts or
  attributes takes 6.6 ns a node instead of 6.3 ns, and a loop that makes an
  element and removes it again takes 1.3 to 1.4 times as long, 12.6 to 13.5
  ns instead of 9.9 ns.

- `Xot::parse` and `Xot::parse_fragment` no longer collect the span
  information that `Xot::parse_with_span_info` and
  `Xot::parse_fragment_with_span_info` return, a span for every element,
  attribute, text, comment and processing instruction, which they dropped
  when they were done. They keep only the spans that their errors can
  report: those of the first text and of the second element under the
  document node, and those of the elements that are open. The errors and
  their spans do not change. On a generated document of 25 megabytes with
  2.0M nodes the span information took 132.0 MiB when parsing ended, and up
  to 198 MiB while it grew; parsing now peaks at 154.9 MiB of heap instead
  of 302.0 MiB (the input text included) and at 167.8 MiB of resident
  memory instead of 429.5 MiB, and takes 284 ms instead of 383 ms.

- Parsing allocates about once for each text node and attribute value, for the
  string that the node keeps, instead of about 3.3 times for each node. Names,
  prefixes and values borrow from the text until they go into the tree. Each
  name and prefix is looked up in the `Xot` once in a parse, and after that by
  the text it was read from, so that a name no longer takes a new `String` for
  each element. An end tag with the prefix and the local name of its start tag
  is not looked up at all. Text and attribute values without a reference, a
  carriage return or, in attribute values, a tab or a newline are copied as
  they are instead of character by character; those with one are built in a
  string that the parser keeps from one to the next, where they grew a new
  string each, and the tree gets a copy of their length, as before. A text of
  100,000 elements with escaped markup in each takes 21.95 MiB of the memory
  of the allocator after parsing, as much as the same text without references
  does, and parsing it makes 17 reallocations instead of 300,017. The room for
  the attributes and the namespace declarations of an element is kept from one
  element to the next. Names, prefixes and namespaces get their ids in the
  same order as before, also when parsing fails. On a generated document of 25
  megabytes with 2.0M nodes, parsing makes 1,000,225 allocations instead of
  6,500,920 and 21 reallocations instead of 499,916, keeps the same heap, and
  takes 0.45 times as long (136 ms against 301 ms in the same runs);
  `Xot::parse_with_span_info` takes 0.66 times as long. An element with
  thousands of namespace declarations, each of which is compared with those
  before it, as before, takes 1.2 to 1.25 times as long to parse: 4.9 ms
  instead of 4.1 ms for 5,000 declarations, and 17.4 ms instead of 13.9 ms for
  10,000.

- `Xot::clone_node` and `Xot::clone_with_prefixes` make each copy as they go
  through the tree they copy, instead of collecting the start and the end of
  each of its nodes in a vector first, which for a tree of 2.0M nodes took
  48 MiB, and up to 72 MiB while it grew. A copy of a namespace, attribute,
  element, comment or processing instruction node goes at the end of the
  children of its new parent, without the checks of `Xot::append` and the
  searches of the attribute and namespace maps, which a node made for a copy
  cannot fail; a copied text joins a text before it, as before. Where a
  namespace or attribute node comes after a node of another kind, or has the
  prefix or name of one before it, as edits can leave them, it is copied
  through the maps as before. The copies are made in the same order and take
  the same slots as before. On a generated document of 25 megabytes with 2.0M
  nodes, a copy of the document takes 39 ms instead of 108 ms (20 ns a node
  instead of 55), with half the allocations, and the heap no longer goes above
  what the copy keeps, where it went 48 MiB above it;
  `Xot::clone_with_prefixes` of the elements under the document element takes
  79 ns a node instead of 120. Two copies of the document and their
  serialization peak at 446.9 MiB of resident memory instead of 538.6 MiB.

- When a text is joined with the text next to it, as when a text is appended
  after a text or put before or after one, or when the node between two texts
  is moved away, detached, removed or unwrapped, the text that stays keeps its
  string and the other text is added to it, instead of to a new string made
  from copies of both; a text added before it that does not fit gets a string
  of the joined length. A text that is appended is no longer copied first to
  see whether it is joined. Appending a text after a text, as
  `Xot::append_text` does, makes one allocation, for the string of the new
  node, instead of three and a reallocation, and takes 63 ns instead of 91;
  appending a new text to an element without one takes 34 ns instead of 49,
  and detaching the element between two texts 37 ns instead of 96.

- An insert into an attribute or namespace map, through
  `MutableNodeMap::insert`, `Xot::set_attribute`, `Xot::set_namespace`,
  `Xot::append_attribute_node`, `Xot::append_namespace_node` or
  `Xot::any_append`, looks through the nodes of the map once, for the key and
  for the place of a new node, where it looked through them twice. As it goes
  from one node to the next, it reads the slot after the slot of the node
  before, where nodes made one after another mostly are, without waiting to
  read where the next node is, which then only confirms it. The nodes end up
  where they did. Adding 14 namespaces one at a time to each of 20,000
  elements takes 300 ns an element instead of 840 ns, 56 namespaces 2.7 µs
  instead of 9.7 µs, and one namespace 16 ns instead of 35 ns; on a
  generated document of 25 megabytes with 2.0M nodes, adding an attribute to
  an element takes 24 ns instead of 44 ns, and 14 namespaces 23 ns each
  instead of 66 ns.

- `Xot::outputs`, `Xot::tokens`, `Xot::pretty_tokens` and the XML and
  HTML 5 serializers walk the tree with one iterator that goes through the
  edges of `Xot::traverse` and, for each element, through its namespace and
  attribute nodes as they stand among its children, where they made a
  generator for every edge and one more for each element. Items, order and
  the prefixes of the top element are the same. On a generated document of 25
  megabytes with 2.0M nodes, `Xot::outputs` goes through the 4.0M items in
  24 ms instead of 197 ms, with no allocation instead of 7,601,022;
  `Xot::serialize_xml_write` into a 64 KiB buffered writer takes 360 ms
  instead of 553 to 572 ms (0.63 to 0.65) with 7.1M allocations instead of
  14.7M, `Xot::tokens` 310 ms instead of 488 to 501 ms and HTML 5
  serialization 329 ms instead of 511 to 535 ms. The calls to the writer
  and the bytes are the same as before for every parameter and for writers
  that fail.

- The XML and HTML 5 serializers escape a text or an attribute value by
  looking at its bytes for the few that need a replacement, and copy the
  runs between them, where they went through the characters one by one and
  pushed each to a new string. A value with nothing to escape is used as it
  is, and the check that a value has only characters XML 1.0 allows looks at
  the same bytes. The escaped text, the character references and the errors
  are the same as before. On a generated document of 25 megabytes with 2.0M
  nodes (release, a 64 KiB buffered writer, load 5 to 8),
  `Xot::serialize_xml_write` takes 319 ms instead of 565 ms (0.56 to 0.57,
  where walking the tree alone gave 0.63 to 0.65) with 6.1M allocations and
  2.1M reallocations instead of 14.7M and 2.6M; with CDATA sections for
  every element with text it takes 345 ms instead of 612 ms, and HTML 5
  serialization 302 ms instead of 525 ms. `Xot::to_string` takes 149 ns for
  a node instead of 264 ns.

- The XML serializer renders each token into one string that it clears and
  uses again for the next token, writes prefixed names in pieces where it
  formatted them, and takes the indentation from a constant of spaces. A
  string that one large token made grow beyond 1,048,576 bytes is given
  back after that token. `Xot::tokens` and `Xot::pretty_tokens` render into such
  a string too and copy each token out of it. The writer gets the same calls
  with the same bytes as before, also when a token fails a check or the
  writer fails, and the errors are the same. On a generated document of 25
  megabytes with 2.0M nodes (release, interleaved, 18 rounds in each of 2
  runs, load 5 to 7), `Xot::serialize_xml_write` into a 64 KiB buffered
  writer takes 102 to 103 ms instead of 557 to 562 ms (0.18), with 10
  allocations and 1 reallocation instead of 14.7M and 2.6M; indented it
  takes 128 ms instead of 584 to 586 ms (0.22), and with CDATA sections for
  every element with text 109 ms instead of 606 to 614 ms (0.18).
  `Xot::to_string` takes 101 ms instead of 525 to 530 ms (0.19),
  `Xot::tokens` 144 ms instead of 491 to 495 ms (0.29) with one allocation
  for each token, and `Xot::pretty_tokens` 167 to 168 ms instead of 518 to
  519 ms (0.32). Into a file that is not buffered, the output of 2.5
  megabytes of a smaller document takes 358 ms instead of 417 ms (0.86),
  where the calls to the file, the same as before, take most of the time.

### Other changes

- `Node` is the position of the node and its generation, so its debug text
  and its hash change: `Node(NodeId { index1: 1, generation: 0 })`. The
  debug text of a `Xot` changes too, as it shows the arena of its nodes, and
  so does that of the attribute and namespace maps (`Attributes`,
  `Namespaces`, `MutableAttributes` and `MutableNamespaces`), which show
  their `Xot`.

- `impl From<indextree::NodeId> for Node` stays, for code that uses it: it
  gives the node at the position of the id, with the generation 0. As the
  arena of a `Xot` was never public, an indextree id never referred to one of
  its nodes. indextree stays a dependency for this and for
  `Error::NodeError`.

- A `Xot` holds at most 2^32 - 1 nodes; one more panics with "Too many nodes
  in the arena".

- A change through the `Node` of a removed node whose slot a later node took
  since, such as `append(parent, node)`, changes the later node, as before,
  but navigation now gives the `Node` of that later node, where it gave the
  `Node` of the removed one.

- A panic that takes a cycle made with `insert_after` or `insert_before`,
  then `element_unwrap` of a node in it, has another message.

- The debug text of a `Xot` shows the parsed trees it keeps and those it
  dropped.

- Each operation that changes the structure (`append` and the other
  appends, `prepend`, `insert_before`, `insert_after`, `detach`, `remove`
  and the operations made of them) compares the positions of the nodes it
  changes with the kept parsed trees after the change. With the kept tree
  of a generated document of 25 megabytes with 2.0M nodes, making an
  element and appending it to an element takes 23.4 ns instead of 23.2 ns,
  inserting it before or after a sibling 23.2 to 23.5 ns instead of 22.5
  ns, and adding a namespace node 21.5 ns instead of 20.8 ns. Adding the
  first attribute to an element takes 23.2 ns instead of 21.5 ns, as long
  as in a control copy without the comparison, so that this difference
  comes from where the code lands rather than from the comparison. With a
  small kept tree, 2,000,000 appends to 1,000 elements in turn take 16.5
  to 17.1 ns each instead of 16.3 to 16.7 ns. Removing 200,000 small
  parsed documents one after another takes 31 to 33 ns a document instead
  of 28 ns. Parsing takes as long as before; the first parse into a new
  `Xot` makes one more allocation.

- A change of a kept parsed tree that is not the first tree the `Xot`
  keeps also drops the trees parsed after it, so that the kept trees stay
  side by side in storage.

- `Xot::dropped_parsed_trees` grows by one entry of 8 bytes for each
  dropped tree.

## [0.31.2] - 2025-04-09

### Bugs fixed

- There was another bug in the new implementation of `following`: if there was
  a following node to a parent it would not be found and it should be.

## [0.31.1] - 2025-04-09

### Bugs fixed

- There was a bug in the new implementation of `following`; child nodes of the
  node in question were returned as well, which was in error.

## [0.31.0] - 2025-04-08

### Features added

- Added `all_following` that can traverse all following nodes in preorder.

### Other changes

- `following` uses a more optimized custom iterator now.

- Updated the `zerovec` and `futures-util` indirect dependencies to newer
  versions, as the versions in the lock file were yanked.

## [0.30.0] - 2025-04-08

### Features added

- Add `reverse_all_traverse`, which is like `reverse_traverse` but does not skip attribute and namespace nodes.

- Add `reverse_preorder` and `all_reverse_preorder`. Iterate back to the root
 in reverse preorder fashion, so that all nodes that were before this one
 in preorder are visited.

## [0.29.0] - 2025-03-11

### Features added

- Duplicate `xml:id` in a document is rejected by the parser with an error.

- A new `xot::xml_id_node` method that given a document node and a value
  returns the element node with that id in the document, if available.

## [0.28.1] - 2025-03-10

### Features added

- The parser now accepts documents with an XML declaration including `standalone=no`. These
  documents would previously raise an `UnsupportedNotStandalone` error. The
  `UnsupportedNotStandalone` error category is now deprecated.

- `ParseError` is now a `std::error::Error`.

### Optimizations

- Use size hint to try to make string value a bit faster.

## [0.28.0] - 2024-11-18

### Features added

- A new error named `ParseError`. This has a `span()` method which returns
  the span to which the parse error occurs. In some cases the span is
  an empty range, if only an error position can be known.

- Added `parse_fragment_with_span_info`. This gives span information for
  fragment parsing.

### Breaking changes

- Parsing related APIs now return a `ParseError` rather than a more general
  `Error`. It's possible to convert a `ParseError` into an `Error` though
  (using `.into()`, or implicitly using the `?` operator).

- `ParseError` has been pulled out of `Error`. If you were relying on catching
   particular errors you may have to adjust your code.

## [0.27.1] - 2024-11-01

### Bugs fixed

- Fixed a bug where `clone_node` didn't work with attribute and namespace
  nodes.
  
## [0.27.0] - 2024-10-31

### Features added

- Add `xot.parse_fragment` function to parse XML fragments. XML fragments are
  document nodes which can contain zero to many elements, and can contain text
  nodes. In order words they have the same regular content as elements do, but
  cannot have attributes or namespace nodes.

- Add a `xot.validate_well_formed_document` function to verify whether a
  document node is a well formed document.

### Bugs fixed

- The `xot.parse` function now rejects documents that do not have a document
  element, which it didn't do before. The empty string is now rejected as
  a well-formed XML document.

### Breaking changes

- Xot now supports the document node representing a document fragment rather
  than a well-formed document. A document node in a well formed document
  can only have a single top-level elements, and no text nodes, but fragments
  relax those restrictions.

  Previously Xot attempted to prevent non-well-formed documents to be created
  by manipulation. These checks have now been lifted. This means you won't get
  errors anymore if you manipulate XML to create a fragment. You can now remove
  the document element, add multiple elements to a document node, and add text,
  without errors. 
  
  If you were previously relying on this behavior, and you want to know whether
  a document node represents a  well-formed document, you can use
  new `xot.validate_well_formed_document` function against a document node.

  Previously used the word "fragment" in some of the documentation to refer to
  an unattached node (or tree of nodes) that is *not* rooted in a document
  node. We have retired that usage and now refer to these as unattached nodes
  or unattached trees instead.

## [0.26.0] - 2024-10-28

### Features added

- `xml:id` processing according to [xml:id
   spec](https://www.w3.org/TR/xml-id/#id-avn). Any `xml:id` attribute is
   normalized, removing both leading and traigling spaces and collapsing
   multiple space characters into one.

- New `xml:xml_id_name()` method to access the name of an ID.

### Bugs fixed

- `xmlns=""` can be used to undeclare the default namespace, meaning it's not
  really there, but this wasn't properly handled by `xot.namespaces_in_scope()`
  and `xot.prefixes_in_scope()`, and `xot.prefix_for_namespace()`.

` `xot.prefix_for_namespace()` and `xot.namespace_for_prefix()` weren't
  properly handling namespace overrides or the default namespace.

## [0.25.0] - 2024-09-12

### Features added

- Previously the `Xot` struct was not cloneable, and now we support `Clone` on
  it. Note that the previous `.clone` method now changes -- see below.

- `NodeEdge` has convenience methods `node()`, `previous()` and `next()` now.

- Update various dependencies to newer versions.

### Breaking changes

- Previously Xot exposed a `clone` method to clone a node (and its children).
  This has now been broken as it conflicts with the `Clone` trait. Modify your
  code to use `clone_node` to get the clone node behavior.

## [0.24.0] - 2024-04-19

### Breaking changes

- Previously a `>` character in text was serialized as itself not as `&gt;`, as
  this is allowed by the spec (except `]]>`, see the bug fix below). Since this
  can be rather surprising, the default is now to always serialize `>` as
  `&gt;`.

- `xot.tokens` and `xot.pretty_tokens` now take a
  `xot::output::TokenSerializeParameters` object that can be used to control
  token generation, as opposed to manually passing in `cdata_section_elements`.

### Features added

- Added `xot.get_element_name`, `xot.set_element_name` convenience APIs.

- Added `xot.get_attribute`,`xot.set_attribute`,
  `xot.delete_attribute`, and `xot.get_namespace`, `xot.set_namespace`,
  `xot.delete_namespace` convenience APIs.

- Added convenience `to_hashmap` on attributes and namespaces accessors.

### Bugs fixed

- `]]>` if seen outside of a CDATA section delimiter should always be rendered
  as `]]&gt;`, and it wasn't before. Now it is even if unescaped gts are
  configured.

- Fixed a bug with `xot.prepend`; it didn't work correctly in the presence of
  namespace and attribute nodes.

- `xot.clone_with_prefixes` crashed if you tried to clone a non-element node.

- `is_empty` for `xot.attributes()` and `xot.namespaces()` gave the wrong output.

- `unwrap_element` had a bug where the element contained namespaces or
  attributes.

- Parsing a CDATA did not produce span info.

- Parsing a CDATA mixed with non-CDATA text in an element content would produce
  multiple text nodes in a row. Now we produce a single consolidated text node.

- Parsing a CDATA node mixed with non-CDATA text results in a span that covers
  the non-CDATA text and the CDATA text both.

## [0.23.0] - 2024-03-22

### Breaking changes

- Created a new API with much more control of serialization:
  `Xot::serialize_xml_string` and `Xot::serialize_xml_write`, which are like
  `Xot::to_string` and `Xot::write`, but allow fine-grained control of
  serialization options, including pretty printing.

- Removed `Xot::with_serialize_options`, `SerializeOptions`, `WithSerializeOptions` in  
  favor of the new APIs.

- The low-level `Xot::tokens` API has additional parameters, a list of element
  names that should be serialized as cdata sections. Putting in an empty vec is
  fine if you don't want any. You can also pass in a normalizer (if you don't
  care, use the `xot::output::NoopNormalizer`).

- The low-level `Xot::pretty_tokens` API has additional parameters: a list of
  elements that should be included from indentation and a list of that should
  be serialized as cdata sections. You can also pass in a normalizer (if you don't
  care, use the `xot::output::NoopNormalizer`).

- `xot::Output`, `xot::OutputToken`, `xot::PrettyOutputToken` are now
  importable from `xot::output` instead of from `xot` directly.

## Changes

### Rust 1.77 compatibility

There was a problem compiling this with Rust 1.77 which has since been fixed.

### New `output::xml::Parameters` API to control XML serialization

- You can cause an XML declaration to be output.

- You can cause a doctype declaration to be output.

- You can suppress specific element names from being pretty-printed.

- You can cause specific element names content to be presented as CDATA
  sections.

- You can cause text content to be unicode normalized. This requires the new
  feature `icu` feature to be enabled. See
  ``Xot::serialize_xml_string_with_normalizer` for more information.

### New API (`Xot::html5`) and `output::html5::Parameters` API to control HTML 5 serialization

- You can output XML as HTML 5 now.

- Indentation rules know about HTML inline elements.

## [0.22.0] - 2024-02-20

### Breaking changes

- `MissingPrefix` now contains the namespace for which the prefix cannot be
  found as a string, not as a `NamespaceId`.

### Changes

- Introduce `xmlname` module which supports advanced use cases surrounding the
  creation and access to XML names. It also offers more convenient creation and
  access of names as opposed to the gaggle of methods that exist on `Xot`.

- The various APIs that take name ids have been modified to accept some
  `xmlname` names (`xmlname::Create`, `xmlname::Ref`) as well. The
  documentation for `new_element` has an example.

- Added `Xot::add_namespace` to create a namespace node based on an
  `xmlname::Namespace`.

- Added `Xot::name_ref` and `Xot::node_name_ref` to get `xmlname::Ref` access to
  a node.

### Changes

Move `id` related functionality into `id` submodule.

## [0.21.0] - 2024-02-15

### Breaking changes

Warning: this release includes many major API breaking changes!

#### Attribute and namespace API changes

The way attributes and namespace prefixes are handled has changed entirely in
this release. Instead of maintaining this information on the `Element` value
(as a vecmap), the information is now entirely handled in-tree. This means that
attributes and namespace prefixes are now addressable as nodes, with as their
parent the element node.

This means some significant breaking API changes:

- The `Element` value does not maintain attributes and namespace prefixes
  anymore. The only thing it has is a `name()` accessor, and a way to
  set the name with `set_name()`.

- The `Value` enum gains two new entries, `Attribute` and `Namespace`.
  Similarly, `ValueType` gains these as well.

- To access attributes, use `xot.attributes(node)`. This returns a hashmap-like
  struct (`NodeMap`). with as key the name id of the attribute, and as value as
  string, that lets you access attributes using `get`, iterate over them, etc.
  `NodeMap`, like `VecMap`, maintains order information and access time is
  linear, not constant time like a real hashmap.

- To access namespaces, use `xot.namespaces(node)`. This returns a hashmap-like
  struct with as key the prefix id, and as value the namespace id. The old
  `xot.namespaces` has been renamed to `xot.inherited_prefixes`. You can
  also access this information as a `Prefixes` map, using `xot.prefixes(node)`.

- `xot::Prefixes` is now a real std hashmap, not a vecmap or the new nodemap,
  and thus does not retain order information. It is returned from specific
  APIs such as `xot.prefixes(node)` and `xot.inherited_prefixes()`.

- You can also update and add attributes and namespaces using `xot.
attributes_mut()` and `xot.namespaces_mut()`, respectively. Since these hold
  a mutable reference to the tree, lifetime issues makes them not so convenient
  for accessing the information.

- While `namespaces` and `attributes` return hashmap-like APIs, this API is at
  present not as complete as attribute and prefixes information previously
  returned as provided by `vecmap-rs`.

- Comparing two elements used to be possible through
  `element.compare_ignore_attributes`. This has now been replaced by
  `xot.shallow_equal_ignore_attributes`, which can also compare non-element
  nodes.

#### Attribute and namespace nodes

The previously discussed `xot.attributes[_mut]` and `xot.namespaces[_mut]` are the most convenient APIs for most use cases.

But in some advanced use cases you want to be able to directly access and manipulate attribute and namespace nodes.

This is done with a new set of APIs. These all have the postfix `_node` in them
to remind you that you are doing explicit node manipulation:

- To create a new namespace node: `xot.new_namespace_node()`.

- To create a new attribute node: `xot.new_attribute_node()`.

- To append a namespace node: `xot.append_namespace_node()`.

- To append an attribute node: `xot.append_attribute_node()`.

- To append any node, including namespace and attribute nodes, use
  `xot.any_append()`

- To check whether something is a namespace node: `xot.is_namespace_node()`.

- To check whether something is an attribute node: `xot.is_attribute_node()`.

- To access or manipulate namespace information on a node:
  `xot.namespace_node` and `xot.namespace_node_mut`.

- To access or manipulate attribute information on a node:
  `xot.attribute_node` and `xot.attribute_node_mut`.

- To access an individual namespace node you can also use `get_node` on
  `Attributes` and `Namespaces`, so for instance
  `xot.namespaces(node).get_node(name)`.

- To access all attributes nodes as an iterator, use `xot.attribute_nodes`.

#### Serializer `Output` enum

The serializer `Output` enum has been simplified:

- Any reference to `Element` has become a copy of `Element`, as element is
  basically just a name and now copy.

- The `Output::Prefix`, `Output::Attribute` `Output::StartTagClose` entries
  don't include the element anymore.

- The enum entries `Output::PrefixesFinished` and `Output::AttributesFinished`
  have been removed.

#### Rename compare APIs to equal

Renamed `compare` to `deep_equal`, `advanced_compare` to
`advanced_deep_equal`,`compare_children` to `deep_equal_children`.

Introduced `xot.shallow_compare` and
`xot.shallow_equal_elements_ignore_attributes` to replace removed
`compare_ignore_attributes` on `Element`.

#### Root renamed to Document

To be more in line with XML naming conventions, renamed the `Root` value to the
`Document` value. So:

- `Value::Root` -> `Value::Document`

- `ValueType:Root` -> `ValueType::Document`

- `Error::NotRoot` -> `Error::NotDocument`

- `xot.new_root_unconnected` -> `xot.new_document`

- `xot.new_root` -> `xot.new_document_with_element`

- `xot.is_root` -> `xot.is_document`

- `Fixed::Root` -> `Fixed::Document`

- `Fixed::RootContent` -> `Fixed::DocumentContent`

- `xot.is_under_root` -> `xot.has_document_parent`

#### Processing instruction target

Processing instruction `target` is now a `xot::NameId`, not a string.

### Other changes

#### New APIs

- Introduced `xot.full_name`, `xot.local_name_str` and `xot.uri_str` to get
  various aspects of a node name.

- Also added `xot.node_name` function to retrieve name id of element, attribute
  or processing instruction target, according to XPath rules.

- Added `xot.string_value` accessor, which gives the string value of a node
  according to XPath rules.

- New `xot.axis` method which lets you do traversal according to axis,
  following XPath.

- Added `xot.deep_equal_xpath` which compares nodes as defined by XPath.

- Added `xot.all_descendants` iterator which includes name and attribute nodes.

- Added `xot.all_traverse` iterator which includes name and attribute nodes.

#### Dependencies

- Eliminated `vecmap-rs` dependency.

- Internally, `next-gen` has been replaced with `genawaiter` to support
  generators. Should not be visible to the outside, but changes the
  dependencies.

## [0.20.0] - 2024-02-08

### Changes

- Added `child_index` method to get the index of a child in a parent.

## [0.19.0] - 2024-01-15

### Changes

- Add `Debug` support for `Xot` struct.

- Add `xot.new_root_unconnected` to create a root node without any document
  element attached.

- Now always expose the `xot::fixed` API, even if the proptest feature is
  disabled. It turns out to be handy to be able to create fixed content and
  then transform it into Xot data for other use cases as well. The details of
  the `fixed` API have also been reworked.

## [0.18.0] - 2024-01-08

### Changes

- Changed representation of attributes and prefixes from
  `std::collections::BTreeMap` to use `VecMap` from `vecmap-rs`. This has the
  benefit of retaining the original definition order. Note that `vecmap-rs` is
  a different implementation than `vector-map`, which Xot used until version
  0.14. `vecmap-rs` offers an ordering guarantee, while `vector-map` doesn't.

  This could cause some breaking changes if there are differences in the map
  API, or if you used the `BTreeMap` type directly instead of Xot's
  `Attributes` and `Prefixes` type.

## [0.17.0] - 2023-11-13

### Changes

- Add `xot.parse_with_spans()` which besides parsing the tree also returns a
  `xot::SpanInfo` which maintains span information. The span information
  becomes invalid as soon as you mutate a Xot document.

- Add `xot.outputs` to get serialization output tokens out without rendering
  them. This can be useful if you want to process these tokens with an external
  parser.

## [0.16.0] - 2023-09-07

### Changes

- Add `xot.following()` function. This returns all nodes that follow a node in
  document order, not including the node itself or any of its ancestors or
  descendants.

- Add `xot.preceding()` function. This returns all nodes that precede a node in
  document order, not including the node itself or any of its ancestors.

## [0.15.0] - 2023-09-06

### Changes

- Add `xot.namespace_for_prefix()` function to look up namespace in the context
  of a node.

- Add `xot.namespaces()` function that returns an iterator that iterates over
  non-overridden `(prefix, namespace)`` pairs in the scope of a node.

- Add `xot.advanced_compare()` function that lets you compare two nodes with a
  node filter and with a custom text/attribute value comparison.

## [0.14.0] - 2023-08-24

### Changes

- Support for XML documents with other encodings than just UTF-8 and US-ASCII.

- Do not use `VecMap` anymore to store attributes and namespace prefixes.
  Instead store attributes in a `std::collections::BTreeMap`, which should
  behave similarily for small amounts of attributes.

  This is a potentially breaking change when accessing `prefixes` or
  `attributes` on a XML value, but the map API compatibility should in fact be
  improved.

  This can also change the order in which attributes or namespace declarations
  are serialized, which is not a semantic change of XML but may break certain
  tests.

## [0.13.5] - 2023-06-29

### Fixed

- There was a stray debugging print that caused output when parsing CDATA.
  This has been removed.

## [0.13.4] - 2023-06-29

### Fixed

- `deduplicate_namespaces` had a bug. Attributes, unlike elements, can only be
  in a namespace if they have an explicit prefix; there is no defaulting. This
  means that a prefix definition for a duplicate namespace can only safely be
  removed if this prefix doesn't overlap with a default namespace, or if the
  prefix is not in use by any attribute. We now check for this.

## [0.13.3] - 2023-06-26

### Fixed

- We were incorrectly doing entity parsing inside CDATA sections. This caused
  `&` to be rejected as incorrect in such a section, but it should be
  acccepted. Now we accept the text in these sections as-is instead and don't
  try to do any processing on them. Thanks to dsrich for the report! Fixes
  https://github.com/faassen/xot/issues/3

## [0.13.2] - 2023-05-02

### Added

- Make Xot a bit more generous about what encodings it supports: lowercase `utf-8` is
  accepted, as well as `us-ascii`. Note that `us-ascii` at present is handled just
  like `UTF-8`, and no invalid ASCII is detected by the parser.

## [0.13.1] - 2023-03-23

### Added

- New `xot.level_order()` method to traverse the tree in level order.

## [0.13.0] - 2023-02-23

### Added

- New `xot.xml_space_name()` to access the `NameId` of the
  `xml:space` attribute.

### Changed

- `remove_insignificant_whitespace` now respects `xml:space="preserve"`.

- Pretty printing now also respects `xml:space="preserve"`.

- Expose `inherited_prefixes` method which gets the prefixes that a node
  inherits but does not define locally.

## [0.12.1] - 2023-02-15

### Changed

- Proptest won't create carriage return as text content anymore, as
  that is supposed to be normalized by XML anyway.

## [0.12.0] - 2023-02-08

### Added

- Expose `no_namespace`, `empty_prefix`, `xml_namepace` and `xml_prefix`
  methods.

### Changed

- Xot doesn't take a lifetime specifier anymore.

- The lifetime restriction on `Xot.add_name` and `Xot.add_name_ns` has been
  lifted; you can now pass in more short-lived names. Internally `Cow` was
  previously in use to store names, but that was a minor optimization far more
  trouble than it was worth.

## [0.11.8] - 2023-02-07

### Added

- Added `remove_insigificant_whitespace` to the `Xot` API. This removes
  insignificant whitespace nodes from the tree.

## [0.11.7] - 2023-02-01

### Added

- Added `is_prefix_defined` to the `Xot` API to check whether a prefix is
  defined in the context of a node.

- Added `prefix_for_namespace` to the `Xot` API to get a defined prefix for
  that namespace in the context of a node, or `None` if it isn't defined.

## [0.11.6] - 2023-02-01

### Added

- Added `unresolved_namespaces` to the `Xot` API. This gives namespaces that
  aren't defined by prefixes within the given node or its descendants.

- Changed the behavior of `clone_with_prefixes` to be more accurate: it only
  includes prefixes for unresolved namespaces, not any additional ones.

## [0.11.5] - 2023-01-26

### Added

- Added `compare` and `compare_ignore_attributes to the `Element` API. This
  allows you to compare two elements semantically, with the option to ignore
  some attributes during the comparison.

- Added `xot.compare_children`. This allows you to compare the children of
  two element nodes, even though they may have different names or attributes.

## [0.11.4] - 2023-01-17

### Fixed

- Fixed another bug with text node consolidation; `xot.clone` is now robust
  against text node consolidation.

## [0.11.3] - 2023-01-17

### Fixed

- Fixed a bug with text node consolidation. In some cases when using insert
  before or insert after, a node was not properly consolidated.

## [0.11.2] - 2023-01-16

### Added

- Make `xot::Value` hashable. This means `Element`, `Text`, `Comment`, etc are
  now hashable too.

## [0.11.1] - 2023-01-13

### Added

- Added the ability to turn off text node consolidation. Use
  `Xot::set_text_consolidation(false)` to turn it off or on again.

## [0.11.0] - 2023-01-13

### Added

- Add pretty printing support. You can use `Xot::with_serialize_options()` to
  turn on pretty printing before serialization.

- Added a `Xot::tokens` and `Xot::pretty_tokens` API to extract raw serialized
  XML for custom rendering. This replaces the `SerializerWriter` API.

### Breaking changes

- Revised the serialization API yet again as it was getting too complicated:

  - To serialize any node (including the root) to a `Xot::to_string`. This
    replaces `Xot::serialize_to_string` and `Xot::serialize_node_to_string`.

  - Serialization never takes a mutable self anymore.

  - To write to a `Write`, use `Xot::write`. This replaces `Xot::serialize` and
    `Xot::serialize_node`.

  - Previously serialization to string did not return an error. This has been
    changed so that an error is returned as well.

  - Serialization now errors if a namespace URI is encountered with a missing
    prefix. This can be fixed by calling `create_missing_prefixes` just before
    serialization, but you need to do this manually; it won't be automatic
    anymore.

- Removed the `SerializerWriter` trait introduced by `0.10.0`, and
  `serialize_with_writer`. The required functionality (and more) is now exposed
  through `Xot::tokens` and `Xot::pretty_tokens`.

## [0.10.3] - 2023-01-09

### Changed

- We still haven't gotten the serialization right. The `SerializerWriter` was
  taking fullname parameters, but that breaks expectations. Instead let the
  `SerializerWriter` own the fullname serializer so it can construct names as
  needed.

## [0.10.2] - 2023-01-09

### Changed

- `serialize_with_writer` now only takes an immutable self, and it's the only
  way to serialize with a writer. It does not attempt to add missing prefixes
  anymore.

## [0.10.1] - 2023-01-09

### Added

- `namespace_for_name` to allow access of the namespace id of a particular
  name.

## [0.10.0] - 2023-01-09

### Added

- New `serialize_with_writer` and `serialize_node_with_writer` methods. These
  can be used to customize the XML write process using the new
  `SerializerWriter` trait. The `StringWriter` struct is also provided and is
  useful to implement this trait.

### Changed

- `serialize` and `serialize_node` now do return an error object, as IO may
  go wrong during the underlying write process and we don't want to hide this.

## [0.9.2] - 2023-01-05

### Fixed

- Now actually working `xot::proptest::arb_xml_root_with_config`.

## [0.9.1] - 2023-01-05

### Fixed

- Fixed incomprehensible text in `element_unwrap` error.

### Added

- Added `xot::proptest::arb_xml_root_with_config` which allows configuration of
  what is generated. Right now you can only turn on or off generation of
  comments and processing instructions outside of the document element.

## [0.9.0] - 2023-01-05

### Added

- `proptest` support. If you enable the `proptest` feature you can use
  `xot::proptest::arb_xml_root` in your own proptests. This generates a
  arbitrary XML document out of a fixed set of elements, attributes, namespace
  prefixes and namespaces.

- Elements in the Xot tree are now more lightweight now as they don't
  carry an inverse map of namespace to prefix anymore.

### Fixed

- Rework the way serialization works for namespaces, fixing various bugs
  surrounding serialization of missing prefixes.

### Removed

- Remove the `get_prefix` method on `Element`. A namespace can have multiple
  prefixes, and the API isn't really very useful.

## [0.8.0] - 2023-01-02

## Added

- `text_content_mut` and `text_content_str` now work with elements without
  any content too.

## Fixed

- Previously we couldn't wrap standalone elements with `element_wrap`, now we
  can.

## [0.7.1] - 2022-12-22

### Fixed

- Fixed a bug in `compare`: if values were the same but the structure was
  different, compare incorrectly returned `true`.

## [0.7.0] - 2022-12-21

### Added

- Lots of examples in the API docs.

### Breaking

- `new_root` has changed and requires you to pass in an element. This is
  because otherwise you could inadvertently create XML documents that are
  not allowed - a document element is required.

## [0.6.1] - 2022-12-20

### Added

- It's now possible to call `element_wrap` on the document element as well.

## [0.6.0] - 2022-12-20

### Added

- It's not possible to apply `element_unwrap` to the document element,
  if and only if it has a single child node that's an element.

- It's allowed to use `replace` on the document element as well, if it
  doesn't break the rule and the document element remains.

- Added new `is_document_element` accessor to check whether a node
  is the document element.

## [0.5.1] - 2022-12-16

### Fixed

- Fixed a bug where we couldn't clone a document root.

## [0.5.0] - 2022-12-16

### Added

- Made built in escaping rules less aggressive for serialization; text values
  now only escape `<` and `&`, and attributes escape those and `'` and '"`.

- `xot.new_root()` which allows you to create a new document from scratch
  without parsing.

- `xot.text_content`, `xot.text_content_mut` and `xot.text_content_str` for
  convenience access to the text content of an element.

- `xot.compare` which allows you to compare two nodes for semantic equality.

### Performance

- More efficient handling of lookup of name, prefix ids and namespace ids.

## [0.4.0] - 2022-12-14

### Added

- Export `ToNamespace` and `Attribute` types.

- Add documentation for `NodeEdge` variants.

## [0.3.2] - 2022-12-13

### Fixed

- `serialize_node_to_string` generated duplicate namespace declarations.

## [0.3.1] - 2022-12-13

- Make `NameId`, `PrefixId` and `NamespaceId` have `Ord`.

## [0.3.0] - 2022-12-13

- Add new `name_ns_str`, `namespace_str` and `prefix_str` methods to access
  name information.

- Changed the processing instruction API to not use `get_target` and
  `get_value` but instead to use `target()` and `value()`.

- Make `NameId`, `NamespaceId` and `PrefixId` public.

- Documentation tweaks.

- Tweak `Cargo.toml`

## [0.2.0] - 2022-12-12

Initial public release.

<!-- next-url -->
[Unreleased]: https://github.com/faassen/xot/compare/v0.31.2...HEAD
[0.31.2]: https://github.com/faassen/xot/compare/v0.31.1...v0.31.2
[0.31.1]: https://github.com/faassen/xot/compare/v0.31.0...v0.31.1
[0.31.0]: https://github.com/faassen/xot/compare/v0.30.0...v0.31.0
[0.30.0]: https://github.com/faassen/xot/compare/v0.29.0...v0.30.0
[0.29.0]: https://github.com/faassen/xot/compare/v0.28.1...v0.29.0
[0.28.1]: https://github.com/faassen/xot/compare/v0.28.0...v0.28.1
[0.28.0]: https://github.com/faassen/xot/compare/v0.27.1...v0.28.0
[0.27.1]: https://github.com/faassen/xot/compare/v0.27.0...v0.27.1
[0.27.0]: https://github.com/faassen/xot/compare/v0.26.0...v0.27.0
[0.26.0]: https://github.com/faassen/xot/compare/v0.25.0...v0.26.0

[0.25.0]: https://github.com/faassen/xot/compare/v0.24.0...v0.25.0
[0.24.0]: https://github.com/faassen/xot/compare/v0.23.0...v0.24.0
[0.23.0]: https://github.com/faassen/xot/compare/v0.22.0...v0.23.0
[0.22.0]: https://github.com/faassen/xot/compare/v0.21.0...v0.22.0
[0.21.0]: https://github.com/faassen/xot/compare/v0.20.0...v0.21.0
[0.20.0]: https://github.com/faassen/xot/compare/v0.19.0...v0.20.0
[0.19.0]: https://github.com/faassen/xot/compare/v0.18.0...v0.19.0
[0.18.0]: https://github.com/faassen/xot/compare/v0.17.0...v0.18.0
[0.17.0]: https://github.com/faassen/xot/compare/v0.16.0...v0.17.0
[0.16.0]: https://github.com/faassen/xot/compare/v0.15.0...v0.16.0
[0.15.0]: https://github.com/faassen/xot/compare/v0.14.0...v0.15.0
[0.14.0]: https://github.com/faassen/xot/compare/v0.13.5...v0.14.0
[0.13.5]: https://github.com/faassen/xot/compare/v0.13.4...v0.13.5
[0.13.4]: https://github.com/faassen/xot/compare/v0.13.3...v0.13.4
[0.13.3]: https://github.com/faassen/xot/compare/v0.13.2...v0.13.3
[0.13.2]: https://github.com/faassen/xot/compare/v0.13.1...v0.13.2
[unreleased]: https://github.com/faassen/xot/compare/v0.13.0...v0.13.1
[unreleased]: https://github.com/faassen/xot/compare/v0.12.1...v0.13.0
[unreleased]: https://github.com/faassen/xot/compare/v0.12.0...v0.12.1
[unreleased]: https://github.com/faassen/xot/compare/v0.11.8...v0.12.0
[unreleased]: https://github.com/faassen/xot/compare/v0.11.7...v0.11.8
[unreleased]: https://github.com/faassen/xot/compare/v0.11.6...v0.11.7
[unreleased]: https://github.com/faassen/xot/compare/v0.11.5...v0.11.6
[unreleased]: https://github.com/faassen/xot/compare/v0.11.4...v0.11.5
[unreleased]: https://github.com/faassen/xot/compare/v0.11.3...v0.11.4
[unreleased]: https://github.com/faassen/xot/compare/v0.11.2...v0.11.3
[unreleased]: https://github.com/faassen/xot/compare/v0.11.1...v0.11.2
[unreleased]: https://github.com/faassen/xot/compare/v0.11.0...v0.11.1
[unreleased]: https://github.com/faassen/xot/compare/v0.10.3...v0.11.0
[unreleased]: https://github.com/faassen/xot/compare/v0.10.2...v0.10.3
[unreleased]: https://github.com/faassen/xot/compare/v0.10.1...v0.10.2
[unreleased]: https://github.com/faassen/xot/compare/v0.10.0...v0.10.1
[unreleased]: https://github.com/faassen/xot/compare/v0.9.2...v0.10.0
[unreleased]: https://github.com/faassen/xot/compare/v0.9.1...v0.9.2
[unreleased]: https://github.com/faassen/xot/compare/v0.9.0...v0.9.1
[unreleased]: https://github.com/faassen/xot/compare/v0.8.0...v0.9.0
[unreleased]: https://github.com/faassen/xot/compare/v0.7.1...v0.8.0
[unreleased]: https://github.com/faassen/xot/compare/v0.7.0...v0.7.1
[unreleased]: https://github.com/faassen/xot/compare/v0.6.1...v0.7.0
[unreleased]: https://github.com/faassen/xot/compare/v0.6.0...v0.6.1
[unreleased]: https://github.com/faassen/xot/compare/v0.5.0...v0.6.0
[unreleased]: https://github.com/faassen/xot/compare/v0.4.0...v0.5.0
