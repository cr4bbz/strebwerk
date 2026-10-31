# Prüfung der exakten Ausgaben

Baut Prüfszenen über den MCP-Server (wie ein Sprachmodell), prüft jede ausgegebene
Gleichungskette unabhängig und erzeugt das Prüfdokument.

```bash
pip install mpmath numpy scipy
cargo build --release
python pruefung/szenen_bauen.py ausgabe      # Szenen, Textausgaben, LaTeX-Exporte
python pruefung/verify.py ausgabe            # Prüfungen A–C → ausgabe/pruefung.json
# Grafiken: Screenshots der Szenen nach ausgabe/bilder/<szene>.png legen, Exporte in ausgabe/szenen/ kompilieren
python pruefung/build_report.py ausgabe <commit>
pdflatex strebwerk-pruefdokument.tex          # im Ordner ausgabe, zweimal
```

- **A** Identitäten X = Y = …: 50 Stellen, zufällige positive Parameterwerte
- **B** erste Form gegen Strebwerks eigenen Zahlwert bzw. Winkel
- **C** Kernaussagen direkt aus den Koordinaten (Determinanten, Schnitt-Hüllen, Clipping)
