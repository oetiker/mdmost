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
fn added(source: &str, width: u16, options: &RenderOptions) -> BTreeSet<char> {
    let doc = Doc::parse_auto(source);
    let canvas = render_document(&doc, width, None, &Theme::default_dark(), options);
    let mut from_source: BTreeSet<char> = source.chars().filter(|c| !c.is_ascii()).collect();
    from_source.extend(math_symbols(doc.root()));
    (0..canvas.height())
        .flat_map(|row| canvas.row_text(row).chars().collect::<Vec<_>>())
        .filter(|c| !c.is_ascii() && !from_source.contains(c))
        .collect()
}

/// The characters every math node in `node`'s subtree resolved to, via
/// [`mdmost::math::symbols`].
///
/// A formula that does not parse contributes nothing, which is right: it draws no
/// symbols either.
fn math_symbols(node: &Node) -> BTreeSet<char> {
    let mut out = BTreeSet::new();
    collect_math_symbols(node, &mut out);
    out
}

/// The walk behind [`math_symbols`].
fn collect_math_symbols(node: &Node, out: &mut BTreeSet<char>) {
    if let NodeKind::Math { literal, .. } = &node.kind
        && let Ok(symbols) = mdmost::math::symbols(literal)
    {
        out.extend(symbols.chars());
    }
    for child in &node.children {
        collect_math_symbols(child, out);
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
    // The elision marker, and `&hellip;`.
    ("General Punctuation (U+2000-U+206F)", "…"),
    // Class-diagram relation glyphs, and math's radical sign (`\sqrt`).
    ("Mathematical Operators (U+2200-U+22FF)", "∧∨√"),
    // Math's raised `n` and `+`, and lowered `=` and `1` (design spec §5.1) —
    // `x^{n+1}` and `\sum_{i=1}^{n}`.
    ("Superscripts and Subscripts (U+2070-U+209F)", "ⁿ⁺₌₁"),
    // Math's subscript i — the one Latin subscript letter Unicode placed outside the
    // Superscripts and Subscripts block, above.
    ("Phonetic Extensions (U+1D00-U+1D7F)", "ᵢ"),
    // Math's subscript j — the one Latin subscript letter Unicode placed here instead.
    ("Latin Extended-C (U+2C60-U+2C7F)", "ⱼ"),
    // Every frame, rule, table border and diagram box.
    (
        "Box Drawing (U+2500-U+257F)",
        "─━│┃┄┆┈┊┌┐┓└┗┘├┤┬┳┴┼╌╎╭╮╯╰╱╲",
    ),
    // Zebra stripes, the gap-row half block, gantt bars.
    ("Block Elements (U+2580-U+259F)", "▀▄█▋▌▍"),
    // Heading marks, diagram node shapes, arrowheads.
    ("Geometric Shapes (U+25A0-U+25FF)", "▲△▶▼▽◀◆◇◈◉○●◯"),
    // The degraded-diagram caption marker.
    ("Dingbats (U+2700-U+27BF)", "✗"),
    // Class-diagram generics.
    ("Misc Mathematical Symbols-A (U+27C0-U+27EF)", "⟨⟩"),
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
fn everything_emitted() -> BTreeSet<char> {
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

    let mut seen = BTreeSet::new();
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
                    seen.extend(added(source, width, &options));
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

    let undocumented: Vec<String> = everything_emitted()
        .difference(&documented)
        .map(|c| format!("U+{:04X} {c}", *c as u32))
        .collect();

    assert!(
        undocumented.is_empty(),
        "the renderer emits {} codepoint(s) the manual does not document.\n\
         Add them to INVENTORY here and to the manual's TERMINAL SETUP section:\n  {}",
        undocumented.len(),
        undocumented.join("\n  ")
    );
}
