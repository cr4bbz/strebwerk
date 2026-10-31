//! Kamera, Projektion und tiefensortierte Darstellung (Maleralgorithmus).
//!
//! Die 3D-Szene wird auf der CPU projiziert und als egui-Primitive
//! gezeichnet, die eframe über wgpu rendert. Für konvexe Körper wird
//! schichtweise sortiert (Rückflächen → Innenstreben → Vorderflächen →
//! sichtbare Kanten), so liegen Verstrebungen korrekt *im* Körper.

use crate::scene::Built;
use eframe::egui::{self, epaint, Color32, Pos2, Rect, Stroke};
use glam::DVec3;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Camera {
    pub target: DVec3,
    pub dist: f64,
    pub yaw: f64,
    pub pitch: f64,
    pub fov: f64,
    pub ortho: bool,
}

impl Default for Camera {
    fn default() -> Self {
        Camera {
            target: DVec3::ZERO,
            dist: 4.5,
            yaw: 0.6,
            pitch: 0.35,
            fov: 40f64.to_radians(),
            ortho: false,
        }
    }
}

pub struct Basis {
    pub eye: DVec3,
    pub right: DVec3,
    pub up: DVec3,
    pub fwd: DVec3,
}

impl Camera {
    pub fn basis(&self) -> Basis {
        let (cp, sp) = (self.pitch.cos(), self.pitch.sin());
        let (cy, sy) = (self.yaw.cos(), self.yaw.sin());
        let eye = self.target + DVec3::new(cp * cy, cp * sy, sp) * self.dist;
        let fwd = (self.target - eye).normalize();
        let right = fwd.cross(DVec3::Z).normalize_or(DVec3::X);
        let up = right.cross(fwd);
        Basis {
            eye,
            right,
            up,
            fwd,
        }
    }
    pub fn focal(&self, rect: Rect) -> f64 {
        (rect.height() as f64 * 0.5) / (self.fov * 0.5).tan()
    }
    /// Bildschirmposition und Tiefe
    pub fn project(&self, b: &Basis, rect: Rect, p: DVec3) -> Option<(Pos2, f64)> {
        let d = p - b.eye;
        let z = d.dot(b.fwd);
        let x = d.dot(b.right);
        let y = d.dot(b.up);
        let f = self.focal(rect);
        let c = rect.center();
        if self.ortho {
            let s = f / self.dist;
            Some((Pos2::new(c.x + (x * s) as f32, c.y - (y * s) as f32), z))
        } else {
            if z < 0.02 * self.dist {
                return None;
            }
            Some((
                Pos2::new(c.x + (x * f / z) as f32, c.y - (y * f / z) as f32),
                z,
            ))
        }
    }
    /// Sehstrahl durch einen Bildschirmpunkt
    pub fn ray(&self, b: &Basis, rect: Rect, s: Pos2) -> (DVec3, DVec3) {
        let f = self.focal(rect);
        let dx = (s.x - rect.center().x) as f64;
        let dy = (s.y - rect.center().y) as f64;
        if self.ortho {
            let k = self.dist / f;
            (b.eye + b.right * dx * k - b.up * dy * k, b.fwd)
        } else {
            (b.eye, (b.fwd * f + b.right * dx - b.up * dy).normalize())
        }
    }
    pub fn is_front(&self, b: &Basis, p: DVec3, n: DVec3) -> bool {
        if self.ortho {
            n.dot(-b.fwd) > 0.0
        } else {
            n.dot(b.eye - p) > 0.0
        }
    }
}

#[derive(Clone, Debug)]
pub enum Prim {
    Poly(Vec<Pos2>, Color32),
    Tri([Pos2; 3], [Color32; 3]),
    Line(Pos2, Pos2, f32, Color32),
    Dot(Pos2, f32, Color32),
    Text(Pos2, String, Color32),
}

pub struct Item {
    pub key: f64,
    pub prim: Prim,
}

pub struct Pick {
    pub pos: Pos2,
    pub obj: u64,
    pub idx: usize,
}

fn shade(c: Color32, n: DVec3, light: DVec3, two_sided: bool) -> Color32 {
    let d = n.dot(light);
    let d = if two_sided { d.abs() } else { d.max(0.0) };
    let k = (0.38 + 0.62 * d) as f32;
    Color32::from_rgba_unmultiplied(
        (c.r() as f32 * k) as u8,
        (c.g() as f32 * k) as u8,
        (c.b() as f32 * k) as u8,
        c.a(),
    )
}

pub struct ViewOpts {
    pub grid: bool,
    pub axes: bool,
    pub section: Option<(DVec3, f64)>,
    pub scene_radius: f64,
}

/// Sammelt alle Primitive, tiefensortiert (fern → nah).
pub fn collect(
    cam: &Camera,
    rect: Rect,
    objs: &[(u64, &Built)],
    opts: &ViewOpts,
    picks: &mut Vec<Pick>,
) -> Vec<Item> {
    let b = cam.basis();
    let light = (-b.fwd * 0.55 + b.up * 0.65 + b.right * 0.35).normalize();
    let mut items: Vec<Item> = vec![];
    let proj = |p: DVec3| cam.project(&b, rect, p);

    let push_line = |items: &mut Vec<Item>,
                     a: DVec3,
                     c: DVec3,
                     w: f32,
                     col: Color32,
                     pieces: usize,
                     key_override: Option<f64>| {
        let n = pieces.max(1);
        for k in 0..n {
            let p0 = a.lerp(c, k as f64 / n as f64);
            let p1 = a.lerp(c, (k + 1) as f64 / n as f64);
            if let (Some((s0, z0)), Some((s1, z1))) = (proj(p0), proj(p1)) {
                items.push(Item {
                    key: key_override.unwrap_or((z0 + z1) * 0.5),
                    prim: Prim::Line(s0, s1, w, col),
                });
            }
        }
    };

    // Boden-Gitter und Achsen
    let r = opts.scene_radius.max(0.5);
    if opts.grid {
        let step = 10f64.powf((r / 2.0).log10().floor()).max(1e-3);
        let n = (r * 1.6 / step).ceil() as i64;
        let ext = n as f64 * step;
        let col = Color32::from_rgba_unmultiplied(120, 130, 150, 45);
        for i in -n..=n {
            let x = i as f64 * step;
            push_line(
                &mut items,
                DVec3::new(x, -ext, 0.0),
                DVec3::new(x, ext, 0.0),
                1.0,
                col,
                6,
                None,
            );
            push_line(
                &mut items,
                DVec3::new(-ext, x, 0.0),
                DVec3::new(ext, x, 0.0),
                1.0,
                col,
                6,
                None,
            );
        }
    }
    if opts.axes {
        let l = r * 1.4;
        for (d, col, name) in [
            (DVec3::X, Color32::from_rgb(230, 90, 90), "x"),
            (DVec3::Y, Color32::from_rgb(90, 200, 110), "y"),
            (DVec3::Z, Color32::from_rgb(100, 140, 255), "z"),
        ] {
            push_line(&mut items, DVec3::ZERO, d * l, 1.6, col, 8, None);
            if let Some((s, _)) = proj(d * l * 1.05) {
                items.push(Item {
                    key: f64::MIN,
                    prim: Prim::Text(s, name.into(), col),
                });
            }
        }
    }

    for (id, bo) in objs {
        let (dmin, dmax) = match proj(bo.center) {
            Some((_, z)) => (z - bo.radius * 1.01, z + bo.radius * 1.01),
            None => (0.0, 0.0),
        };
        let eps = bo.radius.max(1e-6) * 1e-4;
        let front: Vec<bool> = bo
            .faces
            .iter()
            .map(|f| cam.is_front(&b, f.pts[0], f.normal))
            .collect();
        // Flächen konvexer Körper
        for (fi, f) in bo.faces.iter().enumerate() {
            let pts: Option<Vec<Pos2>> = f.pts.iter().map(|p| proj(*p).map(|x| x.0)).collect();
            let Some(pts) = pts else { continue };
            let col = shade(bo.face_color, f.normal, light, false);
            let key = if front[fi] {
                dmin - 2.0 * eps
            } else {
                dmax + 2.0 * eps
            };
            items.push(Item {
                key,
                prim: Prim::Poly(pts, col),
            });
        }
        // Netze
        for m in &bo.meshes {
            let sp: Vec<Option<(Pos2, f64)>> = m.mesh.v.iter().map(|p| proj(*p)).collect();
            let cols: Vec<Color32> = m
                .normals
                .iter()
                .map(|n| shade(m.color, *n, light, true))
                .collect();
            for t in &m.mesh.t {
                let (Some(a), Some(c), Some(d)) =
                    (sp[t[0] as usize], sp[t[1] as usize], sp[t[2] as usize])
                else {
                    continue;
                };
                items.push(Item {
                    key: (a.1 + c.1 + d.1) / 3.0,
                    prim: Prim::Tri(
                        [a.0, c.0, d.0],
                        [
                            cols[t[0] as usize],
                            cols[t[1] as usize],
                            cols[t[2] as usize],
                        ],
                    ),
                });
            }
        }
        // Linien
        let piece_len = (bo.radius * 0.12).max(1e-6);
        for l in &bo.lines {
            if !l.faces.is_empty() && bo.convex {
                let vis = l
                    .faces
                    .iter()
                    .any(|f| front.get(*f).copied().unwrap_or(true));
                let key = if vis { dmin - 3.0 * eps } else { dmax + eps };
                let col = if vis {
                    l.color
                } else {
                    l.color.gamma_multiply(0.75)
                };
                push_line(
                    &mut items,
                    l.a,
                    l.b,
                    if vis { l.width } else { l.width * 0.8 },
                    col,
                    1,
                    Some(key),
                );
            } else {
                let pieces = ((l.a - l.b).length() / piece_len).ceil().clamp(1.0, 14.0) as usize;
                push_line(&mut items, l.a, l.b, l.width, l.color, pieces, None);
            }
        }
        // Punkte
        for p in &bo.points {
            let Some((s, z)) = proj(p.p) else { continue };
            let key = if !p.faces.is_empty() && bo.convex {
                if p.faces
                    .iter()
                    .any(|f| front.get(*f).copied().unwrap_or(true))
                {
                    dmin - 4.0 * eps
                } else {
                    dmax + eps
                }
            } else {
                z - eps
            };
            if p.r > 0.0 {
                items.push(Item {
                    key,
                    prim: Prim::Dot(s, p.r, p.color),
                });
            }
            if let Some(lab) = &p.label {
                items.push(Item {
                    key: f64::MIN,
                    prim: Prim::Text(
                        s + egui::vec2(5.0, -5.0),
                        lab.clone(),
                        Color32::from_gray(225),
                    ),
                });
            }
            if let Some(i) = p.pick {
                picks.push(Pick {
                    pos: s,
                    obj: *id,
                    idx: i,
                });
            }
        }
        for (p, t, c) in &bo.labels {
            if let Some((s, _)) = proj(*p) {
                items.push(Item {
                    key: f64::MIN,
                    prim: Prim::Text(s, t.clone(), *c),
                });
            }
        }
        // Schnittebene
        if let (Some((n, d)), Some(m)) = (opts.section, &bo.section_mesh) {
            let (segs, _) = crate::mesh::slice(m, n, d);
            for (a, c) in segs {
                push_line(
                    &mut items,
                    a,
                    c,
                    2.6,
                    Color32::from_rgb(255, 235, 80),
                    1,
                    None,
                );
            }
        }
    }
    if let Some((n, d)) = opts.section {
        let u = n.any_orthonormal_vector();
        let v = n.cross(u);
        let o = n * d;
        let e = r * 1.3;
        let corners = [
            o + (u + v) * e,
            o + (u - v) * e,
            o - (u + v) * e,
            o - (u - v) * e,
        ];
        for k in 0..4 {
            push_line(
                &mut items,
                corners[k],
                corners[(k + 1) % 4],
                1.2,
                Color32::from_rgba_unmultiplied(255, 235, 80, 120),
                8,
                None,
            );
        }
    }
    items.sort_by(|a, c| {
        c.key
            .partial_cmp(&a.key)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    items
}

/// Zeichnet die Primitive; aufeinanderfolgende Dreiecke werden gebündelt.
pub fn paint(painter: &egui::Painter, items: &[Item]) {
    let mut mesh = epaint::Mesh::default();
    let flush = |mesh: &mut epaint::Mesh, painter: &egui::Painter| {
        if !mesh.is_empty() {
            painter.add(egui::Shape::mesh(std::mem::take(mesh)));
        }
    };
    for it in items {
        match &it.prim {
            Prim::Tri(p, c) => {
                let i = mesh.vertices.len() as u32;
                for k in 0..3 {
                    mesh.colored_vertex(p[k], c[k]);
                }
                mesh.add_triangle(i, i + 1, i + 2);
                if mesh.vertices.len() > 60_000 {
                    flush(&mut mesh, painter);
                }
            }
            Prim::Poly(p, c) => {
                let i = mesh.vertices.len() as u32;
                for q in p {
                    mesh.colored_vertex(*q, *c);
                }
                for k in 1..p.len() as u32 - 1 {
                    mesh.add_triangle(i, i + k, i + k + 1);
                }
            }
            other => {
                flush(&mut mesh, painter);
                match other {
                    Prim::Line(a, b, w, c) => {
                        painter.line_segment([*a, *b], Stroke::new(*w, *c));
                    }
                    Prim::Dot(p, r, c) => {
                        painter.circle_filled(*p, *r, *c);
                    }
                    Prim::Text(p, t, c) => {
                        painter.text(
                            *p,
                            egui::Align2::LEFT_BOTTOM,
                            t,
                            egui::FontId::proportional(13.0),
                            *c,
                        );
                    }
                    _ => {}
                }
            }
        }
    }
    flush(&mut mesh, painter);
}
