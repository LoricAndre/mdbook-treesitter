//! Query execution: runs either a tree-sitter S-expression query or a jq
//! filter against a source file and returns the matching code text.

use anyhow::{Context, Result, bail};
use log::debug;
use serde_json::{Value, json};
use std::collections::HashMap;
use streaming_iterator::StreamingIterator;
use tree_sitter::{Language, Node, Parser, Query, QueryCursor};

use crate::config::{QueryConfig, QueryFormat};

// ─── Tree-sitter query ────────────────────────────────────────────────────────

/// Runs a tree-sitter S-expression query against `source` and returns all
/// captured text segments that survive the `params` filter.
///
/// ## Filtering
///
/// `params` maps capture-name → expected text.  A match is *kept* when every
/// param key that corresponds to a capture name in the query has the expected
/// value.  The **primary output** is the text of captures whose name is *not*
/// used as a filter parameter — or, if every capture is used as a filter
/// parameter, the first captured text is returned.
///
/// ## Example
///
/// Query:
/// ```text
/// (struct_item name: (type_identifier) @name) @struct
/// ```
/// Params: `{ "name": "Foo" }`
///
/// → returns the text of the `@struct` capture for every `struct_item` whose
///   `@name` equals `"Foo"`.
/// Substitutes `{capture_name}` placeholders in `tmpl`, joining multiple
/// nodes captured under the same name with `\n` and optionally stripping each.
fn render_row(row: &HashMap<String, Vec<String>>, tmpl: &str, strip: Option<&str>) -> Result<String> {
    let mut line = tmpl.to_string();
    for (name, texts) in row {
        let joined = texts.join("\n");
        let value = match strip {
            Some(pat) => apply_strip(&joined, pat)?,
            None => joined,
        };
        line = line.replace(&format!("{{{name}}}"), &value);
    }
    Ok(line)
}

pub fn run_treesitter_query(
    language: &Language,
    source: &str,
    query_str: &str,
    params: &HashMap<String, String>,
    strip: Option<&str>,
    template: Option<&str>,
) -> Result<String> {
    let mut parser = Parser::new();
    parser.set_language(language).context("set language")?;

    let tree = parser
        .parse(source, None)
        .context("failed to parse source")?;

    let query = Query::new(language, query_str)
        .with_context(|| format!("invalid tree-sitter query:\n{query_str}"))?;

    let capture_names: Vec<&str> = query.capture_names().to_vec();

    // Identify which capture indices are used purely as filter parameters.
    let filter_indices: Vec<(u32, &str)> = capture_names
        .iter()
        .enumerate()
        .filter_map(|(i, &name)| {
            if params.contains_key(name) {
                Some((i as u32, name))
            } else {
                None
            }
        })
        .collect();

    // Output capture indices: everything that is *not* a filter parameter.
    let output_indices: Vec<u32> = capture_names
        .iter()
        .enumerate()
        .filter_map(|(i, &name)| {
            if !params.contains_key(name) {
                Some(i as u32)
            } else {
                None
            }
        })
        .collect();

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());

    let mut results: Vec<String> = Vec::new();

    'match_loop: while let Some(m) = matches.next() {
        // Check all filter captures.
        for &(filter_idx, param_name) in &filter_indices {
            let expected = params.get(param_name).unwrap();
            let matched = m
                .captures
                .iter()
                .any(|c| c.index == filter_idx && node_text(c.node, source) == expected.as_str());
            if !matched {
                continue 'match_loop;
            }
        }

        if let Some(tmpl) = template {
            // Template mode: walk output captures in source order and group
            // them into rows.  A new row begins when a capture name that is
            // already present in the current row reappears AND every expected
            // output capture name has been seen at least once.  This handles
            // two cases transparently:
            //
            // • One tree-sitter match per item (flat pattern): no repeated
            //   names within a match, so the whole match is one row.
            // • One tree-sitter match for the whole parent (e.g. an enum_item
            //   wrapper): all child captures are bundled into a single match,
            //   and row boundaries are detected via repetition.
            let output_names: std::collections::HashSet<String> = capture_names
                .iter()
                .enumerate()
                .filter(|(i, _)| output_indices.contains(&(*i as u32)))
                .map(|(_, &n)| n.to_string())
                .collect();

            let mut row: HashMap<String, Vec<String>> = HashMap::new();

            for c in m.captures {
                if !output_indices.contains(&c.index) {
                    continue;
                }
                let name = capture_names[c.index as usize].to_string();
                let text = node_text(c.node, source).trim_end().to_string();

                // Flush when a repeated name signals the start of the next row,
                // but only once every expected output name has been seen (so that
                // multi-node captures like `(line_comment)+` don't flush early).
                if row.contains_key(&name) && output_names.iter().all(|n| row.contains_key(n)) {
                    results.push(render_row(&row, tmpl, strip)?);
                    row.clear();
                }

                row.entry(name).or_default().push(text);
            }

            if !row.is_empty() {
                results.push(render_row(&row, tmpl, strip)?);
            }
        } else {
            // Plain mode: collect output captures as flat text segments.
            for c in m.captures {
                if output_indices.contains(&c.index) {
                    // Trim trailing whitespace so that line_comment nodes
                    // (which include their trailing '\n') join cleanly.
                    results.push(node_text(c.node, source).trim_end().to_string());
                }
            }
        }
    }

    if results.is_empty() {
        debug!(
            "tree-sitter query produced no results\nquery:\n{query_str}\ntree:\n{}",
            tree.root_node().to_sexp()
        );
        bail!("tree-sitter query produced no results");
    }

    Ok(results.join("\n"))
}

fn node_text<'a>(node: Node<'a>, source: &'a str) -> &'a str {
    &source[node.byte_range()]
}

// ─── AST → JSON ──────────────────────────────────────────────────────────────

/// Converts a tree-sitter `Node` (and its entire subtree) to a JSON `Value`.
///
/// The schema:
/// ```json
/// {
///   "type":        "<node kind>",
///   "text":        "<verbatim source text>",
///   "start_byte":  42,
///   "end_byte":    99,
///   "is_named":    true,
///   "children":    [ ... ]
/// }
/// ```
pub fn node_to_json(node: Node<'_>, source: &str) -> Value {
    let text = &source[node.byte_range()];
    let mut children = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        children.push(node_to_json(child, source));
    }

    json!({
        "type":       node.kind(),
        "text":       text,
        "start_byte": node.start_byte(),
        "end_byte":   node.end_byte(),
        "is_named":   node.is_named(),
        "children":   children,
    })
}

// ─── jq query ────────────────────────────────────────────────────────────────

/// Runs a jq filter against the tree-sitter AST of `source`.
///
/// The filter receives an object of the form:
/// ```json
/// {
///   "params":   { "name": "Foo", ... },
///   "source":   "...",
///   "children": [ ... ]   // top-level AST children
/// }
/// ```
///
/// Each output value is turned into a string: strings are used verbatim;
/// other JSON values are serialised to compact JSON.
pub fn run_jq_query(
    language: &Language,
    source: &str,
    filter_str: &str,
    params: &HashMap<String, String>,
) -> Result<String> {
    let mut parser = Parser::new();
    parser.set_language(language).context("set language")?;

    let tree = parser.parse(source, None).context("parse source")?;

    let root_json = node_to_json(tree.root_node(), source);

    // Build the input: top-level children + params + source text.
    let input = json!({
        "params":   params,
        "source":   source,
        "type":     root_json["type"],
        "children": root_json["children"],
    });

    run_jq_on_value(filter_str, input)
}

/// Runs a jq `filter_str` against a JSON `Value` using `jaq-core` + `jaq-std`
/// and returns all outputs joined by newlines.
fn run_jq_on_value(filter_str: &str, input: Value) -> Result<String> {
    use jaq_core::load::{Arena, File, Loader};
    use jaq_core::{Compiler, Ctx, Vars, data::JustLut, val::unwrap_valr};
    use jaq_json::Val;

    type D = JustLut<Val>;

    // Collect all definitions: core + std + json.
    let defs = jaq_core::defs()
        .chain(jaq_std::defs())
        .chain(jaq_json::defs());
    let funs = jaq_core::funs::<D>()
        .chain(jaq_std::funs::<D>())
        .chain(jaq_json::funs::<D>());

    let program = File {
        code: filter_str,
        path: (),
    };
    let loader = Loader::new(defs);
    let arena = Arena::default();

    let modules = loader.load(&arena, program).map_err(|errs| {
        let msgs: Vec<String> = errs
            .into_iter()
            .map(|(e, _path)| format!("{e:?}"))
            .collect();
        anyhow::anyhow!("jq load errors: {}", msgs.join("; "))
    })?;

    let filter = Compiler::default()
        .with_funs(funs)
        .compile(modules)
        .map_err(|errs: Vec<_>| {
            let msgs: Vec<String> = errs
                .into_iter()
                .map(|(e, _path)| format!("{e:?}"))
                .collect();
            anyhow::anyhow!("jq compile errors: {}", msgs.join("; "))
        })?;

    // Convert serde_json::Value → JSON bytes → jaq_json::Val.
    let json_bytes = serde_json::to_vec(&input).context("serialise input to JSON")?;
    let jaq_input = jaq_json::read::parse_single(&json_bytes)
        .map_err(|e| anyhow::anyhow!("parse jq input: {e}"))?;

    let ctx = Ctx::<D>::new(&filter.lut, Vars::new([]));

    let mut results = Vec::new();
    for output in filter.id.run((ctx, jaq_input)) {
        let v = unwrap_valr(output).map_err(|e| anyhow::anyhow!("jq runtime error: {e}"))?;
        let text = match &v {
            Val::TStr(s) | Val::BStr(s) => String::from_utf8_lossy(s).into_owned(),
            other => format!("{other}"),
        };
        results.push(text);
    }

    if results.is_empty() {
        bail!("jq filter produced no results");
    }

    Ok(results.join("\n"))
}

// ─── Dispatch ────────────────────────────────────────────────────────────────

/// Runs the query (tree-sitter or jq) against `source` and returns the
/// extracted text.  If the query config contains a `strip` regex, it is
/// applied to every output line (matches are deleted).
pub fn run_query(
    language: &Language,
    source: &str,
    query_cfg: &QueryConfig,
    params: &HashMap<String, String>,
) -> Result<String> {
    match query_cfg.format() {
        QueryFormat::TreeSitter => {
            // strip and template are both forwarded; when a template is active,
            // strip is applied per-capture inside run_treesitter_query, so we
            // must not apply it again here.
            let raw = run_treesitter_query(
                language,
                source,
                query_cfg.query_str(),
                params,
                query_cfg.strip(),
                query_cfg.template(),
            )?;
            if query_cfg.template().is_some() {
                Ok(raw)
            } else {
                match query_cfg.strip() {
                    None => Ok(raw),
                    Some(pattern) => apply_strip(&raw, pattern),
                }
            }
        }
        QueryFormat::Jq => {
            let raw = run_jq_query(language, source, query_cfg.query_str(), params)?;
            match query_cfg.strip() {
                None => Ok(raw),
                Some(pattern) => apply_strip(&raw, pattern),
            }
        }
    }
}

/// Applies `pattern` as a regex, deleting every match from each line of `text`.
pub fn apply_strip(text: &str, pattern: &str) -> Result<String> {
    use regex::Regex;
    let re = Regex::new(pattern).with_context(|| format!("invalid strip regex `{pattern}`"))?;
    let result = text
        .lines()
        .map(|line| re.replace_all(line, "").into_owned())
        .collect::<Vec<_>>()
        .join("\n");
    Ok(result)
}
