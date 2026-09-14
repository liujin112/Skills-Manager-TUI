//! The framed skill card, shared by every page that lays skills out in a grid.
//!
//! A card shows its name, description, tags and source inside a rounded frame. The frame carries the selection,
//! so a match highlighted inside keeps its own background instead of being
//! painted over.

use super::preview::highlight_spans;
use crate::tui::app::Ctx;
use crate::tui::theme::Theme;
use crate::tui::widgets::{fit, pad, width};
use ratatui::Frame;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders};
use skills::reconcile::SkillRecord;

/// Narrowest a card may get before the grid gives up a column. Below this the
/// name and description become too cramped to read.
pub const MIN_CARD_W: u16 = 40;
/// Four content lines: identity, two description lines, and source with tags.
pub const CARD_H: u16 = 6;

/// Shared identity and optional description for tag and preset cards.
pub fn group_card(
    name: &str,
    count: usize,
    description: Option<&str>,
    color: Color,
    columns: usize,
    th: &Theme,
) -> Vec<Line<'static>> {
    let count = fit(&format!("{count} skills"), columns);
    let room = columns.saturating_sub(width(&count) + usize::from(columns > width(&count)));
    let marker = fit("● ", room);
    let name = pad(name, room.saturating_sub(width(&marker)));
    let mut lines = vec![Line::from(vec![
        Span::styled(marker, Style::default().fg(color)),
        Span::styled(name, th.bold()),
        Span::raw(if columns > width(&count) { " " } else { "" }),
        Span::styled(count, th.skill_count()),
    ])];
    if let Some(desc) = description.filter(|s| !s.trim().is_empty()) {
        let plain = tui_markdown::from_str(desc)
            .lines
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(" ");
        lines.push(Line::from(Span::styled(
            fit(&plain, columns),
            Style::default().fg(th.placeholder),
        )));
    }
    lines
}
/// Status and selection share a slot, including a separator before the name.
pub const MARKER_W: usize = 4;

/// Columns that fit in `width`, always at least one.
pub fn cols_for(width: u16) -> usize {
    ((width / MIN_CARD_W) as usize).max(1)
}

/// Draw the frame of one card and hand back the padded area inside it.
/// `on` is the selection; `focused` says whether that selection is the one the
/// keyboard is on: cyan marks focus, while a neutral bold frame remembers an
/// inactive selection.
pub fn frame(f: &mut Frame, cell: Rect, on: bool, focused: bool, th: &Theme) -> Rect {
    frame_styled(
        f,
        cell,
        if on && focused {
            th.accent().add_modifier(Modifier::BOLD)
        } else if on {
            th.bold()
        } else {
            th.dim().add_modifier(Modifier::DIM)
        },
    )
}

/// The same frame in a colour of the caller's choosing, for a card that is
/// warning about something.
pub fn frame_styled(f: &mut Frame, cell: Rect, border: Style) -> Rect {
    let b = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border);
    let inner = b.inner(cell).inner(Margin {
        horizontal: 1,
        vertical: 0,
    });
    f.render_widget(b, cell);
    inner
}

/// Filled labels own their palette: ANSI colours can be remapped by the
/// terminal, making a calculated foreground wrong after a theme switch.
fn pill_fill(color: Color) -> Color {
    let palette = [
        (32, 36, 44),
        (196, 72, 72),
        (112, 168, 88),
        (224, 184, 80),
        (112, 160, 224),
        (192, 144, 200),
        (88, 176, 184),
        (208, 212, 220),
        (96, 104, 116),
        (240, 128, 128),
        (156, 208, 128),
        (248, 216, 128),
        (156, 192, 248),
        (224, 176, 232),
        (144, 216, 224),
        (248, 248, 248),
    ];
    let index = match color {
        Color::Black => 0,
        Color::Red => 1,
        Color::Green => 2,
        Color::Yellow => 3,
        Color::Blue => 4,
        Color::Magenta => 5,
        Color::Cyan => 6,
        Color::Gray => 7,
        Color::DarkGray => 8,
        Color::LightRed => 9,
        Color::LightGreen => 10,
        Color::LightYellow => 11,
        Color::LightBlue => 12,
        Color::LightMagenta => 13,
        Color::LightCyan => 14,
        Color::White => 15,
        Color::Reset => 7,
        Color::Rgb(..) => return color,
        Color::Indexed(i) if i < 16 => i as usize,
        Color::Indexed(i) if i >= 232 => {
            let v = 8 + (i - 232) * 10;
            return Color::Rgb(v, v, v);
        }
        Color::Indexed(i) => {
            let levels = [0, 95, 135, 175, 215, 255];
            let i = (i - 16) as usize;
            return Color::Rgb(levels[i / 36], levels[i / 6 % 6], levels[i % 6]);
        }
    };
    let (r, g, b) = palette[index];
    Color::Rgb(r, g, b)
}

fn luminance(fill: Color) -> f64 {
    let Color::Rgb(r, g, b) = pill_fill(fill) else {
        unreachable!()
    };
    let linear = |c: u8| {
        let c = f64::from(c) / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
}

/// Choose whichever ink has the greater contrast against the rendered RGB.
pub fn ink(fill: Color) -> Color {
    let light = luminance(fill);
    if (light + 0.05) / 0.05 >= 1.05 / (light + 0.05) {
        Color::Rgb(0, 0, 0)
    } else {
        Color::Rgb(255, 255, 255)
    }
}

/// The colour a tag is filled with: the one `[[tags]]` gives it in the config,
/// else the theme's tag colour, so untitled tags still read as tags.
pub fn tag_fill(name: &str, ctx: &Ctx) -> Color {
    ctx.ws
        .config
        .tags
        .iter()
        .find(|t| t.name == name)
        .and_then(|t| t.color.as_deref())
        .and_then(|c| c.parse::<Color>().ok())
        .unwrap_or(ctx.theme.tag)
}

/// Every capsule shares spacing, clipping, contrast and selection styling.
pub struct Pill<'a> {
    pub name: &'a str,
    pub fill: Color,
    pub coverage: Option<(usize, usize)>,
    pub selected: bool,
    pub focused: bool,
}

impl<'a> Pill<'a> {
    pub fn new(name: &'a str, fill: Color) -> Self {
        Self {
            name,
            fill,
            coverage: None,
            selected: false,
            focused: false,
        }
    }

    pub fn render(&self, ctx: &Ctx, max_width: usize) -> Vec<Span<'static>> {
        let (mark, count) = match self.coverage {
            Some((installed, total)) => {
                let mark = if total == 0 {
                    "◦ "
                } else if installed == total {
                    "✓ "
                } else if installed == 0 {
                    "◌ "
                } else {
                    "◐ "
                };
                (mark, format!(" {installed}/{total}"))
            }
            None => ("", String::new()),
        };
        let (left, right) = ctx.ws.config.ui.pill_caps.glyphs();
        let fixed = width(left) + width(right) + width(mark) + width(&count) + 2;
        if max_width < fixed {
            return vec![];
        }
        let body = format!(" {mark}{}{count} ", fit(self.name, max_width - fixed));
        let fill = pill_fill(self.fill);
        let mut style = Style::default().bg(fill).fg(ink(fill));
        if self.selected {
            style = style.add_modifier(if self.focused {
                Modifier::BOLD | Modifier::UNDERLINED
            } else {
                Modifier::BOLD
            });
        }
        vec![
            Span::styled(left, Style::default().fg(fill)),
            Span::styled(body, style),
            Span::styled(right, Style::default().fg(fill)),
        ]
    }
}

/// Tags as capsules, as many as fit in `max_w`, then a count for the rest. A
/// filled shape with round ends is told apart from the text around it at a
/// glance, which a coloured word is not.
pub fn tag_pills(tags: &[String], ctx: &Ctx, max_w: usize) -> Vec<Span<'static>> {
    let mut out = Vec::new();
    let mut used = 0;
    for (i, t) in tags.iter().enumerate() {
        let pill = Pill::new(t, tag_fill(t, ctx)).render(ctx, usize::MAX);
        let w = pill.iter().map(Span::width).sum::<usize>() + usize::from(i > 0);
        // Keep room for the "+n" so the last thing on the line is never a
        // pill cut in half.
        let rest = tags.len() - i - 1;
        let reserve = if rest > 0 {
            width(&format!(" +{rest}"))
        } else {
            0
        };
        if used + w + reserve > max_w {
            let count = fit(&format!(" +{}", rest + 1), max_w.saturating_sub(used));
            if !count.is_empty() {
                out.push(Span::styled(count, ctx.theme.dim()));
            }
            break;
        }
        if i > 0 {
            out.push(Span::raw(" "));
        }
        out.extend(pill);
        used += w;
    }
    out
}

/// Human-facing identity; filesystem and deployment keys remain unchanged.
pub fn display_name(r: &SkillRecord) -> &str {
    r.name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| r.key.rsplit('/').next().unwrap_or(&r.key))
}

/// A readable Git source badge, separate from the skill's own name.
pub fn repository_badge(r: &SkillRecord, icons: skills::config::Icons) -> Option<String> {
    use skills::meta::Source;
    match &r.source {
        Some(Source::Git { url, .. }) => {
            let name = skills::repository::source_name(url).unwrap_or_else(|| url.clone());
            Some(format!("{} {name}", crate::tui::icons::git(icons, url)))
        }
        Some(Source::Local { .. }) => Some(crate::tui::icons::local(icons).into()),
        None => skills::repository::alias_of(&r.key)
            .map(|alias| format!("{} {alias}", crate::tui::icons::package(icons))),
    }
}

/// The checkbox preserves the status slot and separates it from the name.
pub fn checkbox_marker(checked: bool, th: &Theme) -> Span<'static> {
    Span::styled(
        if checked { "[✓] " } else { "[ ] " },
        if checked { th.accent() } else { th.dim() },
    )
}

/// A fixed-width leading slot, replaced by a checkbox in selection mode.
pub fn health_marker(r: &SkillRecord, th: &Theme) -> Span<'static> {
    use skills::reconcile::SkillStatus::*;
    let (glyph, style) = match &r.status {
        Local | Repository => (" ●  ", th.ok()),
        MissingBaseline => (" ●  ", th.warn()),
        MissingSource => ("!   ", th.warn()),
        Modified => ("~   ", th.warn()),
        Missing | Invalid { .. } | CorruptMeta { .. } => ("!   ", th.err()),
        Renamed { .. } => ("!   ", th.warn()),
    };
    Span::styled(glyph, style)
}

/// Render Markdown as readable text and wrap at words where possible. CJK and
/// long unbroken tokens wrap at grapheme boundaries, never splitting an emoji.
pub(super) fn summary_lines(markdown: &str, columns: usize) -> [String; 2] {
    let plain = tui_markdown::from_str(markdown)
        .lines
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if columns == 0 {
        return [String::new(), String::new()];
    }
    let span = Span::raw(plain.as_str());
    let mut used = 0;
    let mut end = 0;
    for g in span.styled_graphemes(Style::default()) {
        let next = width(g.symbol);
        if used + next > columns {
            break;
        }
        used += next;
        end += g.symbol.len();
    }
    if end == plain.len() {
        return [plain, String::new()];
    }
    // Prefer a word boundary unless it would leave more than half a line blank.
    if let Some(space) = plain[..end].rfind(' ')
        && width(&plain[..space]) >= columns / 2
    {
        end = space;
    }
    let first = plain[..end].trim_end().to_owned();
    let rest = plain[end..].trim_start();
    let second = if width(rest) <= columns {
        rest.to_owned()
    } else {
        let span = Span::raw(rest);
        let mut result = String::new();
        let mut used = 0;
        for g in span.styled_graphemes(Style::default()) {
            let next = width(g.symbol);
            if used + next > columns.saturating_sub(1) {
                break;
            }
            used += next;
            result.push_str(g.symbol);
        }
        result.push('…');
        result
    };
    [first, second]
}

/// Identity, a two-line readable summary, and source beside right-aligned tags.
/// `body` supplies a search excerpt; `tail` can add page-specific context.
pub fn skill_card(
    r: &SkillRecord,
    ctx: &Ctx,
    inner_w: usize,
    body: Option<&str>,
    tail: &str,
    terms: &[String],
    show_tags: bool,
) -> Vec<Line<'static>> {
    let th = ctx.theme;
    let source = repository_badge(r, ctx.ws.config.ui.icons)
        .unwrap_or_else(|| crate::tui::icons::local(ctx.ws.config.ui.icons).into());
    let source = if tail.is_empty() || tail == "git" || tail == "local" {
        source
    } else {
        format!("{source} · {tail}")
    };
    let mut head = vec![health_marker(r, th)];
    if inner_w < MARKER_W {
        head[0].content = fit(&head[0].content, inner_w).into();
    }
    head.extend(highlight_spans(
        &pad(display_name(r), inner_w.saturating_sub(MARKER_W)),
        terms,
        th.bold(),
        th,
    ));

    let summary = summary_lines(
        body.or(r.description.as_deref())
            .unwrap_or("No description"),
        inner_w,
    );
    let tags_budget = if show_tags { inner_w / 2 } else { 0 };
    let pills = tag_pills(&r.tags, ctx, tags_budget);
    let pills_w: usize = pills.iter().map(|s| width(&s.content)).sum();
    let source_budget = inner_w.saturating_sub(pills_w + usize::from(pills_w > 0));
    let source = fit(&source, source_budget);
    let mut foot = vec![Span::styled(source.clone(), Style::default().fg(th.source))];
    foot.push(Span::raw(
        " ".repeat(inner_w.saturating_sub(width(&source) + pills_w)),
    ));
    foot.extend(pills);
    vec![
        Line::from(head),
        Line::from(highlight_spans(
            &summary[0],
            terms,
            Style::default().add_modifier(Modifier::DIM),
            th,
        )),
        Line::from(highlight_spans(
            &summary[1],
            terms,
            Style::default().add_modifier(Modifier::DIM),
            th,
        )),
        Line::from(foot),
    ]
}

#[cfg(test)]
mod tests {
    #[test]
    fn pills_share_coverage_spacing_focus_and_width_for_every_cap_style() {
        let root = skills::ops::DownloadDir::new("pill-format").unwrap();
        let mut ws = skills::Workspace::open(root.path()).unwrap();
        let snap = ws.scan().unwrap();
        let theme = Theme::default();
        for caps in [
            skills::config::PillCaps::Round,
            skills::config::PillCaps::Block,
            skills::config::PillCaps::None,
        ] {
            ws.config.ui.pill_caps = caps;
            let ctx = Ctx {
                ws: &ws,
                snap: &snap,
                theme: &theme,
            };
            for (coverage, text) in [
                ((0, 0), " ◦ lark 0/0 "),
                ((0, 28), " ◌ lark 0/28 "),
                ((1, 28), " ◐ lark 1/28 "),
                ((28, 28), " ✓ lark 28/28 "),
            ] {
                let pill = Pill {
                    coverage: Some(coverage),
                    ..Pill::new("lark", theme.tag)
                };
                let spans = pill.render(&ctx, 100);
                assert_eq!(spans[1].content, text);
                assert_eq!(spans[0].content, caps.glyphs().0);
                assert_eq!(spans[2].content, caps.glyphs().1);
                let selected = Pill {
                    selected: true,
                    focused: true,
                    ..pill
                };
                let focused = selected.render(&ctx, 100);
                assert_eq!(focused[1].style.bg, spans[1].style.bg);
                assert!(
                    focused[1]
                        .style
                        .add_modifier
                        .contains(Modifier::BOLD | Modifier::UNDERLINED)
                );
                for width in 0..30 {
                    assert!(
                        selected
                            .render(&ctx, width)
                            .iter()
                            .map(Span::width)
                            .sum::<usize>()
                            <= width
                    );
                }
            }
        }
    }
    use super::*;

    #[test]
    fn summaries_wrap_readable_markdown_and_keep_graphemes_intact() {
        assert_eq!(summary_lines("**Hello** `world`", 20), ["Hello world", ""]);
        assert_eq!(
            summary_lines("中文测试日历管理", 8),
            ["中文测试", "日历管理"]
        );
        assert_eq!(
            summary_lines("one two three four five", 10),
            ["one two", "three fou…"]
        );
        assert_eq!(summary_lines("👩‍💻👩‍💻👩‍💻", 4), ["👩‍💻👩‍💻", "👩‍💻"]);
        assert_eq!(cols_for(280), 7);
    }

    #[test]
    fn cards_use_frontmatter_names_and_keep_repository_identity_separate() {
        let dir = skills::ops::DownloadDir::new("card-render-test").unwrap();
        let root = dir.path();
        skills::config::Config {
            agents: vec![],
            ..Default::default()
        }
        .save(root)
        .unwrap();
        let key = "repos/sampleorg--kit/skills--mock-calendar";
        let path = root.join(key);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(
            path.join("SKILL.md"),
            "---\nname: mock-calendar\ndescription: 日历管理\n---\nCalendar tools\n",
        )
        .unwrap();
        let ws = skills::Workspace::open(root).unwrap();
        ws.meta
            .save(
                key,
                &skills::meta::SkillMeta {
                    source: Some(skills::meta::Source::Git {
                        url: "https://github.com/sampleorg/kit.git".into(),
                        branch: Some("main".into()),
                        subpath: Some("skills/mock-calendar".into()),
                        revision: None,
                    }),
                    ..Default::default()
                },
            )
            .unwrap();
        let snap = ws.scan().unwrap();
        let theme = Theme::default();
        let ctx = Ctx {
            ws: &ws,
            snap: &snap,
            theme: &theme,
        };
        for fill in (0..=255).map(Color::Indexed).chain([
            Color::Black,
            Color::Red,
            Color::Green,
            Color::Yellow,
            Color::Blue,
            Color::Magenta,
            Color::Cyan,
            Color::Gray,
            Color::DarkGray,
            Color::LightRed,
            Color::LightGreen,
            Color::LightYellow,
            Color::LightBlue,
            Color::LightMagenta,
            Color::LightCyan,
            Color::White,
            Color::Reset,
            Color::Rgb(128, 128, 128),
            Color::Rgb(0, 180, 0),
            Color::Rgb(255, 0, 128),
        ]) {
            let spans = Pill::new("label", fill).render(&ctx, usize::MAX);
            let bg = spans[1].style.bg.unwrap();
            let fg = spans[1].style.fg.unwrap();
            assert!(matches!(bg, Color::Rgb(..)));
            assert_eq!(spans[0].style.fg, Some(bg));
            assert_eq!(spans[2].style.fg, Some(bg));
            let (a, b) = (luminance(fg), luminance(bg));
            assert!((a.max(b) + 0.05) / (a.min(b) + 0.05) >= 4.5, "{fill:?}");
        }
        let mut record = snap.get(key).unwrap().clone();
        for status in [
            skills::reconcile::SkillStatus::Repository,
            skills::reconcile::SkillStatus::MissingSource,
            skills::reconcile::SkillStatus::Modified,
            skills::reconcile::SkillStatus::Missing,
        ] {
            let mut marker_record = record.clone();
            marker_record.status = status;
            let marker = health_marker(&marker_record, &theme);
            assert_eq!(marker.width(), MARKER_W);
            assert!(marker.content.ends_with(' '));
        }
        for checked in [false, true] {
            let marker = checkbox_marker(checked, &theme);
            assert_eq!(marker.width(), MARKER_W);
            assert!(marker.content.ends_with("] "));
        }
        let lines = skill_card(&record, &ctx, 60, None, "name", &[], true);
        assert!(lines[0].to_string().contains("mock-calendar"));
        assert!(!lines[0].to_string().contains("skills--"));
        assert!(lines[3].to_string().contains("󰊤 sampleorg/kit"));
        assert!(lines[3].to_string().contains("name"));
        record.description = Some("**Description emphasis** with `code`".into());
        let preview = super::super::preview::preview_lines(&record, &ctx, &[], 60);
        let description_start = preview
            .iter()
            .position(|line| line.to_string() == "Description")
            .unwrap();
        let body_start = preview
            .iter()
            .position(|line| line.to_string() == "SKILL.md")
            .unwrap();
        assert_eq!(preview[description_start + 1], preview[body_start + 1]);
        assert!(
            preview[description_start + 2..body_start]
                .iter()
                .flat_map(|line| &line.spans)
                .any(|span| span.content.contains("Description emphasis")
                    && span.style.add_modifier.contains(Modifier::BOLD))
        );

        assert!(preview[0].to_string().starts_with("mock-calendar"));
        assert!(
            !preview
                .iter()
                .any(|line| line.to_string().contains("≠ directory name"))
        );
        assert_eq!(record.key, key);
        assert_eq!(record.deployment_name(), "skills--mock-calendar");
        record.name = Some("中文日历".into());
        record.tags = vec!["A very long tag".into(), "中文标签".into(), "third".into()];
        for width in [0, 1, 2, 3, 8, 16, 24, 40, 80] {
            for line in skill_card(&record, &ctx, width, None, "git", &[], true) {
                assert!(line.width() <= width);
            }
        }
        record.name = None;
        assert_eq!(display_name(&record), "skills--mock-calendar");
    }
}
