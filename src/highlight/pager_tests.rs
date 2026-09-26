use std::num::NonZeroUsize;

use super::*;
use crate::highlight::{BlockingSource, CodeSource, Outcome, plain};
use crate::theme::Theme;

const RUST: &str = "fn a() {}\nfn b() {}\nfn c() {}\nfn d() {}\n";

fn run_to_end(h: &mut Highlighter, key: u64) {
    while h.outcome(key) == Some(Outcome::Pending) {
        h.advance(key, &mut || false);
    }
}

#[test]
fn a_new_block_is_plain_and_pending_with_a_key() {
    let theme = Theme::default();
    let h = Highlighter::new();
    let block = h.block(Some("rust"), RUST, &theme);
    assert_eq!(block.outcome, Outcome::Pending);
    assert_eq!(block.lines, plain(RUST, &theme.code));
    assert!(block.key.is_some());
    assert_eq!(h.block(Some("rust"), RUST, &theme).key, block.key);
}

#[test]
fn an_untagged_block_is_finished_plain_at_once() {
    let theme = Theme::default();
    let h = Highlighter::new();
    let block = h.block(None, RUST, &theme);
    assert_eq!(block.outcome, Outcome::Plain);
}

#[test]
fn advance_stops_when_asked_and_always_makes_progress() {
    let theme = Theme::default();
    let mut h = Highlighter::new();
    let key = h.block(Some("rust"), RUST, &theme).key.unwrap_or_default();
    // Stop after every line: one line per call.
    assert_eq!(h.advance(key, &mut || true), 0..1);
    // Stop after the second check: two more lines.
    let mut checks = 0;
    assert_eq!(
        h.advance(key, &mut || {
            checks += 1;
            checks == 2
        }),
        1..3
    );
    let partial = h.block(Some("rust"), RUST, &theme);
    assert_eq!(partial.outcome, Outcome::Pending);
    let full = BlockingSource::new().block(Some("rust"), RUST, &theme);
    assert_eq!(&partial.lines[..3], &full.lines[..3]);
    assert_eq!(&partial.lines[3..], &plain(RUST, &theme.code)[3..]);
}

#[test]
fn a_finished_block_equals_the_blocking_result() {
    let theme = Theme::default();
    let mut h = Highlighter::new();
    let key = h.block(Some("rust"), RUST, &theme).key.unwrap_or_default();
    run_to_end(&mut h, key);
    let done = h.block(Some("rust"), RUST, &theme);
    assert_eq!(done.outcome, Outcome::Highlighted);
    assert_eq!(
        done.lines,
        BlockingSource::new()
            .block(Some("rust"), RUST, &theme)
            .lines
    );
    assert_eq!(h.advance(key, &mut || false), 0..0);
}

#[test]
fn a_fifth_parked_block_evicts_the_least_recently_advanced() {
    let theme = Theme::default();
    let mut h = Highlighter::new();
    let srcs: Vec<String> = (0..5)
        .map(|i| format!("/* {i}\n*/ fn x{i}() {{}}\nfn y() {{}}\n"))
        .collect();
    let keys: Vec<u64> = srcs
        .iter()
        .map(|s| h.block(Some("rust"), s, &theme).key.unwrap_or_default())
        .collect();
    for &key in &keys {
        h.advance(key, &mut || true); // one line each; the fifth evicts keys[0]
    }
    assert_eq!(h.parked_keys(), keys[1..].to_vec());
    // The evicted block keeps its first line and, resumed, ends where a fresh parse ends.
    assert!(h.line(keys[0], 0).is_some());
    run_to_end(&mut h, keys[0]);
    let expect = BlockingSource::new()
        .block(Some("rust"), &srcs[0], &theme)
        .lines;
    assert_eq!(h.block(Some("rust"), &srcs[0], &theme).lines, expect);
}

#[test]
fn a_line_over_the_pager_cap_fails_the_block_but_not_the_blocking_path() {
    let theme = Theme::default();
    let mut h = Highlighter::with_line_limit(|_| NonZeroUsize::new(3).unwrap_or(NonZeroUsize::MIN));
    let src = "fn a() {}\nlet x = [1, 2, 3, 4, 5, 6, 7, 8];\n";
    let key = h.block(Some("rust"), src, &theme).key.unwrap_or_default();
    run_to_end(&mut h, key);
    assert_eq!(h.outcome(key), Some(Outcome::Failed));
    assert!(h.take_failed());
    assert!(!h.take_failed());
    let block = h.block(Some("rust"), src, &theme);
    assert_eq!(block.lines, plain(src, &theme.code));
    assert_eq!(
        BlockingSource::new()
            .block(Some("rust"), src, &theme)
            .outcome,
        Outcome::Highlighted
    );
}

#[test]
fn the_pager_cap_is_the_smaller_of_the_two_budgets() {
    let short = "x";
    let long = "x".repeat(100_000);
    assert_eq!(
        pager_limit_for(short),
        crate::highlight::token_limit_for(short)
    );
    assert_eq!(pager_limit_for(&long), PAGER_LINE_TOKENS);
}

#[test]
fn end_render_drops_blocks_the_render_did_not_ask_for() {
    let theme = Theme::default();
    let mut h = Highlighter::new();
    let kept = h.block(Some("rust"), RUST, &theme).key.unwrap_or_default();
    let gone = h
        .block(Some("rust"), "fn gone() {}\n", &theme)
        .key
        .unwrap_or_default();
    h.advance(gone, &mut || true); // parked
    h.end_render();
    // Next render asks only for `kept`.
    h.block(Some("rust"), RUST, &theme);
    h.end_render();
    assert_eq!(h.outcome(kept), Some(Outcome::Pending));
    assert_eq!(h.outcome(gone), None);
    assert!(h.parked_keys().is_empty());
    assert_eq!(h.advance(gone, &mut || false), 0..0);
}

#[test]
fn a_theme_change_starts_every_block_over() {
    let dark = Theme::default_dark();
    let light = Theme::default_light();
    let mut h = Highlighter::new();
    let key = h.block(Some("rust"), RUST, &dark).key.unwrap_or_default();
    run_to_end(&mut h, key);
    let again = h.block(Some("rust"), RUST, &light);
    assert_eq!(again.outcome, Outcome::Pending);
    assert_eq!(again.lines, plain(RUST, &light.code));
}

#[test]
fn more_than_256_blocks_are_each_highlighted_once() {
    let theme = Theme::default();
    let mut h = Highlighter::new();
    let keys: Vec<u64> = (0..300)
        .map(|i| {
            h.block(Some("rust"), &format!("fn f{i}() {{}}\n"), &theme)
                .key
                .unwrap_or_default()
        })
        .collect();
    h.end_render();
    for &key in &keys {
        run_to_end(&mut h, key);
    }
    for &key in &keys {
        assert_eq!(h.outcome(key), Some(Outcome::Highlighted));
        assert_eq!(h.advance(key, &mut || false), 0..0);
    }
}

/// Tokens per millisecond on this machine, release build. Run by hand:
/// `cargo test --release --lib tokens_per_millisecond -- --ignored --nocapture`.
#[test]
#[ignore = "measurement, not a check"]
fn tokens_per_millisecond() {
    use crate::highlight::tests::MINIFIED_JS_LINE;
    let (set, syntax) = resolve_syntax(Some("js")).expect("js resolves");
    let theme = Theme::default();
    let raw = format!("{MINIFIED_JS_LINE}\n");
    let unlimited = NonZeroUsize::new(usize::MAX).unwrap_or(NonZeroUsize::MIN);
    let mut samples = Vec::new();
    for _ in 0..20 {
        let mut parse = Parse::new(set, syntax);
        let start = std::time::Instant::now();
        let _ = parse.line(&raw, &theme.code, unlimited);
        samples.push(start.elapsed());
    }
    samples.sort();
    let median = samples[samples.len() / 2];
    // Token count: the smallest limit that does not fail, by bisection.
    let (mut lo, mut hi) = (1usize, 1_000_000usize);
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let limit = NonZeroUsize::new(mid).unwrap_or(NonZeroUsize::MIN);
        if Parse::new(set, syntax)
            .line(&raw, &theme.code, limit)
            .is_ok()
        {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    println!(
        "{lo} tokens in {median:?}: {:.1} tokens/ms",
        lo as f64 / median.as_secs_f64() / 1000.0
    );
}
