// SPDX-License-Identifier: MIT
//! `classDef`, `class`, `cssClass`, `style` and `:::`, shared by the four families that
//! read them (colour spec §3.2).
//!
//! Everything is recorded while the diagram is read and merged only when it is done,
//! because a class may be defined after its use (§3.5). Nothing here fails: an unknown
//! class, an undeclared node or an unreadable value drops that statement or property
//! (§3.6), and a key no node has is simply never asked for, so `style X` cannot create
//! a node (ruling 15).

use crate::mermaid::ast::Paint;
use crate::mermaid::paint::{self, Props};

use super::lex;

/// The colour statements of one diagram, in source order.
#[derive(Debug, Default)]
pub(super) struct Sheet {
    /// `classDef` definitions: class name and properties.
    defs: Vec<(String, Props)>,
    /// Class assignments from `class`, `cssClass` and `:::`: node key and class name.
    assigned: Vec<(String, String)>,
    /// `style` lines: node key and properties.
    styles: Vec<(String, Props)>,
}

impl Sheet {
    /// `classDef a,b fill:#…`: defines every listed class.
    pub(super) fn define(&mut self, rest: &str, src: &str) {
        let (names, list) = split_list(rest);
        let props = paint::parse_props(list, lex::offset_of(src, list).unwrap_or_default());
        for name in names
            .split(',')
            .map(str::trim)
            .filter(|name| is_class_name(name))
        {
            self.defs.push((name.to_string(), props));
        }
    }

    /// `class A,B c1,c2` or `cssClass "A,B" c`: the last word names the classes, and
    /// everything before it the nodes, quoted or not.
    pub(super) fn assign_list(&mut self, rest: &str, key: impl Fn(&str) -> String) {
        let (list, classes) = rsplit_list(rest);
        if list.is_empty() {
            return;
        }
        let list = lex::unquote(list);
        for target in list.split(',').map(|target| key(target.trim())) {
            for class in classes.split(',').map(str::trim) {
                self.assign(&target, class);
            }
        }
    }

    /// `style A fill:#…`.
    pub(super) fn style(&mut self, rest: &str, src: &str, key: impl Fn(&str) -> String) {
        let (target, list) = lex::split_word(rest);
        if target.is_empty() {
            return;
        }
        let props = paint::parse_props(list, lex::offset_of(src, list).unwrap_or_default());
        self.styles.push((key(target), props));
    }

    /// One class assigned to one node, as `:::name` writes it.
    pub(super) fn assign(&mut self, key: &str, class: &str) {
        if !key.is_empty() && is_class_name(class) {
            self.assigned.push((key.to_string(), class.to_string()));
        }
    }

    /// The merged paint of `key`: `classDef default` when `node` (ruling 14: never for
    /// a subgraph or composite state), then `own` classes, then the classes assigned
    /// to `key`, then its `style` lines.
    pub(super) fn paint(&self, key: Option<&str>, own: &[String], node: bool) -> Option<Paint> {
        let assigned = self
            .assigned
            .iter()
            .filter(|(target, _)| Some(target.as_str()) == key)
            .map(|(_, name)| name.as_str());
        let styles = self
            .styles
            .iter()
            .filter(|(target, _)| Some(target.as_str()) == key)
            .map(|(_, props)| props);
        let default = node
            .then(|| self.class_layers("default"))
            .into_iter()
            .flatten();
        let classes = own
            .iter()
            .map(String::as_str)
            .chain(assigned)
            .flat_map(|name| self.class_layers(name));
        paint::merge(default.chain(classes).chain(styles))
    }

    /// Every definition of class `name`, in source order: a class defined twice is
    /// applied as both (colour spec §3.5).
    fn class_layers<'s>(&'s self, name: &'s str) -> impl Iterator<Item = &'s Props> + 's {
        self.defs
            .iter()
            .filter(move |(def, _)| def == name)
            .map(|(_, props)| props)
    }
}

/// The key a flowchart or state diagram files a `style` or `class` target under.
pub(super) fn plain_key(text: &str) -> String {
    lex::unquote(text).to_string()
}

/// Splits off the leading comma list of `text`: `a, b fill:…` gives `("a, b", "fill:…")`.
/// Whitespace next to a comma belongs to the list.
fn split_list(text: &str) -> (&str, &str) {
    let text = text.trim();
    let mut end = 0;
    while let Some(gap) = text[end..].find(char::is_whitespace).map(|at| end + at) {
        let next = text[gap..].trim_start();
        if !text[..gap].ends_with(',') && !next.starts_with(',') {
            return (&text[..gap], next);
        }
        end = text.len() - next.len();
    }
    (text, "")
}

/// Splits off the trailing comma list of `text`: `A, B c1, c2` gives `("A, B", "c1, c2")`.
fn rsplit_list(text: &str) -> (&str, &str) {
    let text = text.trim();
    let mut start = text.len();
    while let Some(gap) = text[..start].rfind(char::is_whitespace) {
        let before = text[..gap].trim_end();
        if !before.ends_with(',') && !text[gap..].trim_start().starts_with(',') {
            return (before, text[gap..].trim_start());
        }
        start = before.len();
    }
    ("", text)
}

/// A class name is `[A-Za-z0-9_-]+` (colour spec §3.2).
fn is_class_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(lex::is_class_char)
}
