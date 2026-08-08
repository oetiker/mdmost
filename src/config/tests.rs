//! Configuration tests.

use std::path::Path;

use super::*;

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
    assert!(!loaded.config.icons);
    assert!(loaded.config.line_numbers);
    assert_eq!(loaded.config.scroll_step, 5);
    assert!(loaded.config.toc_open);
    assert_eq!(loaded.config.toc_width, 40);
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
fn an_unknown_key_names_itself_and_falls_back() {
    let loaded = Config::parse_str("icons = true\nthemee = \"dark\"\n", path());
    assert_eq!(loaded.config, Config::default());
    let ConfigError::Parse { key, line, .. } = &loaded.problems[0] else {
        panic!("expected a parse problem");
    };
    assert_eq!(key.as_deref(), Some("themee"));
    assert_eq!(*line, 2);
}

#[test]
fn a_bad_theme_name_is_reported_but_the_rest_survives() {
    let loaded = Config::parse_str("theme = \"nope\"\nicons = false\n", path());
    assert_eq!(loaded.config.theme, "dark");
    assert!(!loaded.config.icons, "the good settings must still apply");
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
