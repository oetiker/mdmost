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
    // The cap lives in the configuration and is applied by `wide::render_scrollable`;
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
    let indent = row.len() - row.trim_start().len();
    assert!(
        indent > 30,
        "the paragraph was not capped and centred: {indent} columns of indent\n{row}"
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

    // Force a render wider than the viewport and the offset becomes real.
    let mut wide = App::new(
        Doc::parse(SAMPLE),
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
    let canvas = super::wide::render_scrollable(
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
    // `super::wide` cannot see `render::code`'s private constant, so it keeps its own
    // copy. If the renderer ever changes the glyph, widening silently stops working;
    // this is the tripwire.
    let doc = Doc::parse("```text\nAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n```\n");
    let canvas = crate::render::render_document(
        &doc,
        20,
        &crate::theme::Theme::default_dark(),
        &crate::render::RenderOptions::new(false, false),
    );
    let text = canvas.plain_text();
    assert!(
        text.contains(super::wide::OVERFLOW_MARKER),
        "the renderer still marks clipped content with {:?}: {text}",
        super::wide::OVERFLOW_MARKER
    );
}

#[test]
fn the_quote_bar_matches_the_renderer() {
    // The other private constant `super::wide` keeps a copy of. If the renderer ever
    // changes the glyph, a quote's separator rows stop reading as blank, the quote
    // becomes one run again, and a wide block inside it silently drags the quoted prose
    // off the screen — with no test failing anywhere near the change.
    let doc = Doc::parse("> before\n>\n> after\n");
    let canvas = crate::render::render_document(
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
        super::wide::QUOTE_BAR,
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
    let canvas = crate::render::render_document(
        &doc,
        40,
        &theme,
        &crate::render::RenderOptions::new(false, true),
    );
    let pinned = super::wide::pinned_prefix(&canvas);
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
    let canvas = super::wide::render_scrollable(&doc, width, None, &theme, &options);
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
    let reach = super::wide::scroll_reach(canvas, width);
    let pinned = super::wide::pinned_prefix(canvas);
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
/// ([`super::wide::scroll_reach`]) — which is exactly the case that must *not* pin the
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
    let canvas = crate::render::render_document(
        &doc,
        40,
        &theme,
        // Line numbers *off*: this block has no gutter, so nothing may be pinned.
        &crate::render::RenderOptions::new(false, false),
    );
    let pinned = super::wide::pinned_prefix(&canvas);
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
    let pinned = super::wide::pinned_prefix(&canvas);
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
        super::wide::render_scrollable(
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
    // The tension named in `wide::scroll_reach`: rows scroll as a *run* so a ragged block
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
fn a_drag_over_a_code_block_falls_back_to_what_is_drawn() {
    let mut app = pager(MARKUP);
    let canvas = app.canvas().clone();
    // Code blocks carry no search spans — only `render::inline` records them — so there
    // is nothing to invert and the rendered cells are the answer. For code that is the
    // source, modulo the frame.
    let extract = drag_over(&canvas, MARKUP, "fn main() {}");
    assert_eq!(extract.text, "fn main() {}");
    assert!(
        !extract.from_source,
        "the pager must not claim this came from the source map"
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
#[test]
fn a_drag_ending_inside_spanless_content_stops_at_the_last_mapped_byte() {
    let mut app = pager(MARKUP);
    let canvas = app.canvas().clone();
    let (top, col, _) = drawn(&canvas, "item one");
    let (code, _, _) = drawn(&canvas, "fn main() {}");
    let extract = select::extract(
        &canvas,
        MARKUP,
        drag(Pos::new(top, col), Pos::new(code, 40)),
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
    use super::clipboard::Delivery;
    assert_eq!(
        Delivery::Confirmed.message(47, true),
        ("copied 47 bytes of Markdown source".to_string(), false)
    );
    let (text, is_error) = Delivery::Sent.message(47, false);
    assert!(
        text.contains("unconfirmed") && text.contains("rendered text"),
        "OSC 52 is fire-and-forget and the bar must not pretend otherwise: {text:?}"
    );
    assert!(!is_error);
    let (text, is_error) = Delivery::Failed("no clipboard".to_string()).message(47, true);
    assert!(is_error && text.contains("no clipboard"));
}

/// An X11 or Wayland selection belongs to a process, so a copy that only the local
/// display server holds is a copy that ends when `mdmost` does — unless a clipboard
/// manager takes it over, which is exactly the thing we cannot check. The bar therefore
/// says what it knows: this one is held while the pager runs.
#[test]
fn the_copy_report_marks_a_copy_only_this_process_holds() {
    use super::clipboard::Delivery;
    let (text, is_error) = Delivery::LocalOnly.message(47, false);
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
            .message(47, false)
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
