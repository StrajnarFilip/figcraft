//! Copy, cut and paste of points and segments in vector edit mode (#1875).

use serde_json::json;

use super::tests::{ex, net, rect, ve, vertex_at};
use super::*;
use crate::params::id_of;

/// The page-space points of layer `id`'s network, rounded to 1e-6 and sorted.
fn page_points(s: &Session, id: NodeId) -> Vec<(i64, i64)> {
    let doc = s.doc().unwrap();
    let w = doc.world_transform(id);
    let mut v: Vec<(i64, i64)> =
        network(doc, id).unwrap().vertices.iter().map(|v| w * v.point()).map(|p| ((p.x * 1e6).round() as i64, (p.y * 1e6).round() as i64)).collect();
    v.sort_unstable();
    v
}

/// A view showing the page from -100,-100 to 700,500, so pastes land in place.
fn in_view(s: &mut Session) {
    ex(s, "view.set", json!({"zoom": 1, "cx": 300, "cy": 200, "vw": 800, "vh": 600}));
}

/// Page point (`x`, `y`) rounded as [`page_points`] rounds.
fn p(x: f64, y: f64) -> (i64, i64) {
    ((x * 1e6).round() as i64, (y * 1e6).round() as i64)
}

/// The rectangle's top edge (its two top corners) selected in edit mode, in view (a paste of
/// something off screen goes to the middle of the view instead of in place).
fn top_edge_selected(s: &mut Session) -> NodeId {
    in_view(s);
    let id = rect(s);
    let n = net(s, id);
    let (a, b) = (vertex_at(&n, 0.0, 0.0), vertex_at(&n, 100.0, 0.0));
    ex(s, "vector.edit.select", json!({"vertices": [a, b]}));
    id
}

/// Copy with points selected takes those paths, not the layer: pasting outside edit mode makes a
/// new vector of just that edge, in place, with the layer's stroke.
#[test]
fn copying_points_and_pasting_outside_edit_mode_makes_a_layer_of_them() {
    let mut s = Session::new();
    let id = top_edge_selected(&mut s);
    ex(&mut s, "node.set", json!({"ids": [id], "props": {"strokes": ["#ff0000"], "strokeWeight": 3}}));
    let r = ex(&mut s, "edit.copy", json!({}));
    assert_eq!(r["paths"], true);
    assert_eq!(net(&s, id).segments.len(), 4, "copy leaves the layer alone");
    ex(&mut s, "vector.edit.end", json!({}));
    let r = ex(&mut s, "edit.paste", json!({}));
    let pasted = id_of(&r["ids"][0]).unwrap();
    assert_ne!(pasted, id, "a new layer");
    let n = net(&s, pasted);
    assert_eq!((n.vertices.len(), n.segments.len()), (2, 1), "just the top edge: {n:?}");
    assert_eq!(page_points(&s, pasted), vec![p(0.0, 0.0), p(100.0, 0.0)], "where it was");
    let node = s.doc().unwrap().node(pasted).unwrap();
    assert_eq!(node.stroke.weight, 3.0);
    assert_eq!(node.strokes.len(), 1, "the layer's stroke");
}

/// Cut copies the selected paths and deletes them as Delete does: a selected segment goes, its
/// points stay; selected points go with their segments.
#[test]
fn cutting_a_segment_removes_it_and_pastes_it_back_as_a_layer() {
    let mut s = Session::new();
    in_view(&mut s);
    let id = rect(&mut s);
    let n = net(&s, id);
    let (a, b) = (vertex_at(&n, 0.0, 0.0), vertex_at(&n, 100.0, 0.0));
    let top = n.segments.iter().position(|sg| (sg.a == a && sg.b == b) || (sg.a == b && sg.b == a)).unwrap();
    ex(&mut s, "vector.edit.select", json!({"segments": [top]}));
    ex(&mut s, "edit.cut", json!({}));
    let n = net(&s, id);
    assert_eq!((n.vertices.len(), n.segments.len()), (4, 3), "the edge is gone, its points stay: {n:?}");
    assert!(s.ds().unwrap().vector_edit.is_some(), "still editing");
    ex(&mut s, "vector.edit.end", json!({}));
    let r = ex(&mut s, "edit.paste", json!({}));
    let pasted = id_of(&r["ids"][0]).unwrap();
    assert_eq!(page_points(&s, pasted), vec![p(0.0, 0.0), p(100.0, 0.0)]);
    // One undo brings the cut edge back.
    ex(&mut s, "edit.undo", json!({}));
    ex(&mut s, "edit.undo", json!({}));
    assert_eq!(net(&s, id).segments.len(), 4);
}

/// Paste in edit mode adds the copied paths to the edited layer, in place, and selects them; a
/// paste of points copied from another vector joins them to this one.
#[test]
fn pasting_in_edit_mode_adds_the_paths_to_the_edited_layer() {
    let mut s = Session::new();
    top_edge_selected(&mut s);
    ex(&mut s, "edit.copy", json!({}));
    ex(&mut s, "vector.edit.end", json!({}));
    let other = id_of(&ex(&mut s, "vector.create", json!({"points": [[300, 300], [400, 350]], "closed": false}))["id"]).unwrap();
    ex(&mut s, "vector.edit.begin", json!({"id": other}));
    let r = ex(&mut s, "edit.paste", json!({}));
    assert_eq!(r["ids"], json!([other]), "no new layer");
    let n = net(&s, other);
    assert_eq!((n.vertices.len(), n.segments.len()), (4, 2), "{n:?}");
    assert_eq!(page_points(&s, other), vec![p(0.0, 0.0), p(100.0, 0.0), p(300.0, 300.0), p(400.0, 350.0)], "in place");
    let sel = ve(&s);
    assert_eq!(sel.vertices.len(), 2, "the pasted points are selected");
    assert!(sel.vertices.iter().all(|v| page_points_of(&s, other, *v).1 == 0), "{:?}", sel.vertices);
    let page = s.page().unwrap();
    assert_eq!(s.doc().unwrap().node(page).unwrap().children.len(), 2, "still two layers");
    // One undo takes the paste back.
    ex(&mut s, "edit.undo", json!({}));
    assert_eq!(net(&s, other).segments.len(), 1);
}

/// The page point of vertex `v` of layer `id`, rounded.
fn page_points_of(s: &Session, id: NodeId, v: usize) -> (i64, i64) {
    let doc = s.doc().unwrap();
    let pt = doc.world_transform(id) * network(doc, id).unwrap().vertices[v].point();
    ((pt.x).round() as i64, (pt.y).round() as i64)
}

/// A copied layer that isn't a vector (a rectangle) pastes its outline into the edited layer, a
/// closed one with its fill area.
#[test]
fn a_copied_shape_pastes_its_outline_into_the_edited_layer() {
    let mut s = Session::new();
    let r = id_of(&ex(&mut s, "shape.rectangle", json!({"x": 200, "y": 0, "width": 50, "height": 40}))["id"]).unwrap();
    ex(&mut s, "select.set", json!({"ids": [r]}));
    ex(&mut s, "edit.copy", json!({}));
    let v = id_of(&ex(&mut s, "vector.create", json!({"points": [[0, 0], [100, 0]], "closed": false}))["id"]).unwrap();
    ex(&mut s, "vector.edit.begin", json!({"id": v}));
    ex(&mut s, "edit.paste", json!({}));
    let n = net(&s, v);
    assert_eq!((n.vertices.len(), n.segments.len(), n.regions.len()), (6, 5, 1), "{n:?}");
    let pts = page_points(&s, v);
    for corner in [p(200.0, 0.0), p(250.0, 0.0), p(250.0, 40.0), p(200.0, 40.0)] {
        assert!(pts.contains(&corner), "{corner:?} in {pts:?}");
    }
}

/// A rotated, moved source keeps its page position when its points are copied, and a pasted
/// path lands right in a rotated edited layer.
#[test]
fn copies_and_pastes_keep_page_positions_through_transforms() {
    let mut s = Session::new();
    let id = top_edge_selected(&mut s);
    ex(&mut s, "vector.edit.end", json!({}));
    ex(&mut s, "node.set", json!({"ids": [id], "props": {"rotation": 30, "x": 50, "y": 70}}));
    let before = page_points(&s, id);
    ex(&mut s, "vector.edit.begin", json!({"id": id}));
    let n = net(&s, id);
    let w = s.doc().unwrap().world_transform(id);
    let top: Vec<usize> = (0..n.vertices.len())
        .filter(|v| {
            before.iter().take(2).any(|q| {
                *q == {
                    let pt = w * n.vertices[*v].point();
                    p(pt.x, pt.y)
                }
            })
        })
        .collect();
    ex(&mut s, "vector.edit.select", json!({"vertices": [top[0], top[1]]}));
    ex(&mut s, "edit.copy", json!({}));
    let copied: Vec<(i64, i64)> = top
        .iter()
        .map(|v| {
            let pt = w * n.vertices[*v].point();
            p(pt.x, pt.y)
        })
        .collect();
    ex(&mut s, "vector.edit.end", json!({}));
    let target = id_of(&ex(&mut s, "vector.create", json!({"points": [[500, 500], [600, 500]], "closed": false}))["id"]).unwrap();
    ex(&mut s, "node.set", json!({"ids": [target], "props": {"rotation": -45}}));
    ex(&mut s, "vector.edit.begin", json!({"id": target}));
    ex(&mut s, "edit.paste", json!({}));
    let after = page_points(&s, target);
    for q in copied {
        assert!(after.iter().any(|a| (a.0 - q.0).abs() < 10 && (a.1 - q.1).abs() < 10), "{q:?} in {after:?}");
    }
}

/// Without points selected, Copy in edit mode copies the layer as before; points selected alone,
/// with no segment between them, have no paths to copy.
#[test]
fn copy_without_paths_falls_back_or_says_why() {
    let mut s = Session::new();
    let id = rect(&mut s);
    ex(&mut s, "edit.copy", json!({}));
    assert_eq!(s.clipboard.nodes.first().map(|(root, _)| *root), Some(id));
    assert_eq!(s.clipboard.nodes[0].1[0].kind, s.doc().unwrap().node(id).unwrap().kind, "the whole layer");
    let n = net(&s, id);
    ex(&mut s, "vector.edit.select", json!({"vertices": [vertex_at(&n, 0.0, 0.0)]}));
    let e = s.execute("edit.copy", &json!({})).unwrap_err();
    assert!(e.to_string().contains("select a segment"), "{e}");
}

/// Pasting text (no paths) while editing pastes the layer as before.
#[test]
fn a_paste_without_paths_pastes_layers() {
    let mut s = Session::new();
    let t = id_of(&ex(&mut s, "text.create", json!({"text": "Hi", "x": 300, "y": 0}))["id"]).unwrap();
    ex(&mut s, "select.set", json!({"ids": [t]}));
    ex(&mut s, "edit.copy", json!({}));
    let id = rect(&mut s);
    let before = net(&s, id);
    let r = ex(&mut s, "edit.paste", json!({}));
    assert_ne!(id_of(&r["ids"][0]), Some(id));
    assert_eq!(net(&s, id), before);
}

/// A horizontal line (bounds without area) in view pastes in place, not in the middle of the
/// view as something off screen does.
#[test]
fn a_horizontal_line_in_view_pastes_in_place() {
    let mut s = Session::new();
    in_view(&mut s);
    let line = id_of(&ex(&mut s, "vector.create", json!({"points": [[10, 20], [110, 20]], "closed": false}))["id"]).unwrap();
    ex(&mut s, "select.set", json!({"ids": [line]}));
    ex(&mut s, "edit.copy", json!({}));
    ex(&mut s, "select.set", json!({"ids": []}));
    let pasted = id_of(&ex(&mut s, "edit.paste", json!({}))["ids"][0]).unwrap();
    assert_eq!(page_points(&s, pasted), vec![p(10.0, 20.0), p(110.0, 20.0)]);
    // Off screen it still goes to the middle of the view.
    ex(&mut s, "view.set", json!({"cx": 5000, "cy": 5000}));
    let moved = id_of(&ex(&mut s, "edit.paste", json!({}))["ids"][0]).unwrap();
    assert_ne!(page_points(&s, moved), vec![p(10.0, 20.0), p(110.0, 20.0)]);
}
