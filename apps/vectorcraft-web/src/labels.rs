//! The labels edition (`docs/labels-edition.md`): what YYC's staff need to design a label and nothing else.
//! Built with `--features labels`; the page that hosts it (DE0CH/YYC `labels/`) opens and saves the design,
//! loads the guest list as data sets and prints, through `window.vectorcraft`.

use vectorcraft_ui_egui::VectorcraftApp;
use vectorcraft_ui_egui::edition::{self, Edition};

/// Commands the menus, the context menu, shortcuts and the palette offer.
const COMMANDS: &[&str] = &[
    // Edit
    "edit.undo",
    "edit.redo",
    "edit.cut",
    "edit.copy",
    "edit.paste",
    "edit.pasteInFront",
    "edit.pasteInBack",
    "edit.pasteInPlace",
    "edit.clear",
    // File: Place only (a picture or logo onto the label)
    "file.place",
    // Object
    "object.transformAgain",
    "object.move",
    "object.rotate",
    "object.reflect",
    "object.scale",
    "object.arrange.bringToFront",
    "object.arrange.bringForward",
    "object.arrange.sendBackward",
    "object.arrange.sendToBack",
    "object.align",
    "object.group",
    "object.ungroup",
    "object.lock",
    "object.unlockAll",
    "object.hide",
    "object.showAll",
    // Type
    "type.bold",
    "type.italic",
    "type.convertToAreaType",
    "type.convertToPointType",
    "type.changeCase",
    // Select
    "select.all",
    "select.none",
    "select.reselect",
    "select.inverse",
    "select.nextAbove",
    "select.nextBelow",
    // View
    "view.zoomIn",
    "view.zoomOut",
    "view.fitArtboard",
    "view.actualSize",
    "view.rulers",
    "view.guides",
    "view.guides.lock",
    "view.guides.clear",
    "view.smartGuides",
    "view.grid",
    "view.snapToGrid",
    "view.snapToPoint",
    // Panels, limited to `PANELS` by the edition
    "window.panel",
];

/// Panels that open (the Variables panel is the guest list).
const PANELS: &[&str] = &["properties", "layers", "align", "transform", "character", "paragraph", "variables"];

/// The toolbar.
const TOOLS: &[(&str, &[&[&str]])] = &[
    ("Select", &[&["selection"]]),
    ("Shapes", &[&["rectangle"], &["ellipse"], &["lineSegment"]]),
    ("Type", &[&["type", "areaType"]]),
    ("Navigate", &[&["hand"], &["zoom"]]),
];

static LABELS: Edition =
    Edition { name: "Labels", commands: COMMANDS, menus: &["File", "Edit", "Object", "Type", "Select", "View"], tools: TOOLS, panels: PANELS };

/// Make the app the labels edition: before it starts.
pub fn install() {
    if !edition::set(&LABELS) {
        log::warn!("vectorcraft-web: an edition was set already");
    }
}

/// The edition's layout, on a new app: no contextual task bar or control bar, the basic toolbar,
/// Smart Guides on.
pub fn configure(app: &mut VectorcraftApp) {
    app.ui.task_bar = false;
    app.ui.control_bar = false;
    app.ui.toolbar_advanced = false;
    app.ui.view.smart_guides = true;
}
