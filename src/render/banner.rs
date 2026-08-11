//! The `FIGlet` banner drawn for a document whose one `#` heading is its title.
//!
//! **Added 2026-08-09 at the owner's request** (design spec §9): "for documents where
//! there is only one `#` (an obvious title) level you could use the 'small' figlet font
//! for typesetting the title".
//!
//! # When it applies
//!
//! [`render::render_document`](crate::render::render_document) draws the banner only
//! when the document has **exactly one level-1 heading** *and* that heading is its
//! first block. Both halves matter. A reference manual with a `#` per chapter must not
//! turn into a wall of banners, and a `#` that appears late is a section title, not the
//! document's title — turning it into six-row art would announce the wrong thing.
//! Everything else, including every heading of level 2 and deeper, is untouched.
//!
//! # How it degrades
//!
//! The banner is an *option* the renderer takes only when it clearly works, and the
//! ordinary heading is the answer in every other case:
//!
//! * a title containing anything outside printable ASCII — CJK, emoji, accented Latin
//!   — has no glyph in this font subset, so there is nothing to draw;
//! * a banner wider than the pane is not truncated, wrapped or scrolled; it is
//!   declined, which is what makes a 40-column terminal safe;
//! * `title_banner = false` in the configuration declines it always.
//!
//! Because the fallback is the same [`block::heading`](super::block::heading) every
//! other heading uses, a declined banner is not a special case anywhere downstream.
//!
//! # The font
//!
//! The glyph table below is the printable-ASCII half of the `FIGlet` font *Small* by
//! Glenn Chappell (figlet release 2.1, 1994; "Permission is hereby given to modify
//! this font, as long as the modifier's name is placed on a comment line" — the
//! modification here is the restriction to `0x20..=0x7E` and the transcription into
//! Rust, by the mdmost authors, 2026-08-09). Embedding one font as a table is the
//! whole dependency: no crate, no font files to find at runtime, nothing to load.
//!
//! [`layout`] implements `FIGlet`'s controlled horizontal smushing with the rules this
//! font asks for (equal character, underscore, hierarchy, opposite pair), which is
//! what makes `|_|` and `_` merge into one column instead of standing apart. Two tests
//! check the result against `figlet -f Small` byte for byte; while it was being written
//! the same comparison was run over 290 random printable-ASCII strings, which is how
//! the one-column disagreement in `FIGlet`'s `smushamt` arithmetic — documented at
//! [`smush_amount`] — was found. That sweep is not a committed test, because it needs
//! both `figlet` and a font file that is not in this repository.

use crate::canvas::{Anchor, Canvas, SearchSpan};
use crate::doc::{Node, NodeKind};

use super::Ctx;

/// The number of rows every glyph in the font occupies.
pub(crate) const HEIGHT: usize = 5;

/// The character the font uses where a column must stay blank but must not be smushed
/// through. It becomes a space once layout is done.
const HARDBLANK: char = '$';

/// The first character the table covers; the table runs to `~` (0x7E).
const FIRST: u32 = 0x20;

/// The longest title that is even considered, in characters.
///
/// A banner is at most about eight columns per character, so this is far past the
/// point where any terminal could show one. It exists so a pathological heading costs
/// a length check rather than a large allocation.
const MAX_TITLE_CHARS: usize = 200;

/// One character of the title, and the cells its art occupies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Letter {
    /// The character's index in the title.
    pub index: usize,
    /// The first column of its art.
    pub col: u16,
    /// How many columns its art occupies.
    pub cols: u16,
    /// The first row of the band the character was drawn in.
    ///
    /// A title too wide for its measure is wrapped between words, and each band is a
    /// line of art. Without this a search hit on a word in the second band would light
    /// up the same columns in the first.
    pub row: u16,
    /// How many rows that band occupies.
    pub rows: u16,
}

/// A laid-out banner: the rows to draw, and where each character of the title landed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Banner {
    /// The rows of art, all the same length, with no all-blank row at either end.
    ///
    /// A wrapped title contributes one band of rows per line, stacked in reading order
    /// with no blank row between them — `figlet` sets wrapped lines the same way.
    pub rows: Vec<String>,
    /// Where each character of the title was drawn.
    pub letters: Vec<Letter>,
}

impl Banner {
    /// The width of the banner, in columns.
    ///
    /// Every row is exactly this wide, which is what lets the caller blit them without
    /// measuring. Only the tests ask, because the renderer already knows the budget it
    /// handed in and the rows came back inside it.
    #[cfg(test)]
    pub fn width(&self) -> usize {
        self.rows.first().map_or(0, |row| row.chars().count())
    }
}

/// Lays `text` out in the font, or returns `None` if it cannot be drawn in `budget`
/// columns.
///
/// `None` is not a failure: it is the renderer being told to draw an ordinary heading
/// instead. Every character must be printable ASCII, and the finished art must fit the
/// budget without truncation.
///
/// A title wider than the budget is **wrapped between words** and set as several bands
/// of art, each centred on the widest, rather than declined — the choice between art
/// and text should not turn on whether the words happen to fit one line. Only a single
/// word too wide to draw still declines: there is nowhere to break it, and truncated
/// art is worse than an honest heading.
pub(crate) fn layout(text: &str, budget: usize) -> Option<Banner> {
    let text = text.trim();
    if text.is_empty() || budget == 0 || text.chars().count() > MAX_TITLE_CHARS {
        return None;
    }
    let mut bands: Vec<Banner> = Vec::new();
    for line in wrap(text, budget)? {
        bands.push(line_banner(&line.text, &line.origins, budget)?);
    }
    stack(bands)
}

/// One wrapped line: the text to draw, and the title-character index behind each of its
/// characters.
struct Line {
    text: String,
    /// Indexed by character of `text`, giving that character's index in the whole title.
    origins: Vec<usize>,
}

/// Greedily wraps `text` between words so that every line's art fits `budget`.
///
/// Returns `None` when a single word cannot be drawn in the budget, which is the one
/// case wrapping cannot rescue.
fn wrap(text: &str, budget: usize) -> Option<Vec<Line>> {
    let mut lines: Vec<Line> = Vec::new();
    let mut current: Option<Line> = None;
    for (start, word) in words(text) {
        let candidate = match &current {
            Some(line) => {
                let mut text = line.text.clone();
                let mut origins = line.origins.clone();
                text.push(' ');
                // The space between two words stands for the whitespace that separated
                // them in the title; pointing it at that character keeps every index in
                // this table a real position in the title.
                origins.push(start.saturating_sub(1));
                push_word(&mut text, &mut origins, start, &word);
                Line { text, origins }
            }
            None => {
                let mut text = String::new();
                let mut origins = Vec::new();
                push_word(&mut text, &mut origins, start, &word);
                Line { text, origins }
            }
        };
        if fits(&candidate.text, budget) {
            current = Some(candidate);
            continue;
        }
        // The candidate is too wide. Break before this word — unless it is the only
        // thing on the line, in which case no break can help it and `?` declines.
        lines.push(current.take()?);
        let mut text = String::new();
        let mut origins = Vec::new();
        push_word(&mut text, &mut origins, start, &word);
        if !fits(&text, budget) {
            return None;
        }
        current = Some(Line { text, origins });
    }
    lines.extend(current);
    (!lines.is_empty()).then_some(lines)
}

/// Appends `word`, whose first character is at `start` in the title, to a line.
fn push_word(text: &mut String, origins: &mut Vec<usize>, start: usize, word: &str) {
    for (offset, ch) in word.chars().enumerate() {
        text.push(ch);
        origins.push(start + offset);
    }
}

/// Whether one line's art fits the budget.
fn fits(text: &str, budget: usize) -> bool {
    line_width(text).is_some_and(|width| width <= budget)
}

/// The width one line of art would occupy, or `None` if a character has no art.
fn line_width(text: &str) -> Option<usize> {
    let mut rows: Vec<Vec<char>> = vec![Vec::new(); HEIGHT];
    for ch in text.chars() {
        merge(&mut rows, glyph(ch)?);
    }
    Some(trimmed_width(&rows))
}

/// The title's words, each with the character index it starts at.
fn words(text: &str) -> Vec<(usize, String)> {
    let mut out: Vec<(usize, String)> = Vec::new();
    let mut start = None;
    let mut word = String::new();
    for (index, ch) in text.chars().enumerate() {
        if ch.is_whitespace() {
            if let Some(at) = start.take() {
                out.push((at, std::mem::take(&mut word)));
            }
        } else {
            start.get_or_insert(index);
            word.push(ch);
        }
    }
    if let Some(at) = start {
        out.push((at, word));
    }
    out
}

/// Lays one line out, numbering its letters by their place in the whole title.
fn line_banner(text: &str, origins: &[usize], budget: usize) -> Option<Banner> {
    let mut rows: Vec<Vec<char>> = vec![Vec::new(); HEIGHT];
    let mut letters = Vec::new();
    for (index, ch) in text.chars().enumerate() {
        let art = glyph(ch)?;
        let (col, cols) = merge(&mut rows, art);
        letters.push(Letter {
            index: origins.get(index).copied().unwrap_or(index),
            col: u16::try_from(col).ok()?,
            cols: u16::try_from(cols).ok()?,
            // Filled in by `stack`, which is what knows where this band landed.
            row: 0,
            rows: 0,
        });
    }
    finish(rows, letters, budget)
}

/// Stacks the bands of a wrapped title into one banner, centring each on the widest.
fn stack(bands: Vec<Banner>) -> Option<Banner> {
    let width = bands.iter().map(band_width).max()?;
    let mut rows: Vec<String> = Vec::new();
    let mut letters: Vec<Letter> = Vec::new();
    for band in bands {
        let indent = (width - band_width(&band)) / 2;
        let first = u16::try_from(rows.len()).ok()?;
        let height = u16::try_from(band.rows.len()).ok()?;
        for mut letter in band.letters {
            letter.col = letter.col.checked_add(u16::try_from(indent).ok()?)?;
            letter.row = first;
            letter.rows = height;
            letters.push(letter);
        }
        for row in band.rows {
            let mut padded = " ".repeat(indent);
            padded.push_str(&row);
            let trailing = width - indent - row.chars().count();
            padded.push_str(&" ".repeat(trailing));
            rows.push(padded);
        }
    }
    (!rows.is_empty()).then_some(Banner { rows, letters })
}

/// The width of one band's art.
fn band_width(band: &Banner) -> usize {
    band.rows.first().map_or(0, |row| row.chars().count())
}

/// The width of a laid-out grid, ignoring the blanks each row is padded with.
fn trimmed_width(rows: &[Vec<char>]) -> usize {
    rows.iter()
        .map(|row| {
            row.iter()
                .rposition(|c| *c != ' ' && *c != HARDBLANK)
                .map_or(0, |index| index + 1)
        })
        .max()
        .unwrap_or(0)
}

/// Turns the laid-out character grid into a [`Banner`], or declines it.
fn finish(rows: Vec<Vec<char>>, letters: Vec<Letter>, budget: usize) -> Option<Banner> {
    // The hardblank has done its job — it kept a column from being smushed through —
    // and from here on it is a space like any other.
    let mut rows: Vec<Vec<char>> = rows
        .into_iter()
        .map(|row| {
            row.into_iter()
                .map(|c| if c == HARDBLANK { ' ' } else { c })
                .collect()
        })
        .collect();
    let width = rows
        .iter()
        .map(|row| row.iter().rposition(|c| *c != ' ').map_or(0, |i| i + 1))
        .max()
        .unwrap_or(0);
    if width == 0 || width > budget {
        return None;
    }
    for row in &mut rows {
        row.truncate(width);
    }
    // A title with no descender leaves the bottom row blank, and one with no ascender
    // the top row; drawing them would be a blank line the reader cannot account for.
    while rows.last().is_some_and(|row| is_blank(row)) {
        rows.pop();
    }
    let dropped = rows.iter().take_while(|row| is_blank(row)).count();
    rows.drain(..dropped);
    if rows.is_empty() {
        return None;
    }
    Some(Banner {
        rows: rows
            .into_iter()
            .map(|row| row.into_iter().collect())
            .collect(),
        letters,
    })
}

/// Whether a row is entirely blank.
fn is_blank(row: &[char]) -> bool {
    row.iter().all(|c| *c == ' ')
}

/// The art of one character, or `None` if the font does not cover it.
fn glyph(ch: char) -> Option<&'static [&'static str; HEIGHT]> {
    let index = usize::try_from(u32::from(ch).checked_sub(FIRST)?).ok()?;
    SMALL.get(index)
}

/// Appends one character's art to the rows, smushed into what is already there.
///
/// Returns the columns the new art occupies, which is what lets a search hit on the
/// title highlight the right part of the banner.
fn merge(rows: &mut [Vec<char>], art: &[&str; HEIGHT]) -> (usize, usize) {
    let art: Vec<Vec<char>> = art.iter().map(|row| row.chars().collect()).collect();
    let amount = smush_amount(rows, &art);
    let width = isize::try_from(rows[0].len()).unwrap_or(isize::MAX);
    let start = width - amount;
    for (row, piece) in rows.iter_mut().zip(art.iter()) {
        for (offset, ch) in piece.iter().enumerate() {
            let col = start + isize::try_from(offset).unwrap_or(isize::MAX);
            let Ok(col) = usize::try_from(col) else {
                // Only the *leading blanks* of the new art can fall off the left edge —
                // that is what the minimum in `smush_amount` guarantees — and dropping
                // a blank is how the first character of a banner loses the empty column
                // its art carries. `FIGlet` clamps these to column zero, which for a
                // blank is the same thing.
                debug_assert_eq!(*ch, ' ', "a mark fell off the left of the banner");
                continue;
            };
            if col < row.len() {
                row[col] = combine(row[col], *ch);
            } else {
                row.push(*ch);
            }
        }
    }
    let start = usize::try_from(start).unwrap_or(0);
    (start, rows[0].len() - start)
}

/// How many columns the new art may overlap what is already laid out.
///
/// This is `FIGlet`'s `smushamt`, arithmetic included: every row is asked how far it
/// could give way — its own trailing blanks, plus the new art's leading blanks, plus
/// one more if the two characters that would then collide can be smushed — and the
/// *smallest* answer wins, because the columns move as one.
///
/// The result can exceed what is laid out so far, which is not an error: against an
/// empty line it comes out as the new art's leading blank columns, and those columns
/// are then dropped rather than drawn. That is why a banner does not start with the
/// blank column its first character's art carries, and it is the difference between
/// agreeing with `figlet` and being one column wider than it everywhere.
fn smush_amount(rows: &[Vec<char>], art: &[Vec<char>]) -> isize {
    let mut amount = isize::try_from(art.first().map_or(0, Vec::len)).unwrap_or(isize::MAX);
    for (row, piece) in rows.iter().zip(art.iter()) {
        let len = isize::try_from(row.len()).unwrap_or(isize::MAX);
        // The index of the last mark in the row, as `FIGlet` computes it: zero both for
        // an empty row and for one that is nothing but blanks.
        let last = row.iter().rposition(|c| *c != ' ').unwrap_or(0);
        let leading = piece.iter().position(|c| *c != ' ').unwrap_or(piece.len());
        let mut here = isize::try_from(leading).unwrap_or(isize::MAX) + len
            - 1
            - isize::try_from(last).unwrap_or(0);
        match row.get(last) {
            None | Some(' ') => here += 1,
            Some(left) => {
                if piece
                    .get(leading)
                    .is_some_and(|right| smush(*left, *right).is_some())
                {
                    here += 1;
                }
            }
        }
        amount = amount.min(here);
    }
    amount
}

/// Combines two characters landing in the same column.
fn combine(left: char, right: char) -> char {
    if left == ' ' {
        return right;
    }
    if right == ' ' {
        return left;
    }
    // Only the one boundary column can reach here with two marks in it, and
    // `smush_amount` only allowed the overlap because that pair smushes.
    smush(left, right).unwrap_or(right)
}

/// The character two marks smush into, if this font's rules allow them to at all.
///
/// *Small* asks for rules 1-4: equal character, underscore, hierarchy and opposite
/// pair. Rules 5 (big X) and 6 (hardblank) are not enabled, so `/\` stays `/\` and two
/// hardblanks refuse to merge — which is the entire reason the hardblank exists.
fn smush(left: char, right: char) -> Option<char> {
    if left == HARDBLANK || right == HARDBLANK {
        return None;
    }
    if left == right {
        return Some(left); // rule 1
    }
    if left == '_' && BORDERS.contains(right) {
        return Some(right); // rule 2
    }
    if right == '_' && BORDERS.contains(left) {
        return Some(left);
    }
    if let (Some(l), Some(r)) = (rank(left), rank(right))
        && l != r
    {
        return Some(if l > r { left } else { right }); // rule 3
    }
    match (left, right) {
        ('[', ']') | (']', '[') | ('{', '}') | ('}', '{') | ('(', ')') | (')', '(') => {
            Some('|') // rule 4
        }
        _ => None,
    }
}

/// The characters an underscore gives way to (rule 2).
const BORDERS: &str = "|/\\[]{}()<>";

/// A character's class in the smushing hierarchy (rule 3); higher wins.
fn rank(ch: char) -> Option<u8> {
    match ch {
        '|' => Some(0),
        '/' | '\\' => Some(1),
        '[' | ']' => Some(2),
        '{' | '}' => Some(3),
        '(' | ')' => Some(4),
        '<' | '>' => Some(5),
        _ => None,
    }
}

/// Draws the title of `node` as a banner, or `None` to fall back to a plain heading.
///
/// The canvas carries the heading's anchor, so the table of contents still jumps to it,
/// and one search span per character per row, so searching for the title highlights the
/// art the title was drawn as rather than nothing at all.
pub(crate) fn render_title(node: &Node, id: &str, width: u16, ctx: Ctx<'_>) -> Option<Canvas> {
    let (text, origins) = text_with_origins(node);
    let banner = layout(&text, usize::from(width))?;
    let style = ctx.theme.heading(1);
    let mut out = Canvas::empty(width);
    for row in &banner.rows {
        let index = out.push_blank_row(style);
        out.write_str(index, 0, row, style);
    }
    out.add_anchor(Anchor {
        id: id.to_string(),
        level: 1,
        row: 0,
    });
    // Row-major, so the first segment of a hit is on the banner's top row and jumping
    // to a match lands where a reader would look for it. A letter is highlighted only
    // across the band it was drawn in: a wrapped title has several, and lighting up the
    // same columns in every one would mark words the hit never touched.
    let trimmed_offset = text.len() - text.trim_start().len();
    let trimmed: Vec<char> = text.trim().chars().collect();
    for row in 0..out.height() {
        for letter in &banner.letters {
            let band = usize::from(letter.row)..usize::from(letter.row) + usize::from(letter.rows);
            if !band.contains(&row) {
                continue;
            }
            let Some(start) = origin_of(&origins, &text, trimmed_offset, letter.index) else {
                continue;
            };
            let len = trimmed.get(letter.index).map_or(0, |ch| ch.len_utf8());
            if len == 0 {
                continue;
            }
            out.add_span(SearchSpan {
                source_start: start,
                source_end: start + len,
                unit: None,
                row,
                col: letter.col,
                cols: letter.cols,
            });
        }
    }
    Some(out)
}

/// The source offset of the `index`th character of the trimmed title.
///
/// `origins` is indexed by character of the *untrimmed* text, which is what the walk
/// below produces; the layout numbers its letters from the trimmed text, so the leading
/// whitespace has to be counted back on.
fn origin_of(
    origins: &[Option<usize>],
    text: &str,
    trimmed_offset: usize,
    index: usize,
) -> Option<usize> {
    let skipped = text[..trimmed_offset].chars().count();
    origins.get(skipped + index).copied().flatten()
}

/// The plain text of a node, with the source offset of each character where one is
/// knowable.
///
/// This walks exactly as [`Node::plain_text`] does, so the two stay in step — a test
/// asserts the string it returns is the same one. An offset is `None` wherever the
/// rendered text is not a byte-for-byte copy of its source (an entity, say), which
/// costs a search highlight on that one character and nothing else.
fn text_with_origins(node: &Node) -> (String, Vec<Option<usize>>) {
    let mut text = String::new();
    let mut origins = Vec::new();
    collect(node, &mut text, &mut origins);
    (text, origins)
}

fn collect(node: &Node, text: &mut String, origins: &mut Vec<Option<usize>>) {
    match &node.kind {
        NodeKind::Text(literal) => {
            let faithful = node.source.len() == literal.len();
            for (offset, ch) in literal.char_indices() {
                text.push(ch);
                origins.push(faithful.then_some(node.source.start + offset));
            }
        }
        NodeKind::Code { literal } => {
            for ch in literal.chars() {
                text.push(ch);
                origins.push(None);
            }
        }
        NodeKind::SoftBreak | NodeKind::LineBreak => {
            text.push(' ');
            origins.push(None);
        }
        _ => {}
    }
    for child in &node.children {
        collect(child, text, origins);
    }
}

#[cfg(test)]
mod tests;

/// `FIGlet` *Small* by Glenn Chappell, restricted to `0x20..=0x7E`; see the module docs
/// for the licence note. `$` is the hardblank.
#[rustfmt::skip]
static SMALL: [[&str; HEIGHT]; 95] = [
    // ' '
    [" $", " $", " $", " $", " $"],
    // '!'
    ["  _ ", " | |", " |_|", " (_)", "    "],
    // '"'
    ["  _ _ ", " ( | )", "  V V ", "   $  ", "      "],
    // '#'
    ["    _ _   ", "  _| | |_ ", " |_  .  _|", " |_     _|", "   |_|_|  "],
    // '$'
    ["     ", "  ||_", " (_-<", " / _/", "  || "],
    // '%'
    ["  _  __ ", " (_)/ / ", "   / /_ ", "  /_/(_)", "        "],
    // '&'
    ["  __     ", " / _|___ ", " > _|_ _|", r" \_____| ", "         "],
    // "'"
    ["  _ ", " ( )", " |/ ", "  $ ", "    "],
    // '('
    ["   __", "  / /", " | | ", " | | ", r"  \_\"],
    // ')'
    [" __  ", r" \ \ ", "  | |", "  | |", " /_/ "],
    // '*'
    ["     ", r" _/\_", " >  <", r"  \/ ", "     "],
    // '+'
    ["    _   ", "  _| |_ ", " |_   _|", "   |_|  ", "        "],
    // ','
    ["    ", "    ", "  _ ", " ( )", " |/ "],
    // '-'
    ["      ", "  ___ ", " |___|", "   $  ", "      "],
    // '.'
    ["    ", "    ", "  _ ", " (_)", "    "],
    // '/'
    ["    __", "   / /", "  / / ", " /_/  ", "      "],
    // '0'
    ["   __  ", r"  /  \ ", " | () |", r"  \__/ ", "       "],
    // '1'
    ["  _ ", " / |", " | |", " |_|", "    "],
    // '2'
    ["  ___ ", " |_  )", "  / / ", " /___|", "      "],
    // '3'
    ["  ____", " |__ /", r"  |_ \", " |___/", "      "],
    // '4'
    ["  _ _  ", " | | | ", " |_  _|", "   |_| ", "       "],
    // '5'
    ["  ___ ", " | __|", r" |__ \", " |___/", "      "],
    // '6'
    ["   __ ", "  / / ", r" / _ \", r" \___/", "      "],
    // '7'
    ["  ____ ", " |__  |", "   / / ", "  /_/  ", "       "],
    // '8'
    ["  ___ ", " ( _ )", r" / _ \", r" \___/", "      "],
    // '9'
    ["  ___ ", r" / _ \", r" \_, /", "  /_/ ", "      "],
    // ':'
    ["  _ ", " (_)", "  _ ", " (_)", "    "],
    // ';'
    ["  _ ", " (_)", "  _ ", " ( )", " |/ "],
    // '<'
    ["   __", "  / /", " < < ", r"  \_\", "     "],
    // '='
    ["      ", "  ___ ", " |___|", " |___|", "      "],
    // '>'
    [" __  ", r" \ \ ", "  > >", " /_/ ", "     "],
    // '?'
    ["  ___ ", r" |__ \", "   /_/", "  (_) ", "      "],
    // '@'
    ["   ____  ", r"  / __ \ ", " / / _` |", r" \ \__,_|", r"  \____/ "],
    // 'A'
    ["    _   ", r"   /_\  ", r"  / _ \ ", r" /_/ \_\", "        "],
    // 'B'
    ["  ___ ", " | _ )", r" | _ \", " |___/", "      "],
    // 'C'
    ["   ___ ", "  / __|", " | (__ ", r"  \___|", "       "],
    // 'D'
    ["  ___  ", r" |   \ ", " | |) |", " |___/ ", "       "],
    // 'E'
    ["  ___ ", " | __|", " | _| ", " |___|", "      "],
    // 'F'
    ["  ___ ", " | __|", " | _| ", " |_|  ", "      "],
    // 'G'
    ["   ___ ", "  / __|", " | (_ |", r"  \___|", "       "],
    // 'H'
    ["  _  _ ", " | || |", " | __ |", " |_||_|", "       "],
    // 'I'
    ["  ___ ", " |_ _|", "  | | ", " |___|", "      "],
    // 'J'
    ["     _ ", "  _ | |", " | || |", r"  \__/ ", "       "],
    // 'K'
    ["  _  __", " | |/ /", " | ' < ", r" |_|\_\", "       "],
    // 'L'
    ["  _    ", " | |   ", " | |__ ", " |____|", "       "],
    // 'M'
    ["  __  __ ", r" |  \/  |", r" | |\/| |", " |_|  |_|", "         "],
    // 'N'
    ["  _  _ ", r" | \| |", " | .` |", r" |_|\_|", "       "],
    // 'O'
    ["   ___  ", r"  / _ \ ", " | (_) |", r"  \___/ ", "        "],
    // 'P'
    ["  ___ ", r" | _ \", " |  _/", " |_|  ", "      "],
    // 'Q'
    ["   ___  ", r"  / _ \ ", " | (_) |", r"  \__\_\", "        "],
    // 'R'
    ["  ___ ", r" | _ \", " |   /", r" |_|_\", "      "],
    // 'S'
    ["  ___ ", " / __|", r" \__ \", " |___/", "      "],
    // 'T'
    ["  _____ ", " |_   _|", "   | |  ", "   |_|  ", "        "],
    // 'U'
    ["  _   _ ", " | | | |", " | |_| |", r"  \___/ ", "        "],
    // 'V'
    [" __   __", r" \ \ / /", r"  \ V / ", r"   \_/  ", "        "],
    // 'W'
    [" __      __", r" \ \    / /", r"  \ \/\/ / ", r"   \_/\_/  ", "           "],
    // 'X'
    [" __  __", r" \ \/ /", "  >  < ", r" /_/\_\", "       "],
    // 'Y'
    [" __   __", r" \ \ / /", r"  \ V / ", "   |_|  ", "        "],
    // 'Z'
    ["  ____", " |_  /", "  / / ", " /___|", "      "],
    // '['
    ["  __ ", " | _|", " | | ", " | | ", " |__|"],
    // '\\'
    [" __   ", r" \ \  ", r"  \ \ ", r"   \_\", "      "],
    // ']'
    ["  __ ", " |_ |", "  | |", "  | |", " |__|"],
    // '^'
    [r"  /\ ", r" |/\|", "   $ ", "   $ ", "     "],
    // '_'
    ["      ", "      ", "      ", "  ___ ", " |___|"],
    // '`'
    ["  _ ", " ( )", r"  \|", "  $ ", "    "],
    // 'a'
    ["       ", "  __ _ ", " / _` |", r" \__,_|", "       "],
    // 'b'
    ["  _    ", " | |__ ", r" | '_ \", " |_.__/", "       "],
    // 'c'
    ["     ", "  __ ", " / _|", r" \__|", "     "],
    // 'd'
    ["     _ ", "  __| |", " / _` |", r" \__,_|", "       "],
    // 'e'
    ["      ", "  ___ ", " / -_)", r" \___|", "      "],
    // 'f'
    ["   __ ", "  / _|", " |  _|", " |_|  ", "      "],
    // 'g'
    ["       ", "  __ _ ", " / _` |", r" \__, |", " |___/ "],
    // 'h'
    ["  _    ", " | |_  ", r" | ' \ ", " |_||_|", "       "],
    // 'i'
    ["  _ ", " (_)", " | |", " |_|", "    "],
    // 'j'
    ["    _ ", "   (_)", "   | |", "  _/ |", " |__/ "],
    // 'k'
    ["  _   ", " | |__", " | / /", r" |_\_\", "      "],
    // 'l'
    ["  _ ", " | |", " | |", " |_|", "    "],
    // 'm'
    ["        ", "  _ __  ", r" | '  \ ", " |_|_|_|", "        "],
    // 'n'
    ["       ", "  _ _  ", r" | ' \ ", " |_||_|", "       "],
    // 'o'
    ["      ", "  ___ ", r" / _ \", r" \___/", "      "],
    // 'p'
    ["       ", "  _ __ ", r" | '_ \", " | .__/", " |_|   "],
    // 'q'
    ["       ", "  __ _ ", " / _` |", r" \__, |", "    |_|"],
    // 'r'
    ["      ", "  _ _ ", " | '_|", " |_|  ", "      "],
    // 's'
    ["     ", "  ___", " (_-<", " /__/", "     "],
    // 't'
    ["  _   ", " | |_ ", " |  _|", r"  \__|", "      "],
    // 'u'
    ["       ", "  _  _ ", " | || |", r"  \_,_|", "       "],
    // 'v'
    ["      ", " __ __", r" \ V /", r"  \_/ ", "      "],
    // 'w'
    ["         ", " __ __ __", r" \ V  V /", r"  \_/\_/ ", "         "],
    // 'x'
    ["      ", " __ __", r" \ \ /", r" /_\_\", "      "],
    // 'y'
    ["       ", "  _  _ ", " | || |", r"  \_, |", "  |__/ "],
    // 'z'
    ["     ", "  ___", " |_ /", " /__|", "     "],
    // '{'
    ["    __", "   / /", " _| | ", "  | | ", r"   \_\"],
    // '|'
    ["  _ ", " | |", " | |", " | |", " |_|"],
    // '}'
    [" __   ", r" \ \  ", "  | |_", "  | | ", " /_/  "],
    // '~'
    [r"  /\/|", r" |/\/ ", "   $  ", "   $  ", "      "],
];
