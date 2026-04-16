//! Configuration types for the mdbook-treesitter preprocessor.
//!
//! Loaded from `book.toml` under `[preprocessor.treesitter]`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Top-level preprocessor configuration.
///
/// ```toml
/// [preprocessor.treesitter]
/// command = "cargo run --manifest-path ../Cargo.toml"  # optional, for dev
/// ```
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Config {
    /// The command mdBook uses to invoke the preprocessor.
    /// Declared here so serde doesn't try to parse it as a language config.
    #[serde(default)]
    pub command: Option<String>,
    /// Per-language configuration, keyed by language name (e.g. "rust", "toml").
    #[serde(flatten)]
    pub languages: HashMap<String, LanguageConfig>,
}

/// Configuration for a single language.
///
/// ```toml
/// [preprocessor.treesitter.python]
/// parser = "/path/to/parser.so"
///
/// [preprocessor.treesitter.python.queries]
/// my_query = "(function_definition name: (identifier) @name) @func"
/// ```
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct LanguageConfig {
    /// Path to a custom `.so` parser. Supports absolute and relative paths
    /// (resolved relative to `book.toml`).
    pub parser: Option<String>,
    /// Named queries for this language.
    #[serde(default)]
    pub queries: HashMap<String, QueryConfig>,
}

/// A named query — either a raw tree-sitter S-expression string, or a table
/// with an explicit format and optional post-processing.
///
/// ```toml
/// # Simple tree-sitter query (string form — no post-processing)
/// [preprocessor.treesitter.rust.queries]
/// struct = "(struct_item name: (type_identifier) @name) @struct"
///
/// # Tree-sitter query with a strip regex (table form, format defaults to treesitter)
/// [preprocessor.treesitter.rust.queries.comment_text]
/// query = "((line_comment)+ @doc_comment ...)"
/// strip = "^///? ?"
///
/// # jq query (table form with explicit format)
/// [preprocessor.treesitter.rust.queries.doc_comment_jq]
/// format = "jq"
/// query = ".children[] | select(.type == \"struct_item\") | ..."
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum QueryConfig {
    /// A plain tree-sitter S-expression query string (no strip).
    TreeSitter(String),
    /// A structured query: format defaults to `treesitter` when omitted.
    ///
    /// The optional `strip` field is a regex applied to every output line —
    /// matches are removed, which lets you strip comment delimiters, braces,
    /// leading whitespace, etc.
    Structured {
        #[serde(default)]
        format: QueryFormat,
        query: String,
        /// Regex whose matches are deleted from each output line.
        #[serde(default)]
        strip: Option<String>,
    },
}

/// The query language format.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum QueryFormat {
    /// Tree-sitter S-expression query (default).
    #[default]
    TreeSitter,
    /// jq filter applied to the tree-sitter AST converted to JSON.
    Jq,
}

impl QueryConfig {
    pub fn format(&self) -> QueryFormat {
        match self {
            QueryConfig::TreeSitter(_) => QueryFormat::TreeSitter,
            QueryConfig::Structured { format, .. } => format.clone(),
        }
    }

    pub fn query_str(&self) -> &str {
        match self {
            QueryConfig::TreeSitter(s) => s.as_str(),
            QueryConfig::Structured { query, .. } => query.as_str(),
        }
    }

    /// Returns the strip regex, if any.
    pub fn strip(&self) -> Option<&str> {
        match self {
            QueryConfig::TreeSitter(_) => None,
            QueryConfig::Structured { strip, .. } => strip.as_deref(),
        }
    }
}
