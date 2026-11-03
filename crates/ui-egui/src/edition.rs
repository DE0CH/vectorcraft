//! Editions: VectorCraft cut down for one job (the labels edition: `docs/labels-edition.md`). An edition
//! chooses the commands that menus, the context menu, shortcuts and the command palette offer, the tools
//! on the toolbar and the panels that open. The engine is unchanged: everything an edition hides still
//! exists and still runs from code (a tool committing its shape, the edition's own app). The full app
//! sets no edition and behaves as before.

use std::sync::OnceLock;

use crate::menus::Item;

/// One edition.
#[derive(Debug)]
pub struct Edition {
    /// Shown in the About box and logs.
    pub name: &'static str,
    /// The command ids menus, the context menu, shortcuts and the palette may run. A submenu left with
    /// no command, and a menu left empty, disappear.
    pub commands: &'static [&'static str],
    /// The menu bar's menus, by their English names, in the order shown.
    pub menus: &'static [&'static str],
    /// The toolbar: (category, slots), as [`crate::toolbar::BASIC`]. Its tools are the only ones a
    /// shortcut or the palette selects.
    pub tools: &'static [(&'static str, &'static [&'static [&'static str]])],
    /// The panels (`window.panel` ids) that may open.
    pub panels: &'static [&'static str],
    /// Embedded in another page as just the editor: no app bar (menus), no document tabs, no Home screen; text bound
    /// to a variable is locked against typing (the page fills it from data sets). The page opens and saves the one
    /// document itself.
    pub embedded: bool,
}

/// Whether the app runs embedded ([`Edition::embedded`]).
pub fn embedded() -> bool {
    get().is_some_and(|e| e.embedded)
}

static EDITION: OnceLock<&'static Edition> = OnceLock::new();

/// Make `edition` the app's edition, once, before the app starts; `false` if one was set already.
pub fn set(edition: &'static Edition) -> bool {
    let fresh = EDITION.set(edition).is_ok();
    if fresh && edition.embedded {
        // The page fills bound text from its data sets: typed characters there would be thrown away.
        vectorcraft_doc::set_bound_text_locked(true);
    }
    fresh
}

/// The app's edition, if it runs one.
pub fn get() -> Option<&'static Edition> {
    EDITION.get().copied()
}

/// Whether menus, shortcuts and the palette may offer command `id` (`tool:<id>` for a tool,
/// `panel:<id>` for a panel's shortcut).
pub fn allows(id: &str) -> bool {
    get().is_none_or(|e| e.allows(id))
}

/// Whether tool `id` is on the edition's toolbar.
pub fn allows_tool(id: &str) -> bool {
    get().is_none_or(|e| e.allows_tool(id))
}

/// Whether panel `id` may open.
pub fn allows_panel(id: &str) -> bool {
    get().is_none_or(|e| e.panels.contains(&id))
}

/// The menu bar as the app's edition shows it ([`Edition::menus_of`]); unchanged without one.
pub fn filter_menus(tree: Vec<(&'static str, Vec<Item>)>) -> Vec<(&'static str, Vec<Item>)> {
    match get() {
        Some(e) => e.menus_of(&tree),
        None => tree,
    }
}

/// `items` as the app's edition shows them ([`Edition::items_of`]); unchanged without one.
pub fn filter_items(items: &[Item]) -> Vec<Item> {
    match get() {
        Some(e) => e.items_of(items),
        None => items.to_vec(),
    }
}

impl Edition {
    /// Whether this edition offers command `id` (`tool:<id>`, `panel:<id>` or a command id).
    pub fn allows(&self, id: &str) -> bool {
        if let Some(tool) = id.strip_prefix("tool:") {
            return self.allows_tool(tool);
        }
        if let Some(panel) = id.strip_prefix("panel:") {
            return self.panels.contains(&panel);
        }
        self.commands.contains(&id)
    }

    /// Whether tool `id` is on this edition's toolbar.
    pub fn allows_tool(&self, id: &str) -> bool {
        self.tools.iter().any(|(_, slots)| slots.iter().any(|s| s.contains(&id)))
    }

    /// The menu bar as this edition shows it: its menus in its order, each cut to its commands.
    pub fn menus_of(&self, tree: &[(&'static str, Vec<Item>)]) -> Vec<(&'static str, Vec<Item>)> {
        self.menus
            .iter()
            .filter_map(|name| tree.iter().find(|(n, _)| n == name))
            .map(|(name, items)| (*name, self.items_of(items)))
            .filter(|(_, items)| !items.is_empty())
            .collect()
    }

    /// `items` cut to this edition's commands: placeholders go, emptied submenus go, and separators
    /// and section headers stay only before items that remain.
    pub fn items_of(&self, items: &[Item]) -> Vec<Item> {
        let mut out: Vec<Item> = vec![];
        let mut header: Option<Item> = None;
        for item in items {
            match item {
                Item::Cmd(_, id, _) if self.allows(id) => {
                    if let Some(h) = header.take() {
                        out.push(h);
                    }
                    out.push(item.clone());
                }
                Item::Sub(label, sub) => {
                    let kept = self.items_of(sub);
                    if !kept.is_empty() {
                        if let Some(h) = header.take() {
                            out.push(h);
                        }
                        out.push(Item::Sub(label, kept));
                    }
                }
                Item::Header(_) => header = Some(item.clone()),
                Item::Sep => {
                    header = None;
                    if out.last().is_some_and(|last| !matches!(last, Item::Sep)) {
                        out.push(Item::Sep);
                    }
                }
                Item::Cmd(..) | Item::Todo(..) => {}
            }
        }
        while out.last().is_some_and(|last| matches!(last, Item::Sep)) {
            out.pop();
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn filter_keeps_allowed_commands_and_tidies_separators() {
        // A local edition: the process-wide one is never set in tests, so the full app's tests are unaffected.
        let e = Edition {
            name: "test",
            commands: &["a", "c"],
            menus: &["M"],
            tools: &[("Select", &[&["selection"]])],
            panels: &["layers"],
            embedded: false,
        };
        let items = vec![
            Item::Header("H1"),
            Item::Cmd("A", "a", Value::Null),
            Item::Sep,
            Item::Cmd("B", "b", Value::Null),
            Item::Todo("T", ""),
            Item::Sep,
            Item::Sub("S", vec![Item::Cmd("B", "b", Value::Null)]),
            Item::Sub("S2", vec![Item::Cmd("C", "c", Value::Null)]),
            Item::Sep,
        ];
        let out = e.items_of(&items);
        let shape: Vec<String> = out
            .iter()
            .map(|i| match i {
                Item::Cmd(_, id, _) => format!("cmd:{id}"),
                Item::Sub(l, s) => format!("sub:{l}:{}", s.len()),
                Item::Header(h) => format!("hdr:{h}"),
                Item::Sep => "sep".into(),
                Item::Todo(..) => "todo".into(),
            })
            .collect();
        assert_eq!(shape, vec!["hdr:H1", "cmd:a", "sep", "sub:S2:1"]);
        assert!(e.allows("panel:layers") && !e.allows("panel:brushes"));
        assert!(e.allows("tool:selection") && !e.allows("tool:pen"));
        assert!(get().is_none() && allows("anything"), "no edition: everything is allowed");
        let menus = e.menus_of(&[("M", items.clone()), ("N", items)]);
        assert_eq!(menus.iter().map(|(n, _)| *n).collect::<Vec<_>>(), vec!["M"]);
    }
}
