//! Presets tab: what each preset holds, and adding to or taking from it.
//!
//! Turning a preset on or off is the Agents page's job, one agent at a time.
//! This page is where a preset is defined: which skills belong to it. The
//! cards summarize each preset's purpose and members; deployment lives on Agents.

use super::matrix::Matrix;
use super::{View, wheel};
use crate::tui::app::{Action, Ctx, Hints, Tab};
use crate::tui::components::group;
use crate::tui::components::group_prompt::Prompt;
use crate::tui::components::layout::frame;
use crate::tui::components::layout::split_panes;
use crate::tui::modal::Modal;
use crate::tui::settings::LayoutScope;
use crate::tui::widgets::{CardGrid, ScrollTrack};
use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use skills::history;
use skills::preset::Preset;

#[derive(Default)]
pub struct PresetsView {
    presets: Vec<Preset>,
    tag_groups: Vec<(String, Vec<String>)>,
    tag_cursor: usize,
    tag_offset: usize,
    tag_rects: Vec<(usize, Rect)>,
    focus_tags: bool,
    visible_members: Vec<String>,
    tags_enabled: bool,

    all_presets: Vec<Preset>,
    filter: super::filter::Filter,
    skill_search: Option<(String, super::search::SearchView)>,
    list: CardGrid,
    focus_members: bool,
    left: Rect,
    right: Rect,
    list_track: ScrollTrack,
    list_drag: bool,
    /// The whole preset × agent picture, over the page.
    matrix: Matrix,
    /// A preset to land on when the list next reloads, by name, because the
    /// list is sorted and a preset just created or renamed can appear
    /// anywhere in it.
    pending: Option<String>,
    color_prompt: Option<Prompt>,
}

impl PresetsView {
    fn finish_color(&mut self, result: Option<bool>) -> Vec<Action> {
        match result {
            Some(false) => self.color_prompt = None,
            Some(true) => {
                let Some(prompt) = self.color_prompt.as_ref() else {
                    return vec![];
                };
                let Some(color) = prompt.color_text() else {
                    return vec![Action::Error("Use a colour name or #rrggbb".into())];
                };
                let name = prompt.name().to_string();
                self.color_prompt = None;
                return vec![Action::Write(Box::new(move |ws| {
                    let mut preset = ws
                        .presets
                        .load(&name)?
                        .ok_or_else(|| anyhow::anyhow!("no such preset: {name}"))?;
                    preset.color = color;
                    ws.presets.save(&preset)?;
                    Ok(format!("Updated colour of {name}"))
                }))];
            }
            None => {}
        }
        vec![]
    }

    pub fn batch_finished(&mut self, failed: &[String]) {
        if let Some((_, view)) = self.skill_search.as_mut() {
            view.batch_finished(failed);
        }
    }

    pub fn input_focused(&self) -> bool {
        self.color_prompt.is_some()
            || self.filter.editing
            || (self.focus_members
                && !self.focus_tags
                && self
                    .skill_search
                    .as_ref()
                    .is_some_and(|(_, v)| v.input_focused()))
    }
    pub fn paste(&mut self, text: &str, ctx: &Ctx) -> Vec<Action> {
        let snap = self.member_snapshot(ctx);
        let ctx = &Ctx {
            snap: &snap,
            ..*ctx
        };
        if let Some(prompt) = self.color_prompt.as_mut() {
            return prompt.paste(text);
        }
        if self.filter.editing {
            let actions = self.filter.paste(text);
            self.refilter();
            return actions;
        }
        if self.focus_members
            && !self.focus_tags
            && let Some((_, view)) = self.skill_search.as_mut()
        {
            return view.paste(text, ctx);
        }
        vec![]
    }
    fn refilter(&mut self) {
        let selected = self.selected().map(|p| p.name.clone());
        self.presets = self
            .all_presets
            .iter()
            .filter(|p| {
                self.filter.matches(&format!(
                    "{} {}",
                    p.name,
                    p.description.as_deref().unwrap_or("")
                ))
            })
            .cloned()
            .collect();
        self.list
            .select(selected.and_then(|name| self.presets.iter().position(|p| p.name == name)));
        self.list.clamp(self.presets.len());
        if self
            .skill_search
            .as_ref()
            .is_some_and(|(name, _)| self.selected().is_none_or(|p| &p.name != name))
        {
            self.skill_search = None;
        }
    }

    fn selected(&self) -> Option<&Preset> {
        self.list.selected().and_then(|i| self.presets.get(i))
    }

    /// Land on `name` once it shows up in the list.
    pub fn select(&mut self, name: &str) {
        self.pending = Some(name.to_string());
    }

    /// Prompts that need a preset under the cursor, or say what to do
    /// instead.
    fn with_selected(&self, open: impl FnOnce(&Preset) -> Modal) -> Vec<Action> {
        match self.selected() {
            Some(p) => vec![Action::OpenModal(Box::new(open(p)))],
            None => vec![Action::Error(
                "no preset selected; press c to create one".into(),
            )],
        }
    }

    // Presets can retain keys whose directories and metadata have been removed.
    // Keep those members searchable and removable in the shared skill panel.
    fn member_snapshot<'a>(
        &self,
        ctx: &Ctx<'a>,
    ) -> std::borrow::Cow<'a, skills::reconcile::Snapshot> {
        use skills::reconcile::{SkillRecord, SkillStatus};
        let mut snap = std::borrow::Cow::Borrowed(ctx.snap);
        for key in self.all_presets.iter().flat_map(|p| &p.skills) {
            if snap.get(key).is_none() {
                snap.to_mut().skills.push(SkillRecord {
                    key: key.clone(),
                    path: ctx.ws.root.join(key),
                    status: SkillStatus::Missing,
                    name: None,
                    description: None,
                    body: None,
                    external: false,
                    name_mismatch: false,
                    tags: vec![],
                    note: None,
                    source: None,
                    current_hash: None,
                    baseline_hash: None,
                    deploy: Default::default(),
                    meta: None,
                });
            }
        }
        snap
    }

    fn ensure_skill_search(&mut self, ctx: &Ctx) {
        let name = self.selected().map(|p| p.name.clone()).unwrap_or_default();
        if self
            .skill_search
            .as_ref()
            .is_none_or(|(current, _)| current != &name)
        {
            let mut view = super::search::SearchView::panel(
                self.visible_members.clone(),
                format!("Preset: {name}"),
                LayoutScope::Presets,
                ctx,
            );
            view.focus_list();
            self.skill_search = Some((name, view));
        }
    }

    fn member_count(&self) -> usize {
        self.visible_members.len()
    }

    fn refresh_groups(&mut self, ctx: &Ctx) {
        self.tags_enabled = ctx.settings.tags_enabled;
        let selected = self.tag_groups.get(self.tag_cursor).map(|g| g.0.clone());
        self.tag_groups.clear();
        if self.tags_enabled {
            let mut names: std::collections::BTreeSet<String> =
                ctx.ws.config.tags.iter().map(|t| t.name.clone()).collect();
            names.extend(ctx.snap.all_tags().into_keys());
            for name in names {
                let keys = ctx
                    .snap
                    .skills
                    .iter()
                    .filter(|r| r.status.is_present() && r.tags.contains(&name))
                    .map(|r| r.key.clone())
                    .collect();
                self.tag_groups.push((name, keys));
            }
        }
        self.tags_enabled = !self.tag_groups.is_empty();
        if !self.tags_enabled {
            self.focus_tags = false;
        }
        self.tag_cursor = selected
            .and_then(|name| self.tag_groups.iter().position(|g| g.0 == name))
            .unwrap_or(0);
        self.visible_members = self
            .selected()
            .map(|p| p.skills.clone())
            .unwrap_or_default();
    }

    fn edit_members(&self, keys: Vec<String>, add: bool) -> Vec<Action> {
        let Some(p) = self.selected() else {
            return vec![];
        };
        let name = p.name.clone();
        vec![Action::WriteMeta(Box::new(move |ws| {
            history::preset_edit(ws, &name, |members| {
                if add {
                    for key in &keys {
                        if !members.contains(key) {
                            members.push(key.clone());
                        }
                    }
                } else {
                    members.retain(|key| !keys.contains(key));
                }
            })
        }))]
    }

    fn toggle_members(&self, keys: Vec<String>) -> Vec<Action> {
        let add = self
            .selected()
            .is_some_and(|p| keys.iter().any(|key| !p.skills.contains(key)));
        self.edit_members(keys, add)
    }

    fn draw_tags(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) -> Rect {
        self.tag_rects.clear();
        if !self.tags_enabled || area.height < 3 || self.selected().is_none() {
            return area;
        }
        let budget = area.width.saturating_sub(4) as usize;
        let members = self
            .selected()
            .map(|p| p.skills.clone())
            .unwrap_or_default();
        let rendered: Vec<_> = self
            .tag_groups
            .iter()
            .enumerate()
            .map(|(i, (name, keys))| {
                let count = keys.iter().filter(|key| members.contains(key)).count();
                group::Pill {
                    coverage: Some((count, keys.len())),
                    selected: i == self.tag_cursor,
                    focused: self.focus_tags,
                    ..group::Pill::new(name, group::tag_fill(name, ctx))
                }
                .render(ctx, budget)
            })
            .collect();
        let widths: Vec<_> = rendered
            .iter()
            .map(|p| p.iter().map(Span::width).sum::<usize>() + 1)
            .collect();
        let visible =
            super::agents::pill_window(&widths, self.tag_cursor, &mut self.tag_offset, budget);
        let mut spans = vec![Span::styled(
            if visible.start > 0 { "‹ " } else { "  " },
            ctx.settings.theme.dim(),
        )];
        let mut x = area.x + 2;
        for i in visible.clone() {
            spans.extend(rendered[i].clone());
            spans.push(Span::raw(" "));
            self.tag_rects
                .push((i, Rect::new(x, area.y + 1, (widths[i] - 1) as u16, 1)));
            x += widths[i] as u16;
        }
        spans.push(Span::styled(
            if visible.end < self.tag_groups.len() {
                "›"
            } else {
                " "
            },
            ctx.settings.theme.dim(),
        ));
        f.render_widget(
            Paragraph::new(Line::from(spans)),
            Rect {
                y: area.y + 1,
                height: 1,
                ..area
            },
        );
        Rect {
            y: area.y + 3,
            height: area.height - 3,
            ..area
        }
    }

    fn add_members(&self, ctx: &Ctx) -> Vec<Action> {
        // Membership is edited by picking from the library, never by typing
        // a name from memory.
        self.with_selected(|p| Modal::preset_members(&p.name, ctx))
    }

    /// Tags and presets share their identity, count and description layout.
    fn preset_card(&self, p: &Preset, ctx: &Ctx, inner_w: usize) -> Vec<Line<'static>> {
        group::group_card(
            &p.name,
            p.skills.len(),
            p.description.as_deref(),
            group::preset_fill(p, ctx),
            inner_w,
            &ctx.settings.theme,
        )
    }

    fn draw_presets(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let th = &ctx.settings.theme;
        let block = th.block(" presets ", !self.focus_members && !self.filter.editing);
        let inner = block.inner(area);
        f.render_widget(block, area);
        let content = Rect {
            width: inner.width.saturating_sub(1),
            ..inner
        };
        let heights = self
            .presets
            .iter()
            .map(|preset| group::card_height(preset.description.as_deref()))
            .collect();
        self.list.layout_heights(content, heights);
        if self.presets.is_empty() {
            f.render_widget(
                Paragraph::new(Span::styled(
                    if self.all_presets.is_empty() {
                        "no presets yet — press c to create one"
                    } else {
                        "No matching presets · / to edit · Esc to clear"
                    },
                    th.dim(),
                )),
                Rect {
                    height: 1,
                    ..content
                },
            );
            self.list_track.clear();
            return;
        }
        let selected = self.list.selected();
        for i in self.list.visible() {
            let Some(cell) = self.list.cell(i) else {
                continue;
            };
            let on = selected == Some(i);
            let ci = if cell.height < 3 {
                cell
            } else {
                frame(f, cell, on, !self.focus_members && !self.filter.editing, th)
            };
            let lines = self.preset_card(&self.presets[i], ctx, ci.width as usize);
            f.render_widget(
                Paragraph::new(lines).style(if cell.height < 3 && on {
                    th.selected()
                } else {
                    ratatui::style::Style::default()
                }),
                ci,
            );
        }
        group::draw_track(f, inner, &self.list, selected, &mut self.list_track, th);
    }
}

impl View for PresetsView {
    fn status(&self, ctx: &Ctx) -> String {
        let snap = self.member_snapshot(ctx);
        let ctx = &Ctx {
            snap: &snap,
            ..*ctx
        };
        self.skill_search
            .as_ref()
            .map_or_else(String::new, |(_, v)| v.status(ctx))
    }
    fn refresh(&mut self, ctx: &Ctx) {
        // The list is sorted by name, so a preset keeps its place only by
        // name: one created or deleted above the cursor would otherwise move
        // the selection onto a neighbour.
        let keep = self
            .pending
            .clone()
            .or_else(|| self.selected().map(|p| p.name.clone()));
        self.all_presets = ctx.ws.presets.list().unwrap_or_default();
        let snap = self.member_snapshot(ctx);
        let ctx = &Ctx {
            snap: &snap,
            ..*ctx
        };
        self.refilter();
        if let Some(i) = keep.and_then(|k| self.presets.iter().position(|p| p.name == k)) {
            self.list.select(Some(i));
            self.pending = None;
        }
        self.list.clamp(self.presets.len());
        self.refresh_groups(ctx);
        if let Some((_, view)) = self.skill_search.as_mut() {
            let keys = self.visible_members.clone();
            view.update_panel(keys, ctx);
        }
    }

    fn handle_key(&mut self, k: KeyEvent, ctx: &Ctx) -> Vec<Action> {
        let snap = self.member_snapshot(ctx);
        let ctx = &Ctx {
            snap: &snap,
            ..*ctx
        };
        if let Some(prompt) = self.color_prompt.as_mut() {
            let result = prompt.key(k);
            return self.finish_color(result);
        }
        if let Some(actions) = self.matrix.handle_key(k, ctx) {
            return actions;
        }
        self.refresh_groups(ctx);
        self.ensure_skill_search(ctx);
        if self.focus_members && self.focus_tags {
            match k.code {
                KeyCode::Left | KeyCode::Char('h') if self.tag_cursor > 0 => self.tag_cursor -= 1,
                KeyCode::Right | KeyCode::Char('l') => {
                    self.tag_cursor = (self.tag_cursor + 1).min(self.tag_groups.len() - 1)
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.focus_tags = false;
                    self.skill_search.as_mut().unwrap().1.focus_list();
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.focus_tags = false;
                    self.skill_search.as_mut().unwrap().1.focus_input();
                }
                KeyCode::Esc | KeyCode::Left => {
                    self.focus_members = false;
                    self.focus_tags = false;
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    return self.toggle_members(self.tag_groups[self.tag_cursor].1.clone());
                }
                KeyCode::Char('a') => {
                    return self.edit_members(self.tag_groups[self.tag_cursor].1.clone(), true);
                }
                KeyCode::Char('x') | KeyCode::Delete => {
                    return self.edit_members(self.tag_groups[self.tag_cursor].1.clone(), false);
                }
                KeyCode::Char('/') => {
                    self.focus_tags = false;
                    self.skill_search.as_mut().unwrap().1.focus_input();
                }
                KeyCode::Char('m') => {
                    self.focus_tags = false;
                    let view = &mut self.skill_search.as_mut().unwrap().1;
                    view.focus_list();
                    return view.handle_key(k, ctx);
                }
                _ => return vec![],
            }
            return vec![];
        }
        if self.focus_members
            && self
                .skill_search
                .as_ref()
                .is_some_and(|(_, view)| view.panel_at_top())
            && k.code == KeyCode::Up
            && self.tags_enabled
        {
            self.focus_tags = true;
            return vec![];
        }

        if !self.focus_members && self.filter.key(k) {
            self.refilter();
            return vec![];
        }
        if self.focus_members
            && let Some((name, view)) = self.skill_search.as_mut()
        {
            if k.code == KeyCode::Left && view.panel_back() {
                self.focus_members = false;
                return vec![];
            }
            if view.panel_actions_ready() {
                if matches!(k.code, KeyCode::Char('x') | KeyCode::Delete) && k.modifiers.is_empty()
                {
                    let name = name.clone();
                    let keys = view.panel_keys(ctx);
                    return vec![Action::WriteMeta(Box::new(move |ws| {
                        history::preset_edit(ws, &name, |members| {
                            members.retain(|key| !keys.contains(key))
                        })
                    }))];
                }
                if k.code == KeyCode::Char('a') && k.modifiers.is_empty() {
                    let keys = view.panel_keys(ctx);
                    return self.edit_members(keys, true);
                }
            }
            let editing = view.input_focused();
            let actions = view.handle_key(k, ctx);
            if editing
                && !view.input_focused()
                && matches!(k.code, KeyCode::Down | KeyCode::Enter)
                && self.tags_enabled
            {
                self.focus_tags = true;
            }
            return actions;
        }
        if k.code == KeyCode::Char('M') {
            self.matrix.open(ctx);
            return vec![];
        }
        let n = self.presets.len();
        let m = self.member_count();
        match k.code {
            KeyCode::Char('q') => vec![Action::SwitchTab(Tab::Search)],
            // Esc means "back" everywhere else in the program, so here it goes
            // back to the search page rather than out of the door.
            KeyCode::Esc => vec![Action::SwitchTab(Tab::Search)],
            KeyCode::Down | KeyCode::Char('j') => {
                self.skill_search = None;
                self.list.move_by(1, n);
                vec![]
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.list.selected().unwrap_or(0) == 0 {
                    self.filter.editing = true;
                    return vec![];
                }
                self.skill_search = None;
                self.list.move_by(-1, n);
                vec![]
            }
            KeyCode::Home | KeyCode::Char('g') => {
                self.skill_search = None;
                self.list.first(n);
                vec![]
            }
            KeyCode::End | KeyCode::Char('G') => {
                self.skill_search = None;
                self.list.last(n);
                vec![]
            }
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
                if m > 0 || self.tags_enabled {
                    self.focus_members = true;
                    self.focus_tags = self.tags_enabled;
                }
                vec![]
            }
            KeyCode::Char('c') => vec![Action::OpenModal(Box::new(Modal::new_preset()))],
            KeyCode::Char('C') => {
                if let Some(p) = self.selected() {
                    self.color_prompt = Some(Prompt::for_color(
                        &p.name,
                        p.color.as_deref(),
                        &ctx.settings.theme,
                    ));
                }
                vec![]
            }
            KeyCode::Char('a') => self.add_members(ctx),
            KeyCode::Char('e') => {
                self.with_selected(|p| Modal::preset_description(&p.name, p.description.as_deref()))
            }
            KeyCode::Char('r') => self.with_selected(|p| Modal::rename_preset(&p.name)),
            // Deleting a whole preset is the one destructive key here, and it
            // is the capital so a slip on `x` in the member list cannot reach it.
            KeyCode::Char('D') => match self.selected() {
                Some(p) => vec![Action::OpenModal(Box::new(Modal::delete_preset(&p.name)))],
                None => vec![],
            },
            _ => vec![],
        }
    }

    fn handle_mouse(&mut self, m: MouseEvent, ctx: &Ctx) -> Vec<Action> {
        let snap = self.member_snapshot(ctx);
        let ctx = &Ctx {
            snap: &snap,
            ..*ctx
        };
        if let Some(prompt) = self.color_prompt.as_mut() {
            let result = prompt.mouse(m);
            return self.finish_color(result);
        }
        if let Some(actions) = self.matrix.handle_mouse(m, ctx) {
            return actions;
        }
        self.refresh_groups(ctx);
        self.ensure_skill_search(ctx);
        let at = (m.column, m.row).into();
        let pressing = m.kind == MouseEventKind::Down(MouseButton::Left);
        let dragging = m.kind == MouseEventKind::Drag(MouseButton::Left);
        if pressing
            && let Some((index, _)) = self.tag_rects.iter().find(|(_, rect)| rect.contains(at))
        {
            self.tag_cursor = *index;
            self.focus_members = true;
            self.filter.editing = false;
            self.focus_tags = true;
            return vec![];
        }
        if pressing && self.filter.rect.contains(at) {
            self.focus_members = false;
            self.filter.editing = true;
            return vec![];
        }
        if self.right.contains(at) {
            self.focus_members = true;
            self.focus_tags = false;
            self.filter.editing = false;
            return self.skill_search.as_mut().unwrap().1.handle_mouse(m, ctx);
        }
        if let Some(d) = wheel(&m, ctx) {
            if self.left.contains(at) {
                self.list.move_by(d.signum(), self.presets.len());
            }
            return vec![];
        }
        if (pressing && self.list_track.hit(m.column, m.row)) || (dragging && self.list_drag) {
            self.list_drag = true;
            self.focus_members = false;
            if let Some(row) = self.list_track.index_at(m.row, self.list.grid_rows()) {
                self.list.select_row(row);
            }
            return vec![];
        }
        if !dragging {
            self.list_drag = false;
        }
        if pressing && self.left.contains(at) {
            self.focus_members = false;
            self.list.click(m.column, m.row);
        }
        vec![]
    }

    fn draw(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let snap = self.member_snapshot(ctx);
        let ctx = &Ctx {
            snap: &snap,
            ..*ctx
        };
        self.refresh_groups(ctx);
        let (left, right) = split_panes(area, 38, ctx);
        self.left = left;
        self.right = right;
        let content = self.filter.draw(f, left, "Filter presets", ctx);
        self.draw_presets(f, content, ctx);
        self.ensure_skill_search(ctx);
        if let Some((name, mut view)) = self.skill_search.take() {
            view.set_panel_active(self.focus_members && !self.focus_tags);
            view.draw_with_content_header(f, right, ctx, |f, content| {
                self.draw_tags(f, content, ctx)
            });
            self.skill_search = Some((name, view));
        }
        self.matrix.draw(f, area, ctx);
        if let Some(prompt) = self.color_prompt.as_mut() {
            prompt.draw(f, area, ctx);
        }
    }

    fn hints(&self) -> Hints {
        if self.color_prompt.is_some() {
            return &[("↑↓", "colour"), ("Enter", "apply"), ("Esc", "cancel")];
        }
        if self.focus_members && self.focus_tags {
            return &[
                ("←→", "tags"),
                ("↓", "skills"),
                ("Enter/Space", "toggle group"),
                ("a/x", "add/remove group"),
                ("Esc", "presets"),
            ];
        }
        if self.filter.editing {
            return &[("Enter/↓", "presets"), ("Esc", "clear filter")];
        }
        if self.focus_members
            && let Some((_, view)) = self.skill_search.as_ref()
        {
            return view.preset_panel_hints();
        }
        if let Some(hints) = self.matrix.hints() {
            return hints;
        }
        if self.focus_members {
            &[
                ("/", "filter skills"),
                ("a", "add skills"),
                ("x", "remove"),
                ("m", "multi-select"),
                ("Enter", "preview"),
                ("←/Esc", "presets"),
            ]
        } else {
            &[
                ("Enter/→", "members"),
                ("/", "filter presets"),
                ("r", "rename"),
                ("c", "create"),
                ("C", "colour"),
                ("M", "matrix"),
                ("a", "add skills"),
                ("e", "description"),
                ("D", "delete preset"),
                ("Esc/q", "library"),
            ]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::theme::Theme;
    use crossterm::event::KeyModifiers;
    use skills::{Workspace, config::Config};

    #[test]
    fn preset_colour_picker_saves_resets_and_cancels_without_changing_members() {
        let root = skills::ops::DownloadDir::new("preset-colour-test").unwrap();
        Config {
            agents: vec![],
            ..Default::default()
        }
        .save(root.path())
        .unwrap();
        let ws = Workspace::open(root.path()).unwrap();
        // Old presets have no colour field.
        let preset: Preset = toml::from_str("name = 'Office'\nskills = ['document']").unwrap();
        assert_eq!(preset.color, None);
        ws.presets.save(&preset).unwrap();
        let snap = ws.scan().unwrap();
        let theme = Theme::default();
        let ctx = Ctx {
            ws: &ws,
            snap: &snap,
            settings: &{
                let mut settings = crate::tui::settings::RuntimeSettings::new(&ws.config);
                settings.theme = theme;
                settings
            },
        };
        let mut view = PresetsView::default();
        view.refresh(&ctx);
        for (typed, expected) in [("#b87e54", Some("#b87e54")), ("none", None)] {
            view.handle_key(KeyEvent::new(KeyCode::Char('C'), KeyModifiers::SHIFT), &ctx);
            assert!(view.input_focused());
            assert!(view.paste(typed, &ctx).is_empty());
            let mut actions =
                view.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), &ctx);
            let Action::Write(write) = actions.remove(0) else {
                panic!("expected write")
            };
            write(&ws).unwrap();
            let saved = ws.presets.load("Office").unwrap().unwrap();
            assert_eq!(saved.color.as_deref(), expected);
            assert_eq!(saved.skills, preset.skills);
            view.refresh(&ctx);
            let lines = view.preset_card(&saved, &ctx, 40);
            assert_eq!(
                lines[0].spans[0].style.fg,
                Some(expected.map_or(theme.tag, |_| ratatui::style::Color::Rgb(184, 126, 84)))
            );
        }
        view.handle_key(KeyEvent::new(KeyCode::Char('C'), KeyModifiers::SHIFT), &ctx);
        view.paste("invalid-colour", &ctx);
        assert!(matches!(
            view.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE), &ctx)
                .as_slice(),
            [Action::Error(_)]
        ));
        assert!(view.color_prompt.is_some());
        view.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), &ctx);
        assert!(view.color_prompt.is_none());
        assert_eq!(ws.presets.load("Office").unwrap().unwrap(), preset);
    }

    #[test]
    fn preset_cards_show_members_and_fit_unicode_without_agent_status() {
        let root = skills::ops::DownloadDir::new("preset-card-test").unwrap();
        Config {
            agents: vec![],
            ..Default::default()
        }
        .save(root.path())
        .unwrap();
        let path = root.path().join("document");
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(
            path.join("SKILL.md"),
            "---\nname: document-tools\ndescription: Document tools\n---\nBody",
        )
        .unwrap();
        let ws = Workspace::open(root.path()).unwrap();
        let snap = ws.scan().unwrap();
        let theme = Theme::default();
        let ctx = Ctx {
            ws: &ws,
            snap: &snap,
            settings: &{
                let mut settings = crate::tui::settings::RuntimeSettings::new(&ws.config);
                settings.theme = theme;
                settings
            },
        };
        let preset = Preset {
            name: "Office".into(),
            color: Some("#b87e54".into()),
            description: Some("**Document tools** with 中文说明".into()),
            skills: vec!["document".into(), "missing".into()],
            agents: vec!["SampleAgent".into()],
        };
        let view = PresetsView::default();
        let lines = view.preset_card(&preset, &ctx, 65);
        assert_eq!(lines.len(), 2);
        let mut compact = preset.clone();
        compact.description = None;
        let compact_lines = view.preset_card(&compact, &ctx, 65);
        assert_eq!(compact_lines.len(), 1);
        assert!(lines[0].to_string().ends_with("2 skills"));
        assert!(lines[1].to_string().starts_with("Document tools"));
        assert!(
            !lines
                .iter()
                .any(|line| line.to_string().contains("SampleAgent"))
        );
        let mut rendered = PresetsView {
            presets: vec![preset.clone()],
            ..Default::default()
        };
        for height in [3, 4, 5, 6, 8] {
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, height)).unwrap();
            terminal
                .draw(|f| rendered.draw_presets(f, f.area(), &ctx))
                .unwrap();
            let text: String = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|c| c.symbol())
                .collect();
            assert!(
                text.contains("Office"),
                "preset must remain visible at height {height}"
            );
        }
        for width in 0..80 {
            assert!(
                view.preset_card(&preset, &ctx, width)
                    .iter()
                    .all(|line| line.width() <= width)
            );
        }
    }

    #[test]
    fn preview_blocks_member_removal_until_closed() {
        let root = std::env::temp_dir().join(format!(
            "skills-preview-keys-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let config = Config {
            agents: vec![],
            ..Config::default()
        };
        config.save(&root).unwrap();
        let ws = Workspace::open(&root).unwrap();
        ws.presets
            .save(&Preset {
                name: "reading".into(),
                skills: vec!["printer".into()],
                ..Preset::default()
            })
            .unwrap();
        let before = std::fs::read(ws.presets.path("reading")).unwrap();
        let snap = ws.scan().unwrap();
        let theme = Theme::default();
        let ctx = Ctx {
            ws: &ws,
            snap: &snap,
            settings: &{
                let mut settings = crate::tui::settings::RuntimeSettings::new(&ws.config);
                settings.theme = theme;
                settings
            },
        };
        let mut view = PresetsView::default();
        view.refresh(&ctx);
        let key = |code| KeyEvent::new(code, KeyModifiers::NONE);
        assert!(view.handle_key(key(KeyCode::Char('m')), &ctx).is_empty());
        assert!(view.handle_key(key(KeyCode::Right), &ctx).is_empty());
        let actions = view.handle_key(key(KeyCode::Char('m')), &ctx);
        assert!(actions.is_empty());
        assert!(view.skill_search.is_some());
        view.handle_key(key(KeyCode::Esc), &ctx); // Leave multi-select.
        assert!(view.handle_key(key(KeyCode::Enter), &ctx).is_empty());
        assert!(!view.skill_search.as_ref().unwrap().1.panel_actions_ready());

        for code in [KeyCode::Char('x'), KeyCode::Delete, KeyCode::Char('a')] {
            assert!(view.handle_key(key(code), &ctx).is_empty());
            assert!(!view.skill_search.as_ref().unwrap().1.panel_actions_ready());
            assert!(view.focus_members);
            assert_eq!(
                view.skill_search.as_ref().unwrap().1.panel_keys(&Ctx {
                    snap: &view.member_snapshot(&ctx),
                    ..ctx
                }),
                vec!["printer"]
            );
            assert_eq!(std::fs::read(ws.presets.path("reading")).unwrap(), before);
        }

        assert!(view.handle_key(key(KeyCode::Esc), &ctx).is_empty());
        assert!(view.skill_search.as_ref().unwrap().1.panel_actions_ready());
        assert!(view.focus_members);
        let mut actions = view.handle_key(key(KeyCode::Char('x')), &ctx);
        assert_eq!(actions.len(), 1);
        let Action::WriteMeta(write) = actions.remove(0) else {
            panic!("member removal should resume after closing the preview");
        };
        write(&ws).unwrap();
        assert!(
            ws.presets
                .load("reading")
                .unwrap()
                .unwrap()
                .skills
                .is_empty()
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod tag_group_tests {
    use super::*;
    use crossterm::event::KeyModifiers;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn tag_pills_show_coverage_edit_members_and_disappear_when_disabled() {
        let temp = skills::ops::DownloadDir::new("preset-tags").unwrap();
        let root = temp.path();
        skills::config::Config {
            agents: vec![],
            ..Default::default()
        }
        .save(root)
        .unwrap();
        for key in ["alpha", "beta", "gamma"] {
            std::fs::create_dir(root.join(key)).unwrap();
            std::fs::write(
                root.join(key).join("SKILL.md"),
                format!("---\nname: {key}\ndescription: example\n---\n"),
            )
            .unwrap();
        }
        let mut ws = skills::Workspace::open(root).unwrap();
        skills::ops::edit::tag_add(&ws, "alpha", &["python".into(), "testing".into()]).unwrap();
        skills::ops::edit::tag_add(&ws, "beta", &["python".into()]).unwrap();
        ws.presets
            .save(&Preset {
                name: "dev".into(),
                skills: vec!["alpha".into()],
                ..Default::default()
            })
            .unwrap();
        ws.config = ws.load_config().unwrap();
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
        let mut view = PresetsView::default();
        view.refresh(&ctx);
        let mut terminal = Terminal::new(TestBackend::new(130, 30)).unwrap();
        terminal.draw(|f| view.draw(f, f.area(), &ctx)).unwrap();
        let screen: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(screen.contains("python 1/2"));
        assert!(screen.contains("testing 1/1"));
        assert!(
            terminal
                .backend()
                .buffer()
                .content
                .iter()
                .any(|cell| cell.symbol() == "/" && cell.bg != ratatui::style::Color::Reset)
        );
        assert!(!screen.contains("Untagged"));
        assert!(!screen.contains("All 1/"));
        assert_eq!(view.tag_groups.len(), 2);
        assert!(!screen.contains("[✓]"));
        assert!(!screen.contains("[ ]"));
        assert_eq!(view.visible_members, ["alpha"]);
        let (index, rect) = view
            .tag_rects
            .iter()
            .find(|(i, _)| view.tag_groups[*i].0 == "python")
            .cloned()
            .unwrap();
        view.handle_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: rect.x,
                row: rect.y,
                modifiers: KeyModifiers::NONE,
            },
            &ctx,
        );
        assert_eq!(view.tag_cursor, index);
        assert_eq!(view.visible_members, ["alpha"]);
        let keys = |code| KeyEvent::new(code, KeyModifiers::NONE);
        let Action::WriteMeta(add) = view.handle_key(keys(KeyCode::Char('a')), &ctx).remove(0)
        else {
            panic!("group edit")
        };
        add(&ws).unwrap();
        assert_eq!(
            ws.presets.load("dev").unwrap().unwrap().skills,
            ["alpha", "beta"]
        );
        assert!(ws.scan().unwrap().get("alpha").unwrap().deploy.is_empty());
        // Removing a tag group leaves a member from another group alone.
        skills::history::preset_edit(&ws, "dev", |members| members.push("gamma".into())).unwrap();
        view.refresh(&ctx);
        let Action::WriteMeta(remove) = view.handle_key(keys(KeyCode::Char('x')), &ctx).remove(0)
        else {
            panic!("group edit")
        };
        remove(&ws).unwrap();
        assert_eq!(ws.presets.load("dev").unwrap().unwrap().skills, ["gamma"]);
        assert_eq!(
            skills::config::Config::load(root)
                .unwrap()
                .skill_tags("alpha"),
            ["python", "testing"]
        );
        view.focus_members = false;
        view.focus_tags = false;
        view.filter.input = crate::tui::widgets::Input::with_value("no-such-preset");
        view.refilter();
        terminal.draw(|f| view.draw(f, f.area(), &ctx)).unwrap();
        assert!(
            view.tag_rects.is_empty(),
            "no coverage without a selected preset"
        );
        assert!(view.handle_key(keys(KeyCode::Esc), &ctx).is_empty());
        terminal.draw(|f| view.draw(f, f.area(), &ctx)).unwrap();
        assert!(
            !view.tag_rects.is_empty(),
            "clearing restores preset coverage"
        );
        skills::config::Config::set_tags_enabled(root, false).unwrap();
        ws.config = ws.load_config().unwrap();
        let snap = ws.scan().unwrap();
        assert!(snap.skills.iter().all(|r| r.tags.is_empty()));
        let ctx = Ctx {
            ws: &ws,
            snap: &snap,
            settings: &{
                let mut settings = crate::tui::settings::RuntimeSettings::new(&ws.config);
                settings.theme = theme;
                settings
            },
        };
        view.refresh(&ctx);
        terminal.draw(|f| view.draw(f, f.area(), &ctx)).unwrap();
        assert!(view.tag_rects.is_empty());
        assert_eq!(view.visible_members, ["gamma"]);
        assert!(!Tab::visible(false).contains(&Tab::Tags));
        assert!(Tab::visible(true).contains(&Tab::Tags));
        assert_eq!(
            skills::config::Config::load(root)
                .unwrap()
                .skill_tags("alpha"),
            ["python", "testing"]
        );
        assert!(!root.join(".skills-meta/local.toml").exists());
        skills::config::Config::edit_tags(root, |tags| tags.clear()).unwrap();
        skills::config::Config::set_tags_enabled(root, true).unwrap();
        ws.config = ws.load_config().unwrap();
        let snap = ws.scan().unwrap();
        let ctx = Ctx {
            ws: &ws,
            snap: &snap,
            settings: &{
                let mut settings = crate::tui::settings::RuntimeSettings::new(&ws.config);
                settings.theme = theme;
                settings
            },
        };
        view.refresh(&ctx);
        terminal.draw(|f| view.draw(f, f.area(), &ctx)).unwrap();
        assert!(view.tag_rects.is_empty());
        assert_eq!(view.visible_members, ["gamma"]);
        view.handle_key(keys(KeyCode::Right), &ctx);
        assert!(!view.focus_tags);
    }
}
