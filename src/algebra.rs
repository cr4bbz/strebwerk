//! Exakter Algebra-Kern.
//!
//! Zahlen leben in einem Turm algebraischer Zahlkörper K = ℚ(g₁)(g₂)…(gₖ),
//! jeder Generator gᵢ hat eine monische Relation gᵢ^dᵢ = Rᵢ(g₁,…,gᵢ) mit
//! deg_{gᵢ} Rᵢ < dᵢ. Symbolische Größen sind Polynome in den freien
//! Parametern (a, b, h, t, …) mit Koeffizienten in K, also Elemente von
//! K[p₁,…,pₘ]. Reduziert man jedes Monom bezüglich der Relationen, ist die
//! Darstellung kanonisch – Gleichheit ist damit exakt entscheidbar (unter
//! der Voraussetzung, dass jede Relation über dem darunterliegenden Körper
//! irreduzibel ist).

use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{One, Signed, ToPrimitive, Zero};
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub type Q = BigRational;

pub fn qi(n: i64) -> Q {
    Q::from_integer(BigInt::from(n))
}
pub fn qr(n: i64, d: i64) -> Q {
    Q::new(BigInt::from(n), BigInt::from(d))
}
pub fn qf(x: &Q) -> f64 {
    if let Some(v) = x.to_f64() {
        return v;
    }
    let n = x.numer().to_f64().unwrap_or(f64::NAN);
    let d = x.denom().to_f64().unwrap_or(f64::NAN);
    n / d
}

// ---------------------------------------------------------------------------
// Kleine komplexe Arithmetik (für Konjugierte und Wurzelerkennung)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct C {
    pub re: f64,
    pub im: f64,
}
impl C {
    pub fn new(re: f64, im: f64) -> Self {
        C { re, im }
    }
    pub fn r(re: f64) -> Self {
        C { re, im: 0.0 }
    }
    pub fn add(self, o: C) -> C {
        C::new(self.re + o.re, self.im + o.im)
    }
    pub fn sub(self, o: C) -> C {
        C::new(self.re - o.re, self.im - o.im)
    }
    pub fn mul(self, o: C) -> C {
        C::new(
            self.re * o.re - self.im * o.im,
            self.re * o.im + self.im * o.re,
        )
    }
    pub fn div(self, o: C) -> C {
        let d = o.re * o.re + o.im * o.im;
        C::new(
            (self.re * o.re + self.im * o.im) / d,
            (self.im * o.re - self.re * o.im) / d,
        )
    }
    pub fn abs(self) -> f64 {
        self.re.hypot(self.im)
    }
    pub fn neg(self) -> C {
        C::new(-self.re, -self.im)
    }
    pub fn sqrt(self) -> C {
        let r = self.abs();
        let re = ((r + self.re) / 2.0).max(0.0).sqrt();
        let im = ((r - self.re) / 2.0).max(0.0).sqrt();
        C::new(re, if self.im < 0.0 { -im } else { im })
    }
}

/// Alle komplexen Nullstellen eines Polynoms mit rationalen Koeffizienten
/// (niedrig → hoch), Durand–Kerner mit Newton-Nachpolitur.
pub fn complex_roots(coeffs: &[Q]) -> Vec<C> {
    let n = coeffs.len() - 1;
    if n == 0 {
        return vec![];
    }
    let lead = qf(&coeffs[n]);
    let a: Vec<f64> = coeffs.iter().map(|c| qf(c) / lead).collect();
    let eval = |z: C| -> C {
        let mut acc = C::r(0.0);
        for k in (0..=n).rev() {
            acc = acc.mul(z).add(C::r(a[k]));
        }
        acc
    };
    let bound = 1.0 + a.iter().take(n).fold(0.0f64, |m, x| m.max(x.abs()));
    let mut z: Vec<C> = (0..n)
        .map(|k| {
            let ang = 2.0 * std::f64::consts::PI * (k as f64) / (n as f64) + 0.4;
            C::new(0.9 * bound * ang.cos(), 0.9 * bound * ang.sin())
        })
        .collect();
    for _ in 0..2000 {
        let mut maxd: f64 = 0.0;
        for i in 0..n {
            let mut den = C::r(1.0);
            for j in 0..n {
                if i != j {
                    den = den.mul(z[i].sub(z[j]));
                }
            }
            let delta = eval(z[i]).div(den);
            z[i] = z[i].sub(delta);
            maxd = maxd.max(delta.abs());
        }
        if maxd < 1e-15 {
            break;
        }
    }
    z
}

/// Kettenbruch-Rationalisierung.
pub fn rationalize(x: f64, max_den: i64, tol: f64) -> Option<Q> {
    if !x.is_finite() {
        return None;
    }
    let (mut h0, mut h1): (i128, i128) = (0, 1);
    let (mut k0, mut k1): (i128, i128) = (1, 0);
    let mut v = x;
    for _ in 0..64 {
        let a = v.floor();
        if a.abs() > 1e15 {
            return None;
        }
        let ai = a as i128;
        let h2 = ai * h1 + h0;
        let k2 = ai * k1 + k0;
        if k2 > max_den as i128 {
            break;
        }
        h0 = h1;
        h1 = h2;
        k0 = k1;
        k1 = k2;
        let approx = h1 as f64 / k1 as f64;
        if (approx - x).abs() <= tol * (1.0 + x.abs()) {
            return Some(Q::new(BigInt::from(h1), BigInt::from(k1)));
        }
        let frac = v - a;
        if frac.abs() < 1e-300 {
            break;
        }
        v = 1.0 / frac;
    }
    if k1 != 0 {
        let approx = h1 as f64 / k1 as f64;
        if (approx - x).abs() <= tol * (1.0 + x.abs()) {
            return Some(Q::new(BigInt::from(h1), BigInt::from(k1)));
        }
    }
    None
}

/// n = s²·r mit r quadratfrei (bis auf Faktoren > 10⁵, die nicht weiter zerlegt werden).
pub fn square_split(n: &BigInt) -> (BigInt, BigInt) {
    let mut s = BigInt::one();
    let mut r = BigInt::one();
    let mut m = n.abs();
    if m.is_zero() {
        return (BigInt::zero(), BigInt::one());
    }
    let mut p: u64 = 2;
    while p <= 100_000 {
        let bp = BigInt::from(p);
        if &bp * &bp > m {
            break;
        }
        let mut e = 0;
        while (&m % &bp).is_zero() {
            m /= &bp;
            e += 1;
        }
        for _ in 0..e / 2 {
            s *= &bp;
        }
        if e % 2 == 1 {
            r *= &bp;
        }
        p += if p == 2 { 1 } else { 2 };
    }
    if m > BigInt::one() {
        let rt = m.sqrt();
        if &rt * &rt == m {
            s *= rt;
        } else {
            r *= m;
        }
    }
    (s, r)
}

pub fn rational_sqrt(x: &Q) -> Option<Q> {
    if x.is_negative() {
        return None;
    }
    let n = x.numer();
    let d = x.denom();
    let sn = n.sqrt();
    let sd = d.sqrt();
    if &sn * &sn == *n && &sd * &sd == *d {
        Some(Q::new(sn, sd))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Generatoren
// ---------------------------------------------------------------------------

/// Ring-unabhängige Beschreibung eines Generators.
#[derive(Clone, Debug)]
pub struct GenDef {
    /// Name im Parser, z. B. "phi", "sqrt2", "gamma7".
    pub key: String,
    pub uni: String,
    pub tex: String,
    /// Reeller Wert der gewählten Einbettung.
    pub value: f64,
    pub degree: u32,
    /// g^degree = Σ coef · Π gen^exp (Terme in tieferen Generatoren und g selbst mit exp < degree)
    pub relation: Vec<(Vec<(String, u32)>, Q)>,
    /// Minimalpolynom über ℚ (monisch, niedrig → hoch), falls g direkt über ℚ definiert ist.
    pub qpoly: Option<Vec<Q>>,
    /// Sortierschlüssel im Turm (Abhängigkeiten zuerst).
    pub order: u32,
    pub deps: Vec<String>,
    /// Lesbare Definition, z. B. "γ₇ = 2·cos(2π/7)".
    pub meaning: String,
}

fn subscript(n: u32) -> String {
    n.to_string()
        .chars()
        .map(|c| match c {
            '0' => '₀',
            '1' => '₁',
            '2' => '₂',
            '3' => '₃',
            '4' => '₄',
            '5' => '₅',
            '6' => '₆',
            '7' => '₇',
            '8' => '₈',
            _ => '₉',
        })
        .collect()
}

pub fn superscript(n: u32) -> String {
    n.to_string()
        .chars()
        .map(|c| match c {
            '0' => '⁰',
            '1' => '¹',
            '2' => '²',
            '3' => '³',
            '4' => '⁴',
            '5' => '⁵',
            '6' => '⁶',
            '7' => '⁷',
            '8' => '⁸',
            _ => '⁹',
        })
        .collect()
}

pub fn simple_gen(
    key: &str,
    uni: &str,
    tex: &str,
    qpoly: Vec<Q>,
    value: f64,
    order: u32,
    meaning: &str,
) -> GenDef {
    let lead = qpoly.last().cloned().unwrap_or_else(Q::one);
    let qpoly: Vec<Q> = qpoly.iter().map(|c| c / &lead).collect();
    let d = (qpoly.len() - 1) as u32;
    let mut relation = vec![];
    for (j, c) in qpoly.iter().enumerate().take(d as usize) {
        if !c.is_zero() {
            let mono = if j == 0 {
                vec![]
            } else {
                vec![(key.to_string(), j as u32)]
            };
            relation.push((mono, -c.clone()));
        }
    }
    GenDef {
        key: key.into(),
        uni: uni.into(),
        tex: tex.into(),
        value,
        degree: d,
        relation,
        qpoly: Some(qpoly),
        order,
        deps: vec![],
        meaning: meaning.into(),
    }
}

/// Die eingebauten Generatoren.
pub fn standard_gen(key: &str) -> Option<GenDef> {
    let phi = (1.0 + 5f64.sqrt()) / 2.0;
    match key {
        "phi" => Some(simple_gen(
            "phi",
            "φ",
            "\\varphi",
            vec![qi(-1), qi(-1), qi(1)],
            phi,
            0,
            "φ = (1+√5)/2, goldener Schnitt",
        )),
        "sqrt2" => Some(simple_gen(
            "sqrt2",
            "√2",
            "\\sqrt{2}",
            vec![qi(-2), qi(0), qi(1)],
            2f64.sqrt(),
            1,
            "√2",
        )),
        "sqrt3" => Some(simple_gen(
            "sqrt3",
            "√3",
            "\\sqrt{3}",
            vec![qi(-3), qi(0), qi(1)],
            3f64.sqrt(),
            2,
            "√3",
        )),
        "rho" => Some(simple_gen(
            "rho",
            "ρ",
            "\\rho",
            vec![qi(-1), qi(-1), qi(0), qi(1)],
            1.324_717_957_244_746,
            3,
            "ρ: plastische Zahl, reelle Nullstelle von x³ − x − 1",
        )),
        _ => {
            if let Some(n) = key
                .strip_prefix("gamma")
                .and_then(|s| s.parse::<u32>().ok())
            {
                return Some(gamma_gen(n));
            }
            if let Some(n) = key
                .strip_prefix("sigma")
                .and_then(|s| s.parse::<u32>().ok())
            {
                return Some(sigma_gen(n));
            }
            None
        }
    }
}

// --- Kreisteilung --------------------------------------------------------

fn qpoly_mul(a: &[Q], b: &[Q]) -> Vec<Q> {
    let mut r = vec![Q::zero(); a.len() + b.len() - 1];
    for (i, x) in a.iter().enumerate() {
        for (j, y) in b.iter().enumerate() {
            r[i + j] += x * y;
        }
    }
    r
}

fn qpoly_divexact(a: &[Q], b: &[Q]) -> Vec<Q> {
    let mut rem = a.to_vec();
    let db = b.len() - 1;
    let lb = b[db].clone();
    if rem.len() < b.len() {
        return vec![Q::zero()];
    }
    let mut quo = vec![Q::zero(); rem.len() - db];
    for k in (0..quo.len()).rev() {
        let c = &rem[k + db] / &lb;
        for j in 0..=db {
            rem[k + j] -= &c * &b[j];
        }
        quo[k] = c;
    }
    quo
}

/// Kreisteilungspolynom Φₙ.
pub fn cyclotomic(n: u32) -> Vec<Q> {
    let mut p = vec![Q::zero(); n as usize + 1];
    p[0] = qi(-1);
    p[n as usize] = qi(1);
    for d in 1..n {
        if n % d == 0 {
            let phd = cyclotomic(d);
            p = qpoly_divexact(&p, &phd);
        }
    }
    p
}

/// Dickson-Polynome Dₖ(y) mit Dₖ(z + 1/z) = zᵏ + z⁻ᵏ, D₀ = 2.
pub fn dickson(k: u32) -> Vec<Q> {
    let mut a = vec![qi(2)];
    if k == 0 {
        return a;
    }
    let mut b = vec![qi(0), qi(1)];
    for _ in 1..k {
        let mut c = qpoly_mul(&b, &[qi(0), qi(1)]);
        for (i, x) in a.iter().enumerate() {
            c[i] -= x;
        }
        a = b;
        b = c;
    }
    b
}

/// Eₖ(y) mit E₋₁ = 0, E₀ = 1, Eₖ₊₁ = y·Eₖ − Eₖ₋₁  (Eₖ(2cosθ) = sin((k+1)θ)/sinθ).
pub fn cheb_e(k: i64) -> Vec<Q> {
    if k < 0 {
        return vec![qi(0)];
    }
    let mut a = vec![qi(0)];
    let mut b = vec![qi(1)];
    for _ in 0..k {
        let mut c = qpoly_mul(&b, &[qi(0), qi(1)]);
        while c.len() < a.len() {
            c.push(Q::zero());
        }
        for (i, x) in a.iter().enumerate() {
            c[i] -= x;
        }
        a = b;
        b = c;
    }
    b
}

/// Minimalpolynom von 2·cos(2π/n) über ℚ.
pub fn psi(n: u32) -> Vec<Q> {
    let phi_n = cyclotomic(n);
    let deg = phi_n.len() - 1;
    let m = deg / 2;
    let mut out = vec![Q::zero(); m + 1];
    out[0] += &phi_n[m];
    for k in 1..=m {
        let dk = dickson(k as u32);
        for (i, c) in dk.iter().enumerate() {
            out[i] += &phi_n[m + k] * c;
        }
    }
    out
}

pub fn gamma_gen(n: u32) -> GenDef {
    let v = 2.0 * (2.0 * std::f64::consts::PI / n as f64).cos();
    let mut g = simple_gen(
        &format!("gamma{n}"),
        &format!("γ{}", subscript(n)),
        &format!("\\gamma_{{{n}}}"),
        psi(n),
        v,
        100 + 2 * n,
        &format!("γ{} = 2·cos(2π/{n})", subscript(n)),
    );
    g.meaning = format!("γ{} = 2·cos(2π/{n})", subscript(n));
    g
}

pub fn sigma_gen(n: u32) -> GenDef {
    let v = 2.0 * (2.0 * std::f64::consts::PI / n as f64).sin();
    let (relation, deps) = match n {
        5 => (
            vec![(vec![], qi(2)), (vec![("phi".to_string(), 1)], qi(1))],
            vec!["phi".to_string()],
        ),
        10 => (
            vec![(vec![], qi(3)), (vec![("phi".to_string(), 1)], qi(-1))],
            vec!["phi".to_string()],
        ),
        _ => (
            vec![(vec![], qi(4)), (vec![(format!("gamma{n}"), 2)], qi(-1))],
            vec![format!("gamma{n}")],
        ),
    };
    GenDef {
        key: format!("sigma{n}"),
        uni: format!("σ{}", subscript(n)),
        tex: format!("\\sigma_{{{n}}}"),
        value: v,
        degree: 2,
        relation,
        qpoly: None,
        order: 101 + 2 * n,
        deps,
        meaning: format!("σ{} = 2·sin(2π/{n})", subscript(n)),
    }
}

/// Exakte Darstellung von γ = 2cos(2π/n) und σ = 2sin(2π/n) als Terme über Generatoren.
pub type Terms = Vec<(Vec<(String, u32)>, Q)>;

pub fn cyc_terms(n: u32) -> (Terms, Terms) {
    let k = |s: &str| vec![(s.to_string(), 1u32)];
    match n {
        1 => (vec![(vec![], qi(2))], vec![]),
        2 => (vec![(vec![], qi(-2))], vec![]),
        3 => (vec![(vec![], qi(-1))], vec![(k("sqrt3"), qi(1))]),
        4 => (vec![], vec![(vec![], qi(2))]),
        5 => (
            vec![(k("phi"), qi(1)), (vec![], qi(-1))],
            vec![(k("sigma5"), qi(1))],
        ),
        6 => (vec![(vec![], qi(1))], vec![(k("sqrt3"), qi(1))]),
        8 => (vec![(k("sqrt2"), qi(1))], vec![(k("sqrt2"), qi(1))]),
        10 => (vec![(k("phi"), qi(1))], vec![(k("sigma10"), qi(1))]),
        12 => (vec![(k("sqrt3"), qi(1))], vec![(vec![], qi(1))]),
        _ => {
            let g = format!("gamma{n}");
            let gamma = vec![(vec![(g.clone(), 1)], qi(1))];
            let sigma = if n % 4 == 0 {
                let d = dickson(n / 4 - 1);
                d.iter()
                    .enumerate()
                    .filter(|(_, c)| !c.is_zero())
                    .map(|(i, c)| {
                        (
                            if i == 0 {
                                vec![]
                            } else {
                                vec![(g.clone(), i as u32)]
                            },
                            c.clone(),
                        )
                    })
                    .collect()
            } else {
                vec![(vec![(format!("sigma{n}"), 1)], qi(1))]
            };
            (gamma, sigma)
        }
    }
}

pub fn terms_keys(t: &Terms) -> Vec<String> {
    let mut out = vec![];
    for (m, _) in t {
        for (k, _) in m {
            out.push(k.clone());
        }
    }
    out
}

/// Schließt eine Menge von Generator-Schlüsseln unter Abhängigkeiten ab.
pub fn resolve_gens(keys: &BTreeSet<String>, custom: &[GenDef]) -> Result<Vec<GenDef>, String> {
    let mut out: BTreeMap<String, GenDef> = BTreeMap::new();
    let mut stack: Vec<String> = keys.iter().cloned().collect();
    while let Some(k) = stack.pop() {
        if out.contains_key(&k) {
            continue;
        }
        let g = custom
            .iter()
            .find(|g| g.key == k)
            .cloned()
            .or_else(|| standard_gen(&k))
            .ok_or_else(|| format!("unbekannter Generator „{k}“"))?;
        for d in &g.deps {
            stack.push(d.clone());
        }
        out.insert(k, g);
    }
    let mut v: Vec<GenDef> = out.into_values().collect();
    v.sort_by_key(|g| g.order);
    Ok(v)
}

// ---------------------------------------------------------------------------
// Polynome
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, Default, Hash)]
pub struct Poly {
    pub t: BTreeMap<Vec<u32>, Q>,
}

impl Poly {
    pub fn zero() -> Self {
        Self::default()
    }
    pub fn constant(c: Q, n: usize) -> Self {
        let mut t = BTreeMap::new();
        if !c.is_zero() {
            t.insert(vec![0; n], c);
        }
        Poly { t }
    }
    pub fn monomial(e: Vec<u32>, c: Q) -> Self {
        let mut t = BTreeMap::new();
        if !c.is_zero() {
            t.insert(e, c);
        }
        Poly { t }
    }
    pub fn is_zero(&self) -> bool {
        self.t.is_empty()
    }
    pub fn add(&self, o: &Poly) -> Poly {
        let mut t = self.t.clone();
        for (e, c) in &o.t {
            let z = {
                let ent = t.entry(e.clone()).or_insert_with(Q::zero);
                *ent += c;
                ent.is_zero()
            };
            if z {
                t.remove(e);
            }
        }
        Poly { t }
    }
    pub fn neg(&self) -> Poly {
        Poly {
            t: self
                .t
                .iter()
                .map(|(e, c)| (e.clone(), -c.clone()))
                .collect(),
        }
    }
    pub fn sub(&self, o: &Poly) -> Poly {
        self.add(&o.neg())
    }
    pub fn scale(&self, k: &Q) -> Poly {
        if k.is_zero() {
            return Poly::zero();
        }
        Poly {
            t: self.t.iter().map(|(e, c)| (e.clone(), c * k)).collect(),
        }
    }
    pub fn mul_raw(&self, o: &Poly) -> Poly {
        let mut t: BTreeMap<Vec<u32>, Q> = BTreeMap::new();
        for (e1, c1) in &self.t {
            for (e2, c2) in &o.t {
                let e: Vec<u32> = e1.iter().zip(e2).map(|(a, b)| a + b).collect();
                *t.entry(e).or_insert_with(Q::zero) += c1 * c2;
            }
        }
        t.retain(|_, c| !c.is_zero());
        Poly { t }
    }
    pub fn as_constant(&self) -> Option<Q> {
        if self.t.is_empty() {
            return Some(Q::zero());
        }
        if self.t.len() == 1 {
            let (e, c) = self.t.iter().next().unwrap();
            if e.iter().all(|x| *x == 0) {
                return Some(c.clone());
            }
        }
        None
    }
    pub fn is_one(&self) -> bool {
        self.as_constant().map(|c| c.is_one()).unwrap_or(false)
    }
}

// ---------------------------------------------------------------------------
// Ring K[p₁…pₘ]
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct Gen {
    pub def: GenDef,
    pub relation: Poly,
    /// Konjugierte (nur für über ℚ definierte Generatoren), roots[0] = gewählte Einbettung.
    pub roots: Vec<C>,
}

#[derive(Clone, Debug)]
pub struct ParamInfo {
    pub name: String,
    pub value: f64,
    pub nonneg: bool,
}

#[derive(Clone, Debug)]
pub struct Ring {
    pub gens: Vec<Gen>,
    pub params: Vec<ParamInfo>,
    values: Vec<f64>,
}

/// Symbolischer Bruch num/den in K(p₁…pₘ).
#[derive(Clone, Debug)]
pub struct Sym {
    pub num: Poly,
    pub den: Poly,
}

/// outside · |absm| · √inside
#[derive(Clone, Debug)]
pub struct Radical {
    pub outside: Sym,
    pub absm: Option<Poly>,
    pub inside: Option<Poly>,
}

impl Ring {
    pub fn new(gendefs: Vec<GenDef>, params: Vec<ParamInfo>) -> Result<Ring, String> {
        let mut gendefs = gendefs;
        gendefs.sort_by_key(|g| g.order);
        let ng = gendefs.len();
        let n = ng + params.len();
        let idx: HashMap<String, usize> = gendefs
            .iter()
            .enumerate()
            .map(|(i, g)| (g.key.clone(), i))
            .collect();
        let mut gens = vec![];
        for (gi, g) in gendefs.iter().enumerate() {
            let mut rel = Poly::zero();
            for (mono, c) in &g.relation {
                let mut e = vec![0u32; n];
                for (k, p) in mono {
                    let j = *idx.get(k).ok_or_else(|| {
                        format!("Generator {} hängt von {k} ab, fehlt im Turm", g.key)
                    })?;
                    if j > gi || (j == gi && *p >= g.degree) {
                        return Err(format!("Relation von {} ist nicht turmförmig", g.key));
                    }
                    e[j] += p;
                }
                rel = rel.add(&Poly::monomial(e, c.clone()));
            }
            let roots = match &g.qpoly {
                Some(qp) => {
                    let mut r = complex_roots(qp);
                    if let Some(pos) = (0..r.len()).min_by(|a, b| {
                        let da = r[*a].sub(C::r(g.value)).abs();
                        let db = r[*b].sub(C::r(g.value)).abs();
                        da.partial_cmp(&db).unwrap()
                    }) {
                        r.swap(0, pos);
                        r[0] = C::r(g.value);
                    }
                    r
                }
                None => vec![],
            };
            gens.push(Gen {
                def: g.clone(),
                relation: rel,
                roots,
            });
        }
        let mut values: Vec<f64> = gens.iter().map(|g| g.def.value).collect();
        values.extend(params.iter().map(|p| p.value));
        Ok(Ring {
            gens,
            params,
            values,
        })
    }

    pub fn n(&self) -> usize {
        self.gens.len() + self.params.len()
    }
    pub fn ng(&self) -> usize {
        self.gens.len()
    }

    pub fn gen_poly(&self, key: &str) -> Option<Poly> {
        let i = self.gens.iter().position(|g| g.def.key == key)?;
        let mut e = vec![0; self.n()];
        e[i] = 1;
        Some(Poly::monomial(e, qi(1)))
    }
    pub fn param_poly(&self, name: &str) -> Option<Poly> {
        let i = self.params.iter().position(|p| p.name == name)?;
        let mut e = vec![0; self.n()];
        e[self.ng() + i] = 1;
        Some(Poly::monomial(e, qi(1)))
    }
    pub fn c(&self, x: Q) -> Poly {
        Poly::constant(x, self.n())
    }
    pub fn one(&self) -> Poly {
        self.c(qi(1))
    }

    pub fn terms(&self, t: &Terms) -> Poly {
        let mut out = Poly::zero();
        for (mono, c) in t {
            let mut p = self.c(c.clone());
            for (k, e) in mono {
                let g = self.gen_poly(k).expect("Generator fehlt im Ring");
                p = self.mul(&p, &self.pow(&g, *e));
            }
            out = out.add(&p);
        }
        out
    }

    pub fn reduce(&self, p: Poly) -> Poly {
        let ng = self.ng();
        let mut out: BTreeMap<Vec<u32>, Q> = BTreeMap::new();
        let mut work: Vec<(Vec<u32>, Q)> = p.t.into_iter().collect();
        let mut guard = 0usize;
        while let Some((e, c)) = work.pop() {
            guard += 1;
            if guard > 5_000_000 {
                break;
            }
            if c.is_zero() {
                continue;
            }
            let mut red = None;
            for i in (0..ng).rev() {
                if e[i] >= self.gens[i].def.degree {
                    red = Some(i);
                    break;
                }
            }
            match red {
                None => *out.entry(e).or_insert_with(Q::zero) += c,
                Some(i) => {
                    let mut e2 = e.clone();
                    e2[i] -= self.gens[i].def.degree;
                    for (re, rc) in &self.gens[i].relation.t {
                        let ne: Vec<u32> = e2.iter().zip(re).map(|(a, b)| a + b).collect();
                        work.push((ne, &c * rc));
                    }
                }
            }
        }
        out.retain(|_, c| !c.is_zero());
        Poly { t: out }
    }

    pub fn mul(&self, a: &Poly, b: &Poly) -> Poly {
        self.reduce(a.mul_raw(b))
    }
    pub fn pow(&self, a: &Poly, k: u32) -> Poly {
        let mut r = self.one();
        let mut base = a.clone();
        let mut k = k;
        while k > 0 {
            if k & 1 == 1 {
                r = self.mul(&r, &base);
            }
            k >>= 1;
            if k > 0 {
                base = self.mul(&base, &base);
            }
        }
        r
    }

    pub fn is_param_free(&self, p: &Poly) -> bool {
        let ng = self.ng();
        p.t.keys().all(|e| e[ng..].iter().all(|x| *x == 0))
    }

    fn kbasis(&self) -> Vec<Vec<u32>> {
        let mut out = vec![vec![0u32; self.n()]];
        for (i, g) in self.gens.iter().enumerate() {
            let mut nxt = vec![];
            for b in &out {
                for k in 0..g.def.degree {
                    let mut e = b.clone();
                    e[i] = k;
                    nxt.push(e);
                }
            }
            out = nxt;
        }
        out
    }

    /// Inverses eines parameterfreien Elements in K (lineare Algebra in der regulären Darstellung).
    pub fn inv_k(&self, x: &Poly) -> Option<Poly> {
        if x.is_zero() || !self.is_param_free(x) {
            return None;
        }
        if let Some(c) = x.as_constant() {
            return Some(self.c(c.recip()));
        }
        let basis = self.kbasis();
        let nb = basis.len();
        let index: HashMap<Vec<u32>, usize> = basis
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, e)| (e, i))
            .collect();
        let mut m = vec![vec![Q::zero(); nb + 1]; nb];
        for (j, b) in basis.iter().enumerate() {
            let prod = self.mul(x, &Poly::monomial(b.clone(), qi(1)));
            for (e, c) in prod.t {
                let i = *index.get(&e)?;
                m[i][j] = c;
            }
        }
        let one_idx = *index.get(&vec![0u32; self.n()])?;
        m[one_idx][nb] = qi(1);
        // Gauß-Jordan
        for col in 0..nb {
            let piv = (col..nb).find(|r| !m[*r][col].is_zero())?;
            m.swap(col, piv);
            let p = m[col][col].clone();
            for k in col..=nb {
                m[col][k] = &m[col][k] / &p;
            }
            for r in 0..nb {
                if r != col && !m[r][col].is_zero() {
                    let f = m[r][col].clone();
                    for k in col..=nb {
                        let v = &f * &m[col][k];
                        m[r][k] -= v;
                    }
                }
            }
        }
        let mut y = Poly::zero();
        for (j, b) in basis.iter().enumerate() {
            y = y.add(&Poly::monomial(b.clone(), m[j][nb].clone()));
        }
        if self.mul(x, &y).is_one() {
            Some(y)
        } else {
            None
        }
    }

    pub fn values(&self) -> &[f64] {
        &self.values
    }

    pub fn eval(&self, p: &Poly) -> f64 {
        let v = &self.values;
        let mut acc = 0.0;
        for (e, c) in &p.t {
            let mut t = qf(c);
            for (i, k) in e.iter().enumerate() {
                if *k > 0 {
                    t *= v[i].powi(*k as i32);
                }
            }
            acc += t;
        }
        acc
    }

    /// Quadratwurzel eines parameterfreien Elements, falls sie in K liegt.
    /// Nur für Elemente in ℚ oder in einem einzelnen über ℚ definierten Generator;
    /// das Ergebnis wird exakt verifiziert.
    pub fn ksqrt(&self, x: &Poly) -> Option<Poly> {
        if x.is_zero() {
            return Some(Poly::zero());
        }
        if !self.is_param_free(x) {
            return None;
        }
        if let Some(c) = x.as_constant() {
            if let Some(r) = rational_sqrt(&c) {
                return Some(self.c(r));
            }
            // rationale Zahl, deren Wurzel in einem einfachen Generator liegt (√2 ∈ ℚ(√2), √5 ∈ ℚ(φ) …)
            return (0..self.ng()).find_map(|gi| self.ksqrt_in(x, gi));
        }
        let ng = self.ng();
        let used: BTreeSet<usize> =
            x.t.keys()
                .flat_map(|e| (0..ng).filter(move |i| e[*i] > 0))
                .collect();
        if used.len() != 1 {
            return None;
        }
        self.ksqrt_in(x, *used.iter().next().unwrap())
    }

    /// Wurzel eines Elements von ℚ(g_gi) in ℚ(g_gi), über die Konjugierten gesucht und exakt geprüft.
    fn ksqrt_in(&self, x: &Poly, gi: usize) -> Option<Poly> {
        let g = &self.gens[gi];
        if g.def.qpoly.is_none() || g.roots.len() != g.def.degree as usize {
            return None;
        }
        if self.eval(x) < 0.0 {
            return None;
        }
        let d = g.def.degree as usize;
        let mut a = vec![0.0f64; d];
        for (e, c) in &x.t {
            a[e[gi] as usize] += qf(c);
        }
        let xv: Vec<C> = g
            .roots
            .iter()
            .map(|r| {
                let mut acc = C::r(0.0);
                let mut pw = C::r(1.0);
                for aj in &a {
                    acc = acc.add(pw.mul(C::r(*aj)));
                    pw = pw.mul(*r);
                }
                acc
            })
            .collect();
        let y0 = C::r(xv[0].re.max(0.0).sqrt());
        let combos = 1usize << (d - 1);
        for mask in 0..combos {
            let mut y = vec![y0];
            for k in 1..d {
                let s = xv[k].sqrt();
                y.push(if mask >> (k - 1) & 1 == 1 { s.neg() } else { s });
            }
            // Vandermonde lösen: Σ c_j r_k^j = y_k
            let mut m: Vec<Vec<C>> = (0..d)
                .map(|k| {
                    let mut row = vec![];
                    let mut pw = C::r(1.0);
                    for _ in 0..d {
                        row.push(pw);
                        pw = pw.mul(g.roots[k]);
                    }
                    row.push(y[k]);
                    row
                })
                .collect();
            let mut ok = true;
            for col in 0..d {
                let piv = (col..d)
                    .max_by(|p, q| m[*p][col].abs().partial_cmp(&m[*q][col].abs()).unwrap())
                    .unwrap();
                if m[piv][col].abs() < 1e-14 {
                    ok = false;
                    break;
                }
                m.swap(col, piv);
                for r in 0..d {
                    if r != col {
                        let f = m[r][col].div(m[col][col]);
                        for k in col..=d {
                            let v = f.mul(m[col][k]);
                            m[r][k] = m[r][k].sub(v);
                        }
                    }
                }
            }
            if !ok {
                continue;
            }
            let mut cand = Poly::zero();
            let mut good = true;
            for j in 0..d {
                let cj = m[j][d].div(m[j][j]);
                if cj.im.abs() > 1e-7 * (1.0 + cj.re.abs()) {
                    good = false;
                    break;
                }
                match rationalize(cj.re, 1_000_000, 1e-10) {
                    Some(q) => {
                        let mut e = vec![0; self.n()];
                        e[gi] = j as u32;
                        cand = cand.add(&Poly::monomial(e, q));
                    }
                    None => {
                        good = false;
                        break;
                    }
                }
            }
            if !good {
                continue;
            }
            if self.mul(&cand, &cand) == *x {
                if self.eval(&cand) < 0.0 {
                    cand = cand.neg();
                }
                return Some(cand);
            }
        }
        None
    }

    // ---- Sym -----------------------------------------------------------

    pub fn sym(&self, p: Poly) -> Sym {
        Sym {
            num: p,
            den: self.one(),
        }
    }
    pub fn sym_c(&self, c: Q) -> Sym {
        self.sym(self.c(c))
    }
    pub fn normalize(&self, s: Sym) -> Sym {
        if s.num.is_zero() {
            return self.sym(Poly::zero());
        }
        if s.den.is_one() {
            return s;
        }
        if let Some(inv) = self.inv_k(&s.den) {
            return self.sym(self.mul(&s.num, &inv));
        }
        s
    }
    pub fn sadd(&self, a: &Sym, b: &Sym) -> Sym {
        if a.den == b.den {
            return self.normalize(Sym {
                num: a.num.add(&b.num),
                den: a.den.clone(),
            });
        }
        self.normalize(Sym {
            num: self.mul(&a.num, &b.den).add(&self.mul(&b.num, &a.den)),
            den: self.mul(&a.den, &b.den),
        })
    }
    pub fn sneg(&self, a: &Sym) -> Sym {
        Sym {
            num: a.num.neg(),
            den: a.den.clone(),
        }
    }
    pub fn ssub(&self, a: &Sym, b: &Sym) -> Sym {
        self.sadd(a, &self.sneg(b))
    }
    pub fn smul(&self, a: &Sym, b: &Sym) -> Sym {
        self.normalize(Sym {
            num: self.mul(&a.num, &b.num),
            den: self.mul(&a.den, &b.den),
        })
    }
    pub fn sdiv(&self, a: &Sym, b: &Sym) -> Result<Sym, String> {
        if b.num.is_zero() {
            return Err("Division durch null".into());
        }
        Ok(self.normalize(Sym {
            num: self.mul(&a.num, &b.den),
            den: self.mul(&a.den, &b.num),
        }))
    }
    pub fn sscale(&self, a: &Sym, k: Q) -> Sym {
        Sym {
            num: a.num.scale(&k),
            den: a.den.clone(),
        }
    }
    pub fn spow(&self, a: &Sym, k: i64) -> Result<Sym, String> {
        let p = Sym {
            num: self.pow(&a.num, k.unsigned_abs() as u32),
            den: self.pow(&a.den, k.unsigned_abs() as u32),
        };
        if k >= 0 {
            Ok(self.normalize(p))
        } else {
            self.sdiv(&self.sym(self.one()), &p)
        }
    }
    pub fn seq(&self, a: &Sym, b: &Sym) -> bool {
        if a.den == b.den {
            return a.num == b.num;
        }
        self.mul(&a.num, &b.den) == self.mul(&b.num, &a.den)
    }
    pub fn seval(&self, a: &Sym) -> f64 {
        self.eval(&a.num) / self.eval(&a.den)
    }
    pub fn sis_zero(&self, a: &Sym) -> bool {
        a.num.is_zero()
    }

    fn param_mono(&self, exps: &[u32]) -> Poly {
        let mut e = vec![0; self.n()];
        for (j, k) in exps.iter().enumerate() {
            e[self.ng() + j] = *k;
        }
        Poly::monomial(e, qi(1))
    }

    /// √s, so weit wie möglich exakt vereinfacht.
    pub fn sqrt(&self, s: &Sym) -> Radical {
        let one = self.sym(self.one());
        let (p, mut outside) = if s.den.is_one() {
            (s.num.clone(), one.clone())
        } else {
            // √(n/d) = √(n·d)/d   (d > 0 vorausgesetzt)
            (
                self.mul(&s.num, &s.den),
                Sym {
                    num: self.one(),
                    den: s.den.clone(),
                },
            )
        };
        if p.is_zero() {
            return Radical {
                outside: self.sym(Poly::zero()),
                absm: None,
                inside: None,
            };
        }
        let ng = self.ng();
        let np = self.params.len();
        let mut mins = vec![u32::MAX; np];
        for e in p.t.keys() {
            for j in 0..np {
                mins[j] = mins[j].min(e[ng + j]);
            }
        }
        let mut pr = Poly::zero();
        for (e, c) in &p.t {
            let mut e2 = e.clone();
            for j in 0..np {
                e2[ng + j] -= mins[j];
            }
            pr = pr.add(&Poly::monomial(e2, c.clone()));
        }
        let half: Vec<u32> = mins.iter().map(|m| m / 2).collect();
        let odd: Vec<u32> = mins.iter().map(|m| m % 2).collect();
        // Parameter mit möglichem negativem Wert kommen in |·|
        let mut half_pos = half.clone();
        let mut half_abs = vec![0u32; np];
        for j in 0..np {
            if !self.params[j].nonneg && half[j] > 0 {
                half_abs[j] = half[j];
                half_pos[j] = 0;
            }
        }
        outside = self.smul(&outside, &self.sym(self.param_mono(&half_pos)));
        let absm = if half_abs.iter().any(|x| *x > 0) {
            Some(self.param_mono(&half_abs))
        } else {
            None
        };
        let oddm = self.param_mono(&odd);

        if let Some(k) = self.psqrt(&pr) {
            outside = self.smul(&outside, &self.sym(k));
            let inside = if oddm.is_one() { None } else { Some(oddm) };
            return Radical {
                outside,
                absm,
                inside,
            };
        }
        // rationaler Inhalt
        let mut gnum = BigInt::zero();
        let mut lden = BigInt::one();
        for c in pr.t.values() {
            gnum = gnum.gcd(c.numer());
            lden = lden.lcm(c.denom());
        }
        let content = Q::new(gnum, lden.clone());
        let prim = pr.scale(&content.recip());
        // √(n/d) = √(n·d)/d
        let nd = content.numer() * content.denom();
        let (sq, r) = square_split(&nd);
        outside = self.smul(&outside, &self.sym_c(Q::new(sq, content.denom().clone())));
        let mut inside = self.mul(&self.mul(&prim, &oddm), &self.c(Q::from_integer(r)));
        // Vorzeichen-Normierung: Inneres positiv auswerten
        if self.eval(&inside) < 0.0 && !inside.is_zero() {
            inside = inside.neg();
            outside = self.sneg(&outside);
        }
        let inside = if inside.is_one() { None } else { Some(inside) };
        Radical {
            outside,
            absm,
            inside,
        }
    }

    /// Quadratwurzel in K[Parameter], falls `p` ein Quadrat ist (Leitterm-Verfahren in
    /// graduierter Ordnung, Ergebnis exakt geprüft). Vorzeichen: positiv beim aktuellen Wert.
    pub fn psqrt(&self, p: &Poly) -> Option<Poly> {
        if p.is_zero() {
            return Some(Poly::zero());
        }
        if self.is_param_free(p) {
            return self.ksqrt(p);
        }
        let ng = self.ng();
        let split = |q: &Poly| -> BTreeMap<Vec<u32>, Poly> {
            let mut g: BTreeMap<Vec<u32>, Poly> = BTreeMap::new();
            for (e, c) in &q.t {
                let mut ek = e.clone();
                for x in ek[ng..].iter_mut() {
                    *x = 0;
                }
                let ent = g.entry(e[ng..].to_vec()).or_default();
                *ent = ent.add(&Poly::monomial(ek, c.clone()));
            }
            g
        };
        let ord = |m: &Vec<u32>| (m.iter().sum::<u32>(), m.clone());
        let lead = |q: &Poly| -> Option<(Vec<u32>, Poly)> {
            split(q)
                .into_iter()
                .max_by(|a, b| ord(&a.0).cmp(&ord(&b.0)))
        };
        let mono = |m: &[u32], k: &Poly| -> Poly {
            let mut e = vec![0u32; self.n()];
            e[ng..].copy_from_slice(m);
            self.mul(k, &Poly::monomial(e, qi(1)))
        };
        let (m0, k0) = lead(p)?;
        if m0.iter().any(|x| x % 2 == 1) {
            return None;
        }
        let kq = self.ksqrt(&k0)?;
        let mq: Vec<u32> = m0.iter().map(|x| x / 2).collect();
        let mut q = mono(&mq, &kq);
        let inv2 = self.inv_k(&kq.scale(&qi(2)))?;
        for _ in 0..200 {
            let rem = p.sub(&self.mul(&q, &q));
            if rem.is_zero() {
                if self.eval(&q) < 0.0 {
                    q = q.neg();
                }
                return Some(q);
            }
            let (mr, kr) = lead(&rem)?;
            if mr.iter().zip(&mq).any(|(a, b)| a < b) {
                return None;
            }
            let mt: Vec<u32> = mr.iter().zip(&mq).map(|(a, b)| a - b).collect();
            if ord(&mt) >= ord(&mq) {
                return None;
            }
            q = q.add(&mono(&mt, &self.mul(&kr, &inv2)));
        }
        None
    }

    pub fn rad_eval(&self, r: &Radical) -> f64 {
        let mut v = self.seval(&r.outside);
        if let Some(a) = &r.absm {
            v *= self.eval(a).abs();
        }
        if let Some(i) = &r.inside {
            v *= self.eval(i).max(0.0).sqrt();
        }
        v
    }

    // ---- Ausgabe -------------------------------------------------------

    fn fmt_q(&self, q: &Q, tex: bool) -> String {
        if q.is_integer() {
            q.numer().to_string()
        } else if tex {
            format!("\\tfrac{{{}}}{{{}}}", q.numer(), q.denom())
        } else {
            format!("{}/{}", q.numer(), q.denom())
        }
    }

    fn fmt_gen_mono(&self, e: &[u32], tex: bool) -> String {
        let mut parts = vec![];
        for (i, g) in self.gens.iter().enumerate() {
            let k = e[i];
            if k == 0 {
                continue;
            }
            let base = if tex {
                g.def.tex.clone()
            } else {
                g.def.uni.clone()
            };
            if k == 1 {
                parts.push(base);
            } else if tex {
                let b = if base.starts_with("\\sqrt") {
                    format!("({base})")
                } else {
                    base
                };
                parts.push(format!("{b}^{{{k}}}"));
            } else {
                let b = if base.starts_with('√') {
                    format!("({base})")
                } else {
                    base
                };
                parts.push(format!("{b}{}", superscript(k)));
            }
        }
        parts.join(if tex { "\\," } else { "" })
    }

    fn fmt_param_mono(&self, e: &[u32], tex: bool) -> String {
        let mut s = String::new();
        for (j, p) in self.params.iter().enumerate() {
            let k = e[self.ng() + j];
            if k == 0 {
                continue;
            }
            let name = if tex && p.name.chars().count() > 1 {
                format!("\\mathit{{{}}}", p.name)
            } else {
                p.name.clone()
            };
            if k == 1 {
                s.push_str(&name);
            } else if tex {
                s.push_str(&format!("{name}^{{{k}}}"));
            } else {
                s.push_str(&format!("{name}{}", superscript(k)));
            }
        }
        s
    }

    /// Koeffizient · Parametermonom · Generatormonom zu einem Term zusammensetzen.
    fn compose(coef: &str, pm: &str, gen: &str, tex: bool) -> String {
        let mut s = coef.to_string();
        if !pm.is_empty() {
            if !tex && s.contains('/') {
                s.push('·');
            }
            s.push_str(pm);
        }
        if !gen.is_empty() {
            if !tex && s.contains('/') && pm.is_empty() {
                s.push('·');
            }
            if tex && !s.is_empty() {
                s.push_str("\\,");
            }
            s.push_str(gen);
        }
        if s.is_empty() {
            "1".into()
        } else {
            s
        }
    }

    /// Ein K-Element (parameterfrei) als Liste (negativ?, Koeffizient, Generatorteil).
    fn k_parts(&self, k: &Poly, tex: bool) -> Vec<(bool, String, String)> {
        let mut terms: Vec<(&Vec<u32>, &Q)> = k.t.iter().collect();
        terms.sort_by_key(|(e, _)| (e[..self.ng()].iter().sum::<u32>(), (*e).clone()));
        terms
            .into_iter()
            .map(|(e, c)| {
                let g = self.fmt_gen_mono(e, tex);
                let a = c.abs();
                let coef = if a.is_one() && !g.is_empty() {
                    String::new()
                } else {
                    self.fmt_q(&a, tex)
                };
                (c.is_negative(), coef, g)
            })
            .collect()
    }

    fn k_terms(&self, k: &Poly, tex: bool) -> Vec<(bool, String)> {
        self.k_parts(k, tex)
            .into_iter()
            .map(|(n, c, g)| (n, Self::compose(&c, "", &g, tex)))
            .collect()
    }

    fn join_terms(terms: &[(bool, String)], tex: bool) -> String {
        let minus = if tex { "-" } else { "−" };
        let mut s = String::new();
        for (i, (neg, body)) in terms.iter().enumerate() {
            if i == 0 {
                if *neg {
                    s.push_str(minus);
                }
            } else if *neg {
                s.push_str(if tex { " - " } else { " − " });
            } else {
                s.push_str(" + ");
            }
            s.push_str(body);
        }
        if s.is_empty() {
            s.push('0');
        }
        s
    }

    fn poly_terms(&self, p: &Poly, tex: bool) -> Vec<(bool, String)> {
        let ng = self.ng();
        let mut groups: BTreeMap<Vec<u32>, Poly> = BTreeMap::new();
        for (e, c) in &p.t {
            let pk = e[ng..].to_vec();
            let mut ek = e.clone();
            for x in ek[ng..].iter_mut() {
                *x = 0;
            }
            let ent = groups.entry(pk).or_default();
            *ent = ent.add(&Poly::monomial(ek, c.clone()));
        }
        let mut keys: Vec<Vec<u32>> = groups.keys().cloned().collect();
        keys.sort_by(|a, b| {
            let da: u32 = a.iter().sum();
            let db: u32 = b.iter().sum();
            db.cmp(&da).then(b.cmp(a))
        });
        let mut out = vec![];
        for pk in keys {
            let k = &groups[&pk];
            let mut full = vec![0; self.n()];
            full[ng..].copy_from_slice(&pk);
            let pm = self.fmt_param_mono(&full, tex);
            let kt = self.k_terms(k, tex);
            if pm.is_empty() {
                out.extend(kt);
            } else if kt.len() == 1 {
                let (neg, coef, gen) = self.k_parts(k, tex).remove(0);
                let coef = if coef == "1" { String::new() } else { coef };
                out.push((neg, Self::compose(&coef, &pm, &gen, tex)));
            } else {
                // führendes Vorzeichen herausziehen
                let lead_neg = kt[0].0;
                let kt2: Vec<(bool, String)> = kt
                    .iter()
                    .map(|(n, b)| (*n != lead_neg, b.clone()))
                    .collect();
                let inner = Self::join_terms(&kt2, tex);
                let body = if tex {
                    format!("\\left({inner}\\right){pm}")
                } else {
                    format!("({inner}){pm}")
                };
                out.push((lead_neg, body));
            }
        }
        out
    }

    pub fn fmt_poly(&self, p: &Poly, tex: bool) -> String {
        Self::join_terms(&self.poly_terms(p, tex), tex)
    }

    fn wrap(&self, p: &Poly, tex: bool) -> String {
        let s = self.fmt_poly(p, tex);
        if self.poly_terms(p, tex).len() > 1 {
            if tex {
                format!("\\left({s}\\right)")
            } else {
                format!("({s})")
            }
        } else {
            s
        }
    }

    pub fn fmt_sym(&self, s: &Sym, tex: bool) -> String {
        if s.den.is_one() {
            return self.fmt_poly(&s.num, tex);
        }
        if tex {
            format!(
                "\\frac{{{}}}{{{}}}",
                self.fmt_poly(&s.num, true),
                self.fmt_poly(&s.den, true)
            )
        } else {
            format!("{}/{}", self.wrap(&s.num, false), self.wrap(&s.den, false))
        }
    }

    pub fn fmt_rad(&self, r: &Radical, tex: bool) -> String {
        let mut factors: Vec<String> = vec![];
        let mut neg = false;
        // Vorfaktor
        let out = &r.outside;
        let mut pre = String::new();
        if out.den.is_one() {
            let t = self.poly_terms(&out.num, tex);
            if t.len() == 1 {
                neg = t[0].0;
                if t[0].1 != "1" || (r.inside.is_none() && r.absm.is_none()) {
                    pre = t[0].1.clone();
                }
            } else if r.inside.is_none() && r.absm.is_none() {
                pre = self.fmt_poly(&out.num, tex);
            } else {
                pre = self.wrap(&out.num, tex);
            }
        } else {
            pre = self.fmt_sym(out, tex);
            if !tex {
                pre = format!("({pre})");
            }
        }
        if !pre.is_empty() {
            factors.push(pre);
        }
        if let Some(a) = &r.absm {
            let s = self.fmt_poly(a, tex);
            factors.push(if tex {
                format!("\\lvert {s}\\rvert")
            } else {
                format!("|{s}|")
            });
        }
        if let Some(i) = &r.inside {
            let s = self.fmt_poly(i, tex);
            if tex {
                factors.push(format!("\\sqrt{{{s}}}"));
            } else if self.poly_terms(i, false).len() > 1 || s.contains(' ') {
                factors.push(format!("√({s})"));
            } else {
                factors.push(format!("√{s}"));
            }
        }
        let body = if tex {
            // Wurzel nach vorn, wenn der Vorfaktor nur Parameter enthält, liest sich besser – wir lassen die Ordnung.
            factors.join("\\,")
        } else {
            let mut s = String::new();
            for (i, f) in factors.iter().enumerate() {
                if i > 0 {
                    let prev_digit = s
                        .chars()
                        .last()
                        .map(|c| c.is_ascii_digit())
                        .unwrap_or(false);
                    let next_digit = f
                        .chars()
                        .next()
                        .map(|c| c.is_ascii_digit())
                        .unwrap_or(false);
                    let fraction = prev_digit
                        && s.rsplit(['·', ' '])
                            .next()
                            .map(|w| w.contains('/'))
                            .unwrap_or(false);
                    if prev_digit && next_digit
                        || fraction
                        || f.starts_with('(') && i > 0 && s.ends_with(')')
                    {
                        s.push('·');
                    }
                }
                s.push_str(f);
            }
            s
        };
        let body = if body.is_empty() {
            "1".to_string()
        } else {
            body
        };
        if neg {
            format!("{}{body}", if tex { "-" } else { "−" })
        } else {
            body
        }
    }

    /// Summe von Radikalen, gleiche Radikanden zusammengefasst.
    pub fn rad_sum(&self, items: &[(Q, Radical)]) -> Vec<Radical> {
        let mut out: Vec<Radical> = vec![];
        for (k, r) in items {
            let scaled = Radical {
                outside: self.sscale(&r.outside, k.clone()),
                absm: r.absm.clone(),
                inside: r.inside.clone(),
            };
            if let Some(existing) = out
                .iter_mut()
                .find(|e| e.inside == scaled.inside && e.absm == scaled.absm)
            {
                existing.outside = self.sadd(&existing.outside, &scaled.outside);
            } else {
                out.push(scaled);
            }
        }
        out.retain(|r| !r.outside.num.is_zero());
        out
    }

    pub fn fmt_rad_sum(&self, rs: &[Radical], tex: bool) -> String {
        if rs.is_empty() {
            return "0".into();
        }
        let mut s = String::new();
        for (i, r) in rs.iter().enumerate() {
            let t = self.fmt_rad(r, tex);
            if i == 0 {
                s.push_str(&t);
            } else if let Some(rest) = t.strip_prefix('−').or_else(|| t.strip_prefix('-')) {
                s.push_str(if tex { " - " } else { " − " });
                s.push_str(rest);
            } else {
                s.push_str(" + ");
                s.push_str(&t);
            }
        }
        s
    }

    pub fn gen_legend(&self, tex: bool) -> Vec<String> {
        self.gens
            .iter()
            .map(|g| {
                let rel = self.fmt_poly(&g.relation, tex);
                let lhs = if tex {
                    format!("{}^{{{}}}", g.def.tex, g.def.degree)
                } else {
                    format!("{}{}", g.def.uni, superscript(g.def.degree))
                };
                if tex {
                    format!("{lhs} = {rel}")
                } else {
                    format!("{}   [{lhs} = {rel}]", g.def.meaning)
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ring(keys: &[&str], params: &[&str]) -> Ring {
        let ks: BTreeSet<String> = keys.iter().map(|s| s.to_string()).collect();
        let g = resolve_gens(&ks, &[]).unwrap();
        Ring::new(
            g,
            params
                .iter()
                .map(|p| ParamInfo {
                    name: p.to_string(),
                    value: 1.0,
                    nonneg: true,
                })
                .collect(),
        )
        .unwrap()
    }

    #[test]
    fn phi_relation() {
        let r = ring(&["phi"], &[]);
        let phi = r.gen_poly("phi").unwrap();
        let p2 = r.mul(&phi, &phi);
        assert_eq!(p2, phi.add(&r.one()));
        let inv = r.inv_k(&phi).unwrap();
        assert_eq!(inv, phi.sub(&r.one()));
    }

    #[test]
    fn psi_values() {
        // 2cos(2π/7) erfüllt y³ + y² − 2y − 1
        assert_eq!(psi(7), vec![qi(-1), qi(-2), qi(1), qi(1)]);
        assert_eq!(psi(5), vec![qi(-1), qi(1), qi(1)]);
        assert_eq!(psi(9), vec![qi(1), qi(-3), qi(0), qi(1)]);
    }

    #[test]
    fn sqrt_denest() {
        let r = ring(&["phi"], &["a"]);
        let phi = r.gen_poly("phi").unwrap();
        let a = r.param_poly("a").unwrap();
        // √((φ+1)·a²) = φ·a
        let x = r.mul(&phi.add(&r.one()), &r.mul(&a, &a));
        let rad = r.sqrt(&r.sym(x));
        assert!(rad.inside.is_none());
        assert_eq!(r.fmt_rad(&rad, false), "aφ");
        // √(12 a²) = 2√3 a
        let x = r.mul(&r.c(qi(12)), &r.mul(&a, &a));
        let rad = r.sqrt(&r.sym(x));
        assert_eq!(r.fmt_rad(&rad, false), "2a√3");
    }

    #[test]
    fn poly_sqrt() {
        let r = ring(&["sqrt2"], &["a", "b"]);
        let a = r.param_poly("a").unwrap();
        let b = r.param_poly("b").unwrap();
        let s2 = r.gen_poly("sqrt2").unwrap();
        // (√2·a + b)² hat die Wurzel √2·a + b
        let q = r.mul(&s2, &a).add(&b);
        let sq = r.psqrt(&r.mul(&q, &q)).unwrap();
        assert_eq!(sq, q);
        // a² + b² ist kein Quadrat
        assert!(r.psqrt(&r.mul(&a, &a).add(&r.mul(&b, &b))).is_none());
        // √(2(a + b)²) = √2·(a + b), denn 2 ist in ℚ(√2) ein Quadrat
        let x = r.mul(&r.c(qi(2)), &r.pow(&a.add(&b), 2));
        assert_eq!(r.fmt_rad(&r.sqrt(&r.sym(x)), false), "a√2 + b√2");
    }

    #[test]
    fn plastic() {
        let r = ring(&["rho"], &[]);
        let rho = r.gen_poly("rho").unwrap();
        let r5 = r.pow(&rho, 5);
        // ρ⁵ = ρ² + ρ + 1
        assert_eq!(r.fmt_poly(&r5, false), "1 + ρ + ρ²");
        let inv = r.inv_k(&rho).unwrap();
        assert!((r.eval(&inv) - 1.0 / 1.324_717_957_244_746).abs() < 1e-12);
        // ρ⁴ ist ein Quadrat in ℚ(ρ): √(ρ⁴) = ρ² — Wurzelerkennung über Konjugierte
        let x = r.pow(&rho, 4);
        let s = r.ksqrt(&x).unwrap();
        assert_eq!(s, r.pow(&rho, 2));
    }
}
