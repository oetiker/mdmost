//! Application-state tests.
//!
//! Every test here drives the real state machine with no terminal in sight, which is
//! the separation design spec §13 requires.

use super::app::{App, AppOptions, Focus, Overlay, PromptKind};
use super::help;
use super::icons::meter;
use crate::config::{Action, Config, Key, KeyBindings, KeyCode};
use crate::doc::Doc;

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
    let heading_before = app.current_heading();
    app.resize(40, 12);
    assert!(app.scroll() <= app.max_scroll());
    assert_eq!(
        app.current_heading(),
        heading_before,
        "a reflow must not move the reader to a different section"
    );
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
fn an_invalid_regex_falls_back_to_a_literal_search() {
    let mut app = pager("a (b) c\n");
    app.run_search("(b");
    assert_eq!(app.search().len(), 1, "`(b` must be found literally");
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
fn escape_closes_the_toc_before_it_quits() {
    let mut app = pager(SAMPLE);
    app.act(Action::ToggleToc);
    app.act(Action::Cancel);
    assert!(!app.should_quit(), "escape must close the pane first");
    assert_eq!(app.focus(), Focus::Document);
    app.act(Action::Cancel);
    assert!(app.should_quit());
}

#[test]
fn the_help_overlay_opens_and_any_key_dismisses_it() {
    let mut app = pager(SAMPLE);
    app.act(Action::Help);
    assert_eq!(*app.overlay(), Overlay::Help);
    app.on_key(Key::char('j'));
    assert_eq!(*app.overlay(), Overlay::None);
    assert_eq!(app.scroll(), 0, "the dismissing key must not also scroll");
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
fn the_meter_is_exactly_as_wide_as_asked() {
    for fraction in [0.0, 0.01, 0.5, 0.999, 1.0] {
        for width in [1usize, 4, 8, 20] {
            assert_eq!(
                crate::text::display_width(&meter(fraction, width)),
                width,
                "fraction {fraction} at width {width}"
            );
        }
    }
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
