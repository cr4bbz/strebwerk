//! Polyeder: konvexe Hülle (numerisch, für die Kombinatorik), exakte Analyse
//! (Abstandsklassen, Flächen, Volumen, Kugeln, Diederwinkel) und Symmetrie.

use crate::algebra::*;
use glam::{DMat3, DVec3};
use std::collections::{BTreeMap, HashMap, HashSet};

pub type V3 = [Sym; 3];

pub fn v3_sub(r: &Ring, a: &V3, b: &V3) -> V3 {
    [
        r.ssub(&a[0], &b[0]),
        r.ssub(&a[1], &b[1]),
        r.ssub(&a[2], &b[2]),
    ]
}
pub fn v3_add(r: &Ring, a: &V3, b: &V3) -> V3 {
    [
        r.sadd(&a[0], &b[0]),
        r.sadd(&a[1], &b[1]),
        r.sadd(&a[2], &b[2]),
    ]
}
pub fn v3_dot(r: &Ring, a: &V3, b: &V3) -> Sym {
    let x = r.smul(&a[0], &b[0]);
    let y = r.smul(&a[1], &b[1]);
    let z = r.smul(&a[2], &b[2]);
    r.sadd(&r.sadd(&x, &y), &z)
}
pub fn v3_cross(r: &Ring, a: &V3, b: &V3) -> V3 {
    [
        r.ssub(&r.smul(&a[1], &b[2]), &r.smul(&a[2], &b[1])),
        r.ssub(&r.smul(&a[2], &b[0]), &r.smul(&a[0], &b[2])),
        r.ssub(&r.smul(&a[0], &b[1]), &r.smul(&a[1], &b[0])),
    ]
}
pub fn v3_scale(r: &Ring, a: &V3, k: &Sym) -> V3 {
    [r.smul(&a[0], k), r.smul(&a[1], k), r.smul(&a[2], k)]
}
pub fn v3_eval(r: &Ring, a: &V3) -> DVec3 {
    DVec3::new(r.seval(&a[0]), r.seval(&a[1]), r.seval(&a[2]))
}
pub fn v3_eq(r: &Ring, a: &V3, b: &V3) -> bool {
    (0..3).all(|i| r.seq(&a[i], &b[i]))
}

// ---------------------------------------------------------------------------
// Konvexe Hülle
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Hull {
    /// Flächen als Eckenliste, gegen den Uhrzeigersinn von außen gesehen.
    pub faces: Vec<Vec<usize>>,
    pub normals: Vec<DVec3>,
    pub offsets: Vec<f64>,
}

pub fn extent(pts: &[DVec3]) -> f64 {
    let mut lo = DVec3::splat(f64::MAX);
    let mut hi = DVec3::splat(f64::MIN);
    for p in pts {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    (hi - lo).max_element().max(1e-12)
}

/// Indizes der Punkte, die nach numerischem Zusammenfassen übrig bleiben.
pub fn dedupe(pts: &[DVec3]) -> Vec<usize> {
    let eps = 1e-9 * extent(pts);
    let mut keep: Vec<usize> = vec![];
    for (i, p) in pts.iter().enumerate() {
        if !keep.iter().any(|j| (pts[*j] - *p).length() < eps) {
            keep.push(i);
        }
    }
    keep
}

pub fn convex_hull(pts: &[DVec3]) -> Result<Hull, String> {
    let n = pts.len();
    if n < 4 {
        return Err("weniger als 4 Punkte – kein Körper".into());
    }
    let sc = extent(pts);
    let eps = 1e-9 * sc;
    let mut seen: HashSet<Vec<usize>> = HashSet::new();
    let mut faces = vec![];
    let mut normals = vec![];
    let mut offsets = vec![];
    for i in 0..n {
        for j in i + 1..n {
            for k in j + 1..n {
                let nrm = (pts[j] - pts[i]).cross(pts[k] - pts[i]);
                let len = nrm.length();
                if len < eps * sc {
                    continue;
                }
                let nu = nrm / len;
                let d = nu.dot(pts[i]);
                let mut pos = false;
                let mut neg = false;
                let mut on = vec![];
                for (m, p) in pts.iter().enumerate() {
                    let s = nu.dot(*p) - d;
                    if s > eps {
                        pos = true;
                    } else if s < -eps {
                        neg = true;
                    } else {
                        on.push(m);
                    }
                    if pos && neg {
                        break;
                    }
                }
                if pos && neg {
                    continue;
                }
                if !pos && !neg {
                    return Err("alle Punkte liegen in einer Ebene – kein Körper".into());
                }
                if seen.contains(&on) {
                    continue;
                }
                seen.insert(on.clone());
                let (nu, d) = if pos { (-nu, -d) } else { (nu, d) };
                // Ecken der Fläche ordnen
                let c = on.iter().map(|m| pts[*m]).sum::<DVec3>() / on.len() as f64;
                let u = (pts[on[0]] - c).normalize_or_zero();
                let w = nu.cross(u);
                let mut ord = on.clone();
                ord.sort_by(|a, b| {
                    let pa = pts[*a] - c;
                    let pb = pts[*b] - c;
                    let aa = pa.dot(w).atan2(pa.dot(u));
                    let ab = pb.dot(w).atan2(pb.dot(u));
                    aa.partial_cmp(&ab).unwrap()
                });
                // Punkte im Inneren einer Kante (kollinear) entfernen
                let m = ord.len();
                let ord: Vec<usize> = (0..m)
                    .filter(|t| {
                        let a = pts[ord[(t + m - 1) % m]];
                        let b = pts[ord[*t]];
                        let cc = pts[ord[(t + 1) % m]];
                        (b - a).cross(cc - b).length() > eps * sc
                    })
                    .map(|t| ord[t])
                    .collect();
                faces.push(ord);
                normals.push(nu);
                offsets.push(d);
            }
        }
    }
    if faces.len() < 4 {
        return Err("entartete Hülle".into());
    }
    Ok(Hull {
        faces,
        normals,
        offsets,
    })
}

// ---------------------------------------------------------------------------
// Analyse
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PairKind {
    Edge,
    FaceDiag,
    SpaceDiag,
    /// Gerüst ohne Hülle: frei gewählte Strebe
    Strut,
    /// Gerüst ohne Hülle: übriges Punktpaar
    Pair,
}

impl PairKind {
    pub fn name(&self) -> &'static str {
        match self {
            PairKind::Edge => "Kante",
            PairKind::FaceDiag => "Flächendiagonale",
            PairKind::SpaceDiag => "Raumdiagonale",
            PairKind::Strut => "Strebe",
            PairKind::Pair => "Punktabstand",
        }
    }
    pub fn tex(&self) -> &'static str {
        match self {
            PairKind::Edge => "\\ell",
            PairKind::FaceDiag => "f",
            PairKind::SpaceDiag => "d",
            PairKind::Strut => "s",
            PairKind::Pair => "p",
        }
    }
}

#[derive(Clone, Debug)]
pub struct DistClass {
    pub kind: PairKind,
    pub sub: usize,
    pub d2: Sym,
    pub rad: Radical,
    pub uni: String,
    pub tex: String,
    pub value: f64,
    pub pairs: Vec<(usize, usize)>,
}

/// Quelle eines Zahlenwerts, damit er bei neuen Parameterwerten ohne
/// erneute exakte Rechnung aktualisiert werden kann.
#[derive(Clone, Debug)]
pub enum ValSrc {
    Rad(Radical),
    Sum(Vec<Radical>),
    Sym(Sym),
    /// Winkel in Grad aus cos θ
    AcosDeg(Radical),
}

impl ValSrc {
    pub fn eval(&self, r: &Ring) -> f64 {
        match self {
            ValSrc::Rad(x) => r.rad_eval(x),
            ValSrc::Sum(xs) => xs.iter().map(|x| r.rad_eval(x)).sum(),
            ValSrc::Sym(x) => r.seval(x),
            ValSrc::AcosDeg(x) => r.rad_eval(x).clamp(-1.0, 1.0).acos().to_degrees(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct FLine {
    pub label: String,
    pub uni: String,
    pub tex: String,
    pub num: Option<f64>,
    pub unit: String,
    pub val: Option<ValSrc>,
    /// Index einer Abstandsklasse (für Farbfeld und Sichtbarkeit)
    pub class: Option<usize>,
}

#[derive(Clone, Debug, Default)]
pub struct Analysis {
    pub classes: Vec<DistClass>,
    pub edges: Vec<(usize, usize)>,
    pub lines: Vec<FLine>,
    pub warnings: Vec<String>,
}

fn kind_order(k: PairKind) -> u8 {
    k as u8
}

fn polygon_name(n: usize) -> String {
    match n {
        3 => "Dreieck".into(),
        4 => "Viereck".into(),
        5 => "Fünfeck".into(),
        6 => "Sechseck".into(),
        8 => "Achteck".into(),
        10 => "Zehneck".into(),
        12 => "Zwölfeck".into(),
        _ => format!("{n}-Eck"),
    }
}

fn subscript_n(n: usize) -> String {
    n.to_string()
        .chars()
        .map(|c| char::from_u32(0x2080 + c.to_digit(10).unwrap()).unwrap())
        .collect()
}

/// Exakte Klassifikation von Punktpaaren nach (Art, Quadrat des Abstands).
pub fn classify_pairs(
    r: &Ring,
    ex: &[V3],
    kinds: &dyn Fn(usize, usize) -> Option<PairKind>,
) -> Vec<DistClass> {
    let n = ex.len();
    let mut buckets: BTreeMap<(u8, i64), Vec<usize>> = BTreeMap::new();
    let mut classes: Vec<DistClass> = vec![];
    for i in 0..n {
        for j in i + 1..n {
            let Some(kind) = kinds(i, j) else { continue };
            let d = v3_sub(r, &ex[i], &ex[j]);
            let d2 = v3_dot(r, &d, &d);
            let val = r.seval(&d2);
            let key = (kind_order(kind), (val * 1e7).round() as i64);
            let mut found = None;
            for dk in [-1i64, 0, 1] {
                if let Some(list) = buckets.get(&(key.0, key.1 + dk)) {
                    for ci in list {
                        if classes[*ci].kind == kind && r.seq(&classes[*ci].d2, &d2) {
                            found = Some(*ci);
                        }
                    }
                }
            }
            match found {
                Some(ci) => classes[ci].pairs.push((i, j)),
                None => {
                    let rad = r.sqrt(&d2);
                    let value = val.max(0.0).sqrt();
                    classes.push(DistClass {
                        kind,
                        sub: 0,
                        uni: r.fmt_rad(&rad, false),
                        tex: r.fmt_rad(&rad, true),
                        d2,
                        rad,
                        value,
                        pairs: vec![(i, j)],
                    });
                    buckets.entry(key).or_default().push(classes.len() - 1);
                }
            }
        }
    }
    classes.sort_by(|a, b| {
        kind_order(a.kind)
            .cmp(&kind_order(b.kind))
            .then(a.value.partial_cmp(&b.value).unwrap())
    });
    let mut cnt: HashMap<PairKind, usize> = HashMap::new();
    for c in classes.iter_mut() {
        let e = cnt.entry(c.kind).or_insert(0);
        *e += 1;
        c.sub = *e;
    }
    classes
}

fn class_label(c: &DistClass, total_of_kind: usize) -> (String, String) {
    let base = c.kind.name();
    if total_of_kind > 1 {
        (
            format!(
                "{base} {}{} ({}×)",
                c.kind.tex().replace("\\ell", "ℓ"),
                subscript_n(c.sub),
                c.pairs.len()
            ),
            format!("{}_{{{}}}", c.kind.tex(), c.sub),
        )
    } else {
        (
            format!("{base} ({}×)", c.pairs.len()),
            c.kind.tex().to_string(),
        )
    }
}

pub fn class_lines(classes: &[DistClass]) -> Vec<FLine> {
    let mut out = vec![];
    for (ci, c) in classes.iter().enumerate() {
        let total = classes.iter().filter(|d| d.kind == c.kind).count();
        let (label, texsym) = class_label(c, total);
        out.push(FLine {
            label,
            uni: c.uni.clone(),
            tex: format!("{texsym} &= {}", c.tex),
            num: Some(c.value),
            val: Some(ValSrc::Rad(c.rad.clone())),
            class: Some(ci),
            ..Default::default()
        });
    }
    out
}

pub fn analyze_polyhedron(r: &Ring, ex: &[V3], num: &[DVec3], hull: &Hull) -> Analysis {
    let mut a = Analysis::default();
    let nv = ex.len();
    // Kanten und Inzidenzen
    let mut edge_faces: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
    let mut in_face: HashSet<(usize, usize)> = HashSet::new();
    for (fi, f) in hull.faces.iter().enumerate() {
        let m = f.len();
        for t in 0..m {
            let (p, q) = (f[t], f[(t + 1) % m]);
            edge_faces.entry((p.min(q), p.max(q))).or_default().push(fi);
        }
        for s in 0..m {
            for t in s + 1..m {
                in_face.insert((f[s].min(f[t]), f[s].max(f[t])));
            }
        }
    }
    a.edges = edge_faces.keys().cloned().collect();
    let edge_set: HashSet<(usize, usize)> = a.edges.iter().cloned().collect();
    let used: HashSet<usize> = hull.faces.iter().flatten().cloned().collect();
    let kinds = |i: usize, j: usize| -> Option<PairKind> {
        if !used.contains(&i) || !used.contains(&j) {
            return None;
        }
        if edge_set.contains(&(i, j)) {
            Some(PairKind::Edge)
        } else if in_face.contains(&(i, j)) {
            Some(PairKind::FaceDiag)
        } else {
            Some(PairKind::SpaceDiag)
        }
    };
    a.classes = classify_pairs(r, ex, &kinds);

    let v = used.len();
    let e = a.edges.len();
    let f = hull.faces.len();
    let chi = v as i64 - e as i64 + f as i64;
    a.lines.push(FLine {
        label: "Euler".into(),
        uni: format!("V − E + F = {v} − {e} + {f} = {chi}"),
        tex: format!("V - E + F &= {v} - {e} + {f} = {chi}"),
        ..Default::default()
    });
    if v < nv {
        a.warnings.push(format!(
            "{} Punkt(e) liegen nicht auf dem Rand der Hülle und wurden ignoriert",
            nv - v
        ));
    }

    // Flächen
    let mut normals_ex: Vec<V3> = vec![];
    let mut face_classes: Vec<(usize, Sym, Vec<usize>)> = vec![];
    for (fi, fc) in hull.faces.iter().enumerate() {
        let p0 = &ex[fc[0]];
        let nrm = v3_cross(r, &v3_sub(r, &ex[fc[1]], p0), &v3_sub(r, &ex[fc[2]], p0));
        for extra in fc.iter().skip(3) {
            let d = v3_dot(r, &v3_sub(r, &ex[*extra], p0), &nrm);
            if !r.sis_zero(&d) {
                a.warnings.push(format!("Fläche {fi} ist nur numerisch eben (exakt nicht koplanar) – Formeln dazu sind Näherungen"));
                break;
            }
        }
        normals_ex.push(nrm);
        // 2A·n̂ = Σ pᵢ × pᵢ₊₁
        let m = fc.len();
        let mut s: V3 = [
            r.sym(Poly::zero()),
            r.sym(Poly::zero()),
            r.sym(Poly::zero()),
        ];
        for t in 0..m {
            s = v3_add(r, &s, &v3_cross(r, &ex[fc[t]], &ex[fc[(t + 1) % m]]));
        }
        let a4 = v3_dot(r, &s, &s); // = 4A²
        let area2 = r.sscale(&a4, qr(1, 4));
        if let Some(c) = face_classes
            .iter_mut()
            .find(|(n, ar, _)| *n == m && r.seq(ar, &area2))
        {
            c.2.push(fi);
        } else {
            face_classes.push((m, area2, vec![fi]));
        }
    }
    face_classes.sort_by_key(|c| c.0);
    let mut shape_counts: BTreeMap<usize, usize> = BTreeMap::new();
    for fc in &hull.faces {
        *shape_counts.entry(fc.len()).or_default() += 1;
    }
    let shapes: Vec<String> = shape_counts
        .iter()
        .map(|(n, c)| format!("{c} × {}", polygon_name(*n)))
        .collect();
    a.lines.push(FLine {
        label: "Flächen".into(),
        uni: shapes.join(", "),
        ..Default::default()
    });

    a.lines.extend(class_lines(&a.classes));

    let mut surf_items = vec![];
    for (k, (m, area2, list)) in face_classes.iter().enumerate() {
        let rad = r.sqrt(area2);
        let label = if face_classes.len() > 1 {
            format!(
                "Flächeninhalt A{} ({}, {}×)",
                subscript_n(k + 1),
                polygon_name(*m),
                list.len()
            )
        } else {
            format!("Flächeninhalt A ({}, {}×)", polygon_name(*m), list.len())
        };
        let texl = if face_classes.len() > 1 {
            format!("A_{{{}}}", k + 1)
        } else {
            "A".into()
        };
        a.lines.push(FLine {
            label,
            uni: r.fmt_rad(&rad, false),
            tex: format!("{texl} &= {}", r.fmt_rad(&rad, true)),
            num: Some(r.rad_eval(&rad)),
            val: Some(ValSrc::Rad(rad.clone())),
            ..Default::default()
        });
        surf_items.push((qi(list.len() as i64), rad));
    }
    let surf = r.rad_sum(&surf_items);
    let surf_num: f64 = surf.iter().map(|x| r.rad_eval(x)).sum();
    a.lines.push(FLine {
        label: "Oberfläche O".into(),
        uni: r.fmt_rad_sum(&surf, false),
        tex: format!("O &= {}", r.fmt_rad_sum(&surf, true)),
        num: Some(surf_num),
        val: Some(ValSrc::Sum(surf.clone())),
        ..Default::default()
    });

    // Volumen: Kegel von Ecke 0 über triangulierte Flächen
    let c0 = &ex[hull.faces[0][0]];
    let mut vol = r.sym(Poly::zero());
    for fc in &hull.faces {
        for t in 1..fc.len() - 1 {
            let x = v3_sub(r, &ex[fc[0]], c0);
            let y = v3_sub(r, &ex[fc[t]], c0);
            let z = v3_sub(r, &ex[fc[t + 1]], c0);
            vol = r.sadd(&vol, &v3_dot(r, &x, &v3_cross(r, &y, &z)));
        }
    }
    let mut vol = r.sscale(&vol, qr(1, 6));
    if r.seval(&vol) < 0.0 {
        vol = r.sneg(&vol);
    }
    a.lines.push(FLine {
        label: "Volumen V".into(),
        uni: r.fmt_sym(&vol, false),
        tex: format!("\\mathrm{{Vol}} &= {}", r.fmt_sym(&vol, true)),
        num: Some(r.seval(&vol)),
        val: Some(ValSrc::Sym(vol.clone())),
        ..Default::default()
    });

    // Schwerpunkt der Ecken und Kugeln
    let used_v: Vec<usize> = {
        let mut u: Vec<usize> = used.iter().cloned().collect();
        u.sort();
        u
    };
    let mut cen: V3 = [
        r.sym(Poly::zero()),
        r.sym(Poly::zero()),
        r.sym(Poly::zero()),
    ];
    for i in &used_v {
        cen = v3_add(r, &cen, &ex[*i]);
    }
    let cen = v3_scale(r, &cen, &r.sym_c(qr(1, used_v.len() as i64)));
    let all_equal = |vals: &[Sym]| vals.windows(2).all(|w| r.seq(&w[0], &w[1]));
    let mut push_sphere = |name: &str, texn: &str, vals: Vec<Sym>| {
        if !vals.is_empty() && all_equal(&vals) {
            let rad = r.sqrt(&vals[0]);
            a.lines.push(FLine {
                label: name.into(),
                uni: r.fmt_rad(&rad, false),
                tex: format!("{texn} &= {}", r.fmt_rad(&rad, true)),
                num: Some(r.rad_eval(&rad)),
                val: Some(ValSrc::Rad(rad.clone())),
                ..Default::default()
            });
            true
        } else {
            false
        }
    };
    let rv: Vec<Sym> = used_v
        .iter()
        .map(|i| {
            let d = v3_sub(r, &ex[*i], &cen);
            v3_dot(r, &d, &d)
        })
        .collect();
    let has_r = push_sphere("Umkugelradius R", "R", rv);
    let rm: Vec<Sym> = a
        .edges
        .iter()
        .map(|(p, q)| {
            let ab = v3_sub(r, &ex[*q], &ex[*p]);
            let ac = v3_sub(r, &cen, &ex[*p]);
            let cr = v3_cross(r, &ac, &ab);
            r.sdiv(&v3_dot(r, &cr, &cr), &v3_dot(r, &ab, &ab))
                .unwrap_or_else(|_| r.sym(Poly::zero()))
        })
        .collect();
    let has_m = push_sphere("Kantenkugelradius ρₘ", "\\rho_m", rm);
    let ri: Vec<Sym> = hull
        .faces
        .iter()
        .zip(&normals_ex)
        .map(|(fc, n)| {
            let d = v3_dot(r, &v3_sub(r, &ex[fc[0]], &cen), n);
            r.sdiv(&r.smul(&d, &d), &v3_dot(r, n, n))
                .unwrap_or_else(|_| r.sym(Poly::zero()))
        })
        .collect();
    let has_i = push_sphere("Inkugelradius r", "r", ri);
    if !(has_r || has_m || has_i) {
        a.lines.push(FLine {
            label: "Kugeln".into(),
            uni: "keine Um-, Kanten- oder Inkugel um den Eckenschwerpunkt".into(),
            ..Default::default()
        });
    }

    // Diederwinkel
    let mut dihed: Vec<(Sym, bool, usize, f64)> = vec![];
    for (_, fs) in edge_faces.iter() {
        if fs.len() != 2 {
            continue;
        }
        let (n1, n2) = (&normals_ex[fs[0]], &normals_ex[fs[1]]);
        let dot = v3_dot(r, n1, n2);
        let c2 = r
            .sdiv(
                &r.smul(&dot, &dot),
                &r.smul(&v3_dot(r, n1, n1), &v3_dot(r, n2, n2)),
            )
            .unwrap_or_else(|_| r.sym(Poly::zero()));
        let neg = r.seval(&dot) > 0.0; // cos θ = −n₁·n₂/(|n₁||n₂|)
        let ang = {
            let nn1 = v3_eval(r, n1);
            let nn2 = v3_eval(r, n2);
            std::f64::consts::PI - nn1.angle_between(nn2)
        };
        if let Some(d) = dihed.iter_mut().find(|d| d.1 == neg && r.seq(&d.0, &c2)) {
            d.2 += 1;
        } else {
            dihed.push((c2, neg, 1, ang));
        }
    }
    dihed.sort_by(|x, y| x.3.partial_cmp(&y.3).unwrap());
    for (k, (c2, neg, count, ang)) in dihed.iter().enumerate() {
        let mut rad = r.sqrt(c2);
        if *neg {
            rad.outside = r.sneg(&rad.outside);
        }
        let label = if dihed.len() > 1 {
            format!("Diederwinkel θ{} ({count} Kanten)", subscript_n(k + 1))
        } else {
            format!("Diederwinkel θ ({count} Kanten)")
        };
        let texl = if dihed.len() > 1 {
            format!("\\cos\\theta_{{{}}}", k + 1)
        } else {
            "\\cos\\theta".into()
        };
        let rs = if r.sis_zero(c2) {
            "0".to_string()
        } else {
            r.fmt_rad(&rad, false)
        };
        let rt = if r.sis_zero(c2) {
            "0".to_string()
        } else {
            r.fmt_rad(&rad, true)
        };
        a.lines.push(FLine {
            label,
            uni: format!("cos θ = {rs}"),
            tex: format!(
                "{texl} &= {rt} \\quad (\\theta \\approx {:.4}^\\circ)",
                ang.to_degrees()
            ),
            num: Some(ang.to_degrees()),
            unit: "°".into(),
            val: if r.sis_zero(c2) {
                None
            } else {
                Some(ValSrc::AcosDeg(rad.clone()))
            },
            ..Default::default()
        });
    }
    let _ = num;
    a
}

/// Gerüst ohne Hülle: frei gewählte Streben und alle übrigen Punktpaare.
pub fn analyze_framework(r: &Ring, ex: &[V3], struts: &[(usize, usize)]) -> Analysis {
    let set: HashSet<(usize, usize)> = struts.iter().map(|(a, b)| (*a.min(b), *a.max(b))).collect();
    let kinds = |i: usize, j: usize| {
        Some(if set.contains(&(i, j)) {
            PairKind::Strut
        } else {
            PairKind::Pair
        })
    };
    let mut a = Analysis {
        classes: classify_pairs(r, ex, &kinds),
        ..Default::default()
    };
    a.edges = set.into_iter().collect();
    a.lines.push(FLine {
        label: "Gerüst".into(),
        uni: format!("{} Knoten, {} Streben", ex.len(), a.edges.len()),
        ..Default::default()
    });
    // Maxwell-Zählung: Ein Stabwerk mit n Knoten braucht mindestens 3n − 6 Stäbe für Starrheit.
    let n = ex.len() as i64;
    if n >= 3 {
        let need = 3 * n - 6;
        a.lines.push(FLine {
            label: "Maxwell-Zählung".into(),
            uni: format!(
                "3n − 6 = {need}, vorhanden {} → {}",
                a.edges.len(),
                if a.edges.len() as i64 >= need {
                    "notwendige Bedingung erfüllt"
                } else {
                    "sicher nicht starr"
                }
            ),
            ..Default::default()
        });
    }
    a.lines.extend(class_lines(&a.classes));
    a
}

// ---------------------------------------------------------------------------
// Symmetrie
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
pub struct SymInfo {
    pub order: usize,
    pub rot_order: usize,
    pub name_uni: String,
    pub name_tex: String,
    pub note: String,
    pub axes: Vec<(DVec3, u32)>,
    pub mirrors: Vec<DVec3>,
    pub inversion: bool,
}

pub fn symmetry(pts: &[DVec3], center: DVec3) -> SymInfo {
    let q: Vec<DVec3> = pts.iter().map(|p| *p - center).collect();
    let sc = q.iter().fold(0.0f64, |m, p| m.max(p.length())).max(1e-12);
    let eps = 1e-6 * sc;
    let Some(p0) = q
        .iter()
        .cloned()
        .max_by(|a, b| a.length().partial_cmp(&b.length()).unwrap())
    else {
        return SymInfo::default();
    };
    let p1 = q
        .iter()
        .cloned()
        .max_by(|a, b| {
            p0.cross(*a)
                .length()
                .partial_cmp(&p0.cross(*b).length())
                .unwrap()
        })
        .unwrap();
    if p0.cross(p1).length() < eps * sc {
        return SymInfo {
            order: 0,
            name_uni: "kollinear".into(),
            ..Default::default()
        };
    }
    let b = DMat3::from_cols(p0, p1, p0.cross(p1));
    let binv = b.inverse();
    let maps_onto = |a: &DMat3| -> bool {
        q.iter().all(|p| {
            let ap = *a * *p;
            q.iter().any(|t| (*t - ap).length() < 10.0 * eps)
        })
    };
    let mut group: Vec<DMat3> = vec![];
    for q0 in &q {
        if (q0.length() - p0.length()).abs() > eps {
            continue;
        }
        for q1 in &q {
            if (q1.length() - p1.length()).abs() > eps
                || (q0.dot(*q1) - p0.dot(p1)).abs() > eps * sc
            {
                continue;
            }
            for s in [1.0, -1.0] {
                let a = DMat3::from_cols(*q0, *q1, q0.cross(*q1) * s) * binv;
                let ata = a.transpose() * a;
                if !(ata - DMat3::IDENTITY)
                    .to_cols_array()
                    .iter()
                    .all(|x| x.abs() < 1e-6)
                {
                    continue;
                }
                if group
                    .iter()
                    .any(|g| (*g - a).to_cols_array().iter().all(|x| x.abs() < 1e-6))
                {
                    continue;
                }
                if maps_onto(&a) {
                    group.push(a);
                }
            }
        }
    }
    let mut info = SymInfo {
        order: group.len(),
        ..Default::default()
    };
    let mut axes: Vec<(DVec3, f64)> = vec![];
    for a in &group {
        let det = a.determinant();
        let tr = a.x_axis.x + a.y_axis.y + a.z_axis.z;
        if det > 0.0 {
            info.rot_order += 1;
            if (tr - 3.0).abs() < 1e-6 {
                continue;
            }
            let ang = ((tr - 1.0) / 2.0).clamp(-1.0, 1.0).acos();
            let axis = if (ang - std::f64::consts::PI).abs() < 1e-4 {
                let m = *a + DMat3::IDENTITY;
                [m.x_axis, m.y_axis, m.z_axis]
                    .into_iter()
                    .max_by(|x, y| x.length().partial_cmp(&y.length()).unwrap())
                    .unwrap()
            } else {
                // Spalten-Major: a.col(j).row(i) = a_ij
                DVec3::new(
                    a.y_axis.z - a.z_axis.y,
                    a.z_axis.x - a.x_axis.z,
                    a.x_axis.y - a.y_axis.x,
                )
            }
            .normalize();
            if let Some(ax) = axes
                .iter_mut()
                .find(|(d, _)| d.dot(axis).abs() > 1.0 - 1e-6)
            {
                ax.1 = ax.1.min(ang);
            } else {
                axes.push((axis, ang));
            }
        } else {
            if (tr + 3.0).abs() < 1e-6 {
                info.inversion = true;
            } else if (tr - 1.0).abs() < 1e-6 {
                let m = *a - DMat3::IDENTITY;
                let nrm = [m.x_axis, m.y_axis, m.z_axis]
                    .into_iter()
                    .max_by(|x, y| x.length().partial_cmp(&y.length()).unwrap())
                    .unwrap();
                info.mirrors.push(nrm.normalize());
            }
        }
    }
    info.axes = axes
        .iter()
        .map(|(d, ang)| (*d, (2.0 * std::f64::consts::PI / ang).round() as u32))
        .collect();
    info.axes.sort_by(|a, b| b.1.cmp(&a.1));
    name_group(&mut info);
    info
}

fn name_group(info: &mut SymInfo) {
    let cnt = |n: u32| info.axes.iter().filter(|a| a.1 == n).count();
    let g = info.order;
    let ro = info.rot_order;
    let main = info.axes.first().cloned();
    let sub = |s: &str| -> String {
        s.chars()
            .map(|c| match c.to_digit(10) {
                Some(d) => char::from_u32(0x2080 + d).unwrap(),
                None if c == 'h' => 'ₕ',
                None if c == 'v' => 'ᵥ',
                None => c,
            })
            .collect()
    };
    let (base, suffix, note): (String, String, String) = if cnt(5) > 0 {
        (
            "I".into(),
            if g == 120 { "h".into() } else { String::new() },
            "Drehgruppe I ≅ A₅".into(),
        )
    } else if cnt(4) >= 3 {
        (
            "O".into(),
            if g == 48 { "h".into() } else { String::new() },
            "Drehgruppe O ≅ S₄".into(),
        )
    } else if cnt(3) >= 4 {
        let suf = if g == 24 {
            if info.inversion {
                "h"
            } else {
                "d"
            }
        } else {
            ""
        };
        ("T".into(), suf.into(), "Drehgruppe T ≅ A₄".into())
    } else {
        let n = main.map(|m| m.1).unwrap_or(1);
        let dihedral = ro == 2 * n as usize && n >= 2;
        let axis = main.map(|m| m.0).unwrap_or(DVec3::Z);
        let horiz = info.mirrors.iter().any(|m| m.dot(axis).abs() > 1.0 - 1e-6);
        if dihedral {
            let suf = if g == 4 * n as usize {
                if horiz {
                    "h"
                } else {
                    "d"
                }
            } else {
                ""
            };
            (
                format!("D{n}"),
                suf.into(),
                format!("Drehgruppe D{} (Ordnung {})", sub(&n.to_string()), 2 * n),
            )
        } else if g == ro {
            (
                format!("C{n}"),
                String::new(),
                format!("zyklisch, Ordnung {n}"),
            )
        } else if n == 1 {
            if info.inversion {
                ("Cᵢ".into(), String::new(), "nur Punktspiegelung".into())
            } else {
                ("Cₛ".into(), String::new(), "nur eine Spiegelebene".into())
            }
        } else if horiz {
            (format!("C{n}"), "h".into(), String::new())
        } else if !info.mirrors.is_empty() {
            (format!("C{n}"), "v".into(), String::new())
        } else {
            (
                format!("S{}", 2 * n),
                String::new(),
                "Drehspiegelung".into(),
            )
        }
    };
    let letter: String = base.chars().take(1).collect();
    let digits: String = base.chars().skip(1).collect();
    info.name_uni = format!("{letter}{}", sub(&format!("{digits}{suffix}")));
    info.name_tex = if digits.is_empty() && suffix.is_empty() {
        letter.clone()
    } else {
        format!("{letter}_{{{digits}{suffix}}}")
    };
    info.note = note;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cube_symmetry() {
        let mut pts = vec![];
        for x in [-1.0, 1.0] {
            for y in [-1.0, 1.0] {
                for z in [-1.0, 1.0] {
                    pts.push(DVec3::new(x, y, z));
                }
            }
        }
        let s = symmetry(&pts, DVec3::ZERO);
        assert_eq!(s.order, 48);
        assert_eq!(s.rot_order, 24);
        assert_eq!(s.name_uni, "Oₕ");
        assert_eq!(s.mirrors.len(), 9);
        let h = convex_hull(&pts).unwrap();
        assert_eq!(h.faces.len(), 6);
        assert!(h.faces.iter().all(|f| f.len() == 4));
    }
}
