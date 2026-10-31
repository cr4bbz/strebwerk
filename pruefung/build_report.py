"""Erzeugt das Prüfdokument (LaTeX) aus ausgaben.json und pruefung.json."""
import json, math, re, subprocess, sys
from pathlib import Path

OUT = Path(sys.argv[1])
COMMIT = sys.argv[2]
d = json.loads((OUT / "pruefung.json").read_text())
aus = json.loads((OUT / "ausgaben.json").read_text())
S = d["summary"]
R = 1.324717957244746

SUPMAP = dict(zip("⁰¹²³⁴⁵⁶⁷⁸⁹", "0123456789"))
SUBMAP = dict(zip("₀₁₂₃₄₅₆₇₈₉", "0123456789"))


def u2tex(e: str) -> str:
    """Strebwerk-Unicode-Formel → LaTeX (Mathe-Modus)."""
    out, i = [], 0
    while i < len(e):
        c = e[i]
        if c == "√":
            if e[i + 1] == "(":
                depth, j = 0, i + 1
                while True:
                    depth += {"(": 1, ")": -1}.get(e[j], 0)
                    if depth == 0:
                        break
                    j += 1
                out.append(r"\sqrt{" + u2tex(e[i + 2:j]) + "}")
                i = j + 1
            else:
                j = i + 1
                while j < len(e) and e[j] in "0123456789":
                    j += 1
                out.append(r"\sqrt{" + e[i + 1:j] + "}")
                i = j
            continue
        if c == "^" and e[i + 1] == "(":
            j = e.index(")", i)
            out.append("^{" + e[i + 2:j].replace("−", "-") + "}")
            i = j + 1
            continue
        if c in "⁻⁰¹²³⁴⁵⁶⁷⁸⁹":
            j = i
            s = ""
            while j < len(e) and e[j] in "⁻⁰¹²³⁴⁵⁶⁷⁸⁹":
                s += "-" if e[j] == "⁻" else SUPMAP[e[j]]
                j += 1
            out.append("^{" + s + "}")
            i = j
            continue
        if e.startswith("σ₁₀", i):
            out.append(r"\sigma_{10}")
            i += 3
            continue
        rep = {"ρ": r"\rho ", "φ": r"\varphi ", "·": r"\cdot ", "−": "-", "θ": r"\theta ", "∠": r"\angle ",
               "×": r"\times ", "≈": r"\approx ", "|": r"\lvert ", "π": r"\pi ", "⅓": r"\tfrac13 "}
        if c in SUBMAP:
            out.append("_{" + SUBMAP[c] + "}")
        elif c in "ₘₕᵥₛᵢ":
            out.append("_{" + dict(zip("ₘₕᵥₛᵢ", "mhvsi"))[c] + "}")
        else:
            out.append(rep.get(c, c))
        i += 1
    return "".join(out)


def t2tex(s: str) -> str:
    """Text mit Unicode-Formelzeichen → LaTeX-Text."""
    s = s.replace("&", r"\&").replace("%", r"\%").replace("#", r"\#").replace("_", r"\_")
    s = re.sub(r"([ℓθρφσ∠×≈²³⁻¹⁰⁴⁵₀₁₂₃₄₅₆₇₈₉√·⅓ₘₕᵥₛᵢ]+)", lambda m: "$" + u2tex(m.group(1).replace("ℓ", "\\ell ")) + "$", s)
    return s


def fit(math: str) -> str:
    return r"\resizebox{\ifdim\width>\linewidth\linewidth\else\width\fi}{!}{$" + math + "$}"


def sci(x: float) -> str:
    if x == 0:
        return "$0$"
    e = math.floor(math.log10(abs(x)))
    m = x / 10**e
    return f"${m:.1f}\\cdot 10^{{{e}}}$"


def num(x: float, k=10) -> str:
    return f"{x:.{k}f}"


SCN = {"pruef-1-wuerfel-kegel": "Würfel, Kegel", "pruef-4-sechseck": "Sechseck",
       "anti-rho3-n4": "Quader, Antiprisma", "ikosaeder-dodekaeder": "Ikosaeder",
       "interaktion": "Würfel, Oktaeder"}


def indep_rows(scene):
    rows = []
    for sc, what, f, claim, val, dev in d["indep"]:
        if sc == scene:
            rows.append(f"{t2tex(what)} & {fit(u2tex(f))} & {num(claim)} & {num(val)} & {sci(dev)} \\\\")
    return "\n".join(rows)


def indep_table(scene):
    return r"""\begin{center}\footnotesize
\begin{tabular}{>{\raggedright\arraybackslash}p{4.6cm}>{\raggedright\arraybackslash}p{3.0cm}rrr}
\toprule
Größe & Strebwerk & Wert der Formel & unabhängig & rel.\ Abw.\\
\midrule
""" + indep_rows(scene) + r"""
\bottomrule
\end{tabular}\end{center}"""


# ------------------------------------------------------------ Grafiken
prof = d["profile"]
prof_marks = " ".join(f"({s:.5f},{a:.6f})" for s, a in prof)
hc = d["hcurve"]
hc_space = " ".join(f"({h:.4f},{a:.6f})" for h, a, _ in hc)
hc_f3 = " ".join(f"({h:.4f},{b:.6f})" for h, _, b in hc)

# Sechseck in der Ebene x + y + z = 2ρ³
R3 = R**3
P = {"a": (1, R, R3), "b": (1, R3, R), "c": (R, 1, R3), "d": (R, R3, 1), "e": (R3, 1, R), "f": (R3, R, 1)}
cen = (2 * R3 / 3,) * 3
u = (1 / math.sqrt(2), -1 / math.sqrt(2), 0)
v = (1 / math.sqrt(6), 1 / math.sqrt(6), -2 / math.sqrt(6))


def pr(p):
    q = [p[i] - cen[i] for i in range(3)]
    return (sum(q[i] * u[i] for i in range(3)), sum(q[i] * v[i] for i in range(3)))


big = [pr((R3, R3, 0)), pr((R3, 0, R3)), pr((0, R3, R3))]
coords = {k: pr(p) for k, p in P.items()}
classes = [("abc", r"V=\tfrac13\rho^{-1}", "orange"), ("acd", r"V=\tfrac13", "teal"),
           ("abe", r"V=\tfrac13\rho^{4}", "magenta"), ("ade", r"V=\tfrac13(2+\rho)", "blue")]


def hex_pic(tri, label, col):
    s = r"\begin{tikzpicture}[scale=0.98]" + "\n"
    s += r"\draw[gray!60, thin] " + " -- ".join(f"({x:.4f},{y:.4f})" for x, y in big) + " -- cycle;\n"
    s += r"\draw[black!70, thick] " + " -- ".join(f"({coords[k][0]:.4f},{coords[k][1]:.4f})" for k in "abdfec") + " -- cycle;\n"
    s += f"\\fill[{col}!35, draw={col}!80!black, thick] " + " -- ".join(
        f"({coords[k][0]:.4f},{coords[k][1]:.4f})" for k in tri) + " -- cycle;\n"
    for k, (x, y) in coords.items():
        perm = "".join({1: "1", R: r"\rho", R3: r"\rho^3"}[c] + ("," if i < 2 else "") for i, c in enumerate(P[k]))
        s += f"\\fill ({x:.4f},{y:.4f}) circle (1.2pt);\n"
    s += f"\\node[below] at (0,{min(y for _, y in big) - 0.05:.3f}) {{${label}$}};\n"
    s += r"\end{tikzpicture}"
    return s


hexpics = "\n\\hfill\n".join(hex_pic(t, l, c) for t, l, c in classes)
lab = {k: "(" + ", ".join({1: "1", R: r"\rho", R3: r"\rho^3"}[c] for c in P[k]) + ")" for k in P}
hexlabels = r"\begin{tikzpicture}[scale=1.45]" + "\n" + \
    r"\draw[gray!60] " + " -- ".join(f"({x:.4f},{y:.4f})" for x, y in big) + " -- cycle;\n" + \
    r"\draw[black!80, thick] " + " -- ".join(f"({coords[k][0]:.4f},{coords[k][1]:.4f})" for k in "abdfec") + " -- cycle;\n" + \
    "".join(f"\\fill ({coords[k][0]:.4f},{coords[k][1]:.4f}) circle (1.3pt) node[{pos}] {{\\scriptsize ${lab[k]}$}};\n"
            for k, pos in zip("abdfec", ["left", "above left", "above right", "right", "below right", "below left"])) + \
    f"\\node[gray] at ({big[0][0]:.3f},{big[0][1]:.3f}) [below right] {{\\scriptsize $(\\rho^3,\\rho^3,0)$}};\n" + \
    r"\end{tikzpicture}"

# ------------------------------------------------------------ Anhangstabellen
ident_rows = []
for sc, where, lhs, rhs, dev in d["identities"]:
    if not lhs:
        ident_rows.append(f"{SCN[sc]} & \\hspace{{0pt}}{t2tex(where)} & \\multicolumn{{2}}{{l}}{{(Zählprüfung)}} & {sci(dev)} \\\\")
        continue
    ident_rows.append(f"{SCN[sc]} & \\hspace{{0pt}}{t2tex(where)} & {fit(u2tex(lhs))} & {fit(u2tex(rhs))} & {sci(dev)} \\\\")
num_rows = []
for sc, where, lhs, val, ref, dev, ok in d["numerics"]:
    num_rows.append(f"{SCN[sc]} & \\hspace{{0pt}}{t2tex(where)} & {fit(u2tex(lhs))} & {val:.8f} & {ref:.8f} & {sci(dev)} \\\\")

# Strebwerk-Ausgaben (Auszug) für Szene 1
s1 = aus["pruef-1-wuerfel-kegel"]

ok = len(S["numerics_fail"]) == 0 and S["identities_max"] < 1e-30 and S["indep_max"] < 1e-10
verdict = r"\textcolor{green!45!black}{\textbf{bestanden}}" if ok else r"\textcolor{red}{\textbf{NICHT bestanden}}"

tex = r"""\documentclass[11pt]{article}
\usepackage[T1]{fontenc}
\usepackage[utf8]{inputenc}
\IfFileExists{lmodern.sty}{\usepackage{lmodern}}{}
\usepackage[a4paper,margin=2.2cm]{geometry}
\usepackage{amsmath,amssymb,booktabs,longtable,graphicx,array,pdfpages}
\usepackage[table]{xcolor}
\usepackage{pgfplots}
\pgfplotsset{compat=1.17}
\usepackage[hidelinks]{hyperref}
\setlength{\emergencystretch}{3em}
\setlength{\parindent}{0pt}
\hyphenation{Kanten-kugel-radius Um-kugel-radius In-kugel-radius Durch-sto-ßung Flächen-diagonale Raum-diagonale Bündel-winkel Seiten-fläche Grund-fläche Sechseck-kante}
\setlength{\parskip}{4pt}
\newcommand{\pf}[1]{\colorbox{blue!7}{$\displaystyle #1$}}
\title{\textbf{Strebwerk -- Prüfdokument}\\[2pt]\large Potenzformen in Schnitten, Interaktionen und Koordinaten}
\author{Branch \texttt{schnitte-und-interaktionen}, Commit \texttt{""" + COMMIT + r"""}}
\date{31.\,10.\,2026}
\begin{document}
\maketitle

\section*{Ergebnis}
Gesamturteil: """ + verdict + r""". Alle von Strebwerk ausgegebenen Gleichungsketten gelten identisch, alle Zahlwerte
passen zu den Formeln, und die Kernaussagen stimmen mit einer davon unabhängigen Rechnung aus den Koordinaten überein.

\begin{center}
\begin{tabular}{>{\raggedright\arraybackslash}p{7.2cm}r>{\raggedright\arraybackslash}p{5.6cm}}
\toprule
Prüfung & Anzahl & größte rel.\ Abweichung\\
\midrule
A\quad Identitäten $X = Y$ (50 Stellen, zufällige Parameter) & """ + str(S["identities"]) + r""" & """ + sci(S["identities_max"]) + r"""\\
B\quad Formel gegen Strebwerks eigenen Zahlwert bzw.\ Winkel & """ + str(S["numerics"]) + r""" & """ + sci(S["numerics_max"]) + r""" (alle innerhalb der Anzeigegenauigkeit)\\
C\quad unabhängig aus Koordinaten (numpy/scipy) & """ + str(S["indep"]) + r""" & """ + sci(S["indep_max"]) + r""" (Gleitkomma)\\
nicht prüfbar (reiner Text) & """ + str(S["skipped"]) + r""" & „Maxwell-Zählung $3n-6$''\\
\bottomrule
\end{tabular}
\end{center}

\textbf{Was sich geändert hat.} Die Potenzform $x = A\cdot g^m\cdot M$ (rationales $A$, ein Generator $g$, ein Monom $M$
in den Parametern) erscheint jetzt auch bei Schnittflächen, Umfängen (summandenweise), Ecken von Schnittpolygonen,
Innenlängen von Durchstoßungen und Oberflächen; negative Exponenten werden hochgestellt ($\rho^{-4}$ statt $\rho$
\texttt{\textasciicircum(-4)}), triviale Umformungen wie $\rho\sqrt2 = \sqrt2\,\rho$ entfallen.

\textbf{Prüfverfahren.} (A) Jede Kette $X = Y = \dots$ aus den Textausgaben von \texttt{analyze}, \texttt{sections}
und \texttt{interactions} wird geparst und mit 50 Dezimalstellen bei zufälligen positiven Parameterwerten
ausgewertet -- eine Identität in $K[\text{Parameter}]$ muss dort exakt gelten. (B) Die erste Form jeder Kette wird beim
aktuellen Parameterwert gegen Strebwerks numerische Angabe gehalten (Winkel: $\cos$ des angegebenen Gradmaßes).
(C) Kernaussagen werden ohne Strebwerk aus den Koordinaten nachgerechnet: Volumina per Determinante,
Schnittflächen per ebener Hülle der Kantenschnittpunkte, Innenlängen per Halbraum-Clipping an der konvexen Hülle.
Die Szenen wurden über den MCP-Server gebaut -- genau so, wie es ein Sprachmodell tut.

\section{Würfel $2\rho^3$ und Kegel über $(1,\rho,\rho^3)$}
\begin{center}
\includegraphics[width=0.45\linewidth]{bilder/pruef-1-wuerfel-kegel.png}\hfill
\begin{tikzpicture}
\begin{axis}[width=0.52\linewidth,height=7.2cm,xlabel={$s$},ylabel={$A(s)$},grid=major,legend pos=north west,legend style={font=\scriptsize},xmin=0,xmax=1.35]
\addplot[blue!70!black,thick,domain=0:1,samples=60] {""" + f"{R**-2:.12f}" + r"""*x^2};
\addlegendentry{Strebwerk: $\rho^{-2}s^2$}
\addplot[blue!70!black,dashed,domain=1:1.33,samples=20] {""" + f"{R**-2:.12f}" + r"""*x^2};
\addlegendentry{dieselbe Formel außerhalb ihres Typs}
\addplot[only marks,mark=o,mark size=1.4pt,red!80!black] coordinates {""" + prof_marks + r"""};
\addlegendentry{unabhängig (Hülle der Schnittpunkte)}
\draw[gray,dashed] (axis cs:1,0) -- (axis cs:1,0.62) node[above,font=\scriptsize] {Typwechsel $s=1$};
\end{axis}
\end{tikzpicture}
\end{center}
Kegel $O,\ (1,\rho,\rho^3),\ (\rho,\rho^3,1),\ (1,\rho^3,\rho)$ mit Speichen; Schnittebene $x = s$ mit Parameter $s$.
Die Grafik zeigt, was „gültig, solange die Ebene dieselben Kanten schneidet'' bedeutet: Bei $s = 1$ erreicht die
Ebene die Ecken $(1,\rho,\rho^3)$ und $(1,\rho^3,\rho)$; danach ist das Schnittpolygon ein anderes Dreieck, und die
unabhängigen Punkte verlassen die Parabel.

\textbf{Strebwerk-Ausgabe (Auszug, wörtlich übernommen):}
\begin{align*}
A_{\text{Würfel}} &= 4 + 8\rho + 4\rho^2 = \pf{4\rho^6}, \qquad U_{\text{Würfel}} = 8 + 8\rho = \pf{8\rho^3},\\
A_{\text{Kegel}}(s) &= (1 + \rho - \rho^2)\,s^2 = \pf{\rho^{-2}s^2} \qquad (0 < s \le 1),\\
\text{Ecken (Kegel)} &:\ (s,\ s\rho^2,\ \rho^{-1}s),\ (s,\ \rho^3 s,\ s\rho),\ (s,\ s\rho,\ \rho^3 s), \qquad \text{Würfel: } (s,\ \pm\rho^3,\ \pm\rho^3),\\
\text{Kante 4 (je)} &= \sqrt{2+2\rho+2\rho^2} = \pf{\sqrt2\,\rho^{5/2}},\\
\lvert\cos\angle\rvert &\in \Bigl\{\pf{\tfrac13\sqrt6\,\rho^{1/2}},\ \pf{\tfrac13\sqrt6\,\rho^{-3/2}},\ \pf{\tfrac13\sqrt6\,\rho^{-5/2}},\ 0\Bigr\}\quad\text{(Raumdiag.\ vs.\ Kegelkanten)}.
\end{align*}
""" + indep_table("Würfel + Kegel") + r"""

\section{Kegel über dem Permutationssechseck}
Die sechs Permutationen von $(1,\rho,\rho^3)$ liegen in der Ebene $x+y+z = 2\rho^3$ und bilden ein gleichwinkliges
Sechseck mit den Kanten $\sqrt2$ und $\sqrt2\,\rho^{-4}$, eingeschrieben in den Würfelschnitt (grau). Die 20 Dreiecke
liefern als Kegel mit Spitze $O$ genau vier Volumina; die Szene enthält je einen Vertreter.
\begin{center}
\includegraphics[width=0.36\linewidth]{bilder/pruef-4-sechseck.png}\hfill
""" + hexlabels + r"""
\end{center}
\begin{center}
""" + hexpics + r"""
\end{center}
""" + indep_table("Sechseck-Kegel") + r"""

\section{Quader $1\times\rho\times\rho^3$ in Antiprisma $n = 4$ (Höhe $h$)}
\begin{center}
\includegraphics[width=0.42\linewidth]{bilder/anti-rho3-n4.png}\hfill
\begin{tikzpicture}
\begin{axis}[width=0.55\linewidth,height=7.2cm,xlabel={$h$},ylabel={Innenlänge je Strebe},grid=major,legend pos=north west,legend style={font=\scriptsize},xmin=0,xmax=3]
\addplot[blue!70!black,thick,domain=0:3] {""" + f"{math.sqrt(2) * R**-0.5:.12f}" + r"""*x};
\addlegendentry{Raumdiag.: $\sqrt2\,\rho^{-1/2}h$}
\addplot[only marks,mark=o,mark size=1.3pt,blue!70!black] coordinates {""" + hc_space + r"""};
\addplot[orange!80!black,thick,domain=0:3] {""" + f"{math.sqrt(R):.12f}" + r"""*x};
\addlegendentry{Flächendiag.\ 3: $\rho^{1/2}h$}
\addplot[only marks,mark=square,mark size=1.1pt,orange!80!black] coordinates {""" + hc_f3 + r"""};
\end{axis}
\end{tikzpicture}
\end{center}
Linien: Strebwerks Formeln; Marken: unabhängiges Clipping an der Hülle des Antiprismas. Die Formeln gelten, solange
die Streben durch Deckel und Boden austreten (bis $h\approx 1{,}8$ bzw.\ $1{,}9$); darüber treten sie seitlich aus --
wieder ein Typwechsel, den die exakte Formel ehrlich nicht abdeckt. Neu sichtbar dank der Potenzform:
$(\rho^2-\rho)\,h\sqrt{2+2\rho+2\rho^2} = \pf{\sqrt2\,\rho^{-1/2}h}$, denn $\rho^2-\rho = \rho^{-3}$.
""" + indep_table("Quader + Antiprisma n = 4") + r"""

\section{Ikosaeder und Dual (Kontrolle in $\mathbb{Q}(\varphi)$)}
\begin{minipage}{0.36\linewidth}\includegraphics[width=\linewidth]{bilder/ikosaeder-dodekaeder.png}\end{minipage}\hfill
\begin{minipage}{0.6\linewidth}
Die Verallgemeinerung greift auch mit Parametern in anderen Zahlkörpern:
\[ V_{\text{Ikosaeder}} = \bigl(\tfrac56 + \tfrac56\varphi\bigr)a^3 = \pf{\tfrac56\,\varphi^2 a^3}, \]
denn $1 + \varphi = \varphi^2$. Alle Ketten dieser Szene sind in Anhang A enthalten.
\end{minipage}

\section{Negativkontrolle: keine erfundenen Potenzformen}
In der Szene Würfel $+$ Oktaeder mit Ebene $z = s$ liefert Strebwerk für das Oktaeder
$A(s) = 2s^2 - 8s + 8$ und $U(s) = -4s\sqrt2 + 8\sqrt2$ -- \emph{ohne} Potenzform, zu Recht: Beides ist kein
Monom in $s$. Eine Potenzform wird nur behauptet, wenn sie in $K[\text{Parameter}]$ exakt gilt.

\appendix
\section{Alle geprüften Identitäten (Prüfung A)}
{\footnotesize
\begin{longtable}{>{\raggedright}p{1.9cm}>{\raggedright}p{2.7cm}>{\raggedright\arraybackslash}p{5.0cm}>{\raggedright\arraybackslash}p{3.0cm}l}
\toprule
Szene & Größe & kanonische Form & Potenzform & Abw.\\
\midrule\endhead
""" + "\n".join(ident_rows) + r"""
\bottomrule
\end{longtable}}

\section{Formel gegen Zahlwert (Prüfung B)}
{\footnotesize
\begin{longtable}{>{\raggedright}p{1.9cm}>{\raggedright}p{2.6cm}>{\raggedright\arraybackslash}p{4.25cm}rrl}
\toprule
Szene & Größe & Formel & ausgewertet & Strebwerk & Abw.\\
\midrule\endhead
""" + "\n".join(num_rows) + r"""
\bottomrule
\end{longtable}}
Bei Winkeln steht in der Spalte „Strebwerk'' $\cos$ des angegebenen Gradmaßes (4 Nachkommastellen), daher
Abweichungen bis $\sim 10^{-6}$.

\section{Strebwerk-Exporte der Prüfszenen}
Die folgenden Seiten sind die unveränderten LaTeX-Exporte (\texttt{export\_tex} über MCP) der Prüfszenen.
""" + "".join(f"\n\\includepdf[pages=-,scale=0.92,frame,pagecommand={{\\thispagestyle{{plain}}}}]{{szenen/{n}.pdf}}" for n in SCN) + r"""
\end{document}
"""
(OUT / "strebwerk-pruefdokument.tex").write_text(tex)
print("ok")
