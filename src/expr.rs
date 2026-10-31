//! Ausdrücke: Parser, numerischer Compiler, exakte Auswertung in K(p…), LaTeX.

use crate::algebra::*;
use num_bigint::BigInt;
use num_traits::{One, Signed, Zero};
use std::collections::{BTreeSet, HashMap};
use std::f64::consts::PI;

#[derive(Clone, Debug)]
pub enum Ex {
    Num(Q),
    Var(String),
    Neg(Box<Ex>),
    Bin(char, Box<Ex>, Box<Ex>),
    Call(String, Vec<Ex>),
}

pub const FUNCS: &[&str] = &[
    "sin", "cos", "tan", "asin", "acos", "atan", "sinh", "cosh", "tanh", "exp", "ln", "log",
    "sqrt", "cbrt", "abs", "floor", "ceil", "sign", "min", "max", "atan2", "sec", "csc", "cot",
];

pub const RESERVED: &[&str] = &[
    "pi", "e", "tau", "phi", "rho", "sqrt2", "sqrt3", "sqrt5", "x", "y", "z", "u", "v",
];

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(String),
    Id(String),
    Op(char),
    LP,
    RP,
    Comma,
}

fn tokenize(s: &str) -> Result<Vec<Tok>, String> {
    let cs: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut out = vec![];
    while i < cs.len() {
        let c = cs[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() || (c == '.' && i + 1 < cs.len() && cs[i + 1].is_ascii_digit()) {
            let st = i;
            while i < cs.len() && (cs[i].is_ascii_digit() || cs[i] == '.') {
                i += 1;
            }
            if i < cs.len() && (cs[i] == 'e' || cs[i] == 'E') {
                let save = i;
                let mut j = i + 1;
                if j < cs.len() && (cs[j] == '-' || cs[j] == '+') {
                    j += 1;
                }
                if j < cs.len() && cs[j].is_ascii_digit() {
                    i = j;
                    while i < cs.len() && cs[i].is_ascii_digit() {
                        i += 1;
                    }
                } else {
                    i = save;
                }
            }
            out.push(Tok::Num(cs[st..i].iter().collect()));
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let mapped = match c {
                'φ' | 'ϕ' => Some("phi"),
                'π' => Some("pi"),
                'ρ' => Some("rho"),
                'τ' => Some("tau"),
                'α' => Some("alpha"),
                'β' => Some("beta"),
                'λ' => Some("lambda"),
                'μ' => Some("mu"),
                'θ' => Some("theta"),
                'ω' => Some("omega"),
                _ => None,
            };
            if let Some(m) = mapped {
                out.push(Tok::Id(m.into()));
                i += 1;
                continue;
            }
            let st = i;
            while i < cs.len() && (cs[i].is_ascii_alphanumeric() || cs[i] == '_') {
                i += 1;
            }
            if i == st {
                return Err(format!("unerwartetes Zeichen „{c}“"));
            }
            out.push(Tok::Id(cs[st..i].iter().collect()));
            continue;
        }
        match c {
            '+' | '-' | '*' | '/' | '^' => out.push(Tok::Op(c)),
            '−' => out.push(Tok::Op('-')),
            '·' | '×' | '⋅' => out.push(Tok::Op('*')),
            '÷' => out.push(Tok::Op('/')),
            '²' => {
                out.push(Tok::Op('^'));
                out.push(Tok::Num("2".into()))
            }
            '³' => {
                out.push(Tok::Op('^'));
                out.push(Tok::Num("3".into()))
            }
            '√' => out.push(Tok::Id("sqrt".into())),
            '(' | '[' => out.push(Tok::LP),
            ')' | ']' => out.push(Tok::RP),
            ',' | ';' => out.push(Tok::Comma),
            _ => return Err(format!("unerwartetes Zeichen „{c}“")),
        }
        i += 1;
    }
    Ok(out)
}

fn parse_decimal(s: &str) -> Result<Q, String> {
    let (mant, exp) = match s.find(['e', 'E']) {
        Some(p) => (
            &s[..p],
            s[p + 1..]
                .parse::<i32>()
                .map_err(|_| format!("Zahl „{s}“"))?,
        ),
        None => (s, 0),
    };
    let (ip, fp) = match mant.find('.') {
        Some(p) => (&mant[..p], &mant[p + 1..]),
        None => (mant, ""),
    };
    let digits = format!("{ip}{fp}");
    let digits = if digits.is_empty() {
        "0".to_string()
    } else {
        digits
    };
    let n: BigInt = digits.parse().map_err(|_| format!("Zahl „{s}“"))?;
    let e = exp - fp.len() as i32;
    let ten = BigInt::from(10);
    Ok(if e >= 0 {
        Q::from_integer(n * num_traits::pow(ten, e as usize))
    } else {
        Q::new(n, num_traits::pow(ten, (-e) as usize))
    })
}

struct P {
    t: Vec<Tok>,
    i: usize,
}

impl P {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.i)
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.t.get(self.i).cloned();
        self.i += 1;
        t
    }
    fn expr(&mut self) -> Result<Ex, String> {
        let mut l = self.term()?;
        while let Some(Tok::Op(c)) = self.peek().cloned() {
            if c == '+' || c == '-' {
                self.next();
                let r = self.term()?;
                l = Ex::Bin(c, Box::new(l), Box::new(r));
            } else {
                break;
            }
        }
        Ok(l)
    }
    fn term(&mut self) -> Result<Ex, String> {
        let mut l = self.unary()?;
        loop {
            match self.peek().cloned() {
                Some(Tok::Op(c)) if c == '*' || c == '/' => {
                    self.next();
                    let r = self.unary()?;
                    l = Ex::Bin(c, Box::new(l), Box::new(r));
                }
                Some(Tok::Num(_)) | Some(Tok::Id(_)) | Some(Tok::LP) => {
                    let r = self.power()?;
                    l = Ex::Bin('*', Box::new(l), Box::new(r));
                }
                _ => break,
            }
        }
        Ok(l)
    }
    fn unary(&mut self) -> Result<Ex, String> {
        match self.peek() {
            Some(Tok::Op('-')) => {
                self.next();
                Ok(Ex::Neg(Box::new(self.unary()?)))
            }
            Some(Tok::Op('+')) => {
                self.next();
                self.unary()
            }
            _ => self.power(),
        }
    }
    fn power(&mut self) -> Result<Ex, String> {
        let b = self.primary()?;
        if let Some(Tok::Op('^')) = self.peek() {
            self.next();
            let e = self.unary()?;
            return Ok(Ex::Bin('^', Box::new(b), Box::new(e)));
        }
        Ok(b)
    }
    fn primary(&mut self) -> Result<Ex, String> {
        match self.next() {
            Some(Tok::Num(s)) => Ok(Ex::Num(parse_decimal(&s)?)),
            Some(Tok::Id(name)) => {
                if FUNCS.contains(&name.as_str()) {
                    if let Some(Tok::LP) = self.peek() {
                        self.next();
                        let mut args = vec![];
                        if let Some(Tok::RP) = self.peek() {
                            self.next();
                        } else {
                            loop {
                                args.push(self.expr()?);
                                match self.next() {
                                    Some(Tok::Comma) => continue,
                                    Some(Tok::RP) => break,
                                    _ => {
                                        return Err(format!(
                                            "„)“ erwartet nach Argumenten von {name}"
                                        ))
                                    }
                                }
                            }
                        }
                        return Ok(Ex::Call(name, args));
                    }
                    // Funktion ohne Klammern: √2, sin x
                    let a = self.power()?;
                    return Ok(Ex::Call(name, vec![a]));
                }
                Ok(Ex::Var(name))
            }
            Some(Tok::LP) => {
                let e = self.expr()?;
                match self.next() {
                    Some(Tok::RP) => Ok(e),
                    _ => Err("„)“ fehlt".into()),
                }
            }
            Some(t) => Err(format!("unerwartet: {t:?}")),
            None => Err("Ausdruck endet unerwartet".into()),
        }
    }
}

pub fn parse(s: &str) -> Result<Ex, String> {
    let t = tokenize(s)?;
    if t.is_empty() {
        return Err("leerer Ausdruck".into());
    }
    let mut p = P { t, i: 0 };
    let e = p.expr()?;
    if p.i < p.t.len() {
        return Err(format!("unerwartet am Ende: {:?}", p.t[p.i]));
    }
    Ok(e)
}

pub fn vars_of(e: &Ex, out: &mut BTreeSet<String>) {
    match e {
        Ex::Num(_) => {}
        Ex::Var(v) => {
            out.insert(v.clone());
        }
        Ex::Neg(a) => vars_of(a, out),
        Ex::Bin(_, a, b) => {
            vars_of(a, out);
            vars_of(b, out);
        }
        Ex::Call(_, args) => args.iter().for_each(|a| vars_of(a, out)),
    }
}

// ---------------------------------------------------------------------------
// Numerik
// ---------------------------------------------------------------------------

pub type NFn = Box<dyn Fn(&[f64]) -> f64 + Send + Sync>;

/// Umgebung: Konstanten (Parameterwerte, Generatoren, π, e).
#[derive(Clone, Default)]
pub struct Env {
    pub consts: HashMap<String, f64>,
}

impl Env {
    pub fn new(params: &[(String, f64)], custom_gens: &[GenDef]) -> Env {
        let mut c = HashMap::new();
        c.insert("pi".into(), PI);
        c.insert("tau".into(), 2.0 * PI);
        c.insert("e".into(), std::f64::consts::E);
        for k in ["phi", "sqrt2", "sqrt3", "rho"] {
            c.insert(k.into(), standard_gen(k).unwrap().value);
        }
        c.insert("sqrt5".into(), 5f64.sqrt());
        for g in custom_gens {
            c.insert(g.key.clone(), g.value);
        }
        for (n, v) in params {
            c.insert(n.clone(), *v);
        }
        Env { consts: c }
    }
}

fn is_const(e: &Ex, vars: &[&str]) -> bool {
    let mut s = BTreeSet::new();
    vars_of(e, &mut s);
    !s.iter().any(|v| vars.contains(&v.as_str()))
}

pub fn compile(e: &Ex, vars: &[&str], env: &Env) -> Result<NFn, String> {
    if is_const(e, vars) {
        let v = eval_const(e, env)?;
        return Ok(Box::new(move |_| v));
    }
    Ok(match e {
        Ex::Num(q) => {
            let v = qf(q);
            Box::new(move |_| v)
        }
        Ex::Var(name) => {
            if let Some(i) = vars.iter().position(|v| v == name) {
                Box::new(move |x: &[f64]| x[i])
            } else if let Some(v) = env.consts.get(name) {
                let v = *v;
                Box::new(move |_| v)
            } else {
                return Err(format!("unbekannte Größe „{name}“"));
            }
        }
        Ex::Neg(a) => {
            let f = compile(a, vars, env)?;
            Box::new(move |x| -f(x))
        }
        Ex::Bin(op, a, b) => {
            let f = compile(a, vars, env)?;
            if *op == '^' {
                if let Ex::Num(q) = &**b {
                    if q.is_integer() {
                        let k: i32 = q.numer().try_into().unwrap_or(1);
                        return Ok(match k {
                            2 => Box::new(move |x| {
                                let v = f(x);
                                v * v
                            }),
                            3 => Box::new(move |x| {
                                let v = f(x);
                                v * v * v
                            }),
                            _ => Box::new(move |x| f(x).powi(k)),
                        });
                    }
                }
            }
            let g = compile(b, vars, env)?;
            match op {
                '+' => Box::new(move |x| f(x) + g(x)),
                '-' => Box::new(move |x| f(x) - g(x)),
                '*' => Box::new(move |x| f(x) * g(x)),
                '/' => Box::new(move |x| f(x) / g(x)),
                _ => Box::new(move |x| f(x).powf(g(x))),
            }
        }
        Ex::Call(name, args) => {
            let fs: Vec<NFn> = args
                .iter()
                .map(|a| compile(a, vars, env))
                .collect::<Result<_, _>>()?;
            let need = match name.as_str() {
                "min" | "max" | "atan2" => 2,
                _ => 1,
            };
            if fs.len() != need {
                return Err(format!("{name} erwartet {need} Argument(e)"));
            }
            let mut it = fs.into_iter();
            let f = it.next().unwrap();
            if need == 2 {
                let g = it.next().unwrap();
                match name.as_str() {
                    "min" => Box::new(move |x| f(x).min(g(x))),
                    "max" => Box::new(move |x| f(x).max(g(x))),
                    _ => Box::new(move |x| f(x).atan2(g(x))),
                }
            } else {
                let h: fn(f64) -> f64 = match name.as_str() {
                    "sin" => f64::sin,
                    "cos" => f64::cos,
                    "tan" => f64::tan,
                    "asin" => f64::asin,
                    "acos" => f64::acos,
                    "atan" => f64::atan,
                    "sinh" => f64::sinh,
                    "cosh" => f64::cosh,
                    "tanh" => f64::tanh,
                    "exp" => f64::exp,
                    "ln" | "log" => f64::ln,
                    "sqrt" => f64::sqrt,
                    "cbrt" => f64::cbrt,
                    "abs" => f64::abs,
                    "floor" => f64::floor,
                    "ceil" => f64::ceil,
                    "sign" => f64::signum,
                    "sec" => |v: f64| 1.0 / v.cos(),
                    "csc" => |v: f64| 1.0 / v.sin(),
                    "cot" => |v: f64| 1.0 / v.tan(),
                    _ => return Err(format!("unbekannte Funktion {name}")),
                };
                Box::new(move |x| h(f(x)))
            }
        }
    })
}

pub fn eval_const(e: &Ex, env: &Env) -> Result<f64, String> {
    let f = match e {
        Ex::Num(q) => return Ok(qf(q)),
        Ex::Var(n) => {
            return env
                .consts
                .get(n)
                .copied()
                .ok_or_else(|| format!("unbekannte Größe „{n}“"))
        }
        _ => compile_noconst(e, env)?,
    };
    Ok(f(&[]))
}

fn compile_noconst(e: &Ex, env: &Env) -> Result<NFn, String> {
    // ohne Konstantenfaltung an der Wurzel (sonst Rekursion)
    match e {
        Ex::Neg(a) => {
            let v = eval_const(a, env)?;
            Ok(Box::new(move |_| -v))
        }
        Ex::Bin(op, a, b) => {
            let x = eval_const(a, env)?;
            let y = eval_const(b, env)?;
            let r = match op {
                '+' => x + y,
                '-' => x - y,
                '*' => x * y,
                '/' => x / y,
                _ => x.powf(y),
            };
            Ok(Box::new(move |_| r))
        }
        Ex::Call(..) => {
            // Argumente sind konstant; compile mit leerer Variablenliste würde wieder falten.
            let args: Vec<f64> = match e {
                Ex::Call(_, a) => a
                    .iter()
                    .map(|x| eval_const(x, env))
                    .collect::<Result<_, _>>()?,
                _ => unreachable!(),
            };
            let name = if let Ex::Call(n, _) = e {
                n.clone()
            } else {
                unreachable!()
            };
            let vars = ["__a", "__b"];
            let fake = Ex::Call(
                name,
                (0..args.len()).map(|i| Ex::Var(vars[i].into())).collect(),
            );
            let f = compile(&fake, &vars[..args.len()], env)?;
            let r = f(&args);
            Ok(Box::new(move |_| r))
        }
        _ => {
            let v = eval_const(e, env)?;
            Ok(Box::new(move |_| v))
        }
    }
}

/// Bequem: Ausdruck als Zahl auswerten.
pub fn num(s: &str, env: &Env) -> Result<f64, String> {
    eval_const(&parse(s)?, env)
}

// ---------------------------------------------------------------------------
// Exakt
// ---------------------------------------------------------------------------

fn sqrt_rational_terms(c: &Q) -> Result<(Q, Vec<&'static str>), String> {
    if c.is_negative() {
        return Err("Wurzel aus negativer Zahl".into());
    }
    let nd = c.numer() * c.denom();
    let (s, r) = square_split(&nd);
    let mut r = r;
    let mut keys = vec![];
    for (p, k) in [(2, "sqrt2"), (3, "sqrt3"), (5, "sqrt5")] {
        let bp = BigInt::from(p);
        if (&r % &bp).is_zero() {
            r /= &bp;
            keys.push(k);
        }
    }
    if !r.is_one() {
        return Err(format!(
            "√{} liegt nicht im Zahlkörper – als eigenen Generator definieren",
            r * BigInt::from(1)
        ));
    }
    Ok((Q::new(s, c.denom().clone()), keys))
}

/// Welche Generatoren braucht ein Ausdruck?
pub fn collect_gens(
    e: &Ex,
    env: &Env,
    params: &[String],
    custom: &[GenDef],
    out: &mut BTreeSet<String>,
) -> Result<(), String> {
    match e {
        Ex::Num(_) => Ok(()),
        Ex::Var(v) => {
            if params.contains(v) {
                return Ok(());
            }
            match v.as_str() {
                "phi" | "sqrt2" | "sqrt3" | "rho" => {
                    out.insert(v.clone());
                }
                "sqrt5" => {
                    out.insert("phi".into());
                }
                _ => {
                    if custom.iter().any(|g| &g.key == v) {
                        out.insert(v.clone());
                    }
                }
            }
            Ok(())
        }
        Ex::Neg(a) => collect_gens(a, env, params, custom, out),
        Ex::Bin(_, a, b) => {
            collect_gens(a, env, params, custom, out)?;
            collect_gens(b, env, params, custom, out)
        }
        Ex::Call(name, args) => {
            match name.as_str() {
                "sqrt" if args.len() == 1 => {
                    // rationales Argument?
                    let empty = Ring::new(vec![], vec![]).unwrap();
                    if let Ok(s) = exact(&args[0], &empty) {
                        if let Some(c) = s.num.as_constant() {
                            let c = c / s.den.as_constant().unwrap_or_else(|| qi(1));
                            if let Ok((_, keys)) = sqrt_rational_terms(&c) {
                                for k in keys {
                                    out.insert(if k == "sqrt5" { "phi".into() } else { k.into() });
                                }
                            }
                            return Ok(());
                        }
                    }
                }
                "cos" | "sin" | "tan" if args.len() == 1 => {
                    if let Ok(v) = eval_const(&args[0], env) {
                        if let Some(fr) = rationalize(v / (2.0 * PI), 720, 1e-12) {
                            let nn = fr.denom().clone();
                            let n: u32 = nn.try_into().unwrap_or(1);
                            let (g, s) = cyc_terms(n);
                            for k in terms_keys(&g).into_iter().chain(terms_keys(&s)) {
                                out.insert(k);
                            }
                            return Ok(());
                        }
                    }
                }
                _ => {}
            }
            for a in args {
                collect_gens(a, env, params, custom, out)?;
            }
            Ok(())
        }
    }
}

fn poly1(ring: &Ring, coeffs: &[Q], x: &Poly) -> Poly {
    let mut acc = Poly::zero();
    for c in coeffs.iter().rev() {
        acc = ring.mul(&acc, x).add(&ring.c(c.clone()));
    }
    acc
}

/// cos(2πk/n), sin(2πk/n) exakt.
pub fn cyc_cos_sin(ring: &Ring, k: i64, n: u32) -> (Sym, Sym) {
    let k = k.rem_euclid(n as i64);
    let (gt, st) = cyc_terms(n);
    let gamma = ring.terms(&gt);
    let sigma = ring.terms(&st);
    let c = poly1(ring, &dickson(k as u32), &gamma).scale(&qr(1, 2));
    let s = ring
        .mul(&sigma, &poly1(ring, &cheb_e(k - 1), &gamma))
        .scale(&qr(1, 2));
    (ring.sym(c), ring.sym(s))
}

pub fn exact(e: &Ex, ring: &Ring) -> Result<Sym, String> {
    match e {
        Ex::Num(q) => Ok(ring.sym_c(q.clone())),
        Ex::Var(v) => {
            if let Some(p) = ring.param_poly(v) {
                return Ok(ring.sym(p));
            }
            if v == "sqrt5" {
                let phi = ring.gen_poly("phi").ok_or("φ fehlt im Zahlkörper")?;
                return Ok(ring.sym(phi.scale(&qi(2)).sub(&ring.one())));
            }
            if let Some(g) = ring.gen_poly(v) {
                return Ok(ring.sym(g));
            }
            match v.as_str() {
                "pi" | "tau" | "e" => {
                    Err(format!("{v} ist transzendent – keine exakte Darstellung"))
                }
                _ => Err(format!("unbekannte Größe „{v}“")),
            }
        }
        Ex::Neg(a) => Ok(ring.sneg(&exact(a, ring)?)),
        Ex::Bin(op, a, b) => {
            let x = exact(a, ring)?;
            if *op == '^' {
                let y = exact(b, ring)?;
                let k = y
                    .num
                    .as_constant()
                    .filter(|c| c.is_integer() && y.den.is_one())
                    .ok_or("exakte Potenzen nur mit ganzzahligem Exponenten")?;
                let k: i64 = k.numer().try_into().map_err(|_| "Exponent zu groß")?;
                if k.abs() > 64 {
                    return Err("Exponent zu groß".into());
                }
                return ring.spow(&x, k);
            }
            let y = exact(b, ring)?;
            match op {
                '+' => Ok(ring.sadd(&x, &y)),
                '-' => Ok(ring.ssub(&x, &y)),
                '*' => Ok(ring.smul(&x, &y)),
                _ => ring.sdiv(&x, &y),
            }
        }
        Ex::Call(name, args) => {
            if args.len() != 1 {
                return Err(format!("{name}: nicht exakt darstellbar"));
            }
            match name.as_str() {
                "sqrt" => {
                    let x = exact(&args[0], ring)?;
                    if x.den.is_one() || ring.is_param_free(&x.den) {
                        let x = ring.normalize(x);
                        if let Some(c) = x.num.as_constant() {
                            let (f, keys) = sqrt_rational_terms(&c)?;
                            let mut acc = ring.c(f);
                            for k in keys {
                                let g = if k == "sqrt5" {
                                    ring.gen_poly("phi")
                                        .ok_or("φ fehlt")?
                                        .scale(&qi(2))
                                        .sub(&ring.one())
                                } else {
                                    ring.gen_poly(k).ok_or("Generator fehlt")?
                                };
                                acc = ring.mul(&acc, &g);
                            }
                            return Ok(ring.sym(acc));
                        }
                        if let Some(r) = ring.ksqrt(&x.num) {
                            return Ok(ring.sym(r));
                        }
                    }
                    Err("Wurzel liegt nicht im Zahlkörper (nur numerisch)".into())
                }
                "cos" | "sin" | "tan" => {
                    let env = Env::new(&[], &[]);
                    let v = eval_const(&args[0], &env).map_err(|_| {
                        format!("{name}: Argument muss ein rationales Vielfaches von π sein")
                    })?;
                    let fr = rationalize(v / (2.0 * PI), 720, 1e-12)
                        .ok_or(format!("{name}: kein rationales Vielfaches von π"))?;
                    let k: i64 = fr.numer().try_into().unwrap_or(0);
                    let n: u32 = fr.denom().try_into().unwrap_or(1);
                    let (c, s) = cyc_cos_sin(ring, k, n);
                    match name.as_str() {
                        "cos" => Ok(c),
                        "sin" => Ok(s),
                        _ => ring.sdiv(&s, &c),
                    }
                }
                _ => Err(format!("{name} ist nicht exakt darstellbar")),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// LaTeX
// ---------------------------------------------------------------------------

fn prec(e: &Ex) -> u8 {
    match e {
        Ex::Bin('+', ..) | Ex::Bin('-', ..) => 1,
        Ex::Bin('*', ..) | Ex::Bin('/', ..) => 2,
        Ex::Neg(_) => 3,
        Ex::Bin('^', ..) => 4,
        _ => 5,
    }
}

pub fn tex_name(v: &str) -> String {
    match v {
        "pi" => "\\pi".into(),
        "tau" => "\\tau".into(),
        "phi" => "\\varphi".into(),
        "rho" => "\\rho".into(),
        "sqrt2" => "\\sqrt{2}".into(),
        "sqrt3" => "\\sqrt{3}".into(),
        "sqrt5" => "\\sqrt{5}".into(),
        "alpha" | "beta" | "lambda" | "mu" | "theta" | "omega" => format!("\\{v}"),
        _ if v.chars().count() > 1 => format!("\\mathit{{{v}}}"),
        _ => v.into(),
    }
}

pub fn to_tex(e: &Ex) -> String {
    match e {
        Ex::Num(q) => {
            if q.is_integer() {
                q.numer().to_string()
            } else {
                // Dezimal schöner, wenn endlich
                let f = qf(q);
                let s = format!("{f}");
                if parse_decimal(&s).map(|x| &x == q).unwrap_or(false) {
                    s
                } else {
                    format!("\\tfrac{{{}}}{{{}}}", q.numer(), q.denom())
                }
            }
        }
        Ex::Var(v) => tex_name(v),
        Ex::Neg(a) => {
            let s = to_tex(a);
            if prec(a) <= 2 {
                format!("-\\left({s}\\right)")
            } else {
                format!("-{s}")
            }
        }
        Ex::Bin(op, a, b) => {
            let p = prec(e);
            let wrap = |x: &Ex, strict: bool| {
                let s = to_tex(x);
                if prec(x) < p || (strict && prec(x) == p) {
                    format!("\\left({s}\\right)")
                } else {
                    s
                }
            };
            match op {
                '+' => format!("{} + {}", to_tex(a), to_tex(b)),
                '-' => format!("{} - {}", to_tex(a), wrap(b, true)),
                '*' => {
                    let l = wrap(a, false);
                    let r = wrap(b, false);
                    let needs_dot =
                        matches!(**b, Ex::Num(_)) || r.starts_with(|c: char| c.is_ascii_digit());
                    if needs_dot {
                        format!("{l} \\cdot {r}")
                    } else {
                        format!("{l}\\,{r}")
                    }
                }
                '/' => format!("\\frac{{{}}}{{{}}}", to_tex(a), to_tex(b)),
                _ => {
                    let base = if prec(a) <= 4 || matches!(**a, Ex::Call(..)) {
                        format!("\\left({}\\right)", to_tex(a))
                    } else {
                        to_tex(a)
                    };
                    format!("{base}^{{{}}}", to_tex(b))
                }
            }
        }
        Ex::Call(name, args) => {
            let inner: Vec<String> = args.iter().map(to_tex).collect();
            match name.as_str() {
                "sqrt" => format!("\\sqrt{{{}}}", inner[0]),
                "cbrt" => format!("\\sqrt[3]{{{}}}", inner[0]),
                "abs" => format!("\\left\\lvert {}\\right\\rvert", inner[0]),
                "exp" => format!("e^{{{}}}", inner[0]),
                "sin" | "cos" | "tan" | "sinh" | "cosh" | "tanh" | "exp_" | "ln" | "log"
                | "sec" | "csc" | "cot" | "min" | "max" => {
                    format!("\\{name}\\left({}\\right)", inner.join(", "))
                }
                "asin" => format!("\\arcsin\\left({}\\right)", inner[0]),
                "acos" => format!("\\arccos\\left({}\\right)", inner[0]),
                "atan" => format!("\\arctan\\left({}\\right)", inner[0]),
                _ => format!(
                    "\\operatorname{{{name}}}\\left({}\\right)",
                    inner.join(", ")
                ),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_num() {
        let env = Env::new(&[("a".into(), 2.0)], &[]);
        assert!((num("2a^2 + 1", &env).unwrap() - 9.0).abs() < 1e-12);
        assert!((num("√2·√2", &env).unwrap() - 2.0).abs() < 1e-12);
        assert!((num("-2^2", &env).unwrap() + 4.0).abs() < 1e-12);
        assert!((num("cos(pi)", &env).unwrap() + 1.0).abs() < 1e-12);
    }

    #[test]
    fn exact_trig() {
        let env = Env::new(&[], &[]);
        let e = parse("cos(2*pi/5)").unwrap();
        let mut keys = BTreeSet::new();
        collect_gens(&e, &env, &[], &[], &mut keys).unwrap();
        let ring = Ring::new(resolve_gens(&keys, &[]).unwrap(), vec![]).unwrap();
        let s = exact(&e, &ring).unwrap();
        // cos 72° = (φ − 1)/2
        assert_eq!(ring.fmt_sym(&s, false), "−1/2 + 1/2·φ");
        let e = parse("sqrt(8)").unwrap();
        let mut keys = BTreeSet::new();
        collect_gens(&e, &env, &[], &[], &mut keys).unwrap();
        let ring = Ring::new(resolve_gens(&keys, &[]).unwrap(), vec![]).unwrap();
        assert_eq!(ring.fmt_sym(&exact(&e, &ring).unwrap(), false), "2√2");
    }
}
