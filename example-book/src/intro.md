# Introduction

This book demonstrates the `mdbook-treesitter` preprocessor, which lets you
embed live code snippets extracted directly from source files using
[tree-sitter](https://tree-sitter.github.io) queries.

## How it works

Add a directive to any chapter:

\{{ #treesitter ../examples/geometry.rs }}

\{{ #treesitter ../examples/geometry.rs#doc_comment?name=Point }}

\{{ #treesitter ../examples/geometry.rs#struct?name=Rectangle }}

The preprocessor reads the referenced file, runs the named query (if any),
and replaces the directive with the extracted text.  Wrap the directive in a
fenced code block to get syntax-highlighted output:

````markdown
```rust
\{{ #treesitter ../examples/geometry.rs#doc_comment?name=Point }}
```
````
