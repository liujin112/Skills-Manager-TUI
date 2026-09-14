//! Tags tab: the tags themselves, and a look at what carries each one.
//!
//! Each pane filters its own collection. A tag can be edited directly: renamed, merged into another,
//! deleted, given a colour. The left column uses shared group cards; the right
//! pane is the skills under the selected tag, as the cards the search and
//! presets pages use, so a skill reads the same wherever it turns up.

use super::{View, wheel};
use crate::tui::app::{Action, Ctx, Hints, Tab};
use crate::tui::components::group::{self, tag_fill};
use crate::tui::components::group_prompt::{Ask, Prompt};
use crate::tui::components::layout::frame;
use crate::tui::components::layout::split_panes;
use crate::tui::modal::Modal;
use crate::tui::settings::LayoutScope;
use crate::tui::widgets::{CardGrid, ScrollTrack};
use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;
use skills::config::Config;
use skills::history;
use skills::ops::edit;
use skills::reconcile::Snapshot;

pub const UNTAGGED: &str = "(untagged)";

#[derive(Default)]
pub struct TagsView {
    /// Every tag with how many skills carry it, `(untagged)` last.
    rows: Vec<(String, usize)>,
    all_rows: Vec<(String, usize)>,
    filter: super::filter::Filter,
    skill_search: Option<super::search::SearchView>,
    /// Skills under the selected tag, in snapshot order.
    members: Vec<String>,
    members_tag: Option<String>,
    list: CardGrid,
    focus_grid: bool,
    left: Rect,
    right: Rect,
    list_track: ScrollTrack,
    list_drag: bool,
    /// The merge target or colour being chosen, over the page.
    prompt: Option<Prompt>,
}

impl TagsView {
    /// Route pasted text to the same control that currently owns keyboard input.
    pub fn paste(&mut self, text: &str, ctx: &Ctx) -> Vec<Action> {
        if self.filter.editing {
            let actions = self.filter.paste(text);
            self.refilter(ctx);
            return actions;
        }
        if self.focus_grid
            && let Some(view) = self.skill_search.as_mut()
        {
            return view.paste(text, ctx);
        }
        let Some(prompt) = self.prompt.as_mut() else {
            return vec![];
        };
        prompt.paste(text)
    }

    pub fn input_focused(&self) -> bool {
        self.prompt.is_some()
            || self.filter.editing
            || (self.focus_grid
                && self
                    .skill_search
                    .as_ref()
                    .is_some_and(|v| v.input_focused()))
    }

    pub fn batch_finished(&mut self, failed: &[String]) {
        if let Some(view) = self.skill_search.as_mut() {
            view.batch_finished(failed);
        }
    }

    pub fn dialog_open(&self) -> bool {
        self.prompt.is_some()
    }

    fn refilter(&mut self, ctx: &Ctx) {
        let selected = self.selected_tag().map(str::to_owned);
        self.rows = self
            .all_rows
            .iter()
            .filter(|(tag, _)| self.filter.matches(tag))
            .cloned()
            .collect();
        self.list
            .select(selected.and_then(|tag| self.rows.iter().position(|r| r.0 == tag)));
        self.list.clamp(self.rows.len());
        self.sync_members(ctx.snap);
    }

    pub fn select(&mut self, name: &str, snap: &Snapshot) {
        self.filter = super::filter::Filter::default();
        self.rows = self.all_rows.clone();
        self.list
            .select(self.rows.iter().position(|(tag, _)| tag == name));
        self.skill_search = None;
        self.focus_grid = false;
        self.sync_members(snap);
    }

    fn add_members(&self, ctx: &Ctx) -> Vec<Action> {
        match self.actionable_tag() {
            Some(tag) => vec![Action::OpenModal(Box::new(Modal::PresetSkills(Box::new(
                super::search::SearchView::tag_members(&tag, ctx),
            ))))],
            None => vec![],
        }
    }

    fn remove_members(&self, keys: Vec<String>) -> Vec<Action> {
        let Some(tag) = self.actionable_tag() else {
            return vec![];
        };
        vec![Action::WriteMeta(Box::new(move |ws| {
            history::tag_edit(ws, |ws| {
                Config::edit_tags(&ws.root, |tags| {
                    if let Some(group) = tags.iter_mut().find(|t| t.name == tag) {
                        group.skills.retain(|key| !keys.contains(key));
                    }
                })?;
                Ok(format!("removed {} skill(s) from {tag}", keys.len()))
            })
        }))]
    }

    fn ensure_skill_search(&mut self, ctx: &Ctx) {
        if self.skill_search.is_none() {
            let focused = self.focus_grid;
            self.search_members(ctx);
            self.skill_search.as_mut().unwrap().focus_list();
            self.focus_grid = focused;
        }
    }

    fn search_members(&mut self, ctx: &Ctx) {
        self.skill_search = Some(super::search::SearchView::panel(
            self.members.clone(),
            format!("Tag: {}", self.selected_tag().unwrap_or("none")),
            LayoutScope::Tags,
            ctx,
        ));
        self.skill_search.as_mut().unwrap().hide_tags = true;
        self.focus_grid = true;
    }

    fn selected_tag(&self) -> Option<&str> {
        self.list
            .selected()
            .and_then(|i| self.rows.get(i))
            .map(|(t, _)| t.as_str())
    }

    /// A real tag under the cursor, which the untagged row is not.
    fn actionable_tag(&self) -> Option<String> {
        self.selected_tag()
            .filter(|t| *t != UNTAGGED)
            .map(str::to_string)
    }

    /// Recompute the right pane for the tag under the cursor. Called after
    /// every move of the left one, so the two never disagree.
    fn sync_members(&mut self, snap: &Snapshot) {
        let old_members = self.members.clone();
        let tag = self.selected_tag().map(str::to_string);
        self.members = match tag.as_deref() {
            Some(UNTAGGED) => snap
                .skills
                .iter()
                .filter(|s| s.tags.is_empty() && s.status.is_present())
                .map(|s| s.key.clone())
                .collect(),
            Some(t) => snap
                .skills
                .iter()
                .filter(|s| s.tags.iter().any(|x| x == t))
                .map(|s| s.key.clone())
                .collect(),
            None => Vec::new(),
        };
        if old_members != self.members || self.members_tag != tag {
            self.skill_search = None;
        }
        self.members_tag = tag;
    }

    fn ask_merge(&mut self) -> Vec<Action> {
        let Some(tag) = self.actionable_tag() else {
            return vec![];
        };
        let others: Vec<String> = self
            .rows
            .iter()
            .map(|(t, _)| t.clone())
            .filter(|t| t != &tag && t != UNTAGGED)
            .collect();
        if others.is_empty() {
            return vec![Action::Error("no other tag to merge into".into())];
        }
        self.prompt = Some(Prompt::for_merge(&tag, others));
        vec![]
    }

    fn ask_color(&mut self, ctx: &Ctx) -> Vec<Action> {
        let Some(tag) = self.actionable_tag() else {
            return vec![];
        };
        let config = &ctx.ws.config;
        let current = config
            .tags
            .iter()
            .find(|t| t.name == tag)
            .and_then(|t| t.color.as_deref());
        self.prompt = Some(Prompt::for_color(&tag, current, &ctx.settings.theme));
        vec![]
    }

    /// Carry out what the prompt has settled on and put it away.
    fn submit_prompt(&mut self) -> Vec<Action> {
        let Some(p) = self.prompt.as_ref() else {
            return vec![];
        };
        let tag = p.name().to_string();
        match p.kind() {
            Ask::Merge => {
                let Some(into) = p.chosen().map(str::to_string) else {
                    return vec![Action::Error("pick a tag to merge into".into())];
                };
                self.prompt = None;
                vec![Action::WriteMeta(Box::new(move |ws| {
                    history::tag_edit(ws, |ws| {
                        edit::tag_rename(ws, &tag, &into)
                            .map(|n| format!("merged {tag} into {into} on {n} skill(s)"))
                    })
                }))]
            }
            Ask::Color => {
                let Some(color) = p.color_text() else {
                    return vec![Action::Error(format!(
                        "{:?} is not a colour: use a name or #rrggbb",
                        p.value().trim()
                    ))];
                };
                self.prompt = None;
                // Tag colour changes are not recorded in session undo.
                vec![Action::Write(Box::new(move |ws| {
                    Config::set_tag_color(&ws.root, &tag, color.as_deref())?;
                    Ok(match color {
                        Some(c) => format!("{tag} is now {c}"),
                        None => format!("{tag} is back to the default colour"),
                    })
                }))]
            }
        }
    }

    fn prompt_key(&mut self, k: KeyEvent) -> Vec<Action> {
        match self.prompt.as_mut().and_then(|p| p.key(k)) {
            Some(true) => return self.submit_prompt(),
            Some(false) => self.prompt = None,
            None => {}
        }
        vec![]
    }

    fn prompt_mouse(&mut self, m: MouseEvent) -> Vec<Action> {
        match self.prompt.as_mut().and_then(|p| p.mouse(m)) {
            Some(true) => return self.submit_prompt(),
            Some(false) => self.prompt = None,
            None => {}
        }
        vec![]
    }

    fn draw_tags(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        let th = &ctx.settings.theme;
        let focused = !self.focus_grid && self.prompt.is_none();
        let block = th.block(" tags ", focused);
        let inner = block.inner(area);
        f.render_widget(block, area);
        let content = Rect {
            width: inner.width.saturating_sub(1),
            ..inner
        };
        let descriptions: Vec<_> = self
            .rows
            .iter()
            .map(|(tag, _)| {
                ctx.ws
                    .config
                    .tags
                    .iter()
                    .find(|t| &t.name == tag)
                    .and_then(|t| t.description.as_deref())
            })
            .collect();
        self.list.layout_heights(
            content,
            descriptions
                .iter()
                .map(|description| group::card_height(*description))
                .collect(),
        );
        let selected = self.list.selected();
        for i in self.list.visible() {
            let Some(cell) = self.list.cell(i) else {
                continue;
            };
            let (tag, count) = &self.rows[i];
            let on = selected == Some(i);
            let inner = if cell.height < 3 {
                cell
            } else {
                frame(f, cell, on, focused && !self.filter.editing, th)
            };
            let lines = group::group_card(
                tag,
                *count,
                descriptions[i],
                tag_fill(tag, ctx),
                inner.width as usize,
                th,
            );
            f.render_widget(Paragraph::new(lines), inner);
        }
        group::draw_track(f, inner, &self.list, selected, &mut self.list_track, th);
    }

    fn draw_prompt(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        if let Some(p) = self.prompt.as_mut() {
            p.draw(f, area, ctx);
        }
    }
}

impl View for TagsView {
    fn status(&self, ctx: &Ctx) -> String {
        self.skill_search
            .as_ref()
            .map_or_else(String::new, |v| v.status(ctx))
    }
    fn refresh(&mut self, ctx: &Ctx) {
        let selected = self.selected_tag().map(str::to_owned);
        let panel = self.skill_search.take();
        let mut counts = ctx.snap.all_tags();
        for tag in &ctx.ws.config.tags {
            counts.entry(tag.name.clone()).or_insert(0);
        }
        self.rows = counts.into_iter().collect();
        let untagged = ctx
            .snap
            .skills
            .iter()
            .filter(|s| s.tags.is_empty() && s.status.is_present())
            .count();
        self.rows.push((UNTAGGED.into(), untagged));
        self.all_rows = self.rows.clone();
        self.rows.retain(|(tag, _)| self.filter.matches(tag));
        self.list.select(
            selected
                .as_ref()
                .and_then(|tag| self.rows.iter().position(|r| &r.0 == tag)),
        );
        self.list.clamp(self.rows.len());
        self.sync_members(ctx.snap);
        if self.selected_tag() == selected.as_deref()
            && let Some(mut view) = panel
        {
            view.update_panel(self.members.clone(), ctx);
            self.skill_search = Some(view);
        }
    }

    fn handle_key(&mut self, k: KeyEvent, ctx: &Ctx) -> Vec<Action> {
        self.ensure_skill_search(ctx);
        if self.prompt.is_some() {
            return self.prompt_key(k);
        }
        if self.focus_grid
            && let Some(view) = self.skill_search.as_mut()
        {
            if k.code == KeyCode::Left && view.panel_back() {
                self.focus_grid = false;
                return vec![];
            }
            if view.panel_actions_ready() && k.modifiers.is_empty() {
                if matches!(k.code, KeyCode::Char('x') | KeyCode::Delete) {
                    let keys = view.panel_keys(ctx);
                    return self.remove_members(keys);
                }
                if k.code == KeyCode::Char('a') {
                    return self.add_members(ctx);
                }
            }
            return view.handle_key(k, ctx);
        }
        if !self.focus_grid && self.filter.key(k) {
            self.refilter(ctx);
            return vec![];
        }
        let n = self.rows.len();
        let m = self.members.len();
        match k.code {
            KeyCode::Char('q') => vec![Action::SwitchTab(Tab::Search)],
            // Esc means "back" everywhere else in the program, so here it goes
            // back to the search page rather than out of the door.
            KeyCode::Esc => vec![Action::SwitchTab(Tab::Search)],
            KeyCode::Down | KeyCode::Char('j') => {
                self.list.move_by(1, n);
                self.sync_members(ctx.snap);
                vec![]
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.list.selected().unwrap_or(0) == 0 {
                    self.filter.editing = true;
                    return vec![];
                }
                self.list.move_by(-1, n);
                self.sync_members(ctx.snap);
                vec![]
            }
            KeyCode::Home | KeyCode::Char('g') => {
                self.list.first(n);
                self.sync_members(ctx.snap);
                vec![]
            }
            KeyCode::End | KeyCode::Char('G') => {
                self.list.last(n);
                self.sync_members(ctx.snap);
                vec![]
            }
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
                if m > 0 {
                    self.focus_grid = true;
                }
                vec![]
            }
            KeyCode::Char('r') => match self.actionable_tag() {
                Some(t) => vec![Action::OpenModal(Box::new(Modal::rename_tag(&t)))],
                None => vec![],
            },
            KeyCode::Char('m') => self.ask_merge(),
            KeyCode::Char('c') => vec![Action::OpenModal(Box::new(Modal::new_tag()))],
            KeyCode::Char('a') => self.add_members(ctx),
            KeyCode::Char('e') => match self.actionable_tag() {
                Some(tag) => {
                    let description = ctx
                        .ws
                        .config
                        .tags
                        .iter()
                        .find(|t| t.name == tag)
                        .and_then(|t| t.description.as_deref());
                    vec![Action::OpenModal(Box::new(Modal::tag_description(
                        &tag,
                        description,
                    )))]
                }
                None => vec![],
            },
            KeyCode::Char('C') => self.ask_color(ctx),
            KeyCode::Char('D' | 'x') | KeyCode::Delete => match self.actionable_tag() {
                Some(t) => vec![Action::OpenModal(Box::new(Modal::delete_tag(&t)))],
                None => vec![],
            },
            _ => vec![],
        }
    }

    fn handle_mouse(&mut self, m: MouseEvent, ctx: &Ctx) -> Vec<Action> {
        if self.prompt.is_some() {
            return self.prompt_mouse(m);
        }
        self.ensure_skill_search(ctx);
        let at = (m.column, m.row).into();
        let pressing = m.kind == MouseEventKind::Down(MouseButton::Left);
        let dragging = m.kind == MouseEventKind::Drag(MouseButton::Left);
        if pressing && self.filter.rect.contains(at) {
            self.focus_grid = false;
            self.filter.editing = true;
            return vec![];
        }
        if self.right.contains(at) {
            self.focus_grid = true;
            self.filter.editing = false;
            return self.skill_search.as_mut().unwrap().handle_mouse(m, ctx);
        }
        if let Some(d) = wheel(&m, ctx) {
            if self.left.contains(at) {
                self.list.move_by(d.signum(), self.rows.len());
                self.sync_members(ctx.snap);
            }
            return vec![];
        }
        if (pressing && self.list_track.hit(m.column, m.row)) || (dragging && self.list_drag) {
            self.list_drag = true;
            self.focus_grid = false;
            if let Some(row) = self.list_track.index_at(m.row, self.list.grid_rows()) {
                self.list.select_row(row);
                self.sync_members(ctx.snap);
            }
            return vec![];
        }
        if !dragging {
            self.list_drag = false;
        }
        if pressing && self.left.contains(at) {
            self.focus_grid = false;
            if let Some((_, double)) = self.list.click(m.column, m.row) {
                self.sync_members(ctx.snap);
                if double && !self.members.is_empty() {
                    self.focus_grid = true;
                }
            }
        }
        vec![]
    }

    fn draw(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        self.ensure_skill_search(ctx);
        let (left, right) = split_panes(area, 38, ctx);
        self.left = left;
        self.right = right;
        let content = self.filter.draw(f, left, "Filter tags", ctx);
        self.draw_tags(f, content, ctx);
        if let Some(view) = self.skill_search.as_mut() {
            view.set_panel_active(self.focus_grid);
            view.draw(f, right, ctx);
        }
        self.draw_prompt(f, area, ctx);
    }

    fn hints(&self) -> Hints {
        if self.filter.editing {
            return &[("Enter/↓", "tags"), ("Esc", "clear filter")];
        }
        if self.focus_grid
            && let Some(view) = self.skill_search.as_ref()
        {
            return view.hints();
        }
        match self.prompt.as_ref().map(Prompt::kind) {
            Some(Ask::Merge) => &[("↑/↓", "target"), ("Enter", "merge"), ("Esc", "cancel")],
            Some(Ask::Color) => &[
                ("type", "a name or #rrggbb"),
                ("Enter", "apply"),
                ("Esc", "cancel"),
            ],
            None if self.focus_grid => &[
                ("/", "filter skills"),
                ("Enter", "preview"),
                ("a", "add skills"),
                ("x", "remove from tag"),
                ("t", "edit tags"),
                ("m", "multi-select"),
                ("←/Esc", "tags"),
            ],
            None => &[
                ("/", "filter tags"),
                ("c", "create"),
                ("a", "add skills"),
                ("e", "description"),
                ("r", "rename"),
                ("m", "merge"),
                ("C", "colour"),
                ("D", "delete tag"),
                ("Enter/→", "skills"),
                ("q", "library"),
            ],
        }
    }
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;
    use skills::config::TagConfig;

    #[test]
    fn tags_draw_and_edit_from_the_supplied_configuration_snapshot() {
        let tmp = skills::ops::DownloadDir::new("tags-config-snapshot").unwrap();
        Config {
            agents: vec![],
            tags: vec![TagConfig {
                name: "sample".into(),
                skills: vec![],
                color: Some("blue".into()),
                description: Some("Original description".into()),
            }],
            ..Default::default()
        }
        .save(tmp.path())
        .unwrap();
        let mut ws = skills::Workspace::open(tmp.path()).unwrap();
        let snap = ws.scan().unwrap();
        let theme = crate::tui::theme::Theme::default();

        // A page must not silently acquire newer settings than other pages.
        Config::edit_tags(&ws.root, |tags| {
            tags[0].color = Some("green".into());
            tags[0].description = Some("Updated description".into());
        })
        .unwrap();
        let mut view = TagsView::default();
        for (expected_color, expected_description) in [
            ("blue", "Original description"),
            ("green", "Updated description"),
        ] {
            if expected_color == "green" {
                ws.config = ws.load_config().unwrap();
            }
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
            view.select("sample", &snap);
            let mut term =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 24)).unwrap();
            term.draw(|f| view.draw(f, f.area(), &ctx)).unwrap();
            let text: String = term
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect();
            assert!(text.contains(expected_description));
            view.ask_color(&ctx);
            assert_eq!(
                view.prompt.as_ref().unwrap().color_text(),
                Some(Some(expected_color.into()))
            );
            view.prompt = None;
        }
    }
}
