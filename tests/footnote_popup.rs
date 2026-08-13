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

/// The note sizes the sweeps use: a one-liner, a paragraph, and one past both caps.
const NOTES: [(u16, u16); 4] = [(5, 1), (30, 4), (200, 3), (40, 90)];

#[test]
fn a_popup_never_leaves_the_screen_wherever_its_marker_is() {
    // The property every flip is in service of. A box that hangs off an edge is not a
    // smaller mistake than one drawn in the wrong place: the part that is off screen is
    // simply not there, and the reader has no way to know a footnote was cut.
    let screen = (80u16, 23u16);
    for content in NOTES {
        for anchor in cells(screen.0, screen.1) {
            let Area {
                top,
                left,
                width,
                height,
            } = popup::place(anchor, content, screen);
            assert!(
                left + width <= screen.0,
                "{anchor:?} with {content:?} ran off the right edge: {left}+{width}"
            );
            assert!(
                top + height <= screen.1,
                "{anchor:?} with {content:?} ran off the bottom: {top}+{height}"
            );
            assert!(width >= 3 && height >= 3, "a box has a border to draw");
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
    let content = (30u16, 4u16);
    let height = popup::place((0, 0), content, screen).height;
    for (x, y) in cells(screen.0, screen.1) {
        let area = popup::place((x, y), content, screen);
        if y + 1 + height <= screen.1 {
            assert_eq!(
                area.top,
                y + 1,
                "there was room below the marker at ({x}, {y})"
            );
        } else {
            assert!(
                area.top + area.height <= y,
                "no room below, so it must sit entirely above the marker at ({x}, {y}): {area:?}"
            );
        }
    }
}

#[test]
fn a_popup_starts_at_its_marker_whenever_there_is_room() {
    // The sideways half of the rule above, and the same argument: a box always pushed
    // flush against the right edge would satisfy the containment sweep and point at
    // nothing.
    let screen = (80u16, 23u16);
    let content = (30u16, 4u16);
    let width = popup::place((0, 0), content, screen).width;
    for (x, y) in cells(screen.0, screen.1) {
        let area = popup::place((x, y), content, screen);
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
    let small = popup::place((0, 0), (5, 1), screen);
    let medium = popup::place((0, 0), (30, 4), screen);
    let huge = popup::place((0, 0), (400, 400), screen);

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
        let area = popup::place((0, 0), (inner, 3), (screen_width, 23));
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
