// SPDX-License-Identifier: MIT
//! The manual claims mdmost draws a particular set of Unicode blocks. This pins
//! that claim to what the renderer actually emits.
//!
//! # What this does not test
//!
//! It tests against no font whatsoever, and it cannot. It compares two things
//! that are both inside this repository: the codepoints the renderer emits, and
//! the inventory below. It says nothing about whether any font on any machine
//! has glyphs for them — checking that would mean rasterising a font and
//! asserting something about the reader's system, which is an assumption this
//! project does not make in code and which would be wrong on the next machine.
//!
//! # What it does test
//!
//! That the manual's TERMINAL SETUP block list cannot silently stop matching the
//! renderer. Add a glyph the manual does not mention and this fails, naming the
//! codepoint. It pins the *inventory*, not the appearance, so it does not
//! constrain the renderer — only the honesty of the documentation.
//!
//! # What it cannot see
//!
//! The TUI chrome. The status bar, the scrollbar and the cut markers are drawn
//! in `src/tui/` by inline literals with no central table, and nothing here
//! renders them. Those entries in the manual are maintained by hand.

use mdmost::doc::{Doc, Node, NodeKind};
use mdmost::render::{RenderOptions, render_document};
use mdmost::theme::Theme;
use std::collections::BTreeSet;

/// Every non-ASCII codepoint the renderer *adds* to one source, at one width,
/// with one set of options.
///
/// # Why the source is subtracted
///
/// The renderer passes document text through. `tests/corpus/unicode.md` and
/// `adversarial.md` carry CJK, emoji, Korean, Tangut and math alphanumerics on
/// purpose, and every one of those reaches the canvas — but they are the
/// *document's* characters, not mdmost's, and no terminal-setup advice can or
/// should promise a font covers whatever a reader opens.
///
/// What the manual claims is narrower and is the useful claim: these are the
/// characters **mdmost itself draws** — borders, rules, bullets, markers,
/// diagram art. Subtracting the source's own non-ASCII set is what separates
/// the two, and it is a separation by construction rather than by a corpus
/// that happens to be ASCII today.
///
/// The trade: a glyph the renderer draws that *also* appears in the source is
/// attributed to the source and missed. That is the conservative direction —
/// it can only under-report — and the ASCII-only corpus files reach those
/// glyphs anyway.
///
/// Math extends the same principle rather than exempting itself from it (design
/// spec §13): a formula's own commands resolve to characters too, and an author who
/// writes `\alpha` asked for `α` just as surely as one who typed it. So the
/// subtraction is not only "characters present verbatim in the source" but
/// "characters a math node's own commands resolved to" — [`math_symbols`] walks the
/// tree and adds those in as well. What is left after both subtractions is what
/// mdmost itself drew: the raised and lowered script forms, the radical sign, and
/// the rest of §5's structure.
fn added(source: &str, width: u16, options: &RenderOptions, out: &mut Added) {
    let doc = Doc::parse_auto(source);
    let canvas = render_document(&doc, width, None, &Theme::default_dark(), options);
    let mut from_source: BTreeSet<char> = source.chars().filter(|c| !c.is_ascii()).collect();
    from_source.extend(math_symbols(doc.root(), &mut out.unresolved));
    out.glyphs.extend(
        (0..canvas.height())
            .flat_map(|row| canvas.row_text(row).chars().collect::<Vec<_>>())
            .filter(|c| !c.is_ascii() && !from_source.contains(c)),
    );
}

/// What the renders contributed: the glyphs [`added`] credited to this crate, and the
/// math literals whose own symbols could not be resolved to subtract them.
///
/// The two travel together because the second is the first's most likely explanation.
#[derive(Default)]
struct Added {
    glyphs: BTreeSet<char>,
    /// See [`math_symbols`]. Never an assertion of its own — the corpus has entries here
    /// today and is correct — only the context that makes a failed assertion legible.
    unresolved: BTreeSet<String>,
}

/// The characters every math node in `node`'s subtree resolved to, via
/// [`mdmost::math::symbols`], with the literals it could not resolve recorded in
/// `unresolved`.
///
/// **A literal that does not resolve is two opposite cases, and this walk cannot tell
/// them apart.** One is a formula that does not parse at all: it draws no symbols, so
/// contributing nothing is right. The other is a formula whose macro is defined in an
/// earlier block — `symbols` is handed one literal and no preamble, so `symbols(r"\Q")`
/// fails with *unknown primitive command* while the renderer, which has the preamble,
/// draws ℚ. Contributing nothing there credits the author's character to this crate and
/// inverts design spec §13.
///
/// Swallowing the error made the second case invisible. Recording the literal does not
/// fix the attribution — that needs an entry point which resolves a formula under the
/// document's preamble, and `docs/maintainer-notes.md` holds the seam — but it puts the
/// evidence in front of whoever reads the failure.
fn math_symbols(node: &Node, unresolved: &mut BTreeSet<String>) -> BTreeSet<char> {
    let mut out = BTreeSet::new();
    collect_math_symbols(node, &mut out, unresolved);
    out
}

/// The walk behind [`math_symbols`].
fn collect_math_symbols(node: &Node, out: &mut BTreeSet<char>, unresolved: &mut BTreeSet<String>) {
    if let NodeKind::Math { literal, .. } = &node.kind {
        match mdmost::math::symbols(literal) {
            Ok(symbols) => out.extend(symbols.chars()),
            Err(_) => {
                unresolved.insert(elide(literal));
            }
        }
    }
    for child in &node.children {
        collect_math_symbols(child, out, unresolved);
    }
}

/// `literal` on one line, short enough to read in an assertion message.
fn elide(literal: &str) -> String {
    let one_line = literal.split_whitespace().collect::<Vec<_>>().join(" ");
    match one_line.char_indices().nth(60) {
        Some((cut, _)) => format!("{}…", &one_line[..cut]),
        None => one_line,
    }
}

/// The seven Mermaid families.
///
/// `tests/corpus/` has `diagrams.md` and `pipeline.mmd` but does not exercise
/// all seven, and a family whose glyphs are never rendered is a family this
/// inventory cannot see.
const MERMAID_FIXTURES: &[&str] = &[
    "```mermaid\nflowchart TD\n  A[Start] --> B{OK?}\n  B -->|yes| C([Go])\n  B -->|no| D[(Store)]\n  subgraph S\n    C --> E((End))\n  end\n```\n",
    "```mermaid\nsequenceDiagram\n  participant A as Alice\n  actor B\n  A->>B: hello\n  activate B\n  B-->>A: hi\n  deactivate B\n  loop every day\n    A-xB: ping\n  end\n  Note over A,B: done\n```\n",
    "```mermaid\nclassDiagram\n  class Shape {\n    <<interface>>\n    +draw() void\n    #size int\n  }\n  Shape <|-- Circle\n  Shape *-- Point\n  Shape o-- Style\n  Circle ..> Helper\n```\n",
    "```mermaid\nerDiagram\n  CUSTOMER ||--o{ ORDER : places\n  ORDER }|..|{ LINE : contains\n  CUSTOMER {\n    string name PK \"the name\"\n    int id FK\n  }\n```\n",
    "```mermaid\nstateDiagram-v2\n  [*] --> Idle\n  Idle --> Busy : work\n  state Busy {\n    [*] --> Step\n  }\n  Busy --> [*]\n```\n",
    "```mermaid\npie showData\n  title Languages\n  \"Rust\" : 70\n  \"TOML\" : 30\n```\n",
    "```mermaid\ngantt\n  title Plan\n  dateFormat YYYY-MM-DD\n  section One\n  Design :done, d1, 2026-01-01, 5d\n  Build  :active, crit, after d1, 10d\n  Ship   :milestone, 2026-02-01, 0d\n```\n",
];

/// The documented inventory: every non-ASCII codepoint the renderer is allowed
/// to emit, grouped by Unicode block exactly as the manual groups them.
///
/// Adding a glyph means adding it here AND to the manual's TERMINAL SETUP
/// section. That is the whole point of this file.
const INVENTORY: &[(&str, &str)] = &[
    // `\u{a0}` and `©` arrive by HTML entity decoding — `&nbsp;` and `&copy;` in the
    // source become the characters themselves on the canvas, so they are "added" by
    // the renderer even though they are really the author's content. `¹` and `²` are
    // math's raised `1` and `2` (design spec §5.1) — the two digits whose superscript
    // form Unicode placed here instead of in Superscripts and Subscripts, below.
    ("Latin-1 Supplement (U+0080-U+00FF)", "\u{a0}©¹²"),
    // The elision marker and `&hellip;`, and `‾` (U+203E), the tick that tips a tall
    // radical's diagonal (design spec §6.2), which the corpus's `\sqrt{b^2-4ac}` emits.
    //
    // `‖` (U+2016) is math's double bar at its natural height, emitted by the corpus's
    // `$$\left\|x\right\|$$` — a `\left\|` pair around a single row, which needs no
    // stretching and so draws the plain character rather than the tall `║` below.
    //
    // `\Vert` produces the same character but never reaches this set: `math::symbols`
    // resolves it, so the subtraction credits it to the document and not to us. Only
    // the delimiter built by `\left\|` is drawn by the layout, which is why the corpus
    // line that makes this entry real is a `\left\|` pair and not a `\Vert` pair.
    //
    // `›` (U+203A) is `render::code`'s `OVERFLOW_MARKER` (`src/render/code.rs:48`), the
    // mark on a framed block whose content is wider than the frame. It has been
    // drawable since long before math, but no corpus file clipped a code block at 40,
    // 80 or 200 columns until the too-wide formula's framed source did.
    ("General Punctuation (U+2000-U+206F)", "…‾‖›"),
    // Class-diagram relation glyphs, and math's radical sign (`\sqrt`).
    ("Mathematical Operators (U+2200-U+22FF)", "∧∨√"),
    // Math's raised `n`, `+` and `-`, and lowered `=` and `1` (design spec §5.1) —
    // `x^{n+1}`, `e^{-x}` and `\sum_{i=1}^{n}`.
    ("Superscripts and Subscripts (U+2070-U+209F)", "ⁿ⁺⁻₌₁"),
    // Math's subscript i — the one Latin subscript letter Unicode placed outside the
    // Superscripts and Subscripts block, above.
    ("Phonetic Extensions (U+1D00-U+1D7F)", "ᵢ"),
    // Math's subscript j — the one Latin subscript letter Unicode placed here instead.
    ("Latin Extended-C (U+2C60-U+2C7F)", "ⱼ"),
    // Math's raised `h j l r s w x y` — the superscript letters Unicode placed here
    // instead of in Superscripts and Subscripts, above (design spec §5.1).
    ("Spacing Modifier Letters (U+02B0-U+02FF)", "ʰʲˡʳˢʷˣʸ"),
    // Math's raised `c f z` — the superscript letters Unicode placed here instead.
    ("Phonetic Extensions Supplement (U+1D80-U+1DBF)", "ᶜᶠᶻ"),
    // Every frame, rule, table border and diagram box. `║` (U+2551) is math's tall `\|`
    // (design spec §6.4) and nothing else. The corpus's
    // `$$\left\| \frac{a}{b} \right\|$$` emits it: a `\left\|` pair around three rows
    // has to stretch, and a stretched double bar is this character rather than the
    // plain `‖` above.
    //
    // NOT the other candidate, and it was checked rather than assumed: a sequence
    // diagram's nested activation bar is also `║` (`src/mermaid/sequence/mod.rs:48`),
    // but it can never reach the page. Bars are drawn in `plan.bars` order, and
    // `deactivate` pushes on close, so an inner bar is drawn FIRST and the outer bar's
    // `vline` -- same lifeline column, spanning the inner rows too -- paints `┃` over
    // every `║` it drew. Rendering a nested activation both ways emits no `║` at all.
    (
        "Box Drawing (U+2500-U+257F)",
        "─━│┃┄┆┈┊┌┐┓└┗┘├┤┬┳┴┼╌╎╭╮╯╰╱╲║",
    ),
    // Zebra stripes, the gap-row half block, gantt bars.
    ("Block Elements (U+2580-U+259F)", "▀▄█▋▌▍"),
    // Heading marks, diagram node shapes, arrowheads.
    ("Geometric Shapes (U+25A0-U+25FF)", "▲△▶▼▽◀◆◇◈◉○●◯"),
    // The degraded-diagram caption marker.
    ("Dingbats (U+2700-U+27BF)", "✗"),
    // Class-diagram generics, and math's angle, white-square and flattened-round
    // delimiters -- `\langle`, `\lAngle`, `\lBrack`, `\lgroup` and their closers.
    ("Misc Mathematical Symbols-A (U+27C0-U+27EF)", "⟨⟩⟦⟧⟪⟫⟮⟯"),
    // Math's extensible arrow delimiters: `\left\uparrow` and friends.
    ("Arrows (U+2190-U+21FF)", "↑↓↕⇑⇓⇕"),
    // Math's ceiling, floor and moustache delimiters -- `\lceil`, `\lfloor`,
    // `\lmoustache` and their closers.
    ("Miscellaneous Technical (U+2300-U+23FF)", "⌈⌉⌊⌋⎰⎱"),
    // Math's white-brace and double-parenthesis delimiters -- `\lBrace`,
    // `\llparenthesis`, `\llangle` and their closers.
    ("Misc Mathematical Symbols-B (U+2980-U+29FF)", "⦃⦄⦇⦈⦉⦊"),
    // Code-fence language icons, drawn only when icons are on. These are the
    // one row a reader can opt out of, with `--no-icons`.
    (
        "Private Use Area (U+E000-U+F8FF)",
        "\u{e73c}\u{e795}\u{e7a8}\u{f121}",
    ),
    // Drawn in place of a character that cannot be represented.
    ("Specials (U+FFF0-U+FFFF)", "\u{fffd}"),
];

/// Everything the renderer is asked to draw, across the widths and option sets
/// that change which glyphs come out.
fn everything_emitted() -> Added {
    let corpus: Vec<String> =
        std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/corpus"))
            .expect("tests/corpus must be readable")
            .map(|entry| entry.expect("a readable dir entry").path())
            .filter(|path| {
                matches!(
                    path.extension().and_then(|e| e.to_str()),
                    Some("md" | "mmd")
                )
            })
            .map(|path| std::fs::read_to_string(&path).expect("a readable corpus file"))
            .collect();

    let sources = corpus
        .iter()
        .map(String::as_str)
        .chain(MERMAID_FIXTURES.iter().copied());

    let mut seen = Added::default();
    for source in sources {
        // Both glyph sets, and both line-number settings: icons changes the
        // code-fence language icon, and nothing else in the body.
        for icons in [true, false] {
            for line_numbers in [true, false] {
                let options = RenderOptions::new(icons, line_numbers)
                    .with_title_banner(true)
                    .with_copy_button(true);
                // Narrow forces wrapping, the table gap row and the cut markers;
                // wide leaves everything dense.
                for width in [40, 80, 200] {
                    added(source, width, &options, &mut seen);
                }
            }
        }
    }
    seen
}

#[test]
fn every_glyph_the_renderer_emits_is_in_the_documented_inventory() {
    let documented: BTreeSet<char> = INVENTORY
        .iter()
        .flat_map(|(_, chars)| chars.chars())
        .collect();

    let emitted = everything_emitted();
    let undocumented: Vec<String> = emitted
        .glyphs
        .difference(&documented)
        .map(|c| format!("U+{:04X} {c}", *c as u32))
        .collect();

    assert!(
        undocumented.is_empty(),
        "the renderer emitted {} codepoint(s) this file does not document:\n  {}\n\
         \n\
         Decide whose character it is before you add anything. The answer is not always \
         \"add it\", and adding it wrongly puts a false claim in the one document this \
         test exists to protect.\n\
         \n\
         A formula whose macro is defined in an EARLIER block resolves to nothing here: \
         `math::symbols` is handed one literal and no preamble, so the renderer draws \
         the expansion and this walk sees none of it, and every character that macro \
         drew is credited to mdmost by mistake. Literals that did not resolve in this \
         run ({}):\n  {}\n\
         \n\
         If a codepoint above came from one of those, it is the DOCUMENT's character. \
         Design spec §13 says mdmost does not claim it: do not add it to INVENTORY and \
         do not add it to the manual. See `docs/maintainer-notes.md`, \"cannot attribute \
         a macro's expansion\".\n\
         \n\
         Only a codepoint mdmost itself draws -- a border, rule, marker, script form or \
         diagram stroke -- belongs in INVENTORY here AND in the manual's TERMINAL SETUP \
         section.",
        undocumented.len(),
        undocumented.join("\n  "),
        emitted.unresolved.len(),
        if emitted.unresolved.is_empty() {
            "(none)".to_owned()
        } else {
            emitted
                .unresolved
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join("\n  ")
        }
    );
}

/// Every non-ASCII character `math::render_inline` can substitute for a script operand.
///
/// This probes the public API directly rather than reading `src/math/scripts.rs`'s
/// private substitution tables, on purpose: those tables are a closed set, so asserting
/// this inventory against them would only prove the tables agree with themselves.
/// Driving every printable ASCII character through an actual superscript and subscript
/// is what catches the gap the corpus-only check above cannot -- a substitution the
/// tables support but no `tests/corpus/*.md` line happens to exercise (as `ˣ`, `ᶜ`,
/// `ᶠ` and `ᶻ` were, until this test existed).
fn substituted_script_forms() -> BTreeSet<char> {
    let mut out = BTreeSet::new();
    for byte in 0x20u8..=0x7e {
        let c = char::from(byte);
        // These have their own meaning to the LaTeX parser and are not script content
        // in their own right; excluding them keeps every probe a group with exactly
        // one plain character inside, which is what a substitution table entry is.
        if "\\{}$&%_^~".contains(c) {
            continue;
        }
        for marker in ['^', '_'] {
            let src = format!("x{marker}{{{c}}}");
            if let Ok(rendered) = mdmost::math::render_inline(&src) {
                out.extend(rendered.chars().filter(|ch| !ch.is_ascii()));
            }
        }
    }
    out
}

/// The inclusive codepoint range in an `INVENTORY` label like
/// `"Superscripts and Subscripts (U+2070-U+209F)"`.
fn block_range(label: &str) -> std::ops::RangeInclusive<u32> {
    let hex = label
        .rsplit('(')
        .next()
        .and_then(|s| s.strip_suffix(')'))
        .expect("every INVENTORY label ends in (U+XXXX-U+YYYY)");
    let (lo, hi) = hex
        .split_once('-')
        .expect("every INVENTORY range has a hyphen");
    let parse = |s: &str| {
        u32::from_str_radix(s.trim_start_matches("U+"), 16).expect("every INVENTORY bound is hex")
    };
    parse(lo)..=parse(hi)
}

#[test]
fn every_script_form_the_renderer_can_draw_falls_in_a_documented_block() {
    // Block ranges, not the literal example glyphs above: the manual promises a whole
    // Unicode block per row ("math's raised and lowered digits, operators and
    // parentheses"), not the handful of examples `INVENTORY` happens to spell out for
    // the corpus-driven test above. Checking literal glyphs here would flag every
    // digit and operator the corpus doesn't happen to render, which the manual already
    // covers by block; checking ranges is what actually matches the manual's claim,
    // and it is what makes this assertion immune to the corpus's own coverage gaps.
    let blocks: Vec<_> = INVENTORY
        .iter()
        .map(|(label, _)| block_range(label))
        .collect();

    let undocumented: Vec<String> = substituted_script_forms()
        .into_iter()
        .filter(|c| !blocks.iter().any(|range| range.contains(&(*c as u32))))
        .map(|c| format!("U+{:04X} {c}", c as u32))
        .collect();

    assert!(
        undocumented.is_empty(),
        "the renderer can draw {} script form(s) whose Unicode block the manual does \
         not document, even though no corpus line happens to reach them.\n\
         Add the block to INVENTORY here and to the manual's TERMINAL SETUP section:\n  {}",
        undocumented.len(),
        undocumented.join("\n  ")
    );
}
