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
            settings: &{
                let mut settings = crate::tui::settings::RuntimeSettings::new(&ws.config);
                settings.theme = theme;
                settings
            },
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
use crate::tui::{
    app::Ctx,
    components::{
        group::Pill,
        skill::{SkillPresentation, SkillRenderState, display_name, summary_lines},
    },
    theme::Theme,
};
use ratatui::{style::Modifier, text::Span};

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
        settings: &{
            let mut settings = crate::tui::settings::RuntimeSettings::new(&ws.config);
            settings.theme = theme;
            settings
        },
    };
    let mut record = snap.get(key).unwrap().clone();
    for status in [
        skills::reconcile::SkillStatus::Repository,
        skills::reconcile::SkillStatus::MissingSource,
        skills::reconcile::SkillStatus::Modified,
        skills::reconcile::SkillStatus::Missing,
    ] {
        let mut marker_record = record.clone();
        marker_record.status = status;
        let marker = SkillPresentation::managed(&marker_record, &ctx).card(
            &ctx,
            40,
            &SkillRenderState::default(),
        )[0]
        .spans[0]
            .clone();
        assert_eq!(marker.width(), ctx.settings.layout.marker_width);
        assert!(marker.content.ends_with(' '));
    }
    for checked in [false, true] {
        let marker = SkillPresentation::managed(&record, &ctx).card(
            &ctx,
            40,
            &SkillRenderState {
                checked: Some(checked),
                ..Default::default()
            },
        )[0]
        .spans[0]
            .clone();
        assert_eq!(marker.width(), ctx.settings.layout.marker_width);
        assert!(marker.content.ends_with("] "));
    }
    let lines = SkillPresentation::managed(&record, &ctx).card(
        &ctx,
        60,
        &SkillRenderState {
            context: Some("name"),
            ..Default::default()
        },
    );
    assert!(lines[0].to_string().contains("mock-calendar"));
    assert!(!lines[0].to_string().contains("skills--"));
    assert!(lines[3].to_string().contains("󰊤 sampleorg/kit"));
    assert!(lines[3].to_string().contains("name"));
    record.description = Some("**Description emphasis** with `code`".into());
    let preview = super::preview::preview_lines(&record, &ctx, &[], 60);
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
        for line in SkillPresentation::managed(&record, &ctx).card(
            &ctx,
            width,
            &SkillRenderState::default(),
        ) {
            assert!(line.width() <= width);
        }
    }
    record.name = None;
    assert_eq!(display_name(&record), "skills--mock-calendar");
}
