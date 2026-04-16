# Geometry API

## Point

Doc comment extracted with the `doc_comment` tree-sitter query:

```rust
{{ #treesitter ../examples/geometry.rs#doc_comment?name=Point }}
```

Plain prose (comment delimiters stripped) via the `comment_text` query:

{{ #treesitter ../examples/geometry.rs#comment_text?name=Point }}

## Rectangle

Full struct declaration via the `struct` query:

```rust
{{ #treesitter ../examples/geometry.rs#struct?name=Rectangle }}
```

Fields only (no `pub struct … {…}` scaffolding) via the `struct_fields` query:

```rust
{{ #treesitter ../examples/geometry.rs#struct_fields?name=Rectangle }}
```

## Config

Doc comment retrieved with the `doc_comment_jq` jq-based query:

```rust
{{ #treesitter ../examples/geometry.rs#doc_comment_jq?name=Config }}
```

## Whole file

```rust
{{ #treesitter ../examples/geometry.rs }}
```
