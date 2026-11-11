//! Convex polyhedra from planes, and the operations that use them on convex planar bodies:
//! shell (hollow out with open faces) and draft (tilt faces about a neutral plane). The solid
//! is built directly from its planes (vertices are triple-plane intersections), so it is exact
//! and needs no booleans except the final shell subtraction.

use std::collections::HashMap;

use smpl3d_geom::{Plane, Vec3};
use truck_modeling::{self as mt, builder};

use crate::body::{Body, Solid, from_p3, p3, v3};
use crate::history::{Change, EdgeOrigin, FaceOrigin, FaceProv, History};
use crate::record;
use crate::{KernelError, Result, guard};

/// A half-space `n · x <= d` (n is the outward unit normal).
#[derive(Clone, Copy, Debug)]
pub struct HalfSpace {
    pub n: Vec3,
    pub d: f64,
}

fn det3(a: Vec3, b: Vec3, c: Vec3) -> f64 {
    a.dot(b.cross(c))
}

/// Vertices, and per half-space with a face its vertex loop (counter-clockwise about the
/// outward normal), of the convex polyhedron bounded by the half-spaces.
fn combinatorics(hs: &[HalfSpace]) -> Result<(Vec<Vec3>, Vec<(usize, Vec<usize>)>)> {
    if hs.len() < 4 || hs.len() > 200 {
        return Err(KernelError::Invalid("a polyhedron needs 4…200 planes".into()));
    }
    let scale = hs.iter().map(|h| h.d.abs()).fold(1.0, f64::max);
    let eps = scale * 1e-9;
    // Vertices: intersections of three planes inside all others.
    let mut verts: Vec<Vec3> = Vec::new();
    for i in 0..hs.len() {
        for j in (i + 1)..hs.len() {
            for k in (j + 1)..hs.len() {
                let (a, b, c) = (hs[i], hs[j], hs[k]);
                let den = det3(a.n, b.n, c.n);
                if den.abs() < 1e-12 {
                    continue;
                }
                let x = (b.n.cross(c.n) * a.d + c.n.cross(a.n) * b.d + a.n.cross(b.n) * c.d) / den;
                if !x.is_finite() || hs.iter().any(|h| h.n.dot(x) > h.d + eps * 10.0) {
                    continue;
                }
                if !verts.iter().any(|v| v.dist(x) < scale * 1e-8) {
                    verts.push(x);
                }
            }
        }
    }
    if verts.len() < 4 {
        return Err(KernelError::Invalid("the planes don't enclose a solid".into()));
    }
    // Faces: the vertices on each plane, ordered counter-clockwise about its outward normal.
    let mut faces_idx: Vec<(usize, Vec<usize>)> = Vec::new();
    for (hi, h) in hs.iter().enumerate() {
        let on: Vec<usize> = (0..verts.len()).filter(|i| verts.get(*i).is_some_and(|v| (h.n.dot(*v) - h.d).abs() < scale * 1e-7)).collect();
        if on.len() < 3 {
            continue;
        }
        let c = on.iter().filter_map(|i| verts.get(*i)).fold(Vec3::ZERO, |a, v| a + *v) / on.len() as f64;
        let u = h.n.any_perp();
        let w = h.n.cross(u);
        let mut ordered = on.clone();
        ordered.sort_by(|a, b| {
            let ang = |i: usize| verts.get(i).map(|v| (*v - c).dot(w).atan2((*v - c).dot(u))).unwrap_or(0.0);
            ang(*a).total_cmp(&ang(*b))
        });
        faces_idx.push((hi, ordered));
    }
    Ok((verts, faces_idx))
}

/// The convex polyhedron bounded by the half-spaces.
pub fn convex_polyhedron(hs: &[HalfSpace]) -> Result<Body> {
    polyhedron(hs).map(|(b, _)| b)
}

/// [`convex_polyhedron`], with the half-space each of its faces lies on (in face order).
fn polyhedron(hs: &[HalfSpace]) -> Result<(Body, Vec<usize>)> {
    let (verts, faces_idx) = combinatorics(hs)?;
    let on: Vec<usize> = faces_idx.iter().map(|(hi, _)| *hi).collect();
    guard("polyhedron", || {
        let tv: Vec<mt::Vertex> = verts.iter().map(|v| builder::vertex(p3(*v))).collect();
        let mut edges: HashMap<(usize, usize), mt::Edge> = HashMap::new();
        let mut faces: Vec<mt::Face> = Vec::new();
        for (_, f) in &faces_idx {
            let n = f.len();
            let mut wire: Vec<mt::Edge> = Vec::with_capacity(n);
            for k in 0..n {
                let (Some(&a), Some(&b)) = (f.get(k), f.get((k + 1) % n)) else { continue };
                let key = (a.min(b), a.max(b));
                let e = match edges.get(&key) {
                    Some(e) => e.clone(),
                    None => {
                        let (Some(va), Some(vb)) = (tv.get(key.0), tv.get(key.1)) else { continue };
                        let e = builder::line(va, vb);
                        edges.insert(key, e.clone());
                        e
                    }
                };
                wire.push(if a < b { e } else { e.inverse() });
            }
            let face = builder::try_attach_plane(&[wire.into()]).map_err(|e| KernelError::Failed(format!("polyhedron face: {e}")))?;
            faces.push(face);
        }
        let shell: mt::Shell = faces.into();
        let solid = Solid::try_new(vec![shell]).map_err(|e| KernelError::Failed(format!("polyhedron: {e}")))?;
        Ok((Body::new(solid)?, on))
    })
}

/// The half-spaces of a convex body with only planar faces, each with a point on it and the
/// body's faces on its plane (in face order), or an error.
pub fn planar_convex(b: &Body) -> Result<Vec<(HalfSpace, Vec3, Vec<usize>)>> {
    b.require_brep("this operation")?;
    let tol = (b.size() * 1e-3).max(1e-3);
    let faces = b.faces(tol)?;
    let mut out: Vec<(HalfSpace, Vec3, Vec<usize>)> = Vec::new();
    for f in &faces {
        let n = f.plane_normal.ok_or_else(|| KernelError::Failed("not supported yet: bodies with curved faces".into()))?;
        let d = n.dot(f.centroid);
        match out.iter_mut().find(|(h, ..)| h.n.dot(n) > 1.0 - 1e-9 && (h.d - d).abs() < tol) {
            Some((.., on)) => on.push(f.index),
            None => out.push((HalfSpace { n, d }, f.centroid, vec![f.index])),
        }
    }
    let verts: Vec<Vec3> = b.solid.vertex_iter().map(|v| from_p3(v.point())).collect();
    for (h, ..) in &out {
        if verts.iter().any(|v| h.n.dot(*v) > h.d + tol) {
            return Err(KernelError::Failed("not supported yet: shell and draft of non-convex bodies".into()));
        }
    }
    Ok(out)
}

/// The faces of `b` on half-space `k`'s plane (`groups[k]`) as the face over them: moved, or
/// merged when several.
fn on_plane(under: &[usize]) -> FaceProv {
    record::region(under.iter().map(|&face| FaceOrigin::Face { operand: 0, face }).collect(), false)
}

/// The edges of `b` between faces on two of its planes (`groups[k]`: its faces on plane `k`),
/// by pair of planes (the lower first).
fn plane_edges(b: &Body, groups: &[Vec<usize>]) -> HashMap<(usize, usize), Vec<usize>> {
    let plane_of: HashMap<usize, usize> = groups.iter().enumerate().flat_map(|(k, g)| g.iter().map(move |f| (*f, k))).collect();
    let mut out: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
    for (edge, fs) in crate::history::edge_faces(&b.solid).iter().enumerate() {
        if let [p, q] = fs[..]
            && let (Some(&x), Some(&y)) = (plane_of.get(&p), plane_of.get(&q))
            && x != y
        {
            out.entry((x.min(y), x.max(y))).or_default().push(edge);
        }
    }
    out
}

/// The history over `[b]` of a convex polyhedron `poly` whose face `j` lies on the plane of
/// half-space `on[j]`, made from the faces `groups[k]` of `b` on half-space `k`'s plane: each
/// face the faces under it, each edge along the edges of `b` between faces of its two planes,
/// or new where they met nowhere.
fn planes_history(poly: &Body, on: &[usize], groups: &[Vec<usize>], b: &Body) -> Option<History> {
    let faces: Vec<FaceProv> = on.iter().map(|k| groups.get(*k).map(|under| on_plane(under))).collect::<Option<_>>()?;
    let along = plane_edges(b, groups);
    let mut h = record::from_faces(&poly.solid, 1, faces);
    for e in &mut h.edges {
        if let [x, y] = e.faces[..]
            && let (Some(&kx), Some(&ky)) = (on.get(x), on.get(y))
            && let Some(edges) = along.get(&(kx.min(ky), kx.max(ky)))
        {
            e.origins = edges.iter().map(|&edge| EdgeOrigin::Edge { operand: 0, edge }).collect();
        }
    }
    Some(h)
}

/// The history over `[b]` of a cavity `poly` whose face `j` lies on half-space `on[j]`, moved
/// in from the faces `groups[k]` of `b`: each face an offset of the faces it lines, each edge new.
fn cavity_history(poly: &Body, on: &[usize], groups: &[Vec<usize>]) -> Option<History> {
    let faces = on
        .iter()
        .map(|k| groups.get(*k).map(|under| record::generated(under.iter().map(|&face| FaceOrigin::Offset { operand: 0, face }).collect())))
        .collect::<Option<_>>()?;
    Some(record::from_faces(&poly.solid, 1, faces))
}

/// Hollow a convex planar body: walls of `thickness` inside, faces near `open` removed. The
/// result's history is over `[b]`: the inner walls are offsets of the faces they line.
pub fn shell(b: &Body, open: &[Vec3], thickness: f64) -> Result<Body> {
    guard("shell", || shell_impl(b, open, thickness)?.with_shell_planar_sources(b, thickness, open))
}

fn shell_impl(b: &Body, open: &[Vec3], thickness: f64) -> Result<Body> {
    b.require_brep("shell")?;
    if !(thickness.is_finite() && thickness > 1e-6) {
        return Err(KernelError::Invalid("shell thickness must be positive".into()));
    }
    let hs = match planar_convex(b) {
        Ok(h) => h,
        Err(_) => return shell_planar(b, open, thickness),
    };
    let size = b.size();
    let inner: Vec<HalfSpace> = hs
        .iter()
        .map(|(h, c, _)| {
            let removed = open.iter().any(|p| (h.n.dot(*p) - h.d).abs() < size * 1e-4 + 1e-6 && p.dist(*c) < size * 2.0);
            if removed { HalfSpace { n: h.n, d: h.d + thickness + size * 0.05 } } else { HalfSpace { n: h.n, d: h.d - thickness } }
        })
        .collect();
    if !hs.iter().zip(&inner).any(|((h, ..), i)| i.d > h.d) {
        return Err(KernelError::Invalid("select at least one face to remove".into()));
    }
    let (cavity, on) = polyhedron(&inner)?;
    let groups: Vec<Vec<usize>> = hs.into_iter().map(|(.., under)| under).collect();
    let walls = cavity_history(&cavity, &on, &groups);
    let r = crate::ops::boolean(b, &cavity, crate::BoolOp::Cut)?.ok_or_else(|| KernelError::Failed("the shell removed everything".into()))?;
    Ok(hollowed(r, b, walls))
}

/// `r`, `b` less a cavity whose history over `[b]` is `walls`, with its history over `[b]`.
fn hollowed(r: Body, b: &Body, walls: Option<History>) -> Body {
    let h = crate::boolean_record::of_pair(&r, &crate::boolean_record::kept(b, 0, 1), &walls);
    r.with_checked_history(h, &record::counts(&[b]))
}

/// The history over `[b]` of a cavity `offset` made from `b` by moving its faces (whose history
/// over `[b]` says so): each face an offset of the face it was moved from, each edge new.
fn offsets(offset: &Body) -> Option<History> {
    let h = offset.history()?;
    let faces: Vec<FaceProv> = h
        .faces
        .iter()
        .map(|f| match f.origins.as_slice() {
            [FaceOrigin::Face { operand, face }] => Some(record::generated(vec![FaceOrigin::Offset { operand: *operand, face: *face }])),
            _ => None,
        })
        .collect::<Option<_>>()?;
    Some(record::from_faces(&offset.solid, h.operands, faces))
}

/// Tilt the faces near `faces` by `angle` about their line on the neutral plane, so that they
/// lean in (positive angle) going along `pull`.
pub fn draft(b: &Body, faces: &[Vec3], neutral: &Plane, pull: Vec3, angle: f64) -> Result<Body> {
    b.require_brep("draft")?;
    if !angle.is_finite() || angle.abs() >= std::f64::consts::FRAC_PI_2 - 1e-3 {
        return Err(KernelError::Invalid("draft angle must be between −90° and 90°".into()));
    }
    let pull = pull.normalized().ok_or_else(|| KernelError::Invalid("pull direction".into()))?;
    let hs = match planar_convex(b) {
        Ok(h) => h,
        Err(_) => return draft_walls(b, faces, neutral, pull, angle),
    };
    let size = b.size();
    let mut out = Vec::new();
    let mut moved = 0;
    for (h, c, _) in &hs {
        let chosen = faces.iter().any(|p| (h.n.dot(*p) - h.d).abs() < size * 1e-4 + 1e-6 && p.dist(*c) < size * 2.0);
        if !chosen {
            out.push(*h);
            continue;
        }
        if h.n.dot(pull).abs() > 1e-6 {
            return Err(KernelError::Failed("not supported yet: drafting faces that aren't parallel to the pull direction".into()));
        }
        // Hinge: the face's line on the neutral plane; rotate the normal toward the pull.
        let n2 = (h.n * angle.cos() + pull * angle.sin()).normalized().ok_or_else(|| KernelError::Failed("draft".into()))?;
        let hinge_pt =
            neutral.intersect_ray(*c, pull).ok_or_else(|| KernelError::Failed("the neutral plane is parallel to the pull direction".into()))?;
        out.push(HalfSpace { n: n2, d: n2.dot(hinge_pt) });
        moved += 1;
    }
    if moved == 0 {
        return Err(KernelError::Invalid("no faces to draft".into()));
    }
    // Each half-space in place of the one it was made from: its faces moved (or as they were).
    let (r, on) = polyhedron(&out)?;
    let groups: Vec<Vec<usize>> = hs.into_iter().map(|(.., under)| under).collect();
    let h = planes_history(&r, &on, &groups, b);
    Ok(r.with_checked_history(h, &record::counts(&[b])))
}

/// Draft of a non-convex body: walls (planes, cylinders) next to caps square to `pull`.
fn draft_walls(b: &Body, at: &[Vec3], neutral: &Plane, pull: Vec3, angle: f64) -> Result<Body> {
    let healed = crate::boolean_record::healed(b)?;
    let size = healed.size();
    let mesh = healed.tessellate((size * 1e-3).max(1e-3))?;
    let mut chosen: Vec<usize> = Vec::new();
    for p in at {
        let near = mesh
            .triangles
            .iter()
            .zip(&mesh.tri_face)
            .filter_map(|(t, f)| mesh.tri(t).map(|[x, y, z]| (point_tri(*p, x, y, z), *f as usize)))
            .min_by(|a, c| a.0.total_cmp(&c.0));
        match near {
            Some((d, f)) if d < size * 1e-3 + 1e-6 => {
                if !chosen.contains(&f) {
                    chosen.push(f);
                }
            }
            _ => return Err(KernelError::Invalid(format!("no face at {:?}", [p.x, p.y, p.z]))),
        }
    }
    // A round wall is stored in pieces (a cylinder's two halves): picking one drafts the whole wall.
    let chosen = crate::offset::with_same_surface(&healed, &chosen);
    Ok(record::after(crate::offset::draft_walls(&healed, &chosen, neutral, pull, angle)?, &healed, &record::counts(&[b])))
}

/// Round every edge of a convex body with planar faces by `r`: faces shrink, each edge becomes a
/// cylinder and each corner a sphere patch (the body is the inner polyhedron grown by a ball).
/// The result's history is over `[b]`: each cylinder in place of the edges of `b` between its
/// two planes' faces, each corner patch in place of those round its corner.
pub fn round_all_edges(b: &Body, r: f64) -> Result<Body> {
    let planes = planar_convex(b)?;
    let hs: Vec<HalfSpace> = planes.iter().map(|(h, ..)| *h).collect();
    let groups: Vec<Vec<usize>> = planes.iter().map(|(.., under)| under.clone()).collect();
    // The edges of `b` between faces on planes f and g.
    let edges = plane_edges(b, &groups);
    let along = |f: usize, g: usize| -> Vec<FaceOrigin> {
        edges.get(&(f.min(g), f.max(g))).into_iter().flatten().map(|&edge| FaceOrigin::Edge { operand: 0, edge }).collect()
    };
    let inner: Vec<HalfSpace> = hs.iter().map(|h| HalfSpace { n: h.n, d: h.d - r }).collect();
    let (q, faces) = combinatorics(&inner)?;
    if faces.len() != hs.len() {
        return Err(KernelError::Failed("not supported yet: the rounding is larger than a face".into()));
    }
    let normal = |hi: usize| hs.get(hi).map(|h| h.n).unwrap_or(Vec3::Z);
    // Edges of the inner polyhedron: (vertex a, vertex b) → (face left, face right).
    let mut edge_faces: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
    for (hi, lp) in &faces {
        let n = lp.len();
        for k in 0..n {
            let (Some(&a), Some(&bb)) = (lp.get(k), lp.get((k + 1) % n)) else { continue };
            edge_faces.entry((a.min(bb), a.max(bb))).or_default().push(*hi);
        }
    }
    if edge_faces.values().any(|f| f.len() != 2) {
        return Err(KernelError::Failed("not supported yet: rounding this shape".into()));
    }
    guard("round edges", || {
        // Vertex of face `hi` at inner vertex `qi`.
        let mut verts: HashMap<(usize, usize), mt::Vertex> = HashMap::new();
        let mut vert = |hi: usize, qi: usize| -> mt::Vertex {
            verts.entry((hi, qi)).or_insert_with(|| builder::vertex(p3(q.get(qi).copied().unwrap_or_default() + normal(hi) * r))).clone()
        };
        let mut lines: HashMap<(usize, usize, usize), mt::Edge> = HashMap::new();
        let mut arcs: HashMap<(usize, usize, usize), mt::Edge> = HashMap::new();
        // Straight edge of face hi between inner vertices a and b (stored low → high).
        let mut line = |hi: usize, a: usize, bb: usize, vert: &mut dyn FnMut(usize, usize) -> mt::Vertex| -> mt::Edge {
            let (lo, hi_v) = (a.min(bb), a.max(bb));
            let e = lines.entry((hi, lo, hi_v)).or_insert_with(|| builder::line(&vert(hi, lo), &vert(hi, hi_v))).clone();
            if a < bb { e } else { e.inverse() }
        };
        // Arc at inner vertex qi from face f to face g (stored with f < g).
        let mut arc = |qi: usize, f: usize, g: usize, vert: &mut dyn FnMut(usize, usize) -> mt::Vertex| -> mt::Edge {
            let (lo, hi_f) = (f.min(g), f.max(g));
            let c = q.get(qi).copied().unwrap_or_default();
            let e = arcs
                .entry((qi, lo, hi_f))
                .or_insert_with(|| {
                    let mid = c + (normal(lo) + normal(hi_f)).normalized().unwrap_or(Vec3::Z) * r;
                    builder::circle_arc(&vert(lo, qi), &vert(hi_f, qi), p3(mid))
                })
                .clone();
            if f < g { e } else { e.inverse() }
        };
        let mut out: Vec<mt::Face> = Vec::new();
        // What each face is made from (in `out`'s order).
        let mut made: Vec<FaceProv> = Vec::new();
        // Planar faces.
        for (hi, lp) in &faces {
            let n = lp.len();
            let mut w: Vec<mt::Edge> = Vec::new();
            for k in 0..n {
                let (Some(&a), Some(&bb)) = (lp.get(k), lp.get((k + 1) % n)) else { continue };
                w.push(line(*hi, a, bb, &mut vert));
            }
            out.push(builder::try_attach_plane(&[w.into()]).map_err(|e| KernelError::Failed(format!("rounded face: {e}")))?);
            made.push(on_plane(groups.get(*hi).map_or(&[], Vec::as_slice)));
        }
        use mt::{ParametricSurface3D, SearchNearestParameter};
        let orient = |mut surf: mt::Surface, at: Vec3, outward: Vec3| -> mt::Surface {
            if let Some((u, v)) = surf.search_nearest_parameter(p3(at), None, 100) {
                let nn = surf.normal(u, v);
                if Vec3::new(nn.x, nn.y, nn.z).dot(outward) < 0.0 {
                    surf = mt::Invertible::inverse(&surf);
                }
            }
            surf
        };
        // Edge cylinders: face f's loop runs a → b, so the cylinder (to f's right) runs b → a
        // along f, then across to g.
        for ((a, bb), fs) in &edge_faces {
            let (Some(&f0), Some(&f1)) = (fs.first(), fs.get(1)) else { continue };
            // Which face runs a → b?
            let runs = |hi: usize| {
                faces.iter().find(|(h, _)| *h == hi).is_some_and(|(_, lp)| {
                    let n = lp.len();
                    (0..n).any(|k| lp.get(k) == Some(a) && lp.get((k + 1) % n) == Some(bb))
                })
            };
            let (f, g) = if runs(f0) { (f0, f1) } else { (f1, f0) };
            let w: Vec<mt::Edge> =
                vec![line(f, *bb, *a, &mut vert), arc(*a, f, g, &mut vert), line(g, *a, *bb, &mut vert), arc(*bb, g, f, &mut vert)];
            let (qa, qb) = (q.get(*a).copied().unwrap_or_default(), q.get(*bb).copied().unwrap_or_default());
            let profile = arc(*a, f, g, &mut vert);
            let swept = builder::tsweep(&profile, v3(qb - qa));
            let out_dir = (normal(f) + normal(g)).normalized().unwrap_or(Vec3::Z);
            let mid = (qa + qb) * 0.5 + out_dir * r;
            let surf = orient(swept.oriented_surface(), mid, out_dir);
            out.push(mt::Face::try_new(vec![w.into()], surf).map_err(|e| KernelError::Failed(format!("rounded edge: {e}")))?);
            made.push(record::generated(along(f, g)));
        }
        // Corner sphere patches: the faces around each inner vertex, counter-clockwise.
        for (qi, c) in q.iter().enumerate() {
            let mut around: Vec<usize> = faces.iter().filter(|(_, lp)| lp.contains(&qi)).map(|(h, _)| *h).collect();
            let axis = around.iter().fold(Vec3::ZERO, |acc, h| acc + normal(*h)).normalized().unwrap_or(Vec3::Z);
            let u = axis.any_perp();
            let w2 = axis.cross(u);
            around.sort_by(|x, y| {
                let ang = |h: usize| normal(h).dot(w2).atan2(normal(h).dot(u));
                ang(*x).total_cmp(&ang(*y))
            });
            let k = around.len();
            let w: Vec<mt::Edge> = (0..k).filter_map(|i| Some(arc(qi, *around.get(i)?, *around.get((i + 1) % k)?, &mut vert))).collect();
            // A sphere surface whose poles and seam are away from this patch.
            let side = axis.any_perp();
            let (np, sp, back) = (*c + side * r, *c - side * r, *c - axis * r);
            let meridian = builder::circle_arc(&builder::vertex(p3(np)), &builder::vertex(p3(sp)), p3(back));
            let surf =
                mt::Surface::RevolutedCurve(mt::Processor::new(mt::RevolutedCurve::by_revolution(meridian.oriented_curve(), p3(*c), v3(side))));
            let surf = orient(surf, *c + axis * r, axis);
            out.push(mt::Face::try_new(vec![w.into()], surf).map_err(|e| KernelError::Failed(format!("rounded corner: {e}")))?);
            let mut corner: Vec<FaceOrigin> =
                (0..k).filter_map(|i| Some((*around.get(i)?, *around.get((i + 1) % k)?))).flat_map(|(f, g)| along(f, g)).collect();
            corner.sort();
            corner.dedup();
            made.push(record::generated(corner));
        }
        let shell: mt::Shell = out.into();
        let solid = Solid::try_new(vec![shell]).map_err(|e| KernelError::Failed(format!("rounded body: {e}")))?;
        let body = Body::new(solid)?;
        // Every face from something of `b` (a blend between planes that met nowhere has none).
        let history = made.iter().all(|f| !f.origins.is_empty()).then(|| record::from_faces(&body.solid, 1, made));
        Ok(body.with_checked_history(history, &record::counts(&[b])))
    })
}

/// Shell of a body whose faces are all planar (convex or not): every face plane moves inward
/// by the thickness (open faces outward, past the body) keeping the body's topology, which
/// gives the cavity; the shell is the body minus the cavity.
fn shell_planar(b: &Body, open: &[Vec3], thickness: f64) -> Result<Body> {
    // Face paint and authored splits are construction boundaries. Retain their
    // eligibility veto through the temporary body used for the cavity Cut.
    // Paint indices stay in source order by avoiding healing in that case.
    let healed =
        if b.paint().is_some_and(|paint| !paint.faces.is_empty()) { crate::boolean_record::copied(b)? } else { crate::boolean_record::healed(b)? };
    let healed = healed.with_paint(b.paint().cloned()).with_split_lines(b.split_lines().cloned()).inherit_regularization_protection(b);
    let size = healed.size();
    let mesh = healed.tessellate((size * 1e-3).max(1e-3))?;
    // Faces to open: the face of the triangle nearest each point.
    let mut opened: Vec<usize> = Vec::new();
    for p in open {
        let near = mesh
            .triangles
            .iter()
            .zip(&mesh.tri_face)
            .filter_map(|(t, f)| mesh.tri(t).map(|[x, y, z]| (point_tri(*p, x, y, z), *f as usize)))
            .min_by(|a, c| a.0.total_cmp(&c.0));
        if let Some((d, f)) = near
            && d < size * 1e-3 + 1e-6
            && !opened.contains(&f)
        {
            opened.push(f);
        }
    }
    if opened.is_empty() {
        return Err(KernelError::Invalid("select at least one face to remove".into()));
    }
    // A successful Boolean is not evidence that a tangential rim has wall thickness.
    // Refuse only when the entire planar opening loses its rim. A single collapsed
    // round can still end in a valid sharp edge (the tutorial plate, for example).
    if let Some(r) = rim_without_thickness(&healed, &opened, thickness, true) {
        return Err(unsupported_shell(&format!(
            "the rim would have no thickness: an open face meets a round of radius {r:.4} mm tangentially, and the shell is {thickness:.4} mm thick (make the round smaller than the thickness)"
        )));
    }
    if let Some(shell) = shell_barrel(&healed, &opened, thickness)? {
        return Ok(record::after(shell, &healed, &record::counts(&[b])));
    }
    let margin = thickness + size * 0.05;
    shell_opened(b, &healed, open, &opened, thickness, margin).map_err(|e| match rim_without_thickness(&healed, &opened, thickness, false) {
        Some(r) => unsupported_shell(&format!(
            "the rim would have no thickness: an open face meets a round of radius {r:.4} mm tangentially, and the shell is {thickness:.4} mm thick (make the round smaller than the thickness)"
        )),
        None => e,
    })
}

/// A convex circular-meridian barrel between two axial caps. Sew its exact inset walls
/// to the original walls at annular rims: a Boolean of the two spindle sheets cannot
/// reliably find their cap intersections, and sweeping an inset cap makes a cylindrical
/// lip that cuts away part of the curved wall.
fn shell_barrel(b: &Body, opened: &[usize], thickness: f64) -> Result<Option<Body>> {
    use crate::offset::Surf;
    if b.solid.boundaries().len() != 1 || b.face_count() > 256 {
        return Ok(None);
    }
    let faces: Vec<mt::Face> = b.solid.face_iter().cloned().collect();
    let tol = (b.size() * 1e-7).max(1e-9) * 100.0;
    let surfs: Option<Vec<Surf>> = faces.iter().map(|f| crate::offset::surf_of(f, tol)).collect();
    let Some(surfs) = surfs else { return Ok(None) };
    let Some((centre, axis, big, radius)) = surfs.iter().find_map(|s| match *s {
        Surf::Torus { o, a, big, h, r, convex: true } if big < -tol => Some((o + a * h, a, big, r)),
        _ => None,
    }) else {
        return Ok(None);
    };
    let mut caps = Vec::new();
    for (i, s) in surfs.iter().enumerate() {
        match *s {
            Surf::Torus { o, a, big: ring, h, r, convex: true }
                if (o + a * h).dist(centre) < tol && a.dot(axis).abs() > 1.0 - 1e-9 && (ring - big).abs() < tol && (r - radius).abs() < tol => {}
            Surf::Plane { n, d } if n.dot(axis).abs() > 1.0 - 1e-9 && faces.get(i).is_some_and(|f| f.boundaries().len() == 1) => {
                caps.push((i, (d - n.dot(centre)) / n.dot(axis), n.dot(axis)));
            }
            _ => return Ok(None),
        }
    }
    if caps.len() != 2 || opened.iter().any(|i| !caps.iter().any(|c| c.0 == *i)) {
        return Ok(None);
    }
    caps.sort_by(|a, b| a.1.total_cmp(&b.1));
    let (Some(&(lower, lo, down)), Some(&(upper, hi, up))) = (caps.first(), caps.get(1)) else { return Ok(None) };
    if down >= 0.0 || up <= 0.0 {
        return Ok(None);
    }
    let lo = lo + if opened.contains(&lower) { 0.0 } else { thickness };
    let hi = hi - if opened.contains(&upper) { 0.0 } else { thickness };
    let inner_radius = radius - thickness;
    if lo >= hi || inner_radius <= 0.0 || [lo, hi].iter().any(|z| inner_radius * inner_radius - z * z <= big * big) {
        return Err(KernelError::Invalid("shell thickness leaves no barrel cavity".into()));
    }
    let cavity = offset_planar(b, |i, _| if opened.contains(&i) { 0.0 } else { -thickness })?;
    let history = cavity.history().ok_or_else(|| unsupported_shell("the barrel cavity has no face history"))?;
    // Every wall must still be the one offset of its source, in source order. No collapsed
    // faces or changed topology can be sewn by this construction.
    if cavity.face_count() != faces.len()
        || history.faces.iter().enumerate().any(|(i, f)| f.origins.as_slice() != [FaceOrigin::Face { operand: 0, face: i }])
    {
        return Err(unsupported_shell("the barrel cavity changed topology"));
    }
    let inner: Vec<mt::Face> = cavity.solid.face_iter().cloned().collect();
    let mut out = Vec::new();
    let mut provenance = Vec::new();
    for (i, outer) in faces.iter().enumerate() {
        let inset = inner.get(i).ok_or_else(|| unsupported_shell("the barrel cavity lost a face"))?;
        if opened.contains(&i) {
            let mut wires = outer.absolute_boundaries().to_vec();
            wires.extend(inset.absolute_boundaries().iter().map(|w| w.inverse()));
            let mut rim = mt::Face::try_new(wires, outer.surface()).map_err(|e| KernelError::Failed(format!("shell rim: {e}")))?;
            if !outer.orientation() {
                rim.invert();
            }
            out.push(rim);
            provenance.push(record::from_face(0, i, Change::Modified));
        } else {
            out.extend([outer.clone(), inset.inverse()]);
            provenance.extend([record::from_face(0, i, Change::Kept), record::generated(vec![FaceOrigin::Offset { operand: 0, face: i }])]);
        }
    }
    let solid = Solid::try_new(vec![out.into()]).map_err(|e| KernelError::Failed(format!("shell barrel: {e}")))?;
    let copies = record::edges_by_identity(&cavity, history).filter_map(|(id, origins)| origins.into_iter().next().map(|o| (id, o))).collect();
    record::attach(Body::new(solid)?, &[record::Operand::new(b)], Some(provenance), copies).map(Some)
}

/// The radius of a convex round (cylinder or torus) that meets an open face tangentially and
/// is at least `thickness`: the cavity's wall then passes through the rim where the round meets
/// the open face, leaving nothing between them.
fn rim_without_thickness(b: &Body, opened: &[usize], thickness: f64, whole_rim: bool) -> Option<f64> {
    use mt::{BoundedCurve, ParametricCurve};
    let faces: Vec<&mt::Face> = b.solid.face_iter().take(if whole_rim { 4097 } else { usize::MAX }).collect();
    let tol = (b.size() * 1e-7).max(1e-9) * 100.0;
    // A distant shell can make the body's tolerance larger than this wall.
    // Such a diagnostic candidate is not sufficient to refuse a valid opening.
    if whole_rim && tol > thickness * 1e-3 {
        return None;
    }
    opened.iter().filter_map(|fi| faces.get(*fi)).find_map(|open| {
        let plane = crate::offset::surf_of(open, tol)?;
        if whole_rim && (!matches!(plane, crate::offset::Surf::Plane { .. }) || faces.len() > 4096 || open.edge_iter().take(1025).count() > 1024) {
            return None;
        }
        let radius = |edge: &mt::Edge| {
            faces.iter().filter(|g| !std::ptr::eq(**g, *open)).find_map(|g| {
                if !g.edge_iter().any(|e| e.id() == edge.id()) {
                    return None;
                }
                let round = crate::offset::surf_of(g, tol)?;
                let r = match round {
                    crate::offset::Surf::Cylinder { r, convex: true, .. } | crate::offset::Surf::Torus { r, convex: true, .. } => r,
                    _ => return None,
                };
                let c = edge.curve();
                let (t0, t1) = c.range_tuple();
                if !t0.is_finite() || !t1.is_finite() || t0 >= t1 {
                    return None;
                }
                let parameter = t0 * 0.5 + t1 * 0.5;
                if !parameter.is_finite() || parameter < t0 || parameter > t1 {
                    return None;
                }
                let mid = from_p3(c.subs(parameter));
                let tangent = plane.normal(mid)?.dot(round.normal(mid)?) > 1.0 - 1e-6;
                (tangent && r >= thickness - tol).then_some(r)
            })
        };
        if whole_rim {
            let mut found = None;
            for edge in open.edge_iter() {
                found = Some(radius(&edge)?);
            }
            found
        } else {
            open.edge_iter().find_map(|edge| radius(&edge))
        }
    })
}

fn shell_opened(b: &Body, healed: &Body, open: &[Vec3], opened: &[usize], thickness: f64, margin: f64) -> Result<Body> {
    let size = healed.size();
    // Open faces pushed out past the body: the simple way, unless a face tangent to an open
    // one (a fillet along its edge) can't follow; then every face moves in (tangencies keep)
    // and the opening is the open face's copy swept out through the body.
    let cavity = match offset_planar(healed, |fi, _| if opened.contains(&fi) { margin } else { -thickness }) {
        Ok(c) => c,
        Err(e) => return shell_by_opening(healed, opened, thickness, margin).map_err(|_| e),
    };
    let cavity = match opening_overreach(healed, &cavity, opened, thickness, margin) {
        Ok(Some(trimmed)) => trimmed,
        Ok(None) => cavity,
        Err(e) => {
            log::warn!("shell: could not trim the opening to the open faces ({e}); the wall next to it may be cut");
            cavity
        }
    };
    let r = crate::ops::boolean(healed, &cavity, crate::BoolOp::Cut);
    if r.is_err()
        && let Ok(x) = shell_by_opening(healed, opened, thickness, margin)
    {
        return Ok(x);
    }
    match r {
        Ok(Some(r)) => {
            // Over `[healed]`, the cavity's faces offsets of the faces they line; then over `[b]`.
            Ok(record::after(hollowed(r, healed, offsets(&cavity)), healed, &record::counts(&[b])))
        }
        Ok(None) => Err(KernelError::Failed("the shell removed everything".into())),
        // Healing can leave faces the boolean dislikes (revolved patches): the body as it was,
        // its faces to open found again.
        Err(e) => {
            let raw = b.tessellate((size * 1e-3).max(1e-3)).map_err(|_| e.clone())?;
            let mut opened_raw: Vec<usize> = Vec::new();
            for p in open {
                let near = raw
                    .triangles
                    .iter()
                    .zip(&raw.tri_face)
                    .filter_map(|(t, f)| raw.tri(t).map(|[x, y, z]| (point_tri(*p, x, y, z), *f as usize)))
                    .min_by(|a, c| a.0.total_cmp(&c.0));
                if let Some((d, f)) = near
                    && d < size * 1e-3 + 1e-6
                    && !opened_raw.contains(&f)
                {
                    opened_raw.push(f);
                }
            }
            let opened = opened_raw;
            let cavity = offset_planar(b, |fi, _| if opened.contains(&fi) { margin } else { -thickness }).map_err(|_| e.clone())?;
            let r = crate::ops::boolean(b, &cavity, crate::BoolOp::Cut)
                .map_err(|_| e)?
                .ok_or_else(|| KernelError::Failed("the shell removed everything".into()))?;
            Ok(hollowed(r, b, offsets(&cavity)))
        }
    }
}

/// The cavity with each open face pushed out (`cavity`), trimmed so that beyond an open face's
/// plane it only reaches through the open face itself; `None` when it already does.
///
/// The pushed-out face carries the cavity past every edge of the open face. Past a convex edge
/// that is outside the body, but past a concave one (a box on a box, its side face open where it
/// meets the lower box's top) it runs on into the neighbouring wall and cuts a strip through it,
/// as wide as the push. The trimmed cavity is the shell's cavity with every face moved in,
/// together with the pushed-out cavity's parts that are on the body's side of every open face's
/// plane or in front of that open face.
fn opening_overreach(healed: &Body, cavity: &Body, opened: &[usize], thickness: f64, margin: f64) -> Result<Option<Body>> {
    let size = healed.size();
    let tol = size * 1e-6 + 1e-9;
    let faces: Vec<mt::Face> = healed.solid.face_iter().cloned().collect();
    // Each open face's plane (outward normal, offset) and its triangles.
    let mesh = healed.tessellate((size * 1e-3).max(1e-3))?;
    let mut open: Vec<(&mt::Face, Vec3, f64, Vec<[Vec3; 3]>)> = Vec::new();
    for fi in opened {
        let Some(f) = faces.get(*fi) else { return Ok(None) };
        let mt::Surface::Plane(pl) = f.oriented_surface() else { return Ok(None) };
        let n = pl.normal();
        let Some(n) = Vec3::new(n.x, n.y, n.z).normalized() else { return Ok(None) };
        let d = n.dot(from_p3(pl.origin()));
        let tris: Vec<[Vec3; 3]> =
            mesh.triangles.iter().zip(&mesh.tri_face).filter(|(_, f)| **f as usize == *fi).filter_map(|(t, _)| mesh.tri(t)).collect();
        if tris.is_empty() {
            return Ok(None);
        }
        open.push((f, n, d, tris));
    }
    // Only a corner of the cavity past an open face's plane and not in front of that face can be
    // in a wall; with none, the cavity is right as it is.
    let overreach = cavity.solid.vertex_iter().any(|v| {
        let q = from_p3(v.point());
        open.iter().any(|(_, n, d, tris)| {
            let h = n.dot(q) - d;
            h > tol && {
                let on = q - *n * h;
                !tris.iter().any(|[a, b, c]| point_tri(on, *a, *b, *c) < size * 1e-4 + 1e-6)
            }
        })
    });
    if !overreach {
        return Ok(None);
    }
    let closed = offset_planar(healed, |_, _| -thickness)?;
    // Keep: on the body's side of each open plane, or in front of the open face.
    let reach = size * 4.0;
    let mut keep: Option<Body> = None;
    for (f, n, d, _) in &open {
        let c = *n * *d;
        let mut hs = vec![HalfSpace { n: *n, d: *d }];
        for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
            hs.push(HalfSpace { n: axis, d: axis.dot(c) + reach });
            hs.push(HalfSpace { n: axis * -1.0, d: -axis.dot(c) + reach });
        }
        let (side, _) = polyhedron(&hs)?;
        // In front of the face: its prism, from half a wall inside the body (overlapping `side`
        // rather than touching it) to past the pushed-out face.
        let inset = thickness * 0.5;
        let prism = guard("shell opening prism", || {
            let start = builder::translated(*f, v3(*n * -inset));
            let solid: Solid = builder::tsweep(&start, v3(*n * (inset + margin * 2.0)));
            Ok(solid)
        })?;
        let prism = Body::new(prism)?;
        let k = crate::ops::boolean(&side, &prism, crate::BoolOp::Union)?.ok_or_else(|| KernelError::Failed("shell: opening".into()))?;
        keep = Some(match keep {
            None => k,
            Some(acc) => crate::ops::boolean(&acc, &k, crate::BoolOp::Intersect)?.ok_or_else(|| KernelError::Failed("shell: opening".into()))?,
        });
    }
    let Some(keep) = keep else { return Ok(None) };
    let kept = crate::ops::boolean(cavity, &keep, crate::BoolOp::Intersect)?.ok_or_else(|| KernelError::Failed("shell: opening".into()))?;
    let trimmed = crate::ops::boolean(&closed, &kept, crate::BoolOp::Union)?.ok_or_else(|| KernelError::Failed("shell: opening".into()))?;
    Ok(Some(trimmed))
}

/// A shell made from the cavity with every face moved in by the thickness (open faces too),
/// joined with each open face's moved copy swept out through the body. No history: the
/// opening's walls, swept from the cavity's edges, come from nothing of `b`.
fn shell_by_opening(b: &Body, opened: &[usize], thickness: f64, reach: f64) -> Result<Body> {
    let closed = offset_planar(b, |_, _| -thickness)?;
    let faces: Vec<mt::Face> = closed.solid.face_iter().cloned().collect();
    let mut cavity = closed.clone();
    for fi in opened {
        let f = faces.get(*fi).ok_or_else(|| KernelError::Failed("shell: open face".into()))?;
        let mt::Surface::Plane(pl) = f.oriented_surface() else {
            return Err(unsupported_shell("the open face must be planar here"));
        };
        let n = pl.normal();
        let n = Vec3::new(n.x, n.y, n.z).normalized().ok_or_else(|| KernelError::Failed("shell: open face normal".into()))?;
        let prism = guard("shell opening", || {
            let solid: Solid = builder::tsweep(f, v3(n * (thickness + reach)));
            Ok(solid)
        })?;
        let prism = Body::new(prism)?;
        cavity = crate::ops::boolean(&cavity, &prism, crate::BoolOp::Union)?.ok_or_else(|| KernelError::Failed("shell: opening".into()))?;
    }
    let r = crate::ops::boolean(b, &cavity, crate::BoolOp::Cut)?.ok_or_else(|| KernelError::Failed("the shell removed everything".into()))?;
    Ok(r.without_history())
}

fn unsupported_shell(m: &str) -> KernelError {
    KernelError::Failed(format!("not supported yet: {m}"))
}

pub(crate) fn point_tri(p: Vec3, a: Vec3, b: Vec3, c: Vec3) -> f64 {
    let n = (b - a).cross(c - a);
    let Some(nn) = n.normalized() else { return f64::INFINITY };
    let h = (p - a).dot(nn);
    let q = p - nn * h;
    let inside = [(a, b), (b, c), (c, a)].iter().all(|(u, v)| (*v - *u).cross(q - *u).dot(n) >= -1e-12);
    if inside { h.abs() } else { [(a, b), (b, c), (c, a)].iter().map(|(u, v)| p.dist_to_segment(*u, *v)).fold(f64::INFINITY, f64::min) }
}

/// The body with every planar face moved along its outward normal by `shift(face index,
/// normal)`, keeping the topology: each vertex goes to where its faces' moved planes meet.
pub(crate) fn offset_planar(b: &Body, shift: impl Fn(usize, Vec3) -> f64) -> Result<Body> {
    crate::offset::offset_body(b, shift)
}

pub(crate) fn solve3_pub(m: [[f64; 3]; 3], r: [f64; 3]) -> Option<[f64; 3]> {
    solve3(m, r)
}

fn solve3(m: [[f64; 3]; 3], r: [f64; 3]) -> Option<[f64; 3]> {
    let det = |a: [[f64; 3]; 3]| {
        a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1]) - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
            + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0])
    };
    let d = det(m);
    if d.abs() < 1e-9 {
        return None;
    }
    let mut out = [0.0; 3];
    for (k, o) in out.iter_mut().enumerate() {
        let mut mk = m;
        for (row, rv) in mk.iter_mut().zip(r) {
            row[k] = rv;
        }
        *o = det(mk) / d;
    }
    Some(out)
}

/// The faces of `body` the given points pick, each whole (every piece of its surface).
///
/// Taken per body rather than once, because an operation may be tried again on the same body
/// with a face split in two, and the pieces are numbered differently then. The points do not
/// move, so they pick the same surface either way.
fn faces_at(body: &Body, at: &[Vec3]) -> Result<Vec<usize>> {
    let size = body.size();
    let mesh = body.tessellate((size * 1e-3).max(1e-3))?;
    let mut chosen: Vec<usize> = Vec::new();
    for p in at {
        let near = mesh
            .triangles
            .iter()
            .zip(&mesh.tri_face)
            .filter_map(|(t, f)| mesh.tri(t).map(|[x, y, z]| (point_tri(*p, x, y, z), *f as usize)))
            .min_by(|a, c| a.0.total_cmp(&c.0));
        match near {
            Some((d, f)) if d < size * 1e-3 + 1e-6 => {
                if !chosen.contains(&f) {
                    chosen.push(f);
                }
            }
            _ => return Err(KernelError::Invalid(format!("no face at {:?}", [p.x, p.y, p.z]))),
        }
    }
    Ok(crate::offset::with_same_surface(body, &chosen))
}

/// Move the planar faces at the given points along their outward normals by `distance`
/// (negative: into the body). The body's other faces follow (its topology is kept).
pub fn offset_faces(b: &Body, at: &[Vec3], distance: f64) -> Result<Body> {
    b.require_brep("offset faces")?;
    if !distance.is_finite() || distance.abs() > 1e6 {
        return Err(KernelError::Invalid("offset distance".into()));
    }
    // Healing is part of the attempt, not a step before it: a face that runs along a seam edge
    // twice fails there, and the retry below needs the whole of it to run on the split body.
    let run = |body: &Body| -> Result<Body> {
        let healed = crate::boolean_record::healed(body)?.with_split_lines(body.splits.clone());
        let chosen = faces_at(&healed, at)?;
        match offset_planar(&healed, |fi, _| if chosen.contains(&fi) { distance } else { 0.0 }) {
            Ok(r) => Ok(record::after(r, &healed, &record::counts(&[body]))),
            // A piece of a split face (its neighbour stays): the piece swept along its normal
            // joins on (outward) or cuts in, with walls along the split lines.
            Err(e) if !healed.split_keep().is_empty() => press_pieces(&healed, &chosen, distance).map_err(|_| e),
            Err(e) => Err(e),
        }
    };
    match crate::guard("offset faces", || run(b)) {
        Ok(r) => Ok(r),
        // A face that runs along a seam edge twice — a hole's wall as a STEP file writes it, one
        // whole turn bounded by one wire. Healing it, or moving its neighbour, fails where the
        // same body with that face in two halves works, so split it and ask again, which is what
        // a boolean already does (`seam_faces`).
        Err(_) => match crate::seam_faces::one_body(b, "offset faces off their seams", &run) {
            Some(Ok(r)) => Ok(r),
            // Freeform faces (a loft, a sweep, an imported spline): the face is copied out,
            // thickened along its normals by the distance, and joined on (outward) or cut away
            // (inward).
            _ => crate::guard("offset freeform faces", || freeform_offset(b, at, distance).map_err(|e| match e {
                // A reason the user can act on (no face there, deeper than a round's radius).
                KernelError::Invalid(why) => KernelError::Invalid(why),
                _ if distance < 0.0 => KernelError::Failed("offset: this curved face couldn't be pulled in that far; try a smaller distance".into()),
                _ => KernelError::Failed("offset: this curved face couldn't be moved; try a smaller distance".into()),
            })),
        },
    }
}

/// Offset by thickening a copy of the faces and joining it to the body (or cutting it away).
fn freeform_offset(b: &Body, at: &[Vec3], distance: f64) -> Result<Body> {
    if distance.abs() < 1e-9 {
        return Ok(b.clone());
    }
    let whole = |r: Option<Body>| r.ok_or_else(|| KernelError::Failed("offset removed the whole body".into()));
    // As Shapr3D does, faces meeting the picked ones smoothly (a round's neighbours, and
    // theirs) move with them: a round cannot move alone, as nothing would meet it.
    let budget = Budget::new();
    let chosen = faces_at(b, at)?;
    let chain = tangent_closure(b, &chosen);
    if chain.len() > chosen.len() {
        return chain_offset(b, &chain, distance, &budget);
    }
    // The tool reaches the distance both ways from the face, so none of its faces lies on the
    // body's (two copies of one curved surface send the boolean round for ever): joined, its
    // outer half is what is added; cut, its inner half is what goes.
    let sheet = crate::surfaces::copy_faces(b, at)?;
    if distance > 0.0 {
        // Out, three tools tried together, the first to join on winning: one starting a
        // quarter of the way in, under the face; the face's copy reaching both ways; and one
        // starting on the face itself. Which of them the boolean takes depends on the shape,
        // and one it does not take can keep it going round for ever.
        let lap = distance * 0.25;
        let tools = [
            crate::surface_offset::offset_surface(b, &chosen, -lap, 1e-3).and_then(|s| crate::surfaces::thicken(&s, distance + lap, false)),
            crate::surfaces::thicken(&sheet, 2.0 * distance, true),
            crate::surfaces::thicken(&sheet, distance, false),
        ];
        return first_union(b, tools.into_iter().flatten().collect(), &budget);
    }
    // Pulled in, a flat face's tool would have walls on its neighbours' faces (a box's sides),
    // which the cut cannot take. A tool a little wider than the face puts them just outside:
    // past a convex edge that is outside the body, so it cuts nothing more.
    let size = b.size();
    let tol = (size * 1e-7).max(1e-9);
    let faces: Vec<mt::Face> = b.solid.face_iter().cloned().collect();
    let mut plane = None;
    for &i in &chosen {
        match faces.get(i).and_then(|f| crate::offset::surf_of(f, tol * 100.0)) {
            Some(crate::offset::Surf::Plane { n, d }) if plane.is_none_or(|(m, _): (Vec3, f64)| m.dot(n) > 1.0 - 1e-9) => plane = Some((n, d)),
            _ => {
                plane = None;
                break;
            }
        }
    }
    let tool = match plane {
        Some((n, d)) => {
            let pts: Vec<Vec3> = sheet.solid.vertex_iter().map(|v| crate::body::from_p3(v.point())).collect();
            if pts.is_empty() {
                return Err(KernelError::Invalid("no face".into()));
            }
            let mid = pts.iter().fold(Vec3::ZERO, |a, p| a + *p) * (1.0 / pts.len() as f64);
            let o = mid - n * (n.dot(mid) - d);
            let reach = pts.iter().map(|p| p.dist(o)).fold(0.0, f64::max).max(1e-6);
            let k = 1.0 + 2.0 * (0.05f64).max(distance.abs() * 0.05) / reach;
            let mat = [[k, 0.0, 0.0, 0.0], [0.0, k, 0.0, 0.0], [0.0, 0.0, k, 0.0], [o.x * (1.0 - k), o.y * (1.0 - k), o.z * (1.0 - k), 1.0]];
            crate::surfaces::thicken(&crate::ops::transform_matrix(&sheet, mat)?, 2.0 * distance.abs(), true)?
        }
        None => crate::surfaces::thicken(&sheet, 2.0 * distance.abs(), true)?,
    };
    whole(timed(b, &tool, crate::ops::BoolOp::Cut, &budget)?)
}

/// The picked faces and every face meeting them smoothly, and so on (indices into `b`'s faces).
fn tangent_closure(b: &Body, chosen: &[usize]) -> Vec<usize> {
    let faces: Vec<mt::Face> = b.solid.face_iter().cloned().collect();
    // Which faces hold each edge, found once.
    let mut holders: HashMap<mt::EdgeID, Vec<usize>> = HashMap::new();
    for (i, f) in faces.iter().enumerate() {
        for e in f.edge_iter() {
            let l = holders.entry(e.id()).or_default();
            if !l.contains(&i) {
                l.push(i);
            }
        }
    }
    let mut inside = vec![false; faces.len()];
    let mut set: Vec<usize> = chosen.iter().copied().filter(|i| *i < faces.len()).collect();
    for i in &set {
        if let Some(x) = inside.get_mut(*i) {
            *x = true;
        }
    }
    let mut i = 0;
    while let Some(&f) = set.get(i) {
        i += 1;
        let Some(face) = faces.get(f) else { continue };
        for e in face.edge_iter() {
            let Some(others) = holders.get(&e.id()) else { continue };
            if others.iter().all(|g| inside.get(*g).copied().unwrap_or(true)) || !crate::blend::tangent_faces(&b.solid, &e) {
                continue;
            }
            for g in others {
                if let Some(x) = inside.get_mut(*g).filter(|x| !**x) {
                    *x = true;
                    set.push(*g);
                }
            }
        }
    }
    set
}

/// A smooth run of faces (`chain`, indices) moved along their normals by `distance`: the run
/// thickened both ways into a tool, joined on (out) or cut away (in). Pulled in, where the
/// tool ends flat on a face of the body that stays (a box's end, its floor), it is first made
/// longer past that face, so the cut has no faces lying on each other.
fn chain_offset(b: &Body, chain: &[usize], distance: f64, budget: &Budget) -> Result<Body> {
    // Exact and quick where the run's neighbours are flat and meet it square on (a box's
    // ends and floor round a rounded top): the same faces, the run's moved, the rest trimmed.
    if let Ok(r) = crate::guard("offset a smooth run", || moved_run(b, chain, distance)) {
        return Ok(r);
    }
    let whole = |r: Option<Body>| r.ok_or_else(|| KernelError::Failed("offset removed the whole body".into()));
    let faces: Vec<mt::Face> = b.solid.face_iter().cloned().collect();
    let sheet = crate::surfaces::copy_face_indices(b, chain)?;
    if distance > 0.0 {
        // Out, tried together: tools from a little under the faces (less than any round's
        // radius) to the distance out, and one starting on the faces.
        let all: Vec<usize> = (0..chain.len()).collect();
        let under = |lap: f64| crate::surface_offset::offset_surface(&sheet, &all, -lap, 1e-3).and_then(|s| crate::surfaces::thicken(&s, distance + lap, false));
        let tools = [under((distance * 0.25).min(0.3)), under((distance * 0.1).min(0.05)), crate::surfaces::thicken(&sheet, distance, false)];
        return first_union(b, tools.into_iter().flatten().collect(), budget);
    }
    let tool = crate::surfaces::thicken(&sheet, 2.0 * distance.abs(), true)?;
    let tol = (b.size() * 1e-6).max(1e-7);
    let planes: Vec<(Vec3, f64)> = faces
        .iter()
        .filter_map(|f| match f.oriented_surface() {
            mt::Surface::Plane(pl) => {
                let n = Vec3::new(pl.normal().x, pl.normal().y, pl.normal().z).normalized()?;
                Some((n, n.dot(from_p3(pl.origin()))))
            }
            _ => None,
        })
        .collect();
    let reach = 2.0 * distance.abs() + (b.size() * 1e-3).max(0.1);
    let mut longer = tool.clone();
    for f in tool.solid.face_iter() {
        // The face's outline, sampled.
        let mut pts3: Vec<Vec3> = Vec::new();
        for w in f.boundaries().iter().take(1) {
            for e in w.edge_iter() {
                use mt::{BoundedCurve, ParametricCurve};
                let c = e.oriented_curve();
                let (t0, t1) = c.range_tuple();
                for k in 0..16 {
                    pts3.push(from_p3(c.subs(t0 + (t1 - t0) * f64::from(k) / 16.0)));
                }
            }
        }
        // Lying flat on one of the body's flat faces (the tool's end on a box's end): made
        // longer out past it, along that face's outward normal.
        let Some(&(n, d)) = planes.iter().find(|(n, d)| pts3.len() >= 3 && pts3.iter().all(|p| (n.dot(*p) - d).abs() < tol * 100.0)) else { continue };
        let x = n.any_perp();
        let plane = Plane { origin: n * d, x, y: n.cross(x) };
        let pts: Vec<smpl3d_geom::Vec2> = pts3.iter().map(|p| plane.to_local(*p)).collect();
        let region = smpl3d_geom::Region2 { outer: smpl3d_geom::Loop2::polygon(&pts).ccw(), holes: Vec::new() };
        let cap = crate::extrude_planar_regions(&plane, &[region], 0.0, reach)?;
        if let Some(u) = timed(&longer, &cap, crate::ops::BoolOp::Union, budget)? {
            longer = u;
        }
    }
    whole(timed(b, &longer, crate::ops::BoolOp::Cut, budget)?)
}

/// The body with the smooth run of faces `chain` moved `distance` along their normals and
/// nothing else changed but where faces meet: every face keeps its place in the body. Each
/// face of the run goes onto its offset surface; a corner on the run moves with it; a face
/// beside the run must be flat and stay so, its corners on the run landing in its plane (it
/// meets the run square on), and its edges off the run straight. Refused otherwise.
fn moved_run(b: &Body, chain: &[usize], distance: f64) -> Result<Body> {
    use mt::{BoundedCurve, ParametricCurve};
    let faces: Vec<mt::Face> = b.solid.face_iter().cloned().collect();
    let size = b.size();
    let tol = (size * 1e-5).max(1e-6) + distance.abs() * 2e-3;
    let movers: HashMap<usize, crate::surface_offset::FaceMover> = chain
        .iter()
        .filter_map(|i| faces.get(*i).map(|f| (*i, f)))
        .map(|(i, f)| crate::surface_offset::FaceMover::new(f, &[distance], 1e-3).map(|m| (i, m)))
        .collect::<Result<_>>()?;
    let in_run = |i: usize| movers.contains_key(&i);
    // How far a moved corner or edge may be pulled back into a flat neighbour's plane.
    let slack = tol + distance.abs() * 0.2;
    // The planes of the faces beside the run.
    let mut planes: HashMap<usize, (Vec3, f64)> = HashMap::new();
    for (i, f) in faces.iter().enumerate() {
        if in_run(i) {
            continue;
        }
        if let mt::Surface::Plane(pl) = f.oriented_surface() {
            let n = Vec3::new(pl.normal().x, pl.normal().y, pl.normal().z).normalized().ok_or_else(|| fail_run("a flat face"))?;
            planes.insert(i, (n, n.dot(from_p3(pl.origin()))));
        }
    }
    // Which faces hold each vertex and edge.
    let mut vfaces: HashMap<mt::VertexID, Vec<usize>> = HashMap::new();
    let mut efaces: HashMap<mt::EdgeID, Vec<usize>> = HashMap::new();
    let mut verts: HashMap<mt::VertexID, mt::Vertex> = HashMap::new();
    let mut edges: HashMap<mt::EdgeID, mt::Edge> = HashMap::new();
    for (i, f) in faces.iter().enumerate() {
        for e in f.boundaries().iter().flat_map(|w| w.edge_iter()) {
            let abs = if e.orientation() { e.clone() } else { e.inverse() };
            efaces.entry(abs.id()).or_default().push(i);
            for v in [abs.front(), abs.back()] {
                let l = vfaces.entry(v.id()).or_default();
                if !l.contains(&i) {
                    l.push(i);
                }
                verts.entry(v.id()).or_insert_with(|| v.clone());
            }
            edges.entry(abs.id()).or_insert(abs);
        }
    }
    // New vertices: on the run, moved with it (every run face holding it agreeing), and in the
    // plane of every other face holding it.
    let mut new_v: HashMap<mt::VertexID, mt::Vertex> = HashMap::new();
    for (id, v) in &verts {
        let fs = vfaces.get(id).cloned().unwrap_or_default();
        let p = from_p3(v.point());
        let mut at: Option<Vec3> = None;
        for f in fs.iter().filter(|f| in_run(**f)) {
            let q = movers.get(f).ok_or_else(|| fail_run("a face"))?.point(p, distance)?;
            match at {
                Some(a) if a.dist(q) > tol => return Err(fail_run("the run's faces part at a corner")),
                Some(_) => {}
                None => at = Some(q),
            }
        }
        let Some(q) = at else {
            new_v.insert(*id, v.clone());
            continue;
        };
        let beside: Vec<(Vec3, f64)> = fs.iter().filter(|f| !in_run(**f)).map(|f| planes.get(f).copied().ok_or_else(|| fail_run("a curved face beside the run"))).collect::<Result<_>>()?;
        let q = onto_planes(q, &beside, slack)?;
        new_v.insert(*id, mt::Vertex::new(p3(q)));
    }
    // New edges: on the run, moved with its face; off it, straight between the new corners.
    let mut new_e: HashMap<mt::EdgeID, mt::Edge> = HashMap::new();
    for (id, e) in &edges {
        let fs = efaces.get(id).cloned().unwrap_or_default();
        let (Some(a), Some(z)) = (new_v.get(&e.front().id()), new_v.get(&e.back().id())) else { return Err(fail_run("an edge")) };
        let moved_ends = a.id() != e.front().id() || z.id() != e.back().id();
        let ne = if let Some(f) = fs.iter().find(|f| in_run(**f)) {
            let c = movers.get(f).ok_or_else(|| fail_run("a face"))?.curve(&e.curve(), distance)?;
            // Held in the plane of a face beside the run that holds it (a round whose radius
            // changes leans a little at the run's ends).
            let beside: Vec<(Vec3, f64)> = fs.iter().filter(|g| !in_run(**g)).map(|g| planes.get(g).copied().ok_or_else(|| fail_run("a curved face beside the run"))).collect::<Result<_>>()?;
            let c = if beside.is_empty() {
                c
            } else {
                let (t0, t1) = c.range_tuple();
                let mut pts: Vec<Vec3> = (0..33).map(|k| onto_planes(from_p3(c.subs(t0 + (t1 - t0) * f64::from(k) / 32.0)), &beside, slack)).collect::<Result<_>>()?;
                if let Some(p0) = pts.first_mut() {
                    *p0 = from_p3(a.point());
                }
                if let Some(pn) = pts.last_mut() {
                    *pn = from_p3(z.point());
                }
                if matches!(c, mt::Curve::Line(_)) {
                    mt::Curve::Line(mt::Line(a.point(), z.point()))
                } else {
                    mt::Curve::BSplineCurve(crate::build::interpolate_cubic(&pts).ok_or_else(|| fail_run("an edge"))?)
                }
            };
            mt::Edge::new_unchecked(a, z, c)
        } else if !moved_ends {
            e.clone()
        } else if matches!(e.curve(), mt::Curve::Line(_)) {
            mt::Edge::new_unchecked(a, z, mt::Curve::Line(mt::Line(a.point(), z.point())))
        } else {
            return Err(fail_run("a curved edge beside the run"));
        };
        new_e.insert(*id, ne);
    }
    // The faces, on their new edges.
    let mut rec = crate::record::Rebuild::default();
    let mut out = Vec::with_capacity(faces.len());
    for (i, f) in faces.iter().enumerate() {
        let wires: Vec<mt::Wire> = f
            .absolute_boundaries()
            .iter()
            .map(|w| {
                w.edge_iter()
                    .map(|e| {
                        let abs = if e.orientation() { e.id() } else { e.inverse().id() };
                        let m = new_e.get(&abs).cloned().ok_or_else(|| fail_run("an edge"))?;
                        Ok(if e.orientation() { m } else { m.inverse() })
                    })
                    .collect::<Result<Vec<_>>>()
                    .map(mt::Wire::from)
            })
            .collect::<Result<_>>()?;
        let surface = match movers.get(&i) {
            Some(m) => m.surface(distance)?,
            None => f.surface(),
        };
        let mut nf = mt::Face::new_unchecked(wires, surface);
        if !f.orientation() {
            nf.invert();
        }
        rec.face(&nf, f);
        out.push(nf);
    }
    let solid = Solid::try_new(vec![mt::Shell::from(out)]).map_err(|e| fail_run(&e.to_string()))?;
    let r = Body::new(solid)?;
    // A body that turned inside out somewhere (a corner moved past another) is refused.
    let v0 = crate::measure(b)?.volume;
    let v1 = crate::measure(&r)?.volume;
    if !(v1 > 0.0) || (v1 - v0).signum() != distance.signum() {
        return Err(fail_run("the moved faces fold over"));
    }
    Ok(crate::record::rebuilt_body(r, &[&b.solid], &rec))
}

/// `q` moved onto every plane in `planes` (by alternating projection), refused when that
/// moves it further than `slack`.
fn onto_planes(q: Vec3, planes: &[(Vec3, f64)], slack: f64) -> Result<Vec3> {
    let mut p = q;
    for _ in 0..50 {
        for (n, d) in planes {
            p = p - *n * (n.dot(p) - d);
        }
        if planes.iter().all(|(n, d)| (n.dot(p) - d).abs() < 1e-12) {
            break;
        }
    }
    if p.dist(q) > slack {
        return Err(fail_run(&format!("a face beside the run is not square to it ({:.4} mm)", p.dist(q))));
    }
    Ok(p)
}

fn fail_run(m: &str) -> KernelError {
    KernelError::Failed(format!("offset a smooth run: {m}"))
}

/// How long one fallback boolean may take, how long one offset may spend on all of them, and
/// how many may still be going round after being given up before a new one is refused.
const BOOLEAN_LIMIT: std::time::Duration = std::time::Duration::from_secs(8);
const BOOLEAN_BUDGET: std::time::Duration = std::time::Duration::from_secs(30);
const MAX_STUCK: isize = 6;

/// Booleans given up on that are still running (each holds a processor until it ends).
static STUCK: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

/// The time one offset has left for its fallback booleans: each tool it tries gets a few
/// seconds (a different tool often works where one went round for ever), all of them together
/// no more than the budget.
struct Budget {
    until: std::time::Instant,
}

impl Budget {
    fn new() -> Budget {
        Budget { until: std::time::Instant::now() + BOOLEAN_BUDGET }
    }
}

/// `b` joined with the first of `tools` that the boolean takes, all tried at once, each on
/// its own thread. The others are left to finish.
fn first_union(b: &Body, tools: Vec<Body>, budget: &Budget) -> Result<Body> {
    use std::sync::atomic::Ordering;
    let failed = || KernelError::Failed("offset: no tool joined on".into());
    if cfg!(target_arch = "wasm32") {
        // No threads there: one after another.
        return tools.iter().find_map(|t| timed(b, t, crate::ops::BoolOp::Union, budget).ok().flatten()).ok_or_else(failed);
    }
    // One wait for all of them, so it may be longer than one boolean's by itself.
    let left = budget.until.saturating_duration_since(std::time::Instant::now()).min(BOOLEAN_LIMIT * 2);
    if tools.is_empty() || left.is_zero() || STUCK.load(Ordering::Relaxed) >= MAX_STUCK {
        return Err(failed());
    }
    let until = std::time::Instant::now() + left;
    let (tx, rx) = std::sync::mpsc::channel();
    // One per tool, as in `timed`: 0 running, 1 finished, 2 no longer waited for.
    let mut states = Vec::new();
    for tool in tools {
        let state = std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0));
        let (a, tx, theirs) = (b.clone(), tx.clone(), state.clone());
        let spawned = std::thread::Builder::new().name("offset-boolean".into()).stack_size(256 << 20).spawn(move || {
            let r = crate::guard("offset boolean", || crate::ops::boolean(&a, &tool, crate::ops::BoolOp::Union));
            if theirs.swap(1, Ordering::SeqCst) == 2 {
                STUCK.fetch_sub(1, Ordering::SeqCst);
            } else {
                let _ = tx.send(r);
            }
        });
        if spawned.is_ok() {
            states.push(state);
        }
    }
    drop(tx);
    let mut running = states.len();
    let mut found = None;
    while running > 0 && found.is_none() {
        match rx.recv_timeout(until.saturating_duration_since(std::time::Instant::now())) {
            Ok(Ok(Some(r))) => found = Some(r),
            Ok(_) => running -= 1,
            Err(_) => break,
        }
    }
    // Those still going are counted until they end.
    for state in states {
        if state.compare_exchange(0, 2, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
            STUCK.fetch_add(1, Ordering::SeqCst);
        }
    }
    found.ok_or_else(failed)
}

/// A boolean run on its own thread and given up when the budget runs out: on curved faces
/// lying close to each other the boolean can go round for ever. The thread is left to finish.
fn timed(a: &Body, b: &Body, op: crate::ops::BoolOp, budget: &Budget) -> Result<Option<Body>> {
    use std::sync::atomic::Ordering;
    let late = || KernelError::Failed("offset: took too long".into());
    let left = budget.until.saturating_duration_since(std::time::Instant::now()).min(BOOLEAN_LIMIT);
    if left.is_zero() || STUCK.load(Ordering::Relaxed) >= MAX_STUCK {
        return Err(late());
    }
    // No threads there: it runs here, as every other boolean does.
    if cfg!(target_arch = "wasm32") {
        return crate::ops::boolean(a, b, op);
    }
    let (a, b) = (a.clone(), b.clone());
    let (tx, rx) = std::sync::mpsc::channel();
    // 0: running, 1: finished, 2: given up.
    let state = std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0));
    let theirs = state.clone();
    std::thread::Builder::new()
        .name("offset-boolean".into())
        .stack_size(256 << 20)
        .spawn(move || {
            let r = crate::guard("offset boolean", || crate::ops::boolean(&a, &b, op));
            if theirs.swap(1, Ordering::SeqCst) == 2 {
                STUCK.fetch_sub(1, Ordering::SeqCst);
            }
            let _ = tx.send(r);
        })
        .map_err(|e| KernelError::Failed(format!("offset: {e}")))?;
    match rx.recv_timeout(left) {
        Ok(r) => r,
        // The thread ended without an answer: nothing is left running.
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(KernelError::Failed("offset: the boolean failed".into())),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            if state.compare_exchange(0, 2, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
                STUCK.fetch_add(1, Ordering::SeqCst);
            }
            Err(late())
        }
    }
}

/// No history: the walls along the split lines are swept from edges, and come from nothing of
/// `b`.
fn press_pieces(b: &Body, chosen: &[usize], distance: f64) -> Result<Body> {
    let faces: Vec<mt::Face> = b.solid.face_iter().cloned().collect();
    let mut cur = b.clone();
    for fi in chosen {
        let f = faces.get(*fi).ok_or_else(|| KernelError::Failed("offset: face".into()))?;
        let n = crate::blend::plane_normal(f).ok_or_else(|| KernelError::Failed("offset: only planar pieces".into()))?;
        let tool = guard("offset face", || Body::new(builder::tsweep(f, crate::body::v3(n * distance))))?;
        let op = if distance > 0.0 { crate::BoolOp::Union } else { crate::BoolOp::Cut };
        cur = crate::ops::boolean(&cur, &tool, op)?.ok_or_else(|| KernelError::Failed("offset: the piece moved off the body".into()))?;
    }
    Ok(cur.with_split_lines(b.splits.clone()).without_history())
}

#[cfg(test)]
mod rim_preflight_tests {
    use super::*;

    #[test]
    fn a_distant_shell_does_not_turn_a_smaller_local_round_into_a_zero_rim() {
        let block = crate::box_solid(Vec3::ZERO, Vec3::new(40.0, 30.0, 10.0)).unwrap();
        let edges = [Vec3::new(20.0, 0.0, 0.0), Vec3::new(40.0, 15.0, 0.0), Vec3::new(20.0, 30.0, 0.0), Vec3::new(0.0, 15.0, 0.0)];
        let rounded = crate::fillet(&block, &edges, 1.5).unwrap();
        let distant = crate::box_solid(Vec3::new(1_000_000.0, 0.0, 0.0), Vec3::new(1_000_010.0, 10.0, 10.0)).unwrap();
        let combined = crate::boolean(&rounded, &distant, crate::BoolOp::Union).unwrap().unwrap();
        let faces: Vec<_> = (0..combined.solid.face_iter().count()).collect();
        assert!(rim_without_thickness(&combined, &faces, 2.0, true).is_none());
    }
}
