//! Compact preset and tag coverage for the selected scope.

use super::*;

#[derive(Clone, Copy)]
enum GroupIndex {
    Preset(usize),
    Tag(usize),
}

impl AgentsView {
    pub(super) fn draw_groups(&mut self, f: &mut Frame, area: Rect, ctx: &Ctx) {
        self.preset_rects.clear();
        self.tag_rects.clear();
        if area.is_empty() {
            return;
        }
        if self.presets.is_empty() && self.tags.is_empty() {
            f.render_widget(
                Paragraph::new("No presets or tags").style(ctx.settings.theme.dim()),
                area,
            );
            return;
        }

        let budget = area.width.saturating_sub(3) as usize;
        let mut identities = Vec::with_capacity(self.presets.len() + self.tags.len());
        let mut rendered = Vec::with_capacity(identities.capacity());
        for (i, (preset, status)) in self.presets.iter().enumerate() {
            identities.push(GroupIndex::Preset(i));
            rendered.push(
                group::Badge {
                    kind: group::Kind::Preset,
                    name: &preset.name,
                    fill: group::preset_fill(preset, ctx),
                    coverage: Some((status.installed, status.total)),
                    selected: self.focus() == Focus::Presets && self.preset_cursor == i,
                    focused: self.focus() == Focus::Presets,
                }
                .render(ctx, budget),
            );
        }
        for (i, tag) in self.tags.iter().enumerate() {
            identities.push(GroupIndex::Tag(i));
            rendered.push(
                group::Badge {
                    coverage: Some((tag.included, tag.total)),
                    selected: self.focus() == Focus::Tags && self.tag_cursor == i,
                    focused: self.focus() == Focus::Tags,
                    ..group::Badge::new(&tag.name, group::tag_fill(&tag.name, ctx))
                }
                .render(ctx, budget),
            );
        }

        let selected = match self.focus() {
            Focus::Tags => self.presets.len() + self.tag_cursor,
            _ => self.preset_cursor,
        };
        let widths: Vec<_> = rendered
            .iter()
            .map(|spans| spans.iter().map(Span::width).sum::<usize>() + 1)
            .collect();
        let visible = pill_window(&widths, selected, &mut self.preset_offset, budget);
        let mut spans = vec![Span::styled(
            if visible.start > 0 { "‹" } else { " " },
            ctx.settings.theme.dim(),
        )];
        let mut x = area.x + 1;
        for index in visible.clone() {
            let width = widths[index].saturating_sub(1) as u16;
            let rect = Rect::new(x, area.y, width, 1);
            match identities[index] {
                GroupIndex::Preset(i) => self.preset_rects.push((i, rect)),
                GroupIndex::Tag(i) => self.tag_rects.push((i, rect)),
            }
            spans.extend(rendered[index].clone());
            spans.push(Span::raw(" "));
            x += width + 1;
        }
        if visible.end < identities.len() {
            spans.push(Span::styled("›", ctx.settings.theme.dim()));
        }
        f.render_widget(Paragraph::new(Line::from(spans)), area);
    }
}
