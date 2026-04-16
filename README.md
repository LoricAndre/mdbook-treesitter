# mdBook treesitter

This is an mdBook preprocessor that uses tree-sitter to extract code from source files.

## Usage

Install `mdbook-treesitter` then add this to your `book.toml`:

```toml
[preprocessor.treesitter]
```

## Language support

Out of the box, this supports Rust, TOML & Markdown.

You can add more parsers by configuring the preprocessor in `book.toml`:

```toml
[preprocessor.treesitter.python]
parser = "/path/to/parser.so"  # absolute or relative to book.toml
```

## Adding queries

Queries are defined in `book.toml` — no recompile needed when you change them.

### Tree-sitter queries

```toml
[preprocessor.treesitter.rust.queries]
# Captures the last run of doc-comment lines immediately before a struct.
# Handles 0 or 1 intervening attribute items (e.g. #[derive(...)]).
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

# Captures the full struct declaration (name + body).
struct = "(struct_item name: (type_identifier) @name) @struct"
```

### jq queries

For more complex extractions, use a jq filter applied to the tree-sitter AST
serialised as JSON. The filter receives:

```json
{ "params": { "name": "Foo", ... }, "source": "...", "children": [...] }
```

```toml
[preprocessor.treesitter.rust.queries.doc_comment_jq]
format = "jq"
# Extracts the last contiguous block of doc-comment lines before the named
# struct, skipping any attribute items between the comments and the struct.
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

## Referencing code

Add a preprocessor directive to your markdown:

```markdown
## Foo

{{ #treesitter ../../foo.rs#doc_comment?name=Foo }}
```

Note: the space around the braces is optional.

### Directive syntax

```
{{ #treesitter <path>[#<query>][?<param>=<value>[&...]] }}
```

| Part | Description |
|------|-------------|
| `<path>` | Path to the source file, relative to the chapter's source directory |
| `#<query>` | Named query defined in `book.toml`. Omit to embed the whole file. |
| `?<param>=<value>` | Query parameters passed to the query (e.g. `?name=MyStruct`) |
