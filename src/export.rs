//! Export: Formeln als LaTeX, aktuelle Ansicht als TikZ, Gerüst als STL.

use crate::analysis::{self, InterRes, Profile, SectionRes};
use crate::render::{Item, Prim};
use crate::scene::{Built, Obj, Scene};
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
            '<' => out.push_str("\\textless{}"),
            '>' => out.push_str("\\textgreater{}"),
            c if (c as u32) < 0x250 => out.push(c),
            _ => {}
        }
    }
    out
}

/// Unicode-Text (wie er in der Oberfläche steht) für LaTeX-Textmodus aufbereiten.
pub fn tex_text(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        let m = |x: &str| format!("\\ensuremath{{{x}}}");
        let piece = match c {
            '&' | '%' | '$' | '#' | '_' | '{' | '}' => format!("\\{c}"),
            '\\' => "\\textbackslash{}".into(),
            '<' => "\\textless{}".into(),
            '>' => "\\textgreater{}".into(),
            '^' => "\\^{}".into(),
            '~' => "\\~{}".into(),
            'χ' => m("\\chi"),
            'π' => m("\\pi"),
            'φ' => m("\\varphi"),
            'ρ' => m("\\rho"),
            'κ' => m("\\kappa"),
            'τ' => m("\\tau"),
            'α' => m("\\alpha"),
            'β' => m("\\beta"),
            'θ' => m("\\theta"),
            'γ' => m("\\gamma"),
            'σ' => m("\\sigma"),
            'λ' => m("\\lambda"),
            'μ' => m("\\mu"),
            'Σ' => m("\\Sigma"),
            '√' => {
                // √(…) oder √x
                if chars.peek() == Some(&'(') {
                    chars.next();
                    let mut depth = 1;
                    let mut inner = String::new();
                    for d in chars.by_ref() {
                        match d {
                            '(' => depth += 1,
                            ')' => {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                            _ => {}
                        }
                        inner.push(d);
                    }
                    format!("\\ensuremath{{\\sqrt{{\\text{{{}}}}}}}", tex_text(&inner))
                } else {
                    let mut inner = String::new();
                    while let Some(d) = chars.peek() {
                        if d.is_ascii_alphanumeric() || "φρσγα".contains(*d) {
                            inner.push(*d);
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    format!("\\ensuremath{{\\sqrt{{\\text{{{}}}}}}}", tex_text(&inner))
                }
            }
            '·' | '⋅' => m("\\cdot "),
            '×' => m("\\times "),
            '≈' => m("\\approx "),
            '≤' => m("\\leq "),
            '≥' => m("\\geq "),
            '⇒' => m("\\Rightarrow "),
            '→' => m("\\to "),
            '↔' => m("\\leftrightarrow "),
            '∫' => m("\\int "),
            '∬' => m("\\iint "),
            '−' => m("-"),
            '–' => "--".into(),
            '°' => m("^\\circ"),
            'ℝ' => m("\\mathbb{R}"),
            'ℓ' => m("\\ell"),
            '²' => m("^2"),
            '³' => m("^3"),
            '⁴' => m("^4"),
            '⁵' => m("^5"),
            '⁶' => m("^6"),
            'ᶠ' => m("^f"),
            '₀'..='₉' => m(&format!("_{}", c as u32 - '₀' as u32)),
            'ₕ' => m("_h"),
            'ᵥ' => m("_v"),
            'ₛ' => m("_s"),
            'ᵢ' => m("_i"),
            '⅓' => m("\\tfrac13"),
            '…' => "\\ldots{}".into(),
            '„' => ",,".into(),
            '“' => "``".into(),
            c if (c as u32) < 0x250 => c.to_string(),
            _ => String::new(),
        };
        out.push_str(&piece);
    }
    out
}

/// LaTeX-Block für ein Objekt (ohne Präambel): Formelzeilen im align*-Block,
/// reine Textzeilen (Topologie, Symmetrie, Fáry–Milnor …) als Absätze an ihrer Stelle.
pub fn latex_object(o: &Obj, b: &Built) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "\\subsection*{{{}}}", clean_label(&o.name));
    for d in &b.def_tex {
        let _ = writeln!(s, "\\[ {d} \\]");
    }
    let mut block: Vec<String> = vec![];
    let flush = |s: &mut String, block: &mut Vec<String>| {
        if !block.is_empty() {
            let _ = writeln!(
                s,
                "\\begin{{align*}}\n{}\n\\end{{align*}}",
                block.join(" \\\\\n")
            );
            block.clear();
        }
    };
    for l in &b.formulas {
        // die Definition steht schon als abgesetzte Formel darüber
        if l.label == "Definition" && !b.def_tex.is_empty() {
            continue;
        }
        if l.tex.is_empty() {
            flush(&mut s, &mut block);
            let _ = writeln!(
                s,
                "\\noindent\\textbf{{{}:}} {}\\par",
                clean_label(&l.label),
                tex_text(&l.uni)
            );
        } else {
            block.push(format!(
                "  {} && \\text{{{}}}",
                l.tex,
                clean_label(&l.label)
            ));
            // Klassifikation zur Topologie zusätzlich als Text
            if l.label.starts_with("Topologie") || l.label.starts_with("Geschlossene") {
                flush(&mut s, &mut block);
                let _ = writeln!(s, "\\noindent {}\\par", tex_text(&l.uni));
            }
        }
    }
    flush(&mut s, &mut block);
    if !b.errors.is_empty() {
        let _ = writeln!(
            s,
            "\\noindent\\emph{{Fehler: {}}}\\par",
            tex_text(&b.errors.join("; "))
        );
    }
    if !b.legend_tex.is_empty() {
        let _ = writeln!(
            s,
            "\\noindent mit ${}$.\\par",
            b.legend_tex.join(",\\quad ")
        );
    }
    s
}

fn latex_sections(scene: &Scene, secs: &[SectionRes], profs: &[Profile]) -> String {
    let mut s = String::new();
    if scene.sections.iter().all(|e| !e.visible) {
        return s;
    }
    let _ = writeln!(s, "\\section*{{Schnittebenen}}");
    for (pi, sec) in scene.sections.iter().enumerate().filter(|(_, e)| e.visible) {
        let _ = writeln!(
            s,
            "\\subsection*{{{}: ${}$}}",
            clean_label(&sec.name),
            analysis::plane_text(sec, true)
        );
        let rs: Vec<&SectionRes> = secs.iter().filter(|r| r.plane == pi).collect();
        if rs.is_empty() {
            let _ = writeln!(s, "Die Ebene trifft kein sichtbares Objekt.\\par");
        }
        for r in rs {
            let shape = if r.exact {
                format!("exaktes {}-Eck", r.ngon)
            } else if r.loops > 1 {
                format!("{} Schnittkurven, numerisch", r.loops)
            } else {
                "numerisch".to_string()
            };
            // „&= x“ bzw. „&\approx x“, nie „= ≈“
            let rel = |t: &str| {
                if t.starts_with("\\approx") {
                    format!("&{t}")
                } else {
                    format!("&= {t}")
                }
            };
            let body = if r.closed {
                format!("  A {} \\\\\n  U {}", rel(&r.area_tex), rel(&r.perim_tex))
            } else {
                format!(
                    "  L {} && \\text{{Länge der Schnittkurve{}}}",
                    rel(&r.perim_tex),
                    if r.loops > 1 { "n" } else { "" }
                )
            };
            let _ = writeln!(
                s,
                "\\paragraph{{{}}} ({})\n\\begin{{align*}}\n{}\n\\end{{align*}}",
                clean_label(&r.obj),
                shape,
                body
            );
            if r.exact && !r.points.is_empty() {
                let pts: Vec<String> = r
                    .points
                    .iter()
                    .take(12)
                    .map(|p| format!("${}$", p.tex))
                    .collect();
                let more = if r.points.len() > 12 {
                    format!(", \\ldots{{}} ({} insgesamt)", r.points.len())
                } else {
                    String::new()
                };
                let _ = writeln!(
                    s,
                    "\\noindent Ecken des Schnittpolygons: {}{}.\\par",
                    pts.join(", "),
                    more
                );
                let _ = writeln!(s, "\\noindent\\emph{{Gültig, solange die Ebene dieselben Kanten schneidet (gleicher kombinatorischer Typ).}}\\par");
            }
            if !r.note.is_empty() {
                let _ = writeln!(s, "\\noindent\\emph{{{}}}\\par", tex_text(&r.note));
            }
        }
        for p in profs.iter().filter(|p| p.plane == pi) {
            let coords: Vec<String> = p
                .samples
                .iter()
                .map(|(h, a)| format!("({h:.5},{a:.5})"))
                .collect();
            let rel = if p.volume > 0.0 {
                (p.integral - p.volume).abs() / p.volume * 100.0
            } else {
                0.0
            };
            let _ = writeln!(
                s,
                "\\paragraph{{Cavalieri-Profil {}}}\n\\begin{{center}}\\begin{{tikzpicture}}\n\\begin{{axis}}[width=0.8\\linewidth,height=5cm,xlabel={{$h$}},ylabel={{$A(h)$}},grid=major,thick]\n\\addplot[blue!70!black,mark=none] coordinates {{{}}};\n\\addplot[red!70!black,dashed] coordinates {{({:.5},0) ({:.5},{:.5})}};\n\\end{{axis}}\n\\end{{tikzpicture}}\\end{{center}}\n\\noindent $\\int A(h)\\,dh \\approx {:.6}$, Volumen aus dem Netz $\\approx {:.6}$ (Abweichung {:.3}\\,\\%).\\par",
                clean_label(&p.obj),
                coords.join(" "),
                p.current,
                p.current,
                p.samples.iter().map(|x| x.1).fold(0.0, f64::max) * 1.05,
                p.integral,
                p.volume,
                rel
            );
        }
    }
    s
}

fn latex_interactions(inters: &[InterRes]) -> String {
    let mut s = String::new();
    if inters.is_empty() {
        return s;
    }
    let _ = writeln!(s, "\\section*{{Interaktionen der Verstrebungen}}");
    for r in inters {
        let _ = writeln!(
            s,
            "\\subsection*{{{} $\\leftrightarrow$ {}}}",
            clean_label(&r.a),
            clean_label(&r.b)
        );
        let _ = writeln!(
            s,
            "\\noindent {}\\par",
            if r.exact {
                "Exakt im gemeinsamen Zahlkörper geprüft."
            } else {
                "Numerisch (mindestens ein Objekt ohne exakte Koordinaten)."
            }
        );
        if r.is_empty() {
            match &r.near {
                Some((d, a, b)) => {
                    let _ = writeln!(s, "\\noindent Keine Berührung; kleinster Abstand $\\approx {d:.6}$ zwischen {} und {}.\\par", tex_text(a), tex_text(b));
                }
                None => {
                    let _ = writeln!(s, "\\noindent Keine Berührung.\\par");
                }
            }
        }
        let mut it = String::new();
        if !r.shared.is_empty() {
            let pts: Vec<String> = r
                .shared
                .iter()
                .take(12)
                .map(|p| format!("${}$", p.tex))
                .collect();
            let _ = writeln!(
                it,
                "\\item \\textbf{{Gemeinsame Knoten}} ({}): {}",
                r.shared.len(),
                pts.join(", ")
            );
        }
        for g in analysis::crossing_groups(&r.crossings) {
            let pts: Vec<String> = g.6.iter().take(6).map(|p| format!("${}$", p.tex)).collect();
            let _ = writeln!(
                it,
                "\\item \\textbf{{{}}} {} $\\times$ {} ({}$\\times$): $\\lvert\\cos\\angle\\rvert = {}$, $\\angle \\approx {:.4}^\\circ$; bei {}{}",
                g.2,
                tex_text(&g.0),
                tex_text(&g.1),
                g.6.len(),
                g.4,
                g.5,
                pts.join(", "),
                if g.6.len() > 6 { ", \\ldots" } else { "" }
            );
        }
        for p in &r.pierces {
            let each = p
                .each_tex
                .as_ref()
                .map(|e| format!(", je ${e}$"))
                .unwrap_or_default();
            let _ = writeln!(
                it,
                "\\item \\textbf{{{}}} von {} in {}: {} ({}$\\times${}), Innenlänge gesamt ${}$",
                tex_text(&p.label),
                clean_label(&p.owner),
                clean_label(&p.body),
                tex_text(p.kind),
                p.count,
                each,
                p.total_tex
            );
        }
        for (a, b, l) in &r.overlaps {
            let _ = writeln!(
                it,
                "\\item \\textbf{{Überlappung}} {} / {}: gemeinsame Länge $\\approx {l:.6}$",
                tex_text(a),
                tex_text(b)
            );
        }
        if let (Some((d, a, b)), false) = (&r.near, r.is_empty()) {
            let _ = writeln!(
                it,
                "\\item Kleinster Abstand nicht berührender Streben $\\approx {d:.6}$ ({} / {})",
                tex_text(a),
                tex_text(b)
            );
        }
        if r.near_misses > 0 {
            let _ = writeln!(
                it,
                "\\item {} numerische Beinahe-Treffer, exakt widerlegt",
                r.near_misses
            );
        }
        for n in &r.notes {
            let _ = writeln!(it, "\\item \\emph{{{}}}", tex_text(n));
        }
        if !it.is_empty() {
            let _ = writeln!(s, "\\begin{{itemize}}\n{it}\\end{{itemize}}");
        }
    }
    s
}

pub fn latex_document(
    objs: &[(&Obj, &Built)],
    scene: &Scene,
    secs: &[SectionRes],
    profs: &[Profile],
    inters: &[InterRes],
) -> String {
    let mut s = String::from(
        "% Erzeugt mit Strebwerk\n\\documentclass[11pt]{article}\n\\usepackage[ngerman]{babel}\n\\usepackage{amsmath,amssymb}\n\\usepackage[a4paper,margin=2cm]{geometry}\n",
    );
    if !profs.is_empty() {
        s.push_str("\\usepackage{pgfplots}\n\\pgfplotsset{compat=1.18}\n");
    }
    s.push_str("\\begin{document}\n\\section*{Strebwerk: Formeln}\n");
    for (o, b) in objs {
        s.push_str(&latex_object(o, b));
        s.push('\n');
    }
    s.push_str(&latex_sections(scene, secs, profs));
    s.push_str(&latex_interactions(inters));
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
