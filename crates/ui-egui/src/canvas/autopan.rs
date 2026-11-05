//! The view pans by itself while something dragged on the canvas is held near or past one of its
//! edges, so what lies beyond the edge can be reached without letting go: a layer moved or
//! resized there, a selection box or a new shape drawn there (#2023), a prototype connection
//! dragged to a frame off screen (#2015).

use egui::{Rect, Vec2, vec2};

use crate::FigApp;

/// How far inside the canvas's edge a drag starts panning the view (points).
pub(crate) const AUTOPAN_BAND: f32 = 32.0;

/// Pan speed (points a second) with the pointer on the canvas's edge; it doubles one band past
/// the edge and goes no faster beyond.
pub(crate) const AUTOPAN_SPEED: f32 = 600.0;

/// How far (screen points, this frame of `dt` seconds) the view pans towards the edge of `canvas`
/// that `pos` is near or past: nothing more than [`AUTOPAN_BAND`] inside it, then faster the
/// closer the pointer is and past the edge, up to twice [`AUTOPAN_SPEED`]. Positive x: the view
/// moves right (the content left), revealing what lies to the right.
pub(crate) fn edge_pan(canvas: Rect, pos: egui::Pos2, dt: f32) -> Vec2 {
    if !(pos.x.is_finite() && pos.y.is_finite() && dt.is_finite()) || dt <= 0.0 || !canvas.is_positive() {
        return Vec2::ZERO;
    }
    // How deep into the band (0 at its inner side, 1 on the edge, 2 one band past it) on each side.
    let depth = |inside: f32| ((AUTOPAN_BAND - inside) / AUTOPAN_BAND).clamp(0.0, 2.0);
    let step = |low: f32, high: f32| (depth(high) - depth(low)) * AUTOPAN_SPEED * dt;
    vec2(step(pos.x - canvas.min.x, canvas.max.x - pos.x), step(pos.y - canvas.min.y, canvas.max.y - pos.y))
}

/// Pan the view by `by` screen points (positive x shows what lies to the right). Returns whether
/// it moved.
pub(crate) fn pan_view(app: &mut FigApp, by: Vec2) -> bool {
    if by == Vec2::ZERO {
        return false;
    }
    let Ok(ds) = app.session.ds_mut() else { return false };
    let z = ds.view.zoom.max(1e-6);
    ds.view.cx += f64::from(by.x) / z;
    ds.view.cy += f64::from(by.y) / z;
    ds.revision += 1;
    true
}

#[cfg(test)]
mod tests {
    use egui::pos2;

    use super::*;

    /// Nothing pans away from the edge; on it the view pans towards it at the set speed, faster
    /// past it up to twice that, on both axes at a corner, and not at all for a bad frame time.
    #[test]
    fn the_view_pans_towards_the_edge_a_drag_is_held_at() {
        let canvas = Rect::from_min_max(pos2(300.0, 40.0), pos2(1200.0, 860.0));
        let dt = 0.5;
        let full = AUTOPAN_SPEED * dt;
        assert_eq!(edge_pan(canvas, canvas.center(), dt), Vec2::ZERO);
        assert_eq!(edge_pan(canvas, pos2(1200.0 - AUTOPAN_BAND - 1.0, 400.0), dt), Vec2::ZERO, "outside the band");
        assert_eq!(edge_pan(canvas, pos2(1200.0, 400.0), dt), vec2(full, 0.0), "on the right edge");
        assert_eq!(edge_pan(canvas, pos2(300.0, 400.0), dt), vec2(-full, 0.0), "on the left edge");
        assert_eq!(edge_pan(canvas, pos2(1200.0 - AUTOPAN_BAND / 2.0, 400.0), dt), vec2(full / 2.0, 0.0), "halfway into the band");
        assert_eq!(edge_pan(canvas, pos2(1200.0 + AUTOPAN_BAND, 400.0), dt), vec2(2.0 * full, 0.0), "one band past the edge");
        assert_eq!(edge_pan(canvas, pos2(5000.0, 400.0), dt), vec2(2.0 * full, 0.0), "no faster beyond");
        assert_eq!(edge_pan(canvas, pos2(300.0, 40.0), dt), vec2(-full, -full), "a corner pans both ways");
        assert_eq!(edge_pan(canvas, pos2(700.0, 860.0), dt), vec2(0.0, full), "the bottom edge");
        assert_eq!(edge_pan(canvas, pos2(1200.0, 400.0), 0.0), Vec2::ZERO);
        assert_eq!(edge_pan(canvas, pos2(f32::NAN, 400.0), dt), Vec2::ZERO);
        assert_eq!(edge_pan(Rect::NOTHING, pos2(1200.0, 400.0), dt), Vec2::ZERO);
    }
}
