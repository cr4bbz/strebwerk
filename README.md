# Strebwerk

Interaktive 3D-Umgebung zum Konstruieren und Verändern mathematischer Objekte – mit
Verstrebungen *durch* die Objekte und einem exakten Formelgenerator.
Native Desktop-App in Rust (egui, gerendert über wgpu: Vulkan / Metal / DX12 / OpenGL).

## Starten

```bash
# einmalig: Rust installieren (https://rustup.rs)
cargo run --release                                  # leere Startszene
cargo run --release -- examples/demo.strebwerk       # Szene direkt öffnen
cargo run --release -- --tex examples/interaktion.strebwerk   # LaTeX-Export ohne Fenster
cargo test                                           # Algebra, Geometrie, Topologie, Schnitte, Interaktionen
```

Unter Linux werden die üblichen X11/Wayland-Bibliotheken gebraucht (auf Desktop-Systemen vorhanden).
Die Datei-Dialoge laufen über das XDG-Portal.

## Was es kann

**Objekte**
- Polyeder: platonisch, archimedisch (als exakte Abstumpfungen), Quader, Prismen, Antiprismen,
  Pyramiden, Bipyramiden (n beliebig). Operationen: Abstumpfen mit Parameter t (Morph!),
  Dual (Flächenmitten), Skalieren – beliebig verkettet.
- Freie Konstruktion: Punkte per Ausdruck oder Klick, Streben per Klick, wahlweise konvexe Hülle.
- Formel → Objekt: Parameterflächen r(u,v), implizite Flächen F(x,y,z) = 0, Raumkurven r(t).
  Schnelleingabe links, z. B. `x^2 + y^2 - z^2 = 0.3` oder `(cos t, sin t, t/5)`.

**Verstrebungen**
- Polyeder: Flächen- und Raumdiagonalen (exakt nach Länge klassifiziert, einzeln ausblendbar),
  eingeschriebenes Dual, Speichen, Kegel-Tetraedrisierung, Symmetrieachsen (gefärbt nach
  Zähligkeit), Spiegelebenen, Um-/Inkugel.
- Füllungen: Gitter (kubisch, krz, Oktett-Fachwerk) auf den Körper zugeschnitten, TPMS-Flächen
  (Gyroid, Schwarz P, Schwarz D) im Inneren – für Polyeder und implizite Flächen.
- Flächen: Parameterlinien, Sehnen durch das Innere, Normalen, Höhenlinien.
- Kurven: begleitendes Dreibein (T, N, B), Schmiegkreise, Sehnen.
- Gerüst-Modus: vollständiger Graph, Maxwell-Zählung 3n − 6.

**Formelgenerator (exakt)**
- Koordinaten leben in einem Zahlkörperturm K (ℚ, φ, √2, √3, ρ, γₙ = 2cos(2π/n),
  σₙ = 2sin(2π/n), eigene Generatoren über ihr Minimalpolynom, z. B. Selmer-Trinome
  xⁿ − x − 1). Gerechnet wird in K[Parameter], kanonisch reduziert – Gleichheit ist exakt.
- Kantenlängen, Diagonalklassen, Flächeninhalte, Oberfläche, Volumen, Um-/Kanten-/Inkugelradius,
  Diederwinkel (cos θ exakt), Euler-Charakteristik, Symmetriegruppe (Schoenflies, Ordnung,
  Achsen, Spiegelebenen).
- Wurzeln werden vereinfacht: rationale Quadratanteile, Parameter-Monome und – für Elemente in
  einem einfachen Generator – echte Quadratwurzeln in K (über alle Konjugierten gesucht und
  exakt verifiziert), z. B. √((φ+1)a²) = aφ.
- Flächen und Kurven (numerisch): Flächeninhalt, Volumen, Euler-Charakteristik aus dem Netz mit
  Klassifikation nach dem Klassifikationssatz (Geschlecht, Kreuzhauben, Randkurven – erkennt
  Sphäre, Torus, Möbiusband, Kleinsche Flasche …), Bogenlänge, Totalkrümmung, Totaltorsion,
  Fenchel / Fáry–Milnor.
- Schnittebene mit Schnittfläche (Cavalieri).

**Schnittebenen** (beliebig viele, `n·x = d`, Normale und Lage als Ausdrücke)
- Polyeder: exaktes Schnittpolygon – Ecken, Umfang, Fläche in K[Parameter]. Mit `d = s` und einem
  Parameter `s` entsteht z. B. für das Oktaeder |x|+|y|+|z| ≤ 2 direkt A(s) = 2s² − 8s + 8.
- Netze (Flächen, implizite Körper): Fläche, Umfang, Anzahl der Schnittkurven.
- Cavalieri-Profil h ↦ A(h) über die ganze Ausdehnung; ∫A dh wird mit dem Volumen verglichen.

**Interaktionen der Verstrebungen** (jedes Paar sichtbarer Objekte)
- gemeinsame Knoten, Kreuzungen und Stöße von Streben mit exaktem Schnittpunkt und Winkel (|cos ∠| in K),
- Durchstoßungen: welche Streben in den anderen Körper eindringen, ganz innen liegen oder auf
  seiner Oberfläche verlaufen – mit exakter Innenlänge,
- kollineare Überlappungen und kleinster Abstand nicht berührender Streben,
- Treffer gelten bei exakten Objekten nur, wenn sie im gemeinsamen Zahlkörper nachgewiesen sind.
- Darstellung: Knoten weiß, Kreuzungen magenta, Durchstoßpunkte und Innenabschnitte cyan.

**Export**: Formeln, Schnittebenen (mit pgfplots-Profil) und Interaktionen als LaTeX, aktuelle Ansicht als TikZ (standalone), Gerüst als STL
zum 3D-Drucken (Streben als Stäbe, Knoten als Oktaeder, Maßstab einstellbar).
Szenen werden als lesbares JSON gespeichert (`.strebwerk`).

## Aufbau

| Datei | Inhalt |
|---|---|
| `src/algebra.rs` | Zahlkörpertürme, Polynome in Parametern, Inverse (reguläre Darstellung), Wurzelerkennung, Ausgabe Unicode/LaTeX, Kreisteilungs- und Dickson-Polynome |
| `src/expr.rs` | Parser (implizite Multiplikation, Unicode-Eingabe), numerischer Compiler, exakte Auswertung |
| `src/analysis.rs` | Schnittebenen (exaktes Schnittpolygon, Cavalieri-Profil), Interaktionen zwischen Objekten |
| `src/poly.rs` | konvexe Hülle, exakte Polyeder-Analyse, Symmetriegruppen |
| `src/mesh.rs` | Marching Tetrahedra, Parameterflächen mit Naht-Erkennung, Topologie, Schnitte, Gitter/TPMS |
| `src/scene.rs` | Objektdefinitionen, Bau der Geometrie, Vorlagen |
| `src/render.rs` | Kamera und tiefensortierte Darstellung |
| `src/app.rs` | Oberfläche |
| `src/export.rs` | LaTeX, TikZ, STL |

## Genauigkeit und Grenzen

- **Exakt** ist alles, was in K[Parameter] gerechnet wird. Die *Kombinatorik* (welche Ecken eine
  Fläche bilden) stammt aus der numerischen Hülle beim aktuellen Parameterwert; die Formeln
  gelten im ganzen Bereich mit demselben kombinatorischen Typ. Nicht exakt koplanare Flächen
  werden gemeldet.
- Eigene Generatoren: Das Minimalpolynom muss über dem bisherigen Turm irreduzibel sein – das
  wird nicht geprüft. Ist es reduzibel, entstehen Nullteiler; Inverse schlagen dann fehl
  und werden gemeldet.
- „Dual“ nimmt die Flächenmitten (für reguläre und viele archimedische Körper ähnlich zum polaren Dual).
- Darstellung: CPU-Projektion mit Maleralgorithmus (konvexe Körper schichtweise sortiert). Sich
  durchdringende Objekte können Sortierfehler zeigen; der nächste Schritt wäre ein eigener
  wgpu-Pass mit Tiefenpuffer.
- Gitter-/TPMS-Füllung gibt es für Polyeder und implizite Flächen, nicht für Parameterflächen.

Schrift: DejaVu Sans (eingebettet, Lizenz in `assets/DejaVu-LICENSE.txt`).
