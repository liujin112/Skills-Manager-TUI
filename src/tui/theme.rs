//! Colors and block styles used by every view.

use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, BorderType, Borders};

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub accent: Color,
    pub dim: Color,
    pub placeholder: Color,
    pub usage: Color,
    pub source: Color,
    pub skill_count: Color,
    pub selection_bg: Color,
    pub selection_fg: Color,
    pub ok: Color,
    pub warn: Color,
    pub err: Color,
    pub tag: Color,
    pub border: Color,
    pub border_focus: Color,
    /// Search matches, painted like a highlighter pen.
    pub match_bg: Color,
    pub match_fg: Color,
    /// Stable RGB fills for ANSI-named pills, independent of terminal remapping.
    pub pill_palette: [(u8, u8, u8); 16],
    pub color_choices: [&'static str; 16],
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            accent: Color::Cyan,
            dim: Color::Reset,
            placeholder: Color::Rgb(140, 140, 140),
            usage: Color::Rgb(184, 126, 84),
            source: Color::Rgb(155, 137, 172),
            skill_count: Color::Rgb(155, 137, 172),
            selection_bg: Color::Rgb(44, 50, 60),
            selection_fg: Color::Rgb(240, 242, 246),
            ok: Color::Green,
            warn: Color::Yellow,
            err: Color::Red,
            tag: Color::Rgb(140, 140, 140),
            border: Color::DarkGray,
            border_focus: Color::Cyan,
            match_bg: Color::Rgb(255, 214, 79),
            match_fg: Color::Rgb(24, 24, 24),
            pill_palette: [
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
            ],
            color_choices: [
                "red",
                "green",
                "yellow",
                "blue",
                "magenta",
                "cyan",
                "white",
                "gray",
                "darkgray",
                "lightred",
                "lightgreen",
                "lightyellow",
                "lightblue",
                "lightmagenta",
                "lightcyan",
                "black",
            ],
        }
    }
}

impl Theme {
    pub fn skill_count(&self) -> Style {
        Style::default().fg(self.skill_count)
    }
    pub fn block<'a>(&self, title: impl Into<ratatui::text::Line<'a>>, focused: bool) -> Block<'a> {
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(if focused {
                self.border_focus
            } else {
                self.border
            }))
            .title_style(if focused { self.accent() } else { self.dim() })
            .title(title)
    }

    pub fn dim(&self) -> Style {
        // Inherit the terminal foreground, or the selected row's foreground.
        if self.dim == Color::Reset {
            Style::default()
        } else {
            Style::default().fg(self.dim)
        }
    }
    pub fn accent(&self) -> Style {
        Style::default().fg(self.accent)
    }
    pub fn bold(&self) -> Style {
        Style::default().add_modifier(Modifier::BOLD)
    }
    /// Descriptions share the terminal foreground with reduced emphasis.
    pub fn description(&self) -> Style {
        self.dim().add_modifier(Modifier::DIM)
    }
    pub fn source(&self) -> Style {
        Style::default().fg(self.source)
    }
    pub fn selected(&self) -> Style {
        Style::default()
            .bg(self.selection_bg)
            .fg(self.selection_fg)
            .add_modifier(Modifier::BOLD)
    }
    pub fn selected_unfocused(&self) -> Style {
        Style::default().bg(self.selection_bg).fg(self.selection_fg)
    }
    pub fn ok(&self) -> Style {
        Style::default().fg(self.ok)
    }
    pub fn warn(&self) -> Style {
        Style::default().fg(self.warn)
    }
    pub fn err(&self) -> Style {
        Style::default().fg(self.err)
    }
    /// Marker-pen styling for a search match. Both colors are set so the run
    /// stays readable over a selected row or dimmed excerpt text.
    pub fn match_hit(&self) -> Style {
        Style::default().bg(self.match_bg).fg(self.match_fg)
    }
    pub fn key_hint(&self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{
        Terminal,
        backend::TestBackend,
        text::{Line, Span},
        widgets::Paragraph,
    };

    #[test]
    fn secondary_text_inherits_light_dark_and_selected_foregrounds() {
        let theme = Theme::default();
        for (fg, bg) in [(Color::Black, Color::White), (Color::White, Color::Black)] {
            let mut terminal = Terminal::new(TestBackend::new(8, 2)).unwrap();
            terminal
                .draw(|frame| {
                    frame.render_widget(
                        Paragraph::new("").style(Style::default().fg(fg).bg(bg)),
                        frame.area(),
                    );
                    frame.render_widget(
                        Paragraph::new(Span::styled("normal", theme.dim())),
                        ratatui::layout::Rect::new(0, 0, 8, 1),
                    );
                    frame.render_widget(
                        Paragraph::new(Line::from(vec![
                            Span::raw("row"),
                            Span::styled(" note", theme.dim()),
                        ]))
                        .style(theme.selected()),
                        ratatui::layout::Rect::new(0, 1, 8, 1),
                    );
                })
                .unwrap();
            let buffer = terminal.backend().buffer();
            assert_eq!(buffer[(0, 0)].fg, fg);
            assert_eq!(buffer[(0, 0)].bg, bg);
            for x in 0..8 {
                assert_eq!(buffer[(x, 1)].fg, Color::Rgb(240, 242, 246));
                assert_eq!(buffer[(x, 1)].bg, theme.selection_bg);
            }
        }
    }
}
