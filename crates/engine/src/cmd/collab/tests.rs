use serde_json::{Value, json};

use super::*;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 200, "height": 100})).unwrap();
    s
}

fn ops(s: &mut Session) -> Vec<Value> {
    s.execute("collab.changes", &json!({})).unwrap()["ops"].as_array().unwrap().clone()
}

fn rect(s: &mut Session, x: f64) -> Value {
    s.execute("shape.rectangle", &json!({"x": x, "y": 0, "width": 10, "height": 10})).unwrap()["id"].clone()
}

/// Two editors of one document: `b` opens what `a` saved.
fn pair() -> (Session, Session) {
    let mut a = session();
    rect(&mut a, 0.0);
    let saved = a.execute("document.export", &json!({"format": "vectorcraft"})).unwrap()["dataBase64"].clone();
    let mut b = Session::new();
    b.execute("document.open", &json!({"dataBase64": saved, "name": "x.vectorcraft"})).unwrap();
    for s in [&mut a, &mut b] {
        ops(s); // the first call sets the base
    }
    (a, b)
}

fn same(a: &Session, b: &Session) -> bool {
    let (x, y) = (&a.doc().unwrap().doc, &b.doc().unwrap().doc);
    diff(x, y).unwrap().is_empty()
}

#[test]
fn an_edit_on_one_side_arrives_on_the_other() {
    let (mut a, mut b) = pair();
    let id = rect(&mut a, 30.0);
    a.execute("paint.setFill", &json!({"color": "#000000"})).unwrap();
    let sent = ops(&mut a);
    assert!(sent.iter().any(|o| o["op"] == "insert"), "{sent:?}");
    assert!(ops(&mut a).is_empty(), "the base moved up");
    let r = b.execute("collab.apply", &json!({"ops": sent})).unwrap();
    assert_eq!(r["skipped"], 0);
    assert!(b.doc().unwrap().doc.node(serde_json::from_value(id).unwrap()).is_some());
    assert!(same(&a, &b));
    assert!(ops(&mut b).is_empty(), "what arrived is not sent back");
}

#[test]
fn moves_reorders_and_removals_arrive() {
    let (mut a, mut b) = pair();
    let r2 = rect(&mut a, 30.0);
    b.execute("collab.apply", &json!({"ops": ops(&mut a)})).unwrap();
    a.execute("object.move", &json!({"dx": 5, "dy": 7})).unwrap();
    a.execute("object.arrange.sendToBack", &json!({})).unwrap();
    b.execute("collab.apply", &json!({"ops": ops(&mut a)})).unwrap();
    assert!(same(&a, &b));
    a.execute("select.set", &json!({"ids": [r2]})).unwrap();
    a.execute("edit.clear", &json!({})).unwrap();
    b.execute("collab.apply", &json!({"ops": ops(&mut a)})).unwrap();
    assert!(same(&a, &b));
}

#[test]
fn undo_reverts_ones_own_step_and_keeps_the_others() {
    let (mut a, mut b) = pair();
    let mine = rect(&mut b, 60.0); // b's own step
    ops(&mut b);
    // Both editors run in this one process, so they share an id lane (in the app each has its own,
    // collab.lane): `a`'s counter moves far ahead so that its rectangle's id is one `b` didn't use.
    {
        let st = a.doc_mut().unwrap();
        let d = Arc::make_mut(&mut st.doc);
        let next = d.peek_next_id() + 1000;
        d.reserve_ids(next);
    }
    let theirs = rect(&mut a, 30.0);
    b.execute("collab.apply", &json!({"ops": ops(&mut a)})).unwrap();
    b.execute("edit.undo", &json!({})).unwrap();
    let d = &b.doc().unwrap().doc;
    assert!(d.node(serde_json::from_value(theirs).unwrap()).is_some(), "the other editor's rectangle stays");
    assert!(d.node(serde_json::from_value(mine).unwrap()).is_none(), "b's own rectangle is undone");
}

#[test]
fn lanes_keep_new_ids_apart() {
    assert!(vectorcraft_doc::lane_id(10, 1, 4) % 4 == 1);
    let mut s = session();
    assert!(s.execute("collab.lane", &json!({"lane": 5, "lanes": 3})).is_err());
    assert!(s.execute("collab.lane", &json!({"lane": 0, "lanes": 0})).is_err());
}

#[test]
fn bad_input_is_refused_or_skipped() {
    let (mut a, _) = pair();
    assert!(a.execute("collab.apply", &json!({"ops": "x"})).is_err());
    assert!(a.execute("collab.apply", &json!({"ops": [{"op": "nonsense"}]})).is_err());
    let r = a.execute("collab.apply", &json!({"ops": [{"op": "remove", "id": 999999}, {"op": "set", "id": 5, "node": {"no": 1}}, {"op": "field", "name": "next_id", "value": 1}]})).unwrap();
    assert_eq!((r["applied"].as_u64(), r["skipped"].as_u64()), (Some(0), Some(3)));
}
