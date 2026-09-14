//! Shared presentation rules for tags and presets.

use crate::tui::app::Ctx;
use crate::tui::theme::Theme;
use crate::tui::widgets::{CardGrid, ScrollTrack, fit, pad, width};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::{
    Frame,
    layout::Rect,
    widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState},
};
use skills::preset::Preset;

/// Tags and presets use the same neutral fallback for missing or invalid colours.
pub fn color(explicit: Option<&str>, theme: &Theme) -> Color {
    explicit
        .and_then(|value| value.parse().ok())
        .unwrap_or(theme.tag)
}

pub fn tag_fill(name: &str, ctx: &Ctx) -> Color {
    color(
        ctx.ws
            .config
            .tags
            .iter()
            .find(|tag| tag.name == name)
            .and_then(|tag| tag.color.as_deref()),
        &ctx.settings.theme,
    )
}

pub fn preset_fill(preset: &Preset, ctx: &Ctx) -> Color {
    color(preset.color.as_deref(), &ctx.settings.theme)
}

/// One identity row plus the optional description, enclosed by two border rows.
pub fn card_height(description: Option<&str>) -> u16 {
    3 + u16::from(description.is_some_and(|value| !value.trim().is_empty()))
}

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

/// Filled labels own their palette: ANSI colours can be remapped by the
/// terminal, making a calculated foreground wrong after a theme switch.
fn pill_fill(color: Color, theme: &Theme) -> Color {
    let palette = theme.pill_palette;
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
    let Color::Rgb(r, g, b) = fill else {
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
fn ink(fill: Color) -> Color {
    let light = luminance(fill);
    if (light + 0.05) / 0.05 >= 1.05 / (light + 0.05) {
        Color::Rgb(0, 0, 0)
    } else {
        Color::Rgb(255, 255, 255)
    }
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
        let (left, right) = ctx.settings.ui.pill_caps.glyphs();
        let fixed = width(left) + width(right) + width(mark) + width(&count) + 2;
        if max_width < fixed {
            return vec![];
        }
        let body = format!(" {mark}{}{count} ", fit(self.name, max_width - fixed));
        let fill = pill_fill(self.fill, &ctx.settings.theme);
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
    if !ctx.settings.tags_enabled {
        return vec![];
    }
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
                out.push(Span::styled(count, ctx.settings.theme.dim()));
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

/// Every group card list uses the same scrollbar geometry and styling.
pub fn draw_track(
    f: &mut Frame,
    inner: Rect,
    grid: &CardGrid,
    selected: Option<usize>,
    track: &mut ScrollTrack,
    th: &crate::tui::theme::Theme,
) {
    let vis = grid.visible_rows();
    if grid.grid_rows() > vis && inner.height > 0 {
        let rect = Rect {
            x: inner.right().saturating_sub(1),
            y: inner.y,
            width: 1,
            height: inner.height,
        };
        track.set(rect);
        let mut sb = ScrollbarState::new(grid.grid_rows())
            .position(selected.unwrap_or(0) / grid.cols())
            .viewport_content_length(vis);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None)
                .style(th.dim()),
            rect,
            &mut sb,
        );
    } else {
        track.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filled_labels_keep_readable_contrast() {
        let dir = skills::ops::DownloadDir::new("pill-contrast").unwrap();
        let ws = skills::Workspace::open(dir.path()).unwrap();
        let snap = ws.scan().unwrap();
        let settings = crate::tui::settings::RuntimeSettings::new(&ws.config);
        let ctx = Ctx {
            ws: &ws,
            snap: &snap,
            settings: &settings,
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
    }

    #[test]
    fn group_colours_share_the_theme_default() {
        let theme = Theme::default();
        assert_eq!(color(None, &theme), theme.tag);
        assert_eq!(color(Some("not a colour"), &theme), theme.tag);
        assert_eq!(color(Some("#b87e54"), &theme), Color::Rgb(184, 126, 84));
        assert_eq!(color(Some("blue"), &theme), Color::Blue);
    }

    #[test]
    fn group_card_height_tracks_its_visible_description() {
        let theme = Theme::default();
        for description in [None, Some(""), Some(" \n "), Some("**Useful** tools")] {
            let lines = group_card("tools", 12, description, theme.tag, 40, &theme);
            assert_eq!(usize::from(card_height(description)), lines.len() + 2);
            assert!(lines[0].to_string().ends_with("12 skills"));
            assert_eq!(lines[0].spans.last().unwrap().style, theme.skill_count());
        }
    }
}
