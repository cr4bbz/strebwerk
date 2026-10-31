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
cargo run --release -- --mcp --root szenen           # Werkzeug-Server für Sprachmodelle (MCP)
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

## Sprachmodelle (MCP)

`strebwerk --mcp` startet Strebwerk ohne Fenster als Werkzeug-Server nach dem
[Model Context Protocol](https://modelcontextprotocol.io) (JSON-RPC über stdio). Ein Sprachmodell
in Claude Desktop, Claude Code oder einem anderen MCP-Client kann damit Szenen bauen und auswerten.
Dabei gilt eine klare Arbeitsteilung: **Das Modell entscheidet, was gebaut wird; jede Formel,
Länge, jeder Winkel und jede Potenzform kommt aus der exakten Algebra von Strebwerk.**

| Werkzeug | Zweck |
|---|---|
| `scene_info`, `scene_json` | Überblick bzw. vollständiges JSON der Szene |
| `new_scene`, `load_scene`, `save_scene` | Szenen anlegen, laden, speichern (`save_scene` setzt die Arbeitsdatei) |
| `list_templates`, `add_template` | Vorlagen (Platonische Körper, Quader 1 × ρ × ρ³, Antiprismen, Flächen, Kurven …) |
| `add_points` | freie Konstruktion aus exakten Punkten, z. B. `[["1","rho","rho^3"], …]` |
| `add_object`, `update_object`, `remove_object` | beliebige Objekte im Strebwerk-Format; Änderungen als JSON-Patch |
| `set_param`, `add_generator` | Parameter (bleiben in Formeln Unbestimmte) und eigene Zahlkörper-Generatoren |
| `add_section`, `remove_section`, `sections` | Schnittebenen mit exaktem Polygon, A, U und Cavalieri-Profil |
| `analyze` | alle exakten Formeln eines Objekts inkl. Potenzformen und Symmetriegruppe |
| `interactions` | Knoten, Kreuzungen, Stöße, Bündelpunkte, Durchstoßungen zwischen Objekten |
| `evaluate`, `distance`, `angle` | exakte Vereinfachung, Abstand, Winkel (cos², cos, Potenzform) |
| `export_tex`, `undo` | LaTeX-Dokument wie aus der GUI; letzte Änderung zurücknehmen |

**Sicherheit**: Dateien werden nur innerhalb des mit `--root` freigegebenen Ordners gelesen und
geschrieben (Standard: aktuelles Verzeichnis), nur `.strebwerk`/`.json` bzw. `.tex`. Ausdrücke laufen
ausschließlich durch den eigenen Parser. Fehlerhafte Änderungen (Parse-Fehler, unbekannte Felder,
ungültige Objekte) werden abgewiesen, die Szene bleibt dann unverändert.

**Live mit der GUI**: Nach `save_scene` speichert der Server jede Änderung automatisch. Ist dieselbe
Datei in der GUI geöffnet, lädt sie Änderungen sofort nach (solange dort nichts ungespeichert ist,
sonst bietet *Datei ▸ Neu laden* es an). Umgekehrt übernimmt der Server vor jedem Befehl, was in der
GUI gespeichert wurde (*Datei ▸ Speichern*).

**Einrichten in Claude Desktop** (Windows: `%APPDATA%\Claude\claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "strebwerk": {
      "command": "C:\\Pfad\\zu\\strebwerk\\target\\release\\strebwerk.exe",
      "args": ["--mcp", "--root", "C:\\Users\\Julian\\Documents\\strebwerk-szenen"]
    }
  }
}
```

Danach Claude Desktop neu starten. **Claude Code**:
`claude mcp add strebwerk -- C:\Pfad\zu\strebwerk.exe --mcp --root C:\Users\Julian\Documents\strebwerk-szenen`.

Zum Ausprobieren ohne Client (eine JSON-RPC-Nachricht pro Zeile):

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"evaluate","arguments":{"expr":"sqrt(2+2*rho+2*rho^2)"}}}' \
  | strebwerk --mcp
# → exakt: √(2 + 2ρ + 2ρ²) = √2·ρ^(5/2)
```

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
| `src/mcp.rs` | Befehlsschicht und MCP-Server (`--mcp`) |

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
