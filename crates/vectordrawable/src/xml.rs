//! The pretty-printed XML: elements two spaces deeper per level, one attribute per line four
//! spaces past its element (as Android Studio formats drawables). Numbers are compact (at most
//! three decimals for path coordinates, no `-0`) and always finite; text is escaped for attribute
//! values. Dimensions and group transforms retain precision so small drawables don't collapse.

use std::fmt::Write as _;

use figcraft_color::Color;
use figcraft_geom::BezPath;
use figcraft_geom::kurbo::PathEl;

use crate::model::{Fill, GradKind, GroupItem, Item, ItemKind, NativeStroke, PathItem};

const ANDROID: &str = "http://schemas.android.com/apk/res/android";
const AAPT: &str = "http://schemas.android.com/aapt";

/// A number as Android reads it: rounded to 3 decimals, no trailing zeros, `0` for anything
/// non-finite (the walk leaves out shapes whose numbers aren't finite).
pub(crate) fn num(v: f64) -> String {
    if !v.is_finite() {
        return "0".into();
    }
    let r = (v * 1000.0).round() / 1000.0;
    let r = if !r.is_finite() { v } else { r };
    precise(r)
}

/// Dimensions and transforms must not lose small but significant values through rounding.
fn precise(v: f64) -> String {
    let s = if v.is_finite() { v.to_string() } else { "0".into() };
    if s == "-0" { "0".into() } else { s }
}

/// Characters permitted in an XML 1.0 attribute, before escaping.
pub(crate) fn xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r') || (c >= '\u{20}' && c != '\u{FFFE}' && c != '\u{FFFF}')
}

/// `s` for an attribute value: markup escaped, whitespace that parsers would fold kept as
/// character references, and characters XML 1.0 forbids dropped.
pub(crate) fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' => out.push_str("&#9;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            c if !xml_char(c) => {}
            c => out.push(c),
        }
    }
    out
}

/// Layer names are literal Android strings as well as XML attributes: resource/theme prefixes
/// and backslashes must survive aapt's string processing before `android:name` reaches Android.
fn name(s: &str) -> String {
    let mut literal = String::with_capacity(s.len());
    for (i, c) in s.chars().enumerate() {
        if c == '\\' || (i == 0 && matches!(c, '@' | '?')) {
            literal.push('\\');
        }
        literal.push(c);
    }
    esc(&literal)
}

/// `#RRGGBB`, or `#AARRGGBB` when not opaque.
pub(crate) fn color(c: Color) -> String {
    let [r, g, b, a] = c.sanitized().to_rgba8();
    if a == 255 { format!("#{r:02X}{g:02X}{b:02X}") } else { format!("#{a:02X}{r:02X}{g:02X}{b:02X}") }
}

/// SVG path syntax, absolute commands.
pub(crate) fn path_data(p: &BezPath) -> String {
    let mut s = String::new();
    let pt = |x: f64, y: f64| format!("{},{}", num(x), num(y));
    for el in p.elements() {
        if !s.is_empty() {
            s.push(' ');
        }
        match *el {
            PathEl::MoveTo(a) => {
                let _ = write!(s, "M{}", pt(a.x, a.y));
            }
            PathEl::LineTo(a) => {
                let _ = write!(s, "L{}", pt(a.x, a.y));
            }
            PathEl::QuadTo(a, b) => {
                let _ = write!(s, "Q{} {}", pt(a.x, a.y), pt(b.x, b.y));
            }
            PathEl::CurveTo(a, b, c) => {
                let _ = write!(s, "C{} {} {}", pt(a.x, a.y), pt(b.x, b.y), pt(c.x, c.y));
            }
            PathEl::ClosePath => s.push('Z'),
        }
    }
    s
}

struct Out {
    s: String,
}

impl Out {
    /// `<tag` and its attributes, one per line; `close` ends it with `/>`, else `>`.
    fn open(&mut self, depth: usize, tag: &str, attrs: &[(&str, String)], close: bool) {
        let pad = "  ".repeat(depth);
        let _ = write!(self.s, "{pad}<{tag}");
        for (i, (k, v)) in attrs.iter().enumerate() {
            if i == 0 && attrs.len() == 1 {
                let _ = write!(self.s, " {k}=\"{v}\"");
            } else {
                let _ = write!(self.s, "\n{pad}    {k}=\"{v}\"");
            }
        }
        self.s.push_str(if close { " />\n" } else { ">\n" });
    }

    fn close(&mut self, depth: usize, tag: &str) {
        let _ = writeln!(self.s, "{}</{tag}>", "  ".repeat(depth));
    }
}

fn uses_gradient(items: &[Item]) -> bool {
    items.iter().any(|it| match &it.kind {
        ItemKind::Path(p) => matches!(p.fill, Some(Fill::Gradient(_))),
        ItemKind::Group(g) => uses_gradient(&g.children),
    })
}

/// The whole file: a `width`×`height` dp drawable with a viewport of the same size.
pub(crate) fn document(width: f64, height: f64, items: &[Item]) -> String {
    let mut o = Out { s: String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n") };
    let mut attrs = vec![("xmlns:android", ANDROID.to_string())];
    if uses_gradient(items) {
        attrs.push(("xmlns:aapt", AAPT.to_string()));
    }
    attrs.extend([
        ("android:width", format!("{}dp", precise(width))),
        ("android:height", format!("{}dp", precise(height))),
        ("android:viewportWidth", precise(width)),
        ("android:viewportHeight", precise(height)),
    ]);
    o.open(0, "vector", &attrs, false);
    if items.is_empty() {
        // Android refuses a <vector> without a <path> ("no path defined"): one empty, unfilled path.
        o.open(1, "path", &[("android:pathData", "M0,0".to_string())], true);
    }
    for it in items {
        item(&mut o, 1, it);
    }
    o.close(0, "vector");
    o.s
}

fn item(o: &mut Out, depth: usize, it: &Item) {
    match &it.kind {
        ItemKind::Path(p) => path(o, depth, p),
        ItemKind::Group(g) => group(o, depth, g),
    }
}

fn group(o: &mut Out, depth: usize, g: &GroupItem) {
    let mut attrs = Vec::new();
    if let Some(n) = &g.name {
        attrs.push(("android:name", name(n)));
    }
    if let Some(x) = &g.xf {
        let (sx, sy) = x.scale;
        if x.rotation != 0.0 {
            attrs.push(("android:rotation", precise(x.rotation)));
        }
        if sx != 1.0 {
            attrs.push(("android:scaleX", precise(sx)));
        }
        if sy != 1.0 {
            attrs.push(("android:scaleY", precise(sy)));
        }
        if x.translate.0 != 0.0 {
            attrs.push(("android:translateX", precise(x.translate.0)));
        }
        if x.translate.1 != 0.0 {
            attrs.push(("android:translateY", precise(x.translate.1)));
        }
    }
    if g.children.is_empty() && g.clip.is_none() {
        o.open(depth, "group", &attrs, true);
        return;
    }
    o.open(depth, "group", &attrs, false);
    if let Some(c) = &g.clip {
        o.open(depth + 1, "clip-path", &[("android:pathData", path_data(c))], true);
    }
    for c in &g.children {
        item(o, depth + 1, c);
    }
    o.close(depth, "group");
}

fn stroke_attrs(attrs: &mut Vec<(&str, String)>, s: &NativeStroke) {
    attrs.push(("android:strokeColor", color(s.color)));
    attrs.push(("android:strokeWidth", num(s.width)));
    if s.cap != "butt" {
        attrs.push(("android:strokeLineCap", s.cap.to_string()));
    }
    if s.join != "miter" {
        attrs.push(("android:strokeLineJoin", s.join.to_string()));
    } else if num(s.miter) != "4" {
        attrs.push(("android:strokeMiterLimit", num(s.miter)));
    }
}

fn path(o: &mut Out, depth: usize, p: &PathItem) {
    let mut attrs = Vec::new();
    if let Some(n) = &p.name {
        attrs.push(("android:name", name(n)));
    }
    attrs.push(("android:pathData", path_data(&p.data)));
    if let Some(Fill::Solid(c)) = &p.fill {
        attrs.push(("android:fillColor", color(*c)));
    }
    if let Some(s) = &p.stroke {
        stroke_attrs(&mut attrs, s);
    }
    if p.even_odd && p.fill.is_some() {
        attrs.push(("android:fillType", "evenOdd".to_string()));
    }
    let Some(Fill::Gradient(g)) = &p.fill else {
        o.open(depth, "path", &attrs, true);
        return;
    };
    o.open(depth, "path", &attrs, false);
    o.open(depth + 1, "aapt:attr", &[("name", "android:fillColor".to_string())], false);
    let mut ga = Vec::new();
    match g.kind {
        GradKind::Linear { start, end } => {
            ga.push(("android:type", "linear".to_string()));
            ga.push(("android:startX", num(start.0)));
            ga.push(("android:startY", num(start.1)));
            ga.push(("android:endX", num(end.0)));
            ga.push(("android:endY", num(end.1)));
        }
        GradKind::Radial { center, radius } => {
            ga.push(("android:type", "radial".to_string()));
            ga.push(("android:centerX", num(center.0)));
            ga.push(("android:centerY", num(center.1)));
            ga.push(("android:gradientRadius", num(radius)));
        }
        GradKind::Sweep { center } => {
            ga.push(("android:type", "sweep".to_string()));
            ga.push(("android:centerX", num(center.0)));
            ga.push(("android:centerY", num(center.1)));
        }
    }
    o.open(depth + 2, "gradient", &ga, false);
    for (off, c) in &g.stops {
        o.open(depth + 3, "item", &[("android:offset", num(*off)), ("android:color", color(*c))], true);
    }
    o.close(depth + 2, "gradient");
    o.close(depth + 1, "aapt:attr");
    o.close(depth, "path");
}
