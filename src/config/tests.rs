//! Configuration tests.

use std::path::Path;

use super::*;

/// The README, so its documented configuration can be checked against the parser.
const README: &str = include_str!("../../README.md");

/// A throwaway path, used only in error messages.
fn path() -> &'static Path {
    Path::new("/tmp/mdless/config.toml")
}

#[test]
fn empty_config_is_the_defaults() {
    let loaded = Config::parse_str("", path());
    assert!(loaded.problems.is_empty());
    assert_eq!(loaded.config, Config::default());
}

#[test]
fn scalars_are_applied() {
    let loaded = Config::parse_str(
        r#"
theme = "light"
icons = false
line_numbers = true
scroll_step = 5

[toc]
open = true
width = 40
"#,
        path(),
    );
    assert!(loaded.problems.is_empty(), "{:?}", loaded.problems);
    assert_eq!(loaded.config.theme, "light");
    assert_eq!(loaded.config.icons, Some(false));
    assert!(loaded.config.line_numbers);
    assert_eq!(loaded.config.scroll_step, 5);
    assert!(loaded.config.toc_open);
    assert_eq!(loaded.config.toc_width, 40);
}

#[test]
fn an_absent_icons_key_stays_undecided() {
    // The tri-state is the whole point: an absent key must reach detection rather than
    // being answered here. If this ever became `Some(false)`, autodetection would be
    // dead code and every terminal would get plain Unicode for ever.
    let loaded = Config::parse_str("theme = \"dark\"\n", path());
    assert!(loaded.problems.is_empty(), "{:?}", loaded.problems);
    assert_eq!(loaded.config.icons, None);

    for (text, expected) in [
        ("icons = true\n", Some(true)),
        ("icons = false\n", Some(false)),
    ] {
        let loaded = Config::parse_str(text, path());
        assert_eq!(loaded.config.icons, expected, "for {text:?}");
    }
}

#[test]
fn malformed_toml_falls_back_to_defaults_with_a_located_error() {
    let loaded = Config::parse_str("theme = \n", path());
    assert_eq!(loaded.config, Config::default());
    assert_eq!(loaded.problems.len(), 1);
    let ConfigError::Parse { line, .. } = &loaded.problems[0] else {
        panic!("expected a parse problem, got {:?}", loaded.problems[0]);
    };
    assert_eq!(*line, 1);
}

#[test]
fn an_unknown_key_names_itself_and_costs_only_itself() {
    // It used to cost the whole file: `deny_unknown_fields` failed the entire parse, so
    // one typo silently took the reader's theme, icons and every key binding with it —
    // while the README promised the opposite, and while an unknown *action* and an
    // unknown *theme* both already degraded gracefully.
    let loaded = Config::parse_str("icons = true\nthemee = \"dark\"\n", path());
    let ConfigError::Parse { key, line, .. } = &loaded.problems[0] else {
        panic!("expected a parse problem");
    };
    assert_eq!(key.as_deref(), Some("themee"));
    assert_eq!(*line, 2);
    assert_eq!(
        loaded.config.icons,
        Some(true),
        "the settings around the bad key must survive it"
    );
}

#[test]
fn an_unknown_key_inside_toc_costs_only_itself_too() {
    let loaded = Config::parse_str("icons = false\n[toc]\nopen = true\nwidht = 40\n", path());
    assert_eq!(loaded.problems.len(), 1, "{:?}", loaded.problems);
    assert!(loaded.problems[0].to_string().contains("widht"));
    assert!(loaded.config.toc_open, "the good [toc] key must survive");
    assert_eq!(loaded.config.icons, Some(false));
    assert_eq!(loaded.config.toc_width, DEFAULT_TOC_WIDTH);
}

#[test]
fn the_configuration_example_in_the_readme_is_valid() {
    // The README's example used `toc_open` and `toc_width`, which have never been keys —
    // the real ones live in `[toc]`. Copying the documented starting configuration
    // therefore produced a file that was rejected in full, so the reader got none of the
    // settings they had just been shown and, in the TUI, never saw the warning either:
    // it went to stderr before the alternate screen opened and the restore wiped it.
    //
    // Prose about configuration cannot be checked by reading it, so it is checked here.
    let example = README
        .split("```toml")
        .nth(1)
        .and_then(|rest| rest.split("```").next())
        .expect("the README must contain a toml example");

    let loaded = Config::parse_str(example, path());
    assert!(
        loaded.problems.is_empty(),
        "the README's example configuration does not load cleanly: {:?}",
        loaded.problems
    );

    // And it must be the settings it claims to be, not merely parseable.
    assert_eq!(loaded.config.theme, "dark");
    assert_eq!(loaded.config.icons, Some(true));
    assert!(!loaded.config.toc_open);
    assert_eq!(loaded.config.toc_width, 32);
    assert_eq!(loaded.config.scroll_step, 3);

    // The example also defines a theme; it must really resolve, since a `[themes.*]`
    // table that silently does nothing would be the same class of lie.
    assert!(
        loaded.config.resolve_theme("midnight").is_ok(),
        "the example's [themes.midnight] must resolve"
    );
}

#[test]
fn the_body_width_cap_is_read_and_range_checked() {
    let loaded = Config::parse_str("body_width = 72\n", path());
    assert!(loaded.problems.is_empty(), "{:?}", loaded.problems);
    assert_eq!(loaded.config.body_width, Some(72));

    // Zero is how the file says "no cap", so that turning it off is a value rather
    // than a key you have to know to delete.
    let loaded = Config::parse_str("body_width = 0\n", path());
    assert!(loaded.problems.is_empty(), "{:?}", loaded.problems);
    assert_eq!(loaded.config.body_width, None);

    // Out of range keeps the default and says so, the way `toc.width` does.
    let loaded = Config::parse_str("body_width = 3\n", path());
    assert_eq!(loaded.problems.len(), 1, "{:?}", loaded.problems);
    assert_eq!(loaded.config.body_width, Some(DEFAULT_BODY_WIDTH));
}

#[test]
fn several_unknown_keys_are_all_reported() {
    let loaded = Config::parse_str("nope = 1\ntheme = \"light\"\nalso_nope = 2\n", path());
    assert_eq!(loaded.problems.len(), 2, "{:?}", loaded.problems);
    assert_eq!(loaded.config.theme, "light");
}

#[test]
fn a_known_key_with_the_wrong_type_is_still_fatal() {
    // Distinct from an unknown key: the reader meant *this* setting and got it wrong, so
    // there is no value to carry on with and guessing one would be worse than saying so.
    let loaded = Config::parse_str("scroll_step = \"fast\"\n", path());
    assert_eq!(loaded.config, Config::default());
    assert_eq!(loaded.problems.len(), 1);
}

#[test]
fn a_bad_theme_name_is_reported_but_the_rest_survives() {
    let loaded = Config::parse_str("theme = \"nope\"\nicons = false\n", path());
    assert_eq!(loaded.config.theme, "dark");
    assert_eq!(
        loaded.config.icons,
        Some(false),
        "the good settings must still apply"
    );
    assert_eq!(loaded.problems.len(), 1);
    assert!(loaded.problems[0].to_string().contains("nope"));
}

#[test]
fn an_out_of_range_toc_width_is_rejected_not_clamped_silently() {
    let loaded = Config::parse_str("[toc]\nwidth = 500\n", path());
    assert_eq!(loaded.config.toc_width, DEFAULT_TOC_WIDTH);
    assert_eq!(loaded.problems.len(), 1);
}

#[test]
fn key_bindings_can_be_added_replaced_and_removed() {
    let loaded = Config::parse_str(
        r#"
[keys]
"ctrl-n" = "line_down"
"j" = "line_up"
"q" = "none"
"#,
        path(),
    );
    assert!(loaded.problems.is_empty(), "{:?}", loaded.problems);
    let keys = &loaded.config.keys;
    assert_eq!(keys.action(&Key::ctrl('n')), Some(Action::LineDown));
    assert_eq!(keys.action(&Key::char('j')), Some(Action::LineUp));
    assert_eq!(keys.action(&Key::char('q')), None);
    // Untouched defaults survive.
    assert_eq!(keys.action(&Key::char('k')), Some(Action::LineUp));
}

#[test]
fn a_bad_binding_is_reported_and_the_others_still_load() {
    let loaded = Config::parse_str(
        "[keys]\n\"ctrl-n\" = \"fly_to_the_moon\"\n\"x\" = \"quit\"\n",
        path(),
    );
    assert_eq!(loaded.problems.len(), 1);
    assert!(loaded.problems[0].to_string().contains("fly_to_the_moon"));
    assert_eq!(
        loaded.config.keys.action(&Key::char('x')),
        Some(Action::Quit)
    );
}

#[test]
fn an_unparseable_chord_is_reported() {
    let loaded = Config::parse_str("[keys]\n\"ctrl-nonsense\" = \"quit\"\n", path());
    assert_eq!(loaded.problems.len(), 1);
    assert!(loaded.problems[0].to_string().contains("ctrl-nonsense"));
}

#[test]
fn a_custom_theme_inherits_from_its_base() {
    let loaded = Config::parse_str(
        r##"
theme = "midnight"

[themes.midnight]
base = "dark"
accent = "#ff0000"
"##,
        path(),
    );
    assert!(loaded.problems.is_empty(), "{:?}", loaded.problems);
    let theme = loaded
        .config
        .resolve_theme("midnight")
        .expect("the configured theme must resolve");
    assert_eq!(theme.palette.accent, Color::rgb(0xff, 0, 0));
    assert_eq!(theme.palette.bg, Theme::default_dark().palette.bg);
    assert!(theme.is_dark);
    assert_eq!(loaded.config.theme, "midnight");
}

#[test]
fn a_bad_colour_costs_that_slot_and_not_the_theme() {
    let loaded = Config::parse_str(
        "[themes.broken]\naccent = \"not a colour\"\ngreen = \"#00ff00\"\n",
        path(),
    );
    assert_eq!(loaded.problems.len(), 1, "one slot, one problem");
    let theme = loaded
        .config
        .resolve_theme("broken")
        .expect("the theme survives its one bad slot");
    assert_eq!(
        theme.palette.accent,
        Theme::default_dark().palette.accent,
        "the bad slot falls back to the base theme"
    );
    assert_eq!(
        theme.palette.green,
        Color::rgb(0, 0xff, 0),
        "the good slot is kept"
    );
}

#[test]
fn a_named_config_file_that_is_missing_is_reported() {
    // The default config being absent is silent; a file the user named is a typo.
    let loaded = Config::load_from(Path::new("/nonexistent/mdless/config.toml"));
    assert_eq!(loaded.problems.len(), 1);
    assert_eq!(loaded.config, Config::default());
}

#[test]
fn theme_cycling_visits_every_theme_and_wraps() {
    let loaded = Config::parse_str("[themes.zed]\nbase = \"light\"\n", path());
    let config = loaded.config;
    let names = config.theme_names();
    assert_eq!(names, vec!["dark", "light", "zed"]);
    assert_eq!(config.next_theme_name("dark"), "light");
    assert_eq!(config.next_theme_name("zed"), "dark");
    assert_eq!(config.next_theme_name("unheard of"), "dark");
}

#[test]
fn chords_round_trip_through_their_canonical_form() {
    for text in [
        "j", "G", "/", "-", "ctrl-d", "alt-x", "space", "pgdn", "f1", "enter", "esc", "tab", "up",
    ] {
        let key = Key::parse(text).unwrap_or_else(|| panic!("`{text}` should parse"));
        let round_tripped =
            Key::parse(&key.canonical()).unwrap_or_else(|| panic!("`{text}` should round-trip"));
        assert_eq!(key, round_tripped, "for `{text}`");
    }
}

#[test]
fn chord_parsing_is_case_and_separator_insensitive() {
    assert_eq!(Key::parse("Ctrl-D"), Key::parse("ctrl+d"));
    assert_eq!(Key::parse("CTRL-d"), Some(Key::ctrl('d')));
    assert_eq!(Key::parse("PgDn"), Key::parse("pagedown"));
    assert_eq!(Key::parse(""), None);
    assert_eq!(Key::parse("ctrl-wat"), None);
}

#[test]
fn every_action_has_a_default_binding_and_a_description() {
    let bindings = KeyBindings::defaults();
    for action in Action::ALL {
        assert!(
            !bindings.keys_for(*action).is_empty(),
            "`{action}` has no default binding"
        );
        assert!(!action.description().is_empty());
        assert_eq!(Action::parse(action.name()), Some(*action));
    }
}

#[test]
fn a_config_theme_shadowing_a_builtin_does_not_stall_the_cycle() {
    // `resolve_theme` prefers the configured theme, so listing the name twice would
    // leave `t` cycling from `dark` to `dark` for ever.
    let loaded = Config::parse_str("[themes.dark]\nbase = \"light\"\n", path());
    let config = loaded.config;
    assert_eq!(config.theme_names(), vec!["dark", "light"]);
    assert_eq!(config.next_theme_name("dark"), "light");
    assert_eq!(config.next_theme_name("light"), "dark");
}
