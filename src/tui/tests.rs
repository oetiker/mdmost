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
fn toggling_icons_invalidates_the_render_cache() {
    // The render cache key must include RenderOptions. If it does not, the canvas
    // rendered with Nerd Font glyphs is served again after icons are switched off —
    // a stale frame that looks almost right, which is the worst kind.
    let mut app = pager(SAMPLE);
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
fn the_status_bar_reports_a_horizontal_offset_only_when_scrolled() {
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
