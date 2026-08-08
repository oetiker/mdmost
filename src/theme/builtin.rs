//! The built-in themes.
//!
//! Both themes are derived from a [`Palette`] by the same function, so the two are
//! guaranteed to define exactly the same slots and cannot drift apart.

use super::{
    BlockStyles, CodeStyles, Color, DiagramStyles, Palette, TableStyles, TextStyles, Theme,
    UiStyles,
};
use crate::theme::style::Style;

/// The signature dark theme.
pub(super) fn dark() -> Theme {
    from_palette(
        "dark",
        true,
        Palette {
            bg: Color::hex(0x11141b),
            surface: Color::hex(0x181c25),
            overlay: Color::hex(0x222836),
            fg: Color::hex(0xd6dbe5),
            muted: Color::hex(0x7c869b),
            border: Color::hex(0x39414f),
            accent: Color::hex(0x64b5ff),
            red: Color::hex(0xff6b7f),
            orange: Color::hex(0xffa657),
            yellow: Color::hex(0xf2d06b),
            green: Color::hex(0x76d7a0),
            cyan: Color::hex(0x5fd7d7),
            blue: Color::hex(0x7aa2f7),
            purple: Color::hex(0xb99bf8),
            magenta: Color::hex(0xf07fd0),
        },
    )
}

/// The built-in light theme.
pub(super) fn light() -> Theme {
    from_palette(
        "light",
        false,
        Palette {
            bg: Color::hex(0xfdfcf9),
            surface: Color::hex(0xf1efe9),
            overlay: Color::hex(0xe3e0d7),
            fg: Color::hex(0x2b2f38),
            muted: Color::hex(0x6b7280),
            border: Color::hex(0xc3c0b6),
            accent: Color::hex(0x1a6fd4),
            red: Color::hex(0xc0392b),
            orange: Color::hex(0xb35c00),
            yellow: Color::hex(0x8a6d00),
            green: Color::hex(0x1f7a4d),
            cyan: Color::hex(0x0f7b7b),
            blue: Color::hex(0x2f5fbf),
            purple: Color::hex(0x6b3fc0),
            magenta: Color::hex(0xa8298f),
        },
    )
}

/// Builds a complete theme from a palette.
pub(super) fn from_palette(name: &str, is_dark: bool, p: Palette) -> Theme {
    // A style carrying the page background, used as the base for everything that is
    // not explicitly raised onto a surface.
    let base = Style::new().fg(p.fg).bg(p.bg);
    let on_surface = Style::new().fg(p.fg).bg(p.surface);
    let muted = Style::new().fg(p.muted).bg(p.bg);
    // Slightly stronger than `muted` so highlighted code still reads on the surface.
    let code_muted = Style::new().fg(p.muted).bg(p.surface);

    Theme {
        name: name.to_string(),
        is_dark,
        headings: [
            base.fg(p.accent).bold(),
            base.fg(p.cyan).bold(),
            base.fg(p.green).bold(),
            base.fg(p.yellow).bold(),
            base.fg(p.orange),
            base.fg(p.purple),
        ],
        text: TextStyles {
            body: base,
            emphasis: base.italic(),
            strong: base.bold(),
            strikethrough: muted.strikethrough(),
            link: base.fg(p.blue).underline(),
            link_url: muted,
            code: on_surface.fg(p.magenta),
            footnote_ref: base.fg(p.accent),
            image_alt: base.fg(p.cyan).italic(),
            dim: muted,
        },
        block: BlockStyles {
            quote_bar: base.fg(p.accent),
            quote_text: base.fg(p.muted).italic(),
            list_marker: base.fg(p.accent),
            task_checked: base.fg(p.green),
            task_unchecked: base.fg(p.muted),
            rule: base.fg(p.border),
            heading_rule: base.fg(p.border),
            heading_prefix: base.fg(p.accent),
            footnote_label: base.fg(p.accent).bold(),
            caption: muted.italic(),
            image_border: base.fg(p.border),
        },
        code: CodeStyles {
            text: on_surface,
            background: Style::new().bg(p.surface),
            frame: Style::new().fg(p.border).bg(p.bg),
            language: Style::new().fg(p.accent).bg(p.bg),
            line_number: code_muted,
            overflow_marker: on_surface.fg(p.orange),
            keyword: on_surface.fg(p.purple),
            string: on_surface.fg(p.green),
            number: on_surface.fg(p.orange),
            comment: code_muted.italic(),
            function: on_surface.fg(p.blue),
            type_name: on_surface.fg(p.cyan),
            // Identifiers carry a faint cool tint so they separate from prose-like
            // plain text without becoming a colour of their own.
            variable: on_surface.fg(p.fg.blend(p.cyan, 0.22)),
            // Named constants sit beside numbers in the warm family but lean towards
            // the keyword hue, because `true` and `None` belong to the language rather
            // than to arithmetic.
            constant: on_surface.fg(p.orange.blend(p.purple, 0.35)),
            operator: on_surface.fg(p.muted),
            attribute: on_surface.fg(p.yellow),
            invalid: on_surface.fg(p.red).underline(),
            macro_name: on_surface.fg(p.magenta.blend(p.blue, 0.3)),
            // Quieter than `operator`: blended towards the border colour, which is the
            // palette's "structure, not content" hue.
            punctuation: on_surface.fg(p.muted.blend(p.border, 0.45)),
            namespace: on_surface.fg(p.cyan.blend(p.fg, 0.4)),
            // An escape must break the string run without shouting; amber sits between
            // the number and attribute hues and appears only inside green strings.
            escape: on_surface.fg(p.yellow.blend(p.orange, 0.5)),
        },
        table: TableStyles {
            border: base.fg(p.border),
            header: base.fg(p.accent).bold(),
            cell: base,
            row_alt: Style::new().bg(p.surface),
            overflow_marker: base.fg(p.orange),
        },
        diagram: DiagramStyles {
            line: base.fg(p.border.blend(p.fg, 0.35)),
            arrow: base.fg(p.accent),
            node_border: base.fg(p.blue),
            node_text: base,
            group_border: base.fg(p.purple),
            group_title: base.fg(p.purple).bold(),
            edge_label: base.fg(p.muted),
            note: base.fg(p.yellow),
            lifeline: base.fg(p.border),
            activation: base.fg(p.cyan),
            compartment: base.fg(p.border),
            stereotype: base.fg(p.magenta).italic(),
            title: base.fg(p.fg).bold(),
            axis: base.fg(p.muted),
            legend: base.fg(p.fg),
            task_done: base.fg(p.green),
            task_active: base.fg(p.accent),
            task_crit: base.fg(p.red),
            milestone: base.fg(p.yellow).bold(),
        },
        ui: UiStyles {
            status_bar: Style::new().fg(p.fg).bg(p.overlay),
            status_accent: Style::new().fg(p.accent).bg(p.overlay).bold(),
            status_key: Style::new().fg(p.yellow).bg(p.overlay),
            scrollbar_track: Style::new().fg(p.border).bg(p.bg),
            scrollbar_thumb: Style::new().fg(p.accent).bg(p.bg),
            toc_border: base.fg(p.border),
            toc_item: base,
            toc_active: Style::new().fg(p.bg).bg(p.accent).bold(),
            toc_match: base.fg(p.yellow).bold(),
            help_border: base.fg(p.accent),
            help_title: base.fg(p.accent).bold(),
            search_match: Style::new().fg(p.bg).bg(p.yellow),
            search_current: Style::new().fg(p.bg).bg(p.orange).bold(),
            prompt: Style::new().fg(p.fg).bg(p.overlay),
            error: base.fg(p.red).bold(),
            warning: base.fg(p.yellow),
        },
        palette: p,
    }
}
