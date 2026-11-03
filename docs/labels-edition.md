# The labels edition (DE0CH/vectorcraft)

YYC's label editor (DE0CH/YYC `labels/`) is this fork of VectorCraft running in the browser: Illustrator-style
editing cut down to what staff need for labels, with live co-editing between every open editor and YYC AI, guest
lists as data sets, and printing on the NIIMBOT M3. Decided by Deyao on 2026-11-03 (the hand-written editor before it
had no snapping, guides or undo; VectorCraft was his pick, with live editing built in and functionality staff don't
need stripped).

Upstream stays mergeable: the edition is a separate web app (`apps/vectorcraft-labels`) plus small, named hooks in
the engine and the egui front end. No upstream file is deleted; merges from storytold/vectorcraft are plain merges
(never a rebase or a force push).

## What staff get

- **One artboard, the label:** 72 × 50 mm (the M3's printable area of a 75 × 50 mm label), shown as the
  black-and-white print will look.
- **Tools:** Selection, Type, Rectangle, Ellipse, Line, Place (images), Hand, Zoom. Smart Guides and snapping on.
- **Panels:** Properties (Transform, Character, Paragraph, fill and stroke), Align, Layers, Guest list (the data sets).
- **Menus:** Edit (undo, redo, cut, copy, paste, delete), Object (arrange, group, lock), Type, Select, View.
- **Everything else is hidden:** effects, 3D, brushes, gradients, Pathfinder, Image Trace, symbols, files and
  export menus, plug-ins, workspaces.

## Files

Designs live in the label store (the `yyc-labels` Worker's Durable Object): `designs/<id>/label.vectorcraft` (the
native format), `lists/<name>.csv` (guest lists, shared by designs), `logos/`. The editor opens and saves through
the store; there is no File menu.

## Guest lists = data sets

On open, the design's list becomes its data sets: one variable per column, one data set per guest (named by `_id`).
Text objects bind to columns (Variables). The editor shows one guest at a time (`dataset.select`) and renders only
that label; other guests render only when printed. A change for one guest is that guest's data set value.

## Live co-editing

Each committed history step is diffed against the previous document (structural sharing: unchanged subtrees are the
same `Arc`, so the diff walks only what changed) into node-level patches (node set, insert at index, remove,
document-level field set). The Durable Object orders patches and relays them; every editor applies remote patches as
non-undoable updates. Undo reverts only the person's own steps (the inverse patch of their step, sent like any other).
Conflicts resolve last-writer-wins per node. Other people's selections show as outlines.

## Printing

For each guest: select the data set, render the artboard at 300 dpi (848 × 590 dots) with the engine's renderer,
threshold to 1 bit, send over Web Bluetooth to the M3 (the protocol of DE0CH/YYC `labels/worker/public/lib/print.js`,
which matches `niimbot.py` byte for byte).

## YYC AI

The AI edits the same live document: an MCP server in YYC AI (the label route) joins the session as one more editor
and offers the engine's commands, so its changes appear live and staff's appear to it.

## Milestones

1. The stripped editor in the label site: open and save through the store, guest lists as data sets, printing.
2. Live co-editing (patches, ordering, presence, own-step undo).
3. YYC AI as a live editor through the MCP.
