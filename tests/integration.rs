//! Integration tests for mdbook-treesitter.
//!
//! These tests exercise the full pipeline: directive parsing, query execution,
//! and chapter processing.

use std::collections::HashMap;
use std::path::Path;

use mdbook_treesitter::{
    Directive,
    language::build_registry,
    process_chapter,
    query::{apply_strip, run_jq_query, run_treesitter_query},
};

// ─── Source fixture ───────────────────────────────────────────────────────────

const GEOMETRY_RS: &str = r#"/// A simple point in 2D space.
///
/// This struct is used throughout the geometry module.
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// A rectangle defined by its top-left corner, width, and height.
#[derive(Debug, Clone, PartialEq)]
pub struct Rectangle {
    pub origin: Point,
    pub width: f64,
    pub height: f64,
}

/// Configuration for the geometry module.
#[derive(Debug)]
pub struct Config {
    pub precision: u32,
}
"#;

// ─── Query strings (mirrors what book.toml defines) ───────────────────────────

/// Tree-sitter query: captures doc-comment lines immediately before a struct.
/// Handles 0 or 1 intervening attribute items (e.g. `#[derive(...)]`).
const DOC_COMMENT: &str = r#"[
  ((line_comment)+ @doc_comment
   .
   (struct_item name: (type_identifier) @name))
  ((line_comment)+ @doc_comment
   .
   (attribute_item)
   .
   (struct_item name: (type_identifier) @name))
]"#;

/// Tree-sitter query: captures the full struct declaration.
const STRUCT: &str = r#"(struct_item name: (type_identifier) @name) @struct"#;

/// Tree-sitter query: captures each field_declaration inside a struct body.
const STRUCT_FIELDS: &str = r#"
(struct_item
  name: (type_identifier) @name
  body: (field_declaration_list
    (field_declaration) @field))
"#;

/// Strip regex for doc-comment delimiters (mirrors the book.toml value).
const COMMENT_STRIP: &str = r#"^///? ?"#;

/// jq filter: extracts the last contiguous block of doc-comment lines
/// immediately before the named struct (skipping any attribute items).
const DOC_COMMENT_JQ: &str = r#"
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
    select(.type == "line_comment") | .text | rtrimstr("\n")] |
  join("\n")
end
"#;

// ─── Directive parsing ────────────────────────────────────────────────────────

#[test]
fn parse_directive_file_only() {
    let d = Directive::parse("../../foo.rs").unwrap();
    assert_eq!(d.file_path, "../../foo.rs");
    assert_eq!(d.query_name, None);
    assert!(d.params.is_empty());
}

#[test]
fn parse_directive_with_query() {
    let d = Directive::parse("foo.rs#doc_comment").unwrap();
    assert_eq!(d.file_path, "foo.rs");
    assert_eq!(d.query_name.as_deref(), Some("doc_comment"));
    assert!(d.params.is_empty());
}

#[test]
fn parse_directive_with_query_and_param() {
    let d = Directive::parse("foo.rs#doc_comment?name=Foo").unwrap();
    assert_eq!(d.file_path, "foo.rs");
    assert_eq!(d.query_name.as_deref(), Some("doc_comment"));
    assert_eq!(d.params.get("name").map(|s| s.as_str()), Some("Foo"));
}

#[test]
fn parse_directive_with_multiple_params() {
    let d = Directive::parse("foo.rs#struct?name=Bar&visibility=pub").unwrap();
    assert_eq!(d.query_name.as_deref(), Some("struct"));
    assert_eq!(d.params["name"], "Bar");
    assert_eq!(d.params["visibility"], "pub");
}

#[test]
fn parse_directive_spaces_around_path() {
    let d = Directive::parse("  foo.rs  ").unwrap();
    assert_eq!(d.file_path, "foo.rs");
}

// ─── Tree-sitter query: doc_comment ──────────────────────────────────────────

fn rust_language() -> tree_sitter::Language {
    tree_sitter::Language::new(tree_sitter_rust::LANGUAGE)
}

#[test]
fn ts_doc_comment_point() {
    let mut params = HashMap::new();
    params.insert("name".into(), "Point".into());
    let result = run_treesitter_query(&rust_language(), GEOMETRY_RS, DOC_COMMENT, &params).unwrap();
    assert!(
        result.contains("A simple point in 2D space."),
        "got: {result}"
    );
    assert!(result.contains("This struct is used"), "got: {result}");
    // Must NOT contain Rectangle doc comment
    assert!(!result.contains("rectangle"), "got: {result}");
    // Must NOT have blank lines between comment lines
    assert!(
        !result.contains("\n\n"),
        "unexpected blank line in: {result}"
    );
}

#[test]
fn ts_doc_comment_rectangle() {
    let mut params = HashMap::new();
    params.insert("name".into(), "Rectangle".into());
    let result = run_treesitter_query(&rust_language(), GEOMETRY_RS, DOC_COMMENT, &params).unwrap();
    assert!(result.contains("A rectangle defined"), "got: {result}");
    assert!(!result.contains("Point"), "got: {result}");
    assert!(
        !result.contains("\n\n"),
        "unexpected blank line in: {result}"
    );
}

#[test]
fn ts_doc_comment_no_match_returns_error() {
    let mut params = HashMap::new();
    params.insert("name".into(), "NonExistent".into());
    let err =
        run_treesitter_query(&rust_language(), GEOMETRY_RS, DOC_COMMENT, &params).unwrap_err();
    assert!(err.to_string().contains("no results"), "got: {err}");
}

// ─── Tree-sitter query: struct ────────────────────────────────────────────────

#[test]
fn ts_struct_rectangle_includes_body() {
    let mut params = HashMap::new();
    params.insert("name".into(), "Rectangle".into());
    let result = run_treesitter_query(&rust_language(), GEOMETRY_RS, STRUCT, &params).unwrap();
    assert!(result.contains("Rectangle"), "got: {result}");
    assert!(result.contains("width"), "got: {result}");
}

#[test]
fn ts_struct_point() {
    let mut params = HashMap::new();
    params.insert("name".into(), "Point".into());
    let result = run_treesitter_query(&rust_language(), GEOMETRY_RS, STRUCT, &params).unwrap();
    assert!(result.contains("pub struct Point"), "got: {result}");
    assert!(result.contains("pub x: f64"), "got: {result}");
}

// ─── jq query: doc_comment_jq ────────────────────────────────────────────────

#[test]
fn jq_doc_comment_point() {
    let mut params = HashMap::new();
    params.insert("name".into(), "Point".into());
    let result = run_jq_query(&rust_language(), GEOMETRY_RS, DOC_COMMENT_JQ, &params).unwrap();
    assert!(
        result.contains("A simple point in 2D space."),
        "got: {result}"
    );
    // Must NOT bleed into Rectangle's comments
    assert!(!result.contains("rectangle"), "got: {result}");
    assert!(
        !result.contains("\n\n"),
        "unexpected blank line in: {result}"
    );
}

#[test]
fn jq_doc_comment_config() {
    let mut params = HashMap::new();
    params.insert("name".into(), "Config".into());
    let result = run_jq_query(&rust_language(), GEOMETRY_RS, DOC_COMMENT_JQ, &params).unwrap();
    assert!(
        result.contains("Configuration for the geometry module."),
        "got: {result}"
    );
    // Must NOT contain Point's or Rectangle's doc comments
    assert!(!result.contains("simple point"), "got: {result}");
    assert!(!result.contains("rectangle"), "got: {result}");
}

// ─── process_chapter ─────────────────────────────────────────────────────────

fn make_registry() -> HashMap<String, mdbook_treesitter::language::LanguageEntry> {
    build_registry(&HashMap::new(), Path::new("/")).unwrap()
}

#[test]
fn process_chapter_whole_file() {
    use tempfile::tempdir;
    let dir = tempdir().unwrap();
    let src_path = dir.path().join("geometry.rs");
    std::fs::write(&src_path, GEOMETRY_RS).unwrap();

    let registry = make_registry();
    let content = "# Test\n\n```rust\n{{ #treesitter geometry.rs }}\n```\n";
    let result = process_chapter(content, dir.path(), &registry).unwrap();
    // The fence is passed through unchanged; the directive is replaced with raw text.
    assert!(result.contains("```rust"), "fence missing: {result}");
    assert!(result.contains("pub struct Point"), "got: {result}");
    // No double-fencing.
    assert!(
        !result.contains("```rust\n```rust"),
        "double fence: {result}"
    );
}

#[test]
fn process_chapter_missing_file_returns_error() {
    use tempfile::tempdir;
    let dir = tempdir().unwrap();
    let registry = make_registry();
    let content = "{{ #treesitter nonexistent.rs }}\n";
    let err = process_chapter(content, dir.path(), &registry).unwrap_err();
    assert!(
        err.to_string().contains("nonexistent.rs") || err.to_string().contains("reading"),
        "got: {err}"
    );
}

#[test]
fn process_chapter_unknown_extension_returns_error() {
    use tempfile::tempdir;
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("data.xyz"), "hello").unwrap();
    let registry = make_registry();
    let content = "{{ #treesitter data.xyz }}\n";
    let err = process_chapter(content, dir.path(), &registry).unwrap_err();
    assert!(
        err.to_string().contains(".xyz") || err.to_string().contains("no language"),
        "got: {err}"
    );
}

#[test]
fn process_chapter_no_braces_spaces() {
    use tempfile::tempdir;
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("geometry.rs"), GEOMETRY_RS).unwrap();

    let registry = make_registry();
    // Unspaced variant — whole-file expansion needs no query in the registry.
    let content = "{{#treesitter geometry.rs}}\n";
    let result = process_chapter(content, dir.path(), &registry).unwrap();
    assert!(result.contains("pub struct Point"), "got: {result}");
    assert!(!result.contains("```"), "unexpected fence: {result}");
}

#[test]
fn process_chapter_escaped_directive_is_not_expanded() {
    use tempfile::tempdir;
    let dir = tempdir().unwrap();
    std::fs::write(dir.path().join("geometry.rs"), GEOMETRY_RS).unwrap();

    let registry = make_registry();
    let content = r"\{{ #treesitter geometry.rs#doc_comment?name=Point }}";
    let result = process_chapter(content, dir.path(), &registry).unwrap();
    // Backslash consumed, directive emitted literally.
    assert_eq!(
        result.trim(),
        "{{ #treesitter geometry.rs#doc_comment?name=Point }}"
    );
}

// ─── Language registry ────────────────────────────────────────────────────────

#[test]
fn registry_has_builtin_languages() {
    let registry = make_registry();
    assert!(registry.contains_key("rs"), "missing Rust");
    assert!(registry.contains_key("toml"), "missing TOML");
    assert!(registry.contains_key("md"), "missing Markdown");
}

#[test]
fn registry_rust_has_no_builtin_queries() {
    // Queries are defined in book.toml, not compiled in.
    let registry = make_registry();
    let rust = registry.get("rs").unwrap();
    assert!(rust.queries.is_empty(), "expected no built-in queries");
}

// ─── strip post-processing ────────────────────────────────────────────────────

#[test]
fn apply_strip_removes_comment_delimiters() {
    let input = "/// A simple point in 2D space.\n///\n/// Multi-line.";
    let result = apply_strip(input, COMMENT_STRIP).unwrap();
    assert_eq!(result, "A simple point in 2D space.\n\nMulti-line.");
}

#[test]
fn apply_strip_invalid_regex_returns_error() {
    let err = apply_strip("anything", "[invalid").unwrap_err();
    assert!(
        err.to_string().contains("invalid strip regex"),
        "got: {err}"
    );
}

#[test]
fn ts_comment_text_point_no_delimiters() {
    let mut params = HashMap::new();
    params.insert("name".into(), "Point".into());
    // Run raw query then apply strip — mirrors what run_query does with a config.
    let raw = run_treesitter_query(&rust_language(), GEOMETRY_RS, DOC_COMMENT, &params).unwrap();
    let result = apply_strip(&raw, COMMENT_STRIP).unwrap();
    assert!(
        result.contains("A simple point in 2D space."),
        "got: {result}"
    );
    // No `///` left in output.
    assert!(!result.contains("///"), "delimiters not stripped: {result}");
}

// ─── struct_fields capture ────────────────────────────────────────────────────

#[test]
fn ts_struct_fields_point() {
    let mut params = HashMap::new();
    params.insert("name".into(), "Point".into());
    let result =
        run_treesitter_query(&rust_language(), GEOMETRY_RS, STRUCT_FIELDS, &params).unwrap();
    // Individual fields, no surrounding struct scaffolding.
    assert!(result.contains("pub x: f64"), "got: {result}");
    assert!(result.contains("pub y: f64"), "got: {result}");
    assert!(
        !result.contains("pub struct"),
        "struct header leaked: {result}"
    );
    assert!(!result.contains('{'), "brace leaked: {result}");
}

#[test]
fn ts_struct_fields_rectangle() {
    let mut params = HashMap::new();
    params.insert("name".into(), "Rectangle".into());
    let result =
        run_treesitter_query(&rust_language(), GEOMETRY_RS, STRUCT_FIELDS, &params).unwrap();
    assert!(result.contains("pub origin: Point"), "got: {result}");
    assert!(result.contains("pub width: f64"), "got: {result}");
    assert!(result.contains("pub height: f64"), "got: {result}");
    assert!(
        !result.contains("pub struct"),
        "struct header leaked: {result}"
    );
}
