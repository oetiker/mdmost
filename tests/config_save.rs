//! Saving settings back to the configuration file, without destroying it.
//!
//! Design spec §12.1. The reader presses `S` and the settings they are looking at are
//! there next time. The file they get back is the file they wrote — their comments,
//! their ordering, their keys from a newer version of mdless — with the values this
//! version knows about brought up to date and nothing else touched.
//!
//! Every test here writes inside a temporary directory of its own. Nothing in this
//! file may ever go near the real `~/.config/mdless/config.toml`.

use std::path::{Path, PathBuf};

use mdless::config::{Action, Config};
use mdless::doc::Doc;
use mdless::tui::{App, AppOptions};

/// A directory that removes itself, so a test cannot leak into the developer's home.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let base = std::env::temp_dir().join(format!(
            "mdless-config-save-{}-{}-{name}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|since| since.as_nanos())
                .unwrap_or_default()
        ));
        std::fs::create_dir_all(&base).expect("temp dir");
        Self(base)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A configuration with every saved setting away from its default.
fn settings() -> Config {
    Config {
        theme: "light".to_string(),
        icons: Some(true),
        line_numbers: true,
        toc_open: true,
        toc_width: 44,
        mouse: true,
        scroll_step: 7,
        body_width: Some(72),
        ..Config::default()
    }
}

/// Reads `path` back and returns the configuration it parses to, insisting it is clean.
fn reload(path: &Path) -> Config {
    let loaded = Config::load_from(path);
    assert!(
        loaded.problems.is_empty(),
        "reading back what we wrote reported problems: {:?}\n{}",
        loaded
            .problems
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        std::fs::read_to_string(path).unwrap_or_default()
    );
    loaded.config
}

#[test]
fn what_is_written_reads_back_as_the_same_settings() {
    let dir = TempDir::new("round-trip");
    let path = dir.path("config.toml");
    let saved = settings();
    saved.save_to(&path).expect("save");
    assert_eq!(
        reload(&path),
        saved,
        "the settings did not survive the round trip:\n{}",
        std::fs::read_to_string(&path).unwrap_or_default()
    );
}

#[test]
fn every_default_setting_also_round_trips() {
    let dir = TempDir::new("defaults");
    let path = dir.path("config.toml");
    let saved = Config::default();
    saved.save_to(&path).expect("save");
    assert_eq!(reload(&path), saved);
}

#[test]
fn saving_twice_leaves_the_same_file() {
    let dir = TempDir::new("idempotent");
    let path = dir.path("config.toml");
    let saved = settings();
    saved.save_to(&path).expect("first save");
    let once = std::fs::read_to_string(&path).expect("read");
    saved.save_to(&path).expect("second save");
    let twice = std::fs::read_to_string(&path).expect("read");
    assert_eq!(once, twice, "a second save churned the file");
}

#[test]
fn a_missing_file_and_its_directory_are_created() {
    let dir = TempDir::new("create");
    let path = dir.path("nested").join("deeper").join("config.toml");
    settings().save_to(&path).expect("save");
    assert!(path.exists(), "the file was not created");
    assert_eq!(reload(&path), settings());
}

/// A file a reader wrote by hand: comments, ordering, sections, a key from the future.
const HAND_WRITTEN: &str = r##"# My mdless configuration.
# Please do not eat.

theme = "dark"          # I like the dark one
scroll_step = 5

# A setting a newer mdless understands and this one does not.
telepathy = true

[keys]
"ctrl-s" = "search_forward"

[toc]
# Wide, because my headings are wordy.
width = 60

[themes.mine]
base = "dark"
accent = "#ff8800"
"##;

#[test]
fn a_hand_written_file_keeps_its_comments_its_order_and_its_unknown_keys() {
    let dir = TempDir::new("hand-written");
    let path = dir.path("config.toml");
    std::fs::write(&path, HAND_WRITTEN).expect("write");

    let mut saved = Config::load_from(&path).config;
    saved.theme = "light".to_string();
    saved.line_numbers = true;
    saved.toc_width = 44;
    saved.save_to(&path).expect("save");

    let text = std::fs::read_to_string(&path).expect("read");
    for kept in [
        "# My mdless configuration.",
        "# Please do not eat.",
        "# I like the dark one",
        "# A setting a newer mdless understands and this one does not.",
        "telepathy = true",
        "[keys]",
        "\"ctrl-s\" = \"search_forward\"",
        "# Wide, because my headings are wordy.",
        "[themes.mine]",
        "accent = \"#ff8800\"",
    ] {
        assert!(text.contains(kept), "{kept:?} was destroyed:\n{text}");
    }
    assert!(
        text.contains("theme = \"light\""),
        "the theme was not updated:\n{text}"
    );
    assert!(
        !text.contains("theme = \"dark\""),
        "the old theme was left behind as well:\n{text}"
    );
    // The reader's own ordering survives: their comment block is still first.
    assert!(
        text.starts_with("# My mdless configuration."),
        "the file was reordered:\n{text}"
    );
    // And the settings they did not touch keep the values they gave them.
    let back = Config::load_from(&path).config;
    assert_eq!(back.scroll_step, 5);
    assert_eq!(back.toc_width, 44);
    assert_eq!(back.theme, "light");
    assert!(back.line_numbers);
    assert_eq!(
        back.keys, saved.keys,
        "the key table changed, though nothing here writes one"
    );
    assert!(back.themes.contains_key("mine"), "the theme was lost");
}

#[test]
fn the_previous_file_is_kept_as_a_backup() {
    let dir = TempDir::new("backup");
    let path = dir.path("config.toml");
    std::fs::write(&path, HAND_WRITTEN).expect("write");
    let mut saved = Config::load_from(&path).config;
    saved.theme = "light".to_string();
    saved.save_to(&path).expect("save");
    let backup = dir.path("config.toml.bak");
    assert!(backup.exists(), "no backup was left");
    assert_eq!(
        std::fs::read_to_string(&backup).expect("read backup"),
        HAND_WRITTEN,
        "the backup is not the file we replaced"
    );
}

#[test]
fn a_file_whose_meaning_would_change_is_left_alone() {
    let dir = TempDir::new("refuses");
    let path = dir.path("config.toml");
    std::fs::write(&path, HAND_WRITTEN).expect("write");
    // Settings that did not come from this file: their key table is the default one,
    // so writing them would quietly drop the reader's `ctrl-s` binding. The writer
    // checks that before it touches the disk and declines.
    let error = settings()
        .save_to(&path)
        .expect_err("this must not be written");
    assert!(
        error.to_string().contains("refusing to write"),
        "unexpected failure: {error}"
    );
    assert_eq!(
        std::fs::read_to_string(&path).expect("read"),
        HAND_WRITTEN,
        "the file was modified after all"
    );
    assert!(
        !dir.path("config.toml.bak").exists(),
        "a save that did not happen left a backup behind"
    );
}

#[test]
fn a_toc_section_is_added_when_the_file_has_none() {
    let dir = TempDir::new("toc-section");
    let path = dir.path("config.toml");
    std::fs::write(&path, "theme = \"dark\"\n").expect("write");
    let mut saved = Config::load_from(&path).config;
    saved.toc_width = 55;
    saved.toc_open = true;
    saved.save_to(&path).expect("save");
    let back = reload(&path);
    assert_eq!(back.toc_width, 55);
    assert!(back.toc_open);
}

#[test]
fn a_top_level_key_is_not_appended_inside_somebody_elses_section() {
    let dir = TempDir::new("sections");
    let path = dir.path("config.toml");
    std::fs::write(&path, "[themes.mine]\nbase = \"light\"\n").expect("write");
    let mut saved = Config::load_from(&path).config;
    saved.theme = "mine".to_string();
    saved.scroll_step = 9;
    saved.save_to(&path).expect("save");
    let back = reload(&path);
    assert_eq!(back.theme, "mine");
    assert_eq!(back.scroll_step, 9);
    assert!(back.themes.contains_key("mine"));
}

#[test]
fn the_key_binding_saves_the_live_settings_and_says_where() {
    let dir = TempDir::new("binding");
    let path = dir.path("config.toml");
    let mut app = App::new(
        Doc::parse("# Title\n\nSome prose.\n"),
        Config::default(),
        AppOptions {
            title: "x.md".to_string(),
            icons: false,
            theme: "dark".to_string(),
            toc_open: false,
            config_path: Some(path.clone()),
            width: None,
        },
    );
    // Something the reader changed at run time, which is the whole point of saving.
    app.act(Action::ToggleLineNumbers);
    app.act(Action::CycleTheme);
    let theme = app.theme().name.clone();
    app.act(Action::SaveConfig);

    let notice = app.notice().expect("saving reports what it did");
    assert!(!notice.is_error, "saving failed: {}", notice.text);
    assert!(
        notice.text.contains(&path.display().to_string()),
        "the message does not name the file it wrote: {}",
        notice.text
    );
    let back = reload(&path);
    assert!(back.line_numbers, "the live setting was not saved");
    assert_eq!(back.theme, theme, "the live theme was not saved");
}

#[test]
fn saving_reports_a_failure_rather_than_claiming_success() {
    let dir = TempDir::new("failure");
    // A directory where the file should be: writing it cannot possibly work.
    let path = dir.path("config.toml");
    std::fs::create_dir_all(&path).expect("mkdir");
    let mut app = App::new(
        Doc::parse("# Title\n"),
        Config::default(),
        AppOptions {
            title: "x.md".to_string(),
            icons: false,
            theme: "dark".to_string(),
            toc_open: false,
            config_path: Some(path.clone()),
            width: None,
        },
    );
    app.act(Action::SaveConfig);
    let notice = app.notice().expect("a failure is reported");
    assert!(notice.is_error, "a failed save reported success");
}
