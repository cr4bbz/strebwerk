//! Export: Formeln als LaTeX, aktuelle Ansicht als TikZ, Gerüst als STL.

use crate::render::{Item, Prim};
use crate::scene::{Built, Obj};
use eframe::egui::{Color32, Pos2};
use glam::DVec3;
use std::collections::HashMap;
use std::fmt::Write;

fn clean_label(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '₀'..='₉' => out.push(char::from_digit(c as u32 - '₀' as u32, 10).unwrap()),
            'ℓ' => out.push('l'),
            '×' => out.push('x'),
            'ᵢ' | 'ₛ' | 'ₕ' | 'ᵥ' => {}
            '&' | '%' | '$' | '#' | '_' | '{' | '}' => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x250 => out.push(c),
            _ => {}
        }
    }
    out
}

/// LaTeX-Block für ein Objekt (ohne Präambel).
pub fn latex_object(o: &Obj, b: &Built) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "\\subsection*{{{}}}", clean_label(&o.name));
    for d in &b.def_tex {
        let _ = writeln!(s, "\\[ {d} \\]");
    }
    let lines: Vec<String> = b
        .formulas
        .iter()
        .filter(|l| !l.tex.is_empty())
        .map(|l| format!("  {} && \\text{{{}}}", l.tex, clean_label(&l.label)))
        .collect();
    if !lines.is_empty() {
        let _ = writeln!(
            s,
            "\\begin{{align*}}\n{}\n\\end{{align*}}",
            lines.join(" \\\\\n")
        );
    }
    if !b.legend_tex.is_empty() {
        let _ = writeln!(s, "mit ${}$.", b.legend_tex.join(",\\quad "));
    }
    s
}

pub fn latex_document(objs: &[(&Obj, &Built)]) -> String {
    let mut s = String::from(
        "% Erzeugt mit Strebwerk\n\\documentclass[11pt]{article}\n\\usepackage[ngerman]{babel}\n\\usepackage{amsmath,amssymb}\n\\usepackage[a4paper,margin=2cm]{geometry}\n\\begin{document}\n\\section*{Strebwerk: Formeln}\n",
    );
    for (o, b) in objs {
        s.push_str(&latex_object(o, b));
        s.push('\n');
    }
    s.push_str("\\end{document}\n");
    s
}

/// TikZ aus den bereits tiefensortierten Primitiven der aktuellen Ansicht.
pub fn tikz(items: &[Item], width_cm: f64) -> String {
    let mut lo = Pos2::new(f32::MAX, f32::MAX);
    let mut hi = Pos2::new(f32::MIN, f32::MIN);
    let mut ext = |p: &Pos2| {
        lo = lo.min(*p);
        hi = hi.max(*p);
    };
    for it in items {
        match &it.prim {
            Prim::Poly(ps, _) => ps.iter().for_each(&mut ext),
            Prim::Tri(ps, _) => ps.iter().for_each(&mut ext),
            Prim::Line(a, b, _, _) => {
                ext(a);
                ext(b)
            }
            Prim::Dot(p, _, _) | Prim::Text(p, _, _) => ext(p),
        }
    }
    let w = (hi.x - lo.x).max(1.0) as f64;
    let sc = width_cm / w;
    let tx = |p: &Pos2| {
        format!(
            "({:.3},{:.3})",
            (p.x - lo.x) as f64 * sc,
            (hi.y - p.y) as f64 * sc
        )
    };
    let mut colors: HashMap<[u8; 3], String> = HashMap::new();
    let mut defs = String::new();
    let mut body = String::new();
    let mut col = |c: Color32, defs: &mut String| -> (String, f64) {
        let key = [c.r(), c.g(), c.b()];
        let n = colors.len();
        let name = colors
            .entry(key)
            .or_insert_with(|| {
                let nm = format!("sw{n}");
                let _ = writeln!(
                    defs,
                    "\\definecolor{{{nm}}}{{RGB}}{{{},{},{}}}",
                    key[0], key[1], key[2]
                );
                nm
            })
            .clone();
        (name, c.a() as f64 / 255.0)
    };
    // Linienbreiten: Bildschirm-Pixel → pt
    let lw = |w: f32| (w as f64 * 0.45).max(0.2);
    for it in items {
        match &it.prim {
            Prim::Poly(ps, c) => {
                let (n, a) = col(*c, &mut defs);
                let path: Vec<String> = ps.iter().map(tx).collect();
                let _ = writeln!(
                    body,
                    "\\fill[{n}, fill opacity={a:.2}] {} -- cycle;",
                    path.join(" -- ")
                );
            }
            Prim::Tri(ps, cs) => {
                let avg = Color32::from_rgba_unmultiplied(
                    ((cs[0].r() as u32 + cs[1].r() as u32 + cs[2].r() as u32) / 3) as u8,
                    ((cs[0].g() as u32 + cs[1].g() as u32 + cs[2].g() as u32) / 3) as u8,
                    ((cs[0].b() as u32 + cs[1].b() as u32 + cs[2].b() as u32) / 3) as u8,
                    cs[0].a(),
                );
                let (n, a) = col(avg, &mut defs);
                let _ = writeln!(
                    body,
                    "\\fill[{n}, fill opacity={a:.2}] {} -- {} -- {} -- cycle;",
                    tx(&ps[0]),
                    tx(&ps[1]),
                    tx(&ps[2])
                );
            }
            Prim::Line(p, q, w, c) => {
                let (n, a) = col(*c, &mut defs);
                let _ = writeln!(body, "\\draw[{n}, draw opacity={a:.2}, line width={:.2}pt, line cap=round] {} -- {};", lw(*w), tx(p), tx(q));
            }
            Prim::Dot(p, r, c) => {
                let (n, _) = col(*c, &mut defs);
                let _ = writeln!(
                    body,
                    "\\fill[{n}] {} circle ({:.2}pt);",
                    tx(p),
                    *r as f64 * 0.5
                );
            }
            Prim::Text(p, t, c) => {
                let (n, _) = col(*c, &mut defs);
                let _ = writeln!(
                    body,
                    "\\node[{n}, anchor=south west, font=\\scriptsize] at {} {{{}}};",
                    tx(p),
                    clean_label(t)
                );
            }
        }
    }
    format!(
        "% Erzeugt mit Strebwerk – aktuelle Ansicht\n\\documentclass[tikz,border=4mm]{{standalone}}\n\\usepackage{{xcolor}}\n{defs}\\begin{{document}}\n\\begin{{tikzpicture}}\n\\fill[black!88] (0,0) rectangle ({:.3},{:.3});\n{body}\\end{{tikzpicture}}\n\\end{{document}}\n",
        w * sc,
        (hi.y - lo.y) as f64 * sc
    )
}

/// Druckbares Gerüst: Streben als Sechskantstäbe, Knoten als Oktaeder (ASCII-STL, mm).
pub fn stl_struts(name: &str, segs: &[(DVec3, DVec3)], radius: f64, scale: f64) -> String {
    let mut s = format!("solid {name}\n");
    let tri = |s: &mut String, a: DVec3, b: DVec3, c: DVec3| {
        let n = (b - a).cross(c - a).normalize_or_zero();
        let _ = writeln!(
            s,
            " facet normal {:e} {:e} {:e}\n  outer loop\n   vertex {:e} {:e} {:e}\n   vertex {:e} {:e} {:e}\n   vertex {:e} {:e} {:e}\n  endloop\n endfacet",
            n.x, n.y, n.z, a.x, a.y, a.z, b.x, b.y, b.z, c.x, c.y, c.z
        );
    };
    let mut nodes: Vec<DVec3> = vec![];
    let k = 6;
    for (a, b) in segs {
        let (a, b) = (*a * scale, *b * scale);
        let d = b - a;
        if d.length() < 1e-9 {
            continue;
        }
        let u = d.normalize().any_orthonormal_vector();
        let v = d.normalize().cross(u);
        let ring = |c: DVec3| -> Vec<DVec3> {
            (0..k)
                .map(|i| {
                    let t = i as f64 / k as f64 * std::f64::consts::TAU;
                    c + (u * t.cos() + v * t.sin()) * radius
                })
                .collect()
        };
        let (ra, rb) = (ring(a), ring(b));
        for i in 0..k {
            let j = (i + 1) % k;
            tri(&mut s, ra[i], rb[i], rb[j]);
            tri(&mut s, ra[i], rb[j], ra[j]);
            tri(&mut s, a, ra[j], ra[i]);
            tri(&mut s, b, rb[i], rb[j]);
        }
        for p in [a, b] {
            if !nodes.iter().any(|q| (*q - p).length() < radius * 0.1) {
                nodes.push(p);
            }
        }
    }
    let rr = radius * 1.6;
    for p in nodes {
        let ax = [DVec3::X, DVec3::Y, DVec3::Z];
        for sx in [-1.0, 1.0] {
            for sy in [-1.0, 1.0] {
                for sz in [-1.0, 1.0] {
                    let (a, b, c) = (
                        p + ax[0] * sx * rr,
                        p + ax[1] * sy * rr,
                        p + ax[2] * sz * rr,
                    );
                    if sx * sy * sz > 0.0 {
                        tri(&mut s, a, b, c);
                    } else {
                        tri(&mut s, a, c, b);
                    }
                }
            }
        }
    }
    let _ = writeln!(s, "endsolid {name}");
    s
}
