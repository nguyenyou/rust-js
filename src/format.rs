//! Format the JS as oxfmt does, and carry the source map over to it.
//!
//! ```text
//!   printed JS ──oxc_formatter──► formatted JS
//!       │                              │
//!     parse                          parse
//!       ▼                              ▼
//!   nodes in order  ◄──── paired ────► nodes in order
//!
//!   a mapping at a node's start in the printed JS  ──►  the same node's
//!   start in the formatted JS
//! ```
//!
//! The formatter changes where things are, never what they are, so the two
//! programs have the same nodes in the same order. It can add a few (JSX's
//! `{" "}`), which the pairing steps over.

use oxc_allocator::Allocator;
use oxc_ast::AstKind;
use oxc_ast::ast_kind::AstType;
use oxc_ast_visit::Visit;
use oxc_formatter::{
    ArrowParentheses, AttributePosition, BracketSameLine, BracketSpacing, Expand, JsFormatOptions, QuoteProperties,
    QuoteStyle, Semicolons, TrailingCommas, format, parse_for_format,
};
use oxc_formatter_core::{IndentStyle, IndentWidth, LineEnding, LineWidth};
use oxc_sourcemap::{OwnedSourceMap, SourceMap, SourceMapBuilder};
use oxc_span::{GetSpan, SourceType};

use crate::settings::{ArrowParens, EndOfLine, Format, ObjectWrap, QuoteProps, TrailingComma};

/// `code` as oxfmt formats it, with `options`, and `map`'s mappings moved
/// to match. `None` if the formatter fails, and the caller keeps what it has.
pub fn formatted(
    code: &str,
    map: &SourceMap<'_>,
    jsx: bool,
    js_file_name: &str,
    options: &JsFormatOptions,
) -> Option<(String, String)> {
    let source_type = if jsx { SourceType::jsx() } else { SourceType::mjs() };
    let allocator = Allocator::default();
    let text = format(&allocator, code, source_type, options.clone())
        .ok()?
        .print()
        .ok()?
        .into_code();
    let before = node_starts(&allocator, code, source_type).ok()?;
    let after = node_starts(&allocator, &text, source_type).ok()?;
    let map = moved(code, &text, map, &before, &after, js_file_name);
    Some((text, map))
}

/// `text`, a hook's layout of `code` (ADR 0117), with `map`'s mappings
/// moved to it, if it's the same program: the same nodes, in the same order,
/// but for the ones a formatter adds or takes out, JSX's text and `{" "}`.
/// `map` is JSON. Otherwise, why not.
pub fn transformed(
    code: &str,
    text: String,
    map: &str,
    jsx: bool,
    js_file_name: &str,
) -> Result<(String, String), String> {
    let map = SourceMap::from_json_string(map).map_err(|e| format!("was given a map it can't read: {e}"))?;
    let map = &map;
    let source_type = if jsx { SourceType::jsx() } else { SourceType::mjs() };
    let allocator = Allocator::default();
    let before = node_starts(&allocator, code, source_type).map_err(|e| format!("was given JS it can't read: {e}"))?;
    let after = node_starts(&allocator, &text, source_type).map_err(|e| format!("gave what isn't JS: {e}"))?;
    let (was, is) = (layout_free(&before, code), layout_free(&after, &text));
    if let Some(at) = (0..was.len().max(is.len())).find(|&i| was.get(i) != is.get(i)) {
        // Taken out, with what's in it, if the rest is what the rest was;
        // added, if what was there is the rest; or else made another.
        let change = match (was.get(at), is.get(at)) {
            (Some(node), _) if was.ends_with(&is[at..]) => format!("took out a `{node:?}`"),
            (_, Some(node)) if is.ends_with(&was[at..]) => format!("added a `{node:?}`"),
            (Some(was), Some(is)) => format!("made a `{was:?}` a `{is:?}`"),
            (Some(node), None) => format!("took out a `{node:?}`"),
            (None, Some(node)) => format!("added a `{node:?}`"),
            (None, None) => unreachable!("the kinds differ at {at}"),
        };
        return Err(format!("changed the program: it {change}"));
    }
    let map = moved(code, &text, map, &before, &after, js_file_name);
    Ok((text, map))
}

/// The kinds of `nodes`, of `code`, but for what a formatter lays out
/// otherwise: JSX's text, and the `{" "}` that stands for its spaces.
fn layout_free(nodes: &[(AstType, u32)], code: &str) -> Vec<AstType> {
    let mut kinds = Vec::with_capacity(nodes.len());
    let mut space = false;
    for &(kind, at) in nodes {
        let spaces = space && kind == AstType::StringLiteral;
        space = kind == AstType::JSXExpressionContainer
            && ["{\" \"}", "{' '}"].iter().any(|s| code[at as usize..].starts_with(s));
        if !(space || spaces || kind == AstType::JSXText) {
            kinds.push(kind);
        }
    }
    kinds
}

/// `map`, of `code`, moved to `text`, the same program laid out otherwise,
/// by pairing their nodes, `before` and `after`.
fn moved(
    code: &str,
    text: &str,
    map: &SourceMap<'_>,
    before: &[(AstType, u32)],
    after: &[(AstType, u32)],
    js_file_name: &str,
) -> String {
    let pairs = pair(before, after);
    let (old_lines, new_lines) = (Lines::new(code), Lines::new(text));

    let mut out = SourceMapBuilder::default();
    out.set_file(js_file_name);
    for (source, content) in map.get_sources().zip(map.get_source_contents()) {
        out.set_source_and_content(source, content.unwrap_or_default());
    }
    let name_ids: Vec<u32> = map.get_names().map(|name| out.add_name(name)).collect();
    let mut tokens: Vec<_> = map
        .get_tokens()
        .filter_map(|t| {
            old_lines
                .offset(t.get_dst_line(), t.get_dst_col())
                .map(|offset| (offset, t))
        })
        .collect();
    // Codegen coalesces identical mappings on one line: `return <button>`
    // may have only the return's mapping. Formatting moves the tag onto a
    // new line, where that mapping no longer applies. Carry the mapping
    // active at the original opening tag onto its new position as well.
    let mut openings = Vec::new();
    for &(ty, offset) in before {
        if matches!(ty, AstType::JSXOpeningElement | AstType::JSXOpeningFragment) {
            let at = tokens.partition_point(|&(old, _)| old <= offset);
            if let Some(&(old, token)) = at.checked_sub(1).map(|i| &tokens[i])
                && old != offset
                && !code[old as usize..offset as usize].contains('\n')
            {
                openings.push((offset, token));
            }
        }
    }
    tokens.extend(openings);
    tokens.sort_by_key(|&(offset, _)| offset);
    for (offset, t) in tokens {
        // The node that starts here, or else the last one before it on the
        // same line, and as far into it.
        let at = pairs.partition_point(|&(old, _)| old <= offset);
        let Some(&(old, new)) = at.checked_sub(1).map(|i| &pairs[i]) else {
            continue;
        };
        if old != offset && code[old as usize..offset as usize].contains('\n') {
            continue;
        }
        let (line, col) = new_lines.position(new + (offset - old));
        out.add_token(
            line,
            col,
            t.get_src_line(),
            t.get_src_col(),
            t.get_source_id(),
            t.get_name_id().map(|id| name_ids[id as usize]),
        );
    }
    out.into_sourcemap().to_json_string()
}

/// oxfmt's defaults, but an object is on one line when it fits: Prettier's
/// `objectWrap: "collapse"`. Keeping one on several lines, as oxfmt does by
/// default, keeps a layout a person chose; here, the printer chose it.
fn options() -> JsFormatOptions {
    JsFormatOptions {
        expand: Expand::Never,
        ..JsFormatOptions::default()
    }
}

/// rust-js's options with a crate's over them (ADR 0117), as oxfmt's
/// `to_oxc_formatter` sets them, or why one can't be.
pub fn options_of(format: &Format) -> Result<JsFormatOptions, String> {
    let mut o = options();
    if let Some(width) = format.print_width {
        o.line_width =
            LineWidth::try_from(width).map_err(|_| format!("printWidth = {width} is no width oxfmt takes"))?;
    }
    if let Some(width) = format.tab_width {
        o.indent_width =
            IndentWidth::try_from(width).map_err(|_| format!("tabWidth = {width} is no width oxfmt takes"))?;
    }
    if let Some(tabs) = format.use_tabs {
        o.indent_style = if tabs { IndentStyle::Tab } else { IndentStyle::Space };
    }
    if let Some(end) = format.end_of_line {
        o.line_ending = match end {
            EndOfLine::Lf => LineEnding::Lf,
            EndOfLine::Crlf => LineEnding::Crlf,
            EndOfLine::Cr => LineEnding::Cr,
        };
    }
    let quote = |single| if single { QuoteStyle::Single } else { QuoteStyle::Double };
    if let Some(single) = format.single_quote {
        o.quote_style = quote(single);
    }
    if let Some(single) = format.jsx_single_quote {
        o.jsx_quote_style = quote(single);
    }
    if let Some(props) = format.quote_props {
        o.quote_properties = match props {
            QuoteProps::AsNeeded => QuoteProperties::AsNeeded,
            QuoteProps::Consistent => QuoteProperties::Consistent,
            QuoteProps::Preserve => QuoteProperties::Preserve,
        };
    }
    if let Some(commas) = format.trailing_comma {
        o.trailing_commas = match commas {
            TrailingComma::All => TrailingCommas::All,
            TrailingComma::Es5 => TrailingCommas::Es5,
            TrailingComma::None => TrailingCommas::None,
        };
    }
    if let Some(semi) = format.semi {
        o.semicolons = if semi { Semicolons::Always } else { Semicolons::AsNeeded };
    }
    if let Some(parens) = format.arrow_parens {
        o.arrow_parentheses = match parens {
            ArrowParens::Always => ArrowParentheses::Always,
            ArrowParens::Avoid => ArrowParentheses::AsNeeded,
        };
    }
    if let Some(spacing) = format.bracket_spacing {
        o.bracket_spacing = BracketSpacing::from(spacing);
    }
    if let Some(same_line) = format.bracket_same_line {
        o.bracket_same_line = BracketSameLine::from(same_line);
    }
    if let Some(each) = format.single_attribute_per_line {
        o.attribute_position = if each {
            AttributePosition::Multiline
        } else {
            AttributePosition::Auto
        };
    }
    if let Some(wrap) = format.object_wrap {
        o.expand = match wrap {
            ObjectWrap::Preserve => Expand::Auto,
            ObjectWrap::Collapse => Expand::Never,
        };
    }
    Ok(o)
}

/// A written file's source map, for finding where its JS came from.
pub struct Map(OwnedSourceMap);

impl Map {
    pub fn read(json: &str) -> Option<Map> {
        OwnedSourceMap::from_json_string(json).ok().map(Map)
    }

    /// The source, as the map names it, and the line and column there, all
    /// 0-based, that the JS at `line` and `col` came from.
    pub fn original(&self, line: u32, col: u32) -> Option<(String, u32, u32)> {
        let table = self.0.generate_lookup_table();
        let token = self.0.lookup_token(&table, line, col)?;
        let source = self.0.get_source(token.get_source_id()?)?;
        Some((source.to_string(), token.get_src_line(), token.get_src_col()))
    }
}

/// Each node's kind and where it starts, in the order a visit meets them,
/// or the first error parsing `code`.
fn node_starts(allocator: &Allocator, code: &str, source_type: SourceType) -> Result<Vec<(AstType, u32)>, String> {
    struct Starts(Vec<(AstType, u32)>);
    impl<'a> Visit<'a> for Starts {
        fn enter_node(&mut self, kind: AstKind<'a>) {
            self.0.push((kind.ty(), kind.span().start));
        }
    }
    let parsed = parse_for_format(allocator, allocator.alloc_str(code), source_type);
    if let Some(error) = parsed.diagnostics.errors().next() {
        return Err(error.to_string());
    }
    let mut starts = Starts(Vec::new());
    starts.visit_program(&parsed.program);
    Ok(starts.0)
}

/// Pair the nodes of the two programs: where the kinds differ, the
/// formatted one has a node the other hasn't, which is stepped over. The
/// pairs are sorted by where they start before, first one kept.
fn pair(before: &[(AstType, u32)], after: &[(AstType, u32)]) -> Vec<(u32, u32)> {
    let mut pairs = Vec::with_capacity(before.len());
    let mut j = 0;
    for &(ty, old) in before {
        // Look a little way ahead for the same kind; past that, this node is
        // one the formatter dropped.
        let Some(k) = after[j..].iter().take(8).position(|&(t, _)| t == ty) else {
            continue;
        };
        pairs.push((old, after[j + k].1));
        j += k + 1;
    }
    pairs.sort_by_key(|&(old, _)| old);
    pairs.dedup_by_key(|&mut (old, _)| old);
    pairs
}

/// Byte offsets and a source map's positions (line, UTF-16 column) in a text.
struct Lines<'t> {
    text: &'t str,
    starts: Vec<u32>,
}

impl<'t> Lines<'t> {
    fn new(text: &'t str) -> Self {
        let starts = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(i, _)| i as u32 + 1))
            .collect();
        Lines { text, starts }
    }

    fn offset(&self, line: u32, col: u32) -> Option<u32> {
        let start = *self.starts.get(line as usize)?;
        let mut units = 0;
        for (i, c) in self.text[start as usize..].char_indices() {
            if units >= col || c == '\n' {
                return Some(start + i as u32);
            }
            units += c.len_utf16() as u32;
        }
        Some(self.text.len() as u32)
    }

    fn position(&self, offset: u32) -> (u32, u32) {
        let line = self.starts.partition_point(|&s| s <= offset) - 1;
        let start = self.starts[line] as usize;
        let col = self.text[start..offset as usize].encode_utf16().count() as u32;
        (line as u32, col)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Every snapshot of generated JS is as oxfmt would leave it: formatting
    /// it again changes nothing.
    #[test]
    fn snapshots_are_formatted() {
        let mut checked = 0;
        let mut dirs = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("test/snapshots")];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir).expect("the snapshots") {
                let path = entry.expect("an entry").path();
                let jsx = path.extension().is_some_and(|e| e == "jsx");
                if path.is_dir() {
                    dirs.push(path);
                } else if jsx || path.extension().is_some_and(|e| e == "js") {
                    let text = std::fs::read_to_string(&path).expect("a snapshot");
                    // Up to the source map's comment, which comes after the formatting.
                    let code = text
                        .rsplit_once("//# sourceMappingURL=")
                        .map_or(text.as_str(), |(code, _)| code);
                    let source_type = if jsx { SourceType::jsx() } else { SourceType::mjs() };
                    let allocator = Allocator::default();
                    let again = format(&allocator, code, source_type, options())
                        .expect("it parses")
                        .print()
                        .expect("it prints")
                        .into_code();
                    assert_eq!(again, code, "{} changes when formatted again", path.display());
                    checked += 1;
                }
            }
        }
        assert!(checked > 20, "only {checked} snapshots found");
    }
}
