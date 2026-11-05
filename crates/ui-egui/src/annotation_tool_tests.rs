//! Dev Mode's Annotation tool (Shift+T, #1840): with a layer selected, its new annotation takes
//! the keyboard; with none, the canvas waits (a crosshair) for a click on a layer, and Escape puts
//! the tool away. From the design, the key enters Dev Mode.

use egui::{CursorIcon, Event, Key, Modifiers, Pos2};
use figcraft_engine::Session;
use serde_json::json;

use crate::RightTab;
use crate::layers_drag_tests::Ui;

/// Press and release `key` with `m` held, the pointer at `at`.
fn press(u: &mut Ui, at: Pos2, key: Key, m: Modifiers) -> egui::FullOutput {
    u.frame_with(vec![Event::PointerMoved(at), Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: m }], m);
    u.frame_with(vec![Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers: m }], m)
}

/// A rectangle in view and the screen point at its middle.
fn setup() -> (Ui, figcraft_doc::NodeId, Pos2) {
    let mut s = Session::new();
    let r = s.execute("shape.rectangle", &json!({"x": 0, "y": 0, "width": 200, "height": 120})).unwrap();
    let id = figcraft_engine::params::id_of(&r["id"]).unwrap();
    s.set_selection(Vec::new());
    let mut u = Ui::new(s);
    u.app.run("view.set", json!({"zoom": 1, "cx": 100, "cy": 60}));
    for _ in 0..3 {
        u.frame(vec![]);
    }
    let at = u.app.to_screen(100.0, 60.0);
    (u, id, at)
}

/// The new-annotation field of `id` has the keyboard.
fn note_focused(u: &Ui, id: figcraft_doc::NodeId) -> bool {
    u.ctx.memory(|m| m.has_focus(egui::Id::unique(("annot_new", id)).with("te")))
}

/// From the design, Shift+T with a layer selected enters Dev Mode and its new note takes what is typed.
#[test]
fn shift_t_with_a_layer_selected_opens_its_new_annotation_in_dev_mode() {
    let (mut u, id, at) = setup();
    u.app.session.set_selection(vec![id]);
    assert_eq!(u.app.ui.right_tab, RightTab::Design);
    press(&mut u, at, Key::T, Modifiers::SHIFT);
    for _ in 0..3 {
        u.frame(vec![]);
    }
    assert_eq!(u.app.ui.right_tab, RightTab::Inspect, "the key enters Dev Mode");
    assert!(note_focused(&u, id), "the note takes the keyboard");
    // What is typed there becomes the layer's annotation.
    u.frame(vec![Event::Text("Use 16 px padding".into())]);
    // A click on the panel's empty space below ends the field and keeps the note.
    let below = egui::pos2(1320.0, 880.0);
    for pressed in [true, false] {
        u.frame(vec![
            Event::PointerMoved(below),
            Event::PointerButton { pos: below, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE },
        ]);
    }
    let notes: Vec<String> = u.app.session.doc().unwrap().node(id).unwrap().annotations.iter().map(|a| a.label.clone()).collect();
    assert_eq!(notes, ["Use 16 px padding"]);
}

/// With nothing selected the tool shows a crosshair and the clicked layer's note takes the keyboard.
#[test]
fn with_nothing_selected_the_tool_waits_for_a_click_on_a_layer() {
    let (mut u, id, at) = setup();
    u.app.ui.right_tab = RightTab::Inspect;
    let empty = u.app.to_screen(300.0, 300.0);
    press(&mut u, empty, Key::T, Modifiers::SHIFT);
    assert!(u.app.ui.dev.annotating.is_some(), "the tool waits");
    let out = u.frame_output(vec![Event::PointerMoved(empty)]);
    assert_eq!(out.platform_output.cursor_icon, CursorIcon::Crosshair);
    u.frame(vec![Event::PointerMoved(at)]);
    for pressed in [true, false] {
        u.frame(vec![Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Modifiers::NONE }]);
    }
    for _ in 0..3 {
        u.frame(vec![]);
    }
    assert_eq!(u.app.session.selection(), vec![id]);
    assert!(u.app.ui.dev.annotating.is_none(), "one click, one layer");
    assert!(note_focused(&u, id), "the clicked layer's note takes the keyboard");
}

/// Escape puts the waiting tool away and the crosshair goes.
#[test]
fn escape_puts_the_tool_away() {
    let (mut u, _, _) = setup();
    u.app.ui.right_tab = RightTab::Inspect;
    let empty = u.app.to_screen(300.0, 300.0);
    press(&mut u, empty, Key::T, Modifiers::SHIFT);
    assert!(u.app.ui.dev.annotating.is_some());
    press(&mut u, empty, Key::Escape, Modifiers::NONE);
    assert!(u.app.ui.dev.annotating.is_none());
    let out = u.frame_output(vec![Event::PointerMoved(empty)]);
    assert_ne!(out.platform_output.cursor_icon, CursorIcon::Crosshair);
}
