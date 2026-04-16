//! `mdbook-treesitter` — an mdBook preprocessor that replaces
//! `{{ #treesitter <path>#<query>?<params> }}` directives with the code
//! extracted from the referenced source file using tree-sitter queries.
//!
//! # Directive syntax
//!
//! ```markdown
//! {{ #treesitter path/to/file.rs }}
//! {{ #treesitter path/to/file.rs#query_name }}
//! {{ #treesitter path/to/file.rs#query_name?param1=val1&param2=val2 }}
//! ```
//!
//! The space around the braces is optional:
//!
//! ```markdown
//! {{#treesitter path/to/file.rs#doc_comment?name=Foo}}
//! ```

pub mod config;
pub mod language;
pub mod query;

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use mdbook_preprocessor::book::{Book, BookItem};
use mdbook_preprocessor::{Preprocessor, PreprocessorContext};
use regex::Regex;

use config::Config;
use language::LanguageEntry;

// ─── Regex ────────────────────────────────────────────────────────────────────

/// Matches an optional leading `\` then `{{ #treesitter ... }}`.
/// Group 1: the backslash (present → escaped, do not expand).
/// Group 2: the directive inner text.
fn directive_regex() -> Regex {
    Regex::new(r"(\\)?\{\{[\s]*#treesitter\s+([^\}]+?)\s*\}\}").unwrap()
}

// ─── Parsed directive ─────────────────────────────────────────────────────────

/// A parsed `#treesitter` directive.
#[derive(Debug, PartialEq, Eq)]
pub struct Directive {
    /// Path to the source file, relative to the chapter source file.
    pub file_path: String,
    /// Optional query name (the part after `#`).
    pub query_name: Option<String>,
    /// Query parameters (the part after `?`).
    pub params: HashMap<String, String>,
}

impl Directive {
    /// Parse a directive from its inner text, e.g.
    /// `../../foo.rs#doc_comment?name=Foo`.
    pub fn parse(inner: &str) -> Result<Self> {
        // Split on `#` first to separate the file path from query+params.
        let (file_part, rest) = if let Some(pos) = inner.find('#') {
            (&inner[..pos], Some(&inner[pos + 1..]))
        } else {
            (inner, None)
        };

        let file_path = file_part.trim().to_string();

        let (query_name, params) = match rest {
            None => (None, HashMap::new()),
            Some(rest) => {
                let (qname, params_str) = if let Some(pos) = rest.find('?') {
                    (&rest[..pos], Some(&rest[pos + 1..]))
                } else {
                    (rest, None)
                };

                let params = match params_str {
                    None => HashMap::new(),
                    Some(ps) => ps
                        .split('&')
                        .filter(|s| !s.is_empty())
                        .filter_map(|kv| {
                            let mut parts = kv.splitn(2, '=');
                            let k = parts.next()?.to_string();
                            let v = parts.next().unwrap_or("").to_string();
                            Some((k, v))
                        })
                        .collect(),
                };

                let qname = qname.trim();
                (
                    if qname.is_empty() {
                        None
                    } else {
                        Some(qname.to_string())
                    },
                    params,
                )
            }
        };

        Ok(Directive {
            file_path,
            query_name,
            params,
        })
    }
}

// ─── Preprocessor ─────────────────────────────────────────────────────────────

/// The `mdbook-treesitter` preprocessor.
pub struct TreesitterPreprocessor;

impl Preprocessor for TreesitterPreprocessor {
    fn name(&self) -> &str {
        "treesitter"
    }

    fn run(
        &self,
        ctx: &PreprocessorContext,
        mut book: Book,
    ) -> mdbook_preprocessor::errors::Result<Book> {
        let cfg = load_config(ctx)?;
        let book_root = ctx.root.clone();
        let src_dir = book_root.join(&ctx.config.book.src);

        let registry = language::build_registry(&cfg.languages, &book_root)
            .context("building language registry")?;

        let mut errors: Vec<String> = Vec::new();

        book.for_each_mut(|item| {
            if let BookItem::Chapter(chapter) = item {
                let chapter_dir = chapter
                    .path
                    .as_deref()
                    .and_then(|p| p.parent().map(|parent| src_dir.join(parent)))
                    .unwrap_or_else(|| src_dir.clone());

                match process_chapter(&chapter.content, &chapter_dir, &registry) {
                    Ok(new_content) => chapter.content = new_content,
                    Err(e) => errors.push(format!(
                        "chapter {:?}: {e:#}",
                        chapter.path.as_deref().unwrap_or(Path::new("<unknown>"))
                    )),
                }
            }
        });

        if !errors.is_empty() {
            return Err(anyhow::anyhow!(
                "mdbook-treesitter encountered errors:\n{}",
                errors.join("\n")
            )
            .into());
        }

        Ok(book)
    }

    fn supports_renderer(&self, renderer: &str) -> mdbook_preprocessor::errors::Result<bool> {
        // This preprocessor works with any renderer.
        Ok(renderer != "not-supported")
    }
}

// ─── Config loading ───────────────────────────────────────────────────────────

fn load_config(ctx: &PreprocessorContext) -> Result<Config> {
    match ctx.config.get::<Config>("preprocessor.treesitter") {
        Ok(Some(cfg)) => Ok(cfg),
        Ok(None) => Ok(Config::default()),
        Err(e) => Err(anyhow::anyhow!(
            "invalid [preprocessor.treesitter] config: {e}"
        )),
    }
}

// ─── Chapter processing ───────────────────────────────────────────────────────

/// Replace all `{{ #treesitter ... }}` directives in `content` with the
/// extracted code.  Directives that fail to resolve are reported as errors.
pub fn process_chapter(
    content: &str,
    chapter_dir: &Path,
    registry: &HashMap<String, LanguageEntry>,
) -> Result<String> {
    let re = directive_regex();
    let mut result = String::with_capacity(content.len());
    let mut last_end = 0;
    let mut first_error: Option<anyhow::Error> = None;

    for cap in re.captures_iter(content) {
        let full_match = cap.get(0).unwrap();
        let escaped = cap.get(1).is_some(); // leading backslash present
        let inner = cap.get(2).unwrap().as_str();

        result.push_str(&content[last_end..full_match.start()]);

        if escaped {
            // Strip the backslash; emit the directive literally.
            result.push_str("{{ #treesitter ");
            result.push_str(inner);
            result.push_str(" }}");
        } else {
            match resolve_directive(inner, chapter_dir, registry) {
                Ok(replacement) => result.push_str(&replacement),
                Err(e) => {
                    result.push_str(&format!("<!-- mdbook-treesitter error: {e} -->"));
                    if first_error.is_none() {
                        first_error = Some(e);
                    }
                }
            }
        }

        last_end = full_match.end();
    }

    result.push_str(&content[last_end..]);

    if let Some(e) = first_error {
        return Err(e);
    }

    Ok(result)
}

/// Resolve a single directive inner text (e.g. `../../foo.rs#doc_comment?name=Foo`)
/// and return the markdown replacement (a fenced code block).
fn resolve_directive(
    inner: &str,
    chapter_dir: &Path,
    registry: &HashMap<String, LanguageEntry>,
) -> Result<String> {
    let directive =
        Directive::parse(inner).with_context(|| format!("parsing directive `{inner}`"))?;

    let file_path = chapter_dir.join(&directive.file_path);
    let source = std::fs::read_to_string(&file_path)
        .with_context(|| format!("reading `{}`", file_path.display()))?;

    let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

    let lang_entry = registry
        .get(ext)
        .with_context(|| format!("no language registered for extension `.{ext}`"))?;

    let code = match &directive.query_name {
        None => {
            // No query — return the whole file.
            source.clone()
        }
        Some(qname) => {
            let query_cfg = lang_entry
                .queries
                .get(qname)
                .with_context(|| format!("no query `{qname}` registered for language `.{ext}`"))?;

            query::run_query(&lang_entry.language, &source, query_cfg, &directive.params)
                .with_context(|| format!("running query `{qname}` on `{}`", file_path.display()))?
        }
    };

    Ok(code)
}
