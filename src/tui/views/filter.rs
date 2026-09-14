//! A local filter for lists of tags, presets, repositories, or issues.
use crate::tui::app::{Action, Ctx};
use crate::tui::widgets::Input;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{Frame, layout::Rect};

#[derive(Default)]
pub struct Filter {
    pub input: Input,
    pub editing: bool,
    pub rect: Rect,
}

impl Filter {
    pub fn matches(&self, text: &str) -> bool {
        let text = text.to_lowercase();
        self.input.value().split_whitespace().all(|word| {
            let word = word.to_lowercase();
            let mut chars = text.chars();
            word.chars()
                .all(|c| chars.by_ref().any(|candidate| candidate == c))
        })
    }

    pub fn key(&mut self, key: KeyEvent) -> bool {
        if key.code == KeyCode::Esc && !self.input.is_empty() {
            self.input = Input::default();
            self.editing = false;
            return true;
        }
        if !self.editing {
            if key.code == KeyCode::Char('/') {
                self.editing = true;
                return true;
            }
            return false;
        }
        match key.code {
            KeyCode::Esc | KeyCode::Enter | KeyCode::Down => self.editing = false,
            _ => {
                self.input.handle_key(key);
            }
        }
        true
    }

    pub fn paste(&mut self, text: &str) -> Vec<Action> {
        match self.input.paste(text) {
            Err(error) => vec![Action::Error(error.into())],
            _ => vec![],
        }
    }

    pub fn draw(&mut self, f: &mut Frame, area: Rect, label: &str, ctx: &Ctx) -> Rect {
        let height = area.height.min(3);
        self.rect = Rect::new(area.x, area.y, area.width, height);
        let block = ctx.theme.block(
            format!(
                " {label}{} ",
                if self.input.is_empty() {
                    ""
                } else {
                    " · Esc clear"
                }
            ),
            self.editing,
        );
        let inner = block.inner(self.rect);
        f.render_widget(block, self.rect);
        f.render_widget(
            ratatui::widgets::Paragraph::new(ratatui::text::Span::styled("› ", ctx.theme.accent())),
            Rect::new(
                inner.x + u16::from(inner.width > 0),
                inner.y,
                inner.width.saturating_sub(1).min(2),
                inner.height,
            ),
        );
        self.input.render_hint(
            f,
            Rect::new(
                inner.x + inner.width.min(3),
                inner.y,
                inner.width.saturating_sub(4),
                inner.height,
            ),
            self.editing,
            ("filter…", " · Enter results"),
            ctx.theme,
        );
        Rect::new(area.x, area.y + height, area.width, area.height - height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    #[test]
    fn escape_clears_filter_from_input_or_results_before_leaving_panel() {
        for editing in [false, true] {
            let mut filter = Filter {
                input: Input::with_value("missing"),
                editing,
                ..Default::default()
            };
            let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
            assert!(filter.key(esc));
            assert!(filter.input.is_empty());
            assert!(!filter.editing);
            assert!(!filter.key(esc));
        }
    }
}
