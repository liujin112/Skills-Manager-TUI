//! Shared text highlighting, independent of page rendering.

use crate::tui::theme::Theme;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use skills::search::highlight_ranges;

/// Split `text` into spans, styling the parts that match `terms`.
pub fn highlight_spans<'a>(text: &str, terms: &[String], base: Style, th: &Theme) -> Vec<Span<'a>> {
    let ranges = highlight_ranges(text, terms);
    if ranges.is_empty() {
        return vec![Span::styled(text.to_string(), base)];
    }
    // The hit keeps none of the surrounding style: a highlighter covers what
    // is under it, and the dimmed grey of an excerpt would be unreadable on yellow.
    let hl = th.match_hit();
    let mut out = Vec::new();
    let mut pos = 0;
    for (s, e) in ranges {
        if s > pos {
            out.push(Span::styled(text[pos..s].to_string(), base));
        }
        out.push(Span::styled(text[s..e].to_string(), hl));
        pos = e;
    }
    if pos < text.len() {
        out.push(Span::styled(text[pos..].to_string(), base));
    }
    out
}

/// Apply highlighting to every span of an already styled line (markdown output).
pub fn highlight_line<'a>(line: Line<'a>, terms: &[String], th: &Theme) -> Line<'a> {
    if terms.is_empty() {
        return line;
    }
    let mut spans = Vec::new();
    for sp in line.spans {
        let base = sp.style;
        spans.extend(highlight_spans(&sp.content, terms, base, th));
    }
    Line::from(spans)
        .style(line.style)
        .alignment(line.alignment.unwrap_or_default())
}
