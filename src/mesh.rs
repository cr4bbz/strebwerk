//! Dreiecksnetze: Marching Tetrahedra, Parameterflächen, Topologie,
//! Schnitte, Gitter- und TPMS-Füllungen.

use crate::expr::NFn;
use glam::DVec3;
use std::collections::HashMap;

#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub v: Vec<DVec3>,
    pub t: Vec<[u32; 3]>,
}

impl Mesh {
    pub fn tri_normal(&self, i: usize) -> DVec3 {
        let [a, b, c] = self.t[i];
        let (a, b, c) = (self.v[a as usize], self.v[b as usize], self.v[c as usize]);
        (b - a).cross(c - a)
    }
    pub fn vertex_normals(&self) -> Vec<DVec3> {
        let mut n = vec![DVec3::ZERO; self.v.len()];
        for (i, t) in self.t.iter().enumerate() {
            let fn_ = self.tri_normal(i);
            for k in t {
                n[*k as usize] += fn_;
            }
        }
        n.into_iter().map(|x| x.normalize_or_zero()).collect()
    }
    pub fn area(&self) -> f64 {
        (0..self.t.len())
            .map(|i| 0.5 * self.tri_normal(i).length())
            .sum()
    }
    /// Orientiertes Volumen (Divergenzsatz); sinnvoll nur für geschlossene Netze.
    pub fn volume(&self) -> f64 {
        self.t
            .iter()
            .map(|[a, b, c]| {
                self.v[*a as usize].dot(self.v[*b as usize].cross(self.v[*c as usize])) / 6.0
            })
            .sum()
    }
    pub fn bbox(&self) -> (DVec3, DVec3) {
        let mut lo = DVec3::splat(f64::MAX);
        let mut hi = DVec3::splat(f64::MIN);
        for p in &self.v {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
        (lo, hi)
    }
}

// Würfelecken
const CORNERS: [[usize; 3]; 8] = [
    [0, 0, 0],
    [1, 0, 0],
    [1, 1, 0],
    [0, 1, 0],
    [0, 0, 1],
    [1, 0, 1],
    [1, 1, 1],
    [0, 1, 1],
];
// 6 Tetraeder um die Diagonale 0–6 (konsistent über Nachbarwürfel)
const TETS: [[usize; 4]; 6] = [
    [0, 5, 1, 6],
    [0, 1, 2, 6],
    [0, 2, 3, 6],
    [0, 3, 7, 6],
    [0, 7, 4, 6],
    [0, 4, 5, 6],
];

/// Nullstellenfläche f = 0 im Quader [lo, hi], Auflösung res Zellen pro Achse.
/// Normalen zeigen in Richtung wachsender f (aus {f < 0} heraus).
pub fn marching_tets(
    f: &dyn Fn(DVec3) -> f64,
    lo: DVec3,
    hi: DVec3,
    res: usize,
    keep: Option<&dyn Fn(DVec3) -> bool>,
) -> Mesh {
    let res = res.clamp(4, 160);
    let n = res + 1;
    let step = (hi - lo) / res as f64;
    let idx = |i: usize, j: usize, k: usize| (i + n * (j + n * k)) as u32;
    let mut val = vec![0.0f64; n * n * n];
    for k in 0..n {
        for j in 0..n {
            for i in 0..n {
                let p = lo + step * DVec3::new(i as f64, j as f64, k as f64);
                let mut v = f(p);
                if !v.is_finite() {
                    v = 1.0;
                }
                if v.abs() < 1e-12 {
                    v = 1e-12;
                }
                val[idx(i, j, k) as usize] = v;
            }
        }
    }
    let pos = |id: u32| -> DVec3 {
        let id = id as usize;
        let i = id % n;
        let j = (id / n) % n;
        let k = id / (n * n);
        lo + step * DVec3::new(i as f64, j as f64, k as f64)
    };
    let mut mesh = Mesh::default();
    let mut emap: HashMap<(u32, u32), u32> = HashMap::new();
    let mut vert = |a: u32, b: u32, mesh: &mut Mesh| -> u32 {
        let key = (a.min(b), a.max(b));
        if let Some(v) = emap.get(&key) {
            return *v;
        }
        let (fa, fb) = (val[a as usize], val[b as usize]);
        let t = fa / (fa - fb);
        let p = pos(a).lerp(pos(b), t);
        mesh.v.push(p);
        let id = (mesh.v.len() - 1) as u32;
        emap.insert(key, id);
        id
    };
    for k in 0..res {
        for j in 0..res {
            for i in 0..res {
                let c: Vec<u32> = CORNERS
                    .iter()
                    .map(|o| idx(i + o[0], j + o[1], k + o[2]))
                    .collect();
                let any_in = c.iter().any(|x| val[*x as usize] < 0.0);
                let any_out = c.iter().any(|x| val[*x as usize] >= 0.0);
                if !(any_in && any_out) {
                    continue;
                }
                for tet in TETS {
                    let g: Vec<u32> = tet.iter().map(|x| c[*x]).collect();
                    let ins: Vec<u32> = g
                        .iter()
                        .cloned()
                        .filter(|x| val[*x as usize] < 0.0)
                        .collect();
                    let outs: Vec<u32> = g
                        .iter()
                        .cloned()
                        .filter(|x| val[*x as usize] >= 0.0)
                        .collect();
                    let tris: Vec<[u32; 3]> = match ins.len() {
                        1 => vec![[
                            vert(ins[0], outs[0], &mut mesh),
                            vert(ins[0], outs[1], &mut mesh),
                            vert(ins[0], outs[2], &mut mesh),
                        ]],
                        3 => vec![[
                            vert(outs[0], ins[0], &mut mesh),
                            vert(outs[0], ins[1], &mut mesh),
                            vert(outs[0], ins[2], &mut mesh),
                        ]],
                        2 => {
                            let a = vert(ins[0], outs[0], &mut mesh);
                            let b = vert(ins[0], outs[1], &mut mesh);
                            let cc = vert(ins[1], outs[1], &mut mesh);
                            let d = vert(ins[1], outs[0], &mut mesh);
                            vec![[a, b, cc], [a, cc, d]]
                        }
                        _ => vec![],
                    };
                    let cin = ins.iter().map(|x| pos(*x)).sum::<DVec3>() / ins.len().max(1) as f64;
                    let cout =
                        outs.iter().map(|x| pos(*x)).sum::<DVec3>() / outs.len().max(1) as f64;
                    let dir = cout - cin;
                    for mut t in tris {
                        let (a, b, cc) = (
                            mesh.v[t[0] as usize],
                            mesh.v[t[1] as usize],
                            mesh.v[t[2] as usize],
                        );
                        if (b - a).cross(cc - a).dot(dir) < 0.0 {
                            t.swap(1, 2);
                        }
                        if let Some(kp) = keep {
                            if !kp((a + b + cc) / 3.0) {
                                continue;
                            }
                        }
                        mesh.t.push(t);
                    }
                }
            }
        }
    }
    mesh
}

/// Parameterfläche r(u,v) auf [u0,u1]×[v0,v1]; liefert das (verschweißte) Netz und das Rohgitter.
pub fn param_surface(
    r: &dyn Fn(f64, f64) -> DVec3,
    u0: f64,
    u1: f64,
    v0: f64,
    v1: f64,
    nu: usize,
    nv: usize,
) -> (Mesh, Vec<Vec<DVec3>>) {
    let nu = nu.clamp(2, 600);
    let nv = nv.clamp(2, 600);
    let mut grid = vec![vec![DVec3::ZERO; nv + 1]; nu + 1];
    for (i, row) in grid.iter_mut().enumerate() {
        for (j, p) in row.iter_mut().enumerate() {
            let u = u0 + (u1 - u0) * i as f64 / nu as f64;
            let v = v0 + (v1 - v0) * j as f64 / nv as f64;
            let q = r(u, v);
            *p = if q.is_finite() { q } else { DVec3::ZERO };
        }
    }
    let id = |i: usize, j: usize| i * (nv + 1) + j;
    // Nähte erkennen: nur Randpunkte werden identifiziert, und nur, wenn auch die
    // Nachbarn entlang der Naht übereinstimmen. So bleiben Selbstdurchdringungen
    // (z. B. Achter-Kleinflasche) getrennt, während echte Nähte und Pole verschweißt werden.
    let (lo, hi) = grid.iter().flatten().fold(
        (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN)),
        |(l, h), p| (l.min(*p), h.max(*p)),
    );
    let eps = 1e-7 * (hi - lo).max_element().max(1e-9);
    let close = |a: DVec3, b: DVec3| (a - b).length() < eps;
    let mut dsu = Dsu::new((nu + 1) * (nv + 1));
    let col = |i: usize| -> Vec<usize> { (0..=nv).map(|j| id(i, j)).collect() };
    let row = |j: usize| -> Vec<usize> { (0..=nu).map(|i| id(i, j)).collect() };
    let flat: Vec<DVec3> = grid.iter().flatten().cloned().collect();
    let lines = [col(0), col(nu), row(0), row(nv)];
    for line in &lines {
        if line.iter().all(|k| close(flat[*k], flat[line[0]])) {
            for k in line {
                dsu.union(*k, line[0]);
            }
        }
    }
    for (a, b) in [(&lines[0], &lines[1]), (&lines[2], &lines[3])] {
        let nb = |l: &Vec<usize>, k: usize| -> Vec<DVec3> {
            let mut v = vec![];
            if k > 0 {
                v.push(flat[l[k - 1]]);
            }
            if k + 1 < l.len() {
                v.push(flat[l[k + 1]]);
            }
            v
        };
        for ka in 0..a.len() {
            let pa = flat[a[ka]];
            let na = nb(a, ka);
            for kb in 0..b.len() {
                if !close(pa, flat[b[kb]]) {
                    continue;
                }
                let nbb = nb(b, kb);
                let ok = na.iter().all(|x| nbb.iter().any(|y| close(*x, *y)))
                    || nbb.iter().all(|y| na.iter().any(|x| close(*x, *y)));
                if ok {
                    dsu.union(a[ka], b[kb]);
                    break;
                }
            }
        }
    }
    let mut remap = vec![u32::MAX; flat.len()];
    let mut m = Mesh::default();
    for k in 0..flat.len() {
        let r = dsu.find(k);
        if remap[r] == u32::MAX {
            m.v.push(flat[r]);
            remap[r] = (m.v.len() - 1) as u32;
        }
        remap[k] = remap[r];
    }
    for i in 0..nu {
        for j in 0..nv {
            for t in [
                [id(i, j), id(i + 1, j), id(i + 1, j + 1)],
                [id(i, j), id(i + 1, j + 1), id(i, j + 1)],
            ] {
                let t2 = [remap[t[0]], remap[t[1]], remap[t[2]]];
                if t2[0] != t2[1] && t2[1] != t2[2] && t2[0] != t2[2] {
                    m.t.push(t2);
                }
            }
        }
    }
    (m, grid)
}

/// Verschweißt Ecken gleicher Position und entfernt entartete Dreiecke.
pub fn weld(m: &Mesh) -> Mesh {
    let (lo, hi) = m.bbox();
    let sc = (hi - lo).max_element().max(1e-9);
    let q = 1e-7 * sc;
    let mut map: HashMap<(i64, i64, i64), u32> = HashMap::new();
    let mut out = Mesh::default();
    let mut remap = vec![0u32; m.v.len()];
    for (i, p) in m.v.iter().enumerate() {
        let key = (
            (p.x / q).round() as i64,
            (p.y / q).round() as i64,
            (p.z / q).round() as i64,
        );
        let id = *map.entry(key).or_insert_with(|| {
            out.v.push(*p);
            (out.v.len() - 1) as u32
        });
        remap[i] = id;
    }
    for t in &m.t {
        let t2 = [
            remap[t[0] as usize],
            remap[t[1] as usize],
            remap[t[2] as usize],
        ];
        if t2[0] != t2[1] && t2[1] != t2[2] && t2[0] != t2[2] {
            out.t.push(t2);
        }
    }
    out
}

#[derive(Clone, Debug, Default)]
pub struct Topology {
    pub v: usize,
    pub e: usize,
    pub f: usize,
    pub chi: i64,
    pub boundary_loops: usize,
    pub nonmanifold_edges: usize,
    pub components: usize,
    pub orientable: bool,
}

struct Dsu(Vec<usize>);
impl Dsu {
    fn new(n: usize) -> Self {
        Dsu((0..n).collect())
    }
    fn find(&mut self, x: usize) -> usize {
        let mut r = x;
        while self.0[r] != r {
            r = self.0[r];
        }
        let mut y = x;
        while self.0[y] != r {
            let nx = self.0[y];
            self.0[y] = r;
            y = nx;
        }
        r
    }
    fn union(&mut self, a: usize, b: usize) {
        let (a, b) = (self.find(a), self.find(b));
        if a != b {
            self.0[a] = b;
        }
    }
}

pub fn topology(m: &Mesh) -> Topology {
    let mut edges: HashMap<(u32, u32), Vec<(usize, bool)>> = HashMap::new();
    let mut used = vec![false; m.v.len()];
    for (fi, t) in m.t.iter().enumerate() {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            used[a as usize] = true;
            edges
                .entry((a.min(b), a.max(b)))
                .or_default()
                .push((fi, a < b));
        }
    }
    let v = used.iter().filter(|x| **x).count();
    let e = edges.len();
    let f = m.t.len();
    let mut topo = Topology {
        v,
        e,
        f,
        chi: v as i64 - e as i64 + f as i64,
        orientable: true,
        ..Default::default()
    };
    // Randkomponenten
    let mut dsu = Dsu::new(m.v.len());
    let mut bverts = std::collections::HashSet::new();
    for ((a, b), fs) in &edges {
        match fs.len() {
            1 => {
                dsu.union(*a as usize, *b as usize);
                bverts.insert(*a as usize);
                bverts.insert(*b as usize);
            }
            2 => {}
            _ => topo.nonmanifold_edges += 1,
        }
    }
    let roots: std::collections::HashSet<usize> = bverts.iter().map(|x| dsu.find(*x)).collect();
    topo.boundary_loops = roots.len();
    // Zusammenhangskomponenten
    let mut dsu = Dsu::new(m.v.len());
    for t in &m.t {
        dsu.union(t[0] as usize, t[1] as usize);
        dsu.union(t[1] as usize, t[2] as usize);
    }
    let comps: std::collections::HashSet<usize> = (0..m.v.len())
        .filter(|i| used[*i])
        .map(|i| dsu.find(i))
        .collect();
    topo.components = comps.len();
    // Orientierbarkeit: Flächen per BFS konsistent orientieren
    let mut flip: Vec<Option<bool>> = vec![None; f];
    let mut adj: Vec<Vec<(usize, bool)>> = vec![vec![]; f];
    for fs in edges.values() {
        if fs.len() == 2 {
            let ((f1, d1), (f2, d2)) = (fs[0], fs[1]);
            // gleiche Laufrichtung ⇒ eine der beiden muss umgedreht werden
            let same = d1 == d2;
            adj[f1].push((f2, same));
            adj[f2].push((f1, same));
        }
    }
    for s in 0..f {
        if flip[s].is_some() {
            continue;
        }
        flip[s] = Some(false);
        let mut stack = vec![s];
        while let Some(x) = stack.pop() {
            let fx = flip[x].unwrap();
            for (y, same) in &adj[x] {
                let want = fx ^ *same;
                match flip[*y] {
                    None => {
                        flip[*y] = Some(want);
                        stack.push(*y);
                    }
                    Some(fy) => {
                        if fy != want {
                            topo.orientable = false;
                        }
                    }
                }
            }
        }
    }
    topo
}

pub fn classify(t: &Topology) -> String {
    if t.f == 0 {
        return "leer".into();
    }
    if t.nonmanifold_edges > 0 {
        return format!(
            "keine Mannigfaltigkeit ({} singuläre Kanten)",
            t.nonmanifold_edges
        );
    }
    if t.components != 1 {
        return format!(
            "{} Zusammenhangskomponenten, χ = {} gesamt",
            t.components, t.chi
        );
    }
    let b = t.boundary_loops as i64;
    let rest = 2 - t.chi - b;
    let name = if t.orientable {
        let g = rest / 2;
        let known = match (g, b) {
            (0, 0) => " – Sphäre S²",
            (1, 0) => " – Torus T²",
            (0, 1) => " – Kreisscheibe",
            (0, 2) => " – Kreisring",
            _ => "",
        };
        format!("orientierbar, Geschlecht g = {g}, {b} Randkurve(n){known}")
    } else {
        let known = match (rest, b) {
            (1, 0) => " – projektive Ebene ℝP²",
            (2, 0) => " – Kleinsche Flasche",
            (1, 1) => " – Möbiusband",
            _ => "",
        };
        format!("nicht orientierbar, {rest} Kreuzhaube(n), {b} Randkurve(n){known}")
    };
    format!("χ = {}  ·  {name}", t.chi)
}

/// Schnitt mit der Ebene n·x = d: Segmente und eingeschlossene Fläche (für geschlossene Netze).
pub fn slice(m: &Mesh, n: DVec3, d: f64) -> (Vec<(DVec3, DVec3)>, f64) {
    let mut segs = vec![];
    let mut area = 0.0;
    let o = n * d;
    for (i, t) in m.t.iter().enumerate() {
        let p: Vec<DVec3> = t.iter().map(|k| m.v[*k as usize]).collect();
        let s: Vec<f64> = p.iter().map(|x| n.dot(*x) - d).collect();
        let mut pts = vec![];
        for k in 0..3 {
            let (a, b) = (k, (k + 1) % 3);
            if (s[a] < 0.0) != (s[b] < 0.0) {
                let tt = s[a] / (s[a] - s[b]);
                pts.push(p[a].lerp(p[b], tt));
            }
        }
        if pts.len() == 2 {
            let nt = m.tri_normal(i);
            let dir = n.cross(nt);
            let (a, b) = if (pts[1] - pts[0]).dot(dir) >= 0.0 {
                (pts[0], pts[1])
            } else {
                (pts[1], pts[0])
            };
            area += 0.5 * (a - o).cross(b - o).dot(n);
            segs.push((a, b));
        }
    }
    (segs, area)
}

// ---------------------------------------------------------------------------
// Füllungen
// ---------------------------------------------------------------------------

pub enum Region<'a> {
    /// Halbräume n·x ≤ d
    Convex(Vec<(DVec3, f64)>),
    /// f < 0
    Implicit(&'a NFn),
}

impl Region<'_> {
    pub fn contains(&self, p: DVec3) -> bool {
        match self {
            Region::Convex(hs) => hs.iter().all(|(n, d)| n.dot(p) <= *d + 1e-12),
            Region::Implicit(f) => f(&[p.x, p.y, p.z]) < 0.0,
        }
    }
    pub fn clip(&self, a: DVec3, b: DVec3) -> Option<(DVec3, DVec3)> {
        match self {
            Region::Convex(hs) => {
                let (mut t0, mut t1) = (0.0f64, 1.0f64);
                let d = b - a;
                for (n, off) in hs {
                    let den = n.dot(d);
                    let num = off - n.dot(a);
                    if den.abs() < 1e-15 {
                        if num < 0.0 {
                            return None;
                        }
                    } else {
                        let t = num / den;
                        if den > 0.0 {
                            t1 = t1.min(t);
                        } else {
                            t0 = t0.max(t);
                        }
                    }
                    if t0 > t1 {
                        return None;
                    }
                }
                if t1 - t0 < 1e-9 {
                    return None;
                }
                Some((a + d * t0, a + d * t1))
            }
            Region::Implicit(_) => {
                let ia = self.contains(a);
                let ib = self.contains(b);
                match (ia, ib) {
                    (true, true) => Some((a, b)),
                    (false, false) => None,
                    _ => {
                        let (inside, outside) = if ia { (a, b) } else { (b, a) };
                        let (mut lo, mut hi) = (inside, outside);
                        for _ in 0..30 {
                            let mid = (lo + hi) * 0.5;
                            if self.contains(mid) {
                                lo = mid;
                            } else {
                                hi = mid;
                            }
                        }
                        Some((inside, lo))
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LatticeKind {
    Cubic,
    Bcc,
    Octet,
}

impl LatticeKind {
    pub fn name(&self) -> &'static str {
        match self {
            LatticeKind::Cubic => "kubisch",
            LatticeKind::Bcc => "kubisch-raumzentriert",
            LatticeKind::Octet => "Oktett-Fachwerk (kfz)",
        }
    }
}

pub fn lattice(
    region: &Region,
    lo: DVec3,
    hi: DVec3,
    spacing: f64,
    kind: LatticeKind,
) -> Vec<(DVec3, DVec3)> {
    let s = spacing.max((hi - lo).max_element() / 60.0).max(1e-6);
    let n = ((hi - lo) / s).ceil();
    let (nx, ny, nz) = (n.x as i64 + 1, n.y as i64 + 1, n.z as i64 + 1);
    let base = (lo + hi) * 0.5 - DVec3::new(nx as f64, ny as f64, nz as f64) * s * 0.5;
    let p = |i: f64, j: f64, k: f64| base + DVec3::new(i, j, k) * s;
    let mut segs = vec![];
    let add = |a: DVec3, b: DVec3, segs: &mut Vec<(DVec3, DVec3)>| {
        if segs.len() < 40_000 {
            if let Some(c) = region.clip(a, b) {
                segs.push(c);
            }
        }
    };
    for i in 0..=nx {
        for j in 0..=ny {
            for k in 0..=nz {
                let (fi, fj, fk) = (i as f64, j as f64, k as f64);
                let o = p(fi, fj, fk);
                match kind {
                    LatticeKind::Cubic => {
                        add(o, p(fi + 1.0, fj, fk), &mut segs);
                        add(o, p(fi, fj + 1.0, fk), &mut segs);
                        add(o, p(fi, fj, fk + 1.0), &mut segs);
                    }
                    LatticeKind::Bcc => {
                        let c = p(fi + 0.5, fj + 0.5, fk + 0.5);
                        for (a, b, cc) in [
                            (0., 0., 0.),
                            (1., 0., 0.),
                            (0., 1., 0.),
                            (0., 0., 1.),
                            (1., 1., 0.),
                            (1., 0., 1.),
                            (0., 1., 1.),
                            (1., 1., 1.),
                        ] {
                            add(c, p(fi + a, fj + b, fk + cc), &mut segs);
                        }
                    }
                    LatticeKind::Octet => {
                        // Punkte des kfz-Gitters in dieser Zelle: Ecke + 3 Flächenmitten
                        let pts = [
                            (0.0, 0.0, 0.0),
                            (0.5, 0.5, 0.0),
                            (0.5, 0.0, 0.5),
                            (0.0, 0.5, 0.5),
                        ];
                        for (a, b, cc) in pts {
                            let q = p(fi + a, fj + b, fk + cc);
                            // 6 der 12 nächsten Nachbarn (die andere Hälfte erzeugt der Nachbar)
                            for (x, y, z) in [
                                (0.5, 0.5, 0.0),
                                (0.5, -0.5, 0.0),
                                (0.5, 0.0, 0.5),
                                (0.5, 0.0, -0.5),
                                (0.0, 0.5, 0.5),
                                (0.0, 0.5, -0.5),
                            ] {
                                add(q, p(fi + a + x, fj + b + y, fk + cc + z), &mut segs);
                            }
                        }
                    }
                }
            }
        }
    }
    segs
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Tpms {
    Gyroid,
    SchwarzP,
    Diamond,
}

impl Tpms {
    pub fn name(&self) -> &'static str {
        match self {
            Tpms::Gyroid => "Gyroid",
            Tpms::SchwarzP => "Schwarz P",
            Tpms::Diamond => "Schwarz D (Diamant)",
        }
    }
    pub fn eval(&self, p: DVec3) -> f64 {
        let (x, y, z) = (p.x, p.y, p.z);
        match self {
            Tpms::Gyroid => x.sin() * y.cos() + y.sin() * z.cos() + z.sin() * x.cos(),
            Tpms::SchwarzP => x.cos() + y.cos() + z.cos(),
            Tpms::Diamond => {
                x.sin() * y.sin() * z.sin()
                    + x.sin() * y.cos() * z.cos()
                    + x.cos() * y.sin() * z.cos()
                    + x.cos() * y.cos() * z.sin()
            }
        }
    }
}

pub fn tpms_infill(region: &Region, lo: DVec3, hi: DVec3, period: f64, kind: Tpms) -> Mesh {
    let period = period.max((hi - lo).max_element() / 20.0);
    let k = 2.0 * std::f64::consts::PI / period;
    let res = (((hi - lo).max_element() / period) * 10.0).clamp(16.0, 110.0) as usize;
    let f = |p: DVec3| kind.eval(p * k);
    let keep = |p: DVec3| region.contains(p);
    marching_tets(&f, lo, hi, res, Some(&keep))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn torus_genus() {
        let f = |p: DVec3| {
            let q = (p.x * p.x + p.y * p.y).sqrt() - 1.0;
            q * q + p.z * p.z - 0.16
        };
        let m = marching_tets(&f, DVec3::splat(-1.6), DVec3::splat(1.6), 40, None);
        let t = topology(&m);
        assert_eq!(t.chi, 0);
        assert!(t.orientable);
        assert_eq!(t.boundary_loops, 0);
        let vol = m.volume();
        let exact = 2.0 * std::f64::consts::PI.powi(2) * 1.0 * 0.16;
        assert!((vol - exact).abs() / exact < 0.03, "{vol} vs {exact}");
    }

    #[test]
    fn sphere_param_and_slice() {
        let r = |u: f64, v: f64| DVec3::new(v.sin() * u.cos(), v.sin() * u.sin(), v.cos());
        let (m, _) = param_surface(
            &r,
            0.0,
            2.0 * std::f64::consts::PI,
            0.0,
            std::f64::consts::PI,
            64,
            32,
        );
        let t = topology(&m);
        assert_eq!(t.chi, 2);
        assert_eq!(t.boundary_loops, 0);
        let (_, a) = slice(&m, DVec3::Z, 0.0);
        assert!((a.abs() - std::f64::consts::PI).abs() < 0.02);
    }

    #[test]
    fn moebius() {
        let r = |u: f64, v: f64| {
            let w = 1.0 + 0.5 * v * (u / 2.0).cos();
            DVec3::new(w * u.cos(), w * u.sin(), 0.5 * v * (u / 2.0).sin())
        };
        let (m, _) = param_surface(&r, 0.0, 2.0 * std::f64::consts::PI, -1.0, 1.0, 60, 8);
        let t = topology(&m);
        assert!(!t.orientable);
        assert_eq!(t.boundary_loops, 1);
        assert_eq!(t.chi, 0);
    }
}
