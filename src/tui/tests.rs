//! Application-state tests.
//!
//! Every test here drives the real state machine with no terminal in sight, which is
//! the separation design spec §13 requires.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use super::app::{App, AppOptions, Focus, Overlay, PromptKind};
use super::help;
use crate::config::{Action, Config, Key, KeyBindings, KeyCode};
use crate::doc::Doc;
use crate::search::SearchMode;

/// A document with enough headings and body to scroll through.
const SAMPLE: &str = "\
# Introduction

Alpha beta gamma. The quick brown fox jumps over the lazy dog.

## Details

Delta epsilon zeta. Needle in a haystack.

More prose so the document is comfortably taller than the viewport.

### Deeper

Eta theta iota.

## Summary

Kappa lambda mu. Needle again.
";

/// Builds an app over `source` at a fixed size.
fn pager(source: &str) -> App {
    let mut app = App::new(
        Doc::parse(source),
        Config::default(),
        AppOptions {
            config_path: None,
            title: "sample.md".to_string(),
            icons: false,
            theme: "dark".to_string(),
            toc_open: false,
            width: None,
        },
    );
    app.resize(80, 12);
    let _ = app.canvas();
    app
}

#[test]
fn the_body_cap_reaches_the_render_through_the_pager() {
    // The cap lives in the configuration and is applied by `render::render_document`;
    // this is the wire between them, which nothing else in this module would notice
    // was cut. A paragraph long enough to fill any width, at a viewport far wider
    // than the cap.
    let source = "Alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon phi chi psi omega, and some more words after that.\n";
    let config = Config {
        body_width: Some(40),
        ..Config::default()
    };
    let mut app = App::new(
        Doc::parse(source),
        config,
        AppOptions {
            config_path: None,
            title: "sample.md".to_string(),
            icons: false,
            theme: "dark".to_string(),
            toc_open: false,
            width: None,
        },
    );
    app.resize(120, 12);
    let row = app.canvas().row_text(0);
    // The cap shortens the line; it does not move it. So the wire is visible in where
    // the paragraph *stops*, not in where it starts — it starts at the margin, as
    // everything does. Without the cap this row would run most of the 120 columns.
    let width = row.trim_end().len();
    assert!(
        (30..=42).contains(&width),
        "the paragraph was not capped to 40 columns: {width} columns of text\n{row}"
    );
}

#[test]
fn scrolling_clamps_at_both_ends() {
    let mut app = pager(SAMPLE);
    app.act(Action::LineUp);
    assert_eq!(app.scroll(), 0, "cannot scroll above the top");

    app.act(Action::Bottom);
    let bottom = app.scroll();
    assert_eq!(bottom, app.max_scroll());
    app.act(Action::LineDown);
    assert_eq!(app.scroll(), bottom, "cannot scroll past the bottom");

    app.act(Action::Top);
    assert_eq!(app.scroll(), 0);
}

#[test]
fn paging_moves_by_the_viewport() {
    let mut app = pager(SAMPLE);
    let height = app.viewport_height();
    app.act(Action::PageDown);
    assert_eq!(app.scroll(), (height - 1).min(app.max_scroll()));
    app.act(Action::PageUp);
    assert_eq!(app.scroll(), 0);
}

#[test]
fn progress_is_one_when_the_whole_document_fits() {
    let app = pager("# Tiny\n");
    assert_eq!(app.max_scroll(), 0);
    assert!((app.progress() - 1.0).abs() < f32::EPSILON);
}

#[test]
fn a_resize_keeps_the_reader_in_place() {
    let mut app = pager(SAMPLE);
    app.act(Action::Bottom);
    let top_before = top_of_viewport(&mut app);
    app.resize(40, 12);
    assert!(app.scroll() <= app.max_scroll());
    assert_eq!(
        top_of_viewport(&mut app),
        top_before,
        "a reflow must keep the same text at the top of the viewport"
    );
}

/// The first word on the top row of the viewport.
///
/// This is what "keeps the reader in place" means, and it is what the assertion above
/// is written against. It used to compare `current_heading()` instead, which held only
/// as long as both widths happened to lay the document out to the same height: the
/// reader lands one row above the bottom rather than on it, `heading_probe_row`'s
/// end-of-document case therefore does not apply, and the section it names changes
/// while the text on screen does not. The banner made those heights differ; the
/// assertion was resting on a coincidence either way.
fn top_of_viewport(app: &mut App) -> String {
    let scroll = app.scroll();
    app.canvas()
        .row_text(scroll)
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_string()
}

#[test]
fn dropping_the_render_cache_changes_nothing_visible() {
    let mut app = pager(SAMPLE);
    app.act(Action::HalfPageDown);
    let before = app.canvas().plain_text();
    let scroll = app.scroll();
    let heading = app.current_heading();

    let mut fresh = pager(SAMPLE);
    fresh.scroll_to(scroll);
    assert_eq!(fresh.canvas().plain_text(), before);
    assert_eq!(fresh.current_heading(), heading);
}

#[test]
fn toggling_icons_invalidates_the_render_cache() {
    // The render cache key must include RenderOptions. If it does not, the canvas
    // rendered with Nerd Font glyphs is served again after icons are switched off —
    // a stale frame that looks almost right, which is the worst kind.
    //
    // The document needs something that actually *changes* with the setting. Heading
    // prefixes are gone, and bullets and task boxes are ASCII in both sets, so the
    // cheapest thing that still changes is a code fence's language icon; `SAMPLE` has
    // no fence.
    let mut app = pager("# Title\n\n```rust\ncode\n```\n");
    app.set_icons(true);
    let with_icons = app.canvas().plain_text();

    app.set_icons(false);
    let without_icons = app.canvas().plain_text();
    assert_ne!(
        with_icons, without_icons,
        "switching icons off must re-render the document, not reuse the cache"
    );

    // And switching back reproduces the original exactly.
    app.set_icons(true);
    assert_eq!(app.canvas().plain_text(), with_icons);
}

#[test]
fn render_options_follow_the_flags_that_feed_them() {
    let mut app = pager(SAMPLE);
    app.set_icons(false);
    assert!(!app.render_options().icons);
    app.set_icons(true);
    assert!(app.render_options().icons);

    let config = Config {
        line_numbers: true,
        ..Config::default()
    };
    let app = App::new(
        Doc::parse(SAMPLE),
        config,
        AppOptions {
            config_path: None,
            title: "x".to_string(),
            icons: false,
            theme: "dark".to_string(),
            toc_open: false,
            width: None,
        },
    );
    let options = app.render_options();
    assert!(
        options.line_numbers,
        "config.line_numbers must reach render"
    );
    assert!(!options.icons, "--no-icons must reach render");
}

#[test]
fn there_is_a_horizontal_offset_only_when_something_is_over_wide() {
    let mut app = pager(SAMPLE);
    assert_eq!(app.hscroll(), 0);

    // Prose reflows to the viewport, so there is nothing to scroll sideways to.
    app.act(Action::ScrollRight);
    assert_eq!(
        app.hscroll(),
        0,
        "content that fits must not scroll sideways"
    );
    assert_eq!(app.hscroll_max(), 0);

    // Something that really is wider than the viewport, and the offset becomes real. It
    // has to be *ink* that is over-wide, not just a wide render: since every block
    // anchors at the left margin, forcing `--width 200` on this prose only pads the
    // right-hand side with blanks, and `scroll_reach` rightly refuses to scroll to
    // whitespace. A code line is the standard way to be genuinely too wide.
    let over_wide = format!("{SAMPLE}\n```text\n{}\n```\n", "x".repeat(300));
    let mut wide = App::new(
        Doc::parse(&over_wide),
        Config::default(),
        AppOptions {
            config_path: None,
            title: "x".to_string(),
            icons: false,
            theme: "dark".to_string(),
            toc_open: false,
            width: Some(200),
        },
    );
    wide.resize(80, 12);
    let _ = wide.canvas();
    assert!(wide.hscroll_max() > 0);
    wide.act(Action::ScrollRight);
    assert!(wide.hscroll() > 0, "wide content must scroll sideways");
    wide.act(Action::ScrollLeft);
    assert_eq!(wide.hscroll(), 0, "and back again");
}

#[test]
fn the_toc_pane_toggles_and_takes_focus() {
    let mut app = pager(SAMPLE);
    assert!(!app.toc_is_open());
    assert_eq!(app.focus(), Focus::Document);

    app.act(Action::ToggleToc);
    assert!(app.toc_is_open());
    assert_eq!(app.focus(), Focus::Toc);
    assert!(app.toc_width() > 0);

    app.act(Action::ToggleToc);
    assert!(!app.toc_is_open());
    assert_eq!(app.focus(), Focus::Document);
    assert_eq!(app.toc_width(), 0);
}

#[test]
fn opening_the_toc_narrows_the_document_and_re_renders() {
    let mut app = pager(SAMPLE);
    let wide = app.content_width();
    app.act(Action::ToggleToc);
    let narrow = app.content_width();
    assert!(narrow < wide);
    assert_eq!(app.canvas().width(), narrow);
}

#[test]
fn enter_in_the_toc_jumps_to_the_heading() {
    let mut app = pager(SAMPLE);
    app.act(Action::ToggleToc);
    app.act(Action::LineDown);
    app.act(Action::LineDown);
    let target = app.toc_hits()[app.toc_cursor()].index;
    let expected = app.toc().row_of(target).expect("the heading was rendered");
    app.act(Action::Confirm);
    assert_eq!(app.scroll(), expected.min(app.max_scroll()));
    assert_eq!(
        app.focus(),
        Focus::Document,
        "jumping returns to the document"
    );
}

#[test]
fn slash_in_the_toc_filters_instead_of_searching() {
    let mut app = pager(SAMPLE);
    app.act(Action::ToggleToc);
    app.act(Action::SearchForward);
    assert!(matches!(
        app.overlay(),
        Overlay::Prompt {
            kind: PromptKind::TocFilter,
            ..
        }
    ));

    for ch in "sum".chars() {
        app.on_key(Key::char(ch));
    }
    assert_eq!(app.toc_hits().len(), 1);
    assert_eq!(app.toc().entries()[app.toc_hits()[0].index].text, "Summary");

    app.on_key(Key::plain(KeyCode::Esc));
    assert_eq!(*app.overlay(), Overlay::None);
    assert_eq!(
        app.toc_hits().len(),
        app.toc().len(),
        "escape clears the filter"
    );
}

#[test]
fn heading_stepping_walks_the_document() {
    let mut app = pager(SAMPLE);
    app.act(Action::NextHeading);
    let first = app.scroll();
    assert!(first > 0);
    app.act(Action::NextHeading);
    assert!(app.scroll() > first);
    app.act(Action::PrevHeading);
    assert_eq!(app.scroll(), first);
}

#[test]
fn searching_finds_matches_and_reports_the_count() {
    let mut app = pager(SAMPLE);
    app.run_search("needle");
    assert_eq!(app.search().len(), 2);
    assert_eq!(app.search_index(), Some(0));

    app.act(Action::NextMatch);
    assert_eq!(app.search_index(), Some(1));
    app.act(Action::NextMatch);
    assert_eq!(app.search_index(), Some(0), "search wraps around");
    app.act(Action::PrevMatch);
    assert_eq!(app.search_index(), Some(1), "backwards wraps too");
}

#[test]
fn a_search_with_no_matches_says_so_and_changes_nothing() {
    let mut app = pager(SAMPLE);
    app.act(Action::Bottom);
    let scroll = app.scroll();
    app.run_search("zzz-not-here");
    assert_eq!(app.search().len(), 0);
    assert_eq!(app.scroll(), scroll);
    assert!(app.notice().is_some_and(|notice| notice.is_error));
}

#[test]
fn searching_scrolls_the_match_into_view() {
    let mut app = pager(SAMPLE);
    app.run_search("Kappa");
    let row = app.search().hits()[0].row().expect("the match was drawn");
    assert!(
        (app.scroll()..app.scroll() + app.viewport_height()).contains(&row),
        "row {row} is not visible from scroll {}",
        app.scroll()
    );
}

#[test]
fn searching_is_literal_until_the_user_asks_for_a_regex() {
    let mut app = pager("a (b) c\n");
    assert_eq!(app.search_mode(), SearchMode::Literal);

    // Literal mode takes the query as typed: no pattern compilation, no error.
    app.run_search("(b");
    assert_eq!(app.search().len(), 1, "`(b` must be found literally");
    assert!(app.notice().is_none_or(|notice| !notice.is_error));

    // Switching is explicit and re-runs the query, which now fails to compile.
    app.act(Action::ToggleSearchMode);
    assert_eq!(app.search_mode(), SearchMode::Regex);
    assert!(
        app.notice().is_some(),
        "the mode switch must announce itself"
    );

    app.run_search("(b");
    assert!(
        app.notice().is_some_and(|notice| notice.is_error),
        "a broken pattern must report, not silently fall back"
    );
}

#[test]
fn regex_mode_actually_matches_patterns() {
    let mut app = pager("cat1 dog22 cow333\n");
    app.act(Action::ToggleSearchMode);
    app.run_search(r"[a-z]+\d{2,}");
    assert_eq!(app.search().len(), 2);
    assert_eq!(app.search().mode(), SearchMode::Regex);

    // And the same query in literal mode finds nothing, proving the mode is real.
    app.act(Action::ToggleSearchMode);
    assert_eq!(app.search().len(), 0);
}

#[test]
fn the_search_mode_is_visible_at_the_prompt() {
    let mut app = pager(SAMPLE);
    assert_eq!(
        PromptKind::SearchForward.sigil(app.search_mode()),
        "/",
        "literal search keeps the familiar sigil"
    );
    app.act(Action::ToggleSearchMode);
    assert_eq!(
        PromptKind::SearchForward.sigil(app.search_mode()),
        "re/",
        "a regex search must name itself"
    );
    assert_eq!(PromptKind::SearchBackward.sigil(app.search_mode()), "re?");
    assert_eq!(
        PromptKind::TocFilter.sigil(app.search_mode()),
        "toc /",
        "the toc filter is unaffected by the search mode"
    );
}

#[test]
fn the_mode_can_be_switched_while_typing_a_query() {
    let mut app = pager(SAMPLE);
    app.on_key(Key::char('/'));
    for ch in "Need".chars() {
        app.on_key(Key::char(ch));
    }
    app.on_key(Key::ctrl('r'));
    assert_eq!(app.search_mode(), SearchMode::Regex);

    // Ctrl-R switches the mode; it must not be typed into the query.
    let Overlay::Prompt { input, .. } = app.overlay() else {
        panic!("the prompt should still be up");
    };
    assert_eq!(input, "Need", "the chord must not reach the input");
}

#[test]
fn the_search_prompt_collects_input_and_runs_on_enter() {
    let mut app = pager(SAMPLE);
    app.on_key(Key::char('/'));
    for ch in "needle".chars() {
        app.on_key(Key::char(ch));
    }
    let Overlay::Prompt { input, .. } = app.overlay() else {
        panic!("the prompt should be up");
    };
    assert_eq!(input, "needle");

    app.on_key(Key::plain(KeyCode::Enter));
    assert_eq!(*app.overlay(), Overlay::None);
    assert_eq!(app.search().len(), 2);
}

#[test]
fn backspacing_an_empty_prompt_closes_it() {
    let mut app = pager(SAMPLE);
    app.on_key(Key::char('/'));
    app.on_key(Key::plain(KeyCode::Backspace));
    assert_eq!(*app.overlay(), Overlay::None);
}

#[test]
fn escape_unwinds_state_and_never_quits() {
    let mut app = pager(SAMPLE);
    app.resize(100, 30);
    app.act(Action::ToggleToc);
    assert_eq!(app.focus(), Focus::Toc);

    app.act(Action::Cancel);
    assert_eq!(app.focus(), Focus::Document, "first Esc releases the pane");
    assert!(app.toc_is_open(), "the pane is still on screen");

    app.act(Action::Cancel);
    assert!(!app.toc_is_open(), "second Esc closes the pane");

    // Escape from a bare document does nothing at all: `q` is the way out.
    for _ in 0..5 {
        app.act(Action::Cancel);
        assert!(!app.should_quit(), "Esc must never quit");
    }
    assert!(app.notice().is_some(), "and it says so rather than sulking");
}

#[test]
fn escape_clears_the_search_before_anything_else() {
    let mut app = pager(SAMPLE);
    app.resize(100, 30);
    app.act(Action::ToggleToc);
    app.act(Action::Cancel);
    app.run_search("Alpha");
    assert!(!app.search().query().is_empty());

    app.act(Action::Cancel);
    assert!(app.search().query().is_empty(), "the search goes first");
    assert!(app.toc_is_open(), "and the pane is left alone");
    assert!(!app.should_quit());
}

#[test]
fn escape_does_not_quit_with_an_overlay_open() {
    let mut app = pager(SAMPLE);
    app.act(Action::Help);
    app.on_key(Key::plain(KeyCode::Esc));
    assert_eq!(*app.overlay(), Overlay::None);
    assert!(!app.should_quit());
}

#[test]
fn the_help_overlay_opens_scrolls_and_dismisses() {
    let mut app = pager(SAMPLE);
    app.act(Action::Help);
    assert_eq!(*app.overlay(), Overlay::Help);

    // Movement keys move the overlay, not the document behind it.
    app.on_key(Key::char('j'));
    assert_eq!(*app.overlay(), Overlay::Help, "j scrolls the help");
    assert_eq!(app.help_scroll(), 1);
    assert_eq!(app.scroll(), 0, "and never the document");
    app.on_key(Key::char('k'));
    assert_eq!(app.help_scroll(), 0);

    app.on_key(Key::char('t'));
    assert_eq!(*app.overlay(), Overlay::None, "anything else dismisses it");
    assert_eq!(app.help_scroll(), 0, "and the overlay reopens at the top");
}

#[test]
fn the_help_overlay_is_generated_from_the_live_bindings() {
    let mut keys = KeyBindings::empty();
    keys.bind(Key::char('Z'), Action::Quit);
    let config = Config {
        keys,
        ..Config::default()
    };

    let sections = help::sections(&config.keys);
    let rows: Vec<&super::help::HelpRow> = sections
        .iter()
        .flat_map(|section| section.rows.iter())
        .collect();
    assert_eq!(rows.len(), 1, "only bound actions may be listed");
    assert_eq!(rows[0].keys, "Z");
    assert_eq!(rows[0].description, Action::Quit.description());

    // And the default table lists every action exactly once.
    let defaults = help::sections(&KeyBindings::defaults());
    let listed: usize = defaults.iter().map(|section| section.rows.len()).sum();
    assert_eq!(listed, Action::ALL.len());
}

#[test]
fn the_help_overlay_splits_into_columns_rather_than_clipping() {
    let sections = help::sections(&KeyBindings::defaults());
    let total: usize = sections.iter().map(|section| section.rows.len()).sum();

    // Plenty of room: one column, nothing dropped.
    let one = help::columns(sections.clone(), 100, 4);
    assert_eq!(one.len(), 1);

    // A short terminal: more columns, and still every row.
    let many = help::columns(sections.clone(), 8, 4);
    assert!(many.len() > 1, "a short overlay must use extra columns");
    let kept: usize = many
        .iter()
        .flat_map(|column| column.iter())
        .map(|section| section.rows.len())
        .sum();
    assert_eq!(kept, total, "no binding may be dropped");

    // A narrow terminal cannot add columns, so it falls back to one.
    let narrow = help::columns(sections, 8, 1);
    assert_eq!(narrow.len(), 1);
}

#[test]
fn rebinding_a_key_changes_what_it_does() {
    let mut config = Config::default();
    config.keys.bind(Key::char('j'), Action::Quit);
    let mut app = App::new(
        Doc::parse(SAMPLE),
        config,
        AppOptions {
            config_path: None,
            title: "x".to_string(),
            icons: false,
            theme: "dark".to_string(),
            toc_open: false,
            width: None,
        },
    );
    app.resize(80, 12);
    app.on_key(Key::char('j'));
    assert!(app.should_quit());
}

#[test]
fn an_unbound_key_does_nothing() {
    let mut app = pager(SAMPLE);
    app.on_key(Key::char('\u{1f600}'));
    assert_eq!(app.scroll(), 0);
    assert!(!app.should_quit());
}

#[test]
fn cycling_themes_re_renders_and_wraps() {
    let mut app = pager(SAMPLE);
    assert_eq!(app.theme().name, "dark");
    app.act(Action::CycleTheme);
    assert_eq!(app.theme().name, "light");
    let _ = app.canvas();
    app.act(Action::CycleTheme);
    assert_eq!(app.theme().name, "dark");
}

#[test]
fn an_unknown_start_theme_falls_back_without_refusing_to_start() {
    let app = App::new(
        Doc::parse("# x\n"),
        Config::default(),
        AppOptions {
            config_path: None,
            title: "x".to_string(),
            icons: false,
            theme: "no such theme".to_string(),
            toc_open: false,
            width: None,
        },
    );
    assert_eq!(app.theme().name, "dark");
    assert!(app.notice().is_some_and(|notice| notice.is_error));
}

#[test]
fn the_mouse_wheel_scrolls_the_document() {
    let mut app = pager(SAMPLE);
    app.on_scroll(1, false);
    assert_eq!(app.scroll(), usize::from(app.config().scroll_step));
    app.on_scroll(-1, false);
    assert_eq!(app.scroll(), 0);
}

#[test]
fn clicking_the_toc_jumps() {
    let mut app = pager(SAMPLE);
    app.act(Action::ToggleToc);
    app.on_toc_click(0, 1);
    let expected = app.toc().row_of(1).expect("the heading was rendered");
    assert_eq!(app.scroll(), expected.min(app.max_scroll()));
}

#[test]
fn toc_hit_testing_covers_every_visible_row_and_no_border() {
    let mut app = pager(SAMPLE);
    app.act(Action::ToggleToc);
    // A pane 8 rows tall borders rows 0 and 7 and lists rows 1..=6.
    assert_eq!(super::chrome::toc_row_at(&app, 8, 0), None, "top border");
    assert_eq!(super::chrome::toc_row_at(&app, 8, 1), Some(0));
    assert_eq!(
        super::chrome::toc_row_at(&app, 8, 6),
        Some(5),
        "the bottom-most entry must be clickable"
    );
    assert_eq!(super::chrome::toc_row_at(&app, 8, 7), None, "bottom border");
    assert_eq!(super::chrome::toc_row_at(&app, 8, 99), None);

    app.act(Action::ToggleToc);
    assert_eq!(
        super::chrome::toc_row_at(&app, 8, 1),
        None,
        "a closed pane swallows no clicks"
    );
}

#[test]
fn clicking_past_the_end_of_the_toc_does_nothing() {
    let mut app = pager(SAMPLE);
    app.act(Action::ToggleToc);
    app.on_toc_click(0, 999);
    assert_eq!(app.scroll(), 0);
}

#[test]
fn the_toc_scroll_window_keeps_the_cursor_visible() {
    let mut app = pager(SAMPLE);
    app.act(Action::ToggleToc);
    for _ in 0..10 {
        app.act(Action::LineDown);
    }
    let height = 2;
    let first = app.toc_first_visible(height);
    assert!(
        (first..first + height).contains(&app.toc_cursor()),
        "cursor {} outside window starting at {first}",
        app.toc_cursor()
    );
}

#[test]
fn a_forced_width_overrides_the_terminal() {
    let mut app = App::new(
        Doc::parse(SAMPLE),
        Config::default(),
        AppOptions {
            config_path: None,
            title: "x".to_string(),
            icons: false,
            theme: "dark".to_string(),
            toc_open: false,
            width: Some(40),
        },
    );
    app.resize(120, 20);
    assert_eq!(app.content_width(), 40);
    assert_eq!(app.canvas().width(), 40);
}

#[test]
fn a_document_with_no_headings_still_works() {
    let mut app = pager("just prose, no headings at all\n");
    app.act(Action::ToggleToc);
    assert!(app.toc_hits().is_empty());
    app.act(Action::Confirm);
    app.act(Action::NextHeading);
    assert_eq!(app.scroll(), 0);
    assert!(!app.should_quit());
}

/// A document whose code line and table both need far more room than any viewport.
const WIDE: &str = "\
# Wide

Prose that must keep wrapping to the viewport, never to the widest block.

```rust
fn f() { let a = \"aaaaaaaaaaaaaaaaaaaaaaaaaaaa\"; let b = \"bbbbbbbbbbbbbbbbbbbbbbbbbbbb\"; let c = \"cccccccccccccccccccccccccccc\"; }
```

| AlphaColumnOne | BetaColumnTwo | GammaColumnThree | DeltaColumnFour | EpsilonFive |
|---|---|---|---|---|
| 1 | 2 | 3 | 4 | 5 |
";

/// Builds an app over `source` at an explicit size.
fn pager_at(source: &str, width: u16, height: u16) -> App {
    pager_named(source, "sample.md", width, height)
}

/// Builds an app over `source` at an explicit size, under a chosen file name.
fn pager_named(source: &str, title: &str, width: u16, height: u16) -> App {
    let mut app = App::new(
        Doc::parse(source),
        Config::default(),
        AppOptions {
            config_path: None,
            title: title.to_string(),
            icons: false,
            theme: "dark".to_string(),
            toc_open: false,
            width: None,
        },
    );
    app.resize(width, height);
    let _ = app.canvas();
    app
}

/// Builds an app with `line_numbers = true`, the way a config file would.
fn numbered_pager_at(source: &str, width: u16, height: u16) -> App {
    let mut app = App::new(
        Doc::parse(source),
        Config {
            line_numbers: true,
            ..Config::default()
        },
        AppOptions {
            config_path: None,
            title: "sample.md".to_string(),
            icons: false,
            theme: "dark".to_string(),
            toc_open: false,
            width: None,
        },
    );
    app.resize(width, height);
    let _ = app.canvas();
    app
}

#[test]
fn a_wide_block_makes_the_document_horizontally_scrollable() {
    // The disqualifying bug: the canvas was rendered *at* viewport width, so there
    // was never anything to the right of the viewport and `→` did nothing while the
    // renderer went on painting truncation markers.
    let mut app = pager_at(WIDE, 60, 16);
    assert!(
        app.hscroll_max() > 0,
        "a document with an over-wide block must have somewhere to scroll to"
    );

    app.act(Action::ScrollRight);
    let first = app.hscroll();
    assert!(first > 0, "the right arrow must actually move");
    app.act(Action::ScrollRight);
    assert!(app.hscroll() > first, "and keep moving");

    // And it stops at the end rather than wandering into blank space.
    for _ in 0..200 {
        app.act(Action::ScrollRight);
    }
    assert_eq!(app.hscroll(), app.hscroll_max());

    app.act(Action::ScrollLeft);
    assert!(app.hscroll() < app.hscroll_max());
    for _ in 0..200 {
        app.act(Action::ScrollLeft);
    }
    assert_eq!(app.hscroll(), 0, "and back to the left edge");
}

#[test]
fn widening_a_block_does_not_reflow_the_prose() {
    let app = pager_at(WIDE, 60, 16);
    let viewport = usize::from(app.viewport_width());
    assert!(app.rendered().width() > app.viewport_width());

    let prose = (0..app.rendered().height())
        .map(|row| app.rendered().row_text(row))
        .find(|text| text.contains("Prose that must keep wrapping"))
        .expect("the paragraph is in the render");
    assert!(
        crate::text::display_width(prose.trim_end()) <= viewport,
        "prose must stay wrapped to the viewport, not to the widest block: {prose:?}"
    );
}

#[test]
fn a_document_that_fits_is_not_widened() {
    let app = pager_at(SAMPLE, 80, 12);
    assert_eq!(
        app.rendered().width(),
        app.content_width(),
        "nothing is clipped, so nothing is widened"
    );
    assert_eq!(app.hscroll_max(), 0);
}

#[test]
fn the_pager_keeps_the_document_margin_the_renderer_promises() {
    // `render_document` documents a margin on each side so that "no block — paragraph,
    // table border or code frame — is ever welded to the viewport edge or to the
    // scrollbar next to it". The pager does not call it: it assembles blocks itself, so
    // the promise held only for the piped renderer and `visual-review-3.md` §15 found
    // every line in the live TUI hard against the scrollbar.
    let markdown = concat!(
        "# Heading\n\nA paragraph long enough to reach the right-hand edge of a narrow ",
        "pane and wrap.\n\n| left | right |\n|---|---|\n| a | b |\n\n",
        "```rust\nfn main() {}\n```\n"
    );
    let doc = Doc::parse(markdown);
    let width = 60;
    let canvas = crate::render::render_document(
        &doc,
        width,
        None,
        &crate::theme::Theme::default_dark(),
        &crate::render::RenderOptions::new(false, false),
    );
    let margin = usize::from(crate::render::DOCUMENT_MARGIN);
    assert_eq!(
        canvas.width(),
        width,
        "nothing here is wide enough to widen"
    );
    for row in 0..canvas.height() {
        let text = canvas.row_text(row);
        let (left, rest) = crate::text::split_at_width(&text, margin);
        assert!(
            left.trim().is_empty(),
            "row {row} has no left margin: {text:?}"
        );
        let right = crate::text::split_at_width(rest, usize::from(width) - 2 * margin).1;
        assert!(
            right.trim().is_empty(),
            "row {row} runs into the gutter the scrollbar sits beside: {text:?}"
        );
    }
}

#[test]
fn the_overflow_marker_matches_the_renderer() {
    // `ClipTest` decides whether to re-render a block wider by looking for this glyph in
    // what came back. It used to be a second copy of `render::code`'s constant, kept
    // because the widening lived under `tui` and could not see it, and this test pinned
    // the two together; the module lives in `render` now and uses the original, so what
    // is left to guard is the other half — that the renderer still *draws* the glyph the
    // clip test hunts for. Change how clipping is marked and widening silently stops
    // working, with nothing else failing near the change.
    let doc = Doc::parse("```text\nAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n```\n");
    let canvas = crate::render::render_flat(
        &doc,
        20,
        &crate::theme::Theme::default_dark(),
        &crate::render::RenderOptions::new(false, false),
    );
    let text = canvas.plain_text();
    assert!(
        text.contains(crate::render::document::OVERFLOW_MARKER),
        "the renderer still marks clipped content with {:?}: {text}",
        crate::render::document::OVERFLOW_MARKER
    );
}

#[test]
fn the_quote_bar_matches_the_renderer() {
    // The other former copy; see `the_overflow_marker_matches_the_renderer`. If the
    // renderer ever changes this glyph, a quote's separator rows stop reading as blank,
    // the quote becomes one run again, and a wide block inside it silently drags the
    // quoted prose off the screen — with no test failing anywhere near the change.
    let doc = Doc::parse("> before\n>\n> after\n");
    let canvas = crate::render::render_flat(
        &doc,
        20,
        &crate::theme::Theme::default_dark(),
        &crate::render::RenderOptions::new(false, false),
    );
    let separator = (0..canvas.height())
        .map(|row| canvas.row_text(row))
        .find(|text| {
            let bare = text.trim();
            !bare.is_empty() && !bare.contains("before") && !bare.contains("after")
        })
        .expect("a quote has a row between its two paragraphs");
    assert_eq!(
        separator.trim(),
        crate::render::document::QUOTE_BAR,
        "the row separating a quote's parts draws the bar and nothing else"
    );
}

#[test]
fn the_gutter_rule_matches_the_renderer() {
    // The renderer publishes where its gutter ends, as a canvas pin, and the arithmetic
    // it publishes is not the arithmetic it draws with — one is `1 + padding + gutter`,
    // the other a `write_str` per column. If the drawn layout moves and the pin does not,
    // the pager holds the wrong columns still and either cuts the numbers in half or eats
    // the first character of the code. Assert the published column against the drawn one,
    // which is the only thing that keeps the two honest.
    let doc = Doc::parse("```javascript\nfirst\nsecond\n```\n");
    let theme = crate::theme::Theme::default_dark();
    let canvas = crate::render::render_flat(
        &doc,
        40,
        &theme,
        &crate::render::RenderOptions::new(false, true),
    );
    let pinned = crate::render::document::pinned_prefix(&canvas);
    let text = canvas.plain_text();
    assert!(
        text.contains("1 │ first"),
        "the renderer still draws `N │ code`: {text}"
    );
    // ` │ 1 │ first`: margin, frame, padding, one digit, blank, rule, blank — code at 7.
    let row = (0..canvas.height())
        .find(|&row| canvas.row_text(row).contains("first"))
        .expect("the first code line is drawn");
    let line = canvas.row_text(row);
    let code = line[..line.find("first").expect("drawn")].chars().count();
    assert_eq!(
        usize::from(pinned[row]),
        code,
        "the pinned prefix ends exactly where the code begins: {line:?}"
    );
    assert!(pinned[row] > 0, "and a numbered block has one at all");
    // The frame rows are pinned with the run, or the box would open as the code slides.
    let frame = (0..canvas.height())
        .find(|&row| canvas.row_text(row).contains('╭'))
        .expect("a framed block");
    assert!(
        pinned[frame] >= pinned[row],
        "the fence's own rows travel with the gutter"
    );
    // The label in the top rule is chrome too, and the third style read off the canvas.
    // Cut in the middle it leaves a fragment of a word sitting in a box rule — which is
    // what happens the moment the prefix stops short of it.
    let top = canvas.row_text(frame);
    let after = top[..top
        .find("javascript")
        .expect("the fence names its language")
        + 10]
        .chars()
        .count();
    assert!(
        usize::from(pinned[frame]) >= after,
        "the fence's label is pinned whole, not cut mid-word: {top:?}"
    );
    let bottom = canvas.height() - 1;
    assert_eq!(
        pinned[bottom],
        pinned[row],
        "a rule with no label is pinned no further than the gutter: {:?}",
        canvas.row_text(bottom)
    );
}

#[test]
fn a_viewport_no_wider_than_the_gutter_scrolls_anyway() {
    // The one way a pinned prefix could take something away: if the prefix filled the
    // pane there would be no column left for the code to scroll into, and the content
    // behind the numbers would be unreachable. Losing the numbers is the better trade at
    // that size, so the prefix is dropped rather than the content.
    let reach = [200u16];
    let pinned = [7u16];
    let wide = super::draw::Offsets::scrolled_to(&reach, &pinned, 20, 40);
    assert_eq!(wide.column(0, 0), 0, "the gutter is drawn at column zero");
    assert_eq!(wide.column(0, 7), 27, "and the code scrolls beside it");
    let narrow = super::draw::Offsets::scrolled_to(&reach, &pinned, 20, 7);
    assert_eq!(
        narrow.column(0, 1),
        21,
        "a pane no wider than the gutter scrolls the whole row instead"
    );
    // What survives the clamp is the marker rail — one column, the document's own
    // margin — because that is what keeps the chevron off the text rather than what
    // keeps the numbers on screen.
    assert_eq!(
        narrow.column(0, 0),
        0,
        "the rail is still held still, whatever else the clamp drops"
    );
}

#[test]
fn the_offsets_map_and_its_inverse_agree_everywhere() {
    // `blit` paints by `column`, `highlight_matches` by `x_of`, and a disagreement
    // between them puts a search highlight on a cell that is not the match. The two are
    // meant to be inverses; assert it rather than argue it, over a row with a pinned
    // gutter, one without, and a range of offsets that includes the clamp.
    for pinned in [0u16, 7] {
        let reach = [200u16];
        let pinned = [pinned];
        for offset in [0u16, 1, 5, 40, 300] {
            let offsets = super::draw::Offsets::scrolled_to(&reach, &pinned, offset, 40);
            let prefix = offsets.pinned(0);
            for x in 0..offsets.content() {
                let column = offsets.column(0, x);
                assert_eq!(
                    offsets.x_of(0, u16::try_from(column).expect("a narrow canvas")),
                    Some(x),
                    "column {column} of a row pinned at {prefix} is drawn at {x}, at offset \
                     {offset}"
                );
            }
            // And the columns that are behind the prefix rather than on screen: the
            // *only* ones without a viewport column, which is what the doc comment says.
            for column in 0..offsets.column(0, offsets.content()) {
                let column = u16::try_from(column).expect("a narrow canvas");
                let hidden = column >= prefix && column < prefix + offsets.at(0);
                assert_eq!(
                    offsets.x_of(0, column).is_none(),
                    hidden,
                    "column {column} is {} the prefix at offset {offset}",
                    if hidden { "behind" } else { "not behind" }
                );
            }
        }
    }
}

/// A table wide enough that a fourteen-column viewport has to cut it.
const WIDE_TABLE: &str = "| aaaaaaaaaa | bbbbbbbbbb |\n|---|---|\n| cccccccccc | dddddddddd |\n";

/// Renders `markdown` the way the pager does: per block, widening anything clipped.
fn scrollable(markdown: &str, width: u16) -> (crate::canvas::Canvas, crate::theme::Theme) {
    let doc = Doc::parse(markdown);
    let theme = crate::theme::Theme::default_dark();
    let options = crate::render::RenderOptions::new(false, false);
    let canvas = crate::render::render_document(&doc, width, None, &theme, &options);
    (canvas, theme)
}

/// The edge glyphs `edge_markers` paints down one side of a viewport.
fn edge_column(
    canvas: &crate::canvas::Canvas,
    theme: &crate::theme::Theme,
    left: u16,
) -> Vec<char> {
    let width = 14u16;
    let height = u16::try_from(canvas.height()).expect("a short document");
    let frames = [theme.code.frame, theme.table.border];
    // The real per-row reach, so this test cannot disagree with the pager about which
    // rows move: a row that has nowhere to go is not cut, and must not be marked.
    let reach = crate::render::document::scroll_reach(canvas, width);
    let pinned = crate::render::document::pinned_prefix(canvas);
    let offsets = super::draw::Offsets::scrolled_to(&reach, &pinned, left, width);
    let rows = painted(width, height, |buffer, area| {
        super::draw::edge_markers(
            buffer,
            area,
            canvas,
            0,
            &offsets,
            &super::draw::Marks {
                style: ratatui::style::Style::default(),
                frames: &frames,
                stripe: theme.table.row_alt.bg,
            },
        );
    });
    let at = if left > 0 { 0 } else { usize::from(width) - 1 };
    rows.iter()
        .map(|row| row.chars().nth(at).unwrap_or(' '))
        .collect()
}

#[test]
fn a_clipped_table_is_still_detected_and_widened() {
    // The tripwire `wide.rs` names: a table now closes its cut rules instead of marking
    // them, and `ClipTest` finds clipped blocks by looking for that marker. If the
    // renderer ever stopped marking the content rows as well, this document would come
    // back at viewport width with its second column gone for good.
    let (canvas, _) = scrollable(WIDE_TABLE, 14);
    assert!(
        canvas.width() > 14,
        "the clipped table was widened to {} columns",
        canvas.width()
    );
    assert!(
        canvas.plain_text().contains("bbbbbbbbbb"),
        "the column that did not fit is reachable: {}",
        canvas.plain_text()
    );
}

#[test]
fn a_viewport_edge_closes_the_frame_it_cuts() {
    // `docs/qa/visual-review-3.md` §11, in the pager rather than the renderer: the
    // window, not the render, is what truncates a widened block, so the edge markers are
    // where a table or a fence gets its frame broken. A rule ends in its own glyph on
    // whichever side it was cut; the content rows between them keep the chevron.
    let (canvas, theme) = scrollable(WIDE_TABLE, 14);
    assert_eq!(
        edge_column(&canvas, &theme, 0),
        vec!['╮', '›', '┤', '›', '╯'],
        "the right edge closes the frame it cuts"
    );
    assert_eq!(
        edge_column(&canvas, &theme, 5),
        vec!['╭', '‹', '├', '‹', '╰'],
        "and so does the left edge, once the reader has scrolled"
    );
}

/// A table wide enough to be side-scrolled whose rows are two lines tall, so the
/// renderer puts a gap between them.
const WIDE_SPACED_TABLE: &str = concat!(
    "| aaaaaaaaaa | bbbbbbbbbb |\n|---|---|\n",
    "| cccccccccc cccccccccc | dddddddddd |\n",
    "| eeeeeeeeee eeeeeeeeee | ffffffffff |\n",
);

#[test]
fn a_viewport_edge_passes_over_a_row_gap_in_silence() {
    // The same argument as the rules, one step further: a row gap is shading between two
    // rows and carries nothing that can be cut off. Marked, it would put a chevron where
    // nothing continues — and the rail down the edge would read as a column of markers
    // with holes in it wherever a row happens to end.
    let (canvas, theme) = scrollable(WIDE_SPACED_TABLE, 14);
    let stripe = theme.table.row_alt.bg;
    let gaps: Vec<usize> = canvas
        .rows()
        .iter()
        .enumerate()
        .filter(|(_, cells)| crate::render::table::is_row_gap(cells, stripe))
        .map(|(row, _)| row)
        .collect();
    assert_eq!(gaps.len(), 1, "one gap between two body rows");
    for left in [0, 5] {
        let column = edge_column(&canvas, &theme, left);
        assert_eq!(
            column[gaps[0]], ' ',
            "the gap is marked at offset {left}: {column:?}"
        );
        assert!(
            column.iter().any(|glyph| *glyph == '›' || *glyph == '‹'),
            "the content rows must still be marked: {column:?}"
        );
    }
}

#[test]
fn a_viewport_edge_marks_box_art_inside_a_fence_rather_than_closing_it() {
    // The pager's rule is a guess made from the glyph under the cut, so it is gated on
    // the glyph being painted in a *frame* style. Without that gate a fence full of box
    // art would have its content rewritten into corners, which is both a lie about the
    // content and — in the renderer, where the same guess would have to be made — a way
    // to lose horizontal scrolling.
    let art = "╭─────────────────────────────╮";
    let (canvas, theme) = scrollable(&format!("```text\n{art}\n{art}\n```\n"), 14);
    assert_eq!(
        edge_column(&canvas, &theme, 0),
        vec!['╮', '›', '›', '╯'],
        "the fence's own frame closes; its content is marked"
    );
}

/// A quote whose middle is a fence no terminal is wide enough for.
///
/// The prose lines are deliberately shorter than the viewport: a quoted line long
/// enough to reflow at the widened width would be over-wide in its own right, and would
/// then be merged with the fence by the both-sides-over-wide rule for a different and
/// legitimate reason.
const QUOTED_WIDE: &str = concat!(
    "# Heading\n\n> A quoted sentence before the fence.\n>\n> ```text\n> LEFTEDGE",
    "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
    "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
    "RIGHTEDGE\n> ```\n>\n> A quoted sentence after the fence.\n",
);

#[test]
fn a_wide_block_inside_a_quote_does_not_drag_the_quoted_prose() {
    // A blockquote paints its `▌` on every row, including the rows that separate its
    // parts, so "consecutive non-blank rows" made the whole quote one run and a wide
    // fence inside it gave the quoted prose the fence's reach. Scrolled to the end, both
    // sentences and the quote bar were gone and the viewport was a rail of `‹` over
    // empty space — the exact failure the per-run offsets were introduced to fix, one
    // nesting level down.
    let mut app = pager_at(QUOTED_WIDE, 80, 20);
    assert!(app.hscroll_max() > 0, "the probe document must scroll");
    while app.hscroll() < app.hscroll_max() {
        app.act(Action::ScrollRight);
    }

    let rows = framed(&mut app, 80, 20);
    let has = |needle: &str| rows.iter().any(|row| row.contains(needle));
    assert!(
        has("A quoted sentence before the fence."),
        "the prose above the wide fence stays anchored: {rows:?}"
    );
    assert!(
        has("A quoted sentence after the fence."),
        "and so does the prose below it: {rows:?}"
    );
    // And the fence really did travel, or the two assertions above would pass on a
    // build where the arrow key does nothing at all.
    assert!(has("RIGHTEDGE"), "the fence scrolled to its end: {rows:?}");
    assert!(!has("LEFTEDGE"), "and left its start behind: {rows:?}");
}

#[test]
fn a_ragged_block_still_scrolls_as_one_piece() {
    // The other half of the same rule, and the one it is easy to break while fixing the
    // first: rows are grouped into runs precisely so a block with a ragged right edge —
    // a diagram's rank of boxes, a fence of uneven lines — moves as one piece. Per-row
    // extents would slide every row to its own right edge, flushing a ragged block
    // straight and sliding arrows off the boxes they attach to. A green here before the
    // fix as well as after is the point: this is the invariant, not the defect.
    let filler = "x".repeat(100);
    let ragged =
        format!("```text\n{filler}xxxxxxxxxx AEND\n{filler} BEND\n{filler}xxxxxxxxxx CEND\n```\n");
    let mut app = pager_at(&ragged, 40, 12);
    assert!(app.hscroll_max() > 0, "the probe document must scroll");
    while app.hscroll() < app.hscroll_max() {
        app.act(Action::ScrollRight);
    }
    let rows = framed(&mut app, 40, 12);
    let column = |needle: &str| {
        rows.iter()
            .find_map(|row| row.find(needle))
            .unwrap_or_else(|| panic!("{needle} is on screen: {rows:?}"))
    };
    assert_eq!(
        column("AEND"),
        column("CEND"),
        "rows of equal length still line up: {rows:?}"
    );
    assert_eq!(
        column("BEND") + 10,
        column("AEND"),
        "and the short row keeps its ragged edge instead of being flushed right: {rows:?}"
    );
}

#[test]
fn the_right_edge_marker_is_not_hidden_under_a_double_width_glyph() {
    // `blit` puts a wide glyph's lead in the second-to-last column and an empty
    // continuation symbol in the last one; a chevron stamped into that continuation cell
    // is painted over by the glyph in front of it. Which parity that happens on depends
    // on the terminal width alone, so a CJK reader was given no cut indication at all on
    // half of all widths.
    let mut app = pager_at(
        "# CJK\n\n```text\n丙业丛东丝丞丟丠両丢丣两严並丧丨丩个丫丬中丮丯丰丱串丳临丵丶丷丸丹为主丼丽举丿乀乁乂乃乄久乆乇么义乊之乌乍乎乏乐乑乒乓乔\n```\n",
        60,
        10,
    );
    for width in [59u16, 60, 61, 62] {
        app.resize(width, 10);
        let rows = framed(&mut app, width, 10);
        assert!(
            rows.iter().any(|row| row.contains('\u{203a}')),
            "the cut is marked at width {width}: {rows:?}"
        );
    }
}

#[test]
fn the_toc_tracks_the_section_being_read() {
    let mut app = pager(SAMPLE);
    app.act(Action::ToggleToc);
    app.act(Action::Cancel); // release the pane, keep it on screen
    assert_eq!(app.focus(), Focus::Document);
    let at_top = app.toc_cursor();

    app.act(Action::Bottom);
    let current = app
        .current_heading()
        .expect("the last section is a heading");
    assert_eq!(
        app.toc_hits()[app.toc_cursor()].index,
        current,
        "the marker must follow the viewport, not wait for a Tab"
    );
    assert_ne!(app.toc_cursor(), at_top, "and it must actually have moved");

    app.act(Action::Top);
    assert_eq!(app.toc_cursor(), at_top, "and follow back up again");
}

#[test]
fn tracking_leaves_the_pane_alone_while_it_has_focus() {
    let mut app = pager(SAMPLE);
    app.act(Action::ToggleToc);
    app.act(Action::LineDown);
    let chosen = app.toc_cursor();
    app.on_scroll(1, false);
    assert_eq!(
        app.toc_cursor(),
        chosen,
        "scrolling the document must not yank the cursor someone is using"
    );
}

#[test]
fn closing_the_toc_forgets_its_filter() {
    let mut app = pager(SAMPLE);
    app.act(Action::ToggleToc);
    app.act(Action::SearchForward);
    for ch in "sum".chars() {
        app.on_key(Key::char(ch));
    }
    assert_eq!(app.toc_hits().len(), 1);

    app.act(Action::ToggleToc);
    app.act(Action::ToggleToc);
    assert_eq!(
        app.toc_hits().len(),
        app.toc().len(),
        "a filter that survives a close leaves a permanently one-item map"
    );
    assert_eq!(app.toc_filter(), "");
}

#[test]
fn one_enter_commits_the_filter_and_jumps() {
    let mut app = pager(SAMPLE);
    app.act(Action::ToggleToc);
    app.act(Action::SearchForward);
    for ch in "sum".chars() {
        app.on_key(Key::char(ch));
    }
    app.on_key(Key::plain(KeyCode::Enter));
    let target = app.toc().index_of("summary").expect("Summary is a heading");
    let row = app.toc().row_of(target).expect("and it was rendered");
    assert_eq!(app.scroll(), row.min(app.max_scroll()), "one Enter jumps");
}

#[test]
fn a_repeat_count_multiplies_the_movement() {
    let mut app = pager(SAMPLE);
    for ch in "10".chars() {
        app.on_key(Key::char(ch));
    }
    app.on_key(Key::char('j'));
    assert_eq!(app.scroll(), 10usize.min(app.max_scroll()));
    assert_eq!(app.pending_count(), "", "the count is consumed");

    // And a plain `j` afterwards is one line, not ten.
    let before = app.scroll();
    app.on_key(Key::char('j'));
    assert_eq!(app.scroll(), (before + 1).min(app.max_scroll()));
}

#[test]
fn a_percentage_seek_lands_where_it_says() {
    let mut app = pager_at(SAMPLE, 80, 6);
    assert!(app.max_scroll() > 0);
    for ch in "50".chars() {
        app.on_key(Key::char(ch));
    }
    app.on_key(Key::char('%'));
    assert_eq!(app.scroll(), app.max_scroll() / 2);

    for ch in "100".chars() {
        app.on_key(Key::char(ch));
    }
    app.on_key(Key::char('%'));
    assert_eq!(app.scroll(), app.max_scroll());
}

#[test]
fn the_position_report_names_the_file_and_the_lines() {
    let mut app = pager(SAMPLE);
    app.on_key(Key::ctrl('g'));
    let notice = app.notice().expect("Ctrl-G answers").text.clone();
    assert!(notice.contains("sample.md"), "{notice}");
    assert!(notice.contains("lines 1-"), "{notice}");
    assert!(!app.should_quit());
}

#[test]
fn an_unbound_key_says_so_instead_of_nothing() {
    let mut app = pager(SAMPLE);
    app.on_key(Key::char('v'));
    let notice = app.notice().expect("an unknown key must be answered");
    assert!(notice.text.contains("help"), "{}", notice.text);
}

#[test]
fn escape_clears_a_half_typed_count() {
    let mut app = pager(SAMPLE);
    app.on_key(Key::char('4'));
    assert_eq!(app.pending_count(), "4");
    app.on_key(Key::plain(KeyCode::Esc));
    assert_eq!(app.pending_count(), "");
    assert!(!app.should_quit());
}

/// Renders one chrome element into a fresh buffer and returns it as text rows.
fn painted(width: u16, height: u16, paint: impl FnOnce(&mut Buffer, Rect)) -> Vec<String> {
    let area = Rect::new(0, 0, width, height);
    let mut buffer = Buffer::empty(area);
    paint(&mut buffer, area);
    buffer_rows(&buffer, width, height)
}

/// Reads a painted buffer back as one string per row.
fn buffer_rows(buffer: &Buffer, width: u16, height: u16) -> Vec<String> {
    (0..height)
        .map(|y| {
            let mut row = String::new();
            let mut skip = 0usize;
            for x in 0..width {
                if skip > 0 {
                    // A double-width symbol owns the cell after it, whatever that cell
                    // happens to hold; counting it again would over-measure the row.
                    skip -= 1;
                    continue;
                }
                let Some(symbol) = buffer.cell((x, y)).map(|cell| cell.symbol()) else {
                    continue;
                };
                skip = crate::text::display_width(symbol).saturating_sub(1);
                row.push_str(symbol);
            }
            row
        })
        .collect()
}

#[test]
fn the_status_bar_keeps_the_quit_hint_at_every_width() {
    // The narrower the terminal, the less likely the reader knows how to get out, so
    // the help hint is the one segment that is never dropped (usability review P2).
    let mut app = pager(SAMPLE);
    app.run_search("Needle");
    for width in [16u16, 20, 24, 30, 40, 60, 80, 100] {
        app.resize(width, 12);
        let rows = painted(width, 1, |buffer, area| {
            super::chrome::draw_status(buffer, area, &app)
        });
        let bar = &rows[0];
        assert_eq!(
            crate::text::display_width(bar),
            usize::from(width),
            "the bar is exactly the terminal's width at {width}"
        );
        assert!(
            bar.contains("h help"),
            "the quit hint survives at width {width}: {bar:?}"
        );
    }
}

/// The viewport cell `needle`'s first character sits in, on the frame drawn for `app`.
///
/// Read off the *painted frame*, following the same reasoning as [`painted_button`]:
/// this is the reader's own information, and the coordinate their pointer arrives in.
fn painted_at(app: &mut App, width: u16, height: u16, needle: &str) -> (u16, u16) {
    let rows = framed(app, width, height);
    for (y, line) in rows.iter().enumerate() {
        if let Some(at) = line.find(needle) {
            let x = crate::text::display_width(&line[..at]);
            return (
                u16::try_from(x).expect("a viewport column"),
                u16::try_from(y).expect("a viewport row"),
            );
        }
    }
    panic!("{needle:?} is not on the screen: {rows:?}");
}

#[test]
fn sanitized_url_substitutes_a_control_character_without_moving_its_width() {
    // The direct-call pin: one column in, one column out
    // (`crate::text::cell_clusters`), so nothing measured for the status bar's own
    // layout arithmetic moves because of the substitution. This does not prove the
    // call site in `draw_status` still exists -- see the test below for that.
    let hostile = "https://example.com/\u{9b}pwned";
    let safe = super::chrome::sanitized_url(hostile);
    assert!(
        !safe.contains('\u{9b}'),
        "the control character does not survive: {safe:?}"
    );
    assert_eq!(
        crate::text::display_width(&safe),
        crate::text::display_width(hostile),
        "the substitution preserves width"
    );
}

#[test]
fn a_control_character_in_a_hovered_url_cannot_reach_the_terminal() {
    // A real link, through the real parser and the real `classify` -- not injected
    // past them -- carrying U+009B, the C1 form of CSI (`ESC [` in one byte on a
    // terminal that honours 8-bit controls). CommonMark's *bare* destination grammar
    // excludes ASCII control characters but says nothing about the C1 range, so this
    // one is not awkward the way a literal ESC would have been: it survives
    // `Doc::parse` into the hotspot's `url` unchanged (confirmed with a throwaway
    // probe against this crate before writing this test), which is exactly what
    // design spec §8 says must not be assumed either way rather than tested.
    //
    // Hovered the same way `hovering_a_link_shows_its_full_url_in_the_status_bar`
    // does -- real `set_pointer` against the real rendered canvas -- and drawn
    // through `chrome::draw_status` itself, so this fails if the call site to
    // `sanitized_url` is ever dropped, not only if the helper it calls breaks.
    let mut app = pager_at("[here](https://example.com/\u{9b}[31mpwned)\n", 60, 10);
    let (x, y) = painted_at(&mut app, 60, 10, "here");
    app.set_pointer(x, y);
    let rows = painted(60, 1, |buffer, area| {
        super::chrome::draw_status(buffer, area, &app)
    });
    let status = &rows[0];
    assert!(
        !status.contains('\u{9b}'),
        "the raw C1 control byte does not reach the drawn status bar: {status:?}"
    );
    // The load-bearing assertion is the one below, not the one above. `\u{9b}` never
    // survives to `status` *either way* -- that is what the assertion above pins, and
    // it is true for two different reasons depending on whether the call site exists:
    //
    // * with `sanitized_url` wired in, the raw byte is replaced with `text::UNPLACEABLE`
    //   (U+FFFD) *before* the string ever reaches a `Span`, so ratatui draws an
    //   ordinary printable character in its place;
    // * with the call site removed, the raw byte reaches `Span`/`Buffer::set_line`
    //   unsanitized, and ratatui's own buffer-writing code silently drops it there --
    //   verified outside this project against ESC, TAB, CR, BEL, DEL, NUL and this
    //   same C1 byte, none of which are ever placed in a cell. Nothing stands in for
    //   it: the character is simply gone, one column short of what this program's own
    //   width arithmetic assumed.
    //
    // So "no raw `\u{9b}` in the output" cannot distinguish the two cases, and is not
    // the proof. Whether a *marker* took its place is: only the sanitized path leaves
    // one, which is the width-mismatch defect class `cell_clusters` exists to
    // prevent. That marker's presence is what a removed call site cannot fake.
    assert!(
        status.contains('\u{fffd}'),
        "the control character is substituted, not silently dropped: {status:?}"
    );
    assert!(
        status.contains("https://example.com/"),
        "the rest of the url still draws: {status:?}"
    );
}

#[test]
fn hovering_a_link_shows_its_full_url_in_the_status_bar() {
    // Design spec §8: there is deliberately no confirmation prompt before a link
    // opens, and the status bar showing exactly where it goes is the safeguard that
    // stands in for one.
    let mut app = pager_at("[here](https://example.com/a/path)\n", 60, 10);
    let (x, y) = painted_at(&mut app, 60, 10, "here");
    app.set_pointer(x, y);
    let rows = painted(60, 1, |buffer, area| {
        super::chrome::draw_status(buffer, area, &app)
    });
    let status = &rows[0];
    assert!(
        status.contains("https://example.com/a/path"),
        "the status bar must show where the link goes; it said {status:?}"
    );
}

#[test]
fn hovering_a_copy_button_shows_no_url() {
    // The status bar never lies: a hotspot that is not `Open` carries no URL, and
    // showing one anyway would be a stale answer to a question nobody asked.
    let mut app = pager_at("```\ncode\n```\n", 60, 10);
    app.set_copy_button(true);
    let (x, y) = painted_at(&mut app, 60, 10, crate::render::button::LABEL);
    app.set_pointer(x, y);
    let rows = painted(60, 1, |buffer, area| {
        super::chrome::draw_status(buffer, area, &app)
    });
    let status = &rows[0];
    assert!(
        !status.contains("://"),
        "a copy button hovered is not a link hovered: {status:?}"
    );
}

#[test]
fn a_url_too_long_for_the_status_bar_is_elided_at_the_end() {
    // `elide_middle` is the sibling used for the *drawn* suffix, where both ends
    // carry meaning. Here the reader checks the host first, so the host — the
    // front — must survive, and the `…` belongs at the end instead.
    let long = "https://example.com/a/very/long/path/that/will/certainly/overflow/a/narrow/bar";
    let mut app = pager_at(&format!("[here]({long})\n"), 45, 10);
    let (x, y) = painted_at(&mut app, 45, 10, "here");
    app.set_pointer(x, y);
    let rows = painted(45, 1, |buffer, area| {
        super::chrome::draw_status(buffer, area, &app)
    });
    let status = &rows[0];
    assert_eq!(
        crate::text::display_width(status),
        45,
        "the bar is exactly the terminal's width: {status:?}"
    );
    assert!(
        status.contains("https://example.com"),
        "the host survives at the front: {status:?}"
    );
    // The ellipsis sits right after the truncated url, not mid-string: the help chip
    // still follows it, so it is the url's own end that must carry the mark, not the
    // whole bar's.
    assert!(
        status.contains('\u{2026}'),
        "an elided url ends in the ellipsis, not a hard cut: {status:?}"
    );
    assert!(
        !status.contains("overflow/a/narrow/bar"),
        "the tail of the url is what gets dropped, not the host: {status:?}"
    );
}

#[test]
fn the_help_overlay_shows_the_way_out_at_every_height() {
    // An overlay that cannot tell a trapped reader how to quit is the blocker
    // usability review B4 reported; it must hold at any terminal this runs in.
    let mut app = pager(SAMPLE);
    app.act(Action::Help);
    for (width, height) in [(40u16, 6u16), (40, 10), (60, 16), (80, 20), (100, 29)] {
        app.resize(width, height + 1);
        let rows = painted(width, height, |buffer, area| {
            super::chrome::draw_help(buffer, area, &mut app)
        });
        let text = rows.join("\n");
        assert!(
            text.contains(" q ") || text.contains("q quit") || text.contains("q  Quit"),
            "the quit key is visible at {width}x{height}:\n{text}"
        );
        for row in &rows {
            assert_eq!(
                crate::text::display_width(row),
                usize::from(width),
                "no row overflows at {width}x{height}"
            );
        }
    }
}

#[test]
fn a_deep_toc_entry_still_says_something_in_a_narrow_pane() {
    // The indent is what gives way when the pane is narrow. If the prefix is allowed
    // to eat the whole width the entry renders as a blank row, which reads as a bug
    // rather than as a deep heading.
    let mut app = pager("# A\n\n## B\n\n### C\n\n#### D\n\n##### Epsilon heading\n\ntext\n");
    app.act(Action::ToggleToc);
    for width in [14u16, 16, 20, 30] {
        let rows = painted(width, 8, |buffer, area| {
            super::chrome::draw_toc(buffer, area, &app)
        });
        let last = rows
            .iter()
            .find(|row| row.contains('E'))
            .unwrap_or_else(|| panic!("the deepest entry is drawn at width {width}: {rows:?}"));
        assert!(
            last.trim().len() > 1,
            "the deepest entry is not a blank row at width {width}: {last:?}"
        );
    }
}

#[test]
fn a_wide_character_binding_does_not_ragged_edge_the_help_column() {
    // The key column is measured in display columns, so it must be padded in display
    // columns too; `{:>width$}` counts `char`s and would shift every other row.
    let mut keys = KeyBindings::defaults();
    keys.bind(Key::char('\u{ff21}'), Action::Quit);
    let config = Config {
        keys,
        ..Config::default()
    };
    let mut app = App::new(
        Doc::parse(SAMPLE),
        config,
        AppOptions {
            config_path: None,
            title: "x".to_string(),
            icons: false,
            theme: "dark".to_string(),
            toc_open: false,
            width: None,
        },
    );
    app.resize(100, 30);
    app.act(Action::Help);
    let rows = painted(100, 29, |buffer, area| {
        super::chrome::draw_help(buffer, area, &mut app)
    });
    // The description column must land in the same place whatever the key looks like.
    // Display column, not byte offset: a wide key glyph is three bytes and two
    // columns, and it is columns the alignment is about.
    let column_of = |needle: &str| -> usize {
        rows.iter()
            .find_map(|row| {
                row.find(needle)
                    .map(|at| crate::text::display_width(&row[..at]))
            })
            .unwrap_or_else(|| panic!("{needle:?} is in the overlay: {rows:?}"))
    };
    assert_eq!(
        column_of("Quit"),
        column_of("Show or hide this help"),
        "a wide-character binding must not shift its own row's description"
    );
}

#[test]
fn every_chrome_glyph_is_one_column_wide() {
    // `draw_status` lays the bar out with `display_width`, so a glyph that a font
    // renders double-width would shift every segment to its right. Both sets must
    // measure the same, which is also what makes `--no-icons` layout-neutral.
    for icons in [super::icons::Icons::NERD, super::icons::Icons::PLAIN] {
        for glyph in [
            icons.file,
            icons.toc,
            icons.search,
            icons.heading,
            icons.help,
            icons.selected,
            icons.unselected,
            icons.separator,
            icons.warning,
            icons.horizontal,
        ] {
            assert_eq!(
                crate::text::display_width(glyph),
                1,
                "chrome glyph {glyph:?} must occupy exactly one column"
            );
        }
    }
}

/// Paints one whole frame exactly as the pager does, and returns it as text rows.
fn framed(app: &mut App, width: u16, height: u16) -> Vec<String> {
    let backend = ratatui::backend::TestBackend::new(width, height);
    let mut terminal = ratatui::Terminal::new(backend).expect("a test terminal");
    terminal
        .draw(|frame| super::draw::draw(frame, app))
        .expect("a frame");
    buffer_rows(terminal.backend().buffer(), width, height)
}

#[test]
fn scrolling_sideways_moves_only_the_over_wide_blocks() {
    // The disqualifying bug reported against the first horizontal scroll: one wide
    // table dragged the *whole page* sideways, so the heading vanished and both
    // paragraphs were decapitated mid-word while a third of the screen sat blank.
    // Rows that fit have nowhere to go and must not go anywhere.
    let mut app = pager_at(WIDE, 80, 20);
    assert!(app.hscroll_max() > 0, "the probe document must scroll");
    for _ in 0..8 {
        app.act(Action::ScrollRight);
    }
    assert!(app.hscroll() > 0, "and must have been scrolled");

    let rows = framed(&mut app, 80, 20);
    let has = |needle: &str| rows.iter().any(|row| row.contains(needle));
    assert!(has("Wide"), "the heading stays where it was: {rows:?}");
    assert!(
        has("Prose that must keep wrapping"),
        "the paragraph stays where it was: {rows:?}"
    );

    // And the block that *is* over-wide really did move, or the assertions above
    // would pass on a build where the arrow key does nothing at all.
    assert!(
        !has("fn f() { let a ="),
        "the over-wide code block scrolls: {rows:?}"
    );
}

/// A numbered code block with one line far wider than any viewport, and a token
/// `ZEBRA` late in it that only a scrolled reader ever sees.
///
/// The table below it is over-wide too, so the two blocks merge into one scrolling run
/// ([`crate::render::document::scroll_reach`]) — which is exactly the case that must *not* pin the
/// table's first columns along with the fence's gutter. `Ocelot` is the token only a
/// scrolled reader reaches, and unlike code, table cells carry search spans.
const NUMBERED_WIDE: &str = "\
# Numbered

Prose that must keep wrapping to the viewport, never to the widest block.

```rust
fn f() { let a = \"aaaaaaaaaaaaaaaaaaaaaaaaaaaa\"; let b = \"bbbbbbbbbbbbbbbbbbbbbbbbbb ZEBRA\"; }
let second = 1;
```

| AlphaColumnOne | BetaColumnTwoLonger | GammaColumnThreeWider | DeltaColumnFourWider | Ocelot |
|---|---|---|---|---|
| 1 | 2 | 3 | 4 | 5 |
";

/// Paints one whole frame the way the pager does and hands back the buffer.
fn framed_buffer(app: &mut App, width: u16, height: u16) -> Buffer {
    let backend = ratatui::backend::TestBackend::new(width, height);
    let mut terminal = ratatui::Terminal::new(backend).expect("a test terminal");
    terminal
        .draw(|frame| super::draw::draw(frame, app))
        .expect("a frame");
    terminal.backend().buffer().clone()
}

#[test]
fn the_line_number_gutter_stays_put_while_the_code_scrolls_under_it() {
    // The defect: `line_numbers = true` plus a code line wider than the pane, and the
    // numbers scroll off to the left along with the code — they disappear at exactly
    // the moment a long line makes them useful. `render::code` keeps the gutter out of
    // its *own* clip; nothing kept it out of the pager's horizontal offset.
    let mut app = numbered_pager_at(NUMBERED_WIDE, 80, 16);
    assert!(app.hscroll_max() > 0, "the probe document must scroll");

    let before = framed(&mut app, 80, 16);
    let gutter = |rows: &[String], number: &str| {
        rows.iter()
            .filter(|row| row.starts_with(&format!(" │ {number} │")))
            .count()
    };
    assert_eq!(gutter(&before, "1"), 1, "the gutter is drawn: {before:?}");
    assert_eq!(gutter(&before, "2"), 1, "on every code row: {before:?}");

    for _ in 0..40 {
        app.act(Action::ScrollRight);
    }
    assert!(app.hscroll() > 0, "and must have been scrolled");
    let after = framed(&mut app, 80, 16);

    assert_eq!(
        gutter(&after, "1"),
        1,
        "the gutter is still pinned to the left edge after scrolling: {after:?}"
    );
    assert_eq!(
        gutter(&after, "2"),
        1,
        "on every code row, still: {after:?}"
    );
    // The frame's own corners are pinned with the run, so the box never opens — and the
    // cut through its rules is left bare. A chevron there would claim the chrome had been
    // truncated; a second `╭` stamped by `frame_close` would put a corner in the middle of
    // a rule the box already closed one column to its left.
    let rule = after
        .iter()
        .find(|row| row.starts_with(" ╭"))
        .expect("the fence's top-left corner stays with its gutter");
    assert!(
        rule.matches('╭').count() == 1 && !rule.contains('‹'),
        "the top rule is cut without being marked or re-cornered: {rule:?}"
    );
    let floor = after
        .iter()
        .find(|row| row.starts_with(" ╰"))
        .expect("and so does the bottom-left one");
    assert!(
        floor.matches('╰').count() == 1 && !floor.contains('‹'),
        "and so is the bottom rule: {floor:?}"
    );
    // The label is chrome too: pinned whole rather than cut mid-word.
    assert!(
        rule.contains("rust "),
        "the fence keeps its language label: {rule:?}"
    );
    // And the code really did move, or this passes on a build where `→` does nothing.
    assert!(
        !after.iter().any(|row| row.contains("fn f() { let a =")),
        "the over-wide code scrolls under the gutter: {after:?}"
    );
    assert!(
        after.iter().any(|row| row.contains("ZEBRA")),
        "what the reader scrolled to is on screen: {after:?}"
    );
}

#[test]
fn stepping_to_a_match_scrolls_sideways_to_reach_it() {
    // The owner's report — "what is not clear to me how to navigate multiple matches" —
    // rested on this: `n` moved the counter and nothing else. A match in the over-wide
    // part of a table is off screen to the *right*, and jumping to it only scrolled
    // vertically, so a reader who pressed `n` saw an unchanged screen and concluded the
    // key did nothing. Reaching a match has to mean putting it where it can be read.
    let mut app = numbered_pager_at(NUMBERED_WIDE, 80, 16);
    assert!(app.hscroll_max() > 0, "the probe document must scroll");
    assert_eq!(app.hscroll(), 0, "and starts at the left edge");

    app.run_search("Ocelot");
    assert_eq!(app.search().len(), 1, "the probe token is found once");
    assert!(
        app.hscroll() > 0,
        "reaching an off-screen match scrolls sideways to it"
    );
    let rows = framed(&mut app, 80, 16);
    assert!(
        rows.iter().any(|row| row.contains("Ocelot")),
        "and the match is on screen afterwards: {rows:?}"
    );
}

#[test]
fn a_match_already_on_screen_does_not_move_the_page_sideways() {
    // The other half of the promise: revealing a match must not drag a page that was
    // already showing it, or every `n` inside one paragraph would jolt the text.
    let mut app = numbered_pager_at(NUMBERED_WIDE, 80, 16);
    app.run_search("Prose");
    assert_eq!(app.search().len(), 1);
    assert_eq!(
        app.hscroll(),
        0,
        "a match in the prose is already on screen at the left edge"
    );
}

#[test]
fn the_status_bar_advertises_the_match_keys_while_a_search_is_active() {
    // The report was a discoverability defect: `n` and `N` have been bound since the
    // beginning, and the author of the program could not find them. The counter says
    // matches exist; only the hint says what moves between them.
    let mut app = pager(SAMPLE);
    let bare = painted(80, 1, |buffer, area| {
        super::chrome::draw_status(buffer, area, &app)
    });
    assert!(
        !bare[0].contains("next/prev"),
        "no search, no hint: {:?}",
        bare[0]
    );

    app.run_search("needle");
    assert!(app.search().len() > 1, "the probe has several matches");
    let rows = painted(80, 1, |buffer, area| {
        super::chrome::draw_status(buffer, area, &app)
    });
    assert!(
        rows[0].contains("n/N next/prev"),
        "the bound keys are named beside the count: {:?}",
        rows[0]
    );
}

#[test]
fn the_modified_arrows_step_between_matches_like_the_letters() {
    // Bound at the owner's request for readers who do not reach for vi keys. They are
    // aliases of the same actions, so they must behave identically — including the wrap.
    let mut app = pager(SAMPLE);
    app.run_search("needle");
    assert_eq!(app.search().len(), 2);
    assert_eq!(app.search_index(), Some(0));

    let ctrl = |code| Key {
        code,
        mods: crate::config::KeyMods::CTRL,
    };
    assert_eq!(
        app.config().keys.action(&ctrl(KeyCode::Down)),
        Some(Action::NextMatch)
    );
    assert_eq!(
        app.config().keys.action(&ctrl(KeyCode::Up)),
        Some(Action::PrevMatch)
    );
    app.on_key(ctrl(KeyCode::Down));
    assert_eq!(app.search_index(), Some(1));
    app.on_key(ctrl(KeyCode::Up));
    assert_eq!(app.search_index(), Some(0));

    // And the plain arrows still scroll, or the alias would have stolen them.
    assert_eq!(
        app.config().keys.action(&Key::plain(KeyCode::Down)),
        Some(Action::LineDown)
    );
    assert_eq!(
        app.config().keys.action(&Key::plain(KeyCode::Up)),
        Some(Action::LineUp)
    );
}

#[test]
fn the_status_bar_offers_the_second_way_of_stepping_and_gives_it_up_first() {
    // The owner asked for both `n`/`N` and the modified arrows on the footer. Two chips,
    // not one sentence, precisely so the alias is what a narrow terminal loses: a reader
    // with `n/N next/prev` can still move, a reader with only `or Ctrl-↓/Ctrl-↑` beside
    // no words at all cannot tell what it is for.
    let mut app = pager(SAMPLE);
    app.run_search("needle");
    app.resize(100, 12);
    let wide = painted(100, 1, |buffer, area| {
        super::chrome::draw_status(buffer, area, &app)
    });
    assert!(
        wide[0].contains("n/N next/prev"),
        "the letters lead: {:?}",
        wide[0]
    );
    assert!(
        wide[0].contains("or Ctrl-↓/Ctrl-↑"),
        "and the arrows follow: {:?}",
        wide[0]
    );

    app.resize(76, 12);
    let narrow = painted(76, 1, |buffer, area| {
        super::chrome::draw_status(buffer, area, &app)
    });
    assert!(
        !narrow[0].contains("Ctrl-"),
        "the alias is the first thing given up: {:?}",
        narrow[0]
    );
    assert!(
        narrow[0].contains("n/N next/prev"),
        "the primary hint outlives it: {:?}",
        narrow[0]
    );
}

#[test]
fn the_help_overlay_lists_every_way_to_step_between_matches() {
    // The bar names two chords; the overlay is where the rest live, and a binding the
    // overlay omits is a binding nobody finds.
    let app = pager(SAMPLE);
    let sections = help::sections(&app.config().keys);
    let search = sections
        .iter()
        .find(|section| section.title == "Search")
        .expect("the help overlay has a search heading");
    let row = |description: &str| {
        search
            .rows
            .iter()
            .find(|row| row.description == description)
            .unwrap_or_else(|| panic!("`{description}` is listed: {search:?}"))
    };
    let next = &row("Go to the next match").keys;
    assert!(next.contains('n') && next.contains("Ctrl-↓"), "{next:?}");
    let prev = &row("Go to the previous match").keys;
    assert!(prev.contains('N') && prev.contains("Ctrl-↑"), "{prev:?}");
}

#[test]
fn the_match_key_hint_names_the_keys_the_reader_actually_bound() {
    // Every hint on this bar is generated from the live key table (design spec §10). A
    // reader who moved `next_match` to `>` must be told `>`, not the default nobody
    // bound.
    let mut config = Config::default();
    config.keys.unbind(&Key::char('n'));
    config.keys.unbind(&Key::char('N'));
    config.keys.bind(Key::char('>'), Action::NextMatch);
    config.keys.bind(Key::char('<'), Action::PrevMatch);
    let mut app = App::new(
        Doc::parse(SAMPLE),
        config,
        AppOptions {
            config_path: None,
            title: "sample.md".to_string(),
            icons: false,
            theme: "dark".to_string(),
            toc_open: false,
            width: None,
        },
    );
    app.resize(80, 12);
    let _ = app.canvas();
    app.run_search("needle");
    let rows = painted(80, 1, |buffer, area| {
        super::chrome::draw_status(buffer, area, &app)
    });
    assert!(
        rows[0].contains(">/< next/prev"),
        "the rebound keys are what the bar names: {:?}",
        rows[0]
    );
    assert!(
        !rows[0].contains("n/N"),
        "and the defaults nobody bound are not: {:?}",
        rows[0]
    );
}

/// A numbered fence and an over-wide table inside the *same list item*.
///
/// A container emits its children with no blank row between them, so the two blocks are
/// one contiguous non-blank run of canvas rows — the unit a prefix inferred from the
/// drawn canvas was spread over.
const NESTED_FENCE_AND_TABLE: &str = "\
- ```rust
  let a = 1 + 2;
  ```
  | aaaaaaaaaaaaaaaaaaaaaaaaa | bbbbbbbbbbbbbbbbbbbbbbbbb | ccccccccccccccccccccccccc | \
                                          ddddddddddddddddddddddddd |
  |--|--|--|--|
  | 1 | 2 | 3 | 4 |
";

#[test]
fn a_table_beside_a_fence_in_one_list_item_keeps_none_of_its_gutter() {
    // The prefix belongs to the block that has a gutter, and a table has none. Spread
    // over a contiguous run instead, the fence's four gutter columns were frozen at the
    // left edge of every row of the table below it: the header read `aaaa‹bbbb…`, text
    // that exists nowhere in the document, and the second column was unreachable.
    let mut app = numbered_pager_at(NESTED_FENCE_AND_TABLE, 80, 24);
    assert!(app.hscroll_max() > 0, "the probe document must scroll");
    assert_eq!(app.scroll(), 0, "the probe document fits vertically");
    let header = (0..app.rendered().height())
        .find(|&row| app.rendered().row_text(row).contains("aaaaaaaaaa"))
        .expect("the table's header row is drawn");
    assert_eq!(
        app.pinned()[header],
        0,
        "a table has no gutter to pin, whatever block shares its run: {:?}",
        app.rendered().row_text(header)
    );
    // The fence above it still has one, or this passes on a build that pins nothing.
    let code = (0..app.rendered().height())
        .find(|&row| app.rendered().row_text(row).contains("let a = 1 + 2;"))
        .expect("the fence's one code line is drawn");
    assert!(
        app.pinned()[code] > 0,
        "the numbered fence in the same run keeps its own gutter: {:?}",
        app.rendered().row_text(code)
    );

    for _ in 0..30 {
        app.act(Action::ScrollRight);
    }
    assert!(app.hscroll() > 0, "and must have been scrolled");
    let truth = app.rendered().row_text(header);
    let rows = framed(&mut app, 80, 24);
    // The document occupies every column but the scrollbar's, and the outermost one on
    // each side is the marker rail; what is left is document and nothing else.
    let drawn: String = rows[header].chars().skip(1).take(77).collect();
    assert!(
        truth.contains(drawn.trim_end()),
        "every column of a scrolled table row comes from one contiguous window of the \
         row itself.\n  drawn: {drawn:?}\n  row:   {truth:?}"
    );
}

#[test]
fn an_unnumbered_fence_pins_nothing_however_its_code_is_coloured() {
    // The detector used to rest on a style coincidence: `theme.code.line_number` is not
    // unique — both shipped themes paint `code.operator` in the very same value — so the
    // `=` in an *unnumbered* fence was read as a line number, the fence's right border
    // as the rule closing the gutter, and a prefix as wide as the block came back. It was
    // masked only by `Offsets` clamping a prefix wider than the viewport to zero, which
    // is one theme tweak or one `--width` away from not masking it.
    let doc = Doc::parse("```rust\nlet a = 1 + 2;\n```\n");
    let theme = crate::theme::Theme::default_dark();
    let canvas = crate::render::render_flat(
        &doc,
        40,
        &theme,
        // Line numbers *off*: this block has no gutter, so nothing may be pinned.
        &crate::render::RenderOptions::new(false, false),
    );
    let pinned = crate::render::document::pinned_prefix(&canvas);
    assert!(
        pinned.iter().all(|&prefix| prefix == 0),
        "a fence with no gutter pins nothing: {pinned:?}\n{}",
        canvas.plain_text()
    );
}

#[test]
fn a_numbered_fence_in_a_table_cell_pins_nothing() {
    // The other half of "a pin is a claim about the whole row". A fence in a cell draws a
    // real gutter and publishes a real pin, and the table then blits the cell into the
    // middle of a row it shares with the cells beside it — where the claim is false.
    // `Canvas::blit` drops it there, which is what keeps the first column of a table from
    // freezing because the second one holds numbered code.
    // A fence inside a cell is not something the parser will produce from markdown; the
    // renderer's own tests build it by putting a parsed block into a cell, and so does
    // this one.
    let outer = Doc::parse("| h | i |\n|---|---|\n| x | y |\n");
    let inner = Doc::parse("```rust\nlet x = 1;\nlet y = 2;\n```\n");
    let mut table = outer.root().children[0].clone();
    let row = table
        .children
        .iter_mut()
        .find(|node| matches!(node.kind, crate::doc::NodeKind::TableRow { header: false }))
        .expect("a body row");
    row.children[1].children = inner.root().children.clone();

    let theme = crate::theme::Theme::default_dark();
    let canvas = crate::render::render_block(
        &table,
        60,
        &theme,
        &crate::render::RenderOptions::new(false, true),
    );
    assert!(
        canvas.plain_text().contains("1 │ let x = 1;"),
        "the cell really does draw a numbered gutter:\n{}",
        canvas.plain_text()
    );
    let pinned = crate::render::document::pinned_prefix(&canvas);
    assert!(
        pinned.iter().all(|&prefix| prefix == 0),
        "nothing inside a table cell pins the row it shares: {pinned:?}\n{}",
        canvas.plain_text()
    );
}

#[test]
fn a_search_highlight_lands_on_its_match_in_a_pinned_block() {
    // `blit`, `edge_markers` and `highlight_matches` share one `Offsets` precisely so
    // they cannot disagree about where a row starts. A pinned prefix is a second thing
    // to agree about: if the highlight kept using the plain offset it would paint the
    // match `pinned` columns to the right of the text it belongs to.
    let mut app = numbered_pager_at(NUMBERED_WIDE, 80, 22);
    app.run_search("Ocelot");
    assert_eq!(app.search().len(), 1, "the probe token occurs once");

    // Scroll until the match is actually on screen; the merged run means the table
    // travels the fence's distance, so the exact number of presses is not a constant.
    // The status bar echoes the query, so only the document rows count.
    let on_screen = |rows: &[String]| rows[..21].iter().any(|row| row.contains("Ocelot"));
    let mut rows = framed(&mut app, 80, 22);
    for _ in 0..40 {
        if on_screen(&rows) {
            break;
        }
        app.act(Action::ScrollRight);
        rows = framed(&mut app, 80, 22);
    }
    assert!(app.hscroll() > 0, "the match was off to the right");
    assert!(on_screen(&rows), "the match is on screen: {rows:?}");
    // The same frame pins the fence's gutter, so this is the mixed case: one run, one
    // offset, and only the numbered block keeps its first columns.
    assert!(
        rows.iter().any(|row| row.starts_with(" │ 1 │")),
        "and the fence's gutter is pinned in that very frame: {rows:?}"
    );

    // The highlight is patched over whatever the cell already carried, so the marker is
    // its background rather than the whole style.
    let highlight = super::draw::term_style(app.theme().ui.search_current).bg;
    assert!(
        highlight.is_some(),
        "the current match is marked by a background"
    );
    let buffer = framed_buffer(&mut app, 80, 22);
    let mut painted = String::new();
    for y in 0..22 {
        for x in 0..80u16 {
            let cell = &buffer[(x, y)];
            if cell.style().bg == highlight {
                painted.push_str(cell.symbol());
            }
        }
    }
    assert_eq!(
        painted, "Ocelot",
        "the highlight covers the match and nothing else"
    );
}

#[test]
fn the_status_bar_offers_the_horizontal_readout_before_it_is_needed() {
    // A reader who is only shown `↔ 12/116` after already pressing the key has no way
    // to learn there was anything to the right in the first place.
    let app = pager_at(WIDE, 80, 20);
    assert_eq!(app.hscroll(), 0);
    let max = app.hscroll_max();
    assert!(max > 0);
    let rows = painted(80, 1, |buffer, area| {
        super::chrome::draw_status(buffer, area, &app)
    });
    assert!(
        rows[0].contains(&format!("0/{max}")),
        "the offset readout is shown at offset 0 too: {:?}",
        rows[0]
    );
}

#[test]
fn a_long_file_name_is_elided_before_the_horizontal_chip_is_dropped() {
    // The bar's own comment promises the file name "is elided before anything is
    // dropped and dropped only when eliding it to nothing is still not enough". The
    // elision ran *after* the drop loop, so on a narrow bar a long file name silently
    // cost the reader the `↔ n/N` chip — on exactly the terminal where horizontal
    // scrolling matters most, and with ten columns of the bar left empty.
    for title in ["a.md", "t_two_wide.md", "a-rather-long-file-name.md"] {
        let app = pager_named(WIDE, title, 40, 12);
        let max = app.hscroll_max();
        assert!(max > 0, "the probe document must scroll");
        let rows = painted(40, 1, |buffer, area| {
            super::chrome::draw_status(buffer, area, &app)
        });
        assert!(
            rows[0].contains(&format!("0/{max}")),
            "the horizontal chip outlives the file name at 40 columns with {title:?}: {:?}",
            rows[0]
        );
        assert!(
            rows[0].contains("h help"),
            "and the way out is still there: {:?}",
            rows[0]
        );
        assert_eq!(
            crate::text::display_width(&rows[0]),
            40,
            "the bar still fills the terminal exactly: {:?}",
            rows[0]
        );
        // The other half of the trade: the name is elided to keep the chip, not to keep
        // the meter, whose value is printed next to it in words.
        if title == "a.md" {
            assert!(
                rows[0].contains("a.md"),
                "a name this short is never elided at all: {:?}",
                rows[0]
            );
        }
    }
}

#[test]
fn the_meters_part_filled_cell_shares_the_troughs_exact_colour() {
    // Owner report: "the background color of the progressive bar char is not set to the
    // non-active bar color causing an odd effect", and then, once the cell had been
    // given a background: "the half chars ... not the same color as the chars
    // representing the empty part of the progressbar".
    //
    // An eighth block paints only the left fraction of its cell and lets the cell's own
    // background through on the right, so that background has to be the trough's
    // colour. It could not be, while the trough was `░` inked in `scrollbar_track.fg`:
    // a quarter-coverage dither shows a quarter of its ink mixed into the bar, so a
    // *background* painted `scrollbar_track.fg` landed about four times as heavy beside
    // it. The trough is now a flat colour, and both runs are handed the same one.
    //
    // This asserts styles, not glyphs: the bug never changed which characters appear,
    // so a test that read the row as text would pass with it present.
    for theme_name in ["dark", "light"] {
        let mut app = themed_pager(PAINTED, theme_name, 80, 10);
        let theme = app.theme().clone();
        let thumb = super::draw::term_style(theme.ui.scrollbar_thumb).fg;
        let bar_bg = super::draw::term_style(theme.ui.status_bar).bg;
        let surface = theme
            .ui
            .status_bar
            .bg
            .expect("the status bar has a background of its own");
        let track_ink = theme
            .ui
            .scrollbar_track
            .fg
            .expect("the track has a colour of its own");
        // What the track is meant to be: its ink laid over the bar at the coverage the
        // shade glyph used to have, so the gauge looks as it did but as a flat colour.
        let track = super::draw::term_style(
            crate::theme::Style::new()
                .bg(surface.blend(track_ink, crate::canvas::meter::TRACK_INK)),
        )
        .bg;
        assert_ne!(
            track, bar_bg,
            "{theme_name}: the test is only meaningful while the track is visible \
             against the bar"
        );
        assert_ne!(
            track,
            super::draw::term_style(crate::theme::Style::new().bg(track_ink)).bg,
            "{theme_name}: and while the track is laid over the bar rather than \
             painted in the neat ink colour, which is far heavier"
        );

        // Progress that lands on a cell boundary draws no partial cell at all, and the
        // scroll positions that produce one are not a constant, so walk the document.
        let mut seen = 0usize;
        let mut paired = 0usize;
        for _ in 0..app.max_scroll() + 1 {
            let area = Rect::new(0, 0, 80, 1);
            let mut buffer = Buffer::empty(area);
            super::chrome::draw_status(&mut buffer, area, &app);
            let rows = buffer_rows(&buffer, 80, 1);
            let mut troughs = 0usize;
            let mut partials = 0usize;
            for x in 0..80u16 {
                let cell = &buffer[(x, 0)];
                if crate::canvas::meter::EIGHTH_BLOCKS[1..8].contains(&cell.symbol()) {
                    partials += 1;
                    seen += 1;
                    assert_eq!(
                        cell.style().bg,
                        track,
                        "{theme_name}: the part-filled cell {:?} at column {x} is not \
                         on the same colour as the trough beside it: {:?}",
                        cell.symbol(),
                        rows[0]
                    );
                    assert_eq!(
                        cell.style().fg,
                        thumb,
                        "{theme_name}: and its filled fraction is still the thumb colour"
                    );
                    continue;
                }
                // A trough cell owes its whole appearance to its background, so it is
                // the cells carrying the track colour that count — and each must be
                // blank, because an inked glyph is the very thing a neighbouring
                // background cannot match.
                if cell.style().bg == track {
                    troughs += 1;
                    assert_eq!(
                        cell.symbol(),
                        crate::canvas::meter::TROUGH,
                        "{theme_name}: the track cell at column {x} is inked with a \
                         glyph, so its apparent colour is no longer its background: \
                         {:?}",
                        rows[0]
                    );
                }
            }
            // A value in the last cell leaves no trough at all, so a row without one
            // proves nothing; it is enough that some row put the two side by side.
            if partials > 0 && troughs > 0 {
                paired += 1;
            }
            app.act(Action::LineDown);
        }
        assert!(
            seen > 0,
            "{theme_name}: no scroll position drew a part-filled cell, so the \
             assertion above never ran"
        );
        assert!(
            paired > 0,
            "{theme_name}: no scroll position put a part-filled cell next to a \
             trough, which is the only arrangement where the seam is visible"
        );
    }
}

#[test]
fn going_to_the_top_also_goes_back_to_the_left_edge() {
    // There was no way home: `0` starts a count, and `g`, `Home`, `^` and `G` all
    // left the horizontal offset exactly where it was.
    for key in [Key::char('g'), Key::plain(KeyCode::Home)] {
        // Short enough that the document also scrolls vertically, so the assertion
        // that `g` went home is about both axes.
        let mut app = pager_at(WIDE, 80, 8);
        for _ in 0..8 {
            app.act(Action::ScrollRight);
        }
        app.act(Action::LineDown);
        assert!(app.hscroll() > 0 && app.scroll() > 0);

        app.on_key(key);
        assert_eq!(app.scroll(), 0, "{key:?} goes to the first row");
        assert_eq!(app.hscroll(), 0, "{key:?} goes to the first column too");
    }
}

/// A chart that fits an 80-column viewport.
const FITTING_FENCE: &str = "```mermaid\nflowchart LR\n    A[Read] --> B[Draw]\n```\n";

/// The chart from `tests/corpus/pipeline.mmd`, which needs 188 columns to draw.
const WIDE_FENCE: &str = concat!(
    "```mermaid\n",
    include_str!("../../tests/corpus/pipeline.mmd"),
    "```\n"
);

/// Renders a document the way the pager does, counting diagram layouts.
fn layouts_for(markdown: &str, width: u16) -> (crate::canvas::Canvas, usize) {
    let doc = Doc::parse(markdown);
    crate::render::bridge::counting_layouts(|| {
        crate::render::render_document(
            &doc,
            width,
            None,
            &crate::theme::Theme::default_dark(),
            &crate::render::RenderOptions::new(false, false),
        )
    })
}

#[test]
fn a_diagram_that_fits_is_laid_out_exactly_once() {
    // The seam returns the canvas as well as the width precisely so that this stays 1.
    // A seam that returned only the width would send every fitting diagram through the
    // engine twice — measured at +43 % startup on a diagram-heavy document — and no
    // output test would notice, because both layouts produce the same canvas.
    let (canvas, layouts) = layouts_for(FITTING_FENCE, 80);
    assert_eq!(canvas.width(), 80, "this chart fits; nothing to widen");
    assert_eq!(layouts, 1, "a fitting diagram was laid out {layouts} times");
}

#[test]
fn a_diagram_that_must_be_widened_is_laid_out_exactly_twice() {
    // One layout to learn it does not fit and what it needs, one at that width. The
    // clip hunt this replaced re-rendered the *source dump* several times over and
    // never drew the diagram at all.
    let (canvas, layouts) = layouts_for(WIDE_FENCE, 80);
    assert!(canvas.width() > 80, "the chart should have been widened");
    assert_eq!(layouts, 2, "a widened diagram was laid out {layouts} times");
}

#[test]
fn a_renderer_that_reports_no_floor_stays_inside_the_probe_cap() {
    // `pie` returns `needed: None`, so the search has nothing to aim at and doubles
    // instead. Without a probe cap this walks to the width cap one column at a time.
    let pie = "```mermaid\npie title Votes\n    \"Yes\" : 10\n    \"No\" : 3\n```\n";
    let (_, layouts) = layouts_for(pie, 16);
    assert_eq!(
        layouts, 2,
        "doubling should have found this pie on the second layout"
    );
}

/// Builds an app over `source` at an explicit size in a named theme.
fn themed_pager(source: &str, theme: &str, width: u16, height: u16) -> App {
    let mut app = App::new(
        Doc::parse(source),
        Config::default(),
        AppOptions {
            title: "sample.md".to_string(),
            icons: false,
            theme: theme.to_string(),
            toc_open: false,
            width: None,
            config_path: None,
        },
    );
    app.resize(width, height);
    let _ = app.canvas();
    app
}

/// A document with one of everything the painters treat differently.
const PAINTED: &str = "\
# Heading One

Plain prose with a **bold** word in it and more text after.

## Heading Two

| a | b |
| - | - |
| 1 | 2 |
| 3 | 4 |

```rust
fn main() { let x: Vec<i32> = vec![1]; }
```

---

Final paragraph.
";

#[test]
fn every_cell_of_every_frame_carries_the_theme_background() {
    // A pager that leaves cells on the terminal's own background is only readable by
    // luck: the reader's terminal may be the opposite polarity, in which case body
    // prose measures 1.4:1, and it is *irregularly* readable when the two backgrounds
    // merely differ, because the painted cells then show up as slabs.
    //
    // Written as a whole-frame sweep rather than as a check on one painter, because
    // every painter has to hold it and the ways to break it are not enumerable: a
    // widget rendered without a background, a blit that stops at the end of a canvas
    // row, an overlay that only paints where its text is. A frame is the object the
    // reader looks at, so a frame is what gets measured (lesson §4.5).
    //
    // This invariant is also the premise under which OSC 11 terminal-background
    // detection was *declined*: because every cell is opaque, a dark theme on a light
    // terminal is a mismatch of taste, not of legibility — the page is the theme's own
    // 13.3:1, not the 1.4:1 it would be if the cells were transparent. Detection would
    // buy politeness at the price of a probe that some terminals never answer. If this
    // test is ever relaxed, that trade-off has to be reopened with it.
    for theme in ["dark", "light"] {
        for (width, height) in [(40u16, 10u16), (80, 30), (140, 24)] {
            for overlay in [false, true] {
                let mut app = themed_pager(PAINTED, theme, width, height);
                if overlay {
                    app.act(Action::Help);
                }
                let buffer = framed_buffer(&mut app, width, height);
                for y in 0..height {
                    for x in 0..width {
                        let cell = &buffer[(x, y)];
                        assert!(
                            !matches!(cell.style().bg, None | Some(ratatui::style::Color::Reset)),
                            "{theme} {width}x{height} overlay={overlay}: cell ({x},{y}) \
                             {:?} has no background of its own",
                            cell.symbol()
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn the_help_overlay_paints_its_panel_behind_every_string() {
    // The panel is washed with `ui.status_bar`, but the styles it then draws in —
    // body text, section titles, the border — all carry the *page* background, which
    // is right everywhere else they are used and wrong here. The result was a run of
    // page background exactly as wide as each string: the overlay read as if its text
    // had been gone over with a marker pen.
    //
    // Located by colour rather than by geometry: the panel's own background marks
    // where the panel is, and nothing between its first and last column on a row may
    // be painted in anything else.
    for theme in ["dark", "light"] {
        for (width, height) in [(60u16, 20u16), (80, 30), (140, 24)] {
            let mut app = themed_pager(PAINTED, theme, width, height);
            app.act(Action::Help);
            let panel = super::draw::term_style(app.theme().ui.status_bar).bg;
            let buffer = framed_buffer(&mut app, width, height);
            let mut seen = 0usize;
            for y in 0..height.saturating_sub(1) {
                let row: Vec<_> = (0..width).map(|x| &buffer[(x, y)]).collect();
                let Some(first) = row.iter().position(|c| c.style().bg == panel) else {
                    continue;
                };
                let last = row
                    .iter()
                    .rposition(|c| c.style().bg == panel)
                    .unwrap_or(first);
                for (x, cell) in row.iter().enumerate().take(last + 1).skip(first) {
                    seen += 1;
                    assert_eq!(
                        cell.style().bg,
                        panel,
                        "{theme} {width}x{height}: ({x},{y}) {:?} sits inside the help \
                         panel on a foreign background",
                        cell.symbol()
                    );
                }
            }
            assert!(
                seen > 100,
                "{theme} {width}x{height}: the overlay must actually be on screen, \
                 only {seen} panel cells found"
            );
        }
    }
}
/// A code line whose every column holds a different character.
///
/// Printable ASCII from `!` to `~` is 94 distinct glyphs, none of them a space and none
/// of them a box-drawing glyph, so a frame capture says exactly which columns of the line
/// are on screen — which is what a claim about *losing* one has to be measured against.
fn ruler() -> String {
    (b'!'..=b'~').map(char::from).collect()
}

/// Walks `app` through every horizontal offset it has, checking each frame against the
/// canvas it is a window on, and returns the document text seen along the way.
///
/// The check is the whole of finding 3: for every viewport column that the shared
/// [`super::draw::Offsets`] maps to a column of the document, the terminal must be showing
/// *that* column of the document. The edge markers are excluded by construction rather
/// than by exception — they are painted in the rail, which is the one column on each side
/// the offsets do not map any document column to — so a marker that stands on a character
/// fails here whatever else it gets right.
///
/// Returns the set of canvas columns that were legible at some offset, per row, so a
/// caller can also ask the weaker question the reproduction started from: is every
/// character still reachable?
fn every_offset(app: &mut App, width: u16, height: u16) -> Vec<std::collections::BTreeSet<usize>> {
    let mut seen: Vec<std::collections::BTreeSet<usize>> = Vec::new();
    loop {
        let at = app.hscroll();
        let buffer = framed_buffer(app, width, height);
        let offsets = super::draw::Offsets::scrolled_to(
            app.reach(),
            app.pinned(),
            app.hscroll(),
            app.viewport_width(),
        );
        let canvas = app.rendered();
        let content = offsets.content();
        seen.resize(canvas.height(), std::collections::BTreeSet::new());
        for y in 0..height - 1 {
            let row = app.scroll() + usize::from(y);
            let Some(cells) = canvas.row(row) else { break };
            for x in 0..content {
                // The one column on each side that is chrome: the rail the left marker is
                // painted in sits just inside the pinned prefix, and the right rail is
                // past `content` already.
                if offsets.margin() > 0 && x + 1 == offsets.pinned(row) {
                    continue;
                }
                let column = offsets.column(row, x);
                let Some(cell) = cells.get(column) else { break };
                if cell.is_continuation() {
                    continue;
                }
                let expected = if cell.width() == 2 && x + 1 >= content {
                    " "
                } else {
                    seen[row].insert(column);
                    cell.text()
                };
                assert_eq!(
                    buffer[(x, y)].symbol(),
                    expected,
                    "at offset {at}, viewport column {x} of row {row} shows canvas column \
                     {column}; the frame reads {:?}",
                    buffer_rows(&buffer, width, height)[usize::from(y)]
                );
            }
        }
        if at >= app.hscroll_max() {
            return seen;
        }
        app.act(Action::ScrollRight);
        assert!(app.hscroll() > at, "the offset must advance");
    }
}

#[test]
fn an_edge_marker_never_stands_on_a_column_of_the_document() {
    // Finding 3, and the reproduction it came from: at 60 columns a fenced ruler line
    // scrolled to offset 8 rendered as `‹6789…`, when the first character of the window
    // is `5`. The chevron had been stamped into the first viewport column, over whatever
    // the document had put there — in real code, `hMap::new()` shown as `‹Map::new()`.
    //
    // Marking is not the only thing that has to be right here, so the assertion is the
    // general one, checked at every offset and every row: what the terminal shows is what
    // the canvas holds. `every_offset` is where it lives.
    let line = ruler();
    let mut app = pager_at(&format!("```text\n{line}\n```\n"), 60, 8);
    assert!(app.hscroll_max() > 0, "the probe document must scroll");
    let seen = every_offset(&mut app, 60, 8);

    // And the weaker property the reproduction was written as: every column of the line
    // is legible at *some* offset. Weaker on purpose — it was already true before the
    // fix, because the scroll step is smaller than the viewport, so a column destroyed at
    // one offset survives at the next. It is still the promise the pager makes, and it is
    // the one that breaks if the rail is ever widened without widening the reach with it.
    let canvas = app.rendered();
    let row = (0..canvas.height())
        .find(|&row| canvas.row_text(row).contains("!\"#$"))
        .expect("the ruler is on the canvas");
    let cells = canvas.row(row).expect("a row");
    let missing: Vec<&str> = cells
        .iter()
        .enumerate()
        .filter(|(column, cell)| {
            !cell.is_blank() && !cell.is_continuation() && !seen[row].contains(column)
        })
        .map(|(_, cell)| cell.text())
        .collect();
    assert!(
        missing.is_empty(),
        "every column of the line is legible at some offset; these never were: {missing:?}"
    );
}

// --- Mouse selection, and the source behind it ---------------------------------

use super::select::{self, Pos, Selection};
use crate::canvas::Canvas;

/// A document exercising every construct whose source differs from what is drawn.
const MARKUP: &str = "\
# Wide diagram

# A second level-one heading, so the lone-`#` title banner declines

The **bold** word and a [link](https://example.com/a) here.

- item one

```
fn main() {}
```

After the fence.
";

/// Where `needle` was drawn: its row, its first column and its width.
///
/// The whole point of the selection code is that a *cell rectangle* comes back as
/// source, so tests address cells the way a reader does — by pointing at what they can
/// see — rather than by quoting span offsets the implementation chose.
fn drawn(canvas: &Canvas, needle: &str) -> (usize, u16, u16) {
    for row in 0..canvas.height() {
        let text = canvas.row_text(row);
        if let Some(at) = text.find(needle) {
            let col = crate::text::display_width(&text[..at]);
            return (
                row,
                u16::try_from(col).expect("a test canvas is narrow"),
                u16::try_from(crate::text::display_width(needle)).expect("short needle"),
            );
        }
    }
    panic!("{needle:?} was never drawn:\n{}", canvas.plain_text());
}

/// Drags over `needle` exactly, and returns what the clipboard would have got.
fn drag_over(canvas: &Canvas, source: &str, needle: &str) -> select::Extract {
    let (row, col, cols) = drawn(canvas, needle);
    let selection = drag(Pos::new(row, col), Pos::new(row, col + cols - 1));
    select::extract(canvas, source, selection).expect("the drag covered something")
}

/// A finished drag between two canvas positions.
fn drag(from: Pos, to: Pos) -> Selection {
    let mut selection = Selection::started(from);
    selection.drag_to(to);
    selection.finish();
    selection
}

#[test]
fn a_drag_over_a_heading_yields_its_markdown() {
    let mut app = pager(MARKUP);
    let canvas = app.canvas().clone();
    assert_eq!(
        drag_over(&canvas, MARKUP, "Wide diagram").text,
        "# Wide diagram",
        "the `#` is markup the reader could not have dragged over, so it comes along"
    );
}

#[test]
fn a_drag_over_emphasis_brings_its_delimiters() {
    let mut app = pager(MARKUP);
    let canvas = app.canvas().clone();
    assert_eq!(drag_over(&canvas, MARKUP, "bold").text, "**bold**");
}

#[test]
fn a_drag_over_a_link_brings_its_target() {
    let mut app = pager(MARKUP);
    let canvas = app.canvas().clone();
    assert_eq!(
        drag_over(&canvas, MARKUP, "link").text,
        "[link](https://example.com/a)",
        "the rendered ` (url)` is synthesised, so the source target has to come from \
         the markup walk rather than from a span"
    );
}

#[test]
fn an_edge_marker_stays_off_the_code_beside_a_pinned_gutter() {
    // The pinned case, which is where the chevron cost a character of *code* rather than
    // a column of margin: the left marker deliberately sits at the edge of the scrolling
    // region rather than of the window, and the first column of the scrolling region is
    // the first column of the code. The rail is the blank one before it — the separator
    // `pinned_prefix` keeps between the gutter's rule and the code — so the marker still
    // marks the same seam and no longer eats the first character behind it.
    let mut app = numbered_pager_at(NUMBERED_WIDE, 80, 22);
    assert!(app.hscroll_max() > 0, "the probe document must scroll");
    every_offset(&mut app, 80, 22);
}

#[test]
fn only_the_rows_whose_content_is_cut_are_marked_as_cut() {
    // Finding 8. A widened fence carries its own wall down the far side of the canvas, so
    // *every* row of it has something past the viewport's edge and every row was marked
    // `›` — a blank line and a closing `}` claiming to be cut, in a column of chevrons
    // running between a `╮` and a `╯` that both said the box ended there. `--render-once`
    // has always had this right: the wall is drawn at the edge, and only the line that is
    // really too long gets the marker.
    let source = "```rust\nuse std::collections::HashMap;\n\nfn main() {\n    let mut m: \
                  HashMap<String, Vec<(usize, &'static str)>> = HashMap::new();\n}\n```\n";
    let mut app = pager_at(source, 80, 12);
    let rows = framed(&mut app, 80, 12);
    // The document's last column, which is the terminal's last but one: the scrollbar
    // has the one after it.
    let edge = |row: &str| row.chars().nth(78).expect("a full-width row");
    let ends_with = |needle: &str| {
        edge(
            rows.iter()
                .find(|row| row.contains(needle))
                .unwrap_or_else(|| panic!("{needle} is on screen: {rows:?}")),
        )
    };
    assert_eq!(
        ends_with("HashMap::new()"),
        '›',
        "the line that is actually cut is marked: {rows:?}"
    );
    for row in ["use std::collections::HashMap;", "fn main() {", "}"] {
        assert_eq!(
            ends_with(row),
            '│',
            "a row that fits keeps the fence's wall instead: {rows:?}"
        );
    }
    // The blank line inside the fence has no content of its own to find it by; it is the
    // row between the `fn main() {` row and the one before it.
    let blank = rows
        .iter()
        .position(|row| row.contains("fn main() {"))
        .expect("the fence is on screen")
        - 1;
    assert_eq!(
        edge(&rows[blank]),
        '│',
        "and so does the blank line: {:?}",
        rows[blank]
    );
    // The frame still closes on its rules, which is the behaviour this must not undo.
    assert!(
        rows.iter().any(|row| edge(row) == '╮') && rows.iter().any(|row| edge(row) == '╯'),
        "a cut rule still ends in its own corner: {rows:?}"
    );
}

#[test]
fn scrolling_is_still_per_run_while_marking_is_per_row() {
    // The tension named in `render::document::scroll_reach`: rows scroll as a *run* so a ragged block
    // does not shear, and the marking must become per row without touching that. A block
    // whose rows are of three different lengths moves as one piece — and says, row by
    // row, which of those rows still has something past the edge.
    let filler = "x".repeat(100);
    let source =
        format!("```text\n{filler}xxxxxxxxxx AEND\n{filler} BEND\n{filler}xxxxxxxxxx CEND\n```\n");
    let mut app = pager_at(&source, 40, 12);
    assert!(app.hscroll_max() > 0, "the probe document must scroll");
    while app.hscroll() < app.hscroll_max() {
        app.act(Action::ScrollRight);
    }
    let rows = framed(&mut app, 40, 12);
    let ends = |needle: &str| {
        rows.iter()
            .find(|row| row.contains(needle))
            .unwrap_or_else(|| panic!("{needle} is on screen: {rows:?}"))
            .chars()
            .nth(38)
            .expect("a full-width row")
    };
    // Scrolled to the end of the run, the two long rows have nothing left past the edge
    // and the short one has had nothing for a while; none of them may claim otherwise.
    for row in ["AEND", "BEND", "CEND"] {
        assert_ne!(
            ends(row),
            '›',
            "no row of a fully scrolled run is marked as cut: {rows:?}"
        );
    }
    // And they are still one piece: the ragged edge survived the trip.
    let column = |needle: &str| {
        rows.iter()
            .find_map(|row| row.find(needle))
            .expect("on screen")
    };
    assert_eq!(column("AEND"), column("CEND"));
    assert_eq!(column("BEND") + 10, column("AEND"));
}

/// A document long enough that the scrollbar thumb is at its two-half-cell minimum.
fn long_document(lines: usize) -> String {
    (0..lines)
        .map(|n| format!("line {n} of the very long sample document\n\n"))
        .collect()
}

/// The pagers the scrollbar tests are exercised against: a short document where the
/// thumb is fat and one scroll unit is many half-cells, and a long one where the thumb
/// is at its minimum and one half-cell is many scroll units. The two round in opposite
/// directions, which is where an inverse that is only approximately an inverse breaks.
fn scrollbar_pagers() -> Vec<(&'static str, App)> {
    let mut short = pager(&long_document(9));
    short.resize(80, 12);
    let _ = short.canvas();
    let mut tall = pager(&long_document(400));
    tall.resize(80, 12);
    let _ = tall.canvas();
    let mut narrow = pager(&long_document(400));
    narrow.resize(80, 5);
    let _ = narrow.canvas();
    // A tall terminal holding a document about twice its height: the one shape where
    // the thumb is fat *and* the document has more lines than the track has half-cells,
    // so the thumb can be asked to follow the pointer to half-cell precision.
    let mut medium = pager(&long_document(60));
    medium.resize(80, 60);
    let _ = medium.canvas();
    vec![
        ("short", short),
        ("tall", tall),
        ("narrow", narrow),
        ("medium", medium),
    ]
}

/// The track's height in cells: the body area, status bar excluded, as `draw` lays it.
fn bar_height(app: &App) -> u16 {
    u16::try_from(app.viewport_height()).expect("the viewport fits in a u16")
}

#[test]
fn a_press_on_the_scrollbar_track_puts_the_thumb_under_the_pointer() {
    // The real inverse property: whatever row the reader presses, the thumb the
    // painter then draws covers that row. Asserted against `scrollbar_thumb`, which
    // `draw::scrollbar` is the only other caller of, so this pins the mapping against
    // the drawing rather than against a restatement of the mapping.
    for (name, mut app) in scrollbar_pagers() {
        let height = bar_height(&app);
        assert!(
            app.max_scroll() > 0,
            "{name}: the sample must be scrollable"
        );
        for row in 0..height {
            app.scrollbar_press(height, row);
            let (start, length) = app.scrollbar_thumb(height);
            let top = usize::from(row) * 2;
            assert!(
                top < start + length && top + 1 >= start,
                "{name}: pressing row {row} of {height} left the thumb at \
                 {start}..{} (scroll {} of {})",
                start + length,
                app.scroll(),
                app.max_scroll(),
            );
            app.scrollbar_release();
        }
    }
}

#[test]
fn the_scrollbar_reaches_both_extremes_exactly() {
    for (name, mut app) in scrollbar_pagers() {
        let height = bar_height(&app);
        let max = app.max_scroll();

        app.scrollbar_press(height, height - 1);
        assert_eq!(
            app.scroll(),
            max,
            "{name}: a press on the last row is the end"
        );
        app.scrollbar_release();
        app.scrollbar_press(height, 0);
        assert_eq!(
            app.scroll(),
            0,
            "{name}: a press on the first row is the top"
        );
        app.scrollbar_release();

        // And by dragging, from every row the thumb could have been grabbed on.
        for row in 0..height {
            app.scroll_to(0);
            app.scrollbar_press(height, row);
            app.scrollbar_drag(height, height - 1);
            assert_eq!(
                app.scroll(),
                max,
                "{name}: dragging from row {row} to the bottom must land on \
                 max_scroll, not one line short",
            );
            app.scrollbar_drag(height, 0);
            assert_eq!(
                app.scroll(),
                0,
                "{name}: and dragging back to the top must land on 0",
            );
            app.scrollbar_release();
        }
    }
}

#[test]
fn grabbing_the_thumb_does_not_move_it() {
    for (name, mut app) in scrollbar_pagers() {
        let height = bar_height(&app);
        app.scroll_to(app.max_scroll() / 2);
        let before = app.scroll();
        let (start, length) = app.scrollbar_thumb(height);
        for top in start..start + length {
            let row = u16::try_from(top / 2).expect("the thumb is inside the track");
            app.scrollbar_press(height, row);
            assert_eq!(
                app.scroll(),
                before,
                "{name}: pressing row {row}, which the thumb covers, must not snap \
                 the thumb's top to the pointer",
            );
            app.scrollbar_release();
        }
    }
}

#[test]
fn dragging_the_thumb_tracks_the_pointer_without_drift() {
    for (name, mut app) in scrollbar_pagers() {
        let height = bar_height(&app);
        if height < 4 {
            continue;
        }
        app.scroll_to(app.max_scroll() / 2);
        let before = app.scroll();
        let (start, length) = app.scrollbar_thumb(height);
        let middle = u16::try_from((start + length / 2) / 2).expect("inside the track");

        app.scrollbar_press(height, middle);
        app.scrollbar_drag(height, middle + 1);
        let down = app.scroll();
        assert!(down > before, "{name}: dragging down must scroll down");

        // Back to where the pointer started, and the document must be back exactly —
        // an anchor rewritten on every drag event accumulates rounding and this is
        // where it shows.
        app.scrollbar_drag(height, middle);
        assert_eq!(app.scroll(), before, "{name}: no drift on the way back");

        // And the thumb goes down with the pointer, three rows for three rows, to
        // within the half-cell the painter rounds to. This is the gain: a drag that
        // applied the *track's* rate rather than the thumb's would move the document
        // and leave the thumb sliding out from under the finger holding it.
        // Only where the document has more scrollable lines than the thumb has
        // half-cells of travel. Below that the thumb moves in jumps of several
        // half-cells per line because there is nothing finer to move it by, and no
        // mapping can make it track a pointer more smoothly than the document allows.
        let travel = usize::from(height) * 2 - length;
        if middle + 3 < height - 1 && app.max_scroll() >= travel {
            app.scrollbar_drag(height, middle + 3);
            let (moved_start, _) = app.scrollbar_thumb(height);
            assert!(
                moved_start.abs_diff(start + 6) <= 1,
                "{name}: the pointer moved three rows and the thumb moved {} \
                 half-cells, not six",
                moved_start as i64 - start as i64,
            );
        }
        app.scrollbar_release();
    }
}

#[test]
fn the_scrollbar_grab_is_sticky_and_ends_on_release() {
    let mut app = pager(&long_document(400));
    app.resize(80, 12);
    let _ = app.canvas();
    let height = bar_height(&app);

    assert!(!app.scrollbar_grabbed(), "nothing is grabbed to begin with");
    app.scrollbar_press(height, 1);
    assert!(app.scrollbar_grabbed(), "a press on the track grabs");
    // The drag carries no column at all: there is nothing for the pointer straying
    // off the one-column bar to be tested against, which is the whole of stickiness.
    app.scrollbar_drag(height, height - 2);
    let moved = app.scroll();
    assert!(moved > 0);

    app.scrollbar_release();
    assert!(!app.scrollbar_grabbed());
    app.scrollbar_drag(height, 1);
    assert_eq!(app.scroll(), moved, "a drag after release must do nothing");
}

#[test]
fn a_resize_drops_a_scrollbar_grab() {
    let mut app = pager(&long_document(400));
    app.resize(80, 12);
    let _ = app.canvas();
    app.scrollbar_press(bar_height(&app), 3);
    assert!(app.scrollbar_grabbed());
    // The anchor is a row of a track that no longer exists, and a reflow has moved
    // every line under it.
    app.resize(80, 30);
    assert!(!app.scrollbar_grabbed(), "the anchor died with the layout");
}

#[test]
fn a_drag_over_a_list_item_brings_its_marker() {
    let mut app = pager(MARKUP);
    let canvas = app.canvas().clone();
    assert_eq!(drag_over(&canvas, MARKUP, "item one").text, "- item one");
}

#[test]
fn a_partial_drag_returns_the_covered_source_verbatim() {
    let mut app = pager(MARKUP);
    let canvas = app.canvas().clone();
    let (row, col, _) = drawn(&canvas, "bold");
    // The first three of the four cells of `bold`: the drag started at the run's edge,
    // so the opening `**` comes with it, and ended inside rendered text, so nothing is
    // invented on the right. `**bol` is exactly what was pointed at.
    assert_eq!(
        select::extract(
            &canvas,
            MARKUP,
            drag(Pos::new(row, col), Pos::new(row, col + 2))
        )
        .expect("covered")
        .text,
        "**bol"
    );
    // And a drag that starts inside the run picks up no delimiter at all.
    assert_eq!(
        select::extract(
            &canvas,
            MARKUP,
            drag(Pos::new(row, col + 1), Pos::new(row, col + 2))
        )
        .expect("covered")
        .text,
        "ol"
    );
}

#[test]
fn a_drag_over_a_code_block_reports_it_as_source() {
    let mut app = pager(MARKUP);
    let canvas = app.canvas().clone();
    // `render::code::code_area` now records one search span per drawn code line
    // (design spec §3), so a drag over a code line is no longer a spanless fallback to
    // the rendered cells: it inverts back onto the document source, the same as prose.
    let extract = drag_over(&canvas, MARKUP, "fn main() {}");
    assert_eq!(extract.text, "fn main() {}");
    assert!(
        extract.from_source,
        "code now has provenance and must be reported as source"
    );
}

#[test]
fn a_selection_over_code_yields_markdown_source() {
    let markdown = "```rust\nlet a = 1;\n```\n";
    let extract = extract_over_code(markdown);
    assert!(
        extract.from_source,
        "code now has provenance and must be reported as source"
    );
    assert!(
        extract.text.contains("let a = 1;"),
        "got {:?}",
        extract.text
    );
}

/// Renders `markdown`, drags across the drawn `let a = 1;` code line, and extracts.
fn extract_over_code(markdown: &str) -> select::Extract {
    let mut app = pager(markdown);
    let canvas = app.canvas().clone();
    drag_over(&canvas, markdown, "let a = 1;")
}

/// A press that lands on the drawing rather than on a label takes the diagram whole
/// (design spec §2.2, third case).
///
/// This test used to assert the opposite — that such a drag fell back to the drawn cells
/// with `from_source == false` — and that was the last see/get divergence left inside a
/// diagram: `resolve` found no hull, so the clipboard got `───` while the highlight
/// stayed empty. The reader copied something and saw nothing. The owner's ruling is that
/// a drag which starts outside a label area selects the entire diagram immediately, so
/// the clipboard and the wash both answer for the whole rectangle here. The negative case
/// this test used to carry for `Extract::from_source` moved to
/// [`a_drag_over_a_thematic_break_falls_back_to_what_is_drawn`], which is chrome that
/// belongs to no atom and so still has no mapping at all.
#[test]
fn a_press_on_a_diagrams_box_art_takes_the_diagram_whole() {
    let mut app = pager(FITTING_FENCE);
    let canvas = app.canvas().clone();
    // The row above the label is the top edge of both boxes: drawing, and nothing else.
    let (label, _, _) = drawn(&canvas, "Read");
    let edge = label - 1;
    let on_art = drag(Pos::new(edge, 0), Pos::new(edge, 6));
    let extract =
        select::extract(&canvas, FITTING_FENCE, on_art).expect("the drag covered drawn cells");
    assert!(
        extract.from_source,
        "the rectangle is one thing and it has a source range, got {:?}",
        extract.text
    );
    assert_eq!(
        extract.text,
        FITTING_FENCE.trim_end_matches('\n'),
        "the whole fenced block, opener and closer included"
    );
    // And the wash is the whole rectangle — the see/get half, which is why this test
    // changed at all. Pinned against the drag that crosses both labels, whose wash
    // `the_whole_diagram_wash_covers_its_box_art` already fixes to the rectangle and no
    // more: every row of the canvas has to light identically, or the two ways of asking
    // for the whole diagram disagree.
    let (row, from, _) = drawn(&canvas, "Read");
    let (_, to, cols) = drawn(&canvas, "Draw");
    let across = drag(Pos::new(row, from), Pos::new(row, to + cols - 1));
    for on in 0..canvas.height() {
        assert_eq!(
            select::highlighted_columns(&canvas, FITTING_FENCE, on_art, on),
            select::highlighted_columns(&canvas, FITTING_FENCE, across, on),
            "row {on} washes the same for a press on the art as for a drag across both boxes"
        );
    }
    assert!(
        !select::highlighted_columns(&canvas, FITTING_FENCE, on_art, edge).is_empty(),
        "the top edge lights up, or the equality above compares two empty washes"
    );
}

/// The other half of the contract the test above used to carry: `from_source` still says
/// `false` for content the renderer really never mapped, rather than having drifted to
/// always-true now that fences and labels both carry spans.
///
/// A thematic break is the case that is left. It is drawing with no source range of its
/// own, and — unlike a diagram's box art — it belongs to no atom, so nothing claims it.
#[test]
fn a_drag_over_a_thematic_break_falls_back_to_what_is_drawn() {
    let source = "Above it.\n\n---\n\nBelow it.\n";
    let mut app = pager(source);
    let canvas = app.canvas().clone();
    let (above, _, _) = drawn(&canvas, "Above it.");
    let (below, _, _) = drawn(&canvas, "Below it.");
    let rule = (above + 1..below)
        .find(|&row| canvas.row_text(row).trim_start().starts_with('─'))
        .expect("the thematic break is drawn as a rule");
    let text = canvas.row_text(rule);
    let from = u16::try_from(crate::text::display_width(
        &text[..text.len() - text.trim_start().len()],
    ))
    .expect("a test canvas is narrow");
    let extract = select::extract(
        &canvas,
        source,
        drag(Pos::new(rule, from), Pos::new(rule, from + 6)),
    )
    .expect("the drag covered drawn cells");
    assert!(
        !extract.from_source,
        "a rule has no source span; the pager must not claim it does, got {:?}",
        extract.text
    );
    assert!(
        extract.text.starts_with('─'),
        "what is copied is the drawing the reader pointed at, got {:?}",
        extract.text
    );
}

/// A diagram too wide for the viewport is laid out by `render::diagram` instead of by
/// `render::code`, and that second path has to rebase its spans too — it is a separate
/// call site, and handing it no mapping would lose every wide diagram's provenance
/// while every test on the fitting path stayed green.
#[test]
fn a_widened_diagram_maps_its_labels_back_to_the_document() {
    let mut app = pager(WIDE_FENCE);
    let canvas = app.canvas().clone();
    assert!(canvas.width() > 80, "this chart had to be widened");
    // The label is wrapped onto two rows in this chart, and each row names the bytes it
    // drew (design spec §2.2). This used to filter on the spans naming the whole label,
    // which is the rule the amendment removed; the filter is the label's `unit` now, and
    // the assertion is stronger than it was — each span's source is *exactly* the text
    // under it, which is the property every column walk in the selection depends on.
    let label = WIDE_FENCE
        .find("Parse Markdown")
        .expect("the fixture's label");
    let mapped: Vec<(String, &str)> = canvas
        .spans()
        .iter()
        .filter(|s| s.unit == Some((label, label + "Parse Markdown".len())))
        .map(|s| {
            let drawn: String = canvas
                .row_text(s.row)
                .chars()
                .skip(usize::from(s.col))
                .take(usize::from(s.cols))
                .collect();
            (
                drawn,
                WIDE_FENCE.get(s.source_start..s.source_end).unwrap_or(""),
            )
        })
        .collect();
    assert_eq!(
        mapped,
        vec![
            ("Parse".to_string(), "Parse"),
            ("Markdown".to_string(), "Markdown")
        ],
        "each span of the widened diagram sits on the source bytes it drew"
    );
}

/// A drag that stays inside one label copies the characters it went over, and no more
/// (design spec §2.2, owner ruling 2026-08-11).
#[test]
fn a_drag_over_part_of_a_label_copies_only_those_characters() {
    let mut app = pager(FITTING_FENCE);
    let canvas = app.canvas().clone();
    let (row, col, _) = drawn(&canvas, "Read");
    let half = drag(Pos::new(row, col), Pos::new(row, col + 1));
    assert_eq!(
        select::extract(&canvas, FITTING_FENCE, half)
            .expect("the drag covered a label")
            .text,
        "Re",
        "two characters dragged over, two characters copied"
    );
}

/// The other half: a label *is* source text, and a drag over one copies the Mermaid
/// that produced it rather than the drawn box (design spec §3) — that label, and no
/// more of the line it sits on.
///
/// This test used to pin `    A[Read] --> B[` as intended: `extend_over_markup` widened
/// the hull over every byte nothing drew, which in prose is a pair of asterisks and on a
/// Mermaid line is nearly the whole line. The highlight lit `Read` and the clipboard got
/// a token cut in half. Design spec §2.2 now says a diagram is atomic, so what is copied
/// here is exactly what lights up.
#[test]
fn a_drag_over_a_diagram_label_yields_the_mermaid_source() {
    let mut app = pager(FITTING_FENCE);
    let canvas = app.canvas().clone();
    let extract = drag_over(&canvas, FITTING_FENCE, "Read");
    assert!(
        extract.from_source,
        "a flowchart label now has provenance, got {:?}",
        extract.text
    );
    assert_eq!(
        extract.text, "Read",
        "one label, not the punctuation of the line it was written on"
    );
    // Half a label is half a label (design spec §2.2, amended after live testing; this
    // assertion used to read "Read" and pinned the box as the unit of selection).
    let (row, col, cols) = drawn(&canvas, "Read");
    let half = drag(Pos::new(row, col), Pos::new(row, col + 1));
    assert_eq!(
        select::extract(&canvas, FITTING_FENCE, half)
            .expect("the drag covered a label")
            .text,
        "Re"
    );
    // And the highlight agrees, exactly: the two cells dragged over and nothing beside
    // them. Asserted on the canvas, because the defect being fixed is the two
    // disagreeing (design spec §7). Both directions — that the dragged characters wash
    // *and* that the rest of the same label does not — because a wash that lit the whole
    // label would still pass an assertion that only looked at the start of the range.
    assert_eq!(
        select::highlighted_columns(&canvas, FITTING_FENCE, half, row),
        vec![col..col + 2],
        "the wash is the two characters the clipboard got, not the label"
    );
    // The whole label, dragged end to end, is still the whole label.
    let all = drag(Pos::new(row, col), Pos::new(row, col + cols - 1));
    assert_eq!(
        select::extract(&canvas, FITTING_FENCE, all)
            .expect("the drag covered a label")
            .text,
        "Read"
    );
    assert_eq!(
        select::highlighted_columns(&canvas, FITTING_FENCE, all, row),
        vec![col..col + cols]
    );
    // The narrowest drag the pager acts on at all is two cells — one is a click and
    // `App::end_selection` drops it — and taken at the label's *far* end it pins the
    // other endpoint: a hull that rounded up to the label would pass the test above.
    let tail = drag(Pos::new(row, col + cols - 2), Pos::new(row, col + cols - 1));
    assert_eq!(
        select::extract(&canvas, FITTING_FENCE, tail)
            .expect("the drag covered a label")
            .text,
        "ad"
    );
    assert_eq!(
        select::highlighted_columns(&canvas, FITTING_FENCE, tail, row),
        vec![col + cols - 2..col + cols],
        "and the first half of the label stays dark"
    );
}

/// A drag that leaves one label behind takes the diagram whole (design spec §2.2).
///
/// The clipboard gets the fenced block, opener and closer included — a truncation like
/// `    A[Read] --> B[` is not something a reader can paste anywhere.
#[test]
fn a_drag_across_two_labels_takes_the_whole_fenced_block() {
    let mut app = pager(FITTING_FENCE);
    let canvas = app.canvas().clone();
    let (row, from, _) = drawn(&canvas, "Read");
    let (_, to, cols) = drawn(&canvas, "Draw");
    let across = drag(Pos::new(row, from), Pos::new(row, to + cols - 1));
    let extract = select::extract(&canvas, FITTING_FENCE, across).expect("covered both boxes");
    assert!(extract.from_source);
    assert_eq!(
        extract.text,
        FITTING_FENCE.trim_end_matches('\n'),
        "the whole block, fences and all"
    );
}

/// A press on the drawing takes the diagram *in addition to* what the drag went on to
/// cover, not instead of it (design spec §2.2, third case).
///
/// The case that separates the two readings of "immediately": the button goes down on the
/// boxes' **bottom** edge and comes up in the prose below. Every label is above the press,
/// so §2.1 resolves that endpoint *forward* — the hull it produces lies entirely after the
/// diagram and does not overlap it. An implementation that only widened over the atoms the
/// hull already met would hand back the prose alone, and the reader would watch the
/// rectangle light up under the pointer while the clipboard held a sentence.
#[test]
fn a_press_on_box_art_that_drags_into_prose_takes_the_block_and_the_prose() {
    let source = concat!(
        "```mermaid\nflowchart LR\n    A[Read] --> B[Draw]\n```\n",
        "\nAfter the **fence**.\n"
    );
    let mut app = pager(source);
    let canvas = app.canvas().clone();
    let (label, _, _) = drawn(&canvas, "Read");
    let (below, at, cols) = drawn(&canvas, "After the fence.");
    let bottom = (label + 1..below)
        .find(|&row| canvas.row_text(row).contains('└'))
        .expect("the boxes have a bottom edge below their labels");
    let out = drag(Pos::new(bottom, 1), Pos::new(below, at + cols - 1));
    let extract = select::extract(&canvas, source, out).expect("covered");
    assert_eq!(
        extract.text,
        "```mermaid\nflowchart LR\n    A[Read] --> B[Draw]\n```\n\nAfter the **fence**.",
        "the block the press claimed, then the prose the drag reached"
    );
    assert!(
        !select::highlighted_columns(&canvas, source, out, label - 1).is_empty(),
        "the diagram's top edge is washed, though the drag never went near it"
    );
}

/// The **press** decides, not the drag's extent (design spec §2.2, third case).
///
/// The discriminating case: the button goes down on the arrow between the two boxes and
/// comes up inside `Read`, to its left. The resolved hull of that drag lies wholly inside
/// `Read`'s source range — §2.1 resolves the arrow to the end of the label before it — so
/// every rule stated on the hull, and any rule keyed on the drag's *first* cell in
/// document order, answers `"Read"`. Only asking where the button went down gives the
/// diagram. If this test ever agrees with
/// [`a_drag_over_a_diagram_label_yields_the_mermaid_source`], the anchor has stopped
/// being consulted.
#[test]
fn a_press_on_box_art_takes_the_diagram_even_when_it_ends_in_a_label() {
    let mut app = pager(FITTING_FENCE);
    let canvas = app.canvas().clone();
    let (row, col, _) = drawn(&canvas, "Read");
    let text = canvas.row_text(row);
    let arrow = text.find('▶').expect("the boxes are joined by an arrow");
    let arrow =
        u16::try_from(crate::text::display_width(&text[..arrow])).expect("a test canvas is narrow");
    assert!(arrow > col, "the arrow sits to the right of the label");
    // Anchor on the arrow, head back inside the label: a *reversed* drag, so the earlier
    // of the two positions is the label and only the press is on the art.
    let back = drag(Pos::new(row, arrow), Pos::new(row, col + 1));
    let extract = select::extract(&canvas, FITTING_FENCE, back).expect("covered drawn cells");
    assert_eq!(
        extract.text,
        FITTING_FENCE.trim_end_matches('\n'),
        "the press was on the drawing, so the whole block comes, wherever it was released"
    );
    let ranges = select::highlighted_columns(&canvas, FITTING_FENCE, back, row);
    assert!(
        ranges.iter().any(|range| range.contains(&arrow)),
        "the wash covers the arrow the press landed on, got {ranges:?}"
    );
}

/// A diagram inside a container copies as clean Mermaid: **no** line keeps the container
/// prefix (design spec §2.2).
///
/// The owner's ruling. What a reader copies a diagram *for* is to paste it somewhere
/// else, and `> ```mermaid` pastes as a block quote containing a fence. The block's
/// recorded extent begins at the backticks, so the prefix was already absent from line one
/// and present on every line after it; the fix takes it off the rest rather than putting
/// it back on the first.
///
/// Three containers in one test, because the reason to read the prefix out of the document
/// rather than match `> ` is that there is no single prefix to match: a quote is `> `, a
/// nested quote `> > `, and a fence in a list item is indented instead. All three fall out
/// of the one rule, and the fence lines are stripped along with the content — the opener
/// and the closer are source lines and carry the prefix too.
#[test]
fn a_diagram_in_a_container_copies_without_its_container_prefix() {
    let clean = "```mermaid\nflowchart LR\n    A[Read] --> B[Draw]\n```";
    for (container, source) in [
        (
            "a block quote",
            "> ```mermaid\n> flowchart LR\n>     A[Read] --> B[Draw]\n> ```\n",
        ),
        (
            "a nested block quote",
            "> > ```mermaid\n> > flowchart LR\n> >     A[Read] --> B[Draw]\n> > ```\n",
        ),
        (
            "a list item",
            "- item\n\n  ```mermaid\n  flowchart LR\n      A[Read] --> B[Draw]\n  ```\n",
        ),
    ] {
        let mut app = pager(source);
        let canvas = app.canvas().clone();
        let (row, from, _) = drawn(&canvas, "Read");
        let (_, to, cols) = drawn(&canvas, "Draw");
        let across = drag(Pos::new(row, from), Pos::new(row, to + cols - 1));
        let extract = select::extract(&canvas, source, across).expect("covered both boxes");
        // Whole-block equality, not a check on line one: the defect being fixed lived on
        // every line *but* the first, and the one before it lived only on the first.
        assert_eq!(
            extract.text, clean,
            "a diagram in {container} copies as if it had been written at the top level"
        );
        for line in extract.text.lines() {
            assert!(
                !line.starts_with('>'),
                "no line copied out of {container} keeps a quote marker, got {:?}",
                extract.text
            );
        }
    }
}

/// The prefix comes off the diagram and off nothing else.
///
/// A drag that leaves a quoted diagram for the quoted prose below it: the block is
/// stripped because it is an atom, and the prose is not, because the reader selected prose
/// and decision 1 takes prose verbatim. An implementation that stripped the whole range
/// once it saw an atom in it would quietly rewrite the paragraph too.
#[test]
fn the_prefix_comes_off_the_diagram_and_not_off_the_prose_beside_it() {
    let source = "> ```mermaid\n> flowchart LR\n>     A[Read] --> B[Draw]\n> ```\n>\n> After it.\n";
    let mut app = pager(source);
    let canvas = app.canvas().clone();
    let (row, from, _) = drawn(&canvas, "Read");
    let (below, at, cols) = drawn(&canvas, "After it.");
    let out = drag(Pos::new(row, from), Pos::new(below, at + cols - 1));
    let extract = select::extract(&canvas, source, out).expect("covered");
    assert_eq!(
        extract.text, "```mermaid\nflowchart LR\n    A[Read] --> B[Draw]\n```\n>\n> After it.",
        "the block clean, the prose exactly as the file has it"
    );
}

/// The prefix comes off the **opener** too, when the press lands on the drawing.
///
/// The drag-shape axis, which every other prefix test here misses: they all drag from one
/// label to another, and that is the one shape whose hull starts *inside* a label, so the
/// range's first byte lands exactly on the block's recorded start and the opener's `> `
/// is never in the range to begin with. Press on the box art instead — design spec §2.2's
/// third case, and the commonest gesture on a diagram — and the range starts at the
/// opener's *line*, prefix included. Two reviewers found this independently on
/// `3a3dedb`, where it copied `> ```mermaid` as line one: a quote containing a fence,
/// which is exactly what the owner's ruling was made to prevent.
#[test]
fn a_press_on_box_art_in_a_quote_copies_the_opener_without_its_prefix() {
    let source = "> ```mermaid\n> flowchart LR\n>     A[Read] --> B[Draw]\n> ```\n";
    let mut app = pager(source);
    let canvas = app.canvas().clone();
    // The row above the labels is the boxes' top edge: drawing, and nothing else.
    let (label, _, _) = drawn(&canvas, "Read");
    let (edge, left, _) = drawn(&canvas, "┌");
    assert_eq!(
        edge,
        label - 1,
        "the top edge sits directly above the labels"
    );
    let on_art = drag(Pos::new(edge, left), Pos::new(edge, left.saturating_add(2)));
    let extract = select::extract(&canvas, source, on_art).expect("the drag covered drawn cells");
    assert_eq!(
        extract.text, "```mermaid\nflowchart LR\n    A[Read] --> B[Draw]\n```",
        "a press on the drawing copies the block as cleanly as a drag across its labels"
    );
    for line in extract.text.lines() {
        assert!(
            !line.starts_with('>'),
            "no line keeps a quote marker, opener included, got {:?}",
            extract.text
        );
    }
}

/// The mirror of [`the_prefix_comes_off_the_diagram_and_not_off_the_prose_beside_it`]:
/// quoted prose *above*, dragged down into the diagram.
///
/// The other drag shape that puts the opener's line — prefix and all — inside the range.
/// The prose keeps its `> `, because the reader selected prose and decision 1 takes prose
/// verbatim; the block does not, because it is an atom. On `3a3dedb` the boundary between
/// the two fell one line late, and the clipboard held `> ```mermaid` followed by stripped
/// content: unpasteable read either way.
#[test]
fn a_drag_from_the_quoted_prose_above_strips_the_diagrams_opener() {
    let source =
        "> Before it.\n>\n> ```mermaid\n> flowchart LR\n>     A[Read] --> B[Draw]\n> ```\n";
    let mut app = pager(source);
    let canvas = app.canvas().clone();
    let (above, at, _) = drawn(&canvas, "Before it.");
    let (row, to, cols) = drawn(&canvas, "Draw");
    let down = drag(Pos::new(above, at), Pos::new(row, to + cols - 1));
    let extract = select::extract(&canvas, source, down).expect("covered");
    assert_eq!(
        extract.text, "> Before it.\n>\n```mermaid\nflowchart LR\n    A[Read] --> B[Draw]\n```",
        "the prose exactly as the file has it, the block as if it were at the top level"
    );
}

/// A container prefix is recognised **per line**, not sampled once from the opener.
///
/// `CommonMark` does not require every line of a block quote to carry the same bytes: `>`
/// with no space after it is the same marker as `> `, and a blank quoted line is a bare
/// `>` — which is what `CommonMark` itself produces for one. An implementation that reads
/// one prefix from line one and then requires every later line to *start with that exact
/// string* renders all three of these correctly and copies them wrong, which is the
/// see/get divergence this module exists to remove. Every fixture below is legal
/// `CommonMark` and renders the same diagram as a plain quoted one.
///
/// The remedy the owner's ruling names is comrak's own prefix-stripping, checked back
/// against the document: the block's content is matched line by line as a *suffix* of its
/// source line, so a line whose prefix does not look like any other line's is still
/// stripped exactly, and a line that cannot be located is left alone rather than mangled.
#[test]
fn a_quoted_diagrams_prefix_is_read_line_by_line() {
    let clean = "```mermaid\nflowchart LR\n    A[Read] --> B[Draw]\n```";
    for (shape, source, want) in [
        (
            "a quote marker with no space after it",
            "> ```mermaid\n>flowchart LR\n>     A[Read] --> B[Draw]\n> ```\n",
            clean,
        ),
        (
            "a bare quote marker on a blank line",
            "> ```mermaid\n> flowchart LR\n>\n>     A[Read] --> B[Draw]\n> ```\n",
            "```mermaid\nflowchart LR\n\n    A[Read] --> B[Draw]\n```",
        ),
        (
            "an opener with no space and a body with one",
            ">```mermaid\n> flowchart LR\n>     A[Read] --> B[Draw]\n> ```\n",
            clean,
        ),
    ] {
        let mut app = pager(source);
        let canvas = app.canvas().clone();
        let (row, from, _) = drawn(&canvas, "Read");
        let (_, to, cols) = drawn(&canvas, "Draw");
        let across = drag(Pos::new(row, from), Pos::new(row, to + cols - 1));
        let extract = select::extract(&canvas, source, across).expect("covered both boxes");
        assert_eq!(
            extract.text, want,
            "a diagram written with {shape} copies as clean Mermaid"
        );
        for line in extract.text.lines() {
            assert!(
                !line.starts_with('>'),
                "no line copied from {shape} keeps a quote marker, got {:?}",
                extract.text
            );
        }
        // The third fixture's failure mode is not a marker but what it *leaves behind*:
        // a prefix sampled as `>` takes one byte off a `> ` line, so every content line
        // comes back indented by one space. A `starts_with('>')` loop cannot see that,
        // and neither can it see a closer left half-stripped, so the fence is pinned
        // here as the fence it has to still be.
        assert!(
            extract.text.ends_with("\n```"),
            "the closing fence is a fence, not an indented line, got {:?}",
            extract.text
        );
    }
}

/// The other half of reading the prefix per line: a line the parser and the document
/// **disagree** about comes back exactly as the document has it.
///
/// A tab-indented quoted diagram is the case that exists today. comrak expands the tab
/// when it strips the container, so its content is no longer a suffix of the source line
/// and no prefix can be established for that line. Two ways to go: emit comrak's text
/// anyway, which puts on the clipboard bytes that appear nowhere in the file, or leave
/// the line alone. This asserts the second, and asserts it byte for byte — a line the
/// reader can still read beats a line quietly rewritten, which is the same call
/// `doc::convert::code_lines` makes when it cannot locate a line at all.
///
/// The tab case is *not* fixed here and this test does not pretend it is: the fences come
/// off, the content lines keep their `>` and their tab. What it pins is the direction of
/// the degradation, so that "check comrak's answer against the document" cannot quietly
/// become "trust it".
#[test]
fn a_line_the_parser_cannot_locate_is_copied_as_the_document_has_it() {
    let source = "> ```mermaid\n>\tflowchart LR\n>\t    A[Read] --> B[Draw]\n> ```\n";
    let mut app = pager(source);
    let canvas = app.canvas().clone();
    assert!(
        canvas.spans().is_empty(),
        "comrak's tab expansion costs this block every span it might have had, which is \
         what puts every drag over it on the press-on-drawing path"
    );
    let (edge, left, _) = drawn(&canvas, "┌");
    let on_art = drag(Pos::new(edge, left), Pos::new(edge, left.saturating_add(2)));
    let extract = select::extract(&canvas, source, on_art).expect("the drag covered drawn cells");
    assert_eq!(
        extract.text, "```mermaid\n>\tflowchart LR\n>\t    A[Read] --> B[Draw]\n```",
        "the two lines that could not be located are the document's own bytes, tab and \
         marker included; nothing was invented for them"
    );
    for line in extract.text.lines() {
        assert!(
            source.contains(line),
            "every copied line is a line the file actually has, got {line:?}"
        );
    }
}

/// The wash for a whole diagram covers its box art, and that is deliberate.
///
/// Chrome never highlights anywhere else in this pager (design spec §2), and a test that
/// only looked at the label cells would pass just as happily against an implementation
/// that had quietly kept that rule here — leaving the reader a highlight over two words
/// and a clipboard holding forty bytes of Mermaid.
#[test]
fn the_whole_diagram_wash_covers_its_box_art() {
    let mut app = pager(FITTING_FENCE);
    let canvas = app.canvas().clone();
    let (row, from, _) = drawn(&canvas, "Read");
    let (_, to, cols) = drawn(&canvas, "Draw");
    let across = drag(Pos::new(row, from), Pos::new(row, to + cols - 1));
    // The top edge: a row of pure drawing, one row above the labels.
    let edge = row - 1;
    let corner = canvas
        .row_text(edge)
        .find('┌')
        .expect("the boxes have a top-left corner");
    let corner = u16::try_from(crate::text::display_width(&canvas.row_text(edge)[..corner]))
        .expect("a test canvas is narrow");
    let ranges = select::highlighted_columns(&canvas, FITTING_FENCE, across, edge);
    assert!(
        ranges.iter().any(|range| range.contains(&corner)),
        "the box's corner at column {corner} is washed too, got {ranges:?}"
    );
    // And the arrow between the boxes, on the label row: the interior of the rectangle
    // is filled, not just the two words in it.
    let arrow = canvas
        .row_text(row)
        .find('▶')
        .expect("the boxes are joined by an arrow");
    let arrow = u16::try_from(crate::text::display_width(&canvas.row_text(row)[..arrow]))
        .expect("a test canvas is narrow");
    let ranges = select::highlighted_columns(&canvas, FITTING_FENCE, across, row);
    assert!(
        ranges.iter().any(|range| range.contains(&arrow)),
        "the arrow at column {arrow} is washed too, got {ranges:?}"
    );
    // Solid, but only over the diagram: the blank margin the layout was handed is not
    // part of the drawing and washing it would read as a highlight bug.
    let drawn_to = u16::try_from(crate::text::display_width(canvas.row_text(row).trim_end()))
        .expect("a test canvas is narrow");
    assert!(
        canvas.width() > drawn_to + 8,
        "this canvas has margin to spare, or the assertion below proves nothing"
    );
    assert_eq!(
        ranges.iter().map(|range| range.end).max(),
        Some(drawn_to),
        "the wash stops where the diagram does, got {ranges:?}"
    );
}

/// A wrapped label draws on several rows, and a drag over one of them copies that row.
///
/// This test used to assert the opposite — `Parse Markdown`, the whole label, whichever
/// row was dragged over — because a label was the unit of selection. Design spec §2.2 was
/// amended after live testing and the rows now name their own bytes; what still has to
/// hold, and is the reason the test exists, is that several rows of one label are not
/// read as a drag across two boxes, which would copy the entire chart. That is now the
/// spans' shared `unit`, not their shared range.
#[test]
fn a_drag_over_one_row_of_a_wrapped_label_copies_that_row() {
    let mut app = pager(WIDE_FENCE);
    let canvas = app.canvas().clone();
    let label = WIDE_FENCE
        .find("Parse Markdown")
        .expect("the fixture's label");
    let wrapped = canvas
        .spans()
        .iter()
        .filter(|span| span.unit == Some((label, label + "Parse Markdown".len())))
        .count();
    assert_eq!(
        wrapped, 2,
        "this label has to be wrapped for the test to mean anything"
    );
    assert_eq!(
        drag_over(&canvas, WIDE_FENCE, "Markdown").text,
        "Markdown",
        "the row that was dragged over, not the label it belongs to"
    );
    assert_eq!(
        drag_over(&canvas, WIDE_FENCE, "Parse").text,
        "Parse",
        "and the same for the other row"
    );
    // Across both rows: the hull runs from one to the other and the space between them —
    // a space that *is* in the source, since this label wraps rather than breaking — is
    // between the ends of the hull and comes along (design spec §2, decision 1).
    let (top, from, _) = drawn(&canvas, "Parse");
    let (bottom, to, cols) = drawn(&canvas, "Markdown");
    assert_eq!(top + 1, bottom, "the two rows are consecutive");
    let both = drag(Pos::new(top, from), Pos::new(bottom, to + cols - 1));
    let extract = select::extract(&canvas, WIDE_FENCE, both).expect("the drag covered a label");
    assert_eq!(
        extract.text, "Parse Markdown",
        "still one label: two rows of it are not two boxes"
    );
    assert!(
        !extract.text.contains("```"),
        "and emphatically not the whole block, got {:?}",
        extract.text
    );
}

/// Partial selection inside a *wrapped* label — the case that would silently do the
/// wrong thing, because until this task every row of one named the whole label and no
/// column arithmetic inside a row could be right.
#[test]
fn a_drag_over_part_of_a_wrapped_labels_row_copies_only_those_characters() {
    let mut app = pager(WIDE_FENCE);
    let canvas = app.canvas().clone();
    let (row, col, cols) = drawn(&canvas, "Markdown");
    let (top, parse, parse_cols) = drawn(&canvas, "Parse");
    assert_eq!(top + 1, row, "the label wraps onto the row below `Parse`");
    let half = drag(Pos::new(row, col), Pos::new(row, col + 4));
    assert_eq!(
        select::extract(&canvas, WIDE_FENCE, half)
            .expect("the drag covered a label")
            .text,
        "Markd",
        "five characters of the second row, and not the label they belong to"
    );
    // Both directions: what was dragged over washes, and what was not stays dark —
    // including the row above, which shares the label and would light up under any rule
    // that still answered with the whole of it.
    assert_eq!(
        select::highlighted_columns(&canvas, WIDE_FENCE, half, row),
        vec![col..col + 5],
        "the wash is the five characters, not the eight of the row"
    );
    assert_eq!(
        select::highlighted_columns(&canvas, WIDE_FENCE, half, top),
        Vec::new(),
        "and nothing at all on the row above"
    );
    // The whole of one row, exactly: its own word, still not the label.
    let all = drag(Pos::new(row, col), Pos::new(row, col + cols - 1));
    assert_eq!(
        select::extract(&canvas, WIDE_FENCE, all)
            .expect("the drag covered a label")
            .text,
        "Markdown"
    );
    assert_eq!(
        select::highlighted_columns(&canvas, WIDE_FENCE, all, top),
        Vec::new(),
        "the row above is still dark"
    );
    let above = drag(Pos::new(top, parse), Pos::new(top, parse + parse_cols - 1));
    assert_eq!(
        select::highlighted_columns(&canvas, WIDE_FENCE, above, row),
        Vec::new(),
        "and the same the other way round"
    );
}

/// A label is as far as a confined drag can reach, and the Mermaid around it is not
/// picked up on the way out.
///
/// `extend_over_markup` (design spec §2, decision 2) widens a hull over every byte no
/// span drew, which on a Mermaid line is `A[`, the arrow and half the next box. The
/// confined case must not run it, and the way to see that it does not is to drag right
/// up to a label's edge — where the widening would start — and out onto the box art
/// beside it, which §2.1 resolves back to the label's own end.
#[test]
fn a_drag_to_the_edge_of_a_label_stops_at_the_label() {
    let mut app = pager(FITTING_FENCE);
    let canvas = app.canvas().clone();
    let (row, col, cols) = drawn(&canvas, "Read");
    for reach in 0..3u16 {
        let selection = drag(Pos::new(row, col), Pos::new(row, col + cols - 1 + reach));
        assert_eq!(
            select::extract(&canvas, FITTING_FENCE, selection)
                .expect("the drag covered a label")
                .text,
            "Read",
            "dragging {reach} columns past the label picked up the Mermaid around it"
        );
    }
    // And from the other side: a drag that starts on the box art *left* of the label is
    // a press outside every label, which is the third case and takes the diagram whole.
    let outside = drag(Pos::new(row, col - 1), Pos::new(row, col + 1));
    assert_eq!(
        select::extract(&canvas, FITTING_FENCE, outside)
            .expect("the drag covered the diagram")
            .text,
        FITTING_FENCE.trim_end_matches('\n'),
        "a press on the drawing takes the block, wherever it is released"
    );
}

/// A `<br>` in a label is markup between two drawn rows, and a drag across both of them
/// takes what lies between their ends — the `<br>` included.
///
/// Decision 1's hull, unqualified: nothing between the ends of a selection is dropped,
/// which is the same rule that puts a code fence's own fence lines on the clipboard when
/// a drag crosses one. The rows themselves name only what they drew, so dragging either
/// one alone gives that one word.
///
/// The fixture pads the `<br>`, which is the shape where the padding is *inside* the
/// label's own source text rather than trimmed off by the parser before it gets there:
/// each drawn line is trimmed of it, and a mapping that forgot would slide every run of
/// the second line one byte left.
#[test]
fn a_drag_across_an_explicit_line_break_in_a_label_keeps_it() {
    let source = "```mermaid\nflowchart LR\n    A[One <br> Two] --> B[End]\n```\n";
    let mut app = pager(source);
    let canvas = app.canvas().clone();
    let (top, one, _) = drawn(&canvas, "One");
    let (bottom, two, two_cols) = drawn(&canvas, "Two");
    assert_eq!(top + 1, bottom, "the label draws on two rows");
    assert_eq!(drag_over(&canvas, source, "One").text, "One");
    assert_eq!(drag_over(&canvas, source, "Two").text, "Two");
    let both = drag(Pos::new(top, one), Pos::new(bottom, two + two_cols - 1));
    assert_eq!(
        select::extract(&canvas, source, both)
            .expect("the drag covered a label")
            .text,
        "One <br> Two",
        "the label as written, which is what the reader dragged across"
    );
}

/// An entity in a label is one cell drawn by five bytes, and the selection has to answer
/// with the bytes.
///
/// `&amp;` is the only span in a diagram whose source is not a copy of its cells, and it
/// is cut out into a span of its own precisely so that the text either side of it stays a
/// copy of its own — otherwise every column inside that label would resolve to a byte a
/// few places out, and a reader dragging over `draw` would get `mp; d`.
#[test]
fn a_drag_over_an_entity_in_a_label_copies_the_reference() {
    let source = "```mermaid\nflowchart LR\n    A[Parse &amp; draw] --> B[End]\n```\n";
    let mut app = pager(source);
    let canvas = app.canvas().clone();
    assert_eq!(
        drag_over(&canvas, source, "draw").text,
        "draw",
        "the text after the entity resolves to its own bytes"
    );
    assert_eq!(drag_over(&canvas, source, "Parse").text, "Parse");
    let (row, col, _) = drawn(&canvas, "Parse & draw");
    let entity = drag(Pos::new(row, col + 6), Pos::new(row, col + 6));
    assert_eq!(
        select::extract(&canvas, source, entity)
            .expect("the drag covered a label")
            .text,
        "&amp;",
        "the one cell the entity drew answers with the whole reference"
    );
    assert_eq!(
        drag_over(&canvas, source, "Parse & draw").text,
        "Parse &amp; draw",
        "and the whole label is the label as written"
    );
    // A label *ending* in the entity, because that is the only shape in which the
    // entity's own end is the end of the hull. Anywhere else the next run's start
    // answers for it, and a span claiming one byte of `&amp;` instead of five gives the
    // same clipboard as a correct one — which it does above, and which is why the run
    // rule is pinned at the layout level as well.
    let trailing = "```mermaid\nflowchart LR\n    A[Parse &amp;] --> B[End]\n```\n";
    let mut app = pager(trailing);
    let canvas = app.canvas().clone();
    assert_eq!(
        drag_over(&canvas, trailing, "Parse &").text,
        "Parse &amp;",
        "the entity is five bytes, and they are all in the hull"
    );
}

/// A diagram inside a list item is indented, and its rectangle has to move with it.
///
/// The atom travels through `Canvas::indent` like a pin does; if it did not, the wash
/// would sit a few columns left of the drawing it claims to cover.
#[test]
fn an_indented_diagrams_wash_moves_with_it() {
    let source = "- item\n\n  ```mermaid\n  flowchart LR\n      A[Read] --> B[Draw]\n  ```\n";
    let mut app = pager(source);
    let canvas = app.canvas().clone();
    let (row, from, _) = drawn(&canvas, "Read");
    let (_, to, cols) = drawn(&canvas, "Draw");
    let across = drag(Pos::new(row, from), Pos::new(row, to + cols - 1));
    let extract = select::extract(&canvas, source, across).expect("covered both boxes");
    assert_eq!(
        extract.text, "```mermaid\nflowchart LR\n    A[Read] --> B[Draw]\n```",
        "the fenced block from its opener to its closer, with the list's indent off every \
         line — see `a_diagram_in_a_container_copies_without_its_container_prefix`"
    );
    let ranges = select::highlighted_columns(&canvas, source, across, row);
    let text = canvas.row_text(row);
    let left = u16::try_from(crate::text::display_width(
        &text[..text.len() - text.trim_start().len()],
    ))
    .expect("a test canvas is narrow");
    assert!(left > 0, "the diagram is indented by the list");
    assert_eq!(
        ranges.first().map(|range| range.start),
        Some(left),
        "the wash starts at the indented drawing, got {ranges:?}"
    );
}

/// A drag that starts in a diagram and ends outside it: the block, then what follows.
///
/// Document order, and no second concatenation step to get it wrong — the owner's rule
/// is "```mermaid ... ``` whatever else is selected".
#[test]
fn a_drag_leaving_a_diagram_takes_the_block_and_then_what_follows() {
    let source = concat!(
        "```mermaid\nflowchart LR\n    A[Read] --> B[Draw]\n```\n",
        "\nAfter the **fence**.\n"
    );
    let mut app = pager(source);
    let canvas = app.canvas().clone();
    let (row, from, _) = drawn(&canvas, "Read");
    let (below, at, cols) = drawn(&canvas, "After the fence.");
    let out = drag(Pos::new(row, from), Pos::new(below, at + cols - 1));
    let extract = select::extract(&canvas, source, out).expect("covered");
    assert!(extract.from_source);
    assert_eq!(
        extract.text,
        "```mermaid\nflowchart LR\n    A[Read] --> B[Draw]\n```\n\nAfter the **fence**.",
        "the fenced block first, then the prose the drag reached"
    );
    // The diagram is washed whole even though the drag only entered one of its boxes.
    let ranges = select::highlighted_columns(&canvas, source, out, row - 1);
    assert!(
        !ranges.is_empty(),
        "the diagram's top edge is inside the wash"
    );
}

/// The mirror: a drag that begins in prose and ends inside a diagram.
///
/// The block still arrives whole, and still in document order — the prose the drag
/// started in comes first, because that is where it sits in the file.
#[test]
fn a_drag_entering_a_diagram_from_prose_takes_the_block_whole() {
    let source = "Before it.\n\n```mermaid\nflowchart LR\n    A[Read] --> B[Draw]\n```\n";
    let mut app = pager(source);
    let canvas = app.canvas().clone();
    let (above, at, _) = drawn(&canvas, "Before it.");
    let (row, to, cols) = drawn(&canvas, "Draw");
    let extract = select::extract(
        &canvas,
        source,
        drag(Pos::new(above, at), Pos::new(row, to + cols - 1)),
    )
    .expect("covered");
    assert_eq!(
        extract.text, "Before it.\n\n```mermaid\nflowchart LR\n    A[Read] --> B[Draw]\n```",
        "the prose, then the whole block the drag ended inside"
    );
}

#[test]
fn a_drag_across_a_code_fence_takes_the_fence_from_the_source() {
    let mut app = pager(MARKUP);
    let canvas = app.canvas().clone();
    let (top, col, _) = drawn(&canvas, "item one");
    let (bottom, end, cols) = drawn(&canvas, "After the fence.");
    let extract = select::extract(
        &canvas,
        MARKUP,
        drag(Pos::new(top, col), Pos::new(bottom, end + cols - 1)),
    )
    .expect("covered");
    assert!(extract.from_source);
    assert!(
        extract.text.contains("```\nfn main() {}"),
        "a hull that reaches prose on both sides picks the fence up verbatim, got {:?}",
        extract.text
    );
}

/// Pins the limitation `select`'s module docs admit to, so it stays a decision.
///
/// A code fence carries spans now (design spec §3), and so does a flowchart's label —
/// a diagram's **box art** is what is left that the renderer never mapped, so the drag
/// below ends on the boxes' top edge rather than on the row the labels are drawn on.
#[test]
fn a_drag_ending_inside_spanless_content_stops_at_the_last_mapped_byte() {
    let markdown = "- item one\n\n```mermaid\nflowchart LR\n    A[Read] --> B[Draw]\n```\n";
    let mut app = pager(markdown);
    let canvas = app.canvas().clone();
    let (top, col, _) = drawn(&canvas, "item one");
    let (label, _, _) = drawn(&canvas, "Read");
    let extract = select::extract(
        &canvas,
        markdown,
        drag(Pos::new(top, col), Pos::new(label - 1, 40)),
    )
    .expect("covered");
    assert_eq!(
        extract.text, "- item one",
        "the far end of the hull is a source offset and those cells have none; \
         guessing one would either over-copy the block or invent an offset"
    );
}

#[test]
fn a_multi_row_drag_yields_source_line_structure_not_the_renderers() {
    // Prose long enough to be reflowed across three rows at this width.
    let source = "Alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu \
                  nu xi omicron pi rho sigma tau upsilon phi chi psi omega.\n";
    let mut app = App::new(
        Doc::parse(source),
        Config::default(),
        AppOptions {
            title: "t.md".to_string(),
            icons: false,
            theme: "dark".to_string(),
            toc_open: false,
            width: Some(30),
            config_path: None,
        },
    );
    app.resize(32, 12);
    let canvas = app.canvas().clone();
    assert!(canvas.height() >= 3, "the paragraph must have wrapped");
    let extract = select::extract(
        &canvas,
        source,
        drag(Pos::new(0, 0), Pos::new(canvas.height() - 1, 29)),
    )
    .expect("covered");
    assert!(
        !extract.text.trim_end().contains('\n'),
        "the source is one line; the wraps are the renderer's and must not be copied: \
         {:?}",
        extract.text
    );
    assert!(extract.text.starts_with("Alpha beta"));
    assert!(extract.text.trim_end().ends_with("omega."));
}

#[test]
fn a_double_width_column_maps_back_to_its_own_bytes() {
    let source = "日本語テキスト\n";
    let mut app = pager(source);
    let canvas = app.canvas().clone();
    let (row, col, _) = drawn(&canvas, "日本語");
    // Two columns per cluster: columns `col..col+4` are exactly the first two.
    let extract = select::extract(
        &canvas,
        source,
        drag(Pos::new(row, col), Pos::new(row, col + 3)),
    )
    .expect("covered");
    assert_eq!(extract.text, "日本");
    // Three columns, which is where counting bytes instead of columns diverges: the
    // third column is the lead cell of `本`, and a cluster is never half-copied.
    let extract = select::extract(
        &canvas,
        source,
        drag(Pos::new(row, col), Pos::new(row, col + 2)),
    )
    .expect("covered");
    assert_eq!(extract.text, "日本");
}

#[test]
fn a_selection_is_anchored_to_the_document_not_to_the_viewport() {
    let mut app = pager(SAMPLE);
    app.begin_selection(1, 2);
    let before = app.selection().expect("a drag started");
    app.on_scroll(1, false);
    let after = app.selection().expect("scrolling does not cancel a drag");
    assert_eq!(
        before, after,
        "the anchor is a canvas position, so scrolling under it changes nothing"
    );
    assert!(app.scroll() > 0, "and the document did move");
}

#[test]
fn dragging_past_the_bottom_of_the_viewport_scrolls() {
    let mut app = pager(SAMPLE);
    app.begin_selection(1, 0);
    let height = app.viewport_height();
    app.drag_selection(1, u16::try_from(height - 1).expect("small viewport"));
    assert_eq!(
        app.scroll(),
        1,
        "a drag at the last row pulls the document up"
    );
}

#[test]
fn a_resize_drops_the_selection() {
    let mut app = pager(SAMPLE);
    app.begin_selection(1, 1);
    app.drag_selection(20, 1);
    assert!(app.selection().is_some());
    app.resize(60, 12);
    let _ = app.canvas();
    assert!(
        app.selection().is_none(),
        "a reflow moves every row, so the cells the reader picked are not the same \
         cells any more"
    );
}

#[test]
fn a_height_only_resize_keeps_the_selection() {
    let mut app = pager(SAMPLE);
    app.begin_selection(1, 1);
    app.drag_selection(20, 1);
    app.resize(80, 20);
    let _ = app.canvas();
    assert!(
        app.selection().is_some(),
        "nothing was re-laid-out, so there is nothing to invalidate"
    );
}

#[test]
fn a_click_is_not_a_selection() {
    let mut app = pager(SAMPLE);
    app.begin_selection(3, 0);
    app.end_selection();
    assert!(app.selection().is_none());
    assert!(app.take_pending_copy().is_none());
}

#[test]
fn escape_clears_the_selection_before_anything_else() {
    let mut app = pager(SAMPLE);
    app.begin_selection(1, 0);
    app.drag_selection(20, 0);
    app.end_selection();
    assert!(app.selection().is_some());
    app.act(Action::Cancel);
    assert!(app.selection().is_none());
    assert_eq!(
        app.notice().map(|notice| notice.text.as_str()),
        Some("selection cleared")
    );
}

#[test]
fn finishing_a_drag_offers_the_text_exactly_once() {
    let mut app = pager(MARKUP);
    let canvas = app.canvas().clone();
    let (row, col, cols) = drawn(&canvas, "Wide diagram");
    let y = u16::try_from(row).expect("small canvas");
    app.begin_selection(col, y);
    app.drag_selection(col + cols - 1, y);
    app.end_selection();
    let extract = app.take_pending_copy().expect("a drag produced text");
    assert_eq!(extract.text, "# Wide diagram");
    assert!(extract.from_source);
    assert!(app.take_pending_copy().is_none(), "and not a second time");
}

#[test]
fn a_drag_after_scrolling_reads_the_row_the_reader_is_looking_at() {
    let mut source = String::new();
    for index in 0..20 {
        source.push_str(&format!("Paragraph {index}.\n\n"));
    }
    source.push_str("# Wide diagram\n\n");
    for index in 0..20 {
        source.push_str(&format!("Tail {index}.\n\n"));
    }
    let mut app = pager(&source);
    let canvas = app.canvas().clone();
    let (row, col, cols) = drawn(&canvas, "Wide diagram");
    assert!(row > 4, "the heading must be off the first screen");
    app.scroll_to(row - 2);
    assert_eq!(app.scroll(), row - 2, "the scroll must not have clamped");
    // Viewport row 2 is canvas row `row`: if the anchor forgot to add the scroll
    // offset it would land on `Paragraph 18.` instead.
    app.begin_selection(col, 2);
    app.drag_selection(col + cols - 1, 2);
    app.end_selection();
    assert_eq!(
        app.take_pending_copy().expect("text").text,
        "# Wide diagram"
    );
}

#[test]
fn the_selection_highlight_is_painted_and_does_not_borrow_a_search_colour() {
    let mut app = pager(MARKUP);
    let theme = app.theme().clone();
    let selection = super::draw::term_style(theme.ui.selection).bg;
    assert_ne!(selection, super::draw::term_style(theme.ui.search_match).bg);
    assert_ne!(
        selection,
        super::draw::term_style(theme.ui.search_current).bg
    );

    let canvas = app.canvas().clone();
    let (row, col, cols) = drawn(&canvas, "Wide diagram");
    let y = u16::try_from(row).expect("small canvas");
    app.begin_selection(col, y);
    app.drag_selection(col + cols - 1, y);
    let buffer = framed_buffer(&mut app, 80, 12);
    let painted = (col..col + cols)
        .filter(|x| buffer[(*x, y)].style().bg == selection)
        .count();
    assert_eq!(
        painted,
        usize::from(cols),
        "every selected cell carries the selection wash"
    );
}

#[test]
fn base64_matches_the_rfc_4648_vectors() {
    for (plain, encoded) in [
        ("", ""),
        ("f", "Zg=="),
        ("fo", "Zm8="),
        ("foo", "Zm9v"),
        ("foob", "Zm9vYg=="),
        ("fooba", "Zm9vYmE="),
        ("foobar", "Zm9vYmFy"),
    ] {
        assert_eq!(
            super::clipboard::encode_for_test(plain.as_bytes()),
            encoded,
            "base64({plain:?})"
        );
    }
}

#[test]
fn the_copy_report_claims_only_what_the_route_can_prove() {
    use super::clipboard::{Copied, Delivery};
    assert_eq!(
        Delivery::Confirmed.message(47, Copied::Source),
        ("copied 47 bytes of Markdown source".to_string(), false)
    );
    let (text, is_error) = Delivery::Sent.message(47, Copied::Rendered);
    assert!(
        text.contains("unconfirmed") && text.contains("rendered text"),
        "OSC 52 is fire-and-forget and the bar must not pretend otherwise: {text:?}"
    );
    assert!(!is_error);
    let (text, is_error) = Delivery::Failed("no clipboard".to_string()).message(47, Copied::Source);
    assert!(is_error && text.contains("no clipboard"));
}

/// A button copies a whole table, which is neither the selection's Markdown source nor
/// the drawn cells, and the bar has to be able to say so.
#[test]
fn a_table_copy_says_what_it_was() {
    use super::clipboard::{Copied, Delivery};
    let (text, is_error) = Delivery::Confirmed.message(47, Copied::Table);
    assert_eq!(text, "copied 47 bytes of table");
    assert!(!is_error);
}

/// And a code block's button says `code`, not `Markdown source`: a quoted fence's source
/// carries the `> ` markers the button deliberately does not copy.
#[test]
fn a_code_copy_says_what_it_was() {
    use super::clipboard::{Copied, Delivery};
    let (text, _) = Delivery::Confirmed.message(12, Copied::Code);
    assert_eq!(text, "copied 12 bytes of code");
}

/// An X11 or Wayland selection belongs to a process, so a copy that only the local
/// display server holds is a copy that ends when `mdmost` does — unless a clipboard
/// manager takes it over, which is exactly the thing we cannot check. The bar therefore
/// says what it knows: this one is held while the pager runs.
#[test]
fn the_copy_report_marks_a_copy_only_this_process_holds() {
    use super::clipboard::{Copied, Delivery};
    let (text, is_error) = Delivery::LocalOnly.message(47, Copied::Rendered);
    assert!(!is_error);
    assert!(
        text.starts_with("copied 47 bytes of rendered text"),
        "{text:?}"
    );
    assert!(
        text.contains("while mdmost runs"),
        "the one thing this route cannot promise is surviving the exit: {text:?}"
    );
    // And the route that did reach the terminal emulator must not carry the caveat:
    // OSC 52 hands the bytes to a process that outlives us.
    assert!(
        !Delivery::Confirmed
            .message(47, Copied::Rendered)
            .0
            .contains("while mdmost runs")
    );
}

/// Which claim each combination of routes earns. The distinction that matters is the
/// second row: with OSC 52 gone, the terminal emulator — the one holder that outlives
/// the pager — never got a copy, so the caveat is the whole difference.
#[test]
fn the_delivery_is_decided_by_which_routes_worked() {
    use super::clipboard::{Delivery, classify_for_test as classify};
    let bad = || Err("nope".to_string());
    assert_eq!(classify(Ok(()), Some(Ok(()))), Delivery::Confirmed);
    assert_eq!(classify(bad(), Some(Ok(()))), Delivery::LocalOnly);
    assert_eq!(classify(Ok(()), Some(bad())), Delivery::Sent);
    assert_eq!(classify(Ok(()), None), Delivery::Sent);
    assert!(matches!(classify(bad(), Some(bad())), Delivery::Failed(_)));
    assert!(matches!(classify(bad(), None), Delivery::Failed(_)));
}

/// The bug this covers: `set_text` used to be called on a `Clipboard` that was dropped
/// on the next line, which on X11 un-copies the text before anyone can ask for it.
/// Holding the handle open is not an optimisation, it *is* the copy.
///
/// Needs a display server, and clobbers the desktop clipboard when it has one — there
/// is no way to test ownership without taking it. It reports why it skipped rather than
/// passing silently.
#[test]
fn a_local_copy_stays_owned_until_it_is_released() {
    #[cfg(feature = "clipboard")]
    let _owned = DESKTOP_CLIPBOARD
        .lock()
        .unwrap_or_else(|it| it.into_inner());
    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        eprintln!("skipped: no display server to own a selection");
        return;
    }
    let Some(first) = super::clipboard::local_for_test("mdmost clipboard test one") else {
        eprintln!(
            "skipped: no local clipboard was attempted — either a remote session, where \
             writing to this end's clipboard would be a mistake, or a build without the \
             `clipboard` feature"
        );
        return;
    };
    if let Err(why) = first {
        eprintln!("skipped: this display server would not take a clipboard: {why}");
        return;
    }
    assert!(
        super::clipboard::held_for_test(),
        "the copy is gone the moment the handle is dropped; it must still be held here"
    );
    // The second copy is the case a naive background-thread design gets wrong: the
    // first owner has to yield. Re-using one handle makes it a non-event.
    let second = super::clipboard::local_for_test("mdmost clipboard test two");
    assert_eq!(
        second,
        Some(Ok(())),
        "a second copy must not need a new handle"
    );
    assert!(super::clipboard::held_for_test());
    // Releasing is deliberate and is where a clipboard manager gets its chance.
    super::clipboard::release();
    assert!(!super::clipboard::held_for_test());
}

/// The promise of a two-flavour copy is that it costs nobody anything: an application
/// that reads HTML gets the richer flavour, and one that does not must still find the
/// plain text there. Setting the two in separate calls would leave only the second, so
/// this asserts the plain flavour survives an HTML copy.
///
/// Needs a display server, and clobbers the desktop clipboard when it has one. It
/// reports why it skipped rather than passing silently.
#[cfg(feature = "clipboard")]
#[test]
fn a_rich_copy_still_leaves_the_plain_text_for_whoever_cannot_read_html() {
    let _owned = DESKTOP_CLIPBOARD
        .lock()
        .unwrap_or_else(|it| it.into_inner());
    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        eprintln!("skipped: no display server to own a selection");
        return;
    }
    // The capability probe, and the reason it is a *plain* copy: everything the skips
    // below excuse — no clipboard attempted, a display server that will not take a
    // selection or will not hand one back — is decided here, on a path this test is not
    // about. Past that point the rich copy has no excuses left and every failure is a
    // failure, which is what makes dropping the alternate show up as red rather than as
    // a skipped test.
    let Some(Ok(())) = super::clipboard::local_for_test("mdmost plain probe") else {
        eprintln!(
            "skipped: no local clipboard took a plain copy — a remote session, or a display server that would not have it"
        );
        return;
    };
    if let Err(why) = super::clipboard::paste_for_test() {
        eprintln!("skipped: this display server does not hand a selection back: {why}");
        return;
    }
    assert_eq!(
        super::clipboard::local_rich_for_test("a\tb\n1\t2\n", "<table><tr><td>a</td></tr></table>"),
        Some(Ok(())),
        "the same handle that took a plain copy must take a rich one"
    );
    assert_eq!(
        super::clipboard::paste_for_test(),
        Ok("a\tb\n1\t2\n".to_string()),
        "the HTML flavour must not have displaced the plain text every reader receives"
    );
    super::clipboard::release();
}

/// Serialises the tests that take the desktop selection. There is one clipboard per
/// display server, so two of these running at once would read each other's copy.
#[cfg(feature = "clipboard")]
static DESKTOP_CLIPBOARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// A library writing to standard error must not reach the alternate screen, and must
/// not be discarded either. Both halves are asserted: `arboard` picks between
/// `eprintln!` and `log::warn!` depending on whether standard error is a terminal, so
/// defending one layer alone would have missed one of its two branches.
#[test]
fn a_librarys_complaints_are_held_back_and_then_reported() {
    use std::io::Write;

    let mut capture = super::stderr::Capture::start();
    // Written through the handle rather than with `eprintln!` on purpose: the test
    // harness redirects the *macro* to a per-thread sink, so `eprintln!` here would
    // never reach descriptor 2 and the test would be measuring `libtest`. A library
    // linked into the real binary has no such sink; `io::stderr()` is what it gets.
    let _ = writeln!(std::io::stderr(), "SCRIBBLE-ON-THE-FRAME");
    log::warn!("A-LIBRARY-WARNING");
    let collected = String::from_utf8_lossy(&capture.finish()).into_owned();
    assert!(
        collected.contains("SCRIBBLE-ON-THE-FRAME"),
        "a direct write to standard error escaped onto the screen: {collected:?}"
    );
    assert!(
        collected.contains("A-LIBRARY-WARNING"),
        "a `log` record was not collected — with standard error redirected this is the \
         branch `arboard` takes: {collected:?}"
    );
    // Nothing is swallowed once there is no screen to protect.
    assert!(super::stderr::Capture::start().finish().is_empty());
}

/// A document with both controls in it: a fenced block and a top-level table.
const BUTTONS: &str = "\
# Buttons

```rust
let a = 1;
```

| Name | Since |
| --- | ---: |
| Ada | 1843 |
";

/// A table too wide for any viewport this suite uses, so its button is off to the right.
const BUTTON_TABLE: &str = "\
# Wide

| AlphaColumnOne | BetaColumnTwoLonger | GammaColumnThreeWider | Ocelot |
| --- | --- | --- | --- |
| 1 | 2 | 3 | 4 |
";

/// The viewport cell the `[` of a drawn `[copy]` sits in, counting from `skip` in.
///
/// Read off the *painted frame*, not off the hotspot list: a hit test proved against
/// the very positions it is reading from would pass on a build that draws the label
/// somewhere else entirely. This is the reader's own information — where they can see
/// the control — and it is the coordinate their pointer arrives in.
fn painted_button(app: &mut App, width: u16, height: u16, skip: usize) -> (u16, u16) {
    let rows = framed(app, width, height);
    let mut seen = 0;
    for (y, line) in rows.iter().enumerate() {
        if let Some(at) = line.find(crate::render::button::LABEL) {
            if seen == skip {
                let x = crate::text::display_width(&line[..at]);
                return (
                    u16::try_from(x).expect("a viewport column"),
                    u16::try_from(y).expect("a viewport row"),
                );
            }
            seen += 1;
        }
    }
    panic!("no [copy] label {skip} on the screen: {rows:?}");
}

#[test]
fn the_button_appears_only_once_the_mouse_has_been_captured() {
    // The gate design spec §4 asks for, from the pager's end. `RenderOptions` is part
    // of the render cache key, so the flag has to reach it: the canvas drawn before the
    // mouse was granted would otherwise be served for the rest of the session.
    let mut app = pager_at(BUTTONS, 60, 20);
    assert!(
        !app.canvas()
            .plain_text()
            .contains(crate::render::button::LABEL),
        "no button until the mouse is real"
    );
    app.set_copy_button(true);
    assert!(
        app.canvas()
            .plain_text()
            .contains(crate::render::button::LABEL),
        "and one once it is"
    );
    app.set_copy_button(false);
    assert!(
        !app.canvas()
            .plain_text()
            .contains(crate::render::button::LABEL),
        "a terminal that lost the mouse loses the button with it"
    );
}

#[test]
fn a_press_on_the_code_button_copies_the_block_and_starts_no_drag() {
    let mut app = pager_at(BUTTONS, 60, 20);
    app.set_copy_button(true);
    let (x, y) = painted_button(&mut app, 60, 20, 0);
    assert!(
        app.press_hotspot(x, y),
        "the label the reader sees is a control"
    );
    assert!(
        app.selection().is_none(),
        "a press on a control is not the start of a drag"
    );
    let copy = app.take_hotspot_copy().expect("a payload");
    assert_eq!(copy.text, "let a = 1;\n");
    assert!(copy.html.is_none(), "code has no second flavour");
    assert_eq!(copy.what, super::clipboard::Copied::Code);
}

#[test]
fn a_press_beside_the_button_is_a_drag_like_any_other() {
    let mut app = pager_at(BUTTONS, 60, 20);
    app.set_copy_button(true);
    let (x, y) = painted_button(&mut app, 60, 20, 0);
    for x in [x - 1, x + 6] {
        assert!(
            !app.press_hotspot(x, y),
            "the columns either side of the label are frame, not control"
        );
        app.begin_selection(x, y);
        assert!(app.selection().is_some(), "so they still begin a selection");
    }
}

#[test]
fn a_press_on_the_table_button_copies_a_grid_and_says_it_was_a_table() {
    // The status bar never lies: the same control on a table has to report `Table`, or
    // a reader is told they copied code and pastes a spreadsheet.
    let mut app = pager_at(BUTTONS, 60, 20);
    app.set_copy_button(true);
    let (x, y) = painted_button(&mut app, 60, 20, 1);
    assert!(app.press_hotspot(x, y), "the table's label is a control");
    let copy = app.take_hotspot_copy().expect("a payload");
    assert_eq!(copy.text, "Name\tSince\nAda\t1843\n");
    assert!(copy.html.is_some(), "a table offers the richer flavour too");
    assert_eq!(copy.what, super::clipboard::Copied::Table);
}

#[test]
fn the_button_is_still_under_the_pointer_once_the_document_has_scrolled() {
    // The press goes through the same translation a drag does, and at rest that
    // translation is the identity — so only a scrolled document can tell the two apart.
    // Vertically here, horizontally in the test below.
    let mut app = pager_at(BUTTONS, 60, 8);
    app.set_copy_button(true);
    app.act(Action::LineDown);
    app.act(Action::LineDown);
    assert_eq!(app.scroll(), 2, "the document must have moved");
    let (x, y) = painted_button(&mut app, 60, 8, 0);
    assert!(
        app.press_hotspot(x, y),
        "the control moved up the screen with the block it belongs to"
    );
    assert_eq!(
        app.take_hotspot_copy().map(|copy| copy.text),
        Some("let a = 1;\n".to_string())
    );
}

#[test]
fn the_button_of_an_over_wide_table_is_pressable_once_it_is_scrolled_into_view() {
    // A table can be wider than the viewport, and then its button rides off the right
    // edge with the rest of the top rule — the cost design spec §6 records and accepts.
    // What must not also be true is that scrolling to it produces a label that does not
    // answer, so this presses it where the reader can see it.
    let mut app = pager_at(BUTTON_TABLE, 50, 12);
    app.set_copy_button(true);
    assert!(app.hscroll_max() > 0, "the probe table must be over-wide");
    while app.hscroll() < app.hscroll_max() {
        app.act(Action::ScrollRight);
    }
    let (x, y) = painted_button(&mut app, 50, 12, 0);
    assert!(
        app.press_hotspot(x, y),
        "the control answers at the column it is drawn in, scrolled or not"
    );
    assert_eq!(
        app.take_hotspot_copy().map(|copy| copy.what),
        Some(super::clipboard::Copied::Table)
    );
}

#[test]
fn a_button_clipped_off_the_canvas_cannot_be_pressed_anywhere() {
    // A block too wide even for `render::document`'s widening cap is clipped, and its
    // button's label is drawn nowhere. No press may reach it, so this sweeps every cell
    // of the block at every horizontal offset there is.
    //
    // **Changed 2026-08-12 (Task 2b).** The second premise used to be that the hotspot
    // *outlived* the clip, at a column the clipped canvas no longer had, and the sweep
    // was the proof that no press could name that column. The clip now takes the claim
    // with the cells (`Canvas::truncate_width`), so the control is gone rather than
    // merely unreachable — a stronger fact, asserted here in place of the weaker one.
    // The sweep stays: it is what would catch a claim surviving at a column that *does*
    // exist, which is the failure this test was written for.
    let columns = 300;
    let head: String = (0..columns).map(|i| format!("| C{i:04} ")).collect();
    let rule: String = (0..columns).map(|_| "| --- ".to_string()).collect();
    let body: String = (0..columns).map(|i| format!("| v{i:04} ")).collect();
    let mut app = pager_at(&format!("{head}|\n{rule}|\n{body}|\n"), 80, 12);
    app.set_copy_button(true);
    assert!(
        !app.canvas()
            .plain_text()
            .contains(crate::render::button::LABEL),
        "the premise: the label was clipped away, so nothing is drawn to press"
    );
    assert!(
        app.canvas().hotspots().is_empty(),
        "the clip took the claim with the cells it cut: {:?}",
        app.canvas().hotspots()
    );
    loop {
        for y in 0..11 {
            for x in 0..80 {
                assert!(
                    !app.press_hotspot(x, y),
                    "a control nobody can see answered a press at {x},{y} \
                     with the document scrolled to column {}",
                    app.hscroll()
                );
            }
        }
        if app.hscroll() >= app.hscroll_max() {
            break;
        }
        app.act(Action::ScrollRight);
    }
}

#[test]
fn the_flash_expires_without_anything_scheduling_it() {
    let mut app = pager_at(BUTTONS, 60, 20);
    app.flash_copied(0, 30);
    assert!(app.copied_flash().is_some());
    std::thread::sleep(std::time::Duration::from_millis(
        super::app::FLASH_FOR.saturating_add(50),
    ));
    assert!(
        app.copied_flash().is_none(),
        "the label goes back to [copy] on the next redraw and no earlier"
    );
}

#[test]
fn the_flash_is_painted_over_the_button_that_was_pressed() {
    // The whole point of the nine reserved columns: `[copied]` is longer than `[copy]`
    // and has to fit without a re-render, because a render may not depend on the clock.
    let mut app = pager_at(BUTTONS, 60, 20);
    app.set_copy_button(true);
    let (x, y) = painted_button(&mut app, 60, 20, 0);
    assert!(app.press_hotspot(x, y));
    let copy = app.take_hotspot_copy().expect("a payload");
    app.flash_copied(copy.row, copy.col);
    let rows = framed(&mut app, 60, 20);
    assert!(
        rows.iter()
            .any(|line| line.contains(crate::render::button::FLASH)),
        "the block that was copied says so: {rows:?}"
    );
    assert_eq!(
        rows.iter()
            .filter(|line| line.contains(crate::render::button::LABEL))
            .count(),
        1,
        "the table keeps its [copy] and the code block does not show both: {rows:?}"
    );
    // The frame it was painted into is untouched on either side of the reserved region.
    assert!(
        rows[usize::from(y)].contains("[copied]─╮"),
        "the corner survives the overwrite: {:?}",
        rows[usize::from(y)]
    );
}

/// The foregrounds the `[copy]` label's own columns are painted in, left to right.
///
/// Read off the painted frame rather than off the theme, because "the button changed
/// colour" is a claim about cells on a screen. A state flag says the bookkeeping ran;
/// only this says the reader can see anything.
fn button_inks(
    app: &mut App,
    width: u16,
    height: u16,
    at: (u16, u16),
) -> Vec<Option<ratatui::style::Color>> {
    let buffer = framed_buffer(app, width, height);
    let label = u16::try_from(crate::render::button::LABEL.chars().count()).expect("a width");
    (at.0..at.0 + label)
        .map(|x| buffer[(x, at.1)].style().fg)
        .collect()
}

/// The row the button is drawn on, as text.
fn button_row(app: &mut App, width: u16, height: u16, y: u16) -> String {
    framed(app, width, height)[usize::from(y)].clone()
}

#[test]
fn the_pointer_over_a_button_marks_it_hovered_and_repaints_it() {
    let mut app = pager_at(BUTTONS, 60, 20);
    app.set_copy_button(true);
    let (x, y) = painted_button(&mut app, 60, 20, 0);
    let resting = button_inks(&mut app, 60, 20, (x, y));
    let before = button_row(&mut app, 60, 20, y);

    assert!(
        app.set_pointer(x, y),
        "arriving on a control is a change worth a repaint"
    );
    assert_eq!(app.hovered(), Some(0), "and it is that control");

    let hovered = button_inks(&mut app, 60, 20, (x, y));
    let theme = app.theme();
    let expected = super::draw::term_style(theme.hovered(theme.code.frame)).fg;
    assert_eq!(
        hovered,
        vec![expected; resting.len()],
        "every drawn column of the label takes the hovered ink"
    );
    assert_ne!(hovered, resting, "which is not the ink it had at rest");
    assert_eq!(
        button_row(&mut app, 60, 20, y),
        before,
        "hover restyles cells, it never moves one"
    );
}

#[test]
fn moving_within_one_button_does_not_ask_for_a_redraw() {
    // The failure mode this return value exists to prevent: a pager that re-lays-out
    // and repaints the whole document on every motion event a terminal emits.
    let mut app = pager_at(BUTTONS, 60, 20);
    app.set_copy_button(true);
    let (x, y) = painted_button(&mut app, 60, 20, 0);
    assert!(app.set_pointer(x, y), "the first arrival is a change");
    for step in 1..6 {
        assert!(
            !app.set_pointer(x + step, y),
            "column {} is the same button as column {x}",
            x + step
        );
        assert_eq!(app.hovered(), Some(0), "and it stays hovered");
    }
    assert!(app.set_pointer(x - 1, y), "leaving it is a change again");
}

#[test]
fn the_pointer_off_any_button_clears_the_hover_and_the_paint() {
    let mut app = pager_at(BUTTONS, 60, 20);
    app.set_copy_button(true);
    let (x, y) = painted_button(&mut app, 60, 20, 0);
    let resting = button_inks(&mut app, 60, 20, (x, y));
    app.set_pointer(x, y);
    assert!(app.hovered().is_some(), "the premise");
    assert!(app.set_pointer(0, 0), "moving away is a change");
    assert_eq!(app.hovered(), None, "and nothing is hovered");
    assert_eq!(
        button_inks(&mut app, 60, 20, (x, y)),
        resting,
        "the label goes back to the frame's own colour"
    );
    // And the same for the pointer leaving the document area altogether.
    app.set_pointer(x, y);
    assert!(app.clear_pointer(), "leaving the pane is a change");
    assert_eq!(app.hovered(), None);
    assert!(!app.clear_pointer(), "and clearing nothing is not");
}

#[test]
fn the_resting_button_is_drawn_in_the_frames_own_colour() {
    // The owner's ruling: at rest the button keeps exactly what it draws today. This
    // pins that, so a future shade cannot be slipped in under the hover work.
    let mut app = pager_at(BUTTONS, 60, 20);
    app.set_copy_button(true);
    let (x, y) = painted_button(&mut app, 60, 20, 0);
    let frame = super::draw::term_style(app.theme().code.frame).fg;
    assert_eq!(
        button_inks(&mut app, 60, 20, (x, y)),
        vec![frame; 6],
        "an unhovered [copy] is frame, exactly as it always was"
    );
    // The table's button too, which is drawn in the other frame style.
    let (tx, ty) = painted_button(&mut app, 60, 20, 1);
    let border = super::draw::term_style(app.theme().table.border).fg;
    assert_eq!(button_inks(&mut app, 60, 20, (tx, ty)), vec![border; 6]);
}

#[test]
fn the_copied_flash_beats_the_hover_under_the_pointer() {
    // The pointer is necessarily over the button when it is clicked, so a hover style
    // painted after the flash would mask the one piece of feedback the press produces.
    let mut app = pager_at(BUTTONS, 60, 20);
    app.set_copy_button(true);
    let (x, y) = painted_button(&mut app, 60, 20, 0);
    let resting = button_inks(&mut app, 60, 20, (x, y));
    app.set_pointer(x, y);
    assert!(app.press_hotspot(x, y));
    let copy = app.take_hotspot_copy().expect("a payload");
    app.flash_copied(copy.row, copy.col);
    assert_eq!(app.hovered(), Some(0), "the pointer has not gone anywhere");

    let rows = framed(&mut app, 60, 20);
    assert!(
        rows[usize::from(y)].contains(crate::render::button::FLASH),
        "the flash is drawn: {:?}",
        rows[usize::from(y)]
    );
    assert_eq!(
        button_inks(&mut app, 60, 20, (x, y)),
        resting,
        "and it is drawn in the resting ink, not repainted by the hover under it"
    );
}

#[test]
fn a_pager_without_the_mouse_has_nothing_to_hover() {
    // With `--mouse` off there are no hotspots at all, so there is no state to keep.
    let mut app = pager_at(BUTTONS, 60, 20);
    let rows = framed(&mut app, 60, 20);
    assert!(
        !rows
            .iter()
            .any(|row| row.contains(crate::render::button::LABEL)),
        "the premise: no button is drawn"
    );
    for y in 0..19 {
        for x in 0..59 {
            assert!(!app.set_pointer(x, y), "nothing to hover at {x},{y}");
            assert_eq!(app.hovered(), None);
        }
    }
}

#[test]
fn a_reflow_drops_the_hover_rather_than_moving_it() {
    // The hover names a control by its index in the canvas's hotspot list, and a render
    // replaces that list: the same index at a new width can be a different button, or
    // none. The pointer has not moved, so nothing recomputes it — the stale index would
    // simply be painted, lighting a control the reader is nowhere near. Dropped instead,
    // exactly as the selection is, and for the same reason.
    let mut app = pager_at(BUTTONS, 60, 20);
    app.set_copy_button(true);
    let (x, y) = painted_button(&mut app, 60, 20, 0);
    app.set_pointer(x, y);
    assert_eq!(app.hovered(), Some(0), "the premise");

    let buffer = framed_buffer(&mut app, 40, 20);
    assert_eq!(app.hovered(), None, "the reflow took the hover with it");
    let theme = app.theme();
    let lit: Vec<_> = [theme.code.frame, theme.table.border]
        .iter()
        .map(|style| super::draw::term_style(theme.hovered(*style)).fg)
        .collect();
    for y in 0..20 {
        for x in 0..40 {
            assert!(
                !lit.contains(&buffer[(x, y)].style().fg),
                "a hovered ink survived the reflow at {x},{y}"
            );
        }
    }
}

// --- Hover lights a whole control, not one row of it (design spec §2.2) --------

#[test]
fn hovering_a_link_shades_every_row_of_it() {
    // A wrapped link is several hotspots sharing one `target`; hovering any one row
    // must light all of them, or the link visibly breaks in half under the pointer.
    let mut app = pager_at(
        "[a fairly long link label that wraps across several rows](https://example.com/x)\n",
        24,
        10,
    );
    let spots = app.rendered().hotspots().to_vec();
    assert!(
        spots.len() >= 2,
        "the premise: the link wraps, got {spots:?}"
    );
    let target = spots[0].target;
    assert!(
        spots.iter().all(|spot| spot.target == target),
        "one link, one target: {spots:?}"
    );

    // The label and its ` (url)` suffix take different resting styles
    // (`theme.text.link` and `theme.text.link_url`), so "lit" is checked cell by cell,
    // against what that exact cell had at rest — not against one derived colour.
    let resting = framed_buffer(&mut app, 24, 10);

    app.set_pointer(spots[0].col, u16::try_from(spots[0].row).expect("a row"));
    assert_eq!(
        app.hovered(),
        Some(0),
        "the first row is the one hit-tested"
    );

    let buffer = framed_buffer(&mut app, 24, 10);
    let theme = app.theme();
    let lit: Vec<usize> = spots
        .iter()
        .filter(|spot| {
            let y = u16::try_from(spot.row).expect("a row");
            (spot.col..spot.col.saturating_add(spot.cols)).all(|x| {
                let before = resting[(x, y)].style();
                let expected = super::draw::term_style(theme.hovered(from_term_style(before))).fg;
                buffer[(x, y)].style().fg == expected
            })
        })
        .map(|spot| spot.row)
        .collect();
    assert_eq!(
        lit.len(),
        spots.len(),
        "only row(s) {lit:?} lit out of {spots:?}; the link wraps"
    );
}

/// Recovers a `theme::Style` foreground from a drawn cell's `ratatui::Style`.
///
/// Only the foreground round-trips through the buffer, which is all a hovered cell's
/// ink depends on — [`super::draw::hover_highlight`] derives the shade from the cell's
/// own colour, not from any other attribute.
fn from_term_style(style: ratatui::style::Style) -> crate::theme::Style {
    let ratatui::style::Color::Rgb(r, g, b) = style.fg.expect("a foreground") else {
        panic!("a drawn cell's foreground is always RGB");
    };
    crate::theme::Style {
        fg: Some(crate::theme::Color { r, g, b }),
        bg: None,
        attrs: crate::theme::Attributes::NONE,
    }
}

#[test]
fn ordinary_prose_under_the_pointer_is_not_shaded() {
    // The design spec §9.1 asymmetry, stated as a test: proving the non-hotspot
    // direction too, not only that a hovered control lights.
    let mut app = pager_at("just some words\n", 60, 10);
    let before = framed_buffer(&mut app, 60, 10);
    assert!(!app.set_pointer(2, 0), "plain prose has nothing to hover");
    assert_eq!(app.hovered(), None);
    let after = framed_buffer(&mut app, 60, 10);
    for x in 0..60 {
        assert_eq!(
            after[(x, 0)].style(),
            before[(x, 0)].style(),
            "column {x} of plain prose changed style under the pointer"
        );
    }
}

#[test]
fn hovering_a_link_does_not_light_an_unrelated_copy_button() {
    // Painting "every hotspot sharing this target" must stop at the target: a control
    // with a *different* target — here the fence's `[copy]` button — must stay exactly
    // as it was, or two controls that merely happen to be on screen together would be
    // confused for one.
    let mut app = pager_at(
        "```rust\nfn f() {}\n```\n\n[a link](https://example.com/x)\n",
        60,
        20,
    );
    app.set_copy_button(true);
    let (bx, by) = painted_button(&mut app, 60, 20, 0);
    let resting = button_inks(&mut app, 60, 20, (bx, by));

    let spots = app.rendered().hotspots().to_vec();
    let link = spots
        .iter()
        .find(|spot| matches!(spot.kind, crate::canvas::HotspotKind::Open { .. }))
        .expect("the link recorded a hotspot");
    app.set_pointer(link.col, u16::try_from(link.row).expect("a row"));
    assert!(app.hovered().is_some(), "the link is hovered");

    assert_eq!(
        button_inks(&mut app, 60, 20, (bx, by)),
        resting,
        "the copy button stays at rest while a different control is hovered"
    );
}

/// A bare pointer motion over the terminal, at screen column `column`, row `row`.
fn motion(column: u16, row: u16) -> crossterm::event::MouseEvent {
    crossterm::event::MouseEvent {
        kind: crossterm::event::MouseEventKind::Moved,
        column,
        row,
        modifiers: crossterm::event::KeyModifiers::NONE,
    }
}

#[test]
fn a_motion_event_is_what_lights_the_button() {
    // The state and the paint are tested above; this is the wire they hang off. A
    // terminal in any-event tracking mode — which is what `EnableMouseCapture` asks for
    // — delivers the pointer as `Moved`, and until this arm existed the loop dropped
    // every one of them on the floor. Driven through the same dispatcher the event loop
    // calls, so a hover that works only when a test pokes `App` directly fails here.
    let mut app = pager_at(BUTTONS, 60, 20);
    app.set_copy_button(true);
    let (x, y) = painted_button(&mut app, 60, 20, 0);
    assert_eq!(app.toc_width(), 0, "the probe has no pane in the way");

    assert!(
        super::term::on_mouse(&mut app, motion(x, y), 60, 20),
        "arriving on the control asks for the frame that shows it"
    );
    assert_eq!(app.hovered(), Some(0));
    assert!(
        !super::term::on_mouse(&mut app, motion(x + 1, y), 60, 20),
        "and sliding along it asks for nothing"
    );
    assert_eq!(app.hovered(), Some(0), "while staying on it");
    // Off the document altogether: the scrollbar's own column.
    assert!(
        super::term::on_mouse(&mut app, motion(59, y), 60, 20),
        "leaving the document is a change"
    );
    assert_eq!(
        app.hovered(),
        None,
        "a pointer on the scrollbar leaves no button lit"
    );
}

// --- Resolving a cell to a source offset (design spec §2.1) ----------------------

/// Renders `doc` at `width` directly, with no pager or terminal in the way.
///
/// `offset_at` is tested against the canvas alone, so the tests build one the same way
/// `render/tests.rs` does rather than driving a full [`App`](super::app::App).
fn render(doc: &str, width: u16) -> Canvas {
    let parsed = Doc::parse(doc);
    let theme = crate::theme::Theme::default_dark();
    crate::render::render_document(
        &parsed,
        width,
        None,
        &theme,
        &crate::render::RenderOptions::default(),
    )
}

#[test]
fn a_cell_on_text_resolves_to_that_byte() {
    let doc = "| alpha | beta |\n| --- | --- |\n| one | two |\n";
    let canvas = render(doc, 40);
    let span = canvas
        .spans()
        .iter()
        .find(|s| &doc[s.source_start..s.source_end] == "alpha")
        .expect("a span for alpha");
    let at = select::offset_at(
        &canvas,
        doc,
        Pos {
            row: span.row,
            col: span.col,
        },
        select::Bias::Start,
    );
    assert_eq!(at, Some(span.source_start));
}

#[test]
fn a_cell_on_a_border_resolves_to_the_next_text_in_document_order() {
    // Column 1 of a table row is the left vertical rule: chrome, with no span.
    let doc = "| alpha | beta |\n| --- | --- |\n| one | two |\n";
    let canvas = render(doc, 40);
    let span = canvas
        .spans()
        .iter()
        .find(|s| &doc[s.source_start..s.source_end] == "alpha")
        .expect("a span for alpha");
    let on_rule = Pos {
        row: span.row,
        col: 1,
    };
    assert!(
        canvas
            .spans()
            .iter()
            .all(|s| s.row != span.row || s.col > 1),
        "column 1 must really be chrome for this test to mean anything"
    );
    assert_eq!(
        select::offset_at(&canvas, doc, on_rule, select::Bias::Start),
        Some(span.source_start),
        "a press on the rule takes the start of the cell's text"
    );
    // Column 1 sits before every span on the row, so it cannot tell "reading order" from
    // "row order" apart: a comparison that dropped the column and kept only the row would
    // land on the same answer. The rule *between* alpha and beta can: it sits after
    // alpha's span and before beta's, on the very same row, so only a genuine `(row,
    // col)` comparison picks beta over alpha for `Start` and alpha over beta for `End`.
    let beta = canvas
        .spans()
        .iter()
        .find(|s| &doc[s.source_start..s.source_end] == "beta")
        .expect("a span for beta");
    let between = Pos {
        row: span.row,
        col: span.col + span.cols,
    };
    assert!(
        between.col < beta.col,
        "the rule between the cells must actually sit before beta's span"
    );
    assert_eq!(
        select::offset_at(&canvas, doc, between, select::Bias::Start),
        Some(beta.source_start),
        "reading order, not row order: the rule between cells takes the next cell's start"
    );
    assert_eq!(
        select::offset_at(&canvas, doc, between, select::Bias::End),
        Some(span.source_end),
        "reading order, not row order: the rule between cells takes the previous cell's end"
    );
}

#[test]
fn a_cell_past_the_end_of_a_row_resolves_to_the_last_span_on_it() {
    let doc = "| alpha | beta |\n| --- | --- |\n| one | two |\n";
    let canvas = render(doc, 40);
    let span = canvas
        .spans()
        .iter()
        .find(|s| &doc[s.source_start..s.source_end] == "beta")
        .expect("a span for beta");
    let past = Pos {
        row: span.row,
        col: 200,
    };
    assert_eq!(
        select::offset_at(&canvas, doc, past, select::Bias::End),
        Some(span.source_end)
    );
}

#[test]
fn a_drag_entirely_on_chrome_selects_nothing() {
    // The interior of a diagram: box art, no labels under either endpoint.
    let doc = "```mermaid\nflowchart LR\n  A[Parse] --> B[Layout]\n```\n";
    let canvas = render(doc, 60);
    let lo = select::offset_at(&canvas, doc, Pos { row: 0, col: 0 }, select::Bias::Start);
    let hi = select::offset_at(&canvas, doc, Pos { row: 0, col: 1 }, select::Bias::End);
    assert!(
        lo >= hi,
        "an empty range, not the whole document: {lo:?}..{hi:?}"
    );
    // `lo >= hi` alone is satisfied by *either* fallback landing on the same value as
    // the other, which is not what an inverted fallback actually does — it moves both
    // ends toward each other, and `usize` can never go negative, so `>=` alone cannot
    // tell a single inverted fallback from a correct one. Pin the values themselves.
    //
    // `Bias::Start` no longer clamps to the document end here, and that is the point of
    // the change that moved it: a flowchart's labels carry spans now, so the *next* text
    // after this corner of box art is the `Parse` label, and §2.1 says an endpoint on
    // chrome takes it. `Bias::End` still clamps, because nothing precedes row 0.
    // `a_drag_on_chrome_with_no_spans_at_all_clamps_both_ways` keeps the clamp itself
    // covered on a document that really has no spans.
    let parse = canvas
        .spans()
        .iter()
        .find(|s| doc.get(s.source_start..s.source_end) == Some("Parse"))
        .expect("the Parse label carries a span");
    assert_eq!(
        lo,
        Some(parse.source_start),
        "the next text in document order is the first label"
    );
    assert_eq!(
        hi,
        Some(0),
        "no span at/before the cell clamps to the document start"
    );
}

#[test]
fn a_drag_on_chrome_with_no_spans_at_all_clamps_both_ways() {
    // A document whose rendering carries no spans anywhere, so both fallbacks have to
    // clamp rather than find a neighbour: `Bias::Start` to the document end and
    // `Bias::End` to its start. Inverted on purpose (see `Bias`), so the hull is empty
    // and never the whole document — the one answer design spec §2 rules out.
    let doc = "---\n";
    let canvas = render(doc, 60);
    assert!(
        canvas.spans().is_empty(),
        "a thematic break maps no text: {:?}",
        canvas.spans()
    );
    assert_eq!(
        select::offset_at(&canvas, doc, Pos { row: 0, col: 0 }, select::Bias::Start),
        Some(doc.len()),
        "no span at/after the cell clamps to the document end"
    );
    assert_eq!(
        select::offset_at(&canvas, doc, Pos { row: 0, col: 1 }, select::Bias::End),
        Some(0),
        "no span at/before the cell clamps to the document start"
    );
}

// --- The hull is the range between two endpoints (design spec §2) ----------------

#[test]
fn a_drag_across_a_table_selects_whole_cells_in_document_order() {
    // Dragging from the second cell of row 1 to the first cell of row 2 selects the
    // text between them in the *source*, which is row-major — not the rectangle the
    // two corners describe on screen. `drawn` and `drag` already exist for exactly
    // this (used throughout the drag tests above); no new test helper is needed.
    let doc = "| a | b |\n| --- | --- |\n| one | two |\n| three | four |\n";
    let canvas = render(doc, 40);
    let (two_row, two_col, _) = drawn(&canvas, "two");
    let (three_row, three_col, three_cols) = drawn(&canvas, "three");
    let sel = drag(
        Pos::new(two_row, two_col),
        Pos::new(three_row, three_col + three_cols - 1),
    );
    let (lo, hi) = select::source_hull(&canvas, doc, sel).expect("a hull");
    let text = &doc[lo..hi];
    assert!(text.starts_with("two"), "got {text:?}");
    assert!(text.ends_with("three"), "got {text:?}");
    assert!(
        text.contains('\n'),
        "the hull crosses the source's row boundary: {text:?}"
    );
}

// --- The highlight is painted from the hull (design spec §2) ---------------------

#[test]
fn a_table_border_is_never_highlighted() {
    let doc = "| a | b |\n| --- | --- |\n| one | two |\n";
    let canvas = render(doc, 40);
    let (one_row, one_col, _) = drawn(&canvas, "one");
    let (two_row, two_col, two_cols) = drawn(&canvas, "two");
    let sel = drag(
        Pos::new(one_row, one_col),
        Pos::new(two_row, two_col + two_cols - 1),
    );
    let ranges = select::highlighted_columns(&canvas, doc, sel, one_row);
    let row = canvas.row_text(one_row);
    for range in &ranges {
        for col in range.clone() {
            let ch = row.chars().nth(usize::from(col)).unwrap_or(' ');
            assert!(
                !"│├┤┬┴┼╭╮╰╯─".contains(ch),
                "chrome at column {col} is highlighted: {ch:?} in {row:?}"
            );
        }
    }
    assert!(!ranges.is_empty(), "the cells themselves are highlighted");
}

#[test]
fn a_selection_confined_to_one_cell_does_not_wash_its_neighbour() {
    // Both "one" and "two" carry spans on the same row, so a selection that covers
    // the whole row cannot tell the clipping guard apart from its absence (every span
    // on the row is inside the hull either way — a mutation that deletes the guard
    // still passes `a_table_border_is_never_highlighted`). Confining the drag to just
    // "one" is what actually exercises `span.source_end <= lo || span.source_start >=
    // hi`: without it, "two" is a span on the same row too and would be washed along
    // with it.
    let doc = "| a | b |\n| --- | --- |\n| one | two |\n";
    let canvas = render(doc, 40);
    let (one_row, one_col, one_cols) = drawn(&canvas, "one");
    let (two_row, two_col, _) = drawn(&canvas, "two");
    assert_eq!(
        one_row, two_row,
        "fixture assumption: both cells on one row"
    );
    let sel = drag(
        Pos::new(one_row, one_col),
        Pos::new(one_row, one_col + one_cols - 1),
    );
    let ranges = select::highlighted_columns(&canvas, doc, sel, one_row);
    assert!(!ranges.is_empty(), "\"one\" itself must be highlighted");
    for range in &ranges {
        assert!(
            range.end <= two_col,
            "the wash reaches into \"two\" at column {two_col}: {ranges:?}"
        );
    }
}

#[test]
fn the_highlight_stops_at_the_end_of_the_text() {
    let doc = "short line\n";
    let canvas = render(doc, 60);
    let (row, col, cols) = drawn(&canvas, "short line");
    let sel = drag(Pos::new(row, col), Pos::new(row, 59));
    let ranges = select::highlighted_columns(&canvas, doc, sel, row);
    let last = ranges.iter().map(|r| r.end).max().expect("a range");
    assert_eq!(last, col + cols, "the wash must not run to the pane edge");
}

#[test]
fn the_highlight_never_reaches_a_column_contiguous_next_span() {
    // A carried-over minor from Task 2: the far-endpoint probe can overshoot into a
    // column-contiguous next span, so dragging `bold` in `**bold**text` yields a hull
    // of "bold**" (the closing delimiter comes along, harmlessly, for the clipboard).
    // For the wash the same overshoot would be visible: it must not paint into
    // "text", which sits immediately after "**" with no gap between them on screen.
    let doc = "**bold**text\n";
    let canvas = render(doc, 40);
    let (row, bold_col, bold_cols) = drawn(&canvas, "bold");
    let sel = drag(
        Pos::new(row, bold_col),
        Pos::new(row, bold_col + bold_cols - 1),
    );
    let ranges = select::highlighted_columns(&canvas, doc, sel, row);
    let (text_row, text_col, _) = drawn(&canvas, "text");
    assert_eq!(text_row, row, "fixture assumption: both on one row");
    for range in &ranges {
        assert!(
            range.end <= text_col,
            "the wash reaches into \"text\" at column {}: {ranges:?}",
            text_col
        );
    }
}
#[test]
fn a_soft_line_break_washes_like_any_other_word_separator() {
    // A newline inside a paragraph is drawn as a space between two words. That space
    // is body text the reader dragged over, not chrome, so it has to wash like every
    // other separator. Three source lines reflow onto one row at this width, which
    // puts both newlines mid-row — the shape where the hole is visible. A soft break
    // that lands at a row end is swallowed by wrapping and hides the defect, so the
    // fixture must be one where it does not.
    let doc = "Alpha beta gamma\ndelta epsilon zeta\neta theta.\n";
    let canvas = render(doc, 60);
    let (row, first_col, _) = drawn(&canvas, "Alpha");
    let (last_row, last_col, last_cols) = drawn(&canvas, "theta.");
    assert_eq!(row, last_row, "fixture assumption: one reflowed row");
    let sel = drag(
        Pos::new(row, first_col),
        Pos::new(row, last_col + last_cols - 1),
    );
    let ranges = select::highlighted_columns(&canvas, doc, sel, row);
    let text = canvas.row_text(row);
    for col in first_col..last_col + last_cols {
        assert!(
            ranges.iter().any(|range| range.contains(&col)),
            "column {col} of the paragraph is not washed: {ranges:?} over {text:?}"
        );
    }
}

/// The document source as [`Doc`] stores it, which is what every byte offset in the
/// application indexes — never the bytes as they sat in the file.
///
/// `App` passes `self.doc.source()` to `extract`, to `highlighted_columns` and to
/// `Search`, so a test that passed its own string literal instead would be testing a
/// pairing the pager never makes. That distinction is invisible while the two are equal
/// and load-bearing the moment they are not, which is exactly the CRLF case.
fn read(markdown: &str) -> String {
    Doc::parse(markdown).source().to_string()
}

#[test]
fn a_crlf_soft_line_break_washes_and_copies_a_clean_newline() {
    // Line endings are normalised where the document is read, so by the time anything
    // here runs there is no CRLF left: the separator is one `\n` drawn as one space,
    // `Piece::anchored`'s length guard stops declining it without being touched, and
    // the clipboard cannot carry a `\r` because the document no longer holds one.
    //
    // This replaces `a_crlf_soft_line_break_declines_its_origin_and_leaves_the_clipboard_whole`,
    // whose two assertions this reverses on purpose (owner ruling: "copy paste should
    // always get clean \n newline"). Its second assertion pinned a stray trailing `\r`
    // on the clipboard — `extend_over_markup` walking to the line end over the undrawn
    // `\r` — which was the more visible half of the defect and is gone with the byte.
    let markdown = "Alpha beta gamma\r\ndelta epsilon zeta\r\n";
    let doc = read(markdown);
    let canvas = render(markdown, 60);
    let (row, first_col, _) = drawn(&canvas, "Alpha");
    let (last_row, last_col, last_cols) = drawn(&canvas, "zeta");
    assert_eq!(row, last_row, "fixture assumption: one reflowed row");
    let sel = drag(
        Pos::new(row, first_col),
        Pos::new(row, last_col + last_cols - 1),
    );
    let ranges = select::highlighted_columns(&canvas, &doc, sel, row);
    let separator = first_col + 16;
    assert_eq!(
        canvas.row_text(row).chars().nth(usize::from(separator)),
        Some(' '),
        "fixture assumption: the line ending is drawn as a space at column {separator}"
    );
    assert!(
        ranges.iter().any(|range| range.contains(&separator)),
        "the separator washes like any other: {ranges:?}"
    );
    let extract = select::extract(&canvas, &doc, sel).expect("the drag covered text");
    assert!(
        extract.from_source,
        "the clipboard still answers from source"
    );
    assert_eq!(
        extract.text, "Alpha beta gamma\ndelta epsilon zeta",
        "the clipboard gets a clean newline and no carriage return"
    );
}

#[test]
fn a_crlf_document_drags_exactly_like_its_lf_twin() {
    // The strongest statement of the rule available at this level, and the reason the
    // fix belongs at the read rather than at three places downstream: a CRLF document
    // is not *handled* by the pager, it has ceased to exist by the time the pager sees
    // it. `Canvas` compares every cell, every style and every span, so this covers the
    // paint; the drag covers the clipboard.
    //
    // The fixture is deliberately broad, because every construct that maps cells back to
    // bytes does it by its own route and each of those routes used to have a line-ending
    // clause: a paragraph reflowed across soft breaks (`inline::collect`), a fenced block
    // (`convert::code_lines`' suffix match), an indented one (same rule, four spaces), a
    // quoted fence (same rule again, over `> `), a quoted *diagram* (an `Atom`, whose
    // `strip_to_content` pairing of comrak's literal against the source lines is
    // positional and line for line — the construct with the most to lose from a line
    // ending that is two bytes on one side and one on the other), a table, a list, and an
    // escape and an entity, whose spans `convert::split_transcriptions` cuts by aligning
    // the drawn text against the source byte for byte.
    let body = |eol: &str| {
        [
            "# Title",
            "",
            "Alpha beta gamma",
            "delta epsilon zeta",
            "eta theta \\* and &amp; onwards.",
            "",
            "```rust",
            "let a = 1;",
            "",
            "let b = 2;",
            "```",
            "",
            "> quoted prose",
            "> ",
            "> ```text",
            "> quoted code",
            "> ```",
            "",
            "> ```mermaid",
            "> flowchart LR",
            ">   A[Parse] --> B[Layout]",
            "> ```",
            "",
            "- one item",
            "- another item",
            "",
            "| alpha | beta |",
            "| --- | --- |",
            "| one | two |",
            "",
            "    indented code",
            "",
        ]
        .join(eol)
            + eol
    };
    let crlf = body("\r\n");
    let lf = body("\n");
    // The claim underneath every other assertion here: the two documents are not merely
    // handled alike, they *are* one document by the time anything indexes them.
    assert_eq!(read(&crlf), lf, "a CRLF document reads as its LF twin");
    let (crlf_canvas, lf_canvas) = (render(&crlf, 60), render(&lf, 60));
    // Compared in three widening steps rather than by one `assert_eq!` on the canvases:
    // a whole-`Canvas` mismatch prints every cell of both, which is 170KB of `Debug`
    // nobody can read. The narrow assertions name what differs; the last one is still
    // the full comparison, so nothing is given up for the legibility.
    let rows = |c: &Canvas| (0..c.height()).map(|r| c.row_text(r)).collect::<Vec<_>>();
    assert_eq!(rows(&crlf_canvas), rows(&lf_canvas), "same drawn text");
    assert_eq!(
        crlf_canvas.spans(),
        lf_canvas.spans(),
        "same spans, naming the same document bytes"
    );
    assert!(
        crlf_canvas == lf_canvas,
        "the canvases differ in something other than their text or their spans"
    );
    let sel = drag(
        Pos::new(0, 0),
        Pos::new(
            crlf_canvas.height() - 1,
            crlf_canvas.width().saturating_sub(1),
        ),
    );
    let crlf_extract = select::extract(&crlf_canvas, &read(&crlf), sel).expect("text");
    let lf_extract = select::extract(&lf_canvas, &read(&lf), sel).expect("text");
    assert_eq!(
        crlf_extract.text, lf_extract.text,
        "both documents copy the same bytes"
    );
    assert!(
        !crlf_extract.text.contains('\r'),
        "no carriage return anywhere on the clipboard: {:?}",
        crlf_extract.text
    );
    // Anchors proving the drag is not degenerate — it really did reach the document
    // through the spans rather than falling back to the drawn cells, and it really did
    // cover the constructs the fixture was widened for. Deliberately anchors, not a
    // whole-string comparison: what a drag over a diagram copies is Task 5's ruling and
    // is still moving, and this test is about line endings.
    assert!(crlf_extract.from_source, "answered from the document");
    assert!(crlf_extract.text.contains("let a = 1;\n\nlet b = 2;"));
    assert!(crlf_extract.text.contains("> quoted code"));
    assert!(crlf_extract.text.contains("```mermaid\nflowchart LR"));
    assert!(crlf_extract.text.contains("| one | two |"));
}

#[test]
fn a_crlf_fenced_code_block_copies_clean_newlines() {
    // The one case where preserving `\r` could be argued — a code block is meant to be
    // copied verbatim, and its bytes are the author's. The ruling is explicit that it
    // is not argued: copy-paste always gets clean `\n`. Dragged on its own here rather
    // than as part of the whole document, because a code block reaches the clipboard
    // through `code_lines`' per-line provenance and not through the inline path.
    let markdown = "```rust\r\nlet a = 1;\r\nlet b = 2;\r\n```\r\n";
    let doc = read(markdown);
    let canvas = render(markdown, 40);
    let (first_row, first_col, _) = drawn(&canvas, "let a = 1;");
    let (last_row, last_col, last_cols) = drawn(&canvas, "let b = 2;");
    let sel = drag(
        Pos::new(first_row, first_col),
        Pos::new(last_row, last_col + last_cols - 1),
    );
    let extract = select::extract(&canvas, &doc, sel).expect("the drag covered code");
    assert!(extract.from_source, "code answers from source");
    assert_eq!(extract.text, "let a = 1;\nlet b = 2;");
}

/// Asserts that every column of the paragraph on `row` is washed by a drag across it.
fn assert_paragraph_washes(doc: &str, first: &str, last: &str) -> select::Extract {
    let canvas = render(doc, 60);
    let (row, first_col, _) = drawn(&canvas, first);
    let (last_row, last_col, last_cols) = drawn(&canvas, last);
    assert_eq!(row, last_row, "fixture assumption: one reflowed row");
    let sel = drag(
        Pos::new(row, first_col),
        Pos::new(row, last_col + last_cols - 1),
    );
    let ranges = select::highlighted_columns(&canvas, doc, sel, row);
    let text = canvas.row_text(row);
    for col in first_col..last_col + last_cols {
        assert!(
            ranges.iter().any(|range| range.contains(&col)),
            "column {col} of the paragraph is not washed: {ranges:?} over {text:?}"
        );
    }
    select::extract(&canvas, doc, sel).expect("the drag covered text")
}

#[test]
fn an_escape_no_longer_darkens_its_whole_paragraph() {
    // One `\*` used to cost every span in the paragraph: comrak reports the whole run
    // as one text node whose source is a byte longer than its text, and a node whose
    // lengths disagree carries no origin. Nothing highlighted and the clipboard fell
    // through to the drawn cells. Split at the escape, the prose either side is an
    // exact copy of its source again and the character the backslash protected is a
    // copy of the byte after it, so the whole row washes.
    let doc = "Alpha \\* beta gamma.\n";
    let extract = assert_paragraph_washes(doc, "Alpha", "gamma.");
    assert!(extract.from_source, "and the clipboard answers from source");
    assert_eq!(
        extract.text, "Alpha \\* beta gamma.",
        "the source, backslash and all"
    );
}

#[test]
fn an_entity_no_longer_darkens_its_whole_paragraph() {
    // The other half of the same defect. `&amp;` draws a character that copies no
    // byte of its source, so the run around it is split and the entity's one cell is
    // anchored to the whole of `&amp;` — those five bytes are exactly what drew it.
    let doc = "Alpha &amp; beta gamma.\n";
    let extract = assert_paragraph_washes(doc, "Alpha", "gamma.");
    assert!(extract.from_source, "and the clipboard answers from source");
    assert_eq!(
        extract.text, "Alpha &amp; beta gamma.",
        "the source, unexpanded"
    );
}

#[test]
fn a_numeric_entity_no_longer_darkens_its_whole_paragraph() {
    for doc in ["Alpha &#65; beta gamma.\n", "Alpha &#x41; beta gamma.\n"] {
        let extract = assert_paragraph_washes(doc, "Alpha", "gamma.");
        assert!(extract.from_source, "{doc:?}: from source");
        assert_eq!(extract.text, doc.trim_end(), "{doc:?}");
    }
}

#[test]
fn an_escape_at_the_very_start_of_a_paragraph_keeps_its_spans() {
    // A boundary an alignment walk can drop: there is no prose run in front of the
    // escape to flush, so the first thing the walk does is diverge.
    let doc = "\\*Alpha beta gamma.\n";
    let extract = assert_paragraph_washes(doc, "*Alpha", "gamma.");
    assert!(extract.from_source, "the clipboard answers from source");
    assert_eq!(extract.text, "\\*Alpha beta gamma.");
}

#[test]
fn an_escape_at_the_very_end_of_a_paragraph_keeps_its_spans() {
    // The other boundary: the walk diverges with nothing left to re-synchronise
    // against, so it has to finish on the escape rather than look past it.
    let doc = "Alpha beta gamma\\*\n";
    let extract = assert_paragraph_washes(doc, "Alpha", "gamma*");
    assert!(extract.from_source, "the clipboard answers from source");
    assert_eq!(extract.text, "Alpha beta gamma\\*");
}

#[test]
fn an_entity_at_either_end_of_a_paragraph_keeps_its_spans() {
    for (doc, first, last, text) in [
        (
            "&amp;Alpha beta gamma.\n",
            "&Alpha",
            "gamma.",
            "&amp;Alpha beta gamma.",
        ),
        (
            "Alpha beta gamma&amp;\n",
            "Alpha",
            "gamma&",
            "Alpha beta gamma&amp;",
        ),
    ] {
        let extract = assert_paragraph_washes(doc, first, last);
        assert!(extract.from_source, "{doc:?}: from source");
        assert_eq!(extract.text, text, "{doc:?}");
    }
}

#[test]
fn dragging_the_character_an_escape_protected_copies_the_escape() {
    // The escaped character's own cell is a copy of the byte after the backslash, so
    // it resolves exactly; the backslash is undrawn markup, and `extend_over_markup`
    // brings it along exactly as it brings a heading's `#`.
    let doc = "Alpha \\* beta gamma.\n";
    let canvas = render(doc, 60);
    let extract = drag_over(&canvas, doc, "*");
    assert!(extract.from_source, "one cell, resolved from source");
    assert_eq!(extract.text, "\\*", "the escape, not the character alone");
}

#[test]
fn dragging_the_character_an_entity_produced_copies_the_entity() {
    // A transcribed cell has no interior: its span names the whole entity, so a drag
    // over that one cell copies `&amp;` and never a fragment of it.
    let doc = "Alpha &amp; beta gamma.\n";
    let canvas = render(doc, 60);
    let extract = drag_over(&canvas, doc, "&");
    assert!(extract.from_source, "one cell, resolved from source");
    assert_eq!(extract.text, "&amp;", "the whole entity");
}

#[test]
fn a_search_hit_after_an_escape_lands_on_the_cells_it_drew() {
    // Search runs over the source and projects through the same spans. With the
    // paragraph unspanned there was nothing to project onto at all; with it split,
    // the words after the escape are one run of cells again.
    let doc = "Alpha \\* beta gamma.\n";
    let canvas = render(doc, 60);
    let mut search = crate::search::Search::new(doc, "beta gamma", SearchMode::Literal)
        .expect("a valid pattern");
    search.locate(doc, canvas.spans());
    let hit = search.hits().first().expect("the pattern matches");
    assert_eq!(
        hit.segments.len(),
        1,
        "one unbroken run of cells: {:?}",
        hit.segments
    );
    assert_eq!(
        hit.segments[0].cols,
        u16::try_from("beta gamma".len()).expect("short"),
        "as wide as the rendered match"
    );
    let (row, col, _) = drawn(&canvas, "beta");
    assert_eq!(
        (hit.segments[0].row, hit.segments[0].col),
        (row, col),
        "and it starts where the words are drawn"
    );
}

#[test]
fn an_entity_wider_than_one_column_declines_its_origin() {
    // The fail-closed edge of the transcription rule. A span's source is otherwise a
    // byte-for-byte copy of the cells it names, which is what lets `select` and
    // `search` convert between bytes and columns inside it. A transcribed span breaks
    // that, and it is only harmless while the span has no interior — one column. An
    // emoji entity draws two, so it is declined and that cell stays dark rather than
    // handing the column arithmetic a body it cannot walk.
    let doc = "Alpha &#x1F600; beta gamma.\n";
    let canvas = render(doc, 60);
    let (row, col, _) = drawn(&canvas, "\u{1F600}");
    assert!(
        !canvas
            .spans()
            .iter()
            .any(|s| s.row == row && s.col <= col && col < s.col + s.cols),
        "the emoji cell is deliberately unspanned: {:?}",
        canvas.spans()
    );
    // And the prose either side of it keeps its provenance, which is the whole point:
    // the degradation is one cell wide, not one paragraph wide.
    let extract = drag_over(&canvas, doc, "gamma.");
    assert!(extract.from_source, "the words after it still resolve");
    assert_eq!(extract.text, "gamma.");
}

#[test]
fn a_run_that_cannot_be_aligned_keeps_todays_honest_fallback() {
    // `&fjlig;` expands to two characters and the alignment declines it, so this
    // paragraph keeps the behaviour every escape used to get: no spans, and a
    // clipboard that says so. Pinned because "fail closed" has to be a decision that
    // stays visible, not a case nobody looked at.
    let doc = "Alpha &fjlig; beta gamma.\n";
    let canvas = render(doc, 60);
    let (row, _, _) = drawn(&canvas, "Alpha");
    assert!(
        !canvas.spans().iter().any(|s| s.row == row),
        "the paragraph carries no spans: {:?}",
        canvas.spans()
    );
    let extract = drag_over(&canvas, doc, "gamma.");
    assert!(
        !extract.from_source,
        "and the clipboard admits it is the rendered text"
    );
}

#[test]
fn a_search_hit_across_a_soft_line_break_highlights_in_one_piece() {
    // Anchoring the soft break's space is not only about the selection wash: a search
    // runs over the source and projects its hit onto cells through the same spans, so
    // a match that crosses the newline used to be drawn as two lit runs with a dark
    // cell between them. One span, one segment.
    let doc = "Alpha beta gamma\ndelta epsilon zeta\n";
    let canvas = render(doc, 60);
    let mut search = crate::search::Search::new(doc, "gamma\\s+delta", SearchMode::Regex)
        .expect("a valid pattern");
    search.locate(doc, canvas.spans());
    let hit = search.hits().first().expect("the pattern matches");
    assert_eq!(
        hit.segments.len(),
        1,
        "the match is one unbroken run of cells: {:?}",
        hit.segments
    );
    assert_eq!(
        hit.segments[0].cols,
        u16::try_from("gamma delta".len()).expect("short"),
        "and it is as wide as the rendered match"
    );
}
