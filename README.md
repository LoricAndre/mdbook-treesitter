# mdbook-treesitter

[![Crates.io](https://img.shields.io/crates/v/mdbook-ts)](https://crates.io/crates/mdbook-ts)
[![docs.rs](https://img.shields.io/docsrs/mdbook-treesitter)](https://docs.rs/mdbook-treesitter)
[![CI](https://github.com/LoricAndre/mdbook-treesitter/actions/workflows/ci.yml/badge.svg)](https://github.com/LoricAndre/mdbook-treesitter/actions/workflows/ci.yml)
[![License](https://img.shields.io/crates/l/mdbook-ts)](LICENSE)

An [mdBook](https://rust-lang.github.io/mdBook/) preprocessor that uses
[tree-sitter](https://tree-sitter.github.io) to extract code snippets directly
from source files and embed them in your book.

Directives in your Markdown are replaced with the text extracted by the named
query — wrap them in a fenced code block to get syntax highlighting:

````markdown
```rust
{{ #treesitter src/lib.rs#doc_comment?name=MyStruct }}
```
````

## Installation

```sh
cargo install mdbook-ts
```

Then add the preprocessor to your `book.toml`:

```toml
[preprocessor.treesitter]
```

### Development

To iterate on queries without reinstalling, point `command` at `cargo run`:

```toml
[preprocessor.treesitter]
command = "cargo run --manifest-path /path/to/mdbook-treesitter/Cargo.toml --"
```

## Language support

Rust, TOML, and Markdown parsers are bundled out of the box.

Additional parsers can be loaded from a shared library:

```toml
[preprocessor.treesitter.python]
parser = "/path/to/tree-sitter-python.so"  # absolute or relative to book.toml
```

## Defining queries

Queries live entirely in `book.toml` — no recompile needed when you change them.

### Tree-sitter queries

A plain string is treated as a tree-sitter S-expression query. Captures whose
names match directive parameters are used as filters; the remaining captures are
the output.

```toml
[preprocessor.treesitter.rust.queries]
# Captures doc-comment lines immediately before a struct (0 or 1 #[derive(…)]).
doc_comment = """
[
  ((line_comment)+ @doc_comment
   .
   (struct_item name: (type_identifier) @name))
  ((line_comment)+ @doc_comment
   .
   (attribute_item)
   .
   (struct_item name: (type_identifier) @name))
]"""

# Full struct declaration.
struct = "(struct_item name: (type_identifier) @name) @struct"

# Individual field declarations — no surrounding `pub struct Name { }`.
struct_fields = """
(struct_item
  name: (type_identifier) @name
  body: (field_declaration_list
    (field_declaration) @field))
"""
```

### Strip regex

Add `strip` to remove a regex pattern from every output line — useful for
stripping comment delimiters to get plain prose:

```toml
[preprocessor.treesitter.rust.queries.comment_text]
query = """
[
  ((line_comment)+ @doc_comment
   .
   (struct_item name: (type_identifier) @name))
  ((line_comment)+ @doc_comment
   .
   (attribute_item)
   .
   (struct_item name: (type_identifier) @name))
]"""
strip = "^///? ?"
```

### jq queries

For complex extractions, write a jq filter applied to the tree-sitter AST
serialised as JSON. The filter receives:

```json
{ "params": { "name": "Foo", … }, "source": "…", "children": […] }
```

```toml
[preprocessor.treesitter.rust.queries.doc_comment_jq]
format = "jq"
query = """
.params.name as $target_name |
.children as $all |
([$all | to_entries[] |
  select(
    .value.type == "struct_item" and
    (.value.children[]? | select(.type == "type_identifier") | .text) == $target_name
  )
] | .[0].key) as $idx |
if $idx == null then error("struct not found")
else
  ([$all[0:$idx] | to_entries[] |
    select(.value.type != "line_comment" and .value.type != "attribute_item")
  ] | if length > 0 then last.key else -1 end) as $last_gap |
  [$all[($last_gap+1):$idx][] |
    select(.type == "line_comment") | .text | rtrimstr("\\n")] |
  join("\\n")
end
"""
```

## Directive syntax

```
{{ #treesitter <path>[#<query>][?<param>=<value>[&…]] }}
```

| Part | Description |
|------|-------------|
| `<path>` | Path to the source file, relative to the chapter's directory |
| `#<query>` | Named query from `book.toml`. Omit to embed the whole file. |
| `?<param>=<value>` | Parameters forwarded to the query (e.g. `?name=MyStruct`) |

Prefix the directive with `\` to emit it literally without expansion:

```markdown
\{{ #treesitter src/lib.rs#doc_comment?name=Foo }}
```
