// SPDX-License-Identifier: MIT
//! Popup layout is pure geometry: no terminal, no `App`, no buffer.
//!
//! The pager's own tests drive the box through the painted frame, one document at a
//! time. This sweeps [`mdmost::tui::popup::place`] over *every* cell of a screen and
//! several note sizes, which is the half a hand-picked document cannot cover: a rule
//! that holds at the two edges a test happened to pick and fails one cell in is a rule
//! that fails in the wild.

use mdmost::tui::popup::{self, Area};

/// Every cell of a `width` × `height` screen.
fn cells(width: u16, height: u16) -> impl Iterator<Item = (u16, u16)> {
    (0..height).flat_map(move |y| (0..width).map(move |x| (x, y)))
}

/// The note sizes the sweeps use: a one-liner, a paragraph, one past the width cap, and
/// one past the height cap.
///
/// **The tall one is load-bearing.** A four-row note makes a six-row box, which fits
/// above or below a marker almost anywhere on a 23-row document area — so a sweep of
/// that size alone cannot reach the case where the box fits neither way, and a review
/// found this file asserting the no-overlap rule while never exercising it.
const NOTES: [(u16, u16); 4] = [(5, 1), (30, 4), (200, 3), (40, 90)];

/// The document areas the sweeps run over.
///
/// **More than one height is load-bearing, and the arithmetic says why.** The box is
/// shrunk into the *larger* gap when it fits neither, and the branch that shrinks into
/// the gap **above** the marker needs `above > below` and `wanted > above` at once. On a
/// 23-row area the first needs a marker below row 11 and the second needs `above` under
/// `MAX_HEIGHT`, which is 12 — so the two cannot both hold and that branch is
/// unreachable. A review mutated exactly it and the whole suite stayed green while
/// sweeping every cell of a 23-row screen. Shorter areas are where the case lives: at 20
/// rows a marker on row 11 has 11 rows above it and 8 below, and a twelve-row box fits
/// neither.
///
/// The short ones are also the terminals where a box refuses to open at all, which is the
/// other half of the same rule.
const SCREENS: [(u16, u16); 5] = [(80, 23), (80, 20), (80, 12), (80, 7), (80, 5)];

/// [`popup::place`], for the sweeps where a box is always possible.
fn place(anchor: (u16, u16), content: (u16, u16), screen: (u16, u16)) -> Area {
    popup::place(anchor, content, screen)
        .unwrap_or_else(|| panic!("no box for {anchor:?} with {content:?} on {screen:?}"))
}

#[test]
fn a_popup_never_leaves_the_screen_wherever_its_marker_is() {
    // The property every flip is in service of. A box that hangs off an edge is not a
    // smaller mistake than one drawn in the wrong place: the part that is off screen is
    // simply not there, and the reader has no way to know a footnote was cut.
    for screen in SCREENS {
        for content in NOTES {
            for anchor in cells(screen.0, screen.1) {
                let Some(Area {
                    top,
                    left,
                    width,
                    height,
                }) = popup::place(anchor, content, screen)
                else {
                    continue;
                };
                assert!(
                    left + width <= screen.0,
                    "{anchor:?} with {content:?} on {screen:?} ran off the right edge: \
                     {left}+{width}"
                );
                assert!(
                    top + height <= screen.1,
                    "{anchor:?} with {content:?} on {screen:?} ran off the bottom: {top}+{height}"
                );
                assert!(width >= 3 && height >= 3, "a box has a border to draw");
            }
        }
    }
}

#[test]
fn a_popup_opens_below_its_marker_whenever_there_is_room() {
    // Below is the default because that is where the eye already is. The flip is for the
    // rows where below is impossible, and nowhere else — a box that always opened
    // upwards would pass the "stays on screen" sweep above and still be wrong on every
    // row of the document.
    let screen = (80u16, 23u16);
    for content in NOTES {
        let height = place((0, 0), content, screen).height;
        for (x, y) in cells(screen.0, screen.1) {
            let area = place((x, y), content, screen);
            if y + 1 + height <= screen.1 {
                assert_eq!(
                    area.top,
                    y + 1,
                    "there was room below the marker at ({x}, {y}) for {content:?}"
                );
                assert_eq!(area.height, height, "and no reason to shrink it");
            } else if height <= y {
                assert_eq!(
                    area.top + area.height,
                    y,
                    "no room below, so it must sit directly above the marker at ({x}, {y})"
                );
            }
        }
    }
}

#[test]
fn a_popup_never_covers_its_own_marker() {
    // The rule the flip exists for, and the one containment cannot express: a box merely
    // clamped onto the screen satisfies "inside the viewport" while sitting on top of the
    // sentence the reader asked the question from. `a_popup_never_leaves_the_screen…`
    // passes on a build that gets this wrong, which is why the invariant is asserted here
    // rather than left as a corollary of that one.
    //
    // Every screen in `SCREENS`, not one: the branch that shrinks the box into the gap
    // *above* the marker cannot be reached on a 23-row area at all (see `SCREENS`), and a
    // sweep that ran only that height stated this rule while never exercising it.
    for screen in SCREENS {
        for content in NOTES {
            for (x, y) in cells(screen.0, screen.1) {
                let Some(area) = popup::place((x, y), content, screen) else {
                    continue;
                };
                assert!(
                    y < area.top || y >= area.top + area.height,
                    "the box at {area:?} covers its own marker at ({x}, {y}) on {screen:?}"
                );
            }
        }
    }
}

#[test]
fn the_gap_above_the_marker_is_a_case_the_sweeps_actually_reach() {
    // A tripwire on the sweeps above, not a rule of its own. The two of them are only
    // worth their runtime if the shrink-into-the-gap-above branch is among the answers
    // they see, and that branch is reachable in a narrow band: `above > below` puts the
    // marker in the lower half, `wanted > above` puts `above` under MAX_HEIGHT, and both
    // together need an area shorter than 2 * MAX_HEIGHT. If a future cap makes the band
    // empty again, this fails and says so — rather than the sweeps quietly going hollow.
    let area = popup::place((0, 11), (40, 90), (80, 20)).expect("a shrunken box");
    assert_eq!(
        (area.top, area.height),
        (0, 11),
        "eleven rows above the marker, eight below, and a twelve-row box wanted: it must \
         take the larger gap and stop short of the marker"
    );
    assert!(
        SCREENS.contains(&(80, 20)) && NOTES.contains(&(40, 90)),
        "and the sweeps must actually run that combination"
    );
}

#[test]
fn a_marker_with_no_room_on_either_side_gets_no_box_at_all() {
    // Not a sliver, and not a box over the marker: nothing, so the pager can say so.
    // A five-row document area with the marker in the middle leaves two rows above and
    // two below, and a box needs three.
    assert!(popup::place((0, 2), (30, 4), (80, 5)).is_none());
    // The same refusal reached through the *other* gap: two rows above the marker, one
    // below, so the larger gap is the one above and it is still a row short of a box.
    assert!(popup::place((0, 2), (30, 4), (80, 4)).is_none());
    // One row further up and the gap below is three rows, which is exactly a box.
    let area = popup::place((0, 1), (30, 4), (80, 5)).expect("a box in the gap below");
    assert_eq!((area.top, area.height), (2, 3));
}

#[test]
fn a_popup_starts_at_its_marker_whenever_there_is_room() {
    // The sideways half of the rule above, and the same argument: a box always pushed
    // flush against the right edge would satisfy the containment sweep and point at
    // nothing.
    let screen = (80u16, 23u16);
    let content = (30u16, 4u16);
    let width = place((0, 0), content, screen).width;
    for (x, y) in cells(screen.0, screen.1) {
        let area = place((x, y), content, screen);
        if x + width <= screen.0 {
            assert_eq!(area.left, x, "there was room to the right at ({x}, {y})");
        } else {
            assert!(
                area.left < x,
                "no room to the right, so it must open leftwards at ({x}, {y}): {area:?}"
            );
            assert_eq!(
                area.left + area.width,
                screen.0,
                "and a flipped box is flush with the edge it flipped off: {area:?}"
            );
        }
    }
}

#[test]
fn a_popup_is_as_big_as_its_note_up_to_the_caps() {
    let screen = (200u16, 60u16);
    let small = place((0, 0), (5, 1), screen);
    let medium = place((0, 0), (30, 4), screen);
    let huge = place((0, 0), (400, 400), screen);

    assert!(small.width < medium.width, "content decides the width");
    assert!(small.height < medium.height, "and the height");
    assert_eq!(huge.width, popup::MAX_WIDTH, "the width cap holds");
    assert_eq!(huge.height, popup::MAX_HEIGHT, "the height cap holds");
    // The chrome is the two borders and one column of padding per side, so a note
    // measured at `n` columns gets a box of `n + 4`. Spelled out rather than derived
    // from the same constant twice: this is the claim, not a restatement of it.
    assert_eq!(medium.width, 34);
    assert_eq!(medium.height, 6);
}

#[test]
fn a_screen_with_no_room_for_a_popup_says_so_rather_than_drawing_a_sliver() {
    assert!(popup::fits((40, 20)));
    assert!(
        popup::fits((12, 3)),
        "the smallest box that can hold a note"
    );
    assert!(!popup::fits((11, 20)), "one column short of a note");
    assert!(!popup::fits((40, 2)), "no room for a border and a row");
}

#[test]
fn a_narrow_screen_renders_the_note_narrower_rather_than_hanging_off_the_edge() {
    // The width the note is laid out at is the box's inner width, so a terminal narrower
    // than the cap gets a narrower note — not a sixty-column note clipped by a box that
    // does not fit around it.
    assert_eq!(
        popup::inner_width(200),
        popup::MAX_WIDTH - popup::CHROME_COLS,
        "the cap, not the screen, on a wide terminal"
    );
    assert_eq!(popup::inner_width(30), 26, "the screen on a narrow one");
    for screen_width in 12..200u16 {
        let inner = popup::inner_width(screen_width);
        let area = place((0, 0), (inner, 3), (screen_width, 23));
        assert!(
            area.left + area.width <= screen_width,
            "a note rendered at {inner} needs a box that fits in {screen_width}: {area:?}"
        );
    }
}

#[test]
fn a_footnote_is_found_by_name_and_only_by_name() {
    // The definition is keyed by the name the author wrote, never by the number the
    // marker draws: a reference carries both, a definition only the name.
    let doc = mdmost::Doc::parse("a[^alpha] b[^beta]\n\n[^alpha]: first\n\n[^beta]: second\n");
    let alpha = popup::definition(doc.root(), "alpha").expect("the alpha definition");
    assert!(
        alpha.plain_text().contains("first"),
        "found the wrong note: {:?}",
        alpha.plain_text()
    );
    assert!(
        popup::definition(doc.root(), "beta")
            .expect("the beta definition")
            .plain_text()
            .contains("second")
    );
    assert!(
        popup::definition(doc.root(), "1").is_none(),
        "the number a marker draws is not a name"
    );
    assert!(popup::definition(doc.root(), "gamma").is_none());
}
