//! Shared group colour and merge chooser, independent of page implementations.

use super::group::{self, tag_fill};
use crate::tui::app::{Action, Ctx};
use crate::tui::theme::Theme;
use crate::tui::widgets::OverlayClear as Clear;
use crate::tui::widgets::{Input, ListNav};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, Paragraph};

/// The row in the colour prompt that takes a tag's colour away again.
const NO_COLOR: &str = "none";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask {
    /// Which tag the selected one is merged into.
    Merge,
    /// What colour the selected tag is filled with.
    Color,
}

/// Shared chooser for group colours and tag merge targets.
/// Pages own the write action; the chooser owns filtering, selection and preview.
pub struct Prompt {
    ask: Ask,
    tag: String,
    input: Input,
    choices: Vec<String>,
    /// Indices into `choices` matching the filter, in display order.
    shown: Vec<usize>,
    list: ListNav,
    rect: Rect,
}

impl Prompt {
    pub fn for_merge(name: &str, choices: Vec<String>) -> Self {
        Self::new(Ask::Merge, name, choices)
    }

    pub fn kind(&self) -> Ask {
        self.ask
    }

    pub fn value(&self) -> &str {
        self.input.value()
    }

    pub fn for_color(name: &str, current: Option<&str>, theme: &Theme) -> Self {
        let mut choices: Vec<String> = theme.color_choices.iter().map(|c| c.to_string()).collect();
        choices.push(NO_COLOR.into());
        if let Some(c) = current
            && !choices.iter().any(|s| s == c)
        {
            choices.insert(0, c.to_string());
        }
        let selected = choices
            .iter()
            .position(|s| s == current.unwrap_or(NO_COLOR));
        let mut prompt = Self::new(Ask::Color, name, choices);
        prompt.list.state.select(selected);
        prompt
    }

    pub fn name(&self) -> &str {
        &self.tag
    }

    pub fn paste(&mut self, text: &str) -> Vec<Action> {
        match self.input.paste(text) {
            Ok(true) => {
                self.refilter();
                vec![]
            }
            Ok(false) => vec![],
            Err(e) => vec![Action::Error(e.into())],
        }
    }

    // Some(true) submits, Some(false) dismisses; None keeps the chooser open.
    pub fn key(&mut self, k: KeyEvent) -> Option<bool> {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        match k.code {
            KeyCode::Esc => return Some(false),
            KeyCode::Enter => return Some(true),
            KeyCode::Down => self.list.move_by(1, self.shown.len()),
            KeyCode::Up => self.list.move_by(-1, self.shown.len()),
            KeyCode::Char('n') if ctrl => self.list.move_by(1, self.shown.len()),
            KeyCode::Char('p') if ctrl => self.list.move_by(-1, self.shown.len()),
            KeyCode::PageUp => self.list.first(self.shown.len()),
            KeyCode::PageDown => self.list.last(self.shown.len()),
            _ => {
                if self.input.handle_key(k) {
                    self.refilter();
                }
            }
        }
        None
    }

    pub fn mouse(&mut self, m: MouseEvent) -> Option<bool> {
        match m.kind {
            MouseEventKind::ScrollDown => self.list.move_by(1, self.shown.len()),
            MouseEventKind::ScrollUp => self.list.move_by(-1, self.shown.len()),
            MouseEventKind::Down(MouseButton::Left)
                if !self.rect.contains((m.column, m.row).into()) =>
            {
                return Some(false);
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some((_, true)) = self.list.click(m.row, self.shown.len()) {
                    return Some(true);
                }
                self.input.click(m.column);
            }
            _ => {}
        }
        None
    }

    pub fn draw(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let th = &ctx.settings.theme;
        let p = self;
        let (title, placeholder) = match p.ask {
            Ask::Merge => (format!(" merge {} into ", p.tag), "type to filter…"),
            Ask::Color => (format!(" colour of {} ", p.tag), "a name, or #rrggbb"),
        };
        // Room for the field, a swatch line, the rows, and the frame.
        let want = p.shown.len() as u16 + 6;
        let w = 56.min(area.width.saturating_sub(2));
        let h = want.max(8).min(area.height.saturating_sub(2));
        let rect = Rect::new(
            area.x + (area.width - w) / 2,
            area.y + (area.height - h) / 2,
            w,
            h,
        );
        p.rect = rect;
        f.render_widget(Clear, rect);
        let block = th.block(title, true);
        let inner = block.inner(rect);
        f.render_widget(block, rect);
        if inner.height < 4 || inner.width < 4 {
            p.list.rows = Rect::default();
            f.render_widget(Paragraph::new("Resize · Esc closes").style(th.dim()), inner);
            return;
        }
        let field = Rect {
            x: inner.x + 1,
            width: inner.width.saturating_sub(2),
            height: 1,
            ..inner
        };
        p.input.render(f, field, true, placeholder, th);
        // Preview with the shared pill renderer, including its contrast rules.
        let swatch = Rect {
            y: inner.y + 1,
            height: 1,
            ..field
        };
        let mut preview = vec![Span::raw("  ")];
        match p.ask {
            Ask::Merge => {
                preview.extend(
                    group::Pill::new(&p.tag, tag_fill(&p.tag, ctx)).render(ctx, usize::MAX),
                );
                preview.push(Span::styled(" → ", th.dim()));
                match p.chosen() {
                    Some(into) => preview.extend(
                        group::Pill::new(into, tag_fill(into, ctx)).render(ctx, usize::MAX),
                    ),
                    None => preview.push(Span::styled("nothing matches", th.dim())),
                }
            }
            Ask::Color => match p.color() {
                Some(Some(c)) => {
                    preview.extend(group::Pill::new(&p.tag, c).render(ctx, usize::MAX))
                }
                Some(None) => {
                    preview.extend(group::Pill::new(&p.tag, th.tag).render(ctx, usize::MAX))
                }
                None => preview.push(Span::styled("not a colour", th.warn())),
            },
        }
        f.render_widget(Paragraph::new(Line::from(preview)), swatch);
        let list_area = Rect {
            y: inner.y + 3,
            height: inner.height.saturating_sub(3),
            ..inner
        };
        p.list.rows = list_area;
        let rows: Vec<ListItem> = p
            .shown
            .iter()
            .map(|&i| {
                let name = &p.choices[i];
                let spans = match p.ask {
                    Ask::Merge => {
                        group::Pill::new(name, tag_fill(name, ctx)).render(ctx, usize::MAX)
                    }
                    Ask::Color if name == NO_COLOR => {
                        vec![Span::styled("none — the default", th.dim())]
                    }
                    Ask::Color => {
                        let fill = group::color(Some(name), th);
                        let mut s = group::Pill::new(&p.tag, fill).render(ctx, usize::MAX);
                        s.push(Span::styled(format!("  {name}"), th.dim()));
                        s
                    }
                };
                ListItem::new(Line::from(spans))
            })
            .collect();
        f.render_stateful_widget(
            List::new(rows)
                .highlight_style(th.selected())
                .highlight_symbol("▸ "),
            list_area,
            &mut p.list.state,
        );
        if p.shown.is_empty() {
            f.render_widget(
                Paragraph::new(Span::styled("nothing matches", th.dim())),
                list_area,
            );
        }
    }

    fn new(ask: Ask, tag: &str, choices: Vec<String>) -> Self {
        let shown: Vec<usize> = (0..choices.len()).collect();
        let mut list = ListNav::default();
        list.clamp(shown.len());
        Self {
            ask,
            tag: tag.to_string(),
            input: Input::default(),
            choices,
            shown,
            list,
            rect: Rect::default(),
        }
    }

    fn refilter(&mut self) {
        let q = self.input.value().trim().to_lowercase();
        self.shown = (0..self.choices.len())
            .filter(|&i| q.is_empty() || self.choices[i].to_lowercase().contains(&q))
            .collect();
        self.list.clamp(self.shown.len());
    }

    pub fn chosen(&self) -> Option<&str> {
        self.list
            .selected()
            .and_then(|i| self.shown.get(i))
            .map(|&i| self.choices[i].as_str())
    }

    /// The colour the prompt would apply now: what was typed when that names
    /// one, else the highlighted row. `Some(None)` is "no colour".
    fn color(&self) -> Option<Option<Color>> {
        let typed = self.input.value().trim();
        if let Ok(c) = typed.parse::<Color>() {
            return Some(Some(c));
        }
        match self.chosen() {
            Some(NO_COLOR) => Some(None),
            Some(name) => name.parse::<Color>().ok().map(Some),
            None => None,
        }
    }

    /// The text written to the config for `color()`, as typed or as listed.
    pub fn color_text(&self) -> Option<Option<String>> {
        let typed = self.input.value().trim();
        if typed.parse::<Color>().is_ok() {
            return Some(Some(typed.to_string()));
        }
        match self.chosen() {
            Some(NO_COLOR) => Some(None),
            Some(name) => Some(Some(name.to_string())),
            None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_picker_starts_at_current_or_default_colour() {
        let default = Prompt::for_color("test", None, &Theme::default());
        assert_eq!(default.color(), Some(None));
        assert_eq!(default.color_text(), Some(None));
        for current in ["blue", "#b87e54"] {
            let prompt = Prompt::for_color("test", Some(current), &Theme::default());
            assert_eq!(prompt.color_text(), Some(Some(current.into())));
        }
    }

    #[test]
    fn colour_and_merge_prompts_survive_tiny_terminal_resize() {
        let tmp = skills::ops::DownloadDir::new("group-resize").unwrap();
        let ws = skills::Workspace::open(tmp.path()).unwrap();
        let snap = ws.scan().unwrap();
        let theme = crate::tui::theme::Theme::default();
        let ctx = Ctx {
            ws: &ws,
            snap: &snap,
            settings: &{
                let mut settings = crate::tui::settings::RuntimeSettings::new(&ws.config);
                settings.theme = theme;
                settings
            },
        };
        for ask in [Ask::Color, Ask::Merge] {
            let mut prompt = Prompt::new(ask, "test", vec!["blue".into()]);
            for (width, height) in [(40, 12), (40, 11), (20, 8), (4, 3), (1, 1), (80, 24)] {
                let mut term =
                    ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height))
                        .unwrap();
                term.draw(|f| prompt.draw(f, f.area(), &ctx)).unwrap();
                assert!(prompt.rect.right() <= width && prompt.rect.bottom() <= height);
            }
        }
    }
}
