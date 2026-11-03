//! Live co-editing (`docs/labels-edition.md` › Live co-editing): the document's changes as node-level
//! operations, and operations from other editors applied to it.
//!
//! Each editor keeps a *base*: the document as last exchanged. `collab.changes` diffs the document against it
//! (structural sharing: an unchanged subtree is the same `Arc`, so the walk only visits what changed), returns
//! the operations and moves the base up. `collab.apply` applies other editors' operations to the document, to
//! the base (so they are not sent back) and to every undo and redo step (so undoing a step of one's own keeps
//! the others' changes). Ids don't collide: each editor allocates its own lane (`collab.lane`,
//! [`vectorcraft_doc::set_id_lane`]). A server orders the operations; conflicts end last-writer-wins per node.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use vectorcraft_doc::{Document, Node, NodeId};

use super::{CommandSpec, Result, Session, bad, cmd, has_doc};

/// One change to a document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum Op {
    /// A node's own fields: the node as saved, its children left out (they keep theirs).
    Set { id: NodeId, node: Value },
    /// A new node (with its subtree) at `index` among `parent`'s children (`None`: a top-level layer).
    Insert { parent: Option<NodeId>, index: usize, node: Value },
    /// A node and its subtree gone.
    Remove { id: NodeId },
    /// `parent`'s children in this order (ids it doesn't have are skipped; children not named keep their
    /// order after the named ones).
    Order { parent: Option<NodeId>, ids: Vec<NodeId> },
    /// A document-level field (artboards, swatches, variables, …: everything but the layers) set whole.
    Field { name: String, value: Value },
}

impl Op {
    /// The node an operation changes, for the host's "my pending change wins" check.
    pub fn target(&self) -> Option<NodeId> {
        match self {
            Op::Set { id, .. } | Op::Remove { id } => Some(*id),
            Op::Insert { node, .. } => node.get("id").and_then(|v| serde_json::from_value(v.clone()).ok()),
            Op::Order { parent, .. } => *parent,
            Op::Field { .. } => None,
        }
    }
}

/// Per-document collaboration state (by [`crate::DocState`] uid): the base.
#[derive(Default)]
pub struct Collab {
    base: HashMap<u64, Arc<Document>>,
}

/// Document fields that are this editor's alone: never sent, never applied.
const LOCAL_FIELDS: [&str; 2] = ["layers", "next_id"];

/// Cap on the operations one `collab.apply` takes (untrusted input).
const MAX_OPS: usize = 10_000;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            query "collab.changes",
            "Collaboration Changes",
            [],
            None,
            "{} the document's changes since the last call (or since collab.reset) as operations, and the base moves up to the document as it is → {ops: [{op: set|insert|remove|order|field, …}], targets: [node id…]}; the first call after opening only sets the base and returns no ops",
            has_doc,
            changes
        ),
        cmd!(
            query "collab.apply",
            "Apply Collaboration Changes",
            [],
            None,
            "{ops: [op…]} apply other editors' operations (as collab.changes returns them) to the document, its base and every undo and redo step; not an undo step of its own; operations on nodes that are gone are skipped → {applied, skipped}",
            has_doc,
            apply_cmd
        ),
        cmd!(
            query "collab.reset",
            "Reset Collaboration Base",
            [],
            None,
            "{} the document as it is becomes the base (after opening the shared copy, or after a change that is this editor's alone, such as showing another data set) → {}",
            has_doc,
            reset
        ),
        cmd!(
            query "collab.lane",
            "Collaboration Id Lane",
            [],
            None,
            "{lane, lanes} allocate new object ids in lane `lane` of `lanes` (each editor of a shared document its own lane, lanes ≤ 65536) → {lane, lanes}",
            |_| Ok(()),
            lane
        ),
    ]
}

fn uid(s: &Session) -> Result<u64> {
    Ok(s.doc()?.uid)
}

fn changes(s: &mut Session, _p: &Value) -> Result<Value> {
    let id = uid(s)?;
    let now = s.doc()?.doc.clone();
    let ops = match s.collab.base.insert(id, now.clone()) {
        Some(base) => diff(&base, &now)?,
        None => vec![],
    };
    let targets: Vec<NodeId> = ops.iter().filter_map(Op::target).collect();
    Ok(json!({"ops": ops, "targets": targets}))
}

fn reset(s: &mut Session, _p: &Value) -> Result<Value> {
    let id = uid(s)?;
    let now = s.doc()?.doc.clone();
    s.collab.base.insert(id, now);
    Ok(json!({}))
}

fn lane(_s: &mut Session, p: &Value) -> Result<Value> {
    let lane = p.get("lane").and_then(Value::as_u64).ok_or_else(|| bad("collab.lane", "lane must be a whole number"))?;
    let lanes = p.get("lanes").and_then(Value::as_u64).ok_or_else(|| bad("collab.lane", "lanes must be a whole number"))?;
    if !vectorcraft_doc::set_id_lane(lane, lanes) {
        return Err(bad("collab.lane", &format!("lanes must be 1 to {} and lane under lanes", vectorcraft_doc::MAX_ID_LANES)));
    }
    Ok(json!({"lane": lane, "lanes": lanes}))
}

fn apply_cmd(s: &mut Session, p: &Value) -> Result<Value> {
    let raw = p.get("ops").and_then(Value::as_array).ok_or_else(|| bad("collab.apply", "ops must be a list"))?;
    if raw.len() > MAX_OPS {
        return Err(bad("collab.apply", &format!("at most {MAX_OPS} operations at once")));
    }
    let ops: Vec<Op> = raw.iter().map(|o| serde_json::from_value(o.clone())).collect::<std::result::Result<_, _>>().map_err(|e| bad("collab.apply", &e.to_string()))?;
    let id = uid(s)?;
    let st = s.doc_mut()?;
    let (applied, skipped) = apply(Arc::make_mut(&mut st.doc), &ops);
    for entry in st.history.undo.iter_mut().chain(st.history.redo.iter_mut()) {
        let doc = Arc::make_mut(&mut entry.doc);
        apply(doc, &ops);
        doc.drop_lookup_cache();
    }
    // What was removed is no longer selected.
    let gone: HashSet<NodeId> = ops.iter().filter_map(|o| if let Op::Remove { id } = o { Some(*id) } else { None }).collect();
    if !gone.is_empty() {
        st.selection.objects.retain(|o| !gone.contains(o));
    }
    st.revision = st.revision.wrapping_add(1);
    let now = st.doc.clone();
    if let Some(base) = s.collab.base.get_mut(&id) {
        apply(Arc::make_mut(base), &ops);
    } else {
        s.collab.base.insert(id, now);
    }
    Ok(json!({"applied": applied, "skipped": skipped}))
}

// ---- diff ----

/// The operations that turn `old` into `new`.
pub fn diff(old: &Document, new: &Document) -> Result<Vec<Op>> {
    let mut out = vec![];
    if std::ptr::eq(old, new) {
        return Ok(out);
    }
    diff_children(None, &old.layers, &new.layers, &mut out)?;
    let (a, b) = (fields(old)?, fields(new)?);
    for (name, value) in &b {
        if a.get(name) != Some(value) {
            out.push(Op::Field { name: name.clone(), value: value.clone() });
        }
    }
    Ok(out)
}

fn diff_children(parent: Option<NodeId>, old: &[Arc<Node>], new: &[Arc<Node>], out: &mut Vec<Op>) -> Result<()> {
    if old.len() == new.len() && old.iter().zip(new).all(|(a, b)| Arc::ptr_eq(a, b)) {
        return Ok(());
    }
    let olds: HashMap<NodeId, &Arc<Node>> = old.iter().map(|n| (n.id, n)).collect();
    let news: HashSet<NodeId> = new.iter().map(|n| n.id).collect();
    for n in old {
        if !news.contains(&n.id) {
            out.push(Op::Remove { id: n.id });
        }
    }
    for (index, n) in new.iter().enumerate() {
        match olds.get(&n.id) {
            None => out.push(Op::Insert { parent, index, node: to_value(n)? }),
            Some(o) if !Arc::ptr_eq(o, n) => {
                let own = shallow(n)?;
                if shallow(o)? != own {
                    out.push(Op::Set { id: n.id, node: own });
                }
                let none = vec![];
                diff_children(Some(n.id), o.children().unwrap_or(&none), n.children().unwrap_or(&none), out)?;
            }
            Some(_) => {}
        }
    }
    // The order of the children both have.
    let kept_old: Vec<NodeId> = old.iter().map(|n| n.id).filter(|id| news.contains(id)).collect();
    let kept_new: Vec<NodeId> = new.iter().map(|n| n.id).filter(|id| olds.contains_key(id)).collect();
    if kept_old != kept_new {
        out.push(Op::Order { parent, ids: new.iter().map(|n| n.id).collect() });
    }
    Ok(())
}

fn to_value(n: &Node) -> Result<Value> {
    serde_json::to_value(n).map_err(|e| bad("collab", &e.to_string()))
}

/// A node as saved, without its children.
fn shallow(n: &Node) -> Result<Value> {
    let mut copy = n.clone();
    if let Some(kids) = copy.children_mut() {
        kids.clear();
    }
    to_value(&copy)
}

/// The document's own fields as saved, the layers and the local ones left out.
fn fields(d: &Document) -> Result<Map<String, Value>> {
    let mut bare = d.clone();
    bare.layers = vec![];
    match serde_json::to_value(&bare).map_err(|e| bad("collab", &e.to_string()))? {
        Value::Object(mut m) => {
            for k in LOCAL_FIELDS {
                m.remove(k);
            }
            Ok(m)
        }
        _ => Err(bad("collab", "a document saves as an object")),
    }
}

// ---- apply ----

/// Apply `ops` to `doc`: (applied, skipped). An operation on a node that is gone, or one that doesn't
/// read, is skipped; the rest still apply.
pub fn apply(doc: &mut Document, ops: &[Op]) -> (usize, usize) {
    let (mut applied, mut skipped) = (0, 0);
    let mut fields_changed: Map<String, Value> = Map::new();
    for op in ops {
        let ok = match op {
            Op::Set { id, node } => set(doc, *id, node),
            Op::Insert { parent, index, node } => insert(doc, *parent, *index, node),
            Op::Remove { id } => remove(doc, *id),
            Op::Order { parent, ids } => order(doc, *parent, ids),
            Op::Field { name, value } => {
                if LOCAL_FIELDS.contains(&name.as_str()) {
                    false
                } else {
                    fields_changed.insert(name.clone(), value.clone());
                    true
                }
            }
        };
        if ok {
            applied += 1;
        } else {
            skipped += 1;
        }
        if matches!(op, Op::Insert { .. } | Op::Remove { .. } | Op::Order { .. }) {
            doc.drop_lookup_cache();
        }
    }
    if !fields_changed.is_empty() && set_fields(doc, fields_changed).is_err() {
        skipped += 1;
    }
    doc.drop_lookup_cache();
    // The local counter stays above every id the others used.
    doc.fix_next_id();
    (applied, skipped)
}

fn set(doc: &mut Document, id: NodeId, node: &Value) -> bool {
    let Ok(mut new) = serde_json::from_value::<Node>(node.clone()) else { return false };
    if new.id != id {
        return false;
    }
    let Some(cur) = doc.node_mut(id) else { return false };
    let kids = cur.children_mut().map(std::mem::take);
    if let (Some(kids), Some(slot)) = (kids, new.children_mut()) {
        *slot = kids;
    }
    *cur = new;
    true
}

fn insert(doc: &mut Document, parent: Option<NodeId>, index: usize, node: &Value) -> bool {
    let Ok(new) = serde_json::from_value::<Node>(node.clone()) else { return false };
    if doc.node(new.id).is_some() {
        return false;
    }
    let Ok(kids) = doc.children_mut(parent) else { return false };
    let at = index.min(kids.len());
    kids.insert(at, Arc::new(new));
    true
}

fn remove(doc: &mut Document, id: NodeId) -> bool {
    let Some(chain) = doc.ancestry(id) else { return false };
    let parent = chain.len().checked_sub(2).and_then(|i| chain.get(i)).copied();
    let Ok(kids) = doc.children_mut(parent) else { return false };
    let before = kids.len();
    kids.retain(|n| n.id != id);
    kids.len() != before
}

fn order(doc: &mut Document, parent: Option<NodeId>, ids: &[NodeId]) -> bool {
    let Ok(kids) = doc.children_mut(parent) else { return false };
    let rank: HashMap<NodeId, usize> = ids.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    // Stable: named children in the given order, the others after them in their own order.
    kids.sort_by_key(|n| rank.get(&n.id).copied().unwrap_or(usize::MAX));
    true
}

fn set_fields(doc: &mut Document, changed: Map<String, Value>) -> std::result::Result<(), serde_json::Error> {
    let layers = std::mem::take(&mut doc.layers);
    let next = doc.peek_next_id();
    let mut v = serde_json::to_value(&*doc)?;
    if let Value::Object(m) = &mut v {
        for (k, val) in changed {
            m.insert(k, val);
        }
    }
    let result = serde_json::from_value::<Document>(v);
    match result {
        Ok(mut d) => {
            d.layers = layers;
            d.reserve_ids(next);
            *doc = d;
            Ok(())
        }
        Err(e) => {
            doc.layers = layers;
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests;
