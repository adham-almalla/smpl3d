//! Smpl3D: the Shapr3D-style tools palette that replaces the ribbon (Preferences › Layout ›
//! "Shapr3D tools palette"), and sketch tools that pick their plane first, so Line, Rectangle,
//! Circle… start from nothing open, as they do in Shapr3D.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::OnceLock;

use egui::{Align2, Color32, Rect, Sense, Stroke, pos2, vec2};
use smpl3d_engine::CommandSpec;
use smpl3d_engine::geom::Vec3;

use crate::SolveApp;
use crate::theme::Tokens;

/// Sketch tools that draw: started with no sketch open, they open the plane pick first.
const DRAWING_TOOLS: &[&str] = &[
    "sketch.line",
    "sketch.rectangle",
    "sketch.circle",
    "sketch.arc",
    "sketch.polygon",
    "sketch.spline.fit_point",
    "sketch.spline.control_point",
    "sketch.slot",
    "sketch.point",
    "sketch.ellipse",
    "sketch.text",
];

thread_local! {
    /// The reference images the design had last frame.
    static CANVASES: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
    /// A tool the Move popup switches to, with the bodies it moved.
    static SWITCH: RefCell<Option<(&'static str, Vec<smpl3d_engine::Sel>)>> = const { RefCell::new(None) };
    /// The drawing tool waiting for its sketch, and the frames it has waited without a dialog.
    static PENDING: RefCell<Option<(String, u32)>> = const { RefCell::new(None) };
}

/// Whether `id` should first pick a plane: a drawing tool with no sketch being edited.
pub fn needs_plane(app: &SolveApp, id: &str) -> bool {
    app.session.active_sketch.is_none() && DRAWING_TOOLS.iter().any(|t| id == *t || id.starts_with(&format!("{t}.")))
}

/// Remember `id` to start once the sketch it is waiting for is open.
pub fn remember(id: &str) {
    PENDING.with(|p| *p.borrow_mut() = Some((id.to_string(), 0)));
}

/// Every frame: start the remembered tool once its sketch is open; forget it if the plane pick
/// was cancelled.
pub fn tick(app: &mut SolveApp) {
    calibrate_new_image(app);
    if let Some((cmd, bodies)) = SWITCH.with(|s| s.borrow_mut().take()) {
        app.dialog = None;
        app.start(cmd);
        if let Some(d) = app.dialog.as_mut()
            && let Some(i) = d.inputs.first_mut()
        {
            i.items = bodies;
            // Align goes on to "From": the point on the body to line up.
            if cmd == "solid.align" {
                d.active = 1.min(d.inputs.len() - 1);
            }
        }
    }
    let Some((id, waited)) = PENDING.with(|p| p.borrow().clone()) else { return };
    if app.session.active_sketch.is_some() && app.dialog.is_none() && app.tool.is_none() {
        PENDING.with(|p| *p.borrow_mut() = None);
        app.start(&id);
    } else if app.dialog.is_none() && app.session.active_sketch.is_none() {
        // A few frames for the plane pick to open; after that, no dialog means it was cancelled.
        if waited > 5 {
            PENDING.with(|p| *p.borrow_mut() = None);
        } else {
            PENDING.with(|p| *p.borrow_mut() = Some((id, waited + 1)));
        }
    }
}

fn specs() -> &'static HashMap<&'static str, &'static CommandSpec> {
    static SPECS: OnceLock<HashMap<&'static str, &'static CommandSpec>> = OnceLock::new();
    SPECS.get_or_init(|| smpl3d_engine::command_specs().into_iter().map(|c| (c.id, c)).collect())
}

/// Tool groups, in Shapr3D's order. Sketch tools come first while a sketch is open.
const SKETCH: (&str, &[&str]) = (
    "Sketch",
    &[
        "sketch.create",
        "sketch.line",
        "sketch.rectangle.two_point",
        "sketch.circle.center",
        "sketch.arc.three_point",
        "sketch.polygon.inscribed",
        "sketch.spline.fit_point",
        "sketch.offset",
        "sketch.trim",
        "sketch.project",
        "sketch.dimension",
        "sketch.export_dxf",
    ],
);
const MODEL: (&str, &[&str]) = (
    "Model",
    &["solid.extrude", "solid.revolve", "solid.sweep", "solid.loft", "solid.fillet", "solid.chamfer", "solid.shell", "solid.press_pull"],
);
const TRANSFORM: (&str, &[&str]) = ("Transform", &["solid.move", "solid.scale", "solid.mirror", "solid.align", "solid.pattern.rectangular"]);
const COMBINE: (&str, &[&str]) = ("Combine", &["solid.combine", "solid.split_body", "solid.replace_face"]);
const BODIES: (&str, &[&str]) = ("Bodies", &["solid.box", "solid.cylinder", "solid.sphere"]);
const INSPECT: (&str, &[&str]) = ("Insert", &["canvas.insert", "sketch.insert_dxf"]);

const BUTTON: f32 = 40.0;
/// Columns of tool buttons: two, or three when two would leave the buttons too small to read.
const COLUMNS: usize = 2;
const WIDE_COLUMNS: usize = 3;
/// Below this size a third column is used instead of shrinking the buttons further.
const SMALL_BUTTON: f32 = 34.0;
const PAD: f32 = 8.0;

/// The palette, on the right edge of `area` below the view cube.
pub fn palette(app: &mut SolveApp, ui: &mut egui::Ui, area: Rect) {
    let t = Tokens::get();
    let sketching = app.session.active_sketch.is_some();
    let groups: Vec<(&str, &[&str])> =
        if sketching { vec![SKETCH, MODEL, TRANSFORM, COMBINE] } else { vec![SKETCH, MODEL, TRANSFORM, COMBINE, BODIES, INSPECT] };
    let groups: Vec<(&str, Vec<&'static CommandSpec>)> = groups
        .into_iter()
        .map(|(title, ids)| (title, ids.iter().filter_map(|id| specs().get(id).copied()).collect::<Vec<_>>()))
        .filter(|(_, specs)| !specs.is_empty())
        .collect();
    let running: Option<String> = match (&app.tool, &app.dialog) {
        (Some(tool), _) => Some(tool.cmd.to_string()),
        (None, Some(d)) if matches!(d.kind, crate::dialogs::Kind::Sketch) => Some("sketch.create".to_string()),
        (None, Some(_)) => app.last_command.as_ref().map(|(id, _)| id.clone()),
        _ => None,
    };
    let heading = t.font_micro + 12.0;
    // Below the view cube, and below the command window when one is open over it.
    let mut top = area.top() + 150.0;
    if app.dialog.is_some()
        && let Some(d) = app.viewport.dialog_rect
        && d.right() > area.right() - 140.0
    {
        top = top.max(d.bottom() + 8.0);
    }
    // Clear of the measurement box at the bottom right (Quick Measure).
    let room = (area.bottom() - top - 100.0).max(120.0);
    // Shrink the buttons rather than scroll when the window is short.
    let fixed = PAD * 2.0 + groups.len() as f32 * heading;
    let fit = |columns: usize| {
        let rows: f32 = groups.iter().map(|(_, g)| g.len().div_ceil(columns) as f32).sum();
        (rows, ((room - fixed) / rows - 2.0).clamp(22.0, BUTTON))
    };
    let (columns, (rows, button)) = match fit(COLUMNS) {
        (_, size) if size < SMALL_BUTTON => (WIDE_COLUMNS, fit(WIDE_COLUMNS)),
        two => (COLUMNS, two),
    };
    let width = PAD * 2.0 + button * columns as f32 + 2.0 * (columns as f32 - 1.0);
    let height = fixed + rows * (button + 2.0);
    let rect = Rect::from_min_size(pos2(area.right() - width - 12.0, top), vec2(width, height));
    let mut start: Option<&'static str> = None;
    egui::Area::new(egui::Id::new("smpl3d_palette"))
        .order(egui::Order::Middle)
        .fixed_pos(rect.min)
        .show(ui.ctx(), |ui| {
            ui.allocate_rect(rect, Sense::hover());
            ui.painter().rect_filled(rect, t.radius_xl, t.toolbar);
            ui.painter().rect_stroke(rect, t.radius_xl, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
            let mut y = rect.top() + PAD;
            for (title, items) in &groups {
                ui.painter().text(
                    pos2(rect.left() + PAD + 2.0, y + heading / 2.0),
                    Align2::LEFT_CENTER,
                    *title,
                    crate::fonts::medium(t.font_micro),
                    t.text_dim,
                );
                y += heading;
                for (i, spec) in items.iter().enumerate() {
                    let col = (i % columns) as f32;
                    let row = (i / columns) as f32;
                    let b = Rect::from_min_size(pos2(rect.left() + PAD + col * (button + 2.0), y + row * (button + 2.0)), vec2(button, button));
                    let enabled = (spec.enabled)(&app.session).is_ok() || needs_plane(app, spec.id) || (spec.id == "sketch.offset" && offset_enabled(app));
                    let resp = ui.interact(b, egui::Id::new(("smpl3d_tool", spec.id)), if enabled { Sense::click() } else { Sense::hover() });
                    let active = running.as_deref() == Some(spec.id);
                    let fill = if active {
                        t.accent
                    } else if resp.hovered() && enabled {
                        t.text.gamma_multiply(0.12)
                    } else {
                        Color32::TRANSPARENT
                    };
                    ui.painter().rect_filled(b, t.radius_md, fill);
                    crate::icons::paint_icon(ui.painter(), b.shrink(button * 0.22), spec.icon, if enabled { 1.0 } else { 0.35 });
                    let key = spec.shortcut.map(|k| format!("   {}", crate::keymap::native(k))).unwrap_or_default();
                    let label = crate::i18n::tr_label(spec.label);
                    let resp = resp.on_hover_text(format!("{label}{key}"));
                    if resp.clicked() {
                        start = Some(spec.id);
                    }
                }
                y += items.len().div_ceil(columns) as f32 * (button + 2.0);
            }
        });
    if let Some(id) = start {
        // A second click on the running tool puts it away, as in Shapr3D.
        if running.as_deref() == Some(id) && app.tool.is_some() {
            app.tool = None;
        } else {
            app.start(id);
        }
    }
}

/// Offset, as in Shapr3D: one tool for whatever is picked. Picked faces move (Offset Face,
/// curved or flat); sketch lines, a sketch area or body edges get new lines a distance away, in
/// their sketch or, for edges, in a new sketch on the flat face they lie on. With nothing
/// picked it opens Offset Face to pick faces, or says what to pick when there are none.
pub fn start_offset(app: &mut SolveApp) {
    use smpl3d_engine::Sel;
    if app.session.active_sketch.is_some() {
        app.start("sketch.offset");
        return;
    }
    let sel = app.session.selection.clone();
    if sel.iter().any(|s| matches!(s, Sel::Face { .. })) {
        app.start("solid.offset_face");
        return;
    }
    // Lines or areas of a closed sketch: open it and offset them there.
    let (sketch, curves) = picked_sketch_curves(app, &sel);
    if let Some(k) = sketch
        && !curves.is_empty()
    {
        app.edit_sketch(k);
        if app.session.active_sketch != Some(k) {
            return;
        }
        app.session.selection = curves.into_iter().map(|id| Sel::SketchCurve { id, sketch: Some(k) }).collect();
        app.start("sketch.offset");
        return;
    }
    let edges: Vec<(String, Vec3)> = sel
        .iter()
        .filter_map(|s| match s {
            Sel::Edge { body, point, .. } => Some((body.clone(), *point)),
            _ => None,
        })
        .collect();
    if !edges.is_empty() {
        offset_edges(app, &edges);
        return;
    }
    let st = app.session.world_state();
    let (bodies, sketches) = (!st.bodies.is_empty(), !st.sketches.is_empty());
    drop(st);
    if bodies {
        app.start("solid.offset_face");
        app.set_status("Offset: pick the faces to move, or pick sketch lines or edges first", false);
    } else if sketches {
        app.set_status("Offset: pick the sketch lines or area to offset first", false);
    } else {
        app.set_status("Offset: draw a sketch or a body first", false);
    }
}

/// Whether Offset has anything to work on: a sketch open, or a body or sketch in the design.
pub fn offset_enabled(app: &SolveApp) -> bool {
    if app.session.active_sketch.is_some() {
        return true;
    }
    let st = app.session.world_state();
    !st.bodies.is_empty() || !st.sketches.is_empty()
}

/// The picked lines and areas of one closed sketch (the first one picked), as curve ids.
fn picked_sketch_curves(app: &SolveApp, sel: &[smpl3d_engine::Sel]) -> (Option<u64>, Vec<String>) {
    use smpl3d_engine::Sel;
    let st = app.session.world_state();
    let mut sketch: Option<u64> = None;
    let mut curves: Vec<String> = Vec::new();
    for s in sel {
        let (k, ids) = match s {
            Sel::Profile { sketch: k, index } => {
                let ids = st.sketch(*k).and_then(|ss| ss.profiles.get(*index)).map(|p| smpl3d_engine::sketch::distinct_curves(&p.outer_curves)).unwrap_or_default();
                (*k, ids)
            }
            Sel::SketchCurve { id, sketch: Some(k) } => (*k, vec![id.clone()]),
            _ => continue,
        };
        if *sketch.get_or_insert(k) != k {
            continue;
        }
        for id in ids {
            if !curves.contains(&id) {
                curves.push(id);
            }
        }
    }
    (sketch, curves)
}

/// Offset body edges: a new sketch on the flat face they all lie on (the one facing the view
/// when two do), the edges brought into it, and Offset started on them.
fn offset_edges(app: &mut SolveApp, edges: &[(String, Vec3)]) {
    use smpl3d_engine::Sel;
    let Some((body, index, at)) = edges_face(app, edges) else {
        app.set_status("Offset: pick edges that lie on one flat face", true);
        return;
    };
    let before = app.settled_cam();
    if app.run("sketch.create", serde_json::json!({"plane": {"face": [at.x, at.y, at.z], "body": body, "index": index}, "project_edges": false})).is_err() {
        return;
    }
    let refs: Vec<serde_json::Value> = edges.iter().map(|(b, p)| serde_json::json!({"edge": [p.x, p.y, p.z], "body": b})).collect();
    let Ok(reply) = app.run("sketch.project", serde_json::json!({"refs": refs})) else { return };
    let Some(k) = app.session.active_sketch else { return };
    let ids: Vec<String> =
        reply["created"]["curves"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default();
    if ids.is_empty() {
        app.set_status("Offset: those edges couldn't be brought into a sketch", true);
        return;
    }
    app.look_at_opened_sketch(before);
    app.session.selection = ids.into_iter().map(|id| Sel::SketchCurve { id, sketch: Some(k) }).collect();
    app.start("sketch.offset");
}

/// A point on a flat face of one body that every edge lies on, preferring the face toward the
/// viewer.
fn edges_face(app: &SolveApp, edges: &[(String, Vec3)]) -> Option<(String, usize, Vec3)> {
    let body = &edges.first()?.0;
    if edges.iter().any(|(b, _)| b != body) {
        return None;
    }
    let st = app.session.world_state();
    let b = st.body(body)?;
    let tol = (b.body.size() * 1e-3).max(1e-3);
    let faces = b.body.faces(tol).ok()?;
    let mesh = b.mesh();
    let eye = app.cam.eye();
    let mut best: Option<(f64, usize, Vec3)> = None;
    for f in &faces {
        let Some(n) = f.plane_normal else { continue };
        if !edges.iter().all(|(_, p)| (*p - f.centroid).dot(n).abs() <= tol * 10.0) {
            continue;
        }
        // The edges have to be on this face, not only on its plane.
        let near = |p: Vec3| {
            mesh.triangles.iter().zip(&mesh.tri_face).filter(|(_, i)| **i as usize == f.index).any(|(t, _)| {
                match (mesh.positions.get(t[0] as usize), mesh.positions.get(t[1] as usize), mesh.positions.get(t[2] as usize)) {
                    (Some(a), Some(b), Some(c)) => closest_on_triangle(p, *a, *b, *c).dist(p) <= tol * 10.0,
                    _ => false,
                }
            })
        };
        if !edges.iter().all(|(_, p)| near(*p)) {
            continue;
        }
        // A point inside the face: the middle of its largest triangle.
        let inside = mesh
            .triangles
            .iter()
            .zip(&mesh.tri_face)
            .filter(|(_, i)| **i as usize == f.index)
            .filter_map(|(t, _)| Some((*mesh.positions.get(t[0] as usize)?, *mesh.positions.get(t[1] as usize)?, *mesh.positions.get(t[2] as usize)?)))
            .max_by(|x, y| (x.1 - x.0).cross(x.2 - x.0).len().total_cmp(&(y.1 - y.0).cross(y.2 - y.0).len()))
            .map(|(a, b, c)| (a + b + c) * (1.0 / 3.0))?;
        let facing = (eye - inside).normalized().map_or(0.0, |v| v.dot(n));
        if best.is_none_or(|(s, _, _)| facing > s) {
            best = Some((facing, f.index, inside));
        }
    }
    best.map(|(_, i, p)| (body.clone(), i, p))
}

// ---------------------------------------------------------------------------------------------
// Projects folder: every design saves itself there as you work, and Home lists it, newest first.

/// The one folder that holds every design: `SMPL3D_PROJECTS`, else Smpl3D's own Projects folder.
pub fn projects_dir() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("SMPL3D_PROJECTS")
        && !dir.is_empty()
    {
        return dir.into();
    }
    // Tests never read or write the real designs: a folder of their own, per test process.
    #[cfg(test)]
    {
        std::env::temp_dir().join(format!("smpl3d-test-projects-{}", std::process::id()))
    }
    #[cfg(not(test))]
    {
        let home = std::env::var("HOME").unwrap_or_default();
        std::path::Path::new(&home).join("custom scripts/projects/Smpl3D/Projects")
    }
}

thread_local! {
    /// When the open design was first seen with unsaved changes (autosave waits a moment).
    static DIRTY_SINCE: RefCell<Option<std::time::Instant>> = const { RefCell::new(None) };
    /// An autosave is being written: its "Saved …" notice is not shown.
    static QUIET: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// When Home last read the Projects folder.
    static LISTED_AT: RefCell<Option<std::time::Instant>> = const { RefCell::new(None) };
}

const AUTOSAVE_AFTER: std::time::Duration = std::time::Duration::from_millis(1500);
const DESIGN_EXT: &str = smpl3d_engine::io::DESIGN_EXT;

fn in_projects(path: &str) -> bool {
    let p = std::path::Path::new(path);
    p.extension().is_some_and(|e| e == DESIGN_EXT) && p.parent().is_some_and(|d| d == projects_dir())
}

/// A free file name in the Projects folder for `name`: "Bracket.sc", else "Bracket 2.sc"…
fn free_path(name: &str) -> String {
    let dir = projects_dir();
    let clean: String = name.chars().map(|c| if matches!(c, '/' | ':' | '\\') { '-' } else { c }).collect();
    let clean = if clean.trim().is_empty() { "Untitled".to_string() } else { clean.trim().to_string() };
    let mut n = 1;
    loop {
        let file = if n == 1 { format!("{clean}.{DESIGN_EXT}") } else { format!("{clean} {n}.{DESIGN_EXT}") };
        let p = dir.join(file);
        if !p.exists() {
            return p.to_string_lossy().into_owned();
        }
        n += 1;
    }
}

/// Whether a save can start now.
fn can_save(app: &SolveApp) -> bool {
    app.saving.is_none() && app.file_picker.is_none() && !app.file_load.busy()
}

/// Whether a notice should stay hidden: the "Saved …" of an autosave.
pub fn quiet_notice(text: &str, error: bool) -> bool {
    !error && QUIET.with(std::cell::Cell::get) && text.starts_with("Saved")
}

/// Save the open design now (the Save button): into its file in Projects, or as a new file there.
pub fn save_now(app: &mut SolveApp) {
    save(app, false);
}

fn save(app: &mut SolveApp, quiet: bool) {
    if !can_save(app) {
        return;
    }
    QUIET.with(|q| q.set(quiet));
    DIRTY_SINCE.with(|d| *d.borrow_mut() = None);
    match app.session.path.clone() {
        Some(path) if in_projects(&path) => {
            crate::save_job::start(app, "file.save", serde_json::json!({}));
        }
        _ => {
            let _ = std::fs::create_dir_all(projects_dir());
            let path = free_path(&app.session.save_as_name());
            crate::save_job::start(app, "file.save_as", serde_json::json!({"path": path}));
        }
    }
}

/// Every frame: save the open design a moment after it changes.
pub fn autosave(app: &mut SolveApp) {
    if !app.preferences.shapr_tools || app.home.open {
        return;
    }
    // Saves keep the previous version as "<file>.bak"; Projects stays clean without them.
    if app.saving.is_none() {
        QUIET.with(|q| q.set(false));
    }
    if app.saving.is_none()
        && let Some(path) = app.session.path.as_deref()
        && in_projects(path)
    {
        let bak = format!("{path}.bak");
        if std::path::Path::new(&bak).is_file() {
            let _ = std::fs::remove_file(bak);
        }
    }
    // A blank, untouched design is not worth a file.
    let worth = app.session.is_dirty() || app.session.path.as_deref().is_some_and(|p| !in_projects(p));
    if !worth {
        DIRTY_SINCE.with(|d| *d.borrow_mut() = None);
        return;
    }
    let since = DIRTY_SINCE.with(|d| *d.borrow_mut().get_or_insert_with(std::time::Instant::now));
    if since.elapsed() >= AUTOSAVE_AFTER && can_save(app) && app.tool.is_none() {
        save(app, true);
    }
}

/// The Save button's label: "Saved" once everything is in its file.
pub fn save_label(app: &SolveApp) -> String {
    let saved = !app.session.is_dirty() && app.session.path.as_deref().is_some_and(in_projects);
    if app.saving.is_some() {
        "Saving…".into()
    } else if saved {
        "Saved".into()
    } else {
        "Save".into()
    }
}

/// Home: the recent list is the Projects folder, most recently changed first.
pub fn list_projects(app: &mut SolveApp) {
    if !app.preferences.shapr_tools {
        return;
    }
    let fresh = LISTED_AT.with(|l| l.borrow().is_some_and(|t| t.elapsed() < std::time::Duration::from_secs(2)));
    if fresh {
        return;
    }
    LISTED_AT.with(|l| *l.borrow_mut() = Some(std::time::Instant::now()));
    let Ok(entries) = std::fs::read_dir(projects_dir()) else { return };
    let mut found: Vec<(std::time::SystemTime, String)> = entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == DESIGN_EXT))
        .filter_map(|e| {
            let modified = e.metadata().ok()?.modified().ok()?;
            Some((modified, e.path().to_string_lossy().into_owned()))
        })
        .collect();
    found.sort_by_key(|f| std::cmp::Reverse(f.0));
    let list: Vec<String> = found.into_iter().map(|(_, p)| p).collect();
    if list != app.home.recent {
        app.home.recent = list;
    }
}

/// Scale's two-way arrow: drag up to grow, down to shrink (about 2× per 70 points).
pub fn scale_arrow(ui: &mut egui::Ui, value: &mut String) {
    let t = Tokens::get();
    let (rect, resp) = ui.allocate_exact_size(vec2(18.0, 24.0), Sense::drag());
    crate::scenario::publish_handle("scale_arrow", rect.center());
    let resp = resp.on_hover_cursor(egui::CursorIcon::ResizeVertical).on_hover_text("Drag up to grow, down to shrink");
    let col = if resp.hovered() || resp.dragged() { t.accent } else { t.text };
    if resp.hovered() || resp.dragged() {
        ui.painter().rect_filled(rect, 4.0, t.accent.gamma_multiply(0.15));
    }
    let c = rect.center();
    let p = ui.painter();
    p.line_segment([pos2(c.x, rect.top() + 6.0), pos2(c.x, rect.bottom() - 6.0)], Stroke::new(1.5, col));
    let head = |tip: egui::Pos2, dir: f32| {
        vec![tip, pos2(tip.x - 4.5, tip.y + 5.0 * dir), pos2(tip.x + 4.5, tip.y + 5.0 * dir)]
    };
    p.add(egui::Shape::convex_polygon(head(pos2(c.x, rect.top() + 2.0), 1.0), col, Stroke::NONE));
    p.add(egui::Shape::convex_polygon(head(pos2(c.x, rect.bottom() - 2.0), -1.0), col, Stroke::NONE));
    if resp.dragged() {
        let dy = f64::from(resp.drag_delta().y);
        let now = value.trim().parse::<f64>().ok().filter(|f| f.is_finite() && *f > 0.0).unwrap_or(1.0);
        let next = (now * (-dy * 0.01).exp()).clamp(0.01, 1000.0);
        let s = format!("{next:.3}");
        *value = s.trim_end_matches('0').trim_end_matches('.').to_string();
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use smpl3d_engine::Sel;

    /// Dragging Scale's two-way arrow up grows the factor; down shrinks it.
    #[test]
    fn the_scale_arrow_grows_up_and_shrinks_down() {
        let mut h = crate::scenario::Harness::new();
        h.app.run("solid.box", json!({"length": 20, "width": 30, "height": 10})).unwrap();
        h.app.start("solid.scale");
        if let Some(d) = h.app.dialog.as_mut() {
            d.inputs[0].items = vec![Sel::Body { name: "Body1".into() }];
        }
        h.frames(4);
        let factor = |h: &crate::scenario::Harness| -> f64 {
            let d = h.app.dialog.as_ref().unwrap();
            let p = crate::dialogs::apply_commands(&h.app, d).unwrap();
            p[0].1["factor"].as_str().unwrap().parse().unwrap()
        };
        let at = h.at(&json!({"handle": "scale_arrow"})).unwrap();
        h.step(&json!({"drag": [at, [at[0], at[1] - 70.0]], "steps": 10})).unwrap();
        h.frames(2);
        let up = factor(&h);
        assert!(up > 1.5 && up < 2.5, "dragged up: {up}");
        let at = h.at(&json!({"handle": "scale_arrow"})).unwrap();
        h.step(&json!({"drag": [at, [at[0], at[1] + 140.0]], "steps": 10})).unwrap();
        h.frames(2);
        let down = factor(&h);
        assert!(down < 1.0, "dragged down: {down}");
    }
    /// A selected edge shows a two-way arrow: pulled in it fillets, pulled out it chamfers.
    #[test]
    fn dragging_an_edge_arrow_fillets_in_and_chamfers_out() {
        for (handle, chamfer) in [("selection_arrow", false), ("selection_arrow_out", true)] {
            let mut h = crate::scenario::Harness::new();
            h.app.run("solid.box", json!({"length": 40, "width": 30, "height": 20})).unwrap();
            h.frames(3);
            let st = h.app.session.world_state();
            let mesh = st.body("Body1").unwrap().mesh();
            let (index, poly) = mesh.edges.iter().enumerate().max_by(|a, b| a.1[0].z.total_cmp(&b.1[0].z)).unwrap();
            let point = (poly[0] + poly[poly.len() - 1]) * 0.5;
            let dir = crate::canvas::blend_grow_dir(&mesh, index, point).unwrap();
            assert!(dir.z < 0.0, "in points into the box: {dir:?}");
            drop(st);
            h.app.session.selection = vec![Sel::Edge { body: "Body1".into(), index, point }];
            h.frames(3);
            let mid = h.at(&json!({"handle": "selection_arrow_base"})).expect("the edge shows its arrow");
            let to = h.at(&json!({"handle": handle})).unwrap();
            let far = [mid[0] + (to[0] - mid[0]) * 2.5, mid[1] + (to[1] - mid[1]) * 2.5];
            h.step(&json!({"drag": [mid, far], "steps": 10})).unwrap();
            h.frames(3);
            let d = h.app.dialog.as_ref().expect("the tool is open");
            let crate::dialogs::Kind::Fillet { radius, chamfer: c, .. } = &d.kind else { panic!("{:?}", d.kind) };
            assert_eq!(*c, chamfer, "{handle}");
            assert!(!d.inputs[0].items.is_empty(), "the edge is in it");
            assert_ne!(radius, "2 mm", "the drag set the size");
        }
    }

    #[test]
    fn align_from_the_move_popup_keeps_the_bodies() {
        let mut h = crate::scenario::Harness::new();
        h.app.run("solid.box", json!({"length": 20, "width": 30, "height": 10})).unwrap();
        h.app.start("solid.move");
        if let Some(d) = h.app.dialog.as_mut() {
            d.inputs[0].items = vec![Sel::Body { name: "Body1".into() }];
        }
        h.frames(4);
        h.step(&json!({"click": {"handle": "move:solid.align"}})).unwrap();
        h.frames(3);
        let d = h.app.dialog.as_ref().expect("Align is open");
        assert!(matches!(d.kind, crate::dialogs::Kind::Align { .. }), "{:?}", d.kind);
        assert_eq!(d.inputs[0].items, vec![Sel::Body { name: "Body1".into() }]);
        assert_eq!(d.active, 1);
    }

    /// The selection bar's Isolate shows only the selected body; Exit Isolate brings the rest back.
    #[test]
    fn isolate_and_exit() {
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.app.run("solid.box", json!({"length": 20, "width": 20, "height": 20})).unwrap();
        h.app.run("solid.box", json!({"length": 10, "width": 10, "height": 10, "center": [40, 0, 5], "operation": "new"})).unwrap();
        h.app.session.selection = vec![Sel::Body { name: "Body1".into() }];
        h.frames(4);
        h.step(&json!({"click": {"handle": "smpl3d:isolate"}})).unwrap();
        h.frames(2);
        assert!(h.app.session.visibility.hidden_bodies.contains(&"Body2".to_string()), "{:?}", h.app.session.visibility.hidden_bodies);
        assert!(!h.app.session.visibility.hidden_bodies.contains(&"Body1".to_string()));
        h.step(&json!({"click": {"handle": "smpl3d:exit_isolate"}})).unwrap();
        h.frames(2);
        assert!(h.app.session.visibility.hidden_bodies.is_empty());
    }

    /// The bottom-left Isolate button shows only the selected body, and a second click brings
    /// the others back; with nothing selected it says what to do and hides nothing.
    #[test]
    fn the_isolate_button_toggles() {
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.app.run("solid.box", json!({"length": 20, "width": 20, "height": 20})).unwrap();
        h.app.run("solid.box", json!({"length": 10, "width": 10, "height": 10, "center": [40, 0, 5], "operation": "new"})).unwrap();
        h.frames(4);
        h.step(&json!({"click": {"handle": "smpl3d:isolate_pill"}})).unwrap();
        h.frames(2);
        assert!(h.app.session.visibility.hidden_bodies.is_empty(), "nothing selected: nothing is hidden");
        assert!(h.app.status.as_ref().is_some_and(|(text, ..)| text.contains("Select a body")), "{:?}", h.app.status);
        h.app.session.selection = vec![Sel::Body { name: "Body1".into() }];
        h.frames(2);
        h.step(&json!({"click": {"handle": "smpl3d:isolate_pill"}})).unwrap();
        h.frames(2);
        assert_eq!(h.app.session.visibility.hidden_bodies, vec!["Body2".to_string()]);
        // An export while isolated writes only the body shown; one that names its bodies, or
        // one made after leaving Isolate, is not narrowed.
        let dir = std::env::temp_dir().join(format!("smpl3d-isolate-export-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let size = |h: &mut crate::scenario::Harness, name: &str, extra: serde_json::Value| {
            let mut p = json!({"path": dir.join(name).to_string_lossy()});
            if let Some(bodies) = extra.get("bodies") {
                p["bodies"] = bodies.clone();
            }
            h.app.run("file.export", p).unwrap()["bytes"].as_u64().unwrap()
        };
        let alone = size(&mut h, "alone.stl", json!({}));
        let both = size(&mut h, "named.stl", json!({"bodies": ["Body1", "Body2"]}));
        assert!(alone < both, "isolated {alone} bytes, both bodies {both}");
        h.step(&json!({"click": {"handle": "smpl3d:isolate_pill"}})).unwrap();
        h.frames(2);
        assert_eq!(size(&mut h, "all.stl", json!({})), both, "everything is exported again after Isolate");
        let _ = std::fs::remove_dir_all(&dir);
        h.step(&json!({"click": {"handle": "smpl3d:isolate_pill"}})).unwrap();
        h.frames(2);
        h.step(&json!({"click": {"handle": "smpl3d:isolate_pill"}})).unwrap();
        h.frames(2);
        assert!(h.app.session.visibility.hidden_bodies.is_empty(), "the second click shows everything again");
    }

    /// A selected flat face shows its dead centre.
    #[test]
    fn a_face_shows_its_centre() {
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.app.run("solid.box", json!({"length": 40, "width": 30, "height": 20})).unwrap();
        h.settle();
        h.step(&json!({"click": {"world": [10, 10, 20]}})).unwrap();
        h.frames(3);
        let [Sel::Face { body, index, .. }] = h.app.session.selection.as_slice() else { panic!("{:?}", h.app.session.selection) };
        let c = super::face_centre(&h.app, body, *index).unwrap();
        assert!((c - smpl3d_engine::geom::Vec3::new(20.0, 15.0, 20.0)).len() < 1e-6, "{c:?}");
        assert!(h.at(&json!({"handle": "face_centre"})).is_ok());
    }

    /// A curved face shows its centre too, on the face; and while sketching, the centre of the
    /// area under the pointer shows and is snapped to.
    #[test]
    fn curved_faces_and_sketch_areas_show_their_centre() {
        use smpl3d_engine::geom::Vec3;
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.app.run("solid.cylinder", json!({"diameter": 20, "height": 40})).unwrap();
        h.settle();
        h.frames(4);
        let st = h.app.session.model.state();
        let body = st.bodies[0].name.clone();
        // The side: the face with no flat normal.
        let side = (0..st.bodies[0].body.face_count()).find(|i| crate::dialogs::planar_face(&h.app.session, &body, *i).is_none()).unwrap();
        drop(st);
        h.app.session.selection = vec![Sel::Face { body: body.clone(), index: side, point: smpl3d_engine::geom::Vec3::new(10.0, 0.0, 20.0) }];
        h.frames(3);
        assert!(h.at(&json!({"handle": "face_centre"})).is_ok(), "a curved face shows its centre");

        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.app.run("sketch.create", json!({"plane": "XY"})).unwrap();
        h.app.run("sketch.rectangle.two_point", json!({"p0": [0, 0], "p1": [40, 20]})).unwrap();
        h.call("ui.view", json!({"view": "top"}));
        h.app.cam.distance = 120.0;
        h.app.cam.target = Vec3::new(20.0, 10.0, 0.0);
        h.settle();
        h.step(&json!({"start": "sketch.line"})).unwrap();
        let near = h.at(&json!({"world": [21, 10.5, 0]})).unwrap();
        h.call("ui.move", json!({"x": near[0], "y": near[1]}));
        h.frames(3);
        let mark = h.at(&json!({"handle": "face_centre"})).expect("the area's centre shows");
        let want = h.at(&json!({"world": [20, 10, 0]})).unwrap();
        assert!((mark[0] - want[0]).hypot(mark[1] - want[1]) < 1.0, "{mark:?} vs {want:?}");
    }

    /// A rectangle drawn line by line, closing on its first corner, is an area that extrudes.
    #[test]
    fn a_shape_drawn_line_by_line_extrudes() {
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.app.run("sketch.create", json!({"plane": "XY"})).unwrap();
        h.call("ui.view", json!({"view": "top"}));
        h.app.cam.distance = 120.0;
        h.app.cam.target = smpl3d_engine::geom::Vec3::new(10.0, 10.0, 0.0);
        h.settle();
        h.step(&json!({"start": "sketch.line"})).unwrap();
        for p in [[5, 5], [25, 5], [25, 25], [5, 25], [5, 5]] {
            h.step(&json!({"click": {"world": [p[0], p[1], 0]}})).unwrap();
            h.frames(3);
        }
        h.step(&json!({"key": "Escape"})).unwrap();
        h.frames(3);
        let doc = serde_json::to_value(&*h.app.session.doc).unwrap();
        eprintln!("SKETCH {}", doc["features"]);
        h.step(&json!({"key": "Escape"})).unwrap();
        h.frames(3);
        h.app.session.selection.clear();
        h.step(&json!({"click": {"world": [15, 15, 0]}})).unwrap();
        h.frames(3);
        assert!(matches!(h.app.session.selection.as_slice(), [Sel::Profile { .. }]), "the inside is an area: {:?}", h.app.session.selection);
        // Seen from an angle, the area shows its extrude arrow; dragging it extrudes.
        h.call("ui.view", json!({"view": "isometric"}));
        h.settle();
        let base = h.at(&json!({"handle": "selection_arrow_base"})).expect("the area shows its arrow");
        let tip = h.at(&json!({"handle": "selection_arrow"})).unwrap();
        let far = [base[0] + (tip[0] - base[0]) * 2.5, base[1] + (tip[1] - base[1]) * 2.5];
        h.step(&json!({"drag": [base, far], "steps": 10})).unwrap();
        h.frames(3);
        let d = h.app.dialog.as_ref().expect("Extrude is open");
        assert!(matches!(d.kind, crate::dialogs::Kind::Extrude { .. }), "{:?}", d.kind);
    }

    /// Double-clicking a face selects its whole body.
    #[test]
    fn double_click_selects_the_body() {
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.app.run("solid.box", json!({"length": 40, "width": 30, "height": 20})).unwrap();
        h.settle();
        h.frames(40);
        h.step(&json!({"click": {"world": [20, 15, 20]}, "double": true})).unwrap();
        h.frames(3);
        let d = h.app.dialog.as_ref().expect("Move opened");
        assert!(matches!(d.kind, crate::dialogs::Kind::Move { .. }), "{:?}", d.kind);
        assert_eq!(d.inputs[0].items, vec![Sel::Body { name: "Body1".into() }]);
        // The box's Copy pill turns Copy on; its switch goes to Scale and back to Move, on the
        // same body.
        h.step(&json!({"click": {"handle": "box:copy"}})).unwrap();
        h.frames(2);
        let copy = h.app.dialog.as_ref().and_then(|d| d.extra.get("copy").and_then(serde_json::Value::as_bool));
        assert_eq!(copy, Some(true));
        h.step(&json!({"click": {"handle": "box:scale"}})).unwrap();
        h.frames(3);
        let d = h.app.dialog.as_ref().expect("Scale opened");
        assert_eq!(d.command(), Some("solid.scale"));
        assert_eq!(d.inputs[0].items, vec![Sel::Body { name: "Body1".into() }]);
        h.step(&json!({"click": {"handle": "box:move"}})).unwrap();
        h.frames(3);
        let d = h.app.dialog.as_ref().expect("Move again");
        assert!(matches!(d.kind, crate::dialogs::Kind::Move { .. }), "{:?}", d.kind);
        assert_eq!(d.inputs[0].items, vec![Sel::Body { name: "Body1".into() }]);
    }

    /// Double-clicking a body inside a component opens Move on it and leaves the whole design
    /// active: the other parts are not faded while it is moved.
    #[test]
    fn double_click_on_a_component_body_does_not_fade_the_rest() {
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.app.run("component.create", json!({"name": "Part"})).unwrap();
        h.app.run("solid.box", json!({"length": 40, "width": 30, "height": 20, "body_name": "Inner"})).unwrap();
        h.app.run("component.activate", json!({"component": "root"})).unwrap();
        h.app.run("solid.box", json!({"length": 10, "width": 10, "height": 10, "corner": [60, 0, 0], "body_name": "Other"})).unwrap();
        h.settle();
        h.frames(40);
        h.step(&json!({"click": {"world": [20, 15, 20]}, "double": true})).unwrap();
        h.frames(3);
        let d = h.app.dialog.as_ref().expect("Move opened");
        assert!(matches!(d.kind, crate::dialogs::Kind::Move { .. }), "{:?}", d.kind);
        assert_eq!(d.inputs[0].items, vec![Sel::Body { name: "Inner".into() }]);
        assert_eq!(h.app.session.active_component, 0, "the design stays active");
        let st = crate::open_meshing::shown_world(&h.app);
        assert!(st.bodies.iter().all(|b| !crate::viewport::body_faded(&h.app, b, &None)), "no body is faded");
    }

    /// A sketch area picked in the scene (its sketch closed) goes with the Delete key; one a body
    /// was made from hides instead, keeping the body.
    #[test]
    fn delete_key_removes_a_picked_sketch_area() {
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.app.run("sketch.create", json!({"plane": "XY"})).unwrap();
        h.app.run("sketch.rectangle.two_point", json!({"p0": [0, 0], "p1": [40, 30]})).unwrap();
        h.app.run("sketch.finish", json!({})).unwrap();
        let sketch = h.app.session.doc.features[0].id;
        h.app.run("solid.extrude", json!({"sketch": sketch, "distance": 10, "operation": "new"})).unwrap();
        h.settle();
        h.frames(30);
        h.app.session.selection = vec![Sel::Profile { sketch, index: 0 }];
        h.frames(3);
        assert!(h.app.session.selection.iter().any(|s| matches!(s, Sel::Profile { .. })), "{:?}", h.app.session.selection);
        h.step(&json!({"key": "Delete"})).unwrap();
        h.settle();
        h.frames(5);
        // The extrude was made from it: the sketch hides and the body stays.
        assert!(h.app.menu.confirm.is_none(), "no question asked");
        let st = h.app.session.model.state();
        assert_eq!(st.sketches.first().map(|s| s.sketch.curves.len()), Some(4), "the curves stay for the body");
        assert_eq!(st.bodies.len(), 1, "the body stays");
        assert!(h.app.session.model.results.iter().all(|r| r.error.is_none()));
        let id = sketch.to_string();
        assert!(h.app.session.visibility.hidden_sketches.iter().any(|x| x.to_string() == id), "{:?}", h.app.session.visibility);
        // A sketch no body was made from: Delete takes its lines.
        h.app.run("sketch.create", json!({"plane": "XY"})).unwrap();
        h.app.run("sketch.rectangle.two_point", json!({"p0": [60, 0], "p1": [80, 20]})).unwrap();
        h.app.run("sketch.finish", json!({})).unwrap();
        let free = h.app.session.doc.features.last().map(|f| f.id).unwrap();
        h.settle();
        h.frames(5);
        h.app.session.selection = vec![Sel::Profile { sketch: free, index: 0 }];
        h.step(&json!({"key": "Delete"})).unwrap();
        h.settle();
        h.frames(5);
        let st = h.app.session.model.state();
        assert_eq!(st.sketch(free).map(|s| s.sketch.curves.len()), Some(0), "status {:?}", h.app.status);
    }

    /// Offset works with no sketch open, as in Shapr3D: on a picked face it moves the face, on
    /// a closed sketch's area it offsets its lines in that sketch, and on body edges it offsets
    /// them in a new sketch on their face.
    #[test]
    fn offset_works_on_faces_sketch_areas_and_edges() {
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.app.preferences.shapr_tools = true;
        h.app.run("sketch.create", json!({"plane": "XY"})).unwrap();
        h.app.run("sketch.rectangle.two_point", json!({"p0": [0, 0], "p1": [40, 30]})).unwrap();
        h.app.run("sketch.finish", json!({})).unwrap();
        let sketch = h.app.session.doc.features[0].id;
        h.app.run("solid.extrude", json!({"sketch": sketch, "distance": 10, "operation": "new"})).unwrap();
        h.settle();
        h.frames(5);
        assert!(super::offset_enabled(&h.app), "Offset is on with no sketch open");
        let curves = |h: &crate::scenario::Harness, k: u64| h.app.session.model.state().sketch(k).map_or(0, |s| s.sketch.curves.len());
        let commit = |h: &mut crate::scenario::Harness| {
            h.frames(3);
            h.step(&json!({"key": "Enter"})).unwrap();
            h.settle();
            h.frames(3);
        };

        // A sketch area: its four lines get four more, in the same sketch.
        h.app.session.selection = vec![Sel::Profile { sketch, index: 0 }];
        h.app.start("sketch.offset");
        assert_eq!(h.app.session.active_sketch, Some(sketch), "the sketch opened, status {:?}", h.app.status);
        let d = h.app.dialog.as_ref().expect("Offset is open");
        assert!(matches!(d.kind, crate::dialogs::Kind::SketchModify(_)), "{:?}", d.kind);
        assert_eq!(d.inputs[0].items.len(), 4, "the area's lines are picked: {:?}", d.inputs[0].items);
        commit(&mut h);
        assert_eq!(curves(&h, sketch), 8, "status {:?}", h.app.status);
        h.app.run("sketch.finish", json!({})).unwrap();
        h.settle();
        h.frames(3);

        // A face: Offset Face, with the face in it.
        let top = [20.0, 15.0, 10.0];
        let st = h.app.session.world_state();
        let mesh = st.body("Body1").unwrap().mesh();
        let index = mesh.triangles.iter().zip(&mesh.tri_face).find(|(t, _)| mesh.normals[t[0] as usize].z > 0.9).map(|(_, f)| *f as usize).unwrap();
        drop(st);
        h.app.session.selection = vec![Sel::Face { body: "Body1".into(), index, point: smpl3d_engine::geom::Vec3::new(top[0], top[1], top[2]) }];
        h.app.start("sketch.offset");
        let d = h.app.dialog.as_ref().expect("Offset Face is open");
        assert!(matches!(d.kind, crate::dialogs::Kind::OffsetFaces { .. }), "{:?}", d.kind);
        assert!(!d.inputs[0].items.is_empty(), "the face is in it");
        h.app.dialog = None;

        // Two top edges: a new sketch on the top face, with their offsets.
        let sketches = h.app.session.doc.features.len();
        h.app.session.selection = vec![
            Sel::Edge { body: "Body1".into(), index: 0, point: smpl3d_engine::geom::Vec3::new(20.0, 0.0, 10.0) },
            Sel::Edge { body: "Body1".into(), index: 0, point: smpl3d_engine::geom::Vec3::new(40.0, 15.0, 10.0) },
        ];
        h.app.start("sketch.offset");
        let k = h.app.session.active_sketch.unwrap_or_else(|| panic!("a sketch opened on the face: {:?} {:?}", h.app.status, h.app.session.model.state().bodies.iter().map(|b| b.name.clone()).collect::<Vec<_>>()));
        assert!(h.app.session.doc.features.len() > sketches);
        let d = h.app.dialog.as_ref().expect("Offset is open");
        assert_eq!(d.inputs[0].items.len(), 2, "the two edges are picked: {:?}", d.inputs[0].items);
        commit(&mut h);
        assert_eq!(curves(&h, k), 4, "status {:?}", h.app.status);
    }

    /// Turning the view keeps the Move arrows where they were: each still shows the same end of
    /// its axis, and a drag on it still moves along it.
    #[test]
    fn turning_the_view_keeps_the_move_arrows() {
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.app.run("solid.box", json!({"length": 40, "width": 30, "height": 20})).unwrap();
        h.settle();
        h.app.session.selection = vec![Sel::Body { name: "Body1".into() }];
        h.app.start("solid.move");
        h.frames(4);
        let ends = |h: &crate::scenario::Harness| -> Vec<f64> {
            let c = crate::gizmo::handle_point("ball", smpl3d_engine::geom::Vec3::ZERO, 60.0).unwrap();
            let _ = h;
            ["x", "y", "z"]
                .iter()
                .zip([smpl3d_engine::geom::Vec3::X, smpl3d_engine::geom::Vec3::Y, smpl3d_engine::geom::Vec3::Z])
                .map(|(n, d)| (crate::gizmo::handle_point(n, smpl3d_engine::geom::Vec3::ZERO, 60.0).unwrap() - c).dot(d).signum())
                .collect()
        };
        let before = ends(&h);
        let spots = || -> Vec<Option<smpl3d_engine::geom::Vec3>> {
            ["xy", "yz", "xz", "rx", "ry", "rz"].iter().map(|n| crate::gizmo::handle_point(n, smpl3d_engine::geom::Vec3::ZERO, 60.0)).collect()
        };
        let mut last = spots();
        for turn in [0.15, 0.15, 0.15, 0.15] {
            h.app.cam.yaw += turn;
            h.frames(2);
            assert_eq!(ends(&h), before, "the arrows keep their ends while the view turns");
            // Tiles and rotate arrows stay where they are in the model (shown ones).
            let now = spots();
            for (a, b) in last.iter().zip(&now) {
                if let (Some(a), Some(b)) = (a, b) {
                    assert!(a.dist(*b) < 1e-3, "a handle slid: {a:?} → {b:?}");
                }
            }
            last = now;
        }
        // A drag on the X arrow still moves along X.
        h.step(&json!({"drag": [{"handle": "move_x"}, {"gizmo": "x", "by": [10.0, 0.0, 0.0]}], "steps": 8})).unwrap();
        h.frames(2);
        let Some(crate::dialogs::Kind::Move { x, y, .. }) = h.app.dialog.as_ref().map(|d| &d.kind) else { panic!("Move is open") };
        let x: f64 = x.trim_end_matches(" mm").trim().parse().unwrap_or(0.0);
        assert!((x - 10.0).abs() < 1.5, "x {x} y {y}");
    }

    /// The centre ball moves the Move triad, not the body (as in Shapr3D): the distances stay
    /// zero and the triad's centre goes where the ball was dropped, snapped to a corner.
    #[test]
    fn the_ball_moves_the_triad_not_the_body() {
        use smpl3d_engine::geom::Vec3;
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.app.run("solid.box", json!({"length": 40, "width": 30, "height": 20})).unwrap();
        h.settle();
        h.app.session.selection = vec![Sel::Body { name: "Body1".into() }];
        h.app.start("solid.move");
        h.frames(4);
        let before = crate::gizmo::handle_point("ball", Vec3::ZERO, 60.0).unwrap();
        // Drop it on the box's top corner at (40, 30, 20).
        h.step(&json!({"drag": [{"handle": "move_ball"}, {"world": [40, 30, 20]}], "steps": 10})).unwrap();
        h.frames(3);
        let Some(crate::dialogs::Kind::Move { x, y, z, .. }) = h.app.dialog.as_ref().map(|d| d.kind.clone()) else { panic!("Move is open") };
        for v in [&x, &y, &z] {
            assert!(v.trim_end_matches(" mm").trim().parse::<f64>().unwrap_or(1.0).abs() < 1e-9, "the body stays: {x} {y} {z}");
        }
        let after = crate::gizmo::handle_point("ball", Vec3::ZERO, 60.0).unwrap();
        assert!(after.dist(before) > 1.0, "the triad moved: {before:?} → {after:?}");
        assert!(after.dist(Vec3::new(40.0, 30.0, 20.0)) < 1e-6, "snapped to the corner: {after:?}");
    }

    /// An image just inserted opens Calibrate on it.
    #[test]
    fn an_inserted_image_asks_for_its_size() {
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.frames(2);
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend(400u32.to_be_bytes());
        png.extend(200u32.to_be_bytes());
        png.extend([8, 6, 0, 0, 0, 0, 0, 0, 0]);
        let data = smpl3d_engine::doc::canvas::base64_encode(&png);
        h.app.last_command = Some(("canvas.insert".into(), "Canvas".into()));
        let id = h.app.run("canvas.insert", json!({"data": data, "plane": "XY", "width": 80})).unwrap()["canvas"].as_u64().unwrap();
        h.frames(2);
        let panel = h.app.tree.canvas_panel.as_ref().expect("Calibrate opened");
        assert!(panel.calibrate && panel.canvas == id);
    }

    /// Look: a swatch paints the body; the bottom-left buttons are there.
    #[test]
    fn look_paints_the_body() {
        let mut h = crate::scenario::Harness::new();
        h.app.home.open = false;
        h.app.run("solid.box", json!({"length": 20, "width": 30, "height": 10})).unwrap();
        h.frames(4);
        assert!(h.at(&json!({"handle": "smpl3d:section"})).is_ok(), "Section is shown");
        assert!(h.at(&json!({"handle": "smpl3d:measure"})).is_ok(), "Measure is shown");
        let before = serde_json::to_value(&*h.app.session.doc).unwrap();
        h.step(&json!({"click": {"handle": "smpl3d:look"}})).unwrap();
        h.frames(3);
        h.step(&json!({"click": {"handle": "smpl3d:swatch:Red"}})).unwrap();
        h.frames(2);
        let after = serde_json::to_value(&*h.app.session.doc).unwrap();
        assert_ne!(before, after, "the body was painted");
        assert!(after.to_string().to_lowercase().contains("e5484d") || after.to_string().contains("229"), "red: {after}");
    }

    /// A selected curved face shows an arrow; dragging it opens Offset Face and sets how far.
    #[test]
    fn dragging_a_round_face_arrow_resizes_it() {
        let mut h = crate::scenario::Harness::new();
        h.app.run("solid.cylinder", json!({"radius": 10, "height": 20})).unwrap();
        h.frames(3);
        let st = h.app.session.world_state();
        let mesh = st.body("Body1").unwrap().mesh();
        let (tri, face) = mesh
            .triangles
            .iter()
            .zip(&mesh.tri_face)
            .find(|(t, _)| mesh.normals[t[0] as usize].z.abs() < 0.5)
            .unwrap();
        let point = mesh.positions[tri[0] as usize];
        let index = *face as usize;
        drop(st);
        h.app.session.selection = vec![Sel::Face { body: "Body1".into(), index, point }];
        h.frames(3);
        let at = h.at(&json!({"handle": "selection_arrow"})).expect("the round face shows its arrow");
        h.step(&json!({"drag": [at, [at[0] + 25.0, at[1]]], "steps": 10})).unwrap();
        h.frames(3);
        let d = h.app.dialog.as_ref().expect("a tool is open");
        assert!(matches!(d.kind, crate::dialogs::Kind::OffsetFaces { .. }), "{:?}", d.kind);
        assert!(!d.inputs[0].items.is_empty(), "the face is in it");
    }
}

/// The Move popup's Align or Scale button: start `cmd` on `bodies` next frame.
pub fn switch_from_move(cmd: &'static str, bodies: Vec<smpl3d_engine::Sel>) {
    SWITCH.with(|s| *s.borrow_mut() = Some((cmd, bodies)));
}

/// A command's icon name.
pub fn icon_of(cmd: &str) -> &'static str {
    specs().get(cmd).map_or("box", |c| c.icon)
}

// ---------------------------------------------------------------------------------------------
// Select, then act (as Shapr3D): a selected edge shows a fillet arrow, a selected curved face an
// arrow that grows or shrinks it. Dragging the arrow starts the tool, and the same drag sets its
// value through the tool's own arrow.

/// What a selection's arrow does.
#[derive(Clone, Copy, PartialEq)]
enum ArrowKind {
    /// A sketch area: extrude it.
    Extrude,
    /// An edge: two-way, in for a fillet, out for a chamfer.
    Edge,
    /// A curved face: offset it (grow or shrink).
    Offset,
}

thread_local! {
    /// The last curved face pick's normal: the model, body, face, point, and the normal there.
    #[allow(clippy::type_complexity)]
    static ARROW_NORMAL: RefCell<Option<(std::sync::Arc<smpl3d_engine::doc::ModelState>, String, usize, Vec3, Vec3)>> = const { RefCell::new(None) };
}

/// The arrow a selection offers, where it stands, and the way "in" (along it) points.
fn selection_arrow(app: &SolveApp) -> Option<(ArrowKind, Vec3, Vec3)> {
    use smpl3d_engine::Sel;
    if !app.preferences.shapr_tools || app.tool.is_some() || app.dialog.is_some() {
        return None;
    }
    let st = app.session.world_state();
    match app.session.selection.as_slice() {
        // A sketch area, drawn with any tool (a rectangle, or lines closing a shape), also while
        // the sketch is still open.
        [Sel::Profile { sketch, index }] => {
            let ss = st.sketch(*sketch)?;
            let plane = crate::canvas::profile_plane(app, *sketch)?;
            let p = ss.profiles.get(*index)?;
            let points = p.region.outer.polyline(1e-2);
            let centre = points.iter().fold(smpl3d_engine::geom::Vec2::default(), |sum, q| sum + *q) * (1.0 / points.len().max(1) as f64);
            Some((ArrowKind::Extrude, plane.to_world(centre), plane.normal()))
        }
        _ if app.session.active_sketch.is_some() => None,
        [Sel::Edge { body, index, point }] => {
            let mesh = st.body(body)?.mesh();
            let dir = crate::canvas::blend_grow_dir(&mesh, *index, *point)?;
            Some((ArrowKind::Edge, *point, dir))
        }
        [Sel::Face { body, index, point }] if crate::dialogs::planar_face(&app.session, body, *index).is_none() => {
            // The surface's normal where it was picked: the nearest vertex of that face. Asked
            // for every frame, so kept while the same pick shows on the same model.
            let known = ARROW_NORMAL.with(|c| {
                c.borrow().as_ref().filter(|k| std::sync::Arc::ptr_eq(&k.0, &st) && k.1 == *body && k.2 == *index && k.3.dist(*point) < 1e-12).map(|k| k.4)
            });
            let normal = match known {
                Some(n) => n,
                None => {
                    let mesh = st.body(body)?.mesh();
                    let n = mesh
                        .triangles
                        .iter()
                        .zip(&mesh.tri_face)
                        .filter(|(_, f)| **f as usize == *index)
                        .flat_map(|(t, _)| t.iter().copied())
                        .filter_map(|v| Some((mesh.positions.get(v as usize)?, mesh.normals.get(v as usize)?)))
                        .min_by(|a, b| a.0.dist(*point).total_cmp(&b.0.dist(*point)))
                        .and_then(|(_, n)| n.normalized())?;
                    ARROW_NORMAL.with(|c| *c.borrow_mut() = Some((st.clone(), body.clone(), *index, *point, n)));
                    n
                }
            };
            Some((ArrowKind::Offset, *point, normal))
        }
        _ => None,
    }
}

const SEL_ARROW_PX: f32 = 56.0;

/// The arrow's ends on screen: from `back` (the far end the other way, for a two-way arrow) to the tip.
fn arrow_ends(proj: &crate::viewport::Proj, base: Vec3, dir: Vec3, two_way: bool) -> Option<(egui::Pos2, egui::Pos2)> {
    let a = proj.to_screen(base)?;
    let d = proj.to_screen(base + dir).map(|b| b - a).filter(|d| d.length() > 1e-3)?.normalized();
    let back = if two_way { a - d * SEL_ARROW_PX } else { a };
    Some((back, a + d * SEL_ARROW_PX))
}

fn near_segment(p: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> bool {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
    (a + ab * t - p).length() <= 12.0
}

thread_local! {
    /// An edge arrow being dragged: where it stands and the way "in" points.
    static EDGE_DRAG: RefCell<Option<(Vec3, Vec3)>> = const { RefCell::new(None) };
}

/// A drag that starts on the selection's arrow starts its tool. Extrude and Offset hand the drag
/// to the tool's own arrow; an edge's two-way arrow keeps it, to choose Fillet (in) or Chamfer
/// (out) by the way it is pulled. Returns whether it did.
pub fn press_selection_arrow(app: &mut SolveApp, ctx: &egui::Context, proj: &crate::viewport::Proj, at: egui::Pos2) -> bool {
    let Some((kind, base, dir)) = selection_arrow(app) else { return false };
    let Some((a, b)) = arrow_ends(proj, base, dir, kind == ArrowKind::Edge) else { return false };
    if !near_segment(at, a, b) {
        return false;
    }
    match kind {
        ArrowKind::Edge => {
            app.start("solid.fillet");
            if app.dialog.is_some() {
                EDGE_DRAG.with(|e| *e.borrow_mut() = Some((base, dir)));
                ctx.set_dragged_id(egui::Id::new("smpl3d_edge_drag"));
            }
        }
        ArrowKind::Extrude | ArrowKind::Offset => {
            app.start(if kind == ArrowKind::Extrude { "solid.extrude" } else { "solid.offset_face" });
            if app.dialog.is_some() {
                ctx.set_dragged_id(egui::Id::new("sc_manipulator"));
            }
        }
    }
    true
}

/// Every frame: an edge drag sets Fillet's radius (pulled in) or makes it a chamfer of that
/// distance (pulled out); letting go leaves the tool open to type a value or press Enter.
pub fn edge_drag(app: &mut SolveApp, ctx: &egui::Context, proj: &crate::viewport::Proj) {
    // Inside a sketch the viewport keeps presses for the sketch: an area's extrude arrow takes
    // its press here.
    if app.session.active_sketch.is_some()
        && app.tool.is_none()
        && app.dialog.is_none()
        && ctx.input(|i| i.pointer.primary_pressed())
        && let Some(p) = ctx.input(|i| i.pointer.press_origin())
        && proj.rect.contains(p)
    {
        press_selection_arrow(app, ctx, proj, p);
    }
    let Some((base, dir)) = EDGE_DRAG.with(|e| *e.borrow()) else { return };
    let down = ctx.input(|i| i.pointer.primary_down());
    if !down || app.dialog.is_none() {
        EDGE_DRAG.with(|e| *e.borrow_mut() = None);
        if ctx.dragged_id() == Some(egui::Id::new("smpl3d_edge_drag")) {
            ctx.stop_dragging();
        }
        return;
    }
    let Some(p) = ctx.input(|i| i.pointer.interact_pos()) else { return };
    let Some(v) = crate::canvas::along(proj, p, base, dir) else { return };
    let step = crate::canvas::snap_step(app.cam.half_height());
    let size = ((v.abs() / step).round() * step).max(step);
    let text = crate::canvas::format_len(&app.session.doc, size);
    if let Some(d) = app.dialog.as_mut()
        && let crate::dialogs::Kind::Fillet { radius, chamfer, distance2, .. } = &mut d.kind
    {
        *chamfer = v < 0.0;
        *radius = text.clone();
        *distance2 = text;
    }
}

/// The selection's arrow, drawn in the accent colour (an edge's with a head at each end).
pub fn show_selection_arrow(app: &SolveApp, proj: &crate::viewport::Proj, painter: &egui::Painter) {
    let Some((kind, base, dir)) = selection_arrow(app) else { return };
    let two_way = kind == ArrowKind::Edge;
    let Some((a, b)) = arrow_ends(proj, base, dir, two_way) else { return };
    let Some(mid) = proj.to_screen(base) else { return };
    crate::scenario::publish_handle("selection_arrow", mid + (b - mid) * 0.6);
    crate::scenario::publish_handle("selection_arrow_base", mid);
    if two_way {
        crate::scenario::publish_handle("selection_arrow_out", mid + (a - mid) * 0.6);
    }
    paint_arrow(painter, a, b, mid, two_way);
}

/// An arrow that shows on any colour (a blue selected face too): blue with a white edge and a
/// soft dark shadow, with a head at the tip (and at the back when `two_way`) and a dot at `base`.
pub fn paint_arrow(painter: &egui::Painter, a: egui::Pos2, b: egui::Pos2, base: egui::Pos2, two_way: bool) {
    let t = Tokens::get();
    let shadow = Color32::from_black_alpha(110);
    let edge = Color32::WHITE;
    let fill = t.accent;
    let d = (b - a).normalized();
    let head = |tip: egui::Pos2, d: egui::Vec2| {
        let side = vec2(-d.y, d.x);
        vec![tip + d * 11.0, tip - d * 4.0 + side * 8.0, tip - d * 4.0 - side * 8.0]
    };
    let mut heads = vec![head(b, d)];
    if two_way {
        heads.push(head(a, -d));
    }
    painter.line_segment([a, b], Stroke::new(8.0, shadow));
    painter.line_segment([a, b], Stroke::new(6.0, edge));
    painter.line_segment([a, b], Stroke::new(3.0, fill));
    for h in heads {
        painter.add(egui::Shape::convex_polygon(h.iter().map(|p| *p + vec2(0.0, 1.0)).collect(), shadow, Stroke::NONE));
        painter.add(egui::Shape::convex_polygon(h, fill, Stroke::new(1.5, edge)));
    }
    painter.circle_filled(base, 6.0, edge);
    painter.circle_filled(base, 4.0, fill);
}

// ---------------------------------------------------------------------------------------------
// Bottom left (as Shapr3D): Section View, Measure, and Look — colours, edges, a picture of the
// model and saved views.

thread_local! {
    static LOOK_OPEN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Paint colours offered in Look.
const SWATCHES: &[(&str, &str)] = &[
    ("#c9ccd1", "Grey"),
    ("#f2f2f2", "White"),
    ("#2b2d31", "Black"),
    ("#e5484d", "Red"),
    ("#f76b15", "Orange"),
    ("#ffc53d", "Yellow"),
    ("#30a46c", "Green"),
    ("#0090ff", "Blue"),
    ("#8e4ec6", "Purple"),
    ("#a07553", "Brown"),
];

/// Where screenshots go: Smpl3D's Screenshots folder, beside Projects.
fn screenshots_dir() -> std::path::PathBuf {
    projects_dir().parent().map_or_else(|| projects_dir().join("Screenshots"), |p| p.join("Screenshots"))
}

/// Paint the selection (faces or bodies), or every body when nothing is selected.
fn paint(app: &mut SolveApp, color: &str) {
    use smpl3d_engine::Sel;
    let sel = app.session.selection.clone();
    let faces: Vec<(String, Vec3)> = sel
        .iter()
        .filter_map(|s| match s {
            Sel::Face { body, point, .. } => Some((body.clone(), *point)),
            _ => None,
        })
        .collect();
    let result = if faces.is_empty() {
        let mut bodies: Vec<String> = sel.iter().filter_map(|s| if let Sel::Body { name } = s { Some(name.clone()) } else { None }).collect();
        if bodies.is_empty() {
            bodies = app.session.model.state().bodies.iter().map(|b| b.name.to_string()).collect();
        }
        bodies.dedup();
        app.run("appearance.assign", serde_json::json!({"bodies": bodies, "color": color}))
    } else {
        // One call per body: a face list names one body.
        let mut r = Ok(serde_json::Value::Null);
        let mut by_body: Vec<(String, Vec<[f64; 3]>)> = Vec::new();
        for (b, p) in faces {
            match by_body.iter_mut().find(|(n, _)| *n == b) {
                Some((_, v)) => v.push([p.x, p.y, p.z]),
                None => by_body.push((b, vec![[p.x, p.y, p.z]])),
            }
        }
        for (body, points) in by_body {
            r = app.run("appearance.assign", serde_json::json!({"faces": points, "body": body, "color": color}));
            if r.is_err() {
                break;
            }
        }
        r
    };
    if let Err(e) = result {
        app.set_status(e, true);
    }
}

/// Save a picture of the model, as the window shows it, at twice the window's size.
fn screenshot(app: &mut SolveApp, size: egui::Vec2) {
    let guides = smpl3d_engine::view::Guides { grid: false, axes: false };
    let scene = crate::control::view_scene(app, guides);
    let (w, h) = ((size.x * 2.0).clamp(64.0, 8192.0) as u32, (size.y * 2.0).clamp(64.0, 8192.0) as u32);
    let png = match smpl3d_engine::capture::png(&scene, &app.cam, w, h) {
        Ok(png) => png,
        Err(e) => return app.set_status(format!("Screenshot: {e}"), true),
    };
    let dir = screenshots_dir();
    let _ = std::fs::create_dir_all(&dir);
    let name = app.session.path.as_deref().and_then(|p| std::path::Path::new(p).file_stem()).map_or_else(|| "Smpl3D".to_string(), |s| s.to_string_lossy().into_owned());
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let path = dir.join(format!("{name} {stamp}.png"));
    match std::fs::write(&path, &png.bytes) {
        Ok(()) => app.set_status(format!("Screenshot saved to Screenshots/{}", path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default()), false),
        Err(e) => app.set_status(format!("Screenshot: {e}"), true),
    }
}

fn pill(ui: &mut egui::Ui, rect: Rect, id: &str, icon: &str, label: &str, on: bool) -> egui::Response {
    let t = Tokens::get();
    let resp = ui.interact(rect, egui::Id::new(("smpl3d_pill", id)), Sense::click());
    crate::scenario::publish_target(&format!("smpl3d:{id}"), rect);
    let fill = if on { t.accent } else if resp.hovered() { t.text.gamma_multiply(0.12) } else { t.toolbar };
    ui.painter().rect_filled(rect, rect.height() / 2.0, fill);
    ui.painter().rect_stroke(rect, rect.height() / 2.0, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
    let icon_rect = Rect::from_center_size(pos2(rect.left() + 18.0, rect.center().y), vec2(16.0, 16.0));
    crate::icons::paint_icon(ui.painter(), icon_rect, icon, 1.0);
    ui.painter().text(pos2(rect.left() + 32.0, rect.center().y), Align2::LEFT_CENTER, label, egui::FontId::proportional(t.font_button), t.text);
    resp
}

/// The bottom-left buttons and the Look panel.
pub fn bottom_left(app: &mut SolveApp, ui: &mut egui::Ui, area: Rect) {
    if !app.preferences.shapr_tools || app.home.open {
        return;
    }
    let t = Tokens::get();
    let bottom = area.bottom() - 64.0;
    // Beside the browser when it reaches down to these buttons, never over its rows.
    let left = match app.shell.browser {
        Some(b) if b.bottom() > bottom - 42.0 && b.left() < area.left() + 16.0 + 440.0 && b.right() < area.center().x => b.right() + 12.0,
        _ => area.left() + 16.0,
    };
    let running = |app: &SolveApp, id: &str| app.dialog.as_ref().is_some_and(|d| d.command() == Some(id)) || app.tool.as_ref().is_some_and(|x| x.cmd == id);
    let mut action: Option<&'static str> = None;
    let buttons = Rect::from_min_size(pos2(left, bottom - 34.0), vec2(440.0, 34.0));
    let mut isolate_clicked = false;
    let look_open = LOOK_OPEN.with(std::cell::Cell::get);
    egui::Area::new(egui::Id::new("smpl3d_bottom_left")).order(egui::Order::Middle).fixed_pos(pos2(left, bottom - 34.0)).show(ui.ctx(), |ui| {
        let section = Rect::from_min_size(pos2(left, bottom - 34.0), vec2(118.0, 34.0));
        let measure = Rect::from_min_size(pos2(section.right() + 8.0, section.top()), vec2(104.0, 34.0));
        let look = Rect::from_min_size(pos2(measure.right() + 8.0, section.top()), vec2(84.0, 34.0));
        let isolate = Rect::from_min_size(pos2(look.right() + 8.0, section.top()), vec2(102.0, 34.0));
        ui.allocate_rect(section.union(isolate), Sense::hover());
        if pill(ui, section, "section", icon_of("inspect.section"), "Section", running(app, "inspect.section")).clicked() {
            action = Some("inspect.section");
        }
        if pill(ui, measure, "measure", icon_of("inspect.measure"), "Measure", running(app, "inspect.measure")).clicked() {
            action = Some("inspect.measure");
        }
        if pill(ui, look, "look", icon_of("appearance.assign"), "Look", look_open).clicked() {
            LOOK_OPEN.with(|l| l.set(!look_open));
        }
        // Isolate: lit while only the picked bodies are shown; a second click brings the rest back.
        if pill(ui, isolate, "isolate_pill", "eye", "Isolate", isolated()).on_hover_text("Show only the selected bodies").clicked() {
            isolate_clicked = true;
        }
    });
    if isolate_clicked {
        if isolated() {
            exit_isolate(app);
        } else {
            match selected_bodies(app) {
                bodies if bodies.is_empty() => app.set_status("Select a body first, then Isolate shows only it", false),
                bodies => isolate(app, bodies),
            }
        }
    }
    if let Some(id) = action {
        if running(app, id) {
            app.dialog = None;
            app.tool = None;
        } else {
            app.start(id);
        }
    }
    if !LOOK_OPEN.with(std::cell::Cell::get) {
        return;
    }
    // The Look panel, above its button.
    let width = 236.0;
    let resp = egui::Area::new(egui::Id::new("smpl3d_look")).order(egui::Order::Foreground).pivot(Align2::LEFT_BOTTOM).fixed_pos(pos2(left + 230.0, bottom - 42.0)).show(ui.ctx(), |ui| {
        egui::Frame::new().fill(t.toolbar).stroke(Stroke::new(1.0, t.border)).corner_radius(12.0).inner_margin(12.0).show(ui, |ui| {
            ui.set_width(width);
            let heading = |ui: &mut egui::Ui, s: &str| {
                ui.label(egui::RichText::new(s.to_uppercase()).size(t.font_micro).color(t.text_dim));
            };
            heading(ui, "Colour");
            ui.label(egui::RichText::new(if app.session.selection.is_empty() { "Paints every body" } else { "Paints the selection" }).size(t.font_micro).color(t.text_dim));
            ui.add_space(4.0);
            egui::Grid::new("smpl3d_swatches").spacing(vec2(6.0, 6.0)).show(ui, |ui| {
                for (i, (hex, name)) in SWATCHES.iter().enumerate() {
                    let (r, resp) = ui.allocate_exact_size(vec2(36.0, 24.0), Sense::click());
                    let c = Color32::from_hex(hex).unwrap_or(Color32::GRAY);
                    ui.painter().rect_filled(r, 6.0, c);
                    ui.painter().rect_stroke(r, 6.0, Stroke::new(if resp.hovered() { 2.0 } else { 1.0 }, if resp.hovered() { t.accent } else { t.border }), egui::StrokeKind::Inside);
                    crate::scenario::publish_target(&format!("smpl3d:swatch:{name}"), r);
                    if resp.on_hover_text(*name).clicked() {
                        paint(app, hex);
                    }
                    if i % 5 == 4 {
                        ui.end_row();
                    }
                }
            });
            ui.add_space(10.0);
            heading(ui, "Display");
            let mut edges = app.ui.visual_style != 1;
            if ui.checkbox(&mut edges, "Show edges").changed() {
                let _ = app.run("view.visual_style", serde_json::json!({"style": if edges { "shaded_with_edges" } else { "shaded" }}));
            }
            let mut grid = app.ui.show_grid;
            if ui.checkbox(&mut grid, "Show grid").changed() {
                let _ = app.run("view.grid", serde_json::json!({"on": grid}));
            }
            ui.add_space(10.0);
            heading(ui, "Picture");
            if ui.button("📷  Save screenshot").clicked() {
                let size = ui.ctx().content_rect().size();
                screenshot(app, size);
            }
            ui.add_space(10.0);
            heading(ui, "Views");
            let views: Vec<String> = app.session.doc.named_views.iter().map(|v| v.name.clone()).collect();
            for name in views {
                if ui.button(&name).clicked() {
                    crate::browser::restore_view(app, &name);
                }
            }
            if ui.button("＋  Save this view").clicked() {
                crate::browser::save_view(app, None);
            }
        });
    });
    // A click outside closes it, as a popover does (not on its own button).
    let outside = ui.input(|i| i.pointer.any_click()) && !resp.response.contains_pointer() && !ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| buttons.contains(p))
        || ui.input(|i| i.key_pressed(egui::Key::Escape));
    if outside {
        LOOK_OPEN.with(|l| l.set(false));
    }
}

/// As Shapr3D asks for an inserted image's size: an image just inserted opens Calibrate, to click
/// two points on it and type their true distance.
fn calibrate_new_image(app: &mut SolveApp) {
    let ids: Vec<u64> = app.session.doc.canvases.iter().map(|c| c.id).collect();
    let before = CANVASES.with(|c| std::mem::replace(&mut *c.borrow_mut(), ids.clone()));
    let new: Vec<u64> = ids.iter().copied().filter(|id| !before.contains(id)).collect();
    let inserted = app.last_command.as_ref().is_some_and(|(id, _)| id == "canvas.insert");
    if app.preferences.shapr_tools && new.len() == 1 && ids.len() == before.len() + 1 && inserted && app.tree.canvas_panel.is_none() {
        app.tree.canvas_panel = Some(crate::browser::CanvasPanel::new(app, new[0], true));
        app.set_status(crate::i18n::tr("browser.calibrate_click_a"), false);
    }
}

// ---------------------------------------------------------------------------------------------
// The dead centre of a flat face (as Shapr3D): marked while sketching on it and on a selected
// face, and a snap point for sketch tools.

/// What was worked out from one face's display triangles, kept while the model shown stays
/// the same: these are asked for every frame (the centre mark, the centre snap, the arrow).
#[derive(Clone)]
struct FaceFacts {
    body: String,
    index: usize,
    /// The area centre, and that centre moved onto the face.
    centre: Option<(Vec3, Vec3)>,
}

thread_local! {
    static FACE_FACTS: RefCell<Option<(std::sync::Arc<smpl3d_engine::doc::ModelState>, Vec<FaceFacts>)>> = const { RefCell::new(None) };
}

fn face_facts(app: &SolveApp, body: &str, index: usize) -> FaceFacts {
    let st = crate::open_meshing::shown_world(app);
    let known = FACE_FACTS.with(|c| {
        let c = c.borrow();
        let (world, list) = c.as_ref()?;
        if !std::sync::Arc::ptr_eq(world, &st) {
            return None;
        }
        list.iter().find(|f| f.index == index && f.body == body).cloned()
    });
    if let Some(f) = known {
        return f;
    }
    let centre = area_centre(&st, body, index).map(|c| (c, nearest_on_face(&st, body, index, c)));
    let facts = FaceFacts { body: body.to_string(), index, centre };
    FACE_FACTS.with(|c| {
        let mut c = c.borrow_mut();
        if !c.as_ref().is_some_and(|(world, _)| std::sync::Arc::ptr_eq(world, &st)) {
            *c = Some((st.clone(), Vec::new()));
        }
        if let Some((_, list)) = c.as_mut() {
            if list.len() >= 16 {
                list.remove(0);
            }
            list.push(facts.clone());
        }
    });
    facts
}

/// The area centre of a body's face, from its display triangles.
pub fn face_centre(app: &SolveApp, body: &str, index: usize) -> Option<Vec3> {
    face_facts(app, body, index).centre.map(|c| c.0)
}

fn area_centre(st: &smpl3d_engine::doc::ModelState, body: &str, index: usize) -> Option<Vec3> {
    let mesh = st.body(body)?.mesh();
    let (mut sum, mut area) = (Vec3::ZERO, 0.0);
    for (t, f) in mesh.triangles.iter().zip(&mesh.tri_face) {
        if *f as usize != index {
            continue;
        }
        let [a, b, c] = [mesh.positions.get(t[0] as usize)?, mesh.positions.get(t[1] as usize)?, mesh.positions.get(t[2] as usize)?];
        let w = (*b - *a).cross(*c - *a).len() * 0.5;
        sum += (*a + *b + *c) * (w / 3.0);
        area += w;
    }
    (area > 1e-12).then(|| sum * (1.0 / area))
}

/// The centre to offer while sketching, in the sketch's plane: of the model face under the
/// pointer (`at`, on screen), flat or curved, projected into the plane; else of the sketch area
/// under the pointer (`lp`, in the plane).
pub fn sketch_centre(app: &SolveApp, proj: &crate::viewport::Proj, plane: &smpl3d_engine::geom::Plane, at: egui::Pos2, lp: smpl3d_engine::geom::Vec2) -> Option<smpl3d_engine::geom::Vec2> {
    for h in crate::viewport::pick_for_tool(app, proj, at) {
        if let crate::viewport::Hit::Face { body, index, .. } = h {
            if let Some(c) = face_centre(app, &body, index) {
                return Some(plane.to_local(c));
            }
            break;
        }
    }
    let id = app.session.active_sketch?;
    let st = app.session.model.state();
    let ss = st.sketch(id)?;
    let i = smpl3d_engine::sketch::profile_at_point(&ss.profiles, lp)?;
    ss.profiles.get(i).map(|p| p.centroid)
}

/// The point of a face nearest to `c` (its area centre, which is off a curved face).
fn nearest_on_face(st: &smpl3d_engine::doc::ModelState, body: &str, index: usize, c: Vec3) -> Vec3 {
    let Some(mesh) = st.body(body).map(|b| b.mesh()) else { return c };
    let mut best = (f64::MAX, c);
    for (t, f) in mesh.triangles.iter().zip(&mesh.tri_face) {
        if *f as usize != index {
            continue;
        }
        let (Some(a), Some(b), Some(d)) = (mesh.positions.get(t[0] as usize), mesh.positions.get(t[1] as usize), mesh.positions.get(t[2] as usize)) else { continue };
        let q = closest_on_triangle(c, *a, *b, *d);
        let dd = q.dist(c);
        if dd < best.0 {
            best = (dd, q);
        }
    }
    best.1
}

fn closest_on_triangle(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Vec3 {
    // Ericson, Real-Time Collision Detection, 5.1.5.
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let den = 1.0 / (va + vb + vc);
    a + ab * (vb * den) + ac * (vc * den)
}

/// Where to draw the centre mark: while a sketch tool runs, the centre `sketch_centre` offers
/// under the pointer (in the sketch's plane); otherwise a selected face's centre, on the face.
fn marked_point(app: &SolveApp, proj: &crate::viewport::Proj, pointer: Option<egui::Pos2>) -> Option<Vec3> {
    if !app.preferences.shapr_tools {
        return None;
    }
    let sketching = app.tool.as_ref().is_some_and(|t| t.cmd.starts_with("sketch."));
    if sketching && let Some(id) = app.session.active_sketch {
        let st = app.session.model.state();
        let plane = st.sketch(id)?.plane;
        let at = pointer.filter(|p| proj.rect.contains(*p))?;
        let w = crate::gizmo::on_plane(proj, at, plane.origin, plane.normal())?;
        let c = sketch_centre(app, proj, &plane, at, plane.to_local(w))?;
        return Some(plane.to_world(c));
    }
    let (body, index) = match (&app.tool, &app.viewport.hover) {
        (Some(t), Some(crate::viewport::Hit::Face { body, index, .. })) if t.cmd.starts_with("sketch.") => (body.clone(), *index),
        _ => match app.session.selection.as_slice() {
            [smpl3d_engine::Sel::Face { body, index, .. }] => (body.clone(), *index),
            _ => return None,
        },
    };
    face_facts(app, &body, index).centre.map(|c| c.1)
}

/// Draw the face's centre mark: a ring with a dot.
pub fn show_face_centre(app: &SolveApp, proj: &crate::viewport::Proj, painter: &egui::Painter) {
    let Some(c) = marked_point(app, proj, painter.ctx().pointer_hover_pos()) else { return };
    let Some(p) = proj.to_screen(c) else { return };
    crate::scenario::publish_handle("face_centre", p);
    let t = Tokens::get();
    painter.circle_stroke(p, 7.0, Stroke::new(3.5, Color32::from_black_alpha(140)));
    painter.circle_stroke(p, 7.0, Stroke::new(2.0, Color32::from_rgb(52, 199, 89)));
    painter.circle_filled(p, 2.0, t.text);
}

// ---------------------------------------------------------------------------------------------
// The selection bar (as Shapr3D): at the bottom centre, what is selected ("1 edge") and its
// measurements as chips. Click it to copy them.

fn selection_summary(app: &SolveApp) -> String {
    use smpl3d_engine::Sel;
    let mut counts: Vec<(&str, &str, usize)> = Vec::new();
    for s in &app.session.selection {
        let (one, many) = match s {
            Sel::Edge { .. } => ("edge", "edges"),
            Sel::Face { .. } => ("face", "faces"),
            Sel::Body { .. } => ("body", "bodies"),
            Sel::Vertex { .. } => ("point", "points"),
            Sel::Profile { .. } => ("area", "areas"),
            Sel::SketchCurve { .. } => ("sketch line", "sketch lines"),
            _ => ("item", "items"),
        };
        match counts.iter_mut().find(|c| c.0 == one) {
            Some(c) => c.2 += 1,
            None => counts.push((one, many, 1)),
        }
    }
    counts.iter().map(|(one, many, n)| format!("{n} {}", if *n == 1 { one } else { many })).collect::<Vec<_>>().join(", ")
}

pub fn selection_bar(app: &mut SolveApp, ui: &egui::Ui, view: Rect, rows: &[(String, String)]) -> Option<Rect> {
    isolate_banner(app, ui, view);
    if app.session.selection.is_empty() || app.home.open {
        return None;
    }
    let t = Tokens::get();
    let painter = ui.painter().clone();
    let font = egui::FontId::proportional(15.0);
    let summary = selection_summary(app);
    let chips: Vec<String> = rows.iter().map(|(l, v)| if l.is_empty() { v.clone() } else { format!("{l}  {v}") }).collect();
    let pad = 10.0;
    let gap = 8.0;
    let h = 40.0;
    let head = painter.layout_no_wrap(summary.clone(), font.clone(), t.text);
    let galleys: Vec<_> = chips.iter().map(|c| painter.layout_no_wrap(c.clone(), font.clone(), t.text)).collect();
    let bodies = selected_bodies(app);
    let iso = painter.layout_no_wrap("Isolate".into(), font.clone(), Color32::WHITE);
    let iso_w = if bodies.is_empty() || isolated() { 0.0 } else { iso.size().x + 20.0 + gap };
    let width = pad * 2.0 + head.size().x + galleys.iter().map(|g| g.size().x + 20.0 + gap).sum::<f32>() + gap + iso_w;
    let r = Rect::from_center_size(pos2(view.center().x, view.bottom() - 64.0 - h / 2.0), vec2(width, h));
    if !view.contains_rect(r) {
        return None;
    }
    let response = ui.interact(r, egui::Id::new("sc_quick_measure"), Sense::click()).on_hover_text("Click to copy");
    crate::scenario::publish_rect("measure:quick", r);
    painter.rect_filled(r.translate(vec2(0.0, 2.0)), 12.0, Color32::from_black_alpha(60));
    painter.rect_filled(r, 12.0, if response.hovered() { t.hover } else { t.toolbar });
    painter.rect_stroke(r, 12.0, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
    let mut x = r.left() + pad;
    let y = r.center().y;
    painter.galley(pos2(x, y - head.size().y / 2.0), head.clone(), t.text);
    x += head.size().x + gap;
    for g in galleys {
        let chip = Rect::from_min_size(pos2(x, r.top() + 6.0), vec2(g.size().x + 20.0, h - 12.0));
        painter.rect_filled(chip, 8.0, t.text.gamma_multiply(0.08));
        painter.galley(pos2(chip.left() + 10.0, y - g.size().y / 2.0), g.clone(), t.text);
        x += chip.width() + gap;
    }
    if iso_w > 0.0 {
        let chip = Rect::from_min_size(pos2(x, r.top() + 6.0), vec2(iso.size().x + 20.0, h - 12.0));
        let resp = ui.interact(chip, egui::Id::new("smpl3d_isolate"), Sense::click()).on_hover_text("Show only the selected bodies");
        crate::scenario::publish_target("smpl3d:isolate", chip);
        painter.rect_filled(chip, 8.0, if resp.hovered() { t.accent.gamma_multiply(0.85) } else { t.accent });
        painter.galley(pos2(chip.left() + 10.0, y - iso.size().y / 2.0), iso, Color32::WHITE);
        if resp.clicked() {
            isolate(app, bodies);
            return Some(r);
        }
    }
    if response.clicked() {
        let text = std::iter::once(summary).chain(chips).collect::<Vec<_>>().join("\n");
        ui.ctx().copy_text(text);
        app.set_status("Measurements copied", false);
    }
    Some(r)
}

// ---------------------------------------------------------------------------------------------
// Isolate (as Shapr3D): show only the selected bodies to work on them; Exit Isolate brings back
// the bodies it hid (bodies hidden before stay hidden).

thread_local! {
    static ISOLATED: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
}

fn isolated() -> bool {
    ISOLATED.with(|i| i.borrow().is_some())
}

/// The bodies the selection is on (bodies, or the bodies of faces, edges and points).
fn selected_bodies(app: &SolveApp) -> Vec<String> {
    use smpl3d_engine::Sel;
    let mut out: Vec<String> = Vec::new();
    for s in &app.session.selection {
        let name = match s {
            Sel::Body { name } => name.clone(),
            Sel::Face { body, .. } | Sel::Edge { body, .. } | Sel::Vertex { body, .. } if !body.is_empty() => body.clone(),
            _ => continue,
        };
        if !out.contains(&name) {
            out.push(name);
        }
    }
    out
}

pub fn isolate(app: &mut SolveApp, bodies: Vec<String>) {
    if bodies.is_empty() {
        return;
    }
    let hidden = &app.session.visibility.hidden_bodies;
    let others: Vec<String> =
        app.session.world_state().bodies.iter().map(|b| b.name.clone()).filter(|b| !bodies.contains(b) && !hidden.contains(b)).collect();
    if !others.is_empty() {
        let _ = app.run("browser.visibility", serde_json::json!({ "items": others, "visible": false }));
    }
    ISOLATED.with(|i| *i.borrow_mut() = Some(others));
}

/// An export started while isolated writes only the bodies still shown: `params` with those
/// bodies named, when the export names none itself. Other commands, an export that already names
/// its bodies, and the formats that hold the whole design (JSON, URDF) pass through unchanged.
pub fn export_only_isolated(app: &SolveApp, id: &str, mut params: serde_json::Value) -> serde_json::Value {
    const EXPORTS: [&str; 7] =
        ["file.export", "file.export_step", "file.export_stl", "file.export_3mf", "file.export_obj", "file.save_mesh", "file.send_to_slicer"];
    if !EXPORTS.contains(&id) || !isolated() || !params.is_object() {
        return params;
    }
    let named = params.get("bodies").and_then(serde_json::Value::as_array).is_some_and(|b| !b.is_empty());
    let whole = |key: &str, values: &[&str]| params.get(key).and_then(serde_json::Value::as_str).is_some_and(|v| values.iter().any(|x| v.to_ascii_lowercase().ends_with(x)));
    if named || whole("format", &["json", "design", "urdf"]) || whole("path", &[".json", ".urdf", ".sc"]) {
        return params;
    }
    let shown: Vec<String> = app.session.world_state().bodies.iter().map(|b| b.name.clone()).filter(|b| app.session.body_visible(b)).collect();
    if !shown.is_empty() {
        params["bodies"] = serde_json::json!(shown);
    }
    params
}

pub fn exit_isolate(app: &mut SolveApp) {
    let Some(others) = ISOLATED.with(|i| i.borrow_mut().take()) else { return };
    if !others.is_empty() {
        let _ = app.run("browser.visibility", serde_json::json!({ "items": others, "visible": true }));
    }
}

/// While isolated: a banner at the top centre with Exit Isolate (Esc too, with nothing running).
fn isolate_banner(app: &mut SolveApp, ui: &egui::Ui, view: Rect) {
    if !isolated() {
        return;
    }
    let t = Tokens::get();
    let painter = ui.painter().clone();
    let font = egui::FontId::proportional(15.0);
    let text = painter.layout_no_wrap("Isolated".into(), font.clone(), t.text);
    let exit = painter.layout_no_wrap("Exit Isolate".into(), font, Color32::WHITE);
    let h = 40.0;
    let w = 12.0 + text.size().x + 12.0 + exit.size().x + 20.0 + 6.0;
    let r = Rect::from_center_size(pos2(view.center().x, view.top() + 16.0 + h / 2.0), vec2(w, h));
    painter.rect_filled(r, 12.0, t.toolbar);
    painter.rect_stroke(r, 12.0, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
    painter.galley(pos2(r.left() + 12.0, r.center().y - text.size().y / 2.0), text.clone(), t.text);
    let chip = Rect::from_min_size(pos2(r.left() + 24.0 + text.size().x, r.top() + 6.0), vec2(exit.size().x + 20.0, h - 12.0));
    let resp = ui.interact(chip, egui::Id::new("smpl3d_exit_isolate"), Sense::click());
    crate::scenario::publish_target("smpl3d:exit_isolate", chip);
    painter.rect_filled(chip, 8.0, if resp.hovered() { t.accent.gamma_multiply(0.85) } else { t.accent });
    painter.galley(pos2(chip.left() + 10.0, r.center().y - exit.size().y / 2.0), exit, Color32::WHITE);
    let esc = app.tool.is_none() && app.dialog.is_none() && ui.input(|i| i.key_pressed(egui::Key::Escape)) && app.session.selection.is_empty();
    if resp.clicked() || esc {
        exit_isolate(app);
    }
}

// ---------------------------------------------------------------------------------------------
// The Move and Scale value box (as Shapr3D): one box with a Move | Scale switch, and Copy beside
// it while moving.

/// Set by the canvas for this frame's Move or Scale box: moving (or scaling), the bodies, and
/// Copy's state (moving only).
struct BoxMode {
    moving: bool,
    bodies: Vec<smpl3d_engine::Sel>,
    copy: bool,
    copy_changed: Option<bool>,
}

thread_local! {
    static BOX_MODE: RefCell<Option<BoxMode>> = const { RefCell::new(None) };
}

/// This frame's box is Move's (`moving`) or Scale's, on these bodies; `None` clears it.
pub fn set_box_mode(mode: Option<(bool, Vec<smpl3d_engine::Sel>, bool)>) {
    BOX_MODE.with(|m| *m.borrow_mut() = mode.map(|(moving, bodies, copy)| BoxMode { moving, bodies, copy, copy_changed: None }));
}

/// Copy, when the box's Copy pill was clicked this frame.
pub fn take_copy_change() -> Option<bool> {
    BOX_MODE.with(|m| m.borrow_mut().as_mut().and_then(|b| b.copy_changed.take()))
}

/// The switch and Copy pill, drawn in the value box's row.
pub fn box_switch(ui: &mut egui::Ui) {
    let Some((moving, copy)) = BOX_MODE.with(|m| m.borrow().as_ref().map(|b| (b.moving, b.copy))) else { return };
    let t = crate::theme::Tokens::get();
    ui.add_space(4.0);
    let pill = |ui: &mut egui::Ui, text: &str, on: bool, name: &str| {
        let txt = egui::RichText::new(crate::i18n::tr_label(text)).size(t.font_caption).color(if on { t.on_accent } else { t.text });
        let b = egui::Button::new(txt).fill(if on { t.accent } else { egui::Color32::TRANSPARENT }).corner_radius(10.0).min_size(egui::vec2(0.0, 22.0));
        let r = ui.add(b);
        crate::scenario::publish_target(name, r.rect);
        r.clicked()
    };
    egui::Frame::new().fill(t.panel).corner_radius(12.0).inner_margin(egui::Margin::same(2)).show(ui, |ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        let go_move = pill(ui, "Move", moving, "box:move") && !moving;
        let go_scale = pill(ui, "Scale", !moving, "box:scale") && moving;
        if go_move || go_scale {
            let bodies = BOX_MODE.with(|m| m.borrow().as_ref().map(|b| b.bodies.clone()).unwrap_or_default());
            switch_from_move(if go_move { "solid.move" } else { "solid.scale" }, bodies);
        }
    });
    if moving && pill(ui, "Copy", copy, "box:copy") {
        BOX_MODE.with(|m| {
            if let Some(b) = m.borrow_mut().as_mut() {
                b.copy = !copy;
                b.copy_changed = Some(!copy);
            }
        });
    }
}

// ---------------------------------------------------------------------------------------------
// The brand mark in the application bar: Smpl3D's own icon (the Dock icon).

const MARK_PNG: &[u8] = include_bytes!("../../../assets/branding/smpl3d-mark.png");

/// Draw the app icon, `size` points square, centred on `centre`.
pub fn brand_mark(ui: &egui::Ui, centre: egui::Pos2, size: f32) {
    let id = egui::Id::new("smpl3d_brand_mark");
    let tex = ui.ctx().data(|d| d.get_temp::<egui::TextureHandle>(id)).or_else(|| {
        let img = image::load_from_memory(MARK_PNG).ok()?.to_rgba8();
        let (w, h) = (img.width() as usize, img.height() as usize);
        let ci = egui::ColorImage::from_rgba_unmultiplied([w, h], img.as_raw());
        let t = ui.ctx().load_texture("smpl3d_brand_mark", ci, egui::TextureOptions::LINEAR);
        ui.ctx().data_mut(|d| d.insert_temp(id, t.clone()));
        Some(t)
    });
    if let Some(t) = tex {
        let r = Rect::from_center_size(centre, vec2(size, size));
        ui.painter().image(t.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    }
}
