//! Skill identity and state rendered consistently in cards, lists and compact rows.
//!
//! Pages supply semantic state; they never patch the resulting spans. Entries
//! owned by an agent use the same renderer even without a Library record.

use std::borrow::Cow;

use crate::tui::app::Ctx;
use crate::tui::components::group::tag_pills;
use crate::tui::text::highlight_spans;
use crate::tui::theme::Theme;
use crate::tui::widgets::{fit, pad, width};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use skills::reconcile::{EntryState, SkillRecord, SkillStatus};

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

#[derive(Clone, Copy)]
enum Tone {
    Normal,
    Healthy,
    Warning,
    Error,
}

impl Tone {
    fn style(self, th: &Theme) -> Style {
        match self {
            Self::Normal => th.dim(),
            Self::Healthy => th.ok(),
            Self::Warning => th.warn(),
            Self::Error => th.err(),
        }
    }
}

fn status_marker(status: &SkillStatus) -> (&'static str, Tone) {
    match status {
        SkillStatus::Local | SkillStatus::Repository => ("●", Tone::Healthy),
        SkillStatus::MissingBaseline => ("●", Tone::Warning),
        SkillStatus::Modified => ("✎", Tone::Warning),
        SkillStatus::Missing => ("✗", Tone::Error),
        SkillStatus::Renamed { .. } => ("↪", Tone::Warning),
        SkillStatus::MissingSource => ("!", Tone::Warning),
        SkillStatus::Invalid { .. } | SkillStatus::CorruptMeta { .. } => ("!", Tone::Error),
    }
}

/// Health and preview badges use the same glyph and severity as skill cards.
pub fn status_glyph(status: &SkillStatus, th: &Theme) -> Span<'static> {
    let (glyph, tone) = status_marker(status);
    Span::styled(glyph, tone.style(th))
}

pub fn status_text(status: &SkillStatus) -> String {
    match status {
        SkillStatus::MissingBaseline => "repository · missing baseline".into(),
        SkillStatus::Renamed { to } => format!("renamed? → {to}"),
        SkillStatus::Invalid { reason } => format!("invalid: {reason}"),
        SkillStatus::CorruptMeta { error } => format!("corrupt metadata: {error}"),
        other => other.label().into(),
    }
}

/// Content shared by all densities and by both managed and agent-owned skills.
pub struct SkillPresentation<'a> {
    name: &'a str,
    description: Cow<'a, str>,
    source: String,
    tags: &'a [String],
    marker: &'static str,
    tone: Tone,
    warning: Option<&'a str>,
}

/// Transient selection/search state. This is input to rendering, never a patch
/// to its output. `checked: None` means ordinary single-selection mode.
pub struct SkillRenderState<'a> {
    pub checked: Option<bool>,
    pub update_available: bool,
    pub excerpt: Option<&'a str>,
    pub terms: &'a [String],
    pub context: Option<&'a str>,
    pub show_tags: bool,
    pub show_match_details: bool,
}

impl Default for SkillRenderState<'_> {
    fn default() -> Self {
        Self {
            checked: None,
            update_available: false,
            excerpt: None,
            terms: &[],
            context: None,
            show_tags: true,
            show_match_details: false,
        }
    }
}

impl<'a> SkillPresentation<'a> {
    pub fn managed(r: &'a SkillRecord, ctx: &Ctx) -> Self {
        let (marker, tone) = status_marker(&r.status);
        Self {
            name: display_name(r),
            description: Cow::Borrowed(r.description.as_deref().unwrap_or("No description")),
            source: repository_badge(r, ctx.settings.ui.icons)
                .unwrap_or_else(|| crate::tui::icons::local(ctx.settings.ui.icons).into()),
            tags: &r.tags,
            marker,
            tone,
            warning: (!r.status.is_healthy()).then(|| r.status.label()),
        }
    }

    /// Agent inventory has no synthetic Library record. The relationship is
    /// mapped here once, so every layout uses the same marker and fallback text.
    pub fn entry(
        name: &'a str,
        description: Option<&'a str>,
        state: Option<&'a EntryState>,
    ) -> Self {
        let (marker, tone, label) = match state {
            Some(EntryState::Deployed) => ("✓", Tone::Healthy, "linked"),
            Some(EntryState::Broken { .. }) => ("!", Tone::Error, "broken link"),
            Some(EntryState::Shadow { same_content: true }) => ("▪", Tone::Warning, "shadow"),
            Some(EntryState::Shadow {
                same_content: false,
            }) => ("▪", Tone::Warning, "shadow, differs"),
            Some(EntryState::Foreign { .. }) => ("→", Tone::Warning, "foreign"),
            Some(EntryState::AgentOnly) => ("▪", Tone::Normal, "the agent's own"),
            None => ("—", Tone::Normal, ""),
        };
        let description = description
            .filter(|description| !description.trim().is_empty())
            .map(Cow::Borrowed)
            .unwrap_or_else(|| match state {
                Some(EntryState::Broken { .. }) => {
                    Cow::Borrowed("the link points at something the root no longer has")
                }
                Some(EntryState::Foreign { target }) => {
                    Cow::Owned(format!("links outside the root to {}", target.display()))
                }
                Some(EntryState::AgentOnly) => Cow::Borrowed("only in this agent, not in the root"),
                _ => Cow::Borrowed("No description"),
            });
        Self {
            name,
            description,
            source: label.into(),
            tags: &[],
            marker,
            tone,
            warning: matches!(tone, Tone::Warning | Tone::Error).then_some(label),
        }
    }

    fn marker(&self, state: &SkillRenderState, ctx: &Ctx) -> Span<'static> {
        let th = &ctx.settings.theme;
        let (text, style): (String, Style) = if let Some(checked) = state.checked {
            (
                if checked {
                    "[✓] ".into()
                } else {
                    "[ ] ".into()
                },
                if self.warning.is_some() {
                    self.tone.style(th)
                } else if checked {
                    th.accent()
                } else {
                    th.dim()
                },
            )
        } else if state.update_available && self.warning.is_none() {
            (" ↑  ".into(), th.accent())
        } else {
            (format!(" {}  ", self.marker), self.tone.style(th))
        };
        Span::styled(pad(&text, ctx.settings.layout.marker_width), style)
    }

    fn identity(&self, columns: usize, state: &SkillRenderState, ctx: &Ctx) -> Vec<Span<'static>> {
        let th = &ctx.settings.theme;
        let marker = self.marker(state, ctx);
        let mut head = vec![Span::styled(fit(&marker.content, columns), marker.style)];
        let available = columns.saturating_sub(ctx.settings.layout.marker_width);
        let warning = state
            .checked
            .and(self.warning)
            .map(|label| format!(" ! {label}"));
        let warning = warning.filter(|warning| available > width(warning) + 8);
        let name_width = available.saturating_sub(warning.as_deref().map(width).unwrap_or(0));
        head.extend(highlight_spans(
            &pad(self.name, name_width),
            state.terms,
            th.bold(),
            th,
        ));
        if let Some(warning) = warning {
            head.push(Span::styled(warning, self.tone.style(th)));
        }
        head
    }

    fn source(&self, state: &SkillRenderState) -> String {
        match state
            .context
            .filter(|tail| !tail.is_empty() && *tail != "local" && *tail != "git")
        {
            Some(tail) => format!("{} · {tail}", self.source),
            None => self.source.clone(),
        }
    }

    /// Identity, two description lines, then source and right-aligned tags.
    pub fn card(&self, ctx: &Ctx, columns: usize, state: &SkillRenderState) -> Vec<Line<'static>> {
        let th = &ctx.settings.theme;
        let summary = summary_lines(state.excerpt.unwrap_or(&self.description), columns);
        let pills = tag_pills(
            self.tags,
            ctx,
            if state.show_tags { columns / 2 } else { 0 },
        );
        let pills_width: usize = pills.iter().map(Span::width).sum();
        let source_width = columns.saturating_sub(pills_width + usize::from(pills_width > 0));
        let source = fit(&self.source(state), source_width);
        let mut foot = vec![Span::styled(source.clone(), th.source())];
        foot.push(Span::raw(
            " ".repeat(columns.saturating_sub(width(&source) + pills_width)),
        ));
        foot.extend(pills);
        vec![
            Line::from(self.identity(columns, state, ctx)),
            Line::from(highlight_spans(
                &summary[0],
                state.terms,
                th.description(),
                th,
            )),
            Line::from(highlight_spans(
                &summary[1],
                state.terms,
                th.description(),
                th,
            )),
            Line::from(foot),
        ]
    }

    pub fn list(
        &self,
        ctx: &Ctx,
        columns: usize,
        selected: bool,
        state: &SkillRenderState,
    ) -> Vec<Line<'static>> {
        self.card(ctx, columns.saturating_sub(2), state)
            .into_iter()
            .enumerate()
            .map(|(index, line)| {
                let mut spans = vec![Span::styled(
                    fit(if selected && index == 0 { "▸ " } else { "  " }, columns),
                    ctx.settings.theme.accent(),
                )];
                spans.extend(line.spans);
                Line::from(spans)
            })
            .collect()
    }

    /// Dense identity row with the same marker, source and colored tag pills.
    pub fn compact(
        &self,
        ctx: &Ctx,
        columns: usize,
        selected: bool,
        state: &SkillRenderState,
    ) -> Vec<Line<'static>> {
        let th = &ctx.settings.theme;
        let content_width = columns.saturating_sub(2 + ctx.settings.layout.marker_width);
        let source_width = width(&self.source).min(content_width / 2);
        let source_space = source_width + usize::from(source_width > 0);
        let name_width = ctx
            .settings
            .layout
            .compact_name_width
            .min(content_width.saturating_sub(source_space));
        let marker = self.marker(state, ctx);
        let mut head = vec![
            Span::styled(
                fit(if selected { "▸ " } else { "  " }, columns),
                th.accent(),
            ),
            Span::styled(
                fit(&marker.content, columns.saturating_sub(2)),
                marker.style,
            ),
        ];
        head.extend(highlight_spans(
            &pad(self.name, name_width),
            state.terms,
            th.bold(),
            th,
        ));
        if source_width > 0 {
            head.push(Span::styled(
                format!(" {}", pad(&self.source, source_width)),
                th.source(),
            ));
        }
        let tags_width = content_width.saturating_sub(name_width + source_space);
        if state.show_tags && !self.tags.is_empty() && tags_width > 0 {
            head.push(Span::raw(" "));
            head.extend(tag_pills(self.tags, ctx, tags_width.saturating_sub(1)));
        }
        let mut lines = vec![Line::from(head)];
        if state.show_match_details {
            let context = state
                .context
                .filter(|context| !context.is_empty())
                .map(|context| format!("{context} "))
                .unwrap_or_default();
            let prefix = fit(&format!("    {context}"), columns);
            let available = columns.saturating_sub(width(&prefix));
            let mut sub = vec![Span::styled(
                prefix,
                th.description().add_modifier(Modifier::ITALIC),
            )];
            sub.extend(highlight_spans(
                &fit(state.excerpt.unwrap_or(""), available),
                state.terms,
                th.description(),
                th,
            ));
            lines.push(Line::from(sub));
        }
        lines
    }
}

/// Render Markdown as readable text and wrap at words where possible. CJK and
/// long unbroken tokens wrap at grapheme boundaries, never splitting an emoji.
pub fn summary_lines(markdown: &str, columns: usize) -> [String; 2] {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::settings::RuntimeSettings;
    use ratatui::style::Color;

    #[test]
    fn all_densities_share_source_tag_and_description_styles() {
        let root = skills::ops::DownloadDir::new("skill-presentation-styles").unwrap();
        let mut ws = skills::Workspace::open(root.path()).unwrap();
        std::fs::create_dir_all(root.path().join("calendar")).unwrap();
        std::fs::write(
            root.path().join("calendar/SKILL.md"),
            "---\nname: calendar\ndescription: Calendar details\n---\nCalendar",
        )
        .unwrap();
        ws.config.tags = vec![skills::config::TagConfig {
            name: "events".into(),
            color: Some("#b87e54".into()),
            skills: vec!["calendar".into()],
            description: None,
        }];
        ws.config.save(root.path()).unwrap();
        let snap = ws.scan().unwrap();
        let mut settings = RuntimeSettings::new(&ws.config);
        // Deliberately change the theme to verify that every density reads the
        // supplied settings instead of reproducing a hard-coded style.
        settings.theme.source = Color::Rgb(97, 81, 138);
        let ctx = Ctx {
            ws: &ws,
            snap: &snap,
            settings: &settings,
        };
        let skill = SkillPresentation::managed(snap.get("calendar").unwrap(), &ctx);
        let state = SkillRenderState::default();
        for lines in [
            skill.card(&ctx, 80, &state),
            skill.list(&ctx, 80, true, &state),
            skill.compact(&ctx, 80, true, &state),
        ] {
            let spans: Vec<_> = lines.iter().flat_map(|line| &line.spans).collect();
            assert_eq!(
                spans
                    .iter()
                    .find(|span| span.content.contains("local"))
                    .unwrap()
                    .style,
                settings.theme.source()
            );
            let tag = spans
                .iter()
                .find(|span| span.content.contains("events"))
                .unwrap();
            assert_eq!(tag.style.bg, Some(Color::Rgb(184, 126, 84)));
            assert!(
                spans
                    .iter()
                    .find(|span| span.content.contains("calendar"))
                    .unwrap()
                    .style
                    .add_modifier
                    .contains(Modifier::BOLD)
            );
        }
        for presentation in [
            skill,
            SkillPresentation::entry(
                "calendar",
                Some("Calendar details"),
                Some(&EntryState::AgentOnly),
            ),
        ] {
            let lines = presentation.card(&ctx, 80, &state);
            assert_eq!(lines[1].spans[0].style, settings.theme.description());
            let lines = presentation.compact(
                &ctx,
                80,
                false,
                &SkillRenderState {
                    excerpt: Some("Calendar details"),
                    show_match_details: true,
                    ..Default::default()
                },
            );
            assert_eq!(
                lines[1].spans.last().unwrap().style,
                settings.theme.description()
            );
        }
        let mut settings = settings.clone();
        settings.tags_enabled = false;
        let ctx = Ctx {
            settings: &settings,
            ..ctx
        };
        let skill = SkillPresentation::managed(snap.get("calendar").unwrap(), &ctx);
        for lines in [
            skill.card(&ctx, 80, &state),
            skill.list(&ctx, 80, true, &state),
            skill.compact(&ctx, 80, true, &state),
        ] {
            assert!(!lines.iter().any(|line| line.to_string().contains("events")));
        }
    }

    #[test]
    fn selection_and_update_state_preserve_alignment_and_clipping() {
        let root = skills::ops::DownloadDir::new("skill-presentation-bounds").unwrap();
        let ws = skills::Workspace::open(root.path()).unwrap();
        let snap = ws.scan().unwrap();
        let mut settings = RuntimeSettings::new(&ws.config);
        settings.layout.marker_width = 6;
        settings.layout.compact_name_width = 31;
        let ctx = Ctx {
            ws: &ws,
            snap: &snap,
            settings: &settings,
        };
        let broken = EntryState::Broken {
            target: root.path().join("gone"),
        };
        for status in [
            Some(&EntryState::AgentOnly),
            Some(&EntryState::Deployed),
            Some(&broken),
            None,
        ] {
            let skill = SkillPresentation::entry(
                "中文 👩‍💻 skill",
                Some("**Read** these calendar notes and more"),
                status,
            );
            for checked in [None, Some(false), Some(true)] {
                let state = SkillRenderState {
                    checked,
                    update_available: true,
                    excerpt: Some("long excerpt with 中文 👩‍💻"),
                    context: Some("name·description"),
                    show_match_details: true,
                    ..Default::default()
                };
                for columns in 0..100 {
                    for lines in [
                        skill.card(&ctx, columns, &state),
                        skill.list(&ctx, columns, true, &state),
                        skill.compact(&ctx, columns, true, &state),
                    ] {
                        assert!(
                            lines.iter().all(|line| line.width() <= columns),
                            "columns={columns}, lines={lines:?}"
                        );
                    }
                }
                let card = skill.card(&ctx, 80, &state)[0].to_string();
                let compact = skill.compact(&ctx, 80, true, &state)[0].to_string();
                assert_eq!(
                    width(&card[..card.find("中文").unwrap()]),
                    ctx.settings.layout.marker_width
                );
                assert_eq!(
                    width(&compact[..compact.find("中文").unwrap()]),
                    2 + ctx.settings.layout.marker_width
                );
                if checked.is_some() && matches!(status, Some(EntryState::Broken { .. })) {
                    assert!(card.contains("! broken link"));
                }
            }
        }
    }
}
