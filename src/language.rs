//! Language registry: maps file extensions to tree-sitter parsers.
//!
//! Queries are intentionally not bundled here — define them in `book.toml`
//! under `[preprocessor.treesitter.<lang>.queries]` so they can be edited
//! without recompiling or reinstalling the preprocessor binary.

use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use tree_sitter::{Language, Parser};

use crate::config::QueryConfig;

/// A resolved language entry: the tree-sitter `Language` plus any named queries
/// that are available for it.
pub struct LanguageEntry {
    pub language: Language,
    /// Named queries keyed by query name.
    pub queries: HashMap<String, QueryConfig>,
}

/// Builds the file-extension → `LanguageEntry` map, merging the built-in
/// defaults with any user-supplied overrides from `book.toml`.
///
/// `user_configs` maps language name (as used in `book.toml`) to its config.
/// `book_root` is used to resolve relative parser paths.
pub fn build_registry(
    user_configs: &HashMap<String, crate::config::LanguageConfig>,
    book_root: &std::path::Path,
) -> Result<HashMap<String, LanguageEntry>> {
    let mut registry: HashMap<String, LanguageEntry> = HashMap::new();

    // ── Rust ──────────────────────────────────────────────────────────────
    {
        registry.insert(
            "rs".into(),
            LanguageEntry {
                language: Language::new(tree_sitter_rust::LANGUAGE),
                queries: HashMap::new(),
            },
        );
    }

    // ── TOML ─────────────────────────────────────────────────────────────
    {
        registry.insert(
            "toml".into(),
            LanguageEntry {
                language: tree_sitter_toml::language(),
                queries: HashMap::new(),
            },
        );
    }

    // ── Markdown ─────────────────────────────────────────────────────────
    {
        registry.insert(
            "md".into(),
            LanguageEntry {
                language: Language::new(tree_sitter_md::LANGUAGE),
                queries: HashMap::new(),
            },
        );
    }

    // ── User-supplied overrides ───────────────────────────────────────────
    for (lang_name, lang_cfg) in user_configs {
        let ext = lang_name_to_ext(lang_name);

        if let Some(parser_path_str) = &lang_cfg.parser {
            let parser_path = if std::path::Path::new(parser_path_str).is_absolute() {
                std::path::PathBuf::from(parser_path_str)
            } else {
                book_root.join(parser_path_str)
            };
            let language = unsafe { load_language_from_so(&parser_path)? };
            let entry = registry
                .entry(ext.to_string())
                .or_insert_with(|| LanguageEntry {
                    language: language.clone(),
                    queries: HashMap::new(),
                });
            entry.language = language;
        }

        // Merge user queries on top of any built-ins.
        if let Some(entry) = registry.get_mut(ext) {
            for (qname, qcfg) in &lang_cfg.queries {
                entry.queries.insert(qname.clone(), qcfg.clone());
            }
        }
    }

    Ok(registry)
}

fn lang_name_to_ext(name: &str) -> &str {
    match name {
        "rust" => "rs",
        "markdown" => "md",
        "javascript" => "js",
        "typescript" => "ts",
        "python" => "py",
        "c" => "c",
        "cpp" | "c++" => "cpp",
        "go" => "go",
        other => other,
    }
}

/// Loads a tree-sitter `Language` from an external shared library.
///
/// # Safety
/// The `.so` must export a valid tree-sitter C ABI symbol.
unsafe fn load_language_from_so(_path: &std::path::Path) -> Result<Language> {
    bail!(
        "Dynamic parser loading from `{}` is not yet supported in this build.",
        _path.display()
    )
}

/// Creates a configured `Parser` for the given language.
pub fn make_parser(language: &Language) -> Result<Parser> {
    let mut parser = Parser::new();
    parser
        .set_language(language)
        .context("Failed to configure parser")?;
    Ok(parser)
}
