"""Unabhängige Prüfung der Strebwerk-Ausgaben.

Teil A: jede Gleichungskette X = Y = … in den Textausgaben wird geparst und mit 50 Stellen
        bei zufälligen Parameterwerten ausgewertet (Identität), dazu Abgleich mit den von
        Strebwerk angegebenen Zahlwerten [≈ …] bzw. Winkeln.
Teil B: Kernaussagen direkt aus den Koordinaten (numpy/scipy + mpmath), ohne Strebwerk-Formeln.
"""
import json, re, random, sys, itertools
from pathlib import Path
import mpmath as mp
import numpy as np
from scipy.spatial import ConvexHull

mp.mp.dps = 50
OUT = Path(sys.argv[1])
res = json.loads((OUT / "ausgaben.json").read_text())

RHO = mp.findroot(lambda x: x**3 - x - 1, 1.3)
PHI = (1 + mp.sqrt(5)) / 2
SIG10 = 2 * mp.sin(2 * mp.pi / 10)
CONST = {"ρ": RHO, "φ": PHI, "σ₁₀": SIG10}

SUP = dict(zip("⁰¹²³⁴⁵⁶⁷⁸⁹", "0123456789"))
DIG = "0123456789"  # nicht str.isdigit(): das akzeptiert auch ²


def to_py(e: str) -> str:
    """Strebwerk-Unicode → Python-Ausdruck (mit impliziter Multiplikation)."""
    e = e.strip()
    out = []  # Tokens
    i = 0
    while i < len(e):
        c = e[i]
        if c in "  ":
            i += 1
            continue
        if c in DIG:
            j = i
            while j < len(e) and (e[j] in DIG or e[j] == "."):
                j += 1
            out.append(("atom", e[i:j]))
            i = j
            continue
        if e.startswith("σ₁₀", i):
            out.append(("atom", "SIG10"))
            i += 3
            continue
        if c in "ρφ":
            out.append(("atom", {"ρ": "RHO", "φ": "PHI"}[c]))
            i += 1
            continue
        if c.isalpha() and c.isascii():
            out.append(("atom", f"P_{c}"))
            i += 1
            continue
        if c == "√":
            if i + 1 < len(e) and e[i + 1] in DIG:
                j = i + 1
                while j < len(e) and e[j] in DIG:
                    j += 1
                out.append(("atom", f"mp.sqrt({e[i+1:j]})"))
                i = j
            else:
                out.append(("fn", "mp.sqrt"))
                i += 1
            continue
        if c in "⁻⁰¹²³⁴⁵⁶⁷⁸⁹":
            j = i
            neg = False
            if c == "⁻":
                neg = True
                j += 1
            k = j
            while k < len(e) and e[k] in SUP:
                k += 1
            n = "".join(SUP[x] for x in e[j:k])
            out.append(("pow", f"(-{n})" if neg else n))
            i = k
            continue
        if c == "^":
            # ^(5/2), ^(−1/2), ^{…}
            assert e[i + 1] == "(", e
            depth, j = 0, i + 1
            while True:
                if e[j] == "(":
                    depth += 1
                elif e[j] == ")":
                    depth -= 1
                    if depth == 0:
                        break
                j += 1
            inner = e[i + 2:j].replace("−", "-")
            out.append(("pow", f"(mp.mpf({inner.split('/')[0]})/{inner.split('/')[1]})" if "/" in inner else f"({inner})"))
            i = j + 1
            continue
        if c in "+−-":
            out.append(("op", "-" if c in "−-" else "+"))
            i += 1
            continue
        if c in "·*":
            out.append(("op", "*"))
            i += 1
            continue
        if c == "/":
            out.append(("op", "/"))
            i += 1
            continue
        if c == "(":
            out.append(("open", "("))
            i += 1
            continue
        if c == ")":
            out.append(("close", ")"))
            i += 1
            continue
        raise ValueError(f"Zeichen {c!r} in {e!r}")
    s = ""
    prev = None
    for kind, v in out:
        ends_value = prev in ("atom", "close", "pow")
        if kind in ("atom", "fn", "open") and ends_value:
            s += "*"
        if kind == "pow":
            s += f"**{v}"
        elif kind == "fn":
            s += v
        else:
            s += v
        prev = kind
    # Brüche p/q exakt (nicht als Python-float)
    s = re.sub(r"(?<![\w.)])(\d+)/(\d+)", r"(mp.mpf(\1)/\2)", s)
    return s


def ev(e: str, params: dict):
    env = {"mp": mp, "RHO": RHO, "PHI": PHI, "SIG10": SIG10}
    env.update({f"P_{k}": v for k, v in params.items()})
    return mp.mpf(eval(to_py(e), env))


def scene_params(info: str) -> dict:
    return {m.group(1): mp.mpf(m.group(2)) for m in re.finditer(r"^\s+(\w) = ([\d.]+) \(Bereich", info, re.M)}


def random_params(base: dict) -> dict:
    rnd = random.Random(7)
    return {k: mp.mpf(rnd.uniform(0.3, 1.9)) for k in base}


# ---------------------------------------------------------------- Teil A
identities, numerics, skipped = [], [], []


def check_chain(scene, where, chain, params, rparams, num=None, num_kind=None):
    parts = [p.strip() for p in chain.split(" = ")]
    vals_r, vals_s = [], []
    for p in parts:
        try:
            vals_r.append(ev(p, rparams))
            vals_s.append(ev(p, params))
        except Exception as ex:  # nicht parsbar (z. B. Text) → überspringen
            skipped.append((scene, where, p, str(ex)[:60]))
            return
    for k in range(1, len(parts)):
        d = abs(vals_r[k] - vals_r[0]) / max(1, abs(vals_r[0]))
        identities.append((scene, where, parts[0], parts[k], float(d)))
    if num is not None:
        v = vals_s[0]
        if num_kind == "deg":
            ref = mp.cos(mp.radians(num))
            d = abs(abs(v) - abs(ref))
            tol = 2e-6
        else:
            ref = num
            d = abs(v - ref) / max(1, abs(ref))
            tol = 1e-9
        numerics.append((scene, where, parts[0], float(v), float(ref), float(d), d < tol))


for scene, r in res.items():
    params = scene_params(r["info"])
    rparams = random_params(params)
    for line in r["analyze"].splitlines():
        m = re.match(r"\s+(.+?): (.+?)(?:\s+\[≈ ([\-\d.]+)(°?)\])?$", line)
        if not m or " = " not in m.group(2) and m.group(3) is None:
            continue
        label, body, num, deg = m.groups()
        if label.startswith(("Euler", "Flächen", "Symmetrie", "Zahlkörper", "Kugeln", "Hinweis", "Topologie")):
            continue
        body = body.replace("cos θ = ", "")
        chain_params = dict(params)
        check_chain(scene, label, body, params, rparams,
                    mp.mpf(num) if num else None, "deg" if deg else "val")
    for line in r["sections"].splitlines():
        m = re.match(r"\s+(A|U|Länge der Schnittkurve) = (.+)$", line)
        if m:
            check_chain(scene, f"Schnitt {m.group(1)}", m.group(2), params, rparams)
    for line in r["interactions"].splitlines():
        m = re.match(r"\s+∠ ≈ ([\d.]+)°: \|cos ∠\| = (.+)$", line)
        if m:
            body = m.group(2)
            p2 = dict(params)
            if "(bei h = 1)" in body:
                body = body.replace("(bei h = 1)", "").strip()
                p2["h"] = mp.mpf(1)
            check_chain(scene, "Bündelwinkel", body, p2, random_params(p2) if "h" not in params else {**random_params(p2), "h": mp.mpf(1)} if "(bei h = 1)" in m.group(2) else random_params(p2),
                        mp.mpf(m.group(1)), "deg")
            continue
        m = re.match(r"\s+(.+?) von .+ in .+: .+\((\d+)×(?:, je (.+?))?\), Innenlänge gesamt = (.+)$", line)
        if m:
            label, cnt, each, total = m.groups()
            if each:
                check_chain(scene, f"Durchstoßung {label} (je)", each, params, rparams)
            check_chain(scene, f"Durchstoßung {label} (gesamt)", total, params, rparams)
            if each:
                # Konsistenz: gesamt = Anzahl · je
                try:
                    a = ev(each.split(" = ")[0], rparams) * int(cnt)
                    b = ev(total.split(" = ")[0], rparams)
                    identities.append((scene, f"Durchstoßung {label}: gesamt = {cnt}·je", "", "", float(abs(a - b) / max(1, abs(b)))))
                except Exception as ex:
                    skipped.append((scene, label, total, str(ex)[:60]))

# ---------------------------------------------------------------- Teil B
R = float(RHO)
indep = []  # (Szene, Größe, Strebwerk-Formel, Strebwerk-Wert(mp), unabhängiger Wert(float/mp), Abw.)


def add(scene, what, formula, claim, value):
    d = abs(mp.mpf(claim) - mp.mpf(value)) / max(1, abs(mp.mpf(value)))
    indep.append((scene, what, formula, float(claim), float(value), float(d)))


def section_area(pts, n, d):
    """Fläche des Schnitts der konvexen Hülle von pts mit n·x = d (numerisch)."""
    pts = np.array(pts, float)
    n = np.array(n, float)
    ip = []
    for p, q in itertools.combinations(pts, 2):
        fp, fq = p @ n - d, q @ n - d
        if fp * fq < 0:
            ip.append(p + (q - p) * fp / (fp - fq))
        elif abs(fp) < 1e-12:
            ip.append(p)
    if len(ip) < 3:
        return 0.0, 0.0
    ip = np.unique(np.round(np.array(ip), 12), axis=0)
    u = np.cross(n, [1, 0, 0] if abs(n[0]) < 0.9 else [0, 1, 0])
    u /= np.linalg.norm(u)
    v = np.cross(n / np.linalg.norm(n), u)
    xy = np.c_[ip @ u, ip @ v]
    if len(xy) < 3:
        return 0.0, 0.0
    h = ConvexHull(xy)
    return h.volume, h.area  # 2D: volume = Fläche, area = Umfang


def tet_vol(o, a, b, c):
    return abs(np.linalg.det(np.array([a, b, c], float) - np.array(o, float))) / 6


def tri_area(a, b, c):
    a, b, c = (np.array(x, float) for x in (a, b, c))
    return np.linalg.norm(np.cross(b - a, c - a)) / 2


def ang(u, v):
    u, v = np.array(u, float), np.array(v, float)
    return np.degrees(np.arccos(abs(u @ v) / np.linalg.norm(u) / np.linalg.norm(v)))


# S1: Würfel 2ρ³ + Kegel O, a, d, b
O = (0, 0, 0)
A = (1, R, R**3)
B = (1, R**3, R)
D = (R, R**3, 1)
tet = [O, A, D, B]
half = R**3
cube = [(x, y, z) for x in (-half, half) for y in (-half, half) for z in (-half, half)]
S = "Würfel + Kegel"
add(S, "Volumen Kegel", "⅓·ρ⁻¹", RHO**-1 / 3, tet_vol(*tet))
add(S, "Grundfläche (a, d, b)", "½√3·ρ⁻⁴", mp.sqrt(3) / 2 * RHO**-4, tri_area(A, D, B))
add(S, "Seitenfläche (O, d, b)", "½√3·ρ⁻¹", mp.sqrt(3) / 2 * RHO**-1, tri_area(O, D, B))
add(S, "Kante |db|", "√2·ρ⁻⁴", mp.sqrt(2) * RHO**-4, np.linalg.norm(np.subtract(D, B)))
add(S, "Kante |Oa|", "√2·ρ^(5/2)", mp.sqrt(2) * RHO**2.5, np.linalg.norm(A))
for s in (0.25, 0.5, 0.9):
    a_, _ = section_area(tet, (1, 0, 0), s)
    add(S, f"Schnitt Kegel x = {s}", "ρ⁻²·s²", RHO**-2 * mp.mpf(s)**2, a_)
for s in (-1.7, 0.5):
    a_, u_ = section_area(cube, (1, 0, 0), s)
    add(S, f"Schnitt Würfel x = {s}: A", "4·ρ⁶", 4 * RHO**6, a_)
    add(S, f"Schnitt Würfel x = {s}: U", "8·ρ³", 8 * RHO**3, u_)
diags = [(1, 1, 1), (-1, 1, 1), (1, -1, 1), (1, 1, -1)]
struts = [A, D, B, tuple(np.mean(tet, axis=0))]
def cosabs(u, v):
    u, v = np.array(u, float), np.array(v, float)
    return abs(u @ v) / np.linalg.norm(u) / np.linalg.norm(v)


coss = {}
for dg in diags:
    for st in struts:
        c = cosabs(dg, st)
        coss.setdefault(round(c, 9), c)
angles = sorted(coss.values(), reverse=True)
claimed = {"19.9890": ("1/3·√6·ρ^(1/2)", mp.sqrt(6) / 3 * mp.sqrt(RHO)),
           "57.6214": ("1/3·√6·ρ^(−3/2)", mp.sqrt(6) / 3 * RHO**-1.5),
           "66.1561": ("1/3·√6·ρ^(−5/2)", mp.sqrt(6) / 3 * RHO**-2.5)}
for c in angles:
    k = f"{np.degrees(np.arccos(c)):.4f}"
    if k in claimed:
        f, cl = claimed[k]
        add(S, f"Bündelwinkel {k}°: |cos|", f, cl, c)
add(S, "Anzahl verschiedener Bündelwinkel", "8", 8, len(angles))
# Profil (für die Grafik): unabhängig A(s) über den ganzen Bereich
prof = [(s, section_area(tet, (1, 0, 0), s)[0]) for s in np.linspace(0.02, R - 0.01, 40)]

# S4: Kegel über dem Permutationssechseck
S = "Sechseck-Kegel"
P = {"a": (1, R, R**3), "b": (1, R**3, R), "c": (R, 1, R**3), "d": (R, R**3, 1), "e": (R**3, 1, R), "f": (R**3, R, 1)}
for name, tri, f, c in [("Kegel ρ⁻¹", "abc", "⅓·ρ⁻¹", RHO**-1 / 3), ("Kegel 1", "acd", "⅓", mp.mpf(1) / 3),
                        ("Kegel ρ⁴", "abe", "⅓·ρ⁴", RHO**4 / 3), ("Kegel 2+ρ", "ade", "⅓(2 + ρ)", (2 + RHO) / 3)]:
    add(S, f"Volumen {name}", f, c, tet_vol(O, *(P[k] for k in tri)))
for name, tri, f, c in [("Kegel 1", "acd", "½√3·ρ⁻³", mp.sqrt(3) / 2 * RHO**-3), ("Kegel ρ⁴", "abe", "½√3·ρ", mp.sqrt(3) / 2 * RHO)]:
    add(S, f"Grundfläche {name}", f, c, tri_area(*(P[k] for k in tri)))
for k in "abcdef":
    add(S, f"Ebene x+y+z ({k})", "2ρ³", 2 * RHO**3, sum(P[k]))
hexo = "abdfec"
for i in range(6):
    p, q = P[hexo[i]], P[hexo[(i + 1) % 6]]
    f, c = ("√2", mp.sqrt(2)) if i % 2 == 0 else ("√2·ρ⁻⁴", mp.sqrt(2) * RHO**-4)
    add(S, f"Sechseckkante {hexo[i]}{hexo[(i+1)%6]}", f, c, np.linalg.norm(np.subtract(p, q)))

# S2: Quader 1 × ρ × ρ³ in Antiprisma n = 4 – Innenlängen als Funktion von h
S = "Quader + Antiprisma n = 4"


def antiprism(h, n=4):
    return [(np.cos(np.pi * (2 * k + j) / n), np.sin(np.pi * (2 * k + j) / n), (j - 0.5) * h)
            for k in range(n) for j in (0, 1)]


def inner_len(seg, pts):
    hull = ConvexHull(np.array(pts))
    p, q = np.array(seg[0], float), np.array(seg[1], float)
    t0, t1 = 0.0, 1.0
    for eq in hull.equations:  # n·x + d ≤ 0
        nn, dd = eq[:3], eq[3]
        fp, fq = nn @ p + dd, nn @ q + dd
        if fp > 0 and fq > 0:
            return 0.0
        if fp > 0 or fq > 0:
            t = fp / (fp - fq)
            if fp > 0:
                t0 = max(t0, t)
            else:
                t1 = min(t1, t)
    return max(0.0, t1 - t0) * np.linalg.norm(q - p)


hx, hy, hz = 0.5, R / 2, R**3 / 2
corner = lambda sx, sy, sz: (sx * hx, sy * hy, sz * hz)
space = [(corner(sx, sy, 1), corner(-sx, -sy, -1)) for sx in (-1, 1) for sy in (-1, 1)]
f3 = [(corner(sx, -1, -1), corner(sx, 1, 1)) for sx in (-1, 1)] + [(corner(sx, -1, 1), corner(sx, 1, -1)) for sx in (-1, 1)]
spokes = [((0, 0, 0), corner(sx, sy, sz)) for sx in (-1, 1) for sy in (-1, 1) for sz in (-1, 1)]
for h in (1.0, 0.7, 1.3):
    pts = antiprism(h)
    add(S, f"Raumdiagonale je, h = {h}", "√2·ρ^(−1/2)·h", mp.sqrt(2) * RHO**-0.5 * h, inner_len(space[0], pts))
    add(S, f"Flächendiag. 3 je, h = {h}", "ρ^(1/2)·h", mp.sqrt(RHO) * h, inner_len(f3[0], pts))
    add(S, f"Speiche je, h = {h}", "½√2·ρ^(−1/2)·h", mp.sqrt(2) / 2 * RHO**-0.5 * h, inner_len(spokes[0], pts))

hcurve = []
for h in np.linspace(0.1, 3.0, 59):
    pts = antiprism(h)
    hcurve.append((float(h), inner_len(space[0], pts), inner_len(f3[0], pts)))

# ---------------------------------------------------------------- Ausgabe
summary = {
    "identities": len(identities),
    "identities_max": max(d for *_, d in identities),
    "numerics": len(numerics),
    "numerics_fail": [n for n in numerics if not n[-1]],
    "numerics_max": max(n[5] for n in numerics),
    "skipped": len(skipped),
    "indep": len(indep),
    "indep_max": max(d for *_, d in indep),
}
json.dump({"summary": summary, "identities": identities, "numerics": numerics, "skipped": skipped,
           "indep": indep, "profile": prof, "hcurve": hcurve}, open(OUT / "pruefung.json", "w"), ensure_ascii=False, indent=1, default=str)
print(json.dumps(summary, indent=1, default=str))
print("Übersprungen (Beispiele):")
for s_ in skipped[:12]:
    print("  ", s_)
bad = [i for i in identities if i[-1] > 1e-30]
print("Identitäten > 1e-30:", bad[:10])
print("Unabhängig > 1e-12:", [i for i in indep if i[-1] > 1e-12][:10])
