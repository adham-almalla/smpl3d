//! The move/rotate triad shared by Move/Copy and occurrence moves: X/Y/Z arrows, XY/YZ/XZ plane
//! squares, X/Y/Z rotation rings and a centre ball for a free drag in the view plane. It is a
//! constant size on screen. A drag reports its change since the drag began (distances snap to
//! round steps at the current zoom, angles to 5°; hold Alt or Ctrl for a free drag), so the caller
//! adds it to the values it had when the drag started.

use std::cell::RefCell;

use egui::{Color32, Pos2, Rect, Sense, Shape, Stroke, vec2};
use smpl3d_engine::geom::{Ring, Vec3};

use crate::theme::Tokens;
use crate::viewport::Proj;

/// A part of the triad.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Handle {
    /// Arrow along X, Y or Z (0, 1, 2).
    Axis(usize),
    /// Square in the plane across that axis (Plane(2) is XY).
    Plane(usize),
    /// Ring turning about that axis.
    Ring(usize),
    /// Centre ball: free drag in the view plane.
    Ball,
}

/// What a drag has done so far.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Change {
    Translate(Vec3),
    /// About X, Y or Z through the centre, in radians.
    Rotate {
        axis: usize,
        angle: f64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drag {
    pub handle: Handle,
    pub change: Change,
    /// The first frame of the drag (take a snapshot of the values now).
    pub started: bool,
    /// The drag ended this frame.
    pub done: bool,
}

/// Where the triad is and which handles it shows.
#[derive(Clone, Copy, Debug)]
pub struct Triad {
    pub center: Vec3,
    pub translate: bool,
    pub rotate: bool,
}

#[derive(Clone, Copy, Debug)]
struct State {
    handle: Handle,
    center: Vec3,
    /// Grab point on the plane (planes, ball) or parameter along the axis.
    grab: Vec3,
    t0: f64,
    /// Ring: last angle seen and the angle turned so far.
    last: f64,
    turned: f64,
}

const DIRS: [Vec3; 3] = [Vec3::X, Vec3::Y, Vec3::Z];
/// Arrow length on screen (px).
pub const LEN_PX: f64 = 110.0;

/// How near a handle a press still takes it (px): the widest, the centre ball.
pub const HIT_TOLERANCE: f32 = 9.0;

thread_local! {
    /// The last triad drawn, laid out (for scenarios and tests).
    static LAST: RefCell<Option<Layout>> = const { RefCell::new(None) };
    /// The ends the arrows show (+1 or −1 per axis), for the triad at this centre.
    static SIGNS: RefCell<Option<(Vec3, [f64; 3])>> = const { RefCell::new(None) };
    /// The pass the triad was last drawn in.
    static SHOWN: std::cell::Cell<Option<u64>> = const { std::cell::Cell::new(None) };
}

/// Where the triad's handles are, in the model: so a world point can be named on each.
#[derive(Clone, Copy, Debug)]
struct Layout {
    center: Vec3,
    /// Each arrow's middle: its distance along its axis (mm).
    axis: [f64; 3],
    /// Each tile's middle: its distances along the plane's two directions (`ring_basis`).
    plane: [Option<(f64, f64)>; 3],
    /// Each rotate arrow: where on its ring (radians from `ring_basis`'s first direction) and the
    /// ring's radius (mm).
    ring: [Option<(f64, f64)>; 3],
}

/// A world point on a handle of the last triad drawn: "x"/"y"/"z" on an arrow, "xy"/"yz"/"xz"
/// on a tile, "rx"/"ry"/"rz" on a rotate arrow, "ball". `by` moves it; `turn` (degrees) turns a
/// ring point about its axis, 60 being the rotate arrow itself.
pub fn handle_point(name: &str, by: Vec3, turn: f64) -> Option<Vec3> {
    let lay = LAST.with(|x| *x.borrow())?;
    let c = lay.center;
    let axis = |s: &str| match s {
        "x" => Some(0),
        "y" => Some(1),
        "z" => Some(2),
        _ => None,
    };
    let plane = |i: usize| -> Option<Vec3> {
        let (a, b) = lay.plane[i]?;
        let (e1, e2) = ring_basis(i);
        Some(c + e1 * a + e2 * b)
    };
    let p = match name {
        "ball" => c,
        "xy" => plane(2)?,
        "yz" => plane(0)?,
        "xz" => plane(1)?,
        r if r.starts_with('r') => {
            let i = axis(&r[1..])?;
            let (th0, rad) = lay.ring[i]?;
            let (e1, e2) = ring_basis(i);
            let th = th0 + (turn - 60.0).to_radians();
            return Some(c + (e1 * th.cos() + e2 * th.sin()) * rad + by);
        }
        a => {
            let i = axis(a)?;
            c + DIRS[i] * lay.axis[i]
        }
    };
    Some(p + by)
}

/// Ring radius as a share of the arrow length.
const RING: f64 = 0.62;

/// In-plane directions of the ring about an axis (right-handed: e1 × e2 = axis).
fn ring_basis(i: usize) -> (Vec3, Vec3) {
    match i {
        0 => (Vec3::Y, Vec3::Z),
        1 => (Vec3::Z, Vec3::X),
        _ => (Vec3::X, Vec3::Y),
    }
}

/// Distance from `p` to the screen segment a–b.
fn seg_dist(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
    (a + ab * t).distance(p)
}

fn in_quad(p: Pos2, q: &Ring<Pos2>) -> bool {
    if q.len() < 3 {
        return false;
    }
    let mut sign = 0.0f32;
    for (&a, &b) in q.edges() {
        let c = (b - a).x * (p - a).y - (b - a).y * (p - a).x;
        if c.abs() < 1e-6 {
            continue;
        }
        if sign == 0.0 {
            sign = c.signum();
        } else if c.signum() != sign {
            return false;
        }
    }
    true
}

/// Where the view ray through `pos` meets the plane through `o` with normal `n`.
pub(crate) fn on_plane(proj: &Proj, pos: Pos2, o: Vec3, n: Vec3) -> Option<Vec3> {
    let (ro, rd) = proj.ray(pos);
    let den = rd.dot(n);
    if den.abs() < 1e-9 {
        return None;
    }
    Some(ro + rd * ((o - ro).dot(n) / den))
}

/// Parameter along the line `o + d t` closest to the view ray through `pos`.
fn along(proj: &Proj, pos: Pos2, o: Vec3, d: Vec3) -> Option<f64> {
    let (ro, r) = proj.ray(pos);
    let w0 = o - ro;
    let (b, dd, e) = (d.dot(r), d.dot(w0), r.dot(w0));
    let den = 1.0 - b * b;
    if den.abs() < 1e-6 {
        return None;
    }
    Some((b * e - dd) / den)
}

/// The triad on screen, styled as Shapr3D's handles: arrows along the axes, a tile between each
/// two arrows near the centre, and a curved rotate arrow further out between them.
struct Shapes {
    arrows: Vec<(usize, Pos2, Pos2)>,
    planes: Vec<(usize, Ring<Pos2>)>,
    /// Each rotate arrow, as the arc it is drawn along, and a longer faint arc behind it.
    rings: Vec<(usize, Vec<Pos2>)>,
    guides: Vec<Vec<Pos2>>,
    ball: Pos2,
    layout: Layout,
}

fn shapes(proj: &Proj, t: &Triad, l: f64) -> Option<Shapes> {
    let c = t.center;
    let ball = proj.to_screen(c)?;
    let mut layout = Layout { center: c, axis: [0.75 * l; 3], plane: [None; 3], ring: [None; 3] };
    let mut s = Shapes { arrows: Vec::new(), planes: Vec::new(), rings: Vec::new(), guides: Vec::new(), ball, layout };
    // Each axis on screen: its direction, and px per mm along it. One seen end-on is left out.
    // An axis pointing away from the viewer shows its other end, as Shapr3D's does, so the
    // three arrows spread round the centre; a drag along it works the same either way.
    // Which end each arrow shows is chosen once, when the triad appears at this centre, and kept
    // while the view turns: arrows that flipped as the view went round would swap places
    // under the pointer.
    let view = proj.ray(ball).1;
    let sign = SIGNS.with(|m| {
        let mut m = m.borrow_mut();
        match *m {
            Some((at, s)) if at.dist(c) <= l * 1e-6 => s,
            _ => {
                let s = DIRS.map(|d| if d.dot(view) > 1e-6 { -1.0 } else { 1.0 });
                *m = Some((c, s));
                s
            }
        }
    });
    // Every handle is fixed in the model, as the triad always was, so it turns with the view
    // and nothing slides about on screen: arrows along the axes, a tile in each plane between
    // two arrows, and a rotate arrow on each ring between the same two arrows.
    let lf = l;
    for (i, d) in DIRS.iter().enumerate() {
        if !t.translate {
            break;
        }
        let d = *d * sign[i];
        let (Some(a), Some(b)) = (proj.to_screen(c + d * (0.32 * lf)), proj.to_screen(c + d * lf)) else { continue };
        // Seen end-on: too short to grab or read.
        if a.distance(b) < 10.0 {
            continue;
        }
        s.arrows.push((i, a, b));
        layout.axis[i] = 0.66 * lf * sign[i];
    }
    for i in 0..3 {
        let (e1, e2) = ring_basis(i);
        let (s1, s2) = (sign[(i + 1) % 3], sign[(i + 2) % 3]);
        let (f1, f2) = (e1 * s1, e2 * s2);
        if t.translate {
            let q: Ring<Pos2> = [(0.28, 0.28), (0.42, 0.28), (0.42, 0.42), (0.28, 0.42)]
                .iter()
                .filter_map(|(a, b)| proj.to_screen(c + f1 * (a * lf) + f2 * (b * lf)))
                .collect();
            // A tile seen edge-on is left out.
            let area = q.len() == 4 && {
                let v: Vec<Pos2> = q.iter().copied().collect();
                ((v[2] - v[0]).x * (v[3] - v[1]).y - (v[2] - v[0]).y * (v[3] - v[1]).x).abs() > 40.0
            };
            if area {
                s.planes.push((i, q));
                layout.plane[i] = Some((0.35 * lf * s1, 0.35 * lf * s2));
            }
        }
        if t.rotate {
            // Between the two arrows: 45° round from the first, on a ring of fixed size.
            let th0 = s2.atan2(s1);
            let rad = RING * lf;
            let arc = |span: f64| -> Vec<Pos2> {
                (-8..=8).filter_map(|n| proj.to_screen(c + (e1 * (th0 + span * f64::from(n) / 8.0).cos() + e2 * (th0 + span * f64::from(n) / 8.0).sin()) * rad)).collect()
            };
            let handle = arc(0.26);
            // Seen edge-on, the arc is a line too short to use.
            let span = handle.first().zip(handle.last()).map_or(0.0, |(a, b)| a.distance(*b));
            if handle.len() == 17 && span > 14.0 {
                s.guides.push(arc(0.75));
                s.rings.push((i, handle));
                layout.ring[i] = Some((th0, rad));
            }
        }
    }
    s.layout = layout;
    Some(s)
}

fn hit(s: &Shapes, p: Pos2) -> Option<Handle> {
    if s.ball.distance(p) < HIT_TOLERANCE {
        return Some(Handle::Ball);
    }
    if let Some((i, ..)) = s.arrows.iter().find(|(_, a, b)| seg_dist(p, *a, *b) < 6.0) {
        return Some(Handle::Axis(*i));
    }
    if let Some((i, _)) = s.planes.iter().find(|(_, q)| in_quad(p, q)) {
        return Some(Handle::Plane(*i));
    }
    s.rings.iter().find(|(_, pts)| pts.windows(2).any(|w| seg_dist(p, w[0], w[1]) < 5.0)).map(|(i, _)| Handle::Ring(*i))
}

/// Millimetres per screen pixel at a point.
fn mm_per_px(proj: &Proj, c: Vec3) -> Option<f64> {
    let (_, view) = proj.ray(proj.to_screen(c)?);
    let u = view.any_perp().normalized()?;
    let (a, b) = (proj.to_screen(c)?, proj.to_screen(c + u)?);
    let px = f64::from(a.distance(b));
    (px > 1e-9).then(|| 1.0 / px)
}

/// Radius shared by the triad rings and the selected-axis Rotate handle.
pub(crate) fn ring_radius(proj: &Proj, centre: Vec3) -> Option<f64> {
    Some(mm_per_px(proj, centre)? * LEN_PX * RING)
}

fn snap(v: f64, step: f64, free: bool) -> f64 {
    if free || step <= 0.0 { v } else { (v / step).round() * step }
}

/// Discard a suspended pointer gesture without reporting a final model change.
pub(crate) fn cancel(ctx: &egui::Context, id: egui::Id) {
    ctx.data_mut(|d| d.remove::<State>(id));
}

/// Hit-test before registering the dynamic egui drag widget or changing the camera.
pub(crate) fn contains_pointer(proj: &Proj, t: &Triad, p: Pos2) -> bool {
    mm_per_px(proj, t.center).and_then(|scale| shapes(proj, t, scale * LEN_PX)).is_some_and(|sh| hit(&sh, p).is_some())
}

/// The region the triad's handles occupy on screen: where a press takes one of them. A value
/// box over it takes the press before the gizmo sees it (#6608).
pub fn handles_region(proj: &Proj, t: &Triad) -> Option<egui::Rect> {
    let l = mm_per_px(proj, t.center)? * LEN_PX;
    let sh = shapes(proj, t, l)?;
    let mut pts = vec![sh.ball];
    for (_, a, b) in &sh.arrows {
        pts.push(*a);
        pts.push(*b);
    }
    for (_, ring) in &sh.rings {
        pts.extend(ring.iter().copied());
    }
    for (_, q) in &sh.planes {
        for (&a, &b) in q.edges() {
            pts.push(a);
            pts.push(b);
        }
    }
    let mut min = sh.ball;
    let mut max = sh.ball;
    for p in pts {
        min.x = min.x.min(p.x);
        min.y = min.y.min(p.y);
        max.x = max.x.max(p.x);
        max.y = max.y.max(p.y);
    }
    Some(egui::Rect::from_min_max(min, max).expand(HIT_TOLERANCE))
}

/// Draw the triad and run a drag on it. `step` is the distance snap step.
pub fn show(ui: &mut egui::Ui, painter: &egui::Painter, proj: &Proj, id: egui::Id, t: &Triad, step: f64) -> Option<Drag> {
    let tk = Tokens::get();
    let l = mm_per_px(proj, t.center)? * LEN_PX;
    // A triad shown again after a gap (a new Move, another selection) picks its arrows' ends
    // afresh for the view it opens in.
    let pass = ui.ctx().cumulative_pass_nr();
    if SHOWN.with(|x| x.replace(Some(pass))).is_none_or(|last| pass > last + 2) {
        SIGNS.with(|m| *m.borrow_mut() = None);
    }
    let sh = shapes(proj, t, l)?;
    LAST.with(|x| *x.borrow_mut() = Some(sh.layout));
    // Handles by name for scenarios ("move_x", "move_rz", "move_ball"…).
    for n in ["x", "y", "z", "xy", "yz", "xz", "rx", "ry", "rz", "ball"] {
        if let Some(q) = handle_point(n, Vec3::ZERO, 60.0).and_then(|p| proj.to_screen(p)) {
            crate::scenario::publish_handle(&format!("move_{n}"), q);
        }
    }
    let state: Option<State> = ui.data(|d| d.get_temp(id));
    let pointer = ui.input(|i| i.pointer.hover_pos());
    let hovered = if state.is_none() { pointer.filter(|_| ui.rect_contains_pointer(proj.rect)).and_then(|p| hit(&sh, p)) } else { None };
    let active = state.map(|s| s.handle).or(hovered);
    let hot = |h: Handle| active == Some(h);
    // Smpl3D (as Shapr3D): white handles with a grey outline and a soft shadow; the one under
    // the pointer turns the accent. A thin line in each axis colour runs from the centre to its
    // arrow, so X, Y and Z can still be told apart.
    let fill = |h: Handle| if hot(h) { tk.accent } else { Color32::WHITE };
    let edge = Color32::from_gray(80);
    for g in &sh.guides {
        painter.add(Shape::line(g.clone(), Stroke::new(1.2, Color32::from_white_alpha(60))));
    }
    for (i, a, _) in &sh.arrows {
        painter.line_segment([sh.ball, *a], Stroke::new(1.5, tk.axes()[(*i).min(2)]));
    }
    for (i, pts) in &sh.rings {
        curved_arrow(painter, pts, fill(Handle::Ring(*i)), edge);
    }
    for (i, q) in &sh.planes {
        let tile: Vec<Pos2> = q.iter().copied().collect();
        let shadow: Vec<Pos2> = tile.iter().map(|p| *p + vec2(0.0, 1.5)).collect();
        painter.add(Shape::convex_polygon(shadow, Color32::from_black_alpha(60), Stroke::NONE));
        painter.add(Shape::convex_polygon(tile, fill(Handle::Plane(*i)), Stroke::new(1.5, edge)));
    }
    for (i, a, b) in &sh.arrows {
        fat_arrow(painter, *a, *b, fill(Handle::Axis(*i)), edge, false);
    }
    let bc = if hot(Handle::Ball) { tk.accent } else { Color32::WHITE };
    painter.circle_filled(sh.ball + vec2(0.0, 1.5), 7.0, Color32::from_black_alpha(60));
    painter.circle(sh.ball, 6.5, bc, Stroke::new(1.5, edge));
    let p = pointer?;
    active?;
    ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    let r = ui.interact(Rect::from_center_size(p, vec2(24.0, 24.0)), id.with("hit"), Sense::drag());
    // Free drag is driven by the per-frame thread-local set from prefs::manipulator_free_held.
    let free = crate::tool_modifiers::manipulator_free();
    if state.is_some() || r.dragged() || r.drag_started() {
        crate::tool_modifiers::set_manipulator_dragging(true);
    }
    let view = proj.ray(sh.ball).1;
    let plane_normal = |h: Handle| match h {
        Handle::Plane(i) | Handle::Ring(i) => DIRS[i],
        _ => view,
    };
    let mut started = false;
    let mut st = match state {
        Some(s) => s,
        None => {
            if !r.drag_started() {
                return None;
            }
            let h = hovered?;
            let from = ui.input(|i| i.pointer.press_origin()).unwrap_or(p);
            let c = t.center;
            let mut s = State { handle: h, center: c, grab: c, t0: 0.0, last: 0.0, turned: 0.0 };
            match h {
                Handle::Axis(i) => s.t0 = along(proj, from, c, DIRS[i])?,
                Handle::Ring(i) => {
                    let q = on_plane(proj, from, c, DIRS[i])? - c;
                    let (e1, e2) = ring_basis(i);
                    s.last = q.dot(e2).atan2(q.dot(e1));
                }
                _ => s.grab = on_plane(proj, from, c, plane_normal(h))?,
            }
            started = true;
            s
        }
    };
    let done = r.drag_stopped() || !r.dragged() && !started;
    // The frame's events can hold the release and further movement — the hand keeps moving as
    // the button goes up, or input queued during a busy frame — and `p` is already past the
    // release then. The release event itself carries where the button was let go (#8265): take
    // the final change from there, so the drag ends where the button was released, not where
    // the mouse has since got to.
    let p = if done {
        ui.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::PointerButton { pos, pressed: false, .. } => Some(*pos),
                _ => None,
            })
        })
        .unwrap_or(p)
    } else {
        p
    };
    let change = match st.handle {
        Handle::Axis(i) => {
            let v = along(proj, p, st.center, DIRS[i]).map(|v| v - st.t0).unwrap_or(0.0);
            Change::Translate(DIRS[i] * snap(v, step, free))
        }
        Handle::Ring(i) => {
            if let Some(q) = on_plane(proj, p, st.center, DIRS[i]).map(|q| q - st.center) {
                let (e1, e2) = ring_basis(i);
                let th = q.dot(e2).atan2(q.dot(e1));
                let mut d = th - st.last;
                while d > std::f64::consts::PI {
                    d -= std::f64::consts::TAU;
                }
                while d < -std::f64::consts::PI {
                    d += std::f64::consts::TAU;
                }
                st.turned += d;
                st.last = th;
            }
            Change::Rotate { axis: i, angle: snap(st.turned, 5f64.to_radians(), free) }
        }
        h => {
            let d = on_plane(proj, p, st.center, plane_normal(h)).map(|q| q - st.grab).unwrap_or(Vec3::ZERO);
            Change::Translate(Vec3::new(snap(d.x, step, free), snap(d.y, step, free), snap(d.z, step, free)))
        }
    };
    if done {
        ui.data_mut(|d| d.remove::<State>(id));
    } else {
        ui.data_mut(|d| d.insert_temp(id, st));
    }
    Some(Drag { handle: st.handle, change, started, done })
}

/// A curved two-way arrow along `pts`: a thick white band, outlined, with a head at each end.
fn curved_arrow(painter: &egui::Painter, pts: &[Pos2], fill: Color32, edge: Color32) {
    let (Some(&a0), Some(&a1), Some(&b1), Some(&b0)) = (pts.first(), pts.get(1), pts.get(pts.len().saturating_sub(2)), pts.last()) else { return };
    if pts.len() < 4 {
        return;
    }
    // The band stops short of the heads, which point along the arc's ends.
    let head = |tip: Pos2, from: Pos2| -> Option<[Pos2; 3]> {
        let u = (tip - from).normalized();
        if !u.x.is_finite() {
            return None;
        }
        let n = vec2(-u.y, u.x);
        let base = tip - u * 3.0;
        Some([tip + u * 8.0, base + n * 6.5, base - n * 6.5])
    };
    let band = &pts[1..pts.len() - 1];
    let shadow: Vec<Pos2> = band.iter().map(|p| *p + vec2(0.0, 1.5)).collect();
    painter.add(Shape::line(shadow, Stroke::new(7.5, Color32::from_black_alpha(50))));
    painter.add(Shape::line(band.to_vec(), Stroke::new(7.0, edge)));
    for h in [head(a0, a1), head(b0, b1)].into_iter().flatten() {
        painter.add(Shape::convex_polygon(h.to_vec(), fill, Stroke::new(1.3, edge)));
    }
    painter.add(Shape::line(band.to_vec(), Stroke::new(4.4, fill)));
}

/// A chunky arrow from `a` to `b` (a head at both ends when `both`), filled and outlined, with
/// a soft shadow: Shapr3D's handles.
fn fat_arrow(painter: &egui::Painter, a: Pos2, b: Pos2, fill: Color32, edge: Color32, both: bool) {
    let v = b - a;
    let len = v.length();
    if len < 4.0 {
        return;
    }
    let u = v / len;
    let n = vec2(-u.y, u.x);
    let head = 11.0f32.min(len * 0.45);
    let (w, hw) = (2.6, 6.5);
    let tail = if both { a + u * head } else { a };
    let neck = b - u * head;
    let mut outline = Vec::new();
    if both {
        outline.extend([a, tail + n * hw, tail + n * w]);
    } else {
        outline.extend([a + n * w]);
    }
    outline.extend([neck + n * w, neck + n * hw, b, neck - n * hw, neck - n * w]);
    if both {
        outline.extend([tail - n * w, tail - n * hw]);
    } else {
        outline.push(a - n * w);
    }
    let shadow: Vec<Pos2> = outline.iter().map(|p| *p + vec2(0.0, 1.5)).collect();
    // Not convex: drawn as a closed path over a filled shaft and heads.
    painter.add(Shape::closed_line(shadow, Stroke::new(3.0, Color32::from_black_alpha(50))));
    painter.add(Shape::convex_polygon(vec![tail + n * w, neck + n * w, neck - n * w, tail - n * w], fill, Stroke::NONE));
    painter.add(Shape::convex_polygon(vec![neck + n * hw, b, neck - n * hw], fill, Stroke::NONE));
    if both {
        painter.add(Shape::convex_polygon(vec![tail - n * hw, a, tail + n * hw], fill, Stroke::NONE));
    }
    painter.add(Shape::closed_line(outline, Stroke::new(1.3, edge)));
}

/// A 3×3 rotation (rows) about a unit axis.
pub fn rotation(axis: Vec3, angle: f64) -> [[f64; 3]; 3] {
    let (c, s) = (angle.cos(), angle.sin());
    let t = 1.0 - c;
    let (x, y, z) = (axis.x, axis.y, axis.z);
    [
        [t * x * x + c, t * x * y - s * z, t * x * z + s * y],
        [t * x * y + s * z, t * y * y + c, t * y * z - s * x],
        [t * x * z - s * y, t * y * z + s * x, t * z * z + c],
    ]
}

pub fn mul(a: &[[f64; 3]; 3], b: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let mut m = [[0.0; 3]; 3];
    for (i, row) in m.iter_mut().enumerate() {
        for (j, v) in row.iter_mut().enumerate() {
            *v = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    m
}

pub fn apply(m: &[[f64; 3]; 3], p: Vec3) -> Vec3 {
    Vec3::new(
        m[0][0] * p.x + m[0][1] * p.y + m[0][2] * p.z,
        m[1][0] * p.x + m[1][1] * p.y + m[1][2] * p.z,
        m[2][0] * p.x + m[2][1] * p.y + m[2][2] * p.z,
    )
}

/// The axis and angle of a rotation (angle 0 for none).
pub fn axis_angle(m: &[[f64; 3]; 3]) -> (Vec3, f64) {
    let tr = m[0][0] + m[1][1] + m[2][2];
    let angle = ((tr - 1.0) / 2.0).clamp(-1.0, 1.0).acos();
    if angle < 1e-12 {
        return (Vec3::Z, 0.0);
    }
    let v = Vec3::new(m[2][1] - m[1][2], m[0][2] - m[2][0], m[1][0] - m[0][1]);
    if let Some(a) = v.normalized().filter(|_| v.len() > 1e-6) {
        return (a, angle);
    }
    // Half a turn: the axis from the diagonal.
    let d = [((m[0][0] + 1.0) / 2.0).max(0.0).sqrt(), ((m[1][1] + 1.0) / 2.0).max(0.0).sqrt(), ((m[2][2] + 1.0) / 2.0).max(0.0).sqrt()];
    let (x, mut y, mut z) = (d[0], d[1], d[2]);
    if m[0][1] + m[1][0] < 0.0 && x > 1e-6 {
        y = -y;
    }
    if m[0][2] + m[2][0] < 0.0 && x > 1e-6 {
        z = -z;
    }
    if x <= 1e-6 && m[1][2] + m[2][1] < 0.0 {
        z = -z;
    }
    (Vec3::new(x, y, z).normalized().unwrap_or(Vec3::Z), angle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plane_squares_stay_within_the_triad() {
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0));
        let t = Triad { center: Vec3::ZERO, translate: true, rotate: false };
        // The squares reach 0.45 √2 of the arrow length from the centre, plus stroke and feathering.
        let limit = (0.45 * 2f64.sqrt() * LEN_PX) as f32 + 3.0;
        let mut worst = 0.0f32;
        for dir in crate::viewport::orbit_directions() {
            let cam = smpl3d_engine::render::Camera::default().looking_from(dir);
            let proj = Proj { cam, vp: cam.view_proj(4.0 / 3.0, 100.0), rect };
            let l = mm_per_px(&proj, t.center).unwrap() * LEN_PX;
            let sh = shapes(&proj, &t, l).unwrap();
            for (_, q) in &sh.planes {
                for p in crate::viewport::drawn_points(crate::viewport::projected_polygon(q, Color32::GRAY, Stroke::new(1.0, Color32::BLACK))) {
                    worst = worst.max(p.distance(sh.ball));
                }
            }
        }
        assert!(worst <= limit, "a vertex {worst} px from the centre (limit {limit})");
    }

    #[test]
    fn rotations_compose_to_one_axis_angle() {
        let r = mul(&rotation(Vec3::Z, 0.7), &rotation(Vec3::X, 0.4));
        let (a, ang) = axis_angle(&r);
        let back = rotation(a, ang);
        for i in 0..3 {
            for j in 0..3 {
                assert!((back[i][j] - r[i][j]).abs() < 1e-9);
            }
        }
        let h = rotation(Vec3::Y, std::f64::consts::PI);
        let (a, ang) = axis_angle(&h);
        assert!((ang - std::f64::consts::PI).abs() < 1e-9 && (a.y.abs() - 1.0).abs() < 1e-9);
        assert!((apply(&rotation(Vec3::Z, std::f64::consts::FRAC_PI_2), Vec3::X) - Vec3::Y).len() < 1e-12);
    }
}
