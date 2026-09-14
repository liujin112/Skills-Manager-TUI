//! One module per tab. Views keep their own cursor and layout rects; the
//! data always comes from `Ctx`.

pub mod agents;
pub(crate) mod completion;
pub(crate) mod filter;
pub mod health;
pub mod matrix;
pub mod presets;
pub mod preview;
pub mod repos;
pub mod search;
pub mod tags;

use super::app::{Action, Ctx, Hints};
use crossterm::event::{KeyEvent, MouseEvent};
use ratatui::Frame;
use ratatui::layout::Rect;

pub trait View {
    fn status(&self, _ctx: &Ctx) -> String {
        String::new()
    }
    /// Called after every new snapshot.
    fn refresh(&mut self, ctx: &Ctx);
    /// Called when the tab becomes the active one after being away, before
    /// any key reaches it. A view that keeps a focus of its own can put it
    /// back where someone returning expects to find it; most have nothing to
    /// reset, so the default does nothing.
    fn enter(&mut self) {}
    fn handle_key(&mut self, k: KeyEvent, ctx: &Ctx) -> Vec<Action>;
    fn handle_mouse(&mut self, m: MouseEvent, ctx: &Ctx) -> Vec<Action>;
    fn draw(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx);
    fn hints(&self) -> Hints;
}

/// Mouse wheel delta for list navigation.
pub fn wheel(m: &MouseEvent, ctx: &Ctx) -> Option<i32> {
    use crossterm::event::MouseEventKind::*;
    match m.kind {
        ScrollUp => Some(-ctx.settings.interaction.wheel_rows),
        ScrollDown => Some(ctx.settings.interaction.wheel_rows),
        _ => None,
    }
}

#[cfg(test)]
mod navigation_tests;

#[cfg(test)]
mod presentation_tests;
