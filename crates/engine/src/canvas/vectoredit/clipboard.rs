//! Copy, cut and paste in vector edit mode (#1875). With points or segments selected, Copy puts
//! the selected part of each edited layer on the clipboard as a vector layer of its own (its
//! paints and stroke, just those paths), so a paste outside edit mode makes a new layer of them;
//! Cut copies them, then deletes them as Delete does (points selected alone are only deleted). Paste while editing adds the copied paths
//! (and any other shape's outline) to the edited layer where they were, selected.

use std::collections::BTreeMap;

use figcraft_doc::{Document, Node, NodeId, NodeKind, Region, Segment, VectorNetwork};
use figcraft_geom::{Affine, Point, Rect, Vec2};
use serde_json::{Value, json};

use super::{EditPart, delete_selection, editing, network, ops, points_of, refit_node, set_state};
use crate::{EngineError, Result, Session};

/// The selected part of each edited layer as a copy of that layer holding only those paths, in
/// page space (its transform is its world transform), keyed by the layer it came from. Points
/// selected alone, with no segment between them, carry nothing to paste and are left out.
fn selected_parts(doc: &Document, parts: &[EditPart]) -> Vec<(NodeId, Node)> {
    let mut out = Vec::new();
    for part in parts {
        let (Some(node), Some(net)) = (doc.node(part.id), network(doc, part.id)) else { continue };
        let points = points_of(net, &part.vertices, &part.segments);
        let piece = ops::part(net, &points);
        if piece.segments.is_empty() {
            continue;
        }
        let mut copy = node.clone();
        copy.kind = NodeKind::Vector { network: piece };
        copy.children.clear();
        copy.transform = doc.world_transform(part.id);
        refit_node(&mut copy);
        out.push((part.id, copy));
    }
    out
}

/// The page-space box of `node` (its own box under its transform).
fn page_box(node: &Node) -> Rect {
    let t = node.transform;
    let corners = [Point::ZERO, Point::new(node.width, 0.0), Point::new(node.width, node.height), Point::new(0.0, node.height)].map(|p| t * p);
    corners.iter().skip(1).fold(Rect::from_points(corners[0], corners[0]), |r, p| r.union_pt(*p))
}

/// `edit.copy` / `edit.cut` in vector edit mode with points or segments selected: the selected
/// paths go on the clipboard as vector layers ([`selected_parts`]); a cut then deletes them as
/// Delete does. `None` when nothing is being edited or nothing is selected there (the command
/// then copies the selected layers as usual).
pub fn copy_in_edit_mode(s: &mut Session, cut: bool) -> Option<Result<Value>> {
    let ve = editing(s).ok()?;
    let parts: Vec<EditPart> = ve.parts().into_iter().filter(|p| !p.vertices.is_empty() || !p.segments.is_empty()).collect();
    if parts.is_empty() {
        return None;
    }
    Some(copy_parts(s, parts, cut))
}

/// Put `parts` of the edited layers on the clipboard (and delete them for a cut).
fn copy_parts(s: &mut Session, parts: Vec<EditPart>, cut: bool) -> Result<Value> {
    let cmd = if cut { "edit.cut" } else { "edit.copy" };
    let ds = s.ds()?;
    let pieces = selected_parts(&ds.doc, &parts);
    if pieces.is_empty() {
        if cut {
            // Points alone carry no paths to copy: the cut still deletes them, as Delete does.
            delete_selection(s, parts, false)?;
            return Ok(json!({"copied": 0, "paths": true}));
        }
        return Err(EngineError::Disabled(cmd.into(), "select a segment, or two connected points, to copy paths".into()));
    }
    let mut images = BTreeMap::new();
    for (_, n) in &pieces {
        for p in n.fills.iter().chain(&n.strokes) {
            if let figcraft_color::PaintKind::Image(img) = &p.kind
                && let Some(d) = ds.doc.images.get(&img.image)
            {
                images.insert(img.image.clone(), d.clone());
            }
        }
    }
    let bounds = pieces.iter().map(|(_, n)| page_box(n)).reduce(|a, b| a.union(b));
    let (source, source_path, uid) = (ds.doc.clone(), ds.path.clone(), ds.uid);
    let clip = &mut s.clipboard;
    clip.source = Some(source);
    clip.source_path = source_path;
    clip.source_doc = uid;
    clip.copied_components.clear();
    clip.nodes = pieces.into_iter().map(|(id, n)| (id, vec![n])).collect();
    clip.images = images;
    clip.bounds = bounds;
    clip.layers_generation = clip.layers_generation.wrapping_add(1);
    let copied = clip.nodes.len();
    if cut {
        delete_selection(s, parts, false)?;
    }
    Ok(json!({"copied": copied, "paths": true}))
}

/// The paths of copied layer `n` (a vector's network, another shape's outline) in its own space;
/// `None` for layers without paths (text, images in frames, groups).
fn paths_of(doc: Option<&Document>, n: &Node) -> Option<VectorNetwork> {
    match &n.kind {
        NodeKind::Vector { network } => Some(network.clone()),
        k if k.is_shape() || matches!(k, NodeKind::BooleanOp { .. }) => {
            let (path, _) = figcraft_render::node_outline(doc?, n)?;
            let mut net = VectorNetwork::from_path(&path);
            if matches!(k, NodeKind::Line) {
                net.regions.clear();
            }
            Some(net)
        }
        _ => None,
    }
}

/// Map a relative handle through the linear part of `m`.
fn handle(m: Affine, t: (f64, f64)) -> (f64, f64) {
    let v = (m * Point::new(t.0, t.1)) - (m * Point::ZERO);
    let v: Vec2 = if v.x.is_finite() && v.y.is_finite() { v } else { Vec2::ZERO };
    (v.x, v.y)
}

/// Add `other` (in the space `m` maps into `net`'s) to `net`. Returns the indices of its points
/// in `net`.
fn append(net: &mut VectorNetwork, other: &VectorNetwork, m: Affine) -> Vec<usize> {
    let (v0, s0) = (net.vertices.len(), net.segments.len());
    for v in &other.vertices {
        let p = m * v.point();
        if !(p.x.is_finite() && p.y.is_finite()) {
            return Vec::new();
        }
    }
    net.vertices.extend(other.vertices.iter().map(|v| {
        let p = m * v.point();
        figcraft_doc::Vertex { x: p.x, y: p.y, ..*v }
    }));
    net.segments.extend(other.segments.iter().map(|sg| Segment { a: sg.a + v0, b: sg.b + v0, ta: handle(m, sg.ta), tb: handle(m, sg.tb) }));
    net.regions.extend(other.regions.iter().map(|r| {
        let mut r: Region = r.clone();
        for lp in &mut r.loops {
            for si in lp.iter_mut() {
                *si += s0;
            }
        }
        if let Some(frame) = &mut r.paint_space {
            frame.transform = m * frame.transform;
        }
        r
    }));
    (v0..net.vertices.len()).collect()
}

/// The most points a paste adds to an edited layer.
const MAX_PASTED_POINTS: usize = 1_000_000;

/// `edit.paste` in vector edit mode: the copied layers' paths (vectors' networks, other shapes'
/// outlines) join the active edited layer where they were on the page, and are selected. `None`
/// when nothing is being edited or the clipboard holds no paths (the command then pastes the
/// layers as usual).
pub fn paste_in_edit_mode(s: &mut Session) -> Option<Result<Value>> {
    let ve = editing(s).ok()?;
    let clip = &s.clipboard;
    let source = clip.source.clone();
    let mut pieces: Vec<(VectorNetwork, Affine)> = Vec::new();
    for (_, nodes) in &clip.nodes {
        // Each copied node's page transform: the root's is stored in page space, its layers' are
        // relative to their parents.
        let mut world: BTreeMap<NodeId, Affine> = BTreeMap::new();
        for (k, n) in nodes.iter().enumerate() {
            let parent = if k == 0 { Affine::IDENTITY } else { n.parent.and_then(|p| world.get(&p).copied()).unwrap_or(Affine::IDENTITY) };
            let w = parent * n.transform;
            world.insert(n.id, w);
            if let Some(net) = paths_of(source.as_deref().or_else(|| s.doc().ok()), n).filter(|net| !net.segments.is_empty()) {
                pieces.push((net, w));
            }
        }
    }
    if pieces.is_empty() {
        return None;
    }
    Some(paste_paths(s, ve.id, pieces))
}

/// Add `pieces` (networks with their page transforms) to edited layer `id` and select them.
fn paste_paths(s: &mut Session, id: NodeId, pieces: Vec<(VectorNetwork, Affine)>) -> Result<Value> {
    let doc = s.doc_mut()?;
    let to_local = doc.world_transform(id).inverse();
    let node = doc.get_mut(id)?;
    let NodeKind::Vector { network } = &mut node.kind else { return Err(EngineError::Other("not a vector".into())) };
    let mut added = Vec::new();
    for (piece, w) in &pieces {
        if network.vertices.len() + piece.vertices.len() > MAX_PASTED_POINTS {
            return Err(EngineError::Disabled("edit.paste".into(), "too many points to paste into this layer".into()));
        }
        added.extend(append(network, piece, to_local * *w));
    }
    refit_node(node);
    set_state(s, |ve| {
        ve.clear_selection();
        ve.vertices = added.clone();
    })?;
    Ok(json!({"ids": [id], "vertices": added}))
}
