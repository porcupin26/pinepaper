use pinepaper::{
    exchange::{
        replica::{Change, Replica},
        transport::{endpoint, room_code},
    },
    model::*,
};
use serde_json::json;
fn fixture() -> (Document, Vec<Page>) {
    let id = identifier();
    let doc = Document {
        id: id.clone(),
        title: "Shared notebook".into(),
        kind: "notebook".into(),
        parent: None,
        source: None,
        current_page: 0,
        trashed: false,
    };
    let page = Page {
        id: identifier(),
        document: id,
        position: 0,
        payload: Payload::blank("Lined"),
        bookmarked: false,
        tags: vec![],
    };
    (doc, vec![page])
}
fn stroke(x: f64) -> Stroke {
    Stroke {
        points: vec![[x, 20., 0.6], [x + 10., 30., 0.8]],
        ..Stroke::default()
    }
}
fn merge(replica: &mut Replica, changes: &[Change]) {
    let doc = replica.state.document.clone();
    replica.merge(&doc, changes).unwrap();
}
fn values(r: &mut Replica) -> Vec<Payload> {
    r.pages("local")
        .unwrap()
        .into_iter()
        .map(|p| p.payload)
        .collect()
}
#[test]
fn simultaneous_strokes_erase_and_undo_preserve_other_writers() {
    let (doc, mut ap) = fixture();
    let mut a = Replica::from_local(&doc, &mut ap).unwrap();
    let mut b = Replica::from_shared(a.state.clone()).unwrap();
    let mut bp = b.pages("other-copy").unwrap();
    assert_ne!(ap[0].id, bp[0].id);
    ap[0].payload.strokes.push(stroke(10.));
    let ac = a.capture(&doc.title, &mut ap, false).unwrap();
    bp[0].payload.strokes.push(stroke(30.));
    let bc = b.capture(&doc.title, &mut bp, false).unwrap();
    merge(&mut a, &bc);
    merge(&mut b, &ac);
    merge(&mut b, &ac);
    assert_eq!(values(&mut a), values(&mut b));
    assert_eq!(values(&mut a)[0].strokes.len(), 2);
    let undo = a.undo(false);
    merge(&mut b, &undo);
    assert_eq!(values(&mut a), values(&mut b));
    assert_eq!(values(&mut a)[0].strokes[0].points[0][0], 30.);
    let redo = a.undo(true);
    merge(&mut b, &redo);
    assert_eq!(values(&mut b)[0].strokes.len(), 2);
    ap = a.pages(&doc.id).unwrap();
    bp = b.pages("other-copy").unwrap();
    ap[0].payload.strokes.clear();
    let erased = a.capture(&doc.title, &mut ap, false).unwrap();
    bp[0].payload.strokes.push(stroke(80.));
    let added = b.capture(&doc.title, &mut bp, false).unwrap();
    merge(&mut a, &added);
    merge(&mut b, &erased);
    assert_eq!(values(&mut a), values(&mut b));
    assert_eq!(values(&mut a)[0].strokes.len(), 1);
}
#[test]
fn undo_never_reverts_a_later_peer_edit_of_the_same_object() {
    let (doc, mut ap) = fixture();
    let mut a = Replica::from_local(&doc, &mut ap).unwrap();
    ap[0].payload.strokes.push(stroke(10.));
    a.capture(&doc.title, &mut ap, false).unwrap();
    let mut b = Replica::from_shared(a.state.clone()).unwrap();
    let mut bp = b.pages("other").unwrap();
    bp[0].payload.strokes[0].points[0][0] = 99.;
    let edit = b.capture(&doc.title, &mut bp, false).unwrap();
    merge(&mut a, &edit);
    assert!(a.undo(false).is_empty());
    assert_eq!(values(&mut a)[0].strokes[0].points[0][0], 99.);
}
#[test]
fn concurrent_page_insertion_and_deletion_has_stable_order() {
    let (doc, mut ap) = fixture();
    let mut a = Replica::from_local(&doc, &mut ap).unwrap();
    let mut b = Replica::from_shared(a.state.clone()).unwrap();
    let mut bp = b.pages("other").unwrap();
    let mut anew = ap[0].clone();
    anew.id = identifier();
    anew.position = 1;
    anew.payload.template = "Grid".into();
    ap.push(anew);
    let mut bnew = bp[0].clone();
    bnew.id = identifier();
    bnew.position = 1;
    bnew.payload.template = "Dots".into();
    bp.push(bnew);
    let ac = a.capture(&doc.title, &mut ap, true).unwrap();
    let bc = b.capture(&doc.title, &mut bp, true).unwrap();
    merge(&mut a, &bc);
    merge(&mut b, &ac);
    assert_eq!(values(&mut a), values(&mut b));
    assert_eq!(values(&mut a).len(), 3);
    ap = a.pages(&doc.id).unwrap();
    bp = b.pages("other").unwrap();
    ap.remove(0);
    bp.remove(1);
    let ac = a.capture(&doc.title, &mut ap, true).unwrap();
    let bc = b.capture(&doc.title, &mut bp, true).unwrap();
    merge(&mut a, &bc);
    merge(&mut b, &ac);
    assert_eq!(values(&mut a), values(&mut b));
    assert_eq!(values(&mut a).len(), 1);
}
#[test]
fn metadata_text_and_reordered_operations_converge() {
    let (doc, mut ap) = fixture();
    let mut a = Replica::from_local(&doc, &mut ap).unwrap();
    let mut b = Replica::from_shared(a.state.clone()).unwrap();
    let mut bp = b.pages("other").unwrap();
    ap[0].bookmarked = true;
    ap[0].tags = vec!["review".into()];
    let first = a.capture("Renamed", &mut ap, false).unwrap();
    ap[0].payload.template = "Grid".into();
    let second = a.capture("Renamed", &mut ap, false).unwrap();
    bp[0].payload.texts.push(Text {
        text: "hello".into(),
        x: 10.,
        y: 20.,
        size: 25.,
        color: 0.,
        font: "Sans".into(),
        extra: Default::default(),
    });
    let third = b.capture(&doc.title, &mut bp, false).unwrap();
    merge(&mut a, &third);
    merge(&mut b, &second);
    merge(&mut b, &first);
    assert_eq!(values(&mut a), values(&mut b));
    assert_eq!(b.title(), "Renamed");
    let p = b.pages("other").unwrap();
    assert!(p[0].bookmarked);
    assert_eq!(p[0].tags, ["review"]);
}
#[test]
fn invalid_batch_is_atomic_and_local_store_preserves_unrelated_notes() {
    let (doc, mut p) = fixture();
    let mut a = Replica::from_local(&doc, &mut p).unwrap();
    let before = serde_json::to_value(&a.state).unwrap();
    p[0].payload.strokes.push(stroke(10.));
    let mut b = Replica::from_shared(a.state.clone()).unwrap();
    let mut other = b.pages("other").unwrap();
    other[0].payload.strokes.push(stroke(2.));
    let mut edits = b.capture(&doc.title, &mut other, false).unwrap();
    let mut bad = edits[0].clone();
    bad.key = "../../bad".into();
    bad.cell.value = Some(json!("bad"));
    edits.push(bad);
    let id = a.state.document.clone();
    assert!(a.merge(&id, &edits).is_err());
    assert_eq!(serde_json::to_value(&a.state).unwrap(), before);
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path()).unwrap();
    let private = store
        .create(
            "Private",
            "notebook",
            None,
            vec![Payload::blank("Dots")],
            None,
        )
        .unwrap();
    store.save_shared(&doc, &p).unwrap();
    assert_eq!(store.pages(&private).unwrap()[0].payload.template, "Dots");
    let mut wrong = p.clone();
    wrong[0].document = private.clone();
    assert!(store.save_shared(&doc, &wrong).is_err());
    assert_eq!(store.pages(&doc.id).unwrap()[0].payload, p[0].payload);
}
#[test]
fn url_and_code_validation() {
    assert_eq!(
        endpoint("https://notes.example/relay/").unwrap().as_str(),
        "wss://notes.example/relay/v1/exchange"
    );
    assert_eq!(
        endpoint("http://127.0.0.1:8787").unwrap().as_str(),
        "ws://127.0.0.1:8787/v1/exchange"
    );
    assert_eq!(
        endpoint("wss://example.com/v1/exchange").unwrap().path(),
        "/v1/exchange"
    );
    for bad in [
        "file:///tmp/foo",
        "https://user:password@example.com",
        "https://example.com/?secret=yes",
        "not a URL",
    ] {
        assert!(endpoint(bad).is_err());
    }
    assert_eq!(room_code(" 1g6tw ").unwrap(), "1G6TW");
    for bad in ["1G6TW7", "x", "é1234", "../xx"] {
        assert!(room_code(bad).is_err());
    }
}

#[test]
fn wire_roundtrip_distinguishes_nullable_metadata_from_tombstones() {
    let (doc, mut p) = fixture();
    let mut r = Replica::from_local(&doc, &mut p).unwrap();
    p[0].payload.strokes.push(stroke(1.));
    r.capture(&doc.title, &mut p, false).unwrap();
    p[0].payload.strokes.clear();
    r.capture(&doc.title, &mut p, false).unwrap();
    let decoded: pinepaper::exchange::replica::SharedState =
        serde_json::from_str(&serde_json::to_string(&r.state).unwrap()).unwrap();
    decoded.validate().unwrap();
    assert_eq!(decoded.cells, r.state.cells);
    let mut other = Replica::from_shared(decoded).unwrap();
    assert_eq!(values(&mut r), values(&mut other));
}

#[test]
fn diagram_objects_roundtrip_share_and_undo_as_one_object() {
    use pinepaper::diagram::Shape;
    let (doc, mut pages) = fixture();
    let mut a = Replica::from_local(&doc, &mut pages).unwrap();
    let mut b = Replica::from_shared(a.state.clone()).unwrap();
    assert!(!a.can_undo(false));
    pages[0].payload.strokes.push(Shape::Arrow.stroke(
        [30., 40., 0.5],
        [200., 100., 0.5],
        2.4,
        0.07,
    ));
    let changes = a.capture(&doc.title, &mut pages, false).unwrap();
    let changes: Vec<Change> =
        serde_json::from_str(&serde_json::to_string(&changes).unwrap()).unwrap();
    merge(&mut b, &changes);
    let received = values(&mut b);
    assert_eq!(received[0].strokes.len(), 1);
    assert_eq!(received[0].strokes[0].extra["diagram_shape"], "Arrow");
    assert!(a.can_undo(false));
    merge(&mut b, &a.undo(false));
    assert!(values(&mut b)[0].strokes.is_empty());
    assert!(!a.can_undo(false));
    assert!(a.can_undo(true));
    merge(&mut b, &a.undo(true));
    assert_eq!(values(&mut b)[0].strokes.len(), 1);
}
