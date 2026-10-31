//! Objektübergreifende Auswertung: Schnittebenen (mit exaktem Schnittpolygon
//! bei Polyedern) und Interaktionen der Verstrebungen verschiedener Objekte.

use crate::algebra::*;
use crate::expr;
use crate::mesh::{self, Mesh};
use crate::poly::{self, V3};
use crate::scene::{self, Built, Obj, Scene, Section};
use glam::DVec3;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Ein Punkt mit Zahlwert und (falls bekannt) exakter Darstellung.
#[derive(Clone, Debug)]
pub struct Pt {
    pub p: DVec3,
    pub uni: String,
    pub tex: String,
}

fn pt_exact(r: &Ring, v: &V3) -> Pt {
    let u: Vec<String> = v.iter().map(|x| r.fmt_sym(x, false)).collect();
    let t: Vec<String> = v.iter().map(|x| r.fmt_sym(x, true)).collect();
    Pt {
        p: poly::v3_eval(r, v),
        uni: format!("({})", u.join(", ")),
        // ohne \left/\right, damit TeX zwischen den Koordinaten umbrechen darf
        tex: format!("({})", t.join(",\\allowbreak\\ ")),
    }
}

fn pt_num(p: DVec3) -> Pt {
    // keine „−0.000000“: Werte unterhalb der Anzeigegenauigkeit als 0
    let z = |x: f64| if x.abs() < 5e-7 { 0.0 } else { x };
    let s = format!("({:.6}, {:.6}, {:.6})", z(p.x), z(p.y), z(p.z));
    Pt {
        p,
        uni: format!("≈ {s}"),
        tex: format!("\\approx {s}"),
    }
}

fn scale_of(bs: &[&Built]) -> f64 {
    bs.iter()
        .map(|b| b.radius + b.center.length())
        .fold(1e-9, f64::max)
}

// ---------------------------------------------------------------------------
// Ebenen
// ---------------------------------------------------------------------------

/// Numerische Ebene, normiert: n̂·x = d̂
pub fn plane_num(sec: &Section, scene: &Scene) -> Result<(DVec3, f64), String> {
    let ctx = scene::Ctx::new(scene);
    let mut n = DVec3::ZERO;
    for k in 0..3 {
        n[k] = ctx.num(&sec.normal[k])?;
    }
    let d = ctx.num(&sec.offset)?;
    let l = n.length();
    if l < 1e-12 {
        return Err("Normale ist der Nullvektor".into());
    }
    Ok((n / l, d / l))
}

pub fn plane_text(sec: &Section, tex: bool) -> String {
    let vars = ["x", "y", "z"];
    let mut terms = vec![];
    for k in 0..3 {
        let c = sec.normal[k].trim();
        if c == "0" {
            continue;
        }
        let v = vars[k];
        if c == "1" {
            terms.push(v.to_string());
        } else if c == "-1" {
            terms.push(format!("-{v}"));
        } else {
            let cc = if tex {
                expr::parse(c)
                    .map(|e| expr::to_tex(&e))
                    .unwrap_or_else(|_| c.into())
            } else {
                c.into()
            };
            terms.push(format!("({cc}){v}"));
        }
    }
    let lhs = if terms.is_empty() {
        "0".into()
    } else {
        terms.join(" + ").replace("+ -", "- ")
    };
    let rhs = if tex {
        expr::parse(&sec.offset)
            .map(|e| expr::to_tex(&e))
            .unwrap_or_else(|_| sec.offset.clone())
    } else {
        sec.offset.clone()
    };
    format!("{lhs} = {rhs}")
}

#[derive(Clone, Debug)]
pub struct SectionRes {
    pub plane: usize,
    pub obj: String,
    pub exact: bool,
    pub ngon: usize,
    pub area: f64,
    pub area_uni: String,
    pub area_tex: String,
    pub perim: f64,
    pub perim_uni: String,
    pub perim_tex: String,
    pub points: Vec<Pt>,
    pub loops: usize,
    pub note: String,
    /// Schneidet die Ebene eine geschlossene Fläche? Nur dann ist A definiert.
    pub closed: bool,
}

#[derive(Clone, Debug)]
pub struct Profile {
    pub plane: usize,
    /// aktuelle Lage der Ebene (in Richtung n̂)
    pub current: f64,
    pub obj: String,
    pub samples: Vec<(f64, f64)>,
    pub integral: f64,
    pub volume: f64,
}

fn mesh_loops(segs: &[(DVec3, DVec3)], scale: f64) -> usize {
    let q = 1e-7 * scale.max(1e-9);
    let key = |p: DVec3| {
        (
            (p.x / q).round() as i64,
            (p.y / q).round() as i64,
            (p.z / q).round() as i64,
        )
    };
    let mut id: HashMap<(i64, i64, i64), usize> = HashMap::new();
    let mut parent: Vec<usize> = vec![];
    fn find(p: &mut Vec<usize>, x: usize) -> usize {
        let mut r = x;
        while p[r] != r {
            r = p[r];
        }
        p[x] = r;
        r
    }
    for (a, b) in segs {
        let mut ix = |p: DVec3, parent: &mut Vec<usize>| {
            let n = id.len();
            *id.entry(key(p)).or_insert_with(|| {
                parent.push(n);
                n
            })
        };
        let i = ix(*a, &mut parent);
        let j = ix(*b, &mut parent);
        let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
        if ri != rj {
            parent[ri] = rj;
        }
    }
    let n = parent.len();
    let roots: BTreeSet<usize> = (0..n).map(|i| find(&mut parent, i)).collect();
    roots.len()
}

/// Schnitt einer Ebene mit einem Objekt.
pub fn section(scene: &Scene, pi: usize, obj: &Obj, b: &Built) -> Option<SectionRes> {
    let sec = &scene.sections[pi];
    let (n, d) = plane_num(sec, scene).ok()?;
    let m = b.section_mesh.as_ref()?;
    let (segs, area) = mesh::slice(m, n, d);
    if segs.is_empty() {
        return None;
    }
    let perim: f64 = segs.iter().map(|(a, c)| (*c - *a).length()).sum();
    let loops = mesh_loops(&segs, b.radius);
    // Nur eine geschlossene Fläche begrenzt in der Ebene ein Gebiet; bei offenen Flächen
    // (Möbiusband, Helikoid …) ist die orientierte Summe bedeutungslos.
    let closed = is_closed(m);
    let (area, area_uni, area_tex) = if closed {
        (
            area.abs(),
            format!("≈ {:.6}", area.abs()),
            format!("\\approx {:.6}", area.abs()),
        )
    } else {
        (f64::NAN, "—".to_string(), String::new())
    };
    let mut res = SectionRes {
        plane: pi,
        obj: obj.name.clone(),
        exact: false,
        ngon: 0,
        closed,
        area,
        area_uni,
        area_tex,
        perim,
        perim_uni: format!("≈ {perim:.6}"),
        perim_tex: format!("\\approx {perim:.6}"),
        points: vec![],
        loops,
        note: if closed {
            String::new()
        } else {
            "offene Fläche: die Schnittkurven schließen kein Gebiet ein, nur ihre Länge ist definiert".into()
        },
    };
    if b.exact_ok && !b.hull_faces.is_empty() {
        match exact_section(scene, sec, obj, b) {
            Ok(Some(e)) => {
                return Some(SectionRes {
                    plane: pi,
                    loops: 1,
                    ..e
                });
            }
            Ok(None) => {}
            Err(e) => res.note = format!("nur numerisch: {e}"),
        }
    }
    Some(res)
}

fn exact_section(
    scene: &Scene,
    sec: &Section,
    obj: &Obj,
    b: &Built,
) -> Result<Option<SectionRes>, String> {
    let strs: Vec<&String> = sec
        .normal
        .iter()
        .chain(std::iter::once(&sec.offset))
        .collect();
    let mut keys = scene::gen_keys(obj, scene)?;
    keys.extend(scene::gen_keys_of(&strs, scene)?);
    let r = scene::scene_ring(scene, &keys)?;
    let ex = scene::exact_verts(obj, &r)?;
    if ex.len() != b.verts.len() {
        return Err("Eckenzahl weicht ab".into());
    }
    let mut nv = vec![];
    for c in &sec.normal {
        nv.push(expr::exact(&expr::parse(c)?, &r).map_err(|e| format!("Normale: {e}"))?);
    }
    let n: V3 = [nv[0].clone(), nv[1].clone(), nv[2].clone()];
    let d = expr::exact(&expr::parse(&sec.offset)?, &r).map_err(|e| format!("Lage: {e}"))?;
    let nn = poly::v3_eval(&r, &n);
    let sc = b.radius.max(1e-9) * nn.length();
    let sv: Vec<Sym> = ex
        .iter()
        .map(|v| r.ssub(&poly::v3_dot(&r, &n, v), &d))
        .collect();
    let val: Vec<f64> = sv.iter().map(|x| r.seval(x)).collect();
    let zero: Vec<bool> = sv
        .iter()
        .zip(&val)
        .map(|(x, v)| v.abs() < 1e-9 * sc && r.sis_zero(x))
        .collect();
    let mut pts: Vec<V3> = vec![];
    let mut pnum: Vec<DVec3> = vec![];
    let push = |p: V3, pts: &mut Vec<V3>, pnum: &mut Vec<DVec3>| {
        let q = poly::v3_eval(&r, &p);
        if !pnum.iter().any(|x| (*x - q).length() < 1e-9 * sc) {
            pts.push(p);
            pnum.push(q);
        }
    };
    for (i, v) in ex.iter().enumerate() {
        if zero[i] {
            push(v.clone(), &mut pts, &mut pnum);
        }
    }
    let mut edges = BTreeSet::new();
    for f in &b.hull_faces {
        for k in 0..f.len() {
            let (p, q) = (f[k], f[(k + 1) % f.len()]);
            edges.insert((p.min(q), p.max(q)));
        }
    }
    for (i, j) in edges {
        if zero[i] || zero[j] || (val[i] > 0.0) == (val[j] > 0.0) {
            continue;
        }
        let t = r.sdiv(&sv[i], &r.ssub(&sv[i], &sv[j]))?;
        let p = poly::v3_add(
            &r,
            &ex[i],
            &poly::v3_scale(&r, &poly::v3_sub(&r, &ex[j], &ex[i]), &t),
        );
        push(p, &mut pts, &mut pnum);
    }
    if pts.len() < 3 {
        return Ok(None);
    }
    // ordnen (konvex)
    let nh = nn.normalize();
    let c = pnum.iter().sum::<DVec3>() / pnum.len() as f64;
    let u = nh.any_orthonormal_vector();
    let w = nh.cross(u);
    let mut idx: Vec<usize> = (0..pts.len()).collect();
    idx.sort_by(|a, b| {
        let pa = pnum[*a] - c;
        let pb = pnum[*b] - c;
        pa.dot(w)
            .atan2(pa.dot(u))
            .partial_cmp(&pb.dot(w).atan2(pb.dot(u)))
            .unwrap()
    });
    let pts: Vec<V3> = idx.iter().map(|i| pts[*i].clone()).collect();
    let k = pts.len();
    let mut s: V3 = [r.sym_c(qi(0)), r.sym_c(qi(0)), r.sym_c(qi(0))];
    let mut sides = vec![];
    for i in 0..k {
        let (a, bb) = (&pts[i], &pts[(i + 1) % k]);
        s = poly::v3_add(&r, &s, &poly::v3_cross(&r, a, bb));
        let e = poly::v3_sub(&r, bb, a);
        sides.push((qi(1), r.sqrt(&poly::v3_dot(&r, &e, &e))));
    }
    let sn = poly::v3_dot(&r, &s, &n);
    // A = |S·n| / (2|n|); ist |n| exakt in K, bleibt A rational in S·n (gültig, solange das Vorzeichen fest ist)
    let nrad = r.sqrt(&poly::v3_dot(&r, &n, &n));
    let area = if nrad.inside.is_none() && nrad.absm.is_none() {
        let mut a = r.sdiv(&sn, &r.sscale(&nrad.outside, qi(2)))?;
        if r.seval(&a) < 0.0 {
            a = r.sneg(&a);
        }
        Radical {
            outside: a,
            absm: None,
            inside: None,
        }
    } else {
        let a2 = r.sdiv(
            &r.smul(&sn, &sn),
            &r.sscale(&poly::v3_dot(&r, &n, &n), qi(4)),
        )?;
        r.sqrt(&a2)
    };
    let perim = r.rad_sum(&sides);
    Ok(Some(SectionRes {
        plane: 0,
        obj: obj.name.clone(),
        exact: true,
        closed: true,
        ngon: k,
        area: r.rad_eval(&area),
        area_uni: r.fmt_rad(&area, false),
        area_tex: r.fmt_rad(&area, true),
        perim: perim.iter().map(|x| r.rad_eval(x)).sum(),
        perim_uni: r.fmt_rad_sum(&perim, false),
        perim_tex: r.fmt_rad_sum(&perim, true),
        points: pts.iter().map(|p| pt_exact(&r, p)).collect(),
        loops: 1,
        note: String::new(),
    }))
}

/// Cavalieri-Profil h ↦ A(h) über die Ausdehnung des Objekts in Normalenrichtung.
pub fn profile(scene: &Scene, pi: usize, obj: &Obj, b: &Built) -> Option<Profile> {
    let sec = &scene.sections[pi];
    let (n, current) = plane_num(sec, scene).ok()?;
    let m = b.section_mesh.as_ref()?;
    if !is_closed(m) {
        return None;
    }
    let (lo, hi) = m.v.iter().fold((f64::MAX, f64::MIN), |(l, h), p| {
        (l.min(n.dot(*p)), h.max(n.dot(*p)))
    });
    if hi - lo < 1e-12 {
        return None;
    }
    let k = 64;
    let dh = (hi - lo) / k as f64;
    let mut samples = vec![];
    let mut integral = 0.0;
    for i in 0..k {
        let h = lo + (i as f64 + 0.5) * dh;
        let a = mesh::slice(m, n, h).1.abs();
        integral += a * dh;
        samples.push((h, a));
    }
    Some(Profile {
        plane: pi,
        current,
        obj: obj.name.clone(),
        samples,
        integral,
        volume: m.volume().abs(),
    })
}

pub fn is_closed(m: &Mesh) -> bool {
    let mut cnt: HashMap<(u32, u32), u8> = HashMap::new();
    for t in &m.t {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            *cnt.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    !cnt.is_empty() && cnt.values().all(|c| *c == 2)
}

// ---------------------------------------------------------------------------
// Interaktionen
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Crossing {
    pub p: Pt,
    pub la: String,
    pub lb: String,
    /// „Kreuzung“ (beide im Inneren) oder „Stoß“ (Endpunkt auf Strebe)
    pub kind: &'static str,
    pub cos_uni: String,
    pub cos_tex: String,
    pub angle: f64,
    /// Indizes der beteiligten Streben (für Bündelpunkte)
    pub ia: usize,
    pub ib: usize,
}

/// Punkt, in dem sich mindestens drei Streben treffen (z. B. gemeinsamer Mittelpunkt).
#[derive(Clone, Debug)]
pub struct Bundle {
    pub p: Pt,
    /// (Bezeichnung, Anzahl verschiedener Streben) von Objekt A bzw. B
    pub a: Vec<(String, usize)>,
    pub b: Vec<(String, usize)>,
    /// verschiedene Paarwinkel zwischen A- und B-Streben: (|cos| uni, |cos| tex, Winkel)
    pub angles: Vec<(String, String, f64)>,
}

#[derive(Clone, Debug)]
pub struct PierceGroup {
    /// Objekt, dessen Streben eindringen
    pub owner: String,
    /// Körper, in den sie eindringen
    pub body: String,
    pub label: String,
    pub kind: &'static str,
    pub count: usize,
    pub each_uni: Option<String>,
    pub each_tex: Option<String>,
    pub total_uni: String,
    pub total_tex: String,
    pub total: f64,
}

#[derive(Clone, Debug, Default)]
pub struct InterRes {
    pub a: String,
    pub b: String,
    pub exact: bool,
    pub shared: Vec<Pt>,
    pub crossings: Vec<Crossing>,
    pub overlaps: Vec<(String, String, f64)>,
    pub pierces: Vec<PierceGroup>,
    pub near: Option<(f64, String, String)>,
    pub near_misses: usize,
    pub notes: Vec<String>,
    /// für die Darstellung
    pub marks: Vec<(DVec3, u8)>,
    pub inside: Vec<(DVec3, DVec3)>,
}

impl InterRes {
    pub fn is_empty(&self) -> bool {
        self.shared.is_empty()
            && self.crossings.is_empty()
            && self.overlaps.is_empty()
            && self.pierces.is_empty()
    }
}

#[derive(Clone)]
struct Seg {
    a: DVec3,
    b: DVec3,
    label: String,
    sref: Option<usize>,
}

const SEG_CAP: usize = 2500;

fn segs_of(b: &Built, notes: &mut Vec<String>, name: &str) -> Vec<Seg> {
    // Doppelte Streben (gleiche Endpunkte, auch umgekehrt) nur einmal zählen – z. B. Sehnen mit
    // Versatz ½ einer geschlossenen Kurve: t → t+½ und t+½ → t+1 sind dieselbe Strecke.
    let q = 1e-9 * b.radius.max(1e-9);
    let key = |p: DVec3| {
        (
            (p.x / q).round() as i64,
            (p.y / q).round() as i64,
            (p.z / q).round() as i64,
        )
    };
    let mut seen = std::collections::HashSet::new();
    let mut dups = 0usize;
    let mut v: Vec<Seg> = b
        .lines
        .iter()
        .filter_map(|l| {
            let lab = l.inter.as_ref()?;
            let (ka, kb) = (key(l.a), key(l.b));
            if !seen.insert((ka.min(kb), ka.max(kb))) {
                dups += 1;
                return None;
            }
            Some(Seg {
                a: l.a,
                b: l.b,
                label: lab.clone(),
                sref: l.sref,
            })
        })
        .collect();
    if dups > 0 {
        notes.push(format!("{name}: {dups} doppelte Strebe(n) zusammengefasst"));
    }
    if v.len() > SEG_CAP {
        notes.push(format!(
            "{name}: nur die ersten {SEG_CAP} von {} Streben ausgewertet",
            v.len()
        ));
        v.truncate(SEG_CAP);
    }
    v
}

/// Nächste Punkte zweier Strecken: (s, t, Abstand)
fn closest(p0: DVec3, p1: DVec3, q0: DVec3, q1: DVec3) -> (f64, f64, f64) {
    let d1 = p1 - p0;
    let d2 = q1 - q0;
    let r = p0 - q0;
    let a = d1.dot(d1);
    let e = d2.dot(d2);
    let f = d2.dot(r);
    let (mut s, mut t);
    if a <= 1e-300 && e <= 1e-300 {
        return (0.0, 0.0, (p0 - q0).length());
    }
    if a <= 1e-300 {
        s = 0.0;
        t = (f / e).clamp(0.0, 1.0);
    } else {
        let c = d1.dot(r);
        if e <= 1e-300 {
            t = 0.0;
            s = (-c / a).clamp(0.0, 1.0);
        } else {
            let b = d1.dot(d2);
            let den = a * e - b * b;
            s = if den > 1e-14 * a * e {
                ((b * f - c * e) / den).clamp(0.0, 1.0)
            } else {
                0.0
            };
            t = (b * s + f) / e;
            if t < 0.0 {
                t = 0.0;
                s = (-c / a).clamp(0.0, 1.0);
            } else if t > 1.0 {
                t = 1.0;
                s = ((b - c) / a).clamp(0.0, 1.0);
            }
        }
    }
    let d = (p0 + d1 * s) - (q0 + d2 * t);
    (s, t, d.length())
}

struct ExactSide {
    v: Vec<V3>,
}

fn exact_sides(
    scene: &Scene,
    oa: &Obj,
    ob: &Obj,
    ba: &Built,
    bb: &Built,
) -> Result<(Ring, ExactSide, ExactSide), String> {
    let mut keys = scene::gen_keys(oa, scene)?;
    keys.extend(scene::gen_keys(ob, scene)?);
    let r = scene::scene_ring(scene, &keys)?;
    let va = scene::exact_verts(oa, &r)?;
    let vb = scene::exact_verts(ob, &r)?;
    if va.len() != ba.verts.len() || vb.len() != bb.verts.len() {
        return Err("Eckenzahl weicht ab".into());
    }
    Ok((r, ExactSide { v: va }, ExactSide { v: vb }))
}

fn cos_angle(
    r: Option<&Ring>,
    ex: Option<(&V3, &V3)>,
    d1: DVec3,
    d2: DVec3,
) -> (String, String, f64) {
    let c = (d1.dot(d2) / (d1.length() * d2.length()))
        .abs()
        .clamp(0.0, 1.0);
    let ang = c.acos().to_degrees();
    if let (Some(r), Some((e1, e2))) = (r, ex) {
        let dot = poly::v3_dot(r, e1, e2);
        if let Ok(c2) = r.sdiv(
            &r.smul(&dot, &dot),
            &r.smul(&poly::v3_dot(r, e1, e1), &poly::v3_dot(r, e2, e2)),
        ) {
            if r.sis_zero(&c2) {
                return ("0".into(), "0".into(), ang);
            }
            let rad = r.sqrt(&c2);
            let (u, t) = (r.fmt_rad(&rad, false), r.fmt_rad(&rad, true));
            // Sehr lange Ausdrücke in K(Parameter): exakt am aktuellen (rationalen) Parameterwert angeben
            if t.len() > 160 {
                if let Some((c2v, du, dt)) = r.at_params(&c2) {
                    let rv = r.sqrt(&c2v);
                    return (
                        format!("{}  (bei {du})", r.fmt_rad(&rv, false)),
                        format!("{} \\quad (\\text{{bei }} {dt})", r.fmt_rad(&rv, true)),
                        ang,
                    );
                }
            }
            return (u, t, ang);
        }
    }
    (format!("≈ {c:.6}"), format!("\\approx {c:.6}"), ang)
}

/// Interaktion der Verstrebungen zweier Objekte.
pub fn interact(scene: &Scene, oa: &Obj, ba: &Built, ob: &Obj, bb: &Built) -> InterRes {
    let mut res = InterRes {
        a: oa.name.clone(),
        b: ob.name.clone(),
        ..Default::default()
    };
    let sa = segs_of(ba, &mut res.notes, &oa.name);
    let sb = segs_of(bb, &mut res.notes, &ob.name);
    let sc = scale_of(&[ba, bb]);
    let eps = 1e-9 * sc;
    let exact = if ba.exact_ok && bb.exact_ok {
        match exact_sides(scene, oa, ob, ba, bb) {
            Ok(x) => Some(x),
            Err(e) => {
                res.notes.push(format!("nur numerisch: {e}"));
                None
            }
        }
    } else {
        None
    };
    res.exact = exact.is_some();
    let rr = exact.as_ref().map(|x| &x.0);

    // gemeinsame Knoten
    for (i, p) in ba.verts.iter().enumerate() {
        for (j, q) in bb.verts.iter().enumerate() {
            if (*p - *q).length() < eps {
                match &exact {
                    Some((r, ea, eb)) => {
                        if poly::v3_eq(r, &ea.v[i], &eb.v[j]) {
                            res.shared.push(pt_exact(r, &ea.v[i]));
                            res.marks.push((*p, 0));
                        } else {
                            res.near_misses += 1;
                        }
                    }
                    None => {
                        res.shared.push(pt_num(*p));
                        res.marks.push((*p, 0));
                    }
                }
            }
        }
    }

    // Strebe gegen Strebe
    let strut_ex = |side: &ExactSide, refs: &[scene::StrutRef], s: &Seg| -> Option<(V3, V3)> {
        let k = s.sref?;
        let st = refs.get(k)?;
        Some((side.v[st.i].clone(), side.v[st.j].clone()))
    };
    let mut near = f64::MAX;
    let mut near_pair = (String::new(), String::new());
    let do_near = sa.len() * sb.len() <= 3_000_000;
    if !do_near {
        res.notes
            .push("kleinster Abstand nicht berechnet (zu viele Strebenpaare)".into());
    }
    let bbox = |s: &Seg| (s.a.min(s.b), s.a.max(s.b));
    let bxb: Vec<(DVec3, DVec3)> = sb.iter().map(bbox).collect();
    for (xi, x) in sa.iter().enumerate() {
        let (lo, hi) = bbox(x);
        for (yi, y) in sb.iter().enumerate() {
            let (lo2, hi2) = bxb[yi];
            let overlap = lo.x <= hi2.x + eps
                && lo2.x <= hi.x + eps
                && lo.y <= hi2.y + eps
                && lo2.y <= hi.y + eps
                && lo.z <= hi2.z + eps
                && lo2.z <= hi.z + eps;
            if !overlap && !do_near {
                continue;
            }
            let (s, t, dist) = closest(x.a, x.b, y.a, y.b);
            if dist > eps {
                if dist < near {
                    near = dist;
                    near_pair = (x.label.clone(), y.label.clone());
                }
                continue;
            }
            let d1 = x.b - x.a;
            let d2 = y.b - y.a;
            let at_end = |u: f64, len: f64| u * len < eps || (1.0 - u) * len < eps;
            let (ea, eb) = (at_end(s, d1.length()), at_end(t, d2.length()));
            if ea && eb {
                continue; // gemeinsamer Knoten, oben gezählt
            }
            // exakte Prüfung
            let ex_pair = match (&exact, rr) {
                (Some((r, sa_ex, sb_ex)), Some(_)) => match (
                    strut_ex(sa_ex, &ba.struts, x),
                    strut_ex(sb_ex, &bb.struts, y),
                ) {
                    (Some(p), Some(q)) => Some((r, p, q)),
                    _ => None,
                },
                _ => None,
            };
            let parallel = d1.cross(d2).length() < 1e-9 * d1.length() * d2.length();
            if parallel {
                // kollinear überlappend
                let dir = d1.normalize();
                let (a0, a1): (f64, f64) = (0.0, d1.length());
                let (b0, b1) = ((y.a - x.a).dot(dir), (y.b - x.a).dot(dir));
                let (b0, b1) = (b0.min(b1), b0.max(b1));
                let len = a1.min(b1) - a0.max(b0);
                if len > eps {
                    if let Some((r, p, q)) = &ex_pair {
                        let w = poly::v3_sub(r, &q.0, &p.0);
                        let dd = poly::v3_sub(r, &p.1, &p.0);
                        if !poly::v3_cross(r, &w, &dd).iter().all(|c| r.sis_zero(c)) {
                            res.near_misses += 1;
                            continue;
                        }
                    }
                    res.overlaps.push((x.label.clone(), y.label.clone(), len));
                    res.inside
                        .push((x.a + dir * a0.max(b0), x.a + dir * a1.min(b1)));
                }
                continue;
            }
            let kind = if ea || eb { "Stoß" } else { "Kreuzung" };
            match &ex_pair {
                Some((r, p, q)) => {
                    let d1e = poly::v3_sub(r, &p.1, &p.0);
                    let d2e = poly::v3_sub(r, &q.1, &q.0);
                    let w = poly::v3_sub(r, &q.0, &p.0);
                    let c = poly::v3_cross(r, &d1e, &d2e);
                    let cop = poly::v3_dot(r, &w, &c);
                    if !r.sis_zero(&cop) {
                        res.near_misses += 1;
                        continue;
                    }
                    let cc = poly::v3_dot(r, &c, &c);
                    let se = match r.sdiv(&poly::v3_dot(r, &poly::v3_cross(r, &w, &d2e), &c), &cc) {
                        Ok(v) => v,
                        Err(_) => continue,
                    };
                    let xp = poly::v3_add(r, &p.0, &poly::v3_scale(r, &d1e, &se));
                    let (cu, ct, ang) = cos_angle(Some(r), Some((&d1e, &d2e)), d1, d2);
                    let pt = pt_exact(r, &xp);
                    res.marks.push((pt.p, 1));
                    res.crossings.push(Crossing {
                        p: pt,
                        la: x.label.clone(),
                        lb: y.label.clone(),
                        kind,
                        cos_uni: cu,
                        cos_tex: ct,
                        angle: ang,
                        ia: xi,
                        ib: yi,
                    });
                }
                None => {
                    let xp = x.a + d1 * s;
                    let (cu, ct, ang) = cos_angle(None, None, d1, d2);
                    res.marks.push((xp, 1));
                    res.crossings.push(Crossing {
                        p: pt_num(xp),
                        la: x.label.clone(),
                        lb: y.label.clone(),
                        kind,
                        cos_uni: cu,
                        cos_tex: ct,
                        angle: ang,
                        ia: xi,
                        ib: yi,
                    });
                }
            }
        }
    }
    if near < f64::MAX && do_near {
        res.near = Some((near, near_pair.0, near_pair.1));
    }

    // Durchstoßungen in beide Richtungen
    let ex_a = exact.as_ref().map(|(r, a, _)| (r, a));
    let ex_b = exact.as_ref().map(|(r, _, b)| (r, b));
    pierce(&mut res, &oa.name, &sa, ba, ex_a, &ob.name, bb, ex_b, eps);
    pierce(&mut res, &ob.name, &sb, bb, ex_b, &oa.name, ba, ex_a, eps);
    res
}

#[allow(clippy::too_many_arguments)]
fn pierce(
    res: &mut InterRes,
    owner: &str,
    segs: &[Seg],
    bo: &Built,
    ex_own: Option<(&Ring, &ExactSide)>,
    body: &str,
    bb: &Built,
    ex_body: Option<(&Ring, &ExactSide)>,
    eps: f64,
) {
    // (Bezeichnung, Art) → Längen
    let mut groups: BTreeMap<(String, &'static str), (usize, Vec<Radical>, f64)> = BTreeMap::new();
    let mut ring_used: Option<&Ring> = None;
    if !bb.hull_planes.is_empty() {
        // exakte Ebenen des Körpers
        let planes_ex: Option<Vec<(V3, Sym)>> = ex_body.map(|(r, side)| {
            bb.hull_faces
                .iter()
                .zip(&bb.hull_planes)
                .map(|(f, (nn, _))| {
                    let (q0, q1, q2) = (&side.v[f[0]], &side.v[f[1]], &side.v[f[2]]);
                    let mut n =
                        poly::v3_cross(r, &poly::v3_sub(r, q1, q0), &poly::v3_sub(r, q2, q0));
                    if poly::v3_eval(r, &n).dot(*nn) < 0.0 {
                        n = [r.sneg(&n[0]), r.sneg(&n[1]), r.sneg(&n[2])];
                    }
                    let d = poly::v3_dot(r, &n, q0);
                    (n, d)
                })
                .collect()
        });
        for s in segs {
            let d = s.b - s.a;
            let len = d.length();
            if len < eps {
                continue;
            }
            let (mut t0, mut t1) = (0.0f64, 1.0f64);
            let (mut f0, mut f1) = (None, None);
            let mut empty = false;
            for (fi, (n, off)) in bb.hull_planes.iter().enumerate() {
                let den = n.dot(d);
                let num = off - n.dot(s.a);
                if den.abs() < 1e-15 {
                    if num < -eps {
                        empty = true;
                        break;
                    }
                    continue;
                }
                let t = num / den;
                if den > 0.0 {
                    if t < t1 {
                        t1 = t;
                        f1 = Some(fi);
                    }
                } else if t > t0 {
                    t0 = t;
                    f0 = Some(fi);
                }
                if t0 > t1 {
                    empty = true;
                    break;
                }
            }
            if empty || (t1 - t0) * len < eps * 10.0 {
                continue;
            }
            // auf der Oberfläche?
            let mid = s.a + d * ((t0 + t1) * 0.5);
            let on_surface = bb
                .hull_planes
                .iter()
                .any(|(n, off)| (n.dot(mid) - off).abs() < eps * 10.0);
            let kind: &'static str = if on_surface {
                "auf der Oberfläche"
            } else {
                match (t0 * len < eps, (1.0 - t1) * len < eps) {
                    (true, true) => "ganz innen",
                    (false, false) => "durchstoßend",
                    _ => "eintretend",
                }
            };
            let e = groups
                .entry((s.label.clone(), kind))
                .or_insert((0, vec![], 0.0));
            e.0 += 1;
            e.2 += (t1 - t0) * len;
            res.inside.push((s.a + d * t0, s.a + d * t1));
            if t0 * len >= eps {
                res.marks.push((s.a + d * t0, 2));
            }
            if (1.0 - t1) * len >= eps {
                res.marks.push((s.a + d * t1, 2));
            }
            // exakte Innenlänge
            if let (Some((r, own)), Some(pl), Some(k)) = (ex_own, planes_ex.as_ref(), s.sref) {
                if let Some(st) = bo.struts.get(k) {
                    let (p0, p1) = (&own.v[st.i], &own.v[st.j]);
                    let de = poly::v3_sub(r, p1, p0);
                    let tpar = |fi: Option<usize>, default: i64| -> Option<Sym> {
                        match fi {
                            None => Some(r.sym_c(qi(default))),
                            Some(fi) => {
                                let (n, off) = &pl[fi];
                                r.sdiv(
                                    &r.ssub(off, &poly::v3_dot(r, n, p0)),
                                    &poly::v3_dot(r, n, &de),
                                )
                                .ok()
                            }
                        }
                    };
                    let te0 = if t0 * len < eps {
                        Some(r.sym_c(qi(0)))
                    } else {
                        tpar(f0, 0)
                    };
                    let te1 = if (1.0 - t1) * len < eps {
                        Some(r.sym_c(qi(1)))
                    } else {
                        tpar(f1, 1)
                    };
                    if let (Some(a0), Some(a1)) = (te0, te1) {
                        let mut rad = r.sqrt(&poly::v3_dot(r, &de, &de));
                        rad.outside = r.smul(&rad.outside, &r.ssub(&a1, &a0));
                        e.1.push(rad);
                        ring_used = Some(r);
                    }
                }
            }
        }
    } else if let Some(m) = bb.section_mesh.as_ref().filter(|m| is_closed(m)) {
        let segs = if segs.len() > 600 { &segs[..600] } else { segs };
        for s in segs {
            let d = s.b - s.a;
            let len = d.length();
            if len < eps {
                continue;
            }
            let mut hits: Vec<f64> = vec![];
            let mut beyond = 0usize;
            for t in &m.t {
                if let Some(u) = ray_tri(
                    s.a,
                    d,
                    m.v[t[0] as usize],
                    m.v[t[1] as usize],
                    m.v[t[2] as usize],
                ) {
                    if u > 0.0 && u < 1.0 {
                        hits.push(u);
                    } else if u >= 1.0 {
                        beyond += 1;
                    }
                }
            }
            if hits.is_empty() && (beyond % 2 == 0) {
                continue;
            }
            hits.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let mut inside = (hits.len() + beyond) % 2 == 1;
            let mut last = 0.0;
            let mut tot = 0.0;
            for h in hits.iter().chain(std::iter::once(&1.0)) {
                if inside {
                    tot += (h - last) * len;
                    res.inside.push((s.a + d * last, s.a + d * *h));
                }
                if *h < 1.0 {
                    res.marks.push((s.a + d * *h, 2));
                }
                inside = !inside;
                last = *h;
            }
            if tot > eps {
                let kind = if hits.is_empty() {
                    "ganz innen"
                } else {
                    "durchstoßend"
                };
                let e = groups
                    .entry((s.label.clone(), kind))
                    .or_insert((0, vec![], 0.0));
                e.0 += 1;
                e.2 += tot;
            }
        }
    }
    for ((label, kind), (count, rads, total)) in groups {
        let (each_uni, each_tex, total_uni, total_tex) = match ring_used {
            Some(r) if rads.len() == count => {
                let sum = r.rad_sum(&rads.iter().map(|x| (qi(1), x.clone())).collect::<Vec<_>>());
                let same = rads.windows(2).all(|w| {
                    r.seq(&w[0].outside, &w[1].outside)
                        && w[0].inside == w[1].inside
                        && w[0].absm == w[1].absm
                });
                (
                    if same {
                        Some(r.fmt_rad(&rads[0], false))
                    } else {
                        None
                    },
                    if same {
                        Some(r.fmt_rad(&rads[0], true))
                    } else {
                        None
                    },
                    r.fmt_rad_sum(&sum, false),
                    r.fmt_rad_sum(&sum, true),
                )
            }
            _ => (
                None,
                None,
                format!("≈ {total:.6}"),
                format!("\\approx {total:.6}"),
            ),
        };
        res.pierces.push(PierceGroup {
            owner: owner.into(),
            body: body.into(),
            label,
            kind,
            count,
            each_uni,
            each_tex,
            total_uni,
            total_tex,
            total,
        });
    }
}

/// Strahl p + u·d gegen Dreieck (Möller–Trumbore), liefert u.
fn ray_tri(p: DVec3, d: DVec3, a: DVec3, b: DVec3, c: DVec3) -> Option<f64> {
    let e1 = b - a;
    let e2 = c - a;
    let h = d.cross(e2);
    let det = e1.dot(h);
    if det.abs() < 1e-14 * e1.length() * e2.length() * d.length() {
        return None;
    }
    let f = 1.0 / det;
    let s = p - a;
    let u = f * s.dot(h);
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = f * d.dot(q);
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    Some(f * e2.dot(q))
}

/// Alle Schnittebenen und Interaktionen der sichtbaren Objekte auswerten.
pub fn analyze_all(
    scene: &Scene,
    vis: &[(&Obj, &Built)],
    interactions: bool,
) -> (Vec<SectionRes>, Vec<Profile>, Vec<InterRes>) {
    let mut secs = vec![];
    let mut profs = vec![];
    for (pi, sec) in scene.sections.iter().enumerate() {
        if !sec.visible {
            continue;
        }
        for (o, b) in vis {
            if let Some(r) = section(scene, pi, o, b) {
                secs.push(r);
            }
            if sec.profile {
                if let Some(p) = profile(scene, pi, o, b) {
                    profs.push(p);
                }
            }
        }
    }
    let mut inters = vec![];
    if interactions {
        let has = |b: &Built| b.lines.iter().any(|l| l.inter.is_some());
        for i in 0..vis.len() {
            for j in i + 1..vis.len() {
                let (oa, ba) = vis[i];
                let (ob, bb) = vis[j];
                if has(ba) || has(bb) {
                    inters.push(interact(scene, oa, ba, ob, bb));
                }
            }
        }
    }
    (secs, profs, inters)
}

/// Kurzfassung der Kreuzungen: gleiche (Strebenpaar, Art, Winkel) zusammengefasst.
/// Kreuzungen, die in einem Punkt zusammenfallen und mindestens drei Streben betreffen,
/// werden zu Bündelpunkten; der Rest bleibt als einzelne Kreuzungen.
pub fn bundles(cs: &[Crossing]) -> (Vec<Bundle>, Vec<Crossing>) {
    let mut groups: Vec<Vec<usize>> = vec![];
    for (i, c) in cs.iter().enumerate() {
        let tol = 1e-9 * c.p.p.length().max(1.0);
        match groups
            .iter_mut()
            .find(|g| (cs[g[0]].p.p - c.p.p).length() < tol)
        {
            Some(g) => g.push(i),
            None => groups.push(vec![i]),
        }
    }
    let mut out = vec![];
    let mut rest = vec![];
    for g in groups {
        let ia: BTreeSet<usize> = g.iter().map(|i| cs[*i].ia).collect();
        let ib: BTreeSet<usize> = g.iter().map(|i| cs[*i].ib).collect();
        if ia.len() + ib.len() < 3 {
            rest.extend(g.iter().map(|i| cs[*i].clone()));
            continue;
        }
        let count = |pick: &dyn Fn(&Crossing) -> (String, usize)| -> Vec<(String, usize)> {
            let mut m: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
            for i in &g {
                let (l, k) = pick(&cs[*i]);
                m.entry(l).or_default().insert(k);
            }
            m.into_iter().map(|(l, s)| (l, s.len())).collect()
        };
        // gleiche Winkel nur einmal; exakte Darstellung vor numerischer
        let mut angles: Vec<(String, String, f64)> = vec![];
        let mut order: Vec<usize> = g.clone();
        order.sort_by_key(|i| cs[*i].cos_uni.starts_with('≈'));
        for i in order {
            let c = &cs[i];
            if !angles
                .iter()
                .any(|a| a.0 == c.cos_uni || (a.2 - c.angle).abs() < 1e-7)
            {
                angles.push((c.cos_uni.clone(), c.cos_tex.clone(), c.angle));
            }
        }
        angles.sort_by(|x, y| x.2.partial_cmp(&y.2).unwrap());
        out.push(Bundle {
            p: cs[g[0]].p.clone(),
            a: count(&|c| (c.la.clone(), c.ia)),
            b: count(&|c| (c.lb.clone(), c.ib)),
            angles,
        });
    }
    (out, rest)
}

pub fn crossing_groups(
    cs: &[Crossing],
) -> Vec<(String, String, &'static str, String, String, f64, Vec<Pt>)> {
    let mut out: Vec<(String, String, &'static str, String, String, f64, Vec<Pt>)> = vec![];
    for c in cs {
        if let Some(g) = out
            .iter_mut()
            .find(|g| g.0 == c.la && g.1 == c.lb && g.2 == c.kind && g.3 == c.cos_uni)
        {
            if !g
                .6
                .iter()
                .any(|p| (p.p - c.p.p).length() < 1e-9 * p.p.length().max(1.0))
            {
                g.6.push(c.p.clone());
            }
        } else {
            out.push((
                c.la.clone(),
                c.lb.clone(),
                c.kind,
                c.cos_uni.clone(),
                c.cos_tex.clone(),
                c.angle,
                vec![c.p.clone()],
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::*;

    fn s(x: &str) -> String {
        x.to_string()
    }

    fn two_cubes() -> (Scene, Vec<Built>) {
        let mut sc = default_scene();
        sc.objects.clear();
        sc.add(
            "W1",
            Kind::Polyhedron {
                base: PolyBase::Platonic {
                    kind: Platonic::Cube,
                    a: s("2"),
                },
                ops: vec![],
            },
        );
        sc.add(
            "W2",
            Kind::Polyhedron {
                base: PolyBase::Platonic {
                    kind: Platonic::Cube,
                    a: s("2"),
                },
                ops: vec![],
            },
        );
        sc.objects[1].offset = [s("1"), s("1"), s("0")];
        sc.objects[1].layers.space_diag = true;
        sc.objects[0].layers.space_diag = true;
        let mut cache = Cache::new();
        let bs = sc
            .objects
            .iter()
            .map(|o| build(o, &sc, &mut cache))
            .collect();
        (sc, bs)
    }

    #[test]
    fn exact_square_section_of_octahedron() {
        let mut sc = default_scene();
        sc.objects.clear();
        sc.params.push(Param {
            name: "s".into(),
            value: 0.2,
            min: -1.0,
            max: 1.0,
            animate: false,
            speed: 0.1,
            dir: 1.0,
        });
        sc.add(
            "Oktaeder",
            Kind::Polyhedron {
                base: PolyBase::Platonic {
                    kind: Platonic::Octa,
                    a: s("sqrt2"),
                },
                ops: vec![],
            },
        );
        sc.sections.push(Section::new("E", 2, "s"));
        let mut cache = Cache::new();
        let b = build(&sc.objects[0], &sc, &mut cache);
        let r = section(&sc, 0, &sc.objects[0], &b).unwrap();
        assert!(r.exact, "{}", r.note);
        assert_eq!(r.ngon, 4);
        // Oktaeder |x|+|y|+|z| ≤ 1: Schnitt bei z = s ist ein Quadrat mit Fläche 2(1 − s)²
        assert_eq!(r.area_uni, "2s² − 4s + 2");
        assert!((r.area - 2.0 * 0.8f64.powi(2)).abs() < 1e-12);
        let p = profile(&sc, 0, &sc.objects[0], &b).unwrap();
        assert!((p.integral - 4.0 / 3.0).abs() < 1e-3, "{}", p.integral);
    }

    #[test]
    fn cube_cube_interactions() {
        let (sc, bs) = two_cubes();
        let r = interact(&sc, &sc.objects[0], &bs[0], &sc.objects[1], &bs[1]);
        assert!(r.exact, "{:?}", r.notes);
        // Würfel [−1,1]³ und [0,2]×[0,2]×[−1,1]: gemeinsamer Knoten keiner, Kantenkreuzungen
        assert!(r.shared.is_empty());
        assert!(
            r.crossings
                .iter()
                .any(|c| c.kind == "Stoß" || c.kind == "Kreuzung"),
            "{:?}",
            r.crossings
                .iter()
                .map(|c| (&c.la, &c.lb, c.kind, &c.p.uni))
                .collect::<Vec<_>>()
        );
        // Raumdiagonalen von W1 laufen durch W2 hindurch, exakte Innenlängen
        let g = r
            .pierces
            .iter()
            .find(|g| g.owner == "W1" && g.label == "Raumdiagonale")
            .expect("Durchstoßung fehlt");
        assert!(!g.total_uni.starts_with('≈'), "{}", g.total_uni);
        assert!(g.total > 0.0);
        // die Diagonale (−1,−1,−1)→(1,1,1) liegt zur Hälfte in W2: Innenlänge √3
        assert!(r
            .pierces
            .iter()
            .any(|g| g.owner == "W1" && g.each_uni.as_deref() == Some("√3")
                || g.total_uni.contains("√3")));
    }

    #[test]
    fn compound_and_export() {
        let txt = std::fs::read_to_string("examples/interaktion.strebwerk").unwrap();
        let f: crate::app::SaveFile = serde_json::from_str(&txt).unwrap();
        let mut cache = Cache::new();
        let built: Vec<(Obj, Built)> = f
            .scene
            .objects
            .iter()
            .map(|o| (o.clone(), build(o, &f.scene, &mut cache)))
            .collect();
        let vis: Vec<(&Obj, &Built)> = built.iter().map(|(o, b)| (o, b)).collect();
        let (secs, profs, inters) = analyze_all(&f.scene, &vis, true);
        let okt = secs.iter().find(|r| r.obj == "Oktaeder").unwrap();
        assert_eq!(okt.area_uni, "2s² − 8s + 8");
        assert_eq!(profs.len(), 2);
        let r = &inters[0];
        assert!(r.exact);
        let g = crossing_groups(&r.crossings);
        assert_eq!(g.len(), 1);
        assert_eq!((g[0].3.as_str(), g[0].6.len()), ("0", 12));
        let p = r
            .pierces
            .iter()
            .find(|p| p.label == "Raumdiagonale")
            .unwrap();
        assert_eq!(p.each_uni.as_deref(), Some("4/3·√3"));
        let tex = crate::export::latex_document(&vis, &f.scene, &secs, &profs, &inters);
        assert!(
            tex.contains("\\section*{Schnittebenen}")
                && tex.contains("\\section*{Interaktionen der Verstrebungen}")
        );
        assert!(tex.contains("2s^{2} - 8s + 8") && tex.contains("pgfplots"));
        assert!(!tex.contains("\\begin{itemize}\n\\end{itemize}"));
    }

    /// Julians Szene: offene Fläche + Kugel, Schnitt z = s
    #[test]
    fn open_surface_section_and_escaping() {
        let mut sc = default_scene();
        sc.objects.clear();
        sc.params.push(Param {
            name: "s".into(),
            value: 0.4,
            min: -1.0,
            max: 1.0,
            animate: false,
            speed: 0.1,
            dir: 1.0,
        });
        for name in ["Möbiusband", "Kugel x² + y² + z² = a²"] {
            let t = templates().into_iter().find(|t| t.name == name).unwrap();
            sc.add(name, (t.make)());
        }
        sc.sections.push(Section::new("E1", 2, "s"));
        let mut cache = Cache::new();
        let built: Vec<(Obj, Built)> = sc
            .objects
            .iter()
            .map(|o| (o.clone(), build(o, &sc, &mut cache)))
            .collect();
        let vis: Vec<(&Obj, &Built)> = built.iter().map(|(o, b)| (o, b)).collect();
        let (secs, profs, inters) = analyze_all(&sc, &vis, true);
        let mob = secs.iter().find(|r| r.obj == "Möbiusband").unwrap();
        assert!(!mob.closed && mob.area.is_nan());
        let kug = secs.iter().find(|r| r.obj.starts_with("Kugel")).unwrap();
        assert!(kug.closed && (kug.area - std::f64::consts::PI * (1.0 - 0.16)).abs() < 0.02);
        assert_eq!(profs.len(), 1);
        let tex = crate::export::latex_document(&vis, &sc, &secs, &profs, &inters);
        assert!(!tex.contains("&= \\approx"), "= ≈ im Export");
        assert!(tex.contains("\\textless{}") && !tex.contains("{F < 0}"));
        assert!(tex.contains("Länge der Schnittkurve"));
        assert!(tex.contains("1 Kreuzhaube, 1 Randkurve"));
    }

    /// Julians zweite Szene: Sehnen von (2,5)-Torusknoten und Möbiusband
    #[test]
    fn knot_and_moebius_chords() {
        let mut sc = default_scene();
        sc.objects.clear();
        for name in ["(2,5)-Torusknoten", "Möbiusband"] {
            let t = templates().into_iter().find(|t| t.name == name).unwrap();
            sc.add(name, (t.make)());
        }
        for o in &mut sc.objects {
            o.layers.chords = true;
        }
        let mut cache = Cache::new();
        let built: Vec<(Obj, Built)> = sc
            .objects
            .iter()
            .map(|o| (o.clone(), build(o, &sc, &mut cache)))
            .collect();
        let vis: Vec<(&Obj, &Built)> = built.iter().map(|(o, b)| (o, b)).collect();
        let (_, _, inters) = analyze_all(&sc, &vis, true);
        let r = &inters[0];
        // Die Knotensehne r(0) → r(π) liegt auf der x-Achse zwischen 1 und 3; der Startpunkt der
        // Möbius-Sehne bei u = 0 liegt darauf: genau ein Stoß, nicht doppelt gezählt.
        let st: Vec<_> = r
            .crossings
            .iter()
            .filter(|c| c.kind == "Stoß" && c.la == "Sehne" && c.lb == "Sehne")
            .collect();
        assert_eq!(
            st.len(),
            1,
            "{:?}",
            r.crossings
                .iter()
                .map(|c| (&c.la, &c.lb, c.kind, &c.p.uni))
                .collect::<Vec<_>>()
        );
        assert!(
            st[0].p.p.y.abs() < 1e-9
                && st[0].p.p.z.abs() < 1e-9
                && st[0].p.p.x > 1.0
                && st[0].p.p.x < 3.0
        );
        assert!(!st[0].p.uni.contains("-0.000000"));
        assert!(r.notes.iter().any(|n| n.contains("doppelte")));
        let tex = crate::export::latex_document(&vis, &sc, &[], &[], &inters);
        assert!(!tex.contains("= \\approx") && !tex.contains("Fáry–Milnor lässt"));
    }

    /// Julians Szene „anti-rho3“: Bündelpunkt im Ursprung, Winkel exakt bei h = 1
    #[test]
    fn anti_rho3_bundle() {
        let txt = std::fs::read_to_string("examples/anti-rho3.strebwerk").unwrap();
        let f: crate::app::SaveFile = serde_json::from_str(&txt).unwrap();
        let mut cache = Cache::new();
        let built: Vec<(Obj, Built)> = f.scene.objects.iter().map(|o| (o.clone(), build(o, &f.scene, &mut cache))).collect();
        let vis: Vec<(&Obj, &Built)> = built.iter().map(|(o, b)| (o, b)).collect();
        let (secs, _, inters) = analyze_all(&f.scene, &vis, true);
        let (bs, _) = bundles(&inters[0].crossings);
        assert_eq!(bs.len(), 1);
        let b = &bs[0];
        assert!(b.p.p.length() < 1e-12);
        // keine doppelten Winkel (Speichen liegen auf den Raumdiagonalen)
        assert_eq!(b.angles.len(), 10, "{:?}", b.angles.iter().map(|a| a.2).collect::<Vec<_>>());
        assert!(b.angles.iter().all(|a| a.1.len() < 400), "Ausdruck zu lang");
        // Mittelschnitt des Antiprismas: Zehneck mit Umfang 5σ₁₀, unabhängig von h
        let z0 = secs.iter().find(|r| r.plane == 0 && r.obj.starts_with("Antiprisma")).unwrap();
        assert_eq!((z0.ngon, z0.perim_uni.as_str()), (10, "5σ₁₀"));
    }
}
