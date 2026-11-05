//! A drag on the canvas isn't stopped by the panels (#2023): a layer moved past the canvas's edge
//! keeps following the pointer over the side panel, and the view pans towards the edge while the
//! drag is held near or past it, as it does for a prototype connection.

use egui::{Event, Modifiers, PointerButton, Pos2, pos2};
use figcraft_doc::NodeId;
use figcraft_engine::Session;
use serde_json::json;

use crate::layers_drag_tests::Ui;

/// A 100 × 100 rectangle at the origin in the middle of a 100% view, and its centre on screen.
fn setup() -> (Ui, NodeId, Pos2) {
    let mut s = Session::new();
    let r = s.execute("shape.rectangle", &json!({"x": 0, "y": 0, "width": 100, "height": 100})).unwrap();
    let id = figcraft_engine::params::id_of(&r["id"]).unwrap();
    let mut u = Ui::new(s);
    u.app.run("view.set", json!({"zoom": 1, "cx": 50, "cy": 50}));
    for _ in 0..3 {
        u.frame(vec![]);
    }
    let at = u.app.to_screen(50.0, 50.0);
    (u, id, at)
}

/// The layer's page x.
fn layer_x(u: &Ui, id: NodeId) -> f64 {
    u.app.session.doc().unwrap().node(id).unwrap().transform.translation().x
}

/// The page x at the middle of the view.
fn view_cx(u: &Ui) -> f64 {
    u.app.session.ds().unwrap().view.cx
}

/// A primary button event at `p`.
fn button(p: Pos2, pressed: bool) -> Event {
    Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE }
}

/// Dragged over the right panel, the layer keeps following the pointer: it doesn't stop where the
/// canvas ends, and the view pans after it.
#[test]
fn a_layer_dragged_over_the_side_panel_keeps_following_the_pointer() {
    let (mut u, id, at) = setup();
    let edge = u.app.canvas.rect.max.x;
    u.frame(vec![Event::PointerMoved(at)]);
    u.frame(vec![button(at, true)]);
    let mut xs = Vec::new();
    for x in [edge - 60.0, edge + 60.0, edge + 160.0] {
        u.frame(vec![Event::PointerMoved(pos2(x, at.y))]);
        u.frame(vec![]);
        xs.push(layer_x(&u, id));
    }
    assert!(xs[1] > xs[0] + 100.0, "past the canvas's edge it goes on: {xs:?}");
    assert!(xs[2] > xs[1] + 100.0, "further over the panel, further along: {xs:?}");
    assert!(view_cx(&u) > 50.0, "the view panned after it");
    // Let go over the panel: the move ends there, in one undo step.
    let over = pos2(edge + 160.0, at.y);
    u.frame(vec![button(over, false)]);
    let dropped = layer_x(&u, id);
    u.frame(vec![Event::PointerMoved(at)]);
    u.frame(vec![]);
    assert_eq!(layer_x(&u, id), dropped, "a pointer moving after the release moves nothing");
    u.app.run("edit.undo", json!({}));
    assert_eq!(layer_x(&u, id), 0.0);
}

/// Held still near the edge, the drag goes on: the view pans every frame and the layer with it.
#[test]
fn a_drag_held_at_the_edge_pans_the_view_and_carries_the_layer() {
    let (mut u, id, at) = setup();
    let near = pos2(u.app.canvas.rect.max.x - 4.0, at.y);
    u.frame(vec![Event::PointerMoved(at)]);
    u.frame(vec![button(at, true)]);
    u.frame(vec![Event::PointerMoved(near)]);
    let (cx, x) = (view_cx(&u), layer_x(&u, id));
    for _ in 0..30 {
        u.frame(vec![]);
    }
    assert!(view_cx(&u) > cx + 100.0, "{} → {}", cx, view_cx(&u));
    assert!(layer_x(&u, id) > x + 100.0, "the layer stays under the pointer as the view moves");
    u.frame(vec![button(near, false)]);
}

/// A press near the edge that hasn't moved doesn't pan, nor does a drag away from the edges.
#[test]
fn a_press_near_the_edge_alone_never_pans() {
    let (mut u, _, _) = setup();
    u.app.run("view.set", json!({"zoom": 1, "cx": -350, "cy": 50}));
    for _ in 0..3 {
        u.frame(vec![]);
    }
    let corner = u.app.to_screen(98.0, 50.0);
    assert!(corner.x > u.app.canvas.rect.max.x - 20.0, "the layer's edge is near the canvas's: {corner:?}");
    u.frame(vec![Event::PointerMoved(corner)]);
    u.frame(vec![button(corner, true)]);
    for _ in 0..10 {
        u.frame(vec![]);
    }
    assert_eq!(view_cx(&u), -350.0);
    u.frame(vec![button(corner, false)]);
}
