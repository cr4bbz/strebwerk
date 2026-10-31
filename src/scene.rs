//! Szene: Definitionen (werden gespeichert) und Bau der darstellbaren Geometrie.

use crate::algebra::*;
use crate::expr::{self, Env, Ex};
use crate::mesh::{self, LatticeKind, Mesh, Region, Tpms};
use crate::poly::{self, Analysis, FLine, SymInfo, V3};
use eframe::egui::Color32;
use glam::DVec3;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::hash::{Hash, Hasher};

// ---------------------------------------------------------------------------
// Definitionen
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Param {
    pub name: String,
    pub value: f64,
    pub min: f64,
    pub max: f64,
    #[serde(default)]
    pub animate: bool,
    #[serde(default = "one")]
    pub speed: f64,
    #[serde(skip, default = "one")]
    pub dir: f64,
}
fn one() -> f64 {
    1.0
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CustomGen {
    pub key: String,
    pub poly: String,
    pub approx: f64,
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub enum Platonic {
    Tetra,
    Cube,
    Octa,
    Dodeca,
    Icosa,
}

impl Platonic {
    pub fn name(&self) -> &'static str {
        match self {
            Platonic::Tetra => "Tetraeder",
            Platonic::Cube => "Würfel",
            Platonic::Octa => "Oktaeder",
            Platonic::Dodeca => "Dodekaeder",
            Platonic::Icosa => "Ikosaeder",
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub enum PolyBase {
    Platonic { kind: Platonic, a: String },
    Cuboid { a: String, b: String, c: String },
    Prism { n: u32, r: String, h: String },
    Antiprism { n: u32, r: String, h: String },
    Pyramid { n: u32, r: String, h: String },
    Bipyramid { n: u32, r: String, h: String },
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub enum PolyOp {
    /// Ecken abschneiden: neue Ecken auf jeder Kante im Abstand t·Kantenlänge von beiden Enden
    Truncate(String),
    /// Flächenschwerpunkte als neue Ecken
    Dual,
    Scale(String),
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub enum Kind {
    Polyhedron {
        base: PolyBase,
        ops: Vec<PolyOp>,
    },
    Free {
        pts: Vec<[String; 3]>,
        struts: Vec<(usize, usize)>,
        hull: bool,
    },
    ParamSurface {
        x: String,
        y: String,
        z: String,
        u0: String,
        u1: String,
        v0: String,
        v1: String,
        nu: u32,
        nv: u32,
    },
    Implicit {
        f: String,
        lo: [String; 3],
        hi: [String; 3],
        res: u32,
    },
    Curve {
        x: String,
        y: String,
        z: String,
        t0: String,
        t1: String,
        n: u32,
    },
}

impl Kind {
    pub fn type_name(&self) -> &'static str {
        match self {
            Kind::Polyhedron { .. } => "Polyeder",
            Kind::Free { .. } => "Freie Konstruktion",
            Kind::ParamSurface { .. } => "Parameterfläche",
            Kind::Implicit { .. } => "Implizite Fläche",
            Kind::Curve { .. } => "Raumkurve",
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(default)]
pub struct Layers {
    pub faces: bool,
    pub alpha: f32,
    pub edges: bool,
    pub vertices: bool,
    pub labels: bool,
    pub face_diag: bool,
    pub space_diag: bool,
    pub all_pairs: bool,
    pub dual: bool,
    pub spokes: bool,
    pub axes: bool,
    pub mirrors: bool,
    pub spheres: bool,
    pub cone_tet: bool,
    pub lattice: bool,
    pub lattice_kind: LatticeKind,
    pub lattice_spacing: f64,
    pub tpms: bool,
    pub tpms_kind: Tpms,
    pub tpms_period: f64,
    pub iso: bool,
    pub iso_u: u32,
    pub iso_v: u32,
    pub chords: bool,
    pub chord_du: f64,
    pub chord_dv: f64,
    pub chord_n: u32,
    pub normals: bool,
    pub normal_len: f64,
    pub contours: bool,
    pub contour_n: u32,
    pub frenet: bool,
    pub frenet_n: u32,
    pub osc: bool,
    pub osc_n: u32,
}

impl Default for Layers {
    fn default() -> Self {
        Layers {
            faces: true,
            alpha: 0.30,
            edges: true,
            vertices: true,
            labels: false,
            face_diag: false,
            space_diag: true,
            all_pairs: false,
            dual: false,
            spokes: false,
            axes: false,
            mirrors: false,
            spheres: false,
            cone_tet: false,
            lattice: false,
            lattice_kind: LatticeKind::Octet,
            lattice_spacing: 0.35,
            tpms: false,
            tpms_kind: Tpms::Gyroid,
            tpms_period: 0.9,
            iso: true,
            iso_u: 24,
            iso_v: 12,
            chords: false,
            chord_du: 0.5,
            chord_dv: 0.0,
            chord_n: 24,
            normals: false,
            normal_len: 0.15,
            contours: false,
            contour_n: 12,
            frenet: false,
            frenet_n: 16,
            osc: false,
            osc_n: 6,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Obj {
    pub id: u64,
    pub name: String,
    pub visible: bool,
    pub kind: Kind,
    pub offset: [String; 3],
    pub color: [u8; 3],
    #[serde(default)]
    pub layers: Layers,
    #[serde(default)]
    pub hidden_classes: Vec<usize>,
}

/// Schnittebene {x : n·x = d}; Normale und Lage sind Ausdrücke (exakt, falls möglich).
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct Section {
    pub name: String,
    pub normal: [String; 3],
    pub offset: String,
    pub visible: bool,
    /// Cavalieri-Profil A(h) über die ganze Ausdehnung mitrechnen
    #[serde(default)]
    pub profile: bool,
}

impl Section {
    pub fn new(name: &str, axis: usize, offset: &str) -> Section {
        let mut n = [String::from("0"), String::from("0"), String::from("0")];
        n[axis] = "1".into();
        Section {
            name: name.into(),
            normal: n,
            offset: offset.into(),
            visible: true,
            profile: true,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct Scene {
    pub params: Vec<Param>,
    pub gens: Vec<CustomGen>,
    pub objects: Vec<Obj>,
    pub next_id: u64,
    #[serde(default)]
    pub sections: Vec<Section>,
}

impl Scene {
    pub fn add(&mut self, name: &str, kind: Kind) -> u64 {
        self.next_id += 1;
        let palette = [
            [120, 170, 255],
            [255, 160, 90],
            [140, 220, 150],
            [230, 120, 200],
            [240, 210, 100],
            [120, 220, 230],
        ];
        let color = palette[(self.next_id as usize) % palette.len()];
        self.objects.push(Obj {
            id: self.next_id,
            name: name.into(),
            visible: true,
            kind,
            offset: ["0".into(), "0".into(), "0".into()],
            color,
            layers: Layers::default(),
            hidden_classes: vec![],
        });
        self.next_id
    }

    pub fn custom_gendefs(&self) -> (Vec<GenDef>, Vec<String>) {
        let mut out = vec![];
        let mut errs = vec![];
        for (i, g) in self.gens.iter().enumerate() {
            match custom_gen(g, i) {
                Ok(d) => out.push(d),
                Err(e) => errs.push(format!("Generator {}: {e}", g.key)),
            }
        }
        (out, errs)
    }

    pub fn env(&self, gens: &[GenDef]) -> Env {
        let ps: Vec<(String, f64)> = self
            .params
            .iter()
            .map(|p| (p.name.clone(), p.value))
            .collect();
        Env::new(&ps, gens)
    }
}

fn greek(key: &str) -> (String, String) {
    let table = [
        ("alpha", "α"),
        ("beta", "β"),
        ("delta", "δ"),
        ("epsilon", "ε"),
        ("eta", "η"),
        ("theta", "θ"),
        ("kappa", "κ"),
        ("lambda", "λ"),
        ("mu", "μ"),
        ("nu", "ν"),
        ("xi", "ξ"),
        ("omega", "ω"),
        ("psi", "ψ"),
        ("chi", "χ"),
        ("zeta", "ζ"),
    ];
    for (k, g) in table {
        if key == k {
            return (g.into(), format!("\\{k}"));
        }
    }
    (
        key.into(),
        if key.chars().count() > 1 {
            format!("\\mathit{{{key}}}")
        } else {
            key.into()
        },
    )
}

/// Eigener Generator aus Minimalpolynom und Näherungswert.
pub fn custom_gen(g: &CustomGen, i: usize) -> Result<GenDef, String> {
    if expr::RESERVED.contains(&g.key.as_str())
        || expr::FUNCS.contains(&g.key.as_str())
        || g.key.starts_with("gamma")
        || g.key.starts_with("sigma")
    {
        return Err("Name ist reserviert".into());
    }
    let e = expr::parse(&g.poly)?;
    let ring = Ring::new(
        vec![],
        vec![ParamInfo {
            name: "x".into(),
            value: 0.0,
            nonneg: false,
        }],
    )?;
    let s = expr::exact(&e, &ring)?;
    if !s.den.is_one() {
        return Err("Polynom erwartet".into());
    }
    let mut coeffs: Vec<Q> = vec![];
    for (ex, c) in &s.num.t {
        let k = ex[0] as usize;
        if coeffs.len() <= k {
            coeffs.resize(k + 1, qi(0));
        }
        coeffs[k] = c.clone();
    }
    if coeffs.len() < 3 {
        return Err("Grad ≥ 2 nötig".into());
    }
    if coeffs.len() > 9 {
        return Err("Grad ≤ 8 unterstützt".into());
    }
    // Newton von der Näherung aus
    let f: Vec<f64> = coeffs.iter().map(qf).collect();
    let mut x = g.approx;
    for _ in 0..100 {
        let (mut p, mut dp) = (0.0, 0.0);
        for c in f.iter().rev() {
            dp = dp * x + p;
            p = p * x + c;
        }
        if dp.abs() < 1e-300 {
            break;
        }
        x -= p / dp;
    }
    let (uni, tex) = greek(&g.key);
    let mut d = simple_gen(&g.key, &uni, &tex, coeffs, x, 10 + i as u32, "");
    d.meaning = format!("{uni} ≈ {x:.12}, Nullstelle von {}", g.poly);
    // Irreduzibilität für Grad 2/3: keine rationale Nullstelle (rationaler Nullstellentest)
    Ok(d)
}

// ---------------------------------------------------------------------------
// Darstellbare Geometrie
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct RFace {
    pub pts: Vec<DVec3>,
    pub normal: DVec3,
}

#[derive(Clone, Debug)]
pub struct RLine {
    /// Bezeichnung, unter der die Linie an Interaktionen teilnimmt (None = nur Darstellung)
    pub inter: Option<String>,
    /// Index in `Built::struts`, falls die Linie eine exakt bekannte Strebe ist
    pub sref: Option<usize>,
    pub a: DVec3,
    pub b: DVec3,
    pub color: Color32,
    pub width: f32,
    /// Angrenzende Flächen (für Sichtbarkeit bei konvexen Körpern)
    pub faces: Vec<usize>,
    pub strut: bool,
}

#[derive(Clone, Debug)]
pub struct RPoint {
    pub p: DVec3,
    pub color: Color32,
    pub r: f32,
    pub label: Option<String>,
    pub faces: Vec<usize>,
    pub pick: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct RMesh {
    pub mesh: Mesh,
    pub normals: Vec<DVec3>,
    pub color: Color32,
}

#[derive(Clone, Debug, Default)]
pub struct Built {
    pub faces: Vec<RFace>,
    pub face_color: Color32,
    pub meshes: Vec<RMesh>,
    pub lines: Vec<RLine>,
    pub points: Vec<RPoint>,
    pub labels: Vec<(DVec3, String, Color32)>,
    pub center: DVec3,
    pub radius: f64,
    pub formulas: Vec<FLine>,
    pub legend: Vec<String>,
    pub legend_tex: Vec<String>,
    pub def_tex: Vec<String>,
    pub class_colors: Vec<Color32>,
    pub sym: Option<SymInfo>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub section_mesh: Option<Mesh>,
    pub convex: bool,
    /// Ecken in dem Indexraum, den auch `exact_verts` liefert (inkl. Verschiebung)
    pub verts: Vec<DVec3>,
    /// sichtbare, exakt bekannte Streben (Indexpaare in `verts`)
    pub struts: Vec<StrutRef>,
    /// Flächen der konvexen Hülle (Indizes in `verts`), falls Körper
    pub hull_faces: Vec<Vec<usize>>,
    /// Halbräume n·x ≤ d der Hülle (numerisch, inkl. Verschiebung)
    pub hull_planes: Vec<(DVec3, f64)>,
    /// Koordinaten sind exakt in K bekannt
    pub exact_ok: bool,
}

#[derive(Clone, Debug)]
pub struct StrutRef {
    pub i: usize,
    pub j: usize,
    pub label: String,
    pub tex: String,
}

pub fn class_color(i: usize) -> Color32 {
    let h = (i as f32 * 0.618_034 + 0.08).fract();
    hsv(h, 0.62, 1.0)
}

pub fn hsv(h: f32, s: f32, v: f32) -> Color32 {
    let i = (h * 6.0).floor();
    let f = h * 6.0 - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    let (r, g, b) = match (i as i32).rem_euclid(6) {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}

pub fn axis_color(order: u32) -> Color32 {
    match order {
        2 => Color32::from_rgb(90, 160, 255),
        3 => Color32::from_rgb(100, 215, 120),
        4 => Color32::from_rgb(255, 170, 60),
        5 => Color32::from_rgb(255, 90, 90),
        _ => Color32::from_rgb(200, 130, 255),
    }
}

fn with_alpha(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a.clamp(0.0, 1.0) * 255.0) as u8)
}

/// Exakter Cache: Schlüssel → Analyse
pub type Cache = HashMap<u64, Analysis>;

fn hash_of<T: Hash>(x: &T) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    x.hash(&mut h);
    h.finish()
}

pub(crate) struct Ctx<'a> {
    scene: &'a Scene,
    pub(crate) custom: Vec<GenDef>,
    pub(crate) env: Env,
}

impl<'a> Ctx<'a> {
    pub(crate) fn new(scene: &'a Scene) -> Ctx<'a> {
        let (custom, _) = scene.custom_gendefs();
        let env = scene.env(&custom);
        Ctx { scene, custom, env }
    }
    pub(crate) fn params(&self) -> Vec<String> {
        self.scene.params.iter().map(|p| p.name.clone()).collect()
    }
    pub(crate) fn ring(&self, keys: &BTreeSet<String>) -> Result<Ring, String> {
        let defs = resolve_gens(keys, &self.custom)?;
        let ps = self
            .scene
            .params
            .iter()
            .map(|p| ParamInfo {
                name: p.name.clone(),
                value: p.value,
                nonneg: p.min >= 0.0,
            })
            .collect();
        Ring::new(defs, ps)
    }
    pub(crate) fn num(&self, s: &str) -> Result<f64, String> {
        expr::num(s, &self.env).map_err(|e| format!("„{s}“: {e}"))
    }
}

pub fn build(obj: &Obj, scene: &Scene, cache: &mut Cache) -> Built {
    let (custom, mut gen_errs) = scene.custom_gendefs();
    let env = scene.env(&custom);
    let ctx = Ctx { scene, custom, env };
    let mut b = Built {
        face_color: with_alpha(
            Color32::from_rgb(obj.color[0], obj.color[1], obj.color[2]),
            obj.layers.alpha,
        ),
        ..Default::default()
    };
    b.errors.append(&mut gen_errs);
    let off = {
        let mut o = DVec3::ZERO;
        for k in 0..3 {
            match ctx.num(&obj.offset[k]) {
                Ok(v) => o[k] = v,
                Err(e) => b.errors.push(format!("Verschiebung: {e}")),
            }
        }
        o
    };
    let res = match &obj.kind {
        Kind::Polyhedron { base, ops } => build_polyhedron(obj, base, ops, &ctx, cache, &mut b),
        Kind::Free { pts, struts, hull } => {
            build_free(obj, pts, struts, *hull, &ctx, cache, &mut b)
        }
        Kind::ParamSurface {
            x,
            y,
            z,
            u0,
            u1,
            v0,
            v1,
            nu,
            nv,
        } => build_param(obj, [x, y, z], [u0, u1, v0, v1], *nu, *nv, &ctx, &mut b),
        Kind::Implicit { f, lo, hi, res } => build_implicit(obj, f, lo, hi, *res, &ctx, &mut b),
        Kind::Curve { x, y, z, t0, t1, n } => build_curve(obj, [x, y, z], t0, t1, *n, &ctx, &mut b),
    };
    if let Err(e) = res {
        b.errors.push(e);
    }
    // Verschiebung anwenden
    if off != DVec3::ZERO {
        for f in &mut b.faces {
            f.pts.iter_mut().for_each(|p| *p += off);
        }
        for m in &mut b.meshes {
            m.mesh.v.iter_mut().for_each(|p| *p += off);
        }
        for l in &mut b.lines {
            l.a += off;
            l.b += off;
        }
        for p in &mut b.points {
            p.p += off;
        }
        for l in &mut b.labels {
            l.0 += off;
        }
        if let Some(m) = &mut b.section_mesh {
            m.v.iter_mut().for_each(|p| *p += off);
        }
        b.verts.iter_mut().for_each(|p| *p += off);
        for (n, d) in &mut b.hull_planes {
            *d += n.dot(off);
        }
        b.center += off;
    }
    b
}

// ---------------------------------------------------------------------------
// Polyeder
// ---------------------------------------------------------------------------

fn parse_all(strs: &[&String]) -> Result<Vec<Ex>, String> {
    strs.iter()
        .map(|s| expr::parse(s).map_err(|e| format!("„{s}“: {e}")))
        .collect()
}

fn ex_exact(r: &Ring, e: &Ex, src: &str) -> Result<Sym, String> {
    expr::exact(e, r).map_err(|err| format!("„{src}“: {err}"))
}

fn sv(x: Sym, y: Sym, z: Sym) -> V3 {
    [x, y, z]
}

fn base_strings(base: &PolyBase) -> Vec<&String> {
    match base {
        PolyBase::Platonic { a, .. } => vec![a],
        PolyBase::Cuboid { a, b, c } => vec![a, b, c],
        PolyBase::Prism { r, h, .. }
        | PolyBase::Antiprism { r, h, .. }
        | PolyBase::Pyramid { r, h, .. }
        | PolyBase::Bipyramid { r, h, .. } => {
            vec![r, h]
        }
    }
}

fn base_gens(base: &PolyBase) -> Vec<String> {
    let cyc = |n: u32| {
        let (g, s) = cyc_terms(n);
        terms_keys(&g)
            .into_iter()
            .chain(terms_keys(&s))
            .collect::<Vec<_>>()
    };
    match base {
        PolyBase::Platonic { kind, .. } => match kind {
            Platonic::Tetra | Platonic::Octa => vec!["sqrt2".into()],
            Platonic::Dodeca | Platonic::Icosa => vec!["phi".into()],
            Platonic::Cube => vec![],
        },
        PolyBase::Cuboid { .. } => vec![],
        PolyBase::Prism { n, .. } | PolyBase::Pyramid { n, .. } | PolyBase::Bipyramid { n, .. } => {
            cyc(*n)
        }
        PolyBase::Antiprism { n, .. } => cyc(2 * n),
    }
}

fn base_vertices(r: &Ring, base: &PolyBase, ex: &[Ex]) -> Result<Vec<V3>, String> {
    let c = |x: Q| r.sym_c(x);
    let z = || r.sym_c(qi(0));
    let mut out = vec![];
    match base {
        PolyBase::Platonic { kind, a } => {
            let av = ex_exact(r, &ex[0], a)?;
            match kind {
                Platonic::Cube => {
                    let h = r.sscale(&av, qr(1, 2));
                    for sx in [-1, 1] {
                        for sy in [-1, 1] {
                            for sz in [-1, 1] {
                                out.push(sv(
                                    r.sscale(&h, qi(sx)),
                                    r.sscale(&h, qi(sy)),
                                    r.sscale(&h, qi(sz)),
                                ));
                            }
                        }
                    }
                }
                Platonic::Tetra => {
                    let k = r.smul(&av, &r.sym(r.gen_poly("sqrt2").unwrap().scale(&qr(1, 4))));
                    for (x, y, zz) in [(1, 1, 1), (1, -1, -1), (-1, 1, -1), (-1, -1, 1)] {
                        out.push(sv(
                            r.sscale(&k, qi(x)),
                            r.sscale(&k, qi(y)),
                            r.sscale(&k, qi(zz)),
                        ));
                    }
                }
                Platonic::Octa => {
                    let k = r.smul(&av, &r.sym(r.gen_poly("sqrt2").unwrap().scale(&qr(1, 2))));
                    for s in [-1, 1] {
                        let v = r.sscale(&k, qi(s));
                        out.push(sv(v.clone(), z(), z()));
                        out.push(sv(z(), v.clone(), z()));
                        out.push(sv(z(), z(), v));
                    }
                }
                Platonic::Icosa | Platonic::Dodeca => {
                    let phi = r.sym(r.gen_poly("phi").unwrap());
                    let iphi = r.ssub(&phi, &c(qi(1)));
                    let k = if *kind == Platonic::Icosa {
                        r.sscale(&av, qr(1, 2))
                    } else {
                        r.smul(&av, &r.sscale(&phi, qr(1, 2)))
                    };
                    let mut raw: Vec<[Sym; 3]> = vec![];
                    if *kind == Platonic::Icosa {
                        for s1 in [-1, 1] {
                            for s2 in [-1, 1] {
                                raw.push([z(), c(qi(s1)), r.sscale(&phi, qi(s2))]);
                            }
                        }
                    } else {
                        for sx in [-1, 1] {
                            for sy in [-1, 1] {
                                for sz in [-1, 1] {
                                    out.push(sv(
                                        r.sscale(&k, qi(sx)),
                                        r.sscale(&k, qi(sy)),
                                        r.sscale(&k, qi(sz)),
                                    ));
                                }
                            }
                        }
                        for s1 in [-1, 1] {
                            for s2 in [-1, 1] {
                                raw.push([z(), r.sscale(&iphi, qi(s1)), r.sscale(&phi, qi(s2))]);
                            }
                        }
                    }
                    for p in raw {
                        for rot in 0..3 {
                            let q = [
                                p[(3 - rot) % 3].clone(),
                                p[(4 - rot) % 3].clone(),
                                p[(5 - rot) % 3].clone(),
                            ];
                            out.push(sv(r.smul(&q[0], &k), r.smul(&q[1], &k), r.smul(&q[2], &k)));
                        }
                    }
                }
            }
        }
        PolyBase::Cuboid { a, b, c: cc } => {
            let av = r.sscale(&ex_exact(r, &ex[0], a)?, qr(1, 2));
            let bv = r.sscale(&ex_exact(r, &ex[1], b)?, qr(1, 2));
            let cv = r.sscale(&ex_exact(r, &ex[2], cc)?, qr(1, 2));
            for sx in [-1, 1] {
                for sy in [-1, 1] {
                    for sz in [-1, 1] {
                        out.push(sv(
                            r.sscale(&av, qi(sx)),
                            r.sscale(&bv, qi(sy)),
                            r.sscale(&cv, qi(sz)),
                        ));
                    }
                }
            }
        }
        PolyBase::Prism { n, r: rs, h }
        | PolyBase::Antiprism { n, r: rs, h }
        | PolyBase::Pyramid { n, r: rs, h }
        | PolyBase::Bipyramid { n, r: rs, h } => {
            let n = *n;
            if !(3..=60).contains(&n) {
                return Err("n muss zwischen 3 und 60 liegen".into());
            }
            let rv = ex_exact(r, &ex[0], rs)?;
            let hv = ex_exact(r, &ex[1], h)?;
            let hh = r.sscale(&hv, qr(1, 2));
            let ring_pt = |k: i64, nn: u32, zz: &Sym| {
                let (cs, sn) = expr::cyc_cos_sin(r, k, nn);
                sv(r.smul(&rv, &cs), r.smul(&rv, &sn), zz.clone())
            };
            match base {
                PolyBase::Prism { .. } => {
                    for k in 0..n as i64 {
                        out.push(ring_pt(k, n, &r.sneg(&hh)));
                        out.push(ring_pt(k, n, &hh));
                    }
                }
                PolyBase::Antiprism { .. } => {
                    for k in 0..n as i64 {
                        out.push(ring_pt(2 * k, 2 * n, &r.sneg(&hh)));
                        out.push(ring_pt(2 * k + 1, 2 * n, &hh));
                    }
                }
                PolyBase::Pyramid { .. } => {
                    for k in 0..n as i64 {
                        out.push(ring_pt(k, n, &z()));
                    }
                    out.push(sv(z(), z(), hv.clone()));
                }
                _ => {
                    for k in 0..n as i64 {
                        out.push(ring_pt(k, n, &z()));
                    }
                    out.push(sv(z(), z(), hv.clone()));
                    out.push(sv(z(), z(), r.sneg(&hv)));
                }
            }
        }
    }
    Ok(out)
}

/// Exakte Ecken nach Dedupe + Hülle
fn hull_of(r: &Ring, ex: &[V3]) -> Result<(Vec<V3>, Vec<DVec3>, poly::Hull), String> {
    let num: Vec<DVec3> = ex.iter().map(|p| poly::v3_eval(r, p)).collect();
    if num.iter().any(|p| !p.is_finite()) {
        return Err("Koordinate nicht endlich (Division durch null?)".into());
    }
    let keep = poly::dedupe(&num);
    let ex2: Vec<V3> = keep.iter().map(|i| ex[*i].clone()).collect();
    let num2: Vec<DVec3> = keep.iter().map(|i| num[*i]).collect();
    let h = poly::convex_hull(&num2)?;
    Ok((ex2, num2, h))
}

fn poly_strs<'a>(base: &'a PolyBase, ops: &'a [PolyOp]) -> Vec<&'a String> {
    let mut strs = base_strings(base);
    for op in ops {
        if let PolyOp::Truncate(s) | PolyOp::Scale(s) = op {
            strs.push(s);
        }
    }
    strs
}

fn keys_of(strs: &[&String], ctx: &Ctx, keys: &mut BTreeSet<String>) -> Result<(), String> {
    for e in parse_all(strs)? {
        expr::collect_gens(&e, &ctx.env, &ctx.params(), &ctx.custom, keys)?;
    }
    Ok(())
}

fn poly_keys(base: &PolyBase, ops: &[PolyOp], ctx: &Ctx) -> Result<BTreeSet<String>, String> {
    let mut keys: BTreeSet<String> = base_gens(base).into_iter().collect();
    keys_of(&poly_strs(base, ops), ctx, &mut keys)?;
    Ok(keys)
}

/// Exakte Ecken, Zahlwerte und Hülle eines Polyeders im Ring `r`.
fn poly_exact(
    base: &PolyBase,
    ops: &[PolyOp],
    r: &Ring,
) -> Result<(Vec<V3>, Vec<DVec3>, poly::Hull), String> {
    let strs = poly_strs(base, ops);
    let exs = parse_all(&strs)?;
    let nb = base_strings(base).len();
    let mut ex = base_vertices(r, base, &exs[..nb])?;
    let mut k = nb;
    for op in ops {
        match op {
            PolyOp::Truncate(_) => {
                let t = ex_exact(r, &exs[k], strs[k])?;
                k += 1;
                let (e2, _, h) = hull_of(r, &ex)?;
                let mut edges = BTreeSet::new();
                for f in &h.faces {
                    for i in 0..f.len() {
                        let (p, q) = (f[i], f[(i + 1) % f.len()]);
                        edges.insert((p.min(q), p.max(q)));
                    }
                }
                let mut nx = vec![];
                for (p, q) in edges {
                    let d = poly::v3_sub(r, &e2[q], &e2[p]);
                    nx.push(poly::v3_add(r, &e2[p], &poly::v3_scale(r, &d, &t)));
                    nx.push(poly::v3_sub(r, &e2[q], &poly::v3_scale(r, &d, &t)));
                }
                ex = nx;
            }
            PolyOp::Dual => {
                let (e2, _, h) = hull_of(r, &ex)?;
                ex = h
                    .faces
                    .iter()
                    .map(|f| {
                        let mut s = e2[f[0]].clone();
                        for i in &f[1..] {
                            s = poly::v3_add(r, &s, &e2[*i]);
                        }
                        poly::v3_scale(r, &s, &r.sym_c(qr(1, f.len() as i64)))
                    })
                    .collect();
            }
            PolyOp::Scale(_) => {
                let s = ex_exact(r, &exs[k], strs[k])?;
                k += 1;
                ex = ex.iter().map(|p| poly::v3_scale(r, p, &s)).collect();
            }
        }
    }
    hull_of(r, &ex)
}

fn free_exact(pts: &[[String; 3]], r: &Ring) -> Result<Vec<V3>, String> {
    pts.iter()
        .map(|p| {
            let mut v = vec![];
            for c in p {
                let e = expr::parse(c).map_err(|e| format!("„{c}“: {e}"))?;
                v.push(ex_exact(r, &e, c)?);
            }
            Ok([v[0].clone(), v[1].clone(), v[2].clone()])
        })
        .collect()
}

/// Generatoren, die ein Objekt für seine exakten Koordinaten (inkl. Verschiebung) braucht.
pub fn gen_keys(obj: &Obj, scene: &Scene) -> Result<BTreeSet<String>, String> {
    let ctx = Ctx::new(scene);
    let mut keys = match &obj.kind {
        Kind::Polyhedron { base, ops } => poly_keys(base, ops, &ctx)?,
        Kind::Free { pts, .. } => {
            let mut k = BTreeSet::new();
            keys_of(&pts.iter().flatten().collect::<Vec<_>>(), &ctx, &mut k)?;
            k
        }
        _ => {
            return Err(format!(
                "{} hat keine exakten Koordinaten",
                obj.kind.type_name()
            ))
        }
    };
    keys_of(&obj.offset.iter().collect::<Vec<_>>(), &ctx, &mut keys)?;
    Ok(keys)
}

/// Ring über der Szene (alle Parameter) mit den gegebenen Generatoren.
pub fn scene_ring(scene: &Scene, keys: &BTreeSet<String>) -> Result<Ring, String> {
    Ctx::new(scene).ring(keys)
}

/// Generatoren, die Ausdrücke brauchen (z. B. Normale und Lage einer Ebene).
pub fn gen_keys_of(strs: &[&String], scene: &Scene) -> Result<BTreeSet<String>, String> {
    let ctx = Ctx::new(scene);
    let mut k = BTreeSet::new();
    keys_of(strs, &ctx, &mut k)?;
    Ok(k)
}

/// Exakte Ecken im selben Indexraum wie `Built::verts` (inkl. Verschiebung), im Ring `r`.
pub fn exact_verts(obj: &Obj, r: &Ring) -> Result<Vec<V3>, String> {
    let mut v = match &obj.kind {
        Kind::Polyhedron { base, ops } => poly_exact(base, ops, r)?.0,
        Kind::Free { pts, .. } => free_exact(pts, r)?,
        _ => {
            return Err(format!(
                "{} hat keine exakten Koordinaten",
                obj.kind.type_name()
            ))
        }
    };
    let mut off = vec![];
    for c in &obj.offset {
        let e = expr::parse(c).map_err(|e| format!("Verschiebung „{c}“: {e}"))?;
        off.push(ex_exact(r, &e, c).map_err(|e| format!("Verschiebung: {e}"))?);
    }
    let off: V3 = [off[0].clone(), off[1].clone(), off[2].clone()];
    if !off.iter().all(|x| r.sis_zero(x)) {
        v = v.iter().map(|p| poly::v3_add(r, p, &off)).collect();
    }
    Ok(v)
}

fn build_polyhedron(
    obj: &Obj,
    base: &PolyBase,
    ops: &[PolyOp],
    ctx: &Ctx,
    cache: &mut Cache,
    b: &mut Built,
) -> Result<(), String> {
    let keys = poly_keys(base, ops, ctx)?;
    let r = ctx.ring(&keys)?;
    let (ex, num, hull) = poly_exact(base, ops, &r)?;
    let key = hash_of(&(
        serde_json::to_string(&obj.kind).unwrap_or_default(),
        keys.clone(),
        ctx.params(),
        &hull.faces,
    ));
    let mut an = match cache.get(&key) {
        Some(a) => a.clone(),
        None => {
            let a = poly::analyze_polyhedron(&r, &ex, &num, &hull);
            cache.insert(key, a.clone());
            a
        }
    };
    refresh(&r, &mut an);
    emit_polyhedron(obj, &r, &num, &hull, &an, b);
    b.def_tex.push(def_tex_poly(base, ops));
    Ok(())
}

fn refresh(r: &Ring, an: &mut Analysis) {
    for l in &mut an.lines {
        if let Some(v) = &l.val {
            l.num = Some(v.eval(r));
        }
    }
    for c in &mut an.classes {
        c.value = r.seval(&c.d2).max(0.0).sqrt();
    }
}

fn def_tex_poly(base: &PolyBase, ops: &[PolyOp]) -> String {
    let t = |s: &str| {
        expr::parse(s)
            .map(|e| expr::to_tex(&e))
            .unwrap_or_else(|_| s.to_string())
    };
    let mut s = match base {
        PolyBase::Platonic { kind, a } => format!("\\text{{{}}}, \\ \\ell = {}", kind.name(), t(a)),
        PolyBase::Cuboid { a, b, c } => format!(
            "\\text{{Quader}}\\ {} \\times {} \\times {}",
            t(a),
            t(b),
            t(c)
        ),
        PolyBase::Prism { n, r, h } => format!(
            "\\text{{{n}-seitiges Prisma}},\\ r = {},\\ h = {}",
            t(r),
            t(h)
        ),
        PolyBase::Antiprism { n, r, h } => format!(
            "\\text{{{n}-seitiges Antiprisma}},\\ r = {},\\ h = {}",
            t(r),
            t(h)
        ),
        PolyBase::Pyramid { n, r, h } => format!(
            "\\text{{{n}-seitige Pyramide}},\\ r = {},\\ h = {}",
            t(r),
            t(h)
        ),
        PolyBase::Bipyramid { n, r, h } => format!(
            "\\text{{{n}-seitige Bipyramide}},\\ r = {},\\ h = {}",
            t(r),
            t(h)
        ),
    };
    for op in ops {
        s.push_str(&match op {
            PolyOp::Truncate(x) => format!(",\\ \\text{{abgestumpft mit }} t = {}", t(x)),
            PolyOp::Dual => ",\\ \\text{dualisiert (Flächenmitten)}".to_string(),
            PolyOp::Scale(x) => format!(",\\ \\text{{skaliert mit }} {}", t(x)),
        });
    }
    s
}

fn emit_classes_legend(r: &Ring, an: &Analysis, b: &mut Built) {
    b.formulas = an.lines.clone();
    b.class_colors = (0..an.classes.len()).map(class_color).collect();
    b.legend = r.gen_legend(false);
    b.legend_tex = r.gen_legend(true);
    b.warnings.extend(an.warnings.iter().cloned());
}

fn emit_polyhedron(
    obj: &Obj,
    r: &Ring,
    num: &[DVec3],
    hull: &poly::Hull,
    an: &Analysis,
    b: &mut Built,
) {
    let l = &obj.layers;
    emit_classes_legend(r, an, b);
    b.convex = true;
    let center = num.iter().sum::<DVec3>() / num.len() as f64;
    let radius = num
        .iter()
        .map(|p| (*p - center).length())
        .fold(0.0, f64::max);
    b.center = center;
    b.radius = radius;
    let base = Color32::from_rgb(obj.color[0], obj.color[1], obj.color[2]);
    if l.faces {
        for (fi, f) in hull.faces.iter().enumerate() {
            b.faces.push(RFace {
                pts: f.iter().map(|i| num[*i]).collect(),
                normal: hull.normals[fi],
            });
        }
    }
    // Abschnittsnetz für Schnitte
    let mut sm = Mesh {
        v: num.to_vec(),
        t: vec![],
    };
    for f in &hull.faces {
        for t in 1..f.len() - 1 {
            sm.t.push([f[0] as u32, f[t] as u32, f[t + 1] as u32]);
        }
    }
    b.section_mesh = Some(sm);
    b.verts = num.to_vec();
    b.hull_faces = hull.faces.clone();
    b.hull_planes = hull
        .normals
        .iter()
        .cloned()
        .zip(hull.offsets.iter().cloned())
        .collect();
    b.exact_ok = !an.classes.is_empty();
    // Flächen-Adjazenz der Kanten
    let mut edge_faces: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
    let mut vert_faces: HashMap<usize, Vec<usize>> = HashMap::new();
    for (fi, f) in hull.faces.iter().enumerate() {
        for i in 0..f.len() {
            let (p, q) = (f[i], f[(i + 1) % f.len()]);
            edge_faces.entry((p.min(q), p.max(q))).or_default().push(fi);
            vert_faces.entry(f[i]).or_default().push(fi);
        }
    }
    let n_edge_classes = an
        .classes
        .iter()
        .filter(|c| c.kind == poly::PairKind::Edge)
        .count();
    if n_edge_classes == 1 {
        if let Some(ci) = an
            .classes
            .iter()
            .position(|c| c.kind == poly::PairKind::Edge)
        {
            b.class_colors[ci] = brighten(base);
        }
    }
    for (ci, c) in an.classes.iter().enumerate() {
        if obj.hidden_classes.contains(&ci) {
            continue;
        }
        let total = an.classes.iter().filter(|d| d.kind == c.kind).count();
        let (slabel, stex) = strut_label(c, total);
        let show = match c.kind {
            poly::PairKind::Edge => l.edges,
            poly::PairKind::FaceDiag => l.face_diag,
            poly::PairKind::SpaceDiag => l.space_diag,
            _ => true,
        };
        if !show {
            continue;
        }
        for (p, q) in &c.pairs {
            b.struts.push(StrutRef {
                i: *p,
                j: *q,
                label: slabel.clone(),
                tex: stex.clone(),
            });
            let sref = Some(b.struts.len() - 1);
            let inter = Some(slabel.clone());
            if c.kind == poly::PairKind::Edge {
                let col = if n_edge_classes > 1 {
                    b.class_colors[ci]
                } else {
                    brighten(base)
                };
                b.lines.push(RLine {
                    a: num[*p],
                    b: num[*q],
                    color: col,
                    width: 2.2,
                    faces: edge_faces.get(&(*p, *q)).cloned().unwrap_or_default(),
                    strut: false,
                    inter,
                    sref,
                });
            } else {
                b.lines.push(RLine {
                    a: num[*p],
                    b: num[*q],
                    color: b.class_colors[ci],
                    width: 1.6,
                    faces: vec![],
                    strut: true,
                    inter,
                    sref,
                });
            }
        }
    }
    if l.vertices || l.labels {
        for (i, p) in num.iter().enumerate() {
            b.points.push(RPoint {
                p: *p,
                color: brighten(base),
                r: if l.vertices { 3.5 } else { 0.0 },
                label: if l.labels { Some(format!("{i}")) } else { None },
                faces: vert_faces.get(&i).cloned().unwrap_or_default(),
                pick: Some(i),
            });
        }
    }
    let face_centers: Vec<DVec3> = hull
        .faces
        .iter()
        .map(|f| f.iter().map(|i| num[*i]).sum::<DVec3>() / f.len() as f64)
        .collect();
    if l.dual {
        let col = Color32::from_rgb(255, 230, 120);
        for ((_, _), fs) in edge_faces.iter() {
            if fs.len() == 2 {
                b.lines.push(RLine {
                    a: face_centers[fs[0]],
                    b: face_centers[fs[1]],
                    color: col,
                    width: 1.6,
                    faces: vec![],
                    strut: true,
                    inter: Some("Dualkante".into()),
                    sref: None,
                });
            }
        }
        for c in &face_centers {
            b.points.push(RPoint {
                p: *c,
                color: col,
                r: 2.5,
                label: None,
                faces: vec![],
                pick: None,
            });
        }
    }
    if l.spokes || l.cone_tet {
        let col = Color32::from_rgb(200, 200, 210);
        for p in num {
            b.lines.push(RLine {
                a: center,
                b: *p,
                color: col,
                width: 1.0,
                faces: vec![],
                strut: true,
                inter: Some("Speiche".into()),
                sref: None,
            });
        }
        b.points.push(RPoint {
            p: center,
            color: Color32::WHITE,
            r: 3.0,
            label: None,
            faces: vec![],
            pick: None,
        });
    }
    if l.cone_tet {
        let col = Color32::from_rgb(170, 170, 185);
        let mut tets = 0;
        for f in &hull.faces {
            tets += f.len() - 2;
            for t in 2..f.len() - 1 {
                b.lines.push(RLine {
                    a: num[f[0]],
                    b: num[f[t]],
                    color: col,
                    width: 1.0,
                    faces: vec![],
                    strut: true,
                    inter: Some("Tetraederkante".into()),
                    sref: None,
                });
            }
        }
        b.formulas.push(FLine {
            label: "Kegel-Tetraedrisierung".into(),
            uni: format!(
                "{tets} Tetraeder (Spitze im Eckenschwerpunkt, Σ(nᶠ − 2) über alle Flächen)"
            ),
            ..Default::default()
        });
    }
    emit_symmetry(l, num, center, radius, b);
    if l.spheres {
        let ds: Vec<f64> = num.iter().map(|p| (*p - center).length()).collect();
        if ds.iter().all(|d| (d - ds[0]).abs() < 1e-7 * radius) {
            sphere_rings(b, center, ds[0], Color32::from_rgb(160, 200, 255));
        }
        let din: Vec<f64> = hull
            .normals
            .iter()
            .zip(&hull.offsets)
            .map(|(n, d)| d - n.dot(center))
            .collect();
        if din.iter().all(|d| (d - din[0]).abs() < 1e-7 * radius) {
            sphere_rings(b, center, din[0], Color32::from_rgb(255, 200, 160));
        }
    }
    let region = Region::Convex(
        hull.normals
            .iter()
            .cloned()
            .zip(hull.offsets.iter().cloned())
            .collect(),
    );
    emit_infill(
        l,
        &region,
        center - DVec3::splat(radius),
        center + DVec3::splat(radius),
        base,
        b,
    );
}

/// Kurzbezeichnung einer Abstandsklasse für Streben-Interaktionen.
fn strut_label(c: &poly::DistClass, total: usize) -> (String, String) {
    if total > 1 {
        (
            format!("{} {}", c.kind.name(), c.sub),
            format!("{}_{{{}}}", c.kind.tex(), c.sub),
        )
    } else {
        (c.kind.name().to_string(), c.kind.tex().to_string())
    }
}

fn brighten(c: Color32) -> Color32 {
    let f = |x: u8| (x as f32 * 0.6 + 255.0 * 0.4) as u8;
    Color32::from_rgb(f(c.r()), f(c.g()), f(c.b()))
}

fn emit_symmetry(l: &Layers, pts: &[DVec3], center: DVec3, radius: f64, b: &mut Built) {
    let s = poly::symmetry(pts, center);
    if l.axes {
        for (d, o) in &s.axes {
            let col = axis_color(*o);
            let e = *d * radius * 1.35;
            b.lines.push(RLine {
                a: center - e,
                b: center + e,
                color: col,
                width: 2.0,
                faces: vec![],
                strut: true,
                inter: None,
                sref: None,
            });
            b.labels.push((center + e * 1.04, format!("{o}"), col));
        }
    }
    if l.mirrors {
        for m in &s.mirrors {
            ring(
                b,
                center,
                *m,
                radius * 1.2,
                Color32::from_rgba_unmultiplied(200, 200, 255, 140),
                1.0,
            );
        }
    }
    if s.order > 0 {
        let mut axes: HashMap<u32, usize> = HashMap::new();
        for (_, o) in &s.axes {
            *axes.entry(*o).or_default() += 1;
        }
        let mut ax: Vec<(u32, usize)> = axes.into_iter().collect();
        ax.sort_by(|a, b| b.0.cmp(&a.0));
        let axs: Vec<String> = ax.iter().map(|(o, c)| format!("{c}× {o}-zählig")).collect();
        b.formulas.push(FLine {
            label: "Symmetriegruppe".into(),
            uni: format!(
                "{}, Ordnung {} ({} Drehungen{}){}",
                s.name_uni,
                s.order,
                s.rot_order,
                if s.note.is_empty() {
                    String::new()
                } else {
                    format!("; {}", s.note)
                },
                if s.mirrors.is_empty() {
                    String::new()
                } else {
                    format!(", {} Spiegelebenen", s.mirrors.len())
                }
            ),
            tex: format!("G &= {} ,\\quad \\lvert G\\rvert = {}", s.name_tex, s.order),
            ..Default::default()
        });
        if !axs.is_empty() {
            b.formulas.push(FLine {
                label: "Drehachsen".into(),
                uni: axs.join(", "),
                ..Default::default()
            });
        }
    }
    b.sym = Some(s);
}

fn ring(b: &mut Built, c: DVec3, n: DVec3, rad: f64, col: Color32, w: f32) {
    let u = n.any_orthonormal_vector();
    let v = n.cross(u);
    let k = 64;
    for i in 0..k {
        let a0 = i as f64 / k as f64 * std::f64::consts::TAU;
        let a1 = (i + 1) as f64 / k as f64 * std::f64::consts::TAU;
        b.lines.push(RLine {
            a: c + (u * a0.cos() + v * a0.sin()) * rad,
            b: c + (u * a1.cos() + v * a1.sin()) * rad,
            color: col,
            width: w,
            faces: vec![],
            strut: false,
            inter: None,
            sref: None,
        });
    }
}

fn sphere_rings(b: &mut Built, c: DVec3, rad: f64, col: Color32) {
    for n in [DVec3::X, DVec3::Y, DVec3::Z] {
        ring(b, c, n, rad, col, 1.2);
    }
}

fn emit_infill(l: &Layers, region: &Region, lo: DVec3, hi: DVec3, base: Color32, b: &mut Built) {
    if l.lattice {
        let segs = mesh::lattice(region, lo, hi, l.lattice_spacing, l.lattice_kind);
        let col = Color32::from_rgb(150, 230, 210);
        b.formulas.push(FLine {
            label: "Gitter-Füllung".into(),
            uni: format!("{}: {} Streben", l.lattice_kind.name(), segs.len()),
            ..Default::default()
        });
        for (a, c) in segs {
            b.lines.push(RLine {
                a,
                b: c,
                color: col,
                width: 1.0,
                faces: vec![],
                strut: true,
                inter: Some("Gitterstrebe".into()),
                sref: None,
            });
        }
    }
    if l.tpms {
        let m = mesh::tpms_infill(region, lo, hi, l.tpms_period, l.tpms_kind);
        let col = with_alpha(
            brighten(Color32::from_rgb(255 - base.r() / 2, 200, 120)),
            0.85,
        );
        b.formulas.push(FLine {
            label: "TPMS-Füllung".into(),
            uni: format!(
                "{}: {} Dreiecke, Fläche ≈ {:.4}",
                l.tpms_kind.name(),
                m.t.len(),
                m.area()
            ),
            ..Default::default()
        });
        let normals = m.vertex_normals();
        b.meshes.push(RMesh {
            mesh: m,
            normals,
            color: col,
        });
    }
}

// ---------------------------------------------------------------------------
// Freie Konstruktion
// ---------------------------------------------------------------------------

fn build_free(
    obj: &Obj,
    pts: &[[String; 3]],
    struts: &[(usize, usize)],
    hull: bool,
    ctx: &Ctx,
    cache: &mut Cache,
    b: &mut Built,
) -> Result<(), String> {
    if pts.is_empty() {
        return Err("noch keine Punkte – unten im Inspektor hinzufügen".into());
    }
    let strs: Vec<&String> = pts.iter().flatten().collect();
    let exs = parse_all(&strs)?;
    let mut keys = BTreeSet::new();
    for e in &exs {
        expr::collect_gens(e, &ctx.env, &ctx.params(), &ctx.custom, &mut keys)?;
    }
    let r = ctx.ring(&keys)?;
    let mut ex: Vec<V3> = vec![];
    let mut exact_ok = true;
    let mut num: Vec<DVec3> = vec![];
    for (i, p) in pts.iter().enumerate() {
        let mut v = vec![];
        let mut nv = DVec3::ZERO;
        for k in 0..3 {
            let e = &exs[3 * i + k];
            match expr::exact(e, &r) {
                Ok(s) => {
                    nv[k] = r.seval(&s);
                    v.push(s);
                }
                Err(err) => {
                    if exact_ok {
                        b.warnings.push(format!(
                            "P{i}: „{}“ nicht exakt ({err}) – nur numerisch",
                            p[k]
                        ));
                    }
                    exact_ok = false;
                    nv[k] = expr::eval_const(e, &ctx.env)?;
                    v.push(r.sym_c(qi(0)));
                }
            }
        }
        ex.push([v[0].clone(), v[1].clone(), v[2].clone()]);
        num.push(nv);
    }
    let base = Color32::from_rgb(obj.color[0], obj.color[1], obj.color[2]);
    let l = &obj.layers;
    let center = num.iter().sum::<DVec3>() / num.len() as f64;
    let radius = num
        .iter()
        .map(|p| (*p - center).length())
        .fold(0.0, f64::max)
        .max(0.5);
    let valid_struts: Vec<(usize, usize)> = struts
        .iter()
        .cloned()
        .filter(|(a, c)| *a < num.len() && *c < num.len() && a != c)
        .collect();
    if hull && num.len() >= 4 {
        let keep = poly::dedupe(&num);
        if keep.len() != num.len() {
            b.warnings.push("doppelte Punkte zusammengefasst".into());
        }
        let ex2: Vec<V3> = keep.iter().map(|i| ex[*i].clone()).collect();
        let num2: Vec<DVec3> = keep.iter().map(|i| num[*i]).collect();
        let h = poly::convex_hull(&num2)?;
        if exact_ok {
            let key = hash_of(&(
                serde_json::to_string(&obj.kind).unwrap_or_default(),
                keys.clone(),
                ctx.params(),
                &h.faces,
            ));
            let mut an = match cache.get(&key) {
                Some(a) => a.clone(),
                None => {
                    let a = poly::analyze_polyhedron(&r, &ex2, &num2, &h);
                    cache.insert(key, a.clone());
                    a
                }
            };
            refresh(&r, &mut an);
            emit_polyhedron(obj, &r, &num2, &h, &an, b);
        } else {
            let an = Analysis {
                classes: vec![],
                edges: vec![],
                lines: vec![],
                warnings: vec![],
            };
            emit_polyhedron(obj, &r, &num2, &h, &an, b);
            for f in &h.faces {
                for i in 0..f.len() {
                    b.lines.push(RLine {
                        a: num2[f[i]],
                        b: num2[f[(i + 1) % f.len()]],
                        color: brighten(base),
                        width: 2.0,
                        faces: vec![],
                        strut: false,
                        inter: None,
                        sref: None,
                    });
                }
            }
        }
        // Indizes der Hülle (num2) auf die Originalpunkte zurückführen
        for st in &mut b.struts {
            st.i = keep[st.i];
            st.j = keep[st.j];
        }
        b.hull_faces = b
            .hull_faces
            .iter()
            .map(|f| f.iter().map(|i| keep[*i]).collect())
            .collect();
        b.verts = num.clone();
        b.exact_ok = exact_ok;
        for (p, q) in &valid_struts {
            b.struts.push(StrutRef {
                i: *p,
                j: *q,
                label: format!("Strebe P{p}–P{q}"),
                tex: format!("P_{{{p}}}P_{{{q}}}"),
            });
            let sref = if exact_ok {
                Some(b.struts.len() - 1)
            } else {
                None
            };
            b.lines.push(RLine {
                a: num[*p],
                b: num[*q],
                color: Color32::from_rgb(255, 120, 120),
                width: 2.4,
                faces: vec![],
                strut: true,
                inter: Some(format!("Strebe P{p}–P{q}")),
                sref,
            });
        }
        // Punkt-Picking bezieht sich auf die Originalindizes
        b.points.retain(|p| p.pick.is_none());
        for (i, p) in num.iter().enumerate() {
            b.points.push(RPoint {
                p: *p,
                color: brighten(base),
                r: 4.0,
                label: Some(format!("P{i}")),
                faces: vec![],
                pick: Some(i),
            });
        }
        b.convex = true;
        return Ok(());
    }
    // Gerüst
    b.center = center;
    b.radius = radius;
    b.verts = num.clone();
    b.exact_ok = exact_ok;
    if exact_ok {
        let key = hash_of(&(
            serde_json::to_string(&obj.kind).unwrap_or_default(),
            keys.clone(),
            ctx.params(),
            0u8,
        ));
        let mut an = match cache.get(&key) {
            Some(a) => a.clone(),
            None => {
                let a = poly::analyze_framework(&r, &ex, &valid_struts);
                cache.insert(key, a.clone());
                a
            }
        };
        refresh(&r, &mut an);
        emit_classes_legend(&r, &an, b);
        for (ci, c) in an.classes.iter().enumerate() {
            if obj.hidden_classes.contains(&ci) {
                continue;
            }
            if c.kind == poly::PairKind::Pair && !l.all_pairs {
                continue;
            }
            let total = an.classes.iter().filter(|d| d.kind == c.kind).count();
            let (slabel, stex) = strut_label(c, total);
            for (p, q) in &c.pairs {
                let w = if c.kind == poly::PairKind::Strut {
                    2.4
                } else {
                    1.0
                };
                b.struts.push(StrutRef {
                    i: *p,
                    j: *q,
                    label: slabel.clone(),
                    tex: stex.clone(),
                });
                b.lines.push(RLine {
                    a: num[*p],
                    b: num[*q],
                    color: b.class_colors[ci],
                    width: w,
                    faces: vec![],
                    strut: true,
                    inter: Some(slabel.clone()),
                    sref: Some(b.struts.len() - 1),
                });
            }
        }
    } else {
        for (p, q) in &valid_struts {
            b.lines.push(RLine {
                a: num[*p],
                b: num[*q],
                color: brighten(base),
                width: 2.4,
                faces: vec![],
                strut: true,
                inter: Some(format!("Strebe P{p}–P{q}")),
                sref: None,
            });
        }
    }
    for (i, p) in num.iter().enumerate() {
        b.points.push(RPoint {
            p: *p,
            color: brighten(base),
            r: 4.0,
            label: Some(format!("P{i}")),
            faces: vec![],
            pick: Some(i),
        });
    }
    if num.len() >= 3 {
        emit_symmetry(l, &num, center, radius, b);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Flächen und Kurven
// ---------------------------------------------------------------------------

fn compile(src: &str, vars: &[&str], ctx: &Ctx) -> Result<expr::NFn, String> {
    let e = expr::parse(src).map_err(|e| format!("„{src}“: {e}"))?;
    expr::compile(&e, vars, &ctx.env).map_err(|e| format!("„{src}“: {e}"))
}

fn tex_of(src: &str) -> String {
    expr::parse(src)
        .map(|e| expr::to_tex(&e))
        .unwrap_or_else(|_| src.into())
}

fn topo_lines(m: &Mesh, b: &mut Built) -> mesh::Topology {
    let t = mesh::topology(m);
    b.formulas.push(FLine {
        label: "Netz".into(),
        uni: format!("V = {}, E = {}, F = {}", t.v, t.e, t.f),
        tex: String::new(),
        ..Default::default()
    });
    b.formulas.push(FLine {
        label: "Topologie (aus dem Netz)".into(),
        uni: mesh::classify(&t),
        tex: format!("\\chi &= {}", t.chi),
        ..Default::default()
    });
    t
}

fn emit_mesh_common(obj: &Obj, m: Mesh, b: &mut Built) {
    let l = &obj.layers;
    let (lo, hi) = m.bbox();
    b.center = (lo + hi) * 0.5;
    b.radius = ((hi - lo).length() * 0.5).max(1e-6);
    if l.contours && !m.t.is_empty() {
        let n = l.contour_n.clamp(1, 80);
        for k in 1..=n {
            let z = lo.z + (hi.z - lo.z) * k as f64 / (n + 1) as f64;
            let (segs, _) = mesh::slice(&m, DVec3::Z, z);
            let col = hsv(0.55 + 0.35 * k as f32 / n as f32, 0.5, 1.0);
            for (a, c) in segs {
                b.lines.push(RLine {
                    a,
                    b: c,
                    color: col,
                    width: 1.3,
                    faces: vec![],
                    strut: true,
                    inter: None,
                    sref: None,
                });
            }
        }
    }
    if l.normals {
        let ns = m.vertex_normals();
        let step = (m.v.len() / 400).max(1);
        for i in (0..m.v.len()).step_by(step) {
            b.lines.push(RLine {
                a: m.v[i],
                b: m.v[i] + ns[i] * l.normal_len,
                color: Color32::from_rgb(255, 220, 120),
                width: 1.0,
                faces: vec![],
                strut: true,
                inter: None,
                sref: None,
            });
        }
    }
    let base = Color32::from_rgb(obj.color[0], obj.color[1], obj.color[2]);
    let normals = m.vertex_normals();
    b.section_mesh = Some(m.clone());
    if l.faces {
        b.meshes.push(RMesh {
            mesh: m,
            normals,
            color: with_alpha(base, l.alpha.max(0.05) + 0.25),
        });
    }
}

fn build_param(
    obj: &Obj,
    xyz: [&String; 3],
    range: [&String; 4],
    nu: u32,
    nv: u32,
    ctx: &Ctx,
    b: &mut Built,
) -> Result<(), String> {
    let fx = compile(xyz[0], &["u", "v"], ctx)?;
    let fy = compile(xyz[1], &["u", "v"], ctx)?;
    let fz = compile(xyz[2], &["u", "v"], ctx)?;
    let rg: Vec<f64> = range.iter().map(|s| ctx.num(s)).collect::<Result<_, _>>()?;
    let f = |u: f64, v: f64| DVec3::new(fx(&[u, v]), fy(&[u, v]), fz(&[u, v]));
    let (m, grid) = mesh::param_surface(&f, rg[0], rg[1], rg[2], rg[3], nu as usize, nv as usize);
    b.def_tex.push(format!(
        "\\vec r(u,v) = \\begin{{pmatrix}} {} \\\\ {} \\\\ {} \\end{{pmatrix}},\\quad u \\in [{}, {}],\\ v \\in [{}, {}]",
        tex_of(xyz[0]),
        tex_of(xyz[1]),
        tex_of(xyz[2]),
        tex_of(range[0]),
        tex_of(range[1]),
        tex_of(range[2]),
        tex_of(range[3])
    ));
    b.formulas.push(FLine {
        label: "Definition".into(),
        uni: format!("r(u,v) = ({}, {}, {})", xyz[0], xyz[1], xyz[2]),
        tex: format!(
            "\\vec r(u,v) &= \\left({}, {}, {}\\right)",
            tex_of(xyz[0]),
            tex_of(xyz[1]),
            tex_of(xyz[2])
        ),
        ..Default::default()
    });
    let t = topo_lines(&m, b);
    let area = m.area();
    b.formulas.push(FLine {
        label: "Flächeninhalt".into(),
        uni: "∬ |r_u × r_v| du dv".into(),
        tex: format!("A &\\approx {area:.6}"),
        num: Some(area),
        ..Default::default()
    });
    if t.boundary_loops == 0 && t.orientable && t.nonmanifold_edges == 0 {
        let v = m.volume().abs();
        b.formulas.push(FLine {
            label: "Eingeschlossenes Volumen".into(),
            uni: "⅓∬ r·(r_u × r_v) du dv".into(),
            tex: format!("V &\\approx {v:.6}"),
            num: Some(v),
            ..Default::default()
        });
    }
    let l = &obj.layers;
    let iso_col = brighten(Color32::from_rgb(obj.color[0], obj.color[1], obj.color[2]));
    if l.iso {
        let (gu, gv) = (grid.len() - 1, grid[0].len() - 1);
        let su = (gu / l.iso_u.max(1) as usize).max(1);
        let sv = (gv / l.iso_v.max(1) as usize).max(1);
        for i in (0..=gu).step_by(su) {
            for j in 0..gv {
                b.lines.push(RLine {
                    a: grid[i][j],
                    b: grid[i][j + 1],
                    color: iso_col,
                    width: 1.0,
                    faces: vec![],
                    strut: false,
                    inter: None,
                    sref: None,
                });
            }
        }
        for j in (0..=gv).step_by(sv) {
            for i in 0..gu {
                b.lines.push(RLine {
                    a: grid[i][j],
                    b: grid[i + 1][j],
                    color: iso_col,
                    width: 1.0,
                    faces: vec![],
                    strut: false,
                    inter: None,
                    sref: None,
                });
            }
        }
    }
    if l.chords {
        let n = l.chord_n.clamp(1, 200) as usize;
        let (du, dv) = (l.chord_du * (rg[1] - rg[0]), l.chord_dv * (rg[3] - rg[2]));
        let col = Color32::from_rgb(255, 140, 180);
        let rows = (n as f64).sqrt().ceil() as usize;
        for i in 0..n {
            let s = (i / rows) as f64 / rows as f64;
            let tt = (i % rows) as f64 / rows as f64 + 0.5 / rows as f64;
            let u = rg[0] + (rg[1] - rg[0]) * (i as f64 / n as f64);
            let v = rg[2]
                + (rg[3] - rg[2])
                    * if du.abs() > 0.0 && dv == 0.0 {
                        0.5 + 0.35 * (s - 0.5)
                    } else {
                        tt
                    };
            let a = f(u, v);
            let c = f(u + du, v + dv);
            if a.is_finite() && c.is_finite() {
                b.lines.push(RLine {
                    a,
                    b: c,
                    color: col,
                    width: 1.4,
                    faces: vec![],
                    strut: true,
                    inter: Some("Sehne".into()),
                    sref: None,
                });
            }
        }
    }
    emit_mesh_common(obj, m, b);
    Ok(())
}

fn build_implicit(
    obj: &Obj,
    f: &str,
    lo: &[String; 3],
    hi: &[String; 3],
    res: u32,
    ctx: &Ctx,
    b: &mut Built,
) -> Result<(), String> {
    let fun = compile(f, &["x", "y", "z"], ctx)?;
    let mut l3 = DVec3::ZERO;
    let mut h3 = DVec3::ZERO;
    for k in 0..3 {
        l3[k] = ctx.num(&lo[k])?;
        h3[k] = ctx.num(&hi[k])?;
    }
    if (h3 - l3).min_element() <= 0.0 {
        return Err("Grenzen: max muss größer als min sein".into());
    }
    let ff = |p: DVec3| fun(&[p.x, p.y, p.z]);
    let m = mesh::marching_tets(&ff, l3, h3, res as usize, None);
    b.def_tex.push(format!("{} = 0", tex_of(f)));
    b.formulas.push(FLine {
        label: "Definition".into(),
        uni: format!("{f} = 0"),
        tex: format!("F(x,y,z) &= {} = 0", tex_of(f)),
        ..Default::default()
    });
    let t = topo_lines(&m, b);
    let area = m.area();
    b.formulas.push(FLine {
        label: "Flächeninhalt".into(),
        uni: "Summe der Dreiecke".into(),
        tex: format!("A &\\approx {area:.6}"),
        num: Some(area),
        ..Default::default()
    });
    if t.boundary_loops == 0 && t.nonmanifold_edges == 0 {
        let v = m.volume().abs();
        b.formulas.push(FLine {
            label: "Volumen {F < 0}".into(),
            uni: "Divergenzsatz".into(),
            tex: format!("V &\\approx {v:.6}"),
            num: Some(v),
            ..Default::default()
        });
    } else if t.boundary_loops > 0 {
        b.warnings.push("Fläche wird vom Rechenquader abgeschnitten – Grenzen vergrößern für geschlossene Topologie".into());
    }
    let region = Region::Implicit(&fun);
    let (mlo, mhi) = if m.v.is_empty() { (l3, h3) } else { m.bbox() };
    emit_infill(
        &obj.layers,
        &region,
        mlo,
        mhi,
        Color32::from_rgb(obj.color[0], obj.color[1], obj.color[2]),
        b,
    );
    emit_mesh_common(obj, m, b);
    Ok(())
}

fn build_curve(
    obj: &Obj,
    xyz: [&String; 3],
    t0: &str,
    t1: &str,
    n: u32,
    ctx: &Ctx,
    b: &mut Built,
) -> Result<(), String> {
    let fx = compile(xyz[0], &["t"], ctx)?;
    let fy = compile(xyz[1], &["t"], ctx)?;
    let fz = compile(xyz[2], &["t"], ctx)?;
    let a = ctx.num(t0)?;
    let z = ctx.num(t1)?;
    let r = |t: f64| DVec3::new(fx(&[t]), fy(&[t]), fz(&[t]));
    let n = n.clamp(8, 20000) as usize;
    let pts: Vec<DVec3> = (0..=n)
        .map(|i| r(a + (z - a) * i as f64 / n as f64))
        .collect();
    let (lo, hi) = pts.iter().fold(
        (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN)),
        |(l, h), p| (l.min(*p), h.max(*p)),
    );
    b.center = (lo + hi) * 0.5;
    b.radius = ((hi - lo).length() * 0.5).max(1e-6);
    let base = Color32::from_rgb(obj.color[0], obj.color[1], obj.color[2]);
    for w in pts.windows(2) {
        b.lines.push(RLine {
            a: w[0],
            b: w[1],
            color: brighten(base),
            width: 3.0,
            faces: vec![],
            strut: false,
            inter: Some("Kurve".into()),
            sref: None,
        });
    }
    // Ableitungen (zentrale Differenzen)
    let hstep = (z - a) * 1e-4;
    let d1 = |t: f64| (r(t + hstep) - r(t - hstep)) / (2.0 * hstep);
    let d2 = |t: f64| (r(t + hstep) - 2.0 * r(t) + r(t - hstep)) / (hstep * hstep);
    let d3 = |t: f64| {
        (r(t + 2.0 * hstep) - 2.0 * r(t + hstep) + 2.0 * r(t - hstep) - r(t - 2.0 * hstep))
            / (2.0 * hstep.powi(3))
    };
    let mut len = 0.0;
    let mut tk = 0.0;
    let mut tt = 0.0;
    let m = 4 * n;
    for i in 0..m {
        let t = a + (z - a) * (i as f64 + 0.5) / m as f64;
        let (v1, v2, v3) = (d1(t), d2(t), d3(t));
        let sp = v1.length();
        let cr = v1.cross(v2);
        let ds = sp * (z - a) / m as f64;
        len += ds;
        if sp > 1e-12 {
            tk += cr.length() / sp.powi(3) * ds;
            let c2 = cr.length_squared();
            if c2 > 1e-18 {
                tt += cr.dot(v3) / c2 * ds;
            }
        }
    }
    let closed = (pts[0] - pts[n]).length() < 1e-6 * b.radius;
    b.def_tex.push(format!(
        "\\vec r(t) = \\begin{{pmatrix}} {} \\\\ {} \\\\ {} \\end{{pmatrix}},\\quad t \\in [{}, {}]",
        tex_of(xyz[0]),
        tex_of(xyz[1]),
        tex_of(xyz[2]),
        tex_of(t0),
        tex_of(t1)
    ));
    b.formulas.push(FLine {
        label: "Definition".into(),
        uni: format!("r(t) = ({}, {}, {})", xyz[0], xyz[1], xyz[2]),
        tex: format!(
            "\\vec r(t) &= \\left({}, {}, {}\\right)",
            tex_of(xyz[0]),
            tex_of(xyz[1]),
            tex_of(xyz[2])
        ),
        ..Default::default()
    });
    b.formulas.push(FLine {
        label: "Bogenlänge L".into(),
        uni: "∫ |r'(t)| dt".into(),
        tex: format!("L &\\approx {len:.6}"),
        num: Some(len),
        ..Default::default()
    });
    b.formulas.push(FLine {
        label: "Totalkrümmung".into(),
        uni: "∫ κ ds,  κ = |r' × r''| / |r'|³".into(),
        tex: format!("\\int \\kappa\\, ds &\\approx {tk:.6}"),
        num: Some(tk),
        ..Default::default()
    });
    b.formulas.push(FLine {
        label: "Totaltorsion".into(),
        uni: "∫ τ ds,  τ = (r' × r'')·r''' / |r' × r''|²".into(),
        tex: format!("\\int \\tau\\, ds &\\approx {tt:.6}"),
        num: Some(tt),
        ..Default::default()
    });
    if closed {
        let ratio = tk / std::f64::consts::PI;
        // Fáry–Milnor: verknotet ⇒ ∫κ ds > 4π. Nur die Kontraposition liefert eine Aussage.
        let note = if tk < 4.0 * std::f64::consts::PI - 1e-3 {
            "< 4π ⇒ unverknotet (Kontraposition von Fáry–Milnor; Wert numerisch)"
        } else {
            "> 4π: mit Verknotung verträglich, entscheidet aber nichts (Fáry–Milnor: verknotet ⇒ ∫κ ds > 4π, nicht umgekehrt)"
        };
        b.formulas.push(FLine {
            label: "Geschlossene Kurve".into(),
            uni: format!("∫κ ds ≈ {ratio:.4}·π  (Fenchel: ≥ 2π)  {note}"),
            ..Default::default()
        });
    }
    let l = &obj.layers;
    let sc = b.radius * 0.12;
    if l.frenet {
        let k = l.frenet_n.clamp(1, 200) as usize;
        for i in 0..k {
            let t = a + (z - a) * (i as f64 + 0.5) / k as f64;
            let (v1, v2) = (d1(t), d2(t));
            let tv = v1.normalize_or_zero();
            let bv = v1.cross(v2).normalize_or_zero();
            let nv = bv.cross(tv);
            let p = r(t);
            for (d, c) in [
                (tv, Color32::from_rgb(255, 90, 90)),
                (nv, Color32::from_rgb(90, 220, 110)),
                (bv, Color32::from_rgb(90, 150, 255)),
            ] {
                b.lines.push(RLine {
                    a: p,
                    b: p + d * sc,
                    color: c,
                    width: 2.0,
                    faces: vec![],
                    strut: true,
                    inter: None,
                    sref: None,
                });
            }
        }
    }
    if l.osc {
        let k = l.osc_n.clamp(1, 60) as usize;
        for i in 0..k {
            let t = a + (z - a) * (i as f64 + 0.5) / k as f64;
            let (v1, v2) = (d1(t), d2(t));
            let kap = v1.cross(v2).length() / v1.length().powi(3);
            if kap < 1e-9 || 1.0 / kap > 20.0 * b.radius {
                continue;
            }
            let tv = v1.normalize_or_zero();
            let bv = v1.cross(v2).normalize_or_zero();
            let nv = bv.cross(tv);
            let c = r(t) + nv / kap;
            ring(
                b,
                c,
                bv,
                1.0 / kap,
                Color32::from_rgba_unmultiplied(255, 200, 120, 170),
                1.0,
            );
        }
    }
    if l.chords {
        let k = l.chord_n.clamp(1, 400) as usize;
        let dt = l.chord_du * (z - a);
        for i in 0..k {
            let t = a + (z - a) * i as f64 / k as f64;
            b.lines.push(RLine {
                a: r(t),
                b: r(t + dt),
                color: Color32::from_rgb(255, 140, 180),
                width: 1.2,
                faces: vec![],
                strut: true,
                inter: Some("Sehne".into()),
                sref: None,
            });
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Vorlagen
// ---------------------------------------------------------------------------

pub fn default_scene() -> Scene {
    let mut s = Scene::default();
    s.params.push(Param {
        name: "a".into(),
        value: 1.0,
        min: 0.1,
        max: 3.0,
        animate: false,
        speed: 0.3,
        dir: 1.0,
    });
    s.params.push(Param {
        name: "t".into(),
        value: 0.3333,
        min: 0.0,
        max: 0.5,
        animate: false,
        speed: 0.15,
        dir: 1.0,
    });
    s.params.push(Param {
        name: "h".into(),
        value: 1.0,
        min: 0.1,
        max: 3.0,
        animate: false,
        speed: 0.3,
        dir: 1.0,
    });
    let id = s.add(
        "Ikosaeder",
        Kind::Polyhedron {
            base: PolyBase::Platonic {
                kind: Platonic::Icosa,
                a: "a".into(),
            },
            ops: vec![],
        },
    );
    if let Some(o) = s.objects.iter_mut().find(|o| o.id == id) {
        o.layers.space_diag = true;
    }
    s
}

pub struct Template {
    pub group: &'static str,
    pub name: &'static str,
    pub make: fn() -> Kind,
}

fn s(x: &str) -> String {
    x.to_string()
}

pub fn templates() -> Vec<Template> {
    fn pl(k: Platonic) -> Kind {
        Kind::Polyhedron {
            base: PolyBase::Platonic { kind: k, a: s("a") },
            ops: vec![],
        }
    }
    fn tr(k: Platonic, t: &str) -> Kind {
        Kind::Polyhedron {
            base: PolyBase::Platonic { kind: k, a: s("a") },
            ops: vec![PolyOp::Truncate(s(t))],
        }
    }
    vec![
        Template {
            group: "Platonisch",
            name: "Tetraeder",
            make: || pl(Platonic::Tetra),
        },
        Template {
            group: "Platonisch",
            name: "Würfel",
            make: || pl(Platonic::Cube),
        },
        Template {
            group: "Platonisch",
            name: "Oktaeder",
            make: || pl(Platonic::Octa),
        },
        Template {
            group: "Platonisch",
            name: "Dodekaeder",
            make: || pl(Platonic::Dodeca),
        },
        Template {
            group: "Platonisch",
            name: "Ikosaeder",
            make: || pl(Platonic::Icosa),
        },
        Template {
            group: "Archimedisch",
            name: "Kuboktaeder",
            make: || tr(Platonic::Cube, "1/2"),
        },
        Template {
            group: "Archimedisch",
            name: "Ikosidodekaeder",
            make: || tr(Platonic::Dodeca, "1/2"),
        },
        Template {
            group: "Archimedisch",
            name: "Tetraederstumpf",
            make: || tr(Platonic::Tetra, "1/3"),
        },
        Template {
            group: "Archimedisch",
            name: "Würfelstumpf",
            make: || tr(Platonic::Cube, "1/(2 + sqrt2)"),
        },
        Template {
            group: "Archimedisch",
            name: "Oktaederstumpf",
            make: || tr(Platonic::Octa, "1/3"),
        },
        Template {
            group: "Archimedisch",
            name: "Dodekaederstumpf",
            make: || tr(Platonic::Dodeca, "1/(2 + phi)"),
        },
        Template {
            group: "Archimedisch",
            name: "Ikosaederstumpf (Fußball)",
            make: || tr(Platonic::Icosa, "1/3"),
        },
        Template {
            group: "Archimedisch",
            name: "Würfel, Abstumpfung t (Morph)",
            make: || tr(Platonic::Cube, "t"),
        },
        Template {
            group: "Weitere",
            name: "Quader a × b × c",
            make: || Kind::Polyhedron {
                base: PolyBase::Cuboid {
                    a: s("a"),
                    b: s("2a"),
                    c: s("h"),
                },
                ops: vec![],
            },
        },
        Template {
            group: "Weitere",
            name: "Gemischter Quader 1 × ρ × ρ³ (binomisch)",
            make: || Kind::Polyhedron {
                base: PolyBase::Cuboid {
                    a: s("1"),
                    b: s("rho"),
                    c: s("rho^3"),
                },
                ops: vec![],
            },
        },
        Template {
            group: "Weitere",
            name: "Van-der-Laan-Quader 1 × ρ × ρ²",
            make: || Kind::Polyhedron {
                base: PolyBase::Cuboid {
                    a: s("1"),
                    b: s("rho"),
                    c: s("rho^2"),
                },
                ops: vec![],
            },
        },
        Template {
            group: "Weitere",
            name: "Prisma (n = 7)",
            make: || Kind::Polyhedron {
                base: PolyBase::Prism {
                    n: 7,
                    r: s("1"),
                    h: s("h"),
                },
                ops: vec![],
            },
        },
        Template {
            group: "Weitere",
            name: "Antiprisma (n = 5)",
            make: || Kind::Polyhedron {
                base: PolyBase::Antiprism {
                    n: 5,
                    r: s("1"),
                    h: s("h"),
                },
                ops: vec![],
            },
        },
        Template {
            group: "Weitere",
            name: "Pyramide (n = 4)",
            make: || Kind::Polyhedron {
                base: PolyBase::Pyramid {
                    n: 4,
                    r: s("1"),
                    h: s("h"),
                },
                ops: vec![],
            },
        },
        Template {
            group: "Weitere",
            name: "Bipyramide (n = 5)",
            make: || Kind::Polyhedron {
                base: PolyBase::Bipyramid {
                    n: 5,
                    r: s("1"),
                    h: s("h"),
                },
                ops: vec![],
            },
        },
        Template {
            group: "Weitere",
            name: "Rhombendodekaeder (Dual Kuboktaeder)",
            make: || Kind::Polyhedron {
                base: PolyBase::Platonic {
                    kind: Platonic::Cube,
                    a: s("a"),
                },
                ops: vec![PolyOp::Truncate(s("1/2")), PolyOp::Dual],
            },
        },
        Template {
            group: "Frei",
            name: "Leere Konstruktion",
            make: || Kind::Free {
                pts: vec![],
                struts: vec![],
                hull: false,
            },
        },
        Template {
            group: "Frei",
            name: "Oktaeder-Gerüst aus Punkten",
            make: || Kind::Free {
                pts: vec![
                    [s("a"), s("0"), s("0")],
                    [s("-a"), s("0"), s("0")],
                    [s("0"), s("a"), s("0")],
                    [s("0"), s("-a"), s("0")],
                    [s("0"), s("0"), s("a")],
                    [s("0"), s("0"), s("-a")],
                ],
                struts: vec![(0, 1), (2, 3), (4, 5)],
                hull: true,
            },
        },
        Template {
            group: "Parameterfläche",
            name: "Torus",
            make: || Kind::ParamSurface {
                x: s("(1 + 0.4cos v) cos u"),
                y: s("(1 + 0.4cos v) sin u"),
                z: s("0.4 sin v"),
                u0: s("0"),
                u1: s("2pi"),
                v0: s("0"),
                v1: s("2pi"),
                nu: 96,
                nv: 48,
            },
        },
        Template {
            group: "Parameterfläche",
            name: "Kugel",
            make: || Kind::ParamSurface {
                x: s("sin v cos u"),
                y: s("sin v sin u"),
                z: s("cos v"),
                u0: s("0"),
                u1: s("2pi"),
                v0: s("0"),
                v1: s("pi"),
                nu: 64,
                nv: 32,
            },
        },
        Template {
            group: "Parameterfläche",
            name: "Möbiusband",
            make: || Kind::ParamSurface {
                x: s("(1 + v/2 cos(u/2)) cos u"),
                y: s("(1 + v/2 cos(u/2)) sin u"),
                z: s("v/2 sin(u/2)"),
                u0: s("0"),
                u1: s("2pi"),
                v0: s("-1"),
                v1: s("1"),
                nu: 96,
                nv: 12,
            },
        },
        Template {
            group: "Parameterfläche",
            name: "Kleinsche Flasche (Achterform)",
            make: || Kind::ParamSurface {
                x: s("(1.5 + cos(u/2) sin v - sin(u/2) sin(2v)) cos u"),
                y: s("(1.5 + cos(u/2) sin v - sin(u/2) sin(2v)) sin u"),
                z: s("sin(u/2) sin v + cos(u/2) sin(2v)"),
                u0: s("0"),
                u1: s("2pi"),
                v0: s("0"),
                v1: s("2pi"),
                nu: 120,
                nv: 48,
            },
        },
        Template {
            group: "Parameterfläche",
            name: "Enneper-Fläche",
            make: || Kind::ParamSurface {
                x: s("u - u^3/3 + u v^2"),
                y: s("v - v^3/3 + v u^2"),
                z: s("u^2 - v^2"),
                u0: s("-1.5"),
                u1: s("1.5"),
                v0: s("-1.5"),
                v1: s("1.5"),
                nu: 48,
                nv: 48,
            },
        },
        Template {
            group: "Parameterfläche",
            name: "Helikoid",
            make: || Kind::ParamSurface {
                x: s("v cos u"),
                y: s("v sin u"),
                z: s("0.3u"),
                u0: s("-2pi"),
                u1: s("2pi"),
                v0: s("-1"),
                v1: s("1"),
                nu: 96,
                nv: 16,
            },
        },
        Template {
            group: "Implizite Fläche",
            name: "Kugel x² + y² + z² = a²",
            make: || Kind::Implicit {
                f: s("x^2 + y^2 + z^2 - a^2"),
                lo: [s("-1.6a"), s("-1.6a"), s("-1.6a")],
                hi: [s("1.6a"), s("1.6a"), s("1.6a")],
                res: 40,
            },
        },
        Template {
            group: "Implizite Fläche",
            name: "Torus",
            make: || Kind::Implicit {
                f: s("(sqrt(x^2 + y^2) - 1)^2 + z^2 - 0.16"),
                lo: [s("-1.6"), s("-1.6"), s("-0.7")],
                hi: [s("1.6"), s("1.6"), s("0.7")],
                res: 56,
            },
        },
        Template {
            group: "Implizite Fläche",
            name: "Brezel (Geschlecht 2)",
            make: || Kind::Implicit {
                f: s("((x^2 + y^2)^2 - x^2 + y^2)^2 + z^2 - 0.012"),
                lo: [s("-1.3"), s("-0.8"), s("-0.3")],
                hi: [s("1.3"), s("0.8"), s("0.3")],
                res: 90,
            },
        },
        Template {
            group: "Implizite Fläche",
            name: "Goursat-Fläche",
            make: || Kind::Implicit {
                f: s("x^4 + y^4 + z^4 - 1.5(x^2 + y^2 + z^2) + 1"),
                lo: [s("-1.6"), s("-1.6"), s("-1.6")],
                hi: [s("1.6"), s("1.6"), s("1.6")],
                res: 56,
            },
        },
        Template {
            group: "Implizite Fläche",
            name: "Würfelkugel x⁴+y⁴+z⁴ = 1",
            make: || Kind::Implicit {
                f: s("x^4 + y^4 + z^4 - 1"),
                lo: [s("-1.3"), s("-1.3"), s("-1.3")],
                hi: [s("1.3"), s("1.3"), s("1.3")],
                res: 44,
            },
        },
        Template {
            group: "Raumkurve",
            name: "Kleeblattknoten",
            make: || Kind::Curve {
                x: s("sin t + 2 sin(2t)"),
                y: s("cos t - 2 cos(2t)"),
                z: s("-sin(3t)"),
                t0: s("0"),
                t1: s("2pi"),
                n: 600,
            },
        },
        Template {
            group: "Raumkurve",
            name: "Helix",
            make: || Kind::Curve {
                x: s("cos t"),
                y: s("sin t"),
                z: s("0.15t"),
                t0: s("0"),
                t1: s("6pi"),
                n: 600,
            },
        },
        Template {
            group: "Raumkurve",
            name: "Viviani-Kurve",
            make: || Kind::Curve {
                x: s("1 + cos t"),
                y: s("sin t"),
                z: s("2 sin(t/2)"),
                t0: s("0"),
                t1: s("4pi"),
                n: 600,
            },
        },
        Template {
            group: "Raumkurve",
            name: "(2,5)-Torusknoten",
            make: || Kind::Curve {
                x: s("(2 + cos(5t)) cos(2t)"),
                y: s("(2 + cos(5t)) sin(2t)"),
                z: s("-sin(5t)"),
                t0: s("0"),
                t1: s("2pi"),
                n: 900,
            },
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line<'a>(b: &'a Built, label: &str) -> &'a FLine {
        b.formulas
            .iter()
            .find(|l| l.label.starts_with(label))
            .unwrap_or_else(|| {
                panic!(
                    "{label} fehlt: {:#?}",
                    b.formulas.iter().map(|l| &l.label).collect::<Vec<_>>()
                )
            })
    }

    fn build_tpl(name: &str) -> Built {
        let mut sc = default_scene();
        sc.objects.clear();
        let t = templates().into_iter().find(|t| t.name == name).unwrap();
        sc.add(name, (t.make)());
        let mut cache = Cache::new();
        let b = build(&sc.objects[0], &sc, &mut cache);
        assert!(b.errors.is_empty(), "{:?}", b.errors);
        b
    }

    #[test]
    fn icosahedron_exact() {
        let b = build_tpl("Ikosaeder");
        // V = 5(3+√5)/12 a³ = (5/6)(1+φ)a³ = 5/6·φ² a³
        assert_eq!(line(&b, "Volumen").uni, "(5/6 + 5/6·φ)a³");
        assert_eq!(line(&b, "Euler").uni, "V − E + F = 12 − 30 + 20 = 2");
        assert!(line(&b, "Symmetriegruppe")
            .uni
            .starts_with("Iₕ, Ordnung 120"));
        // zwei Diagonalklassen: Abstand 2 (übernächste Nachbarn) = aφ, Gegenecken = 2R = a√(2+φ)
        assert_eq!(line(&b, "Raumdiagonale d₁ (30×)").uni, "aφ");
        let d = line(&b, "Raumdiagonale d₂ (6×)");
        assert_eq!(d.uni, "a√(2 + φ)");
        assert!(
            (d.num.unwrap() - (1.0f64 + 1.618_033_988_749_895f64.powi(2)).sqrt()).abs() < 1e-12
        );
    }

    #[test]
    fn cube_and_truncation() {
        let b = build_tpl("Würfel");
        assert_eq!(line(&b, "Volumen").uni, "a³");
        assert_eq!(line(&b, "Oberfläche").uni, "6a²");
        assert_eq!(line(&b, "Raumdiagonale").uni, "a√3");
        assert_eq!(line(&b, "Umkugelradius").uni, "1/2·a√3");
        let b = build_tpl("Kuboktaeder");
        assert_eq!(line(&b, "Euler").uni, "V − E + F = 12 − 24 + 14 = 2");
        assert_eq!(line(&b, "Volumen").uni, "5/6·a³");
        let b = build_tpl("Würfelstumpf");
        assert_eq!(line(&b, "Euler").uni, "V − E + F = 24 − 36 + 14 = 2");
        assert!(line(&b, "Kante").uni.contains('√'));
    }

    #[test]
    fn plastic_cuboid() {
        let b = build_tpl("Van-der-Laan-Quader 1 × ρ × ρ²");
        let d = line(&b, "Raumdiagonale");
        // 1 + ρ² + ρ⁴ = 1 + ρ + 2ρ²
        assert_eq!(d.uni, "√(1 + ρ + 2ρ²)");
    }

    #[test]
    fn heptagonal_prism() {
        let b = build_tpl("Prisma (n = 7)");
        assert_eq!(line(&b, "Euler").uni, "V − E + F = 14 − 21 + 9 = 2");
        assert!(b.legend.iter().any(|l| l.contains("γ₇")), "{:?}", b.legend);
        assert!(
            line(&b, "Symmetriegruppe").uni.starts_with("D₇ₕ"),
            "{}",
            line(&b, "Symmetriegruppe").uni
        );
    }

    #[test]
    fn custom_generator_and_exports() {
        let mut sc = default_scene();
        sc.objects.clear();
        sc.gens.push(CustomGen {
            key: "alpha".into(),
            poly: "x^5 - x - 1".into(),
            approx: 1.17,
        });
        sc.add(
            "Selmer-Quader",
            Kind::Polyhedron {
                base: PolyBase::Cuboid {
                    a: s("1"),
                    b: s("alpha"),
                    c: s("alpha^2"),
                },
                ops: vec![],
            },
        );
        let mut cache = Cache::new();
        let b = build(&sc.objects[0], &sc, &mut cache);
        assert!(b.errors.is_empty(), "{:?}", b.errors);
        // 1 + α² + α⁴ bleibt unreduziert (Grad 5), die Diagonale enthält α
        let d = line(&b, "Raumdiagonale");
        assert_eq!(d.uni, "√(1 + α² + α⁴)");
        assert!(b.legend[0].contains("x^5 - x - 1"), "{:?}", b.legend);
        let tex = crate::export::latex_object(&sc.objects[0], &b);
        assert!(
            tex.contains("\\begin{align*}") && tex.contains("\\alpha"),
            "{tex}"
        );
        let stl = crate::export::stl_struts("t", &[(DVec3::ZERO, DVec3::X)], 0.1, 10.0);
        assert!(stl.starts_with("solid t") && stl.contains("facet normal"));
    }

    #[test]
    fn free_framework() {
        let mut sc = default_scene();
        sc.objects.clear();
        let pts = vec![
            [s("0"), s("0"), s("0")],
            [s("1"), s("0"), s("0")],
            [s("0"), s("1"), s("0")],
            [s("0"), s("0"), s("1")],
        ];
        sc.add(
            "Gerüst",
            Kind::Free {
                pts: pts.clone(),
                struts: vec![(0, 1), (0, 2), (0, 3)],
                hull: false,
            },
        );
        let mut cache = Cache::new();
        let b = build(&sc.objects[0], &sc, &mut cache);
        assert!(b.errors.is_empty(), "{:?}", b.errors);
        assert!(line(&b, "Maxwell").uni.contains("sicher nicht starr"));
        assert_eq!(line(&b, "Punktabstand").uni, "√2");
        sc.objects[0].kind = Kind::Free {
            pts,
            struts: vec![],
            hull: true,
        };
        let b = build(&sc.objects[0], &sc, &mut cache);
        assert_eq!(line(&b, "Volumen").uni, "1/6");
    }

    #[test]
    fn surfaces() {
        let b = build_tpl("Kleinsche Flasche (Achterform)");
        assert!(
            line(&b, "Topologie").uni.contains("Kleinsche Flasche"),
            "{}",
            line(&b, "Topologie").uni
        );
        let b = build_tpl("Brezel (Geschlecht 2)");
        assert!(
            line(&b, "Topologie").uni.contains("g = 2"),
            "{}",
            line(&b, "Topologie").uni
        );
        let b = build_tpl("Kleeblattknoten");
        assert!(line(&b, "Geschlossene").uni.contains("Fáry"));
    }
}
