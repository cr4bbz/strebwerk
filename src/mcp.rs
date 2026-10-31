//! MCP-Schnittstelle: Strebwerk als Werkzeug-Server für Sprachmodelle
//! (Model Context Protocol, JSON-RPC 2.0 über stdio, eine Nachricht pro Zeile).
//!
//! Aufbau in drei Schichten:
//! * [`Command`] – die Befehlsschicht: eine Operation auf der Szene = ein Werkzeug.
//! * [`Session`] – hält Szene, Arbeitsdatei und Undo-Stapel und führt Befehle aus.
//! * [`serve`] – spricht das Protokoll (initialize, tools/list, tools/call, ping).
//!
//! Grundsatz: Das Sprachmodell entscheidet, *was* gebaut wird; jede Zahl, Formel und
//! Potenzform in den Antworten stammt aus der exakten Algebra von Strebwerk.

use crate::analysis::{self, InterRes, Profile, SectionRes};
use crate::app::SaveFile;
use crate::export;
use crate::expr::{self, Ex};
use crate::poly;
use crate::render::Camera;
use crate::scene::{self, Built, CustomGen, Kind, Layers, Obj, Param, Scene, Section};
use serde::Deserialize;
use serde_json::{json, Value};
use std::fmt::Write as _;
use std::io::{BufRead, Write};
use std::path::{Component, Path, PathBuf};

const UNDO_MAX: usize = 50;
const PROTOCOL_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
const PROTOCOL_DEFAULT: &str = "2025-06-18";

// ---------------------------------------------------------------------------
// Befehlsschicht
// ---------------------------------------------------------------------------

/// Ausdruck als Zeichenkette oder JSON-Zahl (Zahlen werden exakt als Dezimalbruch gelesen).
#[derive(Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum Term {
    S(String),
    N(serde_json::Number),
}

impl Term {
    fn s(&self) -> String {
        match self {
            Term::S(s) => s.trim().to_string(),
            Term::N(n) => n.to_string(),
        }
    }
}

fn three(t: &[Term; 3]) -> [String; 3] {
    [t[0].s(), t[1].s(), t[2].s()]
}

fn yes() -> bool {
    true
}

/// Eine Operation auf der Sitzung. Der Werkzeugname ist der `op`-Tag.
#[derive(Deserialize, Debug)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    SceneInfo,
    SceneJson,
    NewScene {
        #[serde(default)]
        example: bool,
    },
    LoadScene {
        path: String,
    },
    SaveScene {
        path: Option<String>,
    },
    ListTemplates,
    AddTemplate {
        template: String,
        name: Option<String>,
        layers: Option<Value>,
    },
    AddPoints {
        name: String,
        points: Vec<[Term; 3]>,
        #[serde(default = "yes")]
        hull: bool,
        #[serde(default)]
        struts: Vec<[usize; 2]>,
        layers: Option<Value>,
    },
    AddObject {
        name: String,
        kind: Value,
        layers: Option<Value>,
    },
    UpdateObject {
        id: u64,
        patch: Value,
    },
    RemoveObject {
        id: u64,
    },
    SetParam {
        name: String,
        value: Option<f64>,
        min: Option<f64>,
        max: Option<f64>,
    },
    AddGenerator {
        key: String,
        poly: String,
        approx: f64,
    },
    AddSection {
        normal: Option<[Term; 3]>,
        axis: Option<String>,
        offset: Term,
        #[serde(default = "yes")]
        profile: bool,
        name: Option<String>,
    },
    RemoveSection {
        index: usize,
    },
    Analyze {
        id: Option<u64>,
    },
    Sections,
    Interactions,
    Evaluate {
        expr: String,
    },
    Distance {
        a: [Term; 3],
        b: [Term; 3],
    },
    Angle {
        u: [Term; 3],
        v: [Term; 3],
    },
    ExportTex {
        path: Option<String>,
    },
    Undo,
}

impl Command {
    /// Werkzeugaufruf (Name + Argumente) → Befehl.
    pub fn from_call(name: &str, args: Option<&Value>) -> Result<Command, String> {
        let mut obj = match args {
            None | Some(Value::Null) => serde_json::Map::new(),
            Some(Value::Object(m)) => m.clone(),
            Some(_) => return Err("Argumente müssen ein JSON-Objekt sein".into()),
        };
        if obj.contains_key("op") {
            return Err("„op“ ist kein Argument".into());
        }
        obj.insert("op".into(), Value::String(name.into()));
        serde_json::from_value(Value::Object(obj)).map_err(|e| format!("Argumente: {e}"))
    }

    fn mutates(&self) -> bool {
        matches!(
            self,
            Command::NewScene { .. }
                | Command::LoadScene { .. }
                | Command::AddTemplate { .. }
                | Command::AddPoints { .. }
                | Command::AddObject { .. }
                | Command::UpdateObject { .. }
                | Command::RemoveObject { .. }
                | Command::SetParam { .. }
                | Command::AddGenerator { .. }
                | Command::AddSection { .. }
                | Command::RemoveSection { .. }
        )
    }
}

// ---------------------------------------------------------------------------
// Sitzung
// ---------------------------------------------------------------------------

pub struct Session {
    pub scene: Scene,
    pub camera: Camera,
    /// Nur innerhalb dieses Ordners wird gelesen und geschrieben.
    root: PathBuf,
    /// Arbeitsdatei: nach jeder Änderung automatisch gespeichert (für die GUI).
    file: Option<PathBuf>,
    /// Änderungszeit der Arbeitsdatei nach unserem letzten Lesen/Schreiben
    file_mtime: Option<std::time::SystemTime>,
    undo: Vec<Scene>,
    cache: scene::Cache,
}

fn empty_scene() -> Scene {
    let mut s = scene::default_scene();
    s.objects.clear();
    s.next_id = 0;
    s
}

impl Session {
    pub fn new(root: PathBuf) -> Session {
        let root = root.canonicalize().unwrap_or(root);
        Session {
            scene: empty_scene(),
            camera: Camera::default(),
            root,
            file: None,
            file_mtime: None,
            undo: vec![],
            cache: scene::Cache::new(),
        }
    }

    pub fn apply(&mut self, cmd: Command) -> Result<String, String> {
        // Hat jemand (z. B. die GUI) die Arbeitsdatei inzwischen gespeichert? Dann zuerst übernehmen.
        let note = self.sync_from_disk();
        let res = self.apply_inner(cmd);
        match (note, res) {
            (Some(n), Ok(t)) => Ok(format!("{n}\n\n{t}")),
            (Some(n), Err(e)) => Err(format!("{n}\n\n{e}")),
            (None, r) => r,
        }
    }

    fn sync_from_disk(&mut self) -> Option<String> {
        let f = self.file.clone()?;
        let now = mtime_of(&f);
        if now.is_none() || now == self.file_mtime {
            return None;
        }
        let sf: SaveFile = std::fs::read_to_string(&f)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())?;
        self.undo.push(std::mem::replace(&mut self.scene, sf.scene));
        self.camera = sf.camera;
        self.file_mtime = now;
        let (n, k) = (self.scene.objects.len(), self.scene.sections.len());
        Some(format!(
            "[Arbeitsdatei wurde außerhalb geändert (z. B. in der GUI) – übernommen: {n} {}, {k} {}]",
            if n == 1 { "Objekt" } else { "Objekte" },
            if k == 1 { "Schnittebene" } else { "Schnittebenen" }
        ))
    }

    fn apply_inner(&mut self, cmd: Command) -> Result<String, String> {
        if let Command::Undo = cmd {
            let prev = self.undo.pop().ok_or("Nichts rückgängig zu machen")?;
            self.scene = prev;
            let mut out = format!("Rückgängig gemacht.\n\n{}", self.info());
            self.autosave(&mut out);
            return Ok(out);
        }
        let mutating = cmd.mutates();
        let before = self.scene.clone();
        let res = self.run(cmd);
        match res {
            Ok(mut out) if mutating => {
                self.undo.push(before);
                if self.undo.len() > UNDO_MAX {
                    self.undo.remove(0);
                }
                self.autosave(&mut out);
                Ok(out)
            }
            Err(e) if mutating => {
                self.scene = before;
                Err(format!("{e}\n(Szene unverändert)"))
            }
            r => r,
        }
    }

    fn autosave(&mut self, out: &mut String) {
        if let Some(f) = self.file.clone() {
            match self.write_scene(&f) {
                Ok(()) => {
                    let _ = write!(out, "\n\n[Arbeitsdatei aktualisiert: {}]", disp(&f));
                }
                Err(e) => {
                    let _ = write!(out, "\n\n[Speichern fehlgeschlagen: {e}]");
                }
            }
        }
    }

    fn write_scene(&mut self, f: &Path) -> Result<(), String> {
        let sf = SaveFile {
            format: "strebwerk-1".into(),
            scene: self.scene.clone(),
            camera: self.camera.clone(),
        };
        let s = serde_json::to_string_pretty(&sf).map_err(|e| e.to_string())?;
        std::fs::write(f, s).map_err(|e| format!("{}: {e}", disp(&f)))?;
        self.file_mtime = mtime_of(f);
        Ok(())
    }

    /// Pfad relativ zum Wurzelordner auflösen; nichts außerhalb, nur erlaubte Endungen.
    fn resolve(&self, p: &str, exts: &[&str]) -> Result<PathBuf, String> {
        let rel = Path::new(p);
        let joined = if rel.is_absolute() {
            rel.to_path_buf()
        } else {
            self.root.join(rel)
        };
        // „..“ lexikalisch auflösen, dann den existierenden Elternordner kanonisieren
        let mut norm = PathBuf::new();
        for c in joined.components() {
            match c {
                Component::ParentDir => {
                    norm.pop();
                }
                Component::CurDir => {}
                c => norm.push(c),
            }
        }
        let parent = norm.parent().ok_or("ungültiger Pfad")?;
        let parent = parent
            .canonicalize()
            .map_err(|e| format!("{}: {e}", disp(&parent)))?;
        if !parent.starts_with(&self.root) {
            return Err(format!(
                "Pfad liegt außerhalb des freigegebenen Ordners {}",
                disp(&self.root)
            ));
        }
        let name = norm.file_name().ok_or("Dateiname fehlt")?;
        let ext = Path::new(name)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        if !exts.contains(&ext) {
            return Err(format!("erlaubte Endungen: {}", exts.join(", ")));
        }
        Ok(parent.join(name))
    }

    fn obj(&self, id: u64) -> Result<&Obj, String> {
        self.scene
            .objects
            .iter()
            .find(|o| o.id == id)
            .ok_or_else(|| format!("kein Objekt mit id {id} (siehe scene_info)"))
    }

    fn build(&mut self, id: u64) -> Result<(Obj, Built), String> {
        let o = self.obj(id)?.clone();
        let b = scene::build(&o, &self.scene, &mut self.cache);
        Ok((o, b))
    }

    /// Objekt bauen; Fehler machen die Änderung ungültig.
    fn check(&mut self, id: u64) -> Result<String, String> {
        let (o, b) = self.build(id)?;
        if !b.errors.is_empty() {
            return Err(format!("„{}“: {}", o.name, b.errors.join("; ")));
        }
        let mut s = format!("#{} „{}“ ({})", o.id, o.name, o.kind.type_name());
        if let Some(sym) = &b.sym {
            let _ = write!(s, ", Symmetrie {}", sym.name_uni);
        }
        if !b.formulas.is_empty() {
            let _ = write!(s, ", {} Formelzeilen (analyze)", b.formulas.len());
        }
        for w in &b.warnings {
            let _ = write!(s, "\nHinweis: {w}");
        }
        Ok(s)
    }

    fn built_visible(&mut self) -> Vec<(Obj, Built)> {
        let objs: Vec<Obj> = self
            .scene
            .objects
            .iter()
            .filter(|o| o.visible)
            .cloned()
            .collect();
        objs.into_iter()
            .map(|o| {
                let b = scene::build(&o, &self.scene, &mut self.cache);
                (o, b)
            })
            .collect()
    }

    fn analyze_all(&mut self, inter: bool) -> (Vec<SectionRes>, Vec<Profile>, Vec<InterRes>) {
        let built = self.built_visible();
        let vis: Vec<(&Obj, &Built)> = built.iter().map(|(o, b)| (o, b)).collect();
        analysis::analyze_all(&self.scene, &vis, inter)
    }

    fn info(&self) -> String {
        let mut s = String::new();
        let _ = writeln!(s, "Parameter:");
        for p in &self.scene.params {
            let _ = writeln!(
                s,
                "  {} = {} (Bereich {} … {}; exakt als Unbestimmte behandelt)",
                p.name, p.value, p.min, p.max
            );
        }
        if !self.scene.gens.is_empty() {
            let _ = writeln!(s, "Eigene Generatoren:");
            for g in &self.scene.gens {
                let _ = writeln!(s, "  {}: {} = 0, ≈ {}", g.key, g.poly, g.approx);
            }
        }
        let _ = writeln!(s, "Objekte ({}):", self.scene.objects.len());
        for o in &self.scene.objects {
            let def = serde_json::to_string(&o.kind).unwrap_or_default();
            let off = if o.offset.iter().all(|x| x.trim() == "0") {
                String::new()
            } else {
                format!(", verschoben um ({})", o.offset.join(", "))
            };
            let _ = writeln!(
                s,
                "  #{} „{}“ – {}{}{}\n     {}",
                o.id,
                o.name,
                o.kind.type_name(),
                if o.visible { "" } else { " (ausgeblendet)" },
                off,
                def
            );
        }
        let _ = writeln!(s, "Schnittebenen ({}):", self.scene.sections.len());
        for (i, sec) in self.scene.sections.iter().enumerate() {
            let _ = writeln!(
                s,
                "  [{i}] {}: {}{}{}",
                sec.name,
                analysis::plane_text(sec, false),
                if sec.profile { ", mit Profil" } else { "" },
                if sec.visible { "" } else { " (aus)" }
            );
        }
        if let Some(f) = &self.file {
            let _ = writeln!(s, "Arbeitsdatei: {}", disp(&f));
        }
        s
    }

    fn run(&mut self, cmd: Command) -> Result<String, String> {
        match cmd {
            Command::SceneInfo => Ok(self.info()),
            Command::SceneJson => {
                serde_json::to_string_pretty(&self.scene).map_err(|e| e.to_string())
            }
            Command::NewScene { example } => {
                self.scene = if example {
                    scene::default_scene()
                } else {
                    empty_scene()
                };
                self.file = None;
                Ok(format!(
                    "Neue Szene (Arbeitsdatei gelöst).\n\n{}",
                    self.info()
                ))
            }
            Command::LoadScene { path } => {
                let f = self.resolve(&path, &["strebwerk", "json"])?;
                let txt = std::fs::read_to_string(&f).map_err(|e| format!("{path}: {e}"))?;
                let sf: SaveFile =
                    serde_json::from_str(&txt).map_err(|e| format!("{path}: {e}"))?;
                self.scene = sf.scene;
                self.camera = sf.camera;
                self.file = Some(f.clone());
                self.file_mtime = mtime_of(&f);
                Ok(format!(
                    "Geladen; {} ist jetzt Arbeitsdatei (Änderungen werden dort gespeichert).\n\n{}",
                    disp(&f),
                    self.info()
                ))
            }
            Command::SaveScene { path } => {
                let f = match path {
                    Some(p) => self.resolve(&p, &["strebwerk"])?,
                    None => self
                        .file
                        .clone()
                        .ok_or("keine Arbeitsdatei – bitte „path“ angeben")?,
                };
                self.write_scene(&f)?;
                self.file = Some(f.clone());
                Ok(format!(
                    "Gespeichert: {} (ab jetzt Arbeitsdatei; die GUI lädt Änderungen automatisch nach)",
                    disp(&f)
                ))
            }
            Command::ListTemplates => {
                let mut s = String::new();
                let mut group = "";
                for (i, t) in scene::templates().iter().enumerate() {
                    if t.group != group {
                        group = t.group;
                        let _ = writeln!(s, "{group}:");
                    }
                    let _ = writeln!(s, "  {:>2}. {}", i + 1, t.name);
                }
                s.push_str("\nVorlagen nutzen die Parameter a (Kante), t (Abstumpfung), h (Höhe).");
                Ok(s)
            }
            Command::AddTemplate {
                template,
                name,
                layers,
            } => {
                let t = find_template(&template)?;
                let n = name.unwrap_or_else(|| t.name.to_string());
                let id = self.scene.add(&n, (t.make)());
                self.set_layers(id, layers.as_ref())?;
                Ok(format!("Hinzugefügt: {}", self.check(id)?))
            }
            Command::AddPoints {
                name,
                points,
                hull,
                struts,
                layers,
            } => {
                if points.is_empty() {
                    return Err("keine Punkte".into());
                }
                for [i, j] in &struts {
                    if *i >= points.len() || *j >= points.len() || i == j {
                        return Err(format!("Strebe [{i}, {j}]: ungültige Punktindizes"));
                    }
                }
                let pts = points.iter().map(three).collect();
                let id = self.scene.add(
                    &name,
                    Kind::Free {
                        pts,
                        struts: struts.iter().map(|[i, j]| (*i, *j)).collect(),
                        hull,
                    },
                );
                self.set_layers(id, layers.as_ref())?;
                Ok(format!("Hinzugefügt: {}", self.check(id)?))
            }
            Command::AddObject { name, kind, layers } => {
                let kind: Kind = serde_json::from_value(kind)
                    .map_err(|e| format!("kind: {e} (Beispiele in der Werkzeugbeschreibung)"))?;
                let id = self.scene.add(&name, kind);
                self.set_layers(id, layers.as_ref())?;
                Ok(format!("Hinzugefügt: {}", self.check(id)?))
            }
            Command::UpdateObject { id, patch } => {
                let Value::Object(p) = &patch else {
                    return Err("patch muss ein JSON-Objekt sein".into());
                };
                const KEYS: &[&str] = &["name", "visible", "kind", "offset", "color", "layers"];
                for k in p.keys() {
                    if !KEYS.contains(&k.as_str()) {
                        return Err(format!(
                            "„{k}“ ist nicht änderbar; erlaubt: {}",
                            KEYS.join(", ")
                        ));
                    }
                }
                if let Some(l) = p.get("layers") {
                    check_layer_keys(l)?;
                }
                let i = self
                    .scene
                    .objects
                    .iter()
                    .position(|o| o.id == id)
                    .ok_or_else(|| format!("kein Objekt mit id {id}"))?;
                let mut cur =
                    serde_json::to_value(&self.scene.objects[i]).map_err(|e| e.to_string())?;
                // anderer Kind-Typ → ersetzen statt zusammenführen
                if let (Some(Value::Object(new)), Some(Value::Object(old))) =
                    (p.get("kind"), cur.get("kind"))
                {
                    if new.keys().next() != old.keys().next() {
                        cur["kind"] = Value::Null;
                    }
                }
                merge(&mut cur, &patch);
                let o: Obj = serde_json::from_value(cur).map_err(|e| format!("patch: {e}"))?;
                self.scene.objects[i] = o;
                Ok(format!("Geändert: {}", self.check(id)?))
            }
            Command::RemoveObject { id } => {
                let o = self.obj(id)?.name.clone();
                self.scene.objects.retain(|o| o.id != id);
                Ok(format!("Entfernt: #{id} „{o}“"))
            }
            Command::SetParam {
                name,
                value,
                min,
                max,
            } => {
                let ok = !name.is_empty()
                    && name.chars().next().is_some_and(|c| c.is_alphabetic())
                    && name.chars().all(|c| c.is_alphanumeric() || c == '_');
                if !ok
                    || expr::RESERVED.contains(&name.as_str())
                    || expr::FUNCS.contains(&name.as_str())
                {
                    return Err(format!("„{name}“ ist als Parametername nicht erlaubt"));
                }
                let created;
                if let Some(p) = self.scene.params.iter_mut().find(|p| p.name == name) {
                    created = false;
                    if let Some(v) = value {
                        p.value = v;
                    }
                    if let Some(v) = min {
                        p.min = v;
                    }
                    if let Some(v) = max {
                        p.max = v;
                    }
                } else {
                    created = true;
                    let v = value.unwrap_or(1.0);
                    self.scene.params.push(Param {
                        name: name.clone(),
                        value: v,
                        min: min.unwrap_or(v.min(0.0)),
                        max: max.unwrap_or(v.max(1.0) * 3.0),
                        animate: false,
                        speed: 0.3,
                        dir: 1.0,
                    });
                }
                let p = self.scene.params.iter().find(|p| p.name == name).unwrap();
                if p.min > p.max {
                    return Err("min > max".into());
                }
                Ok(format!(
                    "Parameter {} {}: Wert {} (Bereich {} … {}). In Formeln bleibt {} eine Unbestimmte; der Wert zählt nur für Darstellung und Zahlwerte.",
                    p.name,
                    if created { "angelegt" } else { "geändert" },
                    p.value,
                    p.min,
                    p.max,
                    p.name
                ))
            }
            Command::AddGenerator { key, poly, approx } => {
                let g = CustomGen { key, poly, approx };
                let d = scene::custom_gen(&g, self.scene.gens.len())?;
                self.scene.gens.retain(|x| x.key != g.key);
                self.scene.gens.push(g);
                Ok(format!(
                    "Generator {} ({}) angelegt, Wurzel ≈ {}.",
                    d.uni, d.key, d.value
                ))
            }
            Command::AddSection {
                normal,
                axis,
                offset,
                profile,
                name,
            } => {
                let n = match (normal, axis.as_deref()) {
                    (Some(n), None) => three(&n),
                    (None, Some(a)) => {
                        let k = match a {
                            "x" => 0,
                            "y" => 1,
                            "z" => 2,
                            _ => return Err("axis: x, y oder z".into()),
                        };
                        Section::new("", k, "0").normal
                    }
                    _ => return Err("entweder „normal“ oder „axis“ angeben".into()),
                };
                let idx = self.scene.sections.len();
                let sec = Section {
                    name: name.unwrap_or_else(|| format!("E{}", idx + 1)),
                    normal: n,
                    offset: offset.s(),
                    visible: true,
                    profile,
                };
                self.scene.sections.push(sec);
                analysis::plane_num(&self.scene.sections[idx], &self.scene)?;
                let (secs, profs, _) = self.analyze_all(false);
                let secs: Vec<SectionRes> = secs.into_iter().filter(|r| r.plane == idx).collect();
                let profs: Vec<Profile> = profs.into_iter().filter(|r| r.plane == idx).collect();
                Ok(format!(
                    "Schnittebene [{idx}] angelegt.\n\n{}",
                    fmt_sections(&self.scene, &secs, &profs)
                ))
            }
            Command::RemoveSection { index } => {
                if index >= self.scene.sections.len() {
                    return Err(format!("keine Schnittebene [{index}]"));
                }
                let s = self.scene.sections.remove(index);
                Ok(format!("Schnittebene „{}“ entfernt.", s.name))
            }
            Command::Analyze { id } => {
                let ids: Vec<u64> = match id {
                    Some(i) => vec![self.obj(i)?.id],
                    None => self
                        .scene
                        .objects
                        .iter()
                        .filter(|o| o.visible)
                        .map(|o| o.id)
                        .collect(),
                };
                if ids.is_empty() {
                    return Ok("Keine sichtbaren Objekte.".into());
                }
                let mut s = String::new();
                for i in ids {
                    let (o, b) = self.build(i)?;
                    s.push_str(&fmt_object(&o, &b));
                    s.push('\n');
                }
                Ok(s)
            }
            Command::Sections => {
                if self.scene.sections.is_empty() {
                    return Ok("Keine Schnittebenen (add_section).".into());
                }
                let (secs, profs, _) = self.analyze_all(false);
                Ok(fmt_sections(&self.scene, &secs, &profs))
            }
            Command::Interactions => {
                let (_, _, inters) = self.analyze_all(true);
                if inters.is_empty() {
                    return Ok(
                        "Keine Objektpaare mit Streben (mindestens zwei sichtbare Objekte nötig)."
                            .into(),
                    );
                }
                Ok(fmt_interactions(&inters))
            }
            Command::Evaluate { expr } => self.evaluate(&expr),
            Command::Distance { a, b } => self.distance(&three(&a), &three(&b)),
            Command::Angle { u, v } => self.angle(&three(&u), &three(&v)),
            Command::ExportTex { path } => {
                let built = self.built_visible();
                let vis: Vec<(&Obj, &Built)> = built.iter().map(|(o, b)| (o, b)).collect();
                let (secs, profs, inters) = analysis::analyze_all(&self.scene, &vis, true);
                let tex = export::latex_document(&vis, &self.scene, &secs, &profs, &inters);
                match path {
                    Some(p) => {
                        let f = self.resolve(&p, &["tex"])?;
                        std::fs::write(&f, &tex).map_err(|e| format!("{}: {e}", disp(&f)))?;
                        Ok(format!(
                            "LaTeX geschrieben: {} ({} Zeichen)",
                            disp(&f),
                            tex.len()
                        ))
                    }
                    None => Ok(tex),
                }
            }
            Command::Undo => unreachable!(),
        }
    }

    fn set_layers(&mut self, id: u64, layers: Option<&Value>) -> Result<(), String> {
        let Some(l) = layers else { return Ok(()) };
        check_layer_keys(l)?;
        let o = self
            .scene
            .objects
            .iter_mut()
            .find(|o| o.id == id)
            .ok_or("Objekt fehlt")?;
        let mut cur = serde_json::to_value(&o.layers).map_err(|e| e.to_string())?;
        merge(&mut cur, l);
        o.layers = serde_json::from_value(cur).map_err(|e| format!("layers: {e}"))?;
        Ok(())
    }

    // ----------------------------------------------------------- exakte Rechnung

    fn ring_for(&self, strs: &[String]) -> Result<crate::algebra::Ring, String> {
        let refs: Vec<&String> = strs.iter().collect();
        let keys = scene::gen_keys_of(&refs, &self.scene)?;
        scene::scene_ring(&self.scene, &keys)
    }

    fn exact_of(
        &self,
        ring: &crate::algebra::Ring,
        s: &str,
    ) -> Result<crate::algebra::Sym, String> {
        let e = expr::parse(s).map_err(|e| format!("„{s}“: {e}"))?;
        expr::exact(&e, ring)
            .map(|x| ring.normalize(x))
            .map_err(|e| format!("„{s}“: {e}"))
    }

    fn num_of(&self, s: &str) -> Result<f64, String> {
        scene::Ctx::new(&self.scene).num(s)
    }

    fn field_note(ring: &crate::algebra::Ring) -> String {
        let l = ring.gen_legend(false);
        if l.is_empty() {
            "Zahlkörper: ℚ".into()
        } else {
            format!("Zahlkörper: {}", l.join("; "))
        }
    }

    fn evaluate(&self, s: &str) -> Result<String, String> {
        let num = self.num_of(s);
        let ring = self.ring_for(&[s.to_string()])?;
        let e = expr::parse(s).map_err(|e| format!("„{s}“: {e}"))?;
        let exact = match expr::exact(&e, &ring) {
            Ok(v) => Ok(poly::sym_str(&ring, &ring.normalize(v), false)),
            Err(err) => match &e {
                Ex::Call(f, a) if f == "sqrt" && a.len() == 1 => expr::exact(&a[0], &ring)
                    .map(|x| poly::rad_str(&ring, &ring.sqrt(&ring.normalize(x)), false)),
                _ => Err(err),
            },
        };
        let mut out = String::new();
        match &exact {
            Ok(x) => {
                let _ = writeln!(out, "exakt: {x}");
            }
            Err(e) => {
                let _ = writeln!(out, "nicht exakt darstellbar: {e}");
            }
        }
        match &num {
            Ok(v) => {
                let _ = writeln!(out, "Zahlwert: ≈ {v:.15} (Parameter mit aktuellen Werten)");
            }
            Err(e) => {
                let _ = writeln!(out, "kein Zahlwert: {e}");
            }
        }
        out.push_str(&Self::field_note(&ring));
        if exact.is_err() && num.is_err() {
            return Err(out);
        }
        Ok(out)
    }

    fn distance(&self, a: &[String; 3], b: &[String; 3]) -> Result<String, String> {
        let all: Vec<String> = a.iter().chain(b.iter()).cloned().collect();
        let ring = self.ring_for(&all)?;
        let mut d2 = ring.sym_c(crate::algebra::qi(0));
        let mut n2 = 0.0;
        for k in 0..3 {
            let x = ring.ssub(&self.exact_of(&ring, &a[k])?, &self.exact_of(&ring, &b[k])?);
            d2 = ring.sadd(&d2, &ring.smul(&x, &x));
            let dn = self.num_of(&a[k])? - self.num_of(&b[k])?;
            n2 += dn * dn;
        }
        let d2 = ring.normalize(d2);
        Ok(format!(
            "|PQ|² = {}\n|PQ| = {}\nZahlwert: ≈ {:.15}\n{}",
            poly::sym_str(&ring, &d2, false),
            poly::rad_str(&ring, &ring.sqrt(&d2), false),
            n2.sqrt(),
            Self::field_note(&ring)
        ))
    }

    fn angle(&self, u: &[String; 3], v: &[String; 3]) -> Result<String, String> {
        let all: Vec<String> = u.iter().chain(v.iter()).cloned().collect();
        let ring = self.ring_for(&all)?;
        let zero = || ring.sym_c(crate::algebra::qi(0));
        let (mut dot, mut nu, mut nv) = (zero(), zero(), zero());
        let (mut fd, mut fu, mut fv) = (0.0, 0.0, 0.0);
        for k in 0..3 {
            let x = self.exact_of(&ring, &u[k])?;
            let y = self.exact_of(&ring, &v[k])?;
            dot = ring.sadd(&dot, &ring.smul(&x, &y));
            nu = ring.sadd(&nu, &ring.smul(&x, &x));
            nv = ring.sadd(&nv, &ring.smul(&y, &y));
            let (a, b) = (self.num_of(&u[k])?, self.num_of(&v[k])?);
            fd += a * b;
            fu += a * a;
            fv += b * b;
        }
        if fu == 0.0 || fv == 0.0 {
            return Err("Nullvektor".into());
        }
        let c2 = ring.normalize(ring.sdiv(&ring.smul(&dot, &dot), &ring.smul(&nu, &nv))?);
        let c = (fd / (fu * fv).sqrt()).clamp(-1.0, 1.0);
        let sign = if c < -1e-12 { "−" } else { "" };
        Ok(format!(
            "u·v = {}\ncos²∠ = {}\ncos∠ = {sign}{}\n∠ ≈ {:.10}° (cos ≈ {c:.15})\n{}",
            poly::sym_str(&ring, &ring.normalize(dot), false),
            poly::sym_str(&ring, &c2, false),
            poly::rad_str(&ring, &ring.sqrt(&c2), false),
            c.acos().to_degrees(),
            Self::field_note(&ring)
        ))
    }
}

/// Pfad zur Anzeige; ohne das Windows-Präfix `\\?\` aus `canonicalize`.
fn disp(p: &Path) -> String {
    let s = p.display().to_string();
    match s.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC") => rest.to_string(),
        _ => s,
    }
}

fn mtime_of(p: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

fn merge(base: &mut Value, patch: &Value) {
    match (base, patch) {
        (Value::Object(b), Value::Object(p)) => {
            for (k, v) in p {
                match b.get_mut(k) {
                    Some(bv) if bv.is_object() && v.is_object() => merge(bv, v),
                    _ => {
                        b.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (b, p) => *b = p.clone(),
    }
}

fn check_layer_keys(l: &Value) -> Result<(), String> {
    let Value::Object(m) = l else {
        return Err("layers muss ein JSON-Objekt sein".into());
    };
    let def = serde_json::to_value(Layers::default()).map_err(|e| e.to_string())?;
    for k in m.keys() {
        if def.get(k).is_none() {
            let all: Vec<String> = def.as_object().unwrap().keys().cloned().collect();
            return Err(format!(
                "unbekannte Ebene „{k}“; erlaubt: {}",
                all.join(", ")
            ));
        }
    }
    Ok(())
}

fn find_template(q: &str) -> Result<scene::Template, String> {
    let ts = scene::templates();
    if let Ok(i) = q.trim().parse::<usize>() {
        let n = ts.len();
        return ts
            .into_iter()
            .nth(i.wrapping_sub(1))
            .ok_or_else(|| format!("Vorlage {i} gibt es nicht (1 … {n})"));
    }
    let lc = |s: &str| s.to_lowercase();
    let q = lc(q.trim());
    let full = |t: &scene::Template| lc(&format!("{}/{}", t.group, t.name));
    let pick = |pred: &dyn Fn(&scene::Template) -> bool| -> Vec<usize> {
        ts.iter()
            .enumerate()
            .filter(|(_, t)| pred(t))
            .map(|(i, _)| i)
            .collect()
    };
    let mut hits = pick(&|t| lc(t.name) == q || full(t) == q);
    if hits.is_empty() {
        hits = pick(&|t| lc(t.name).contains(&q) || full(t).contains(&q));
    }
    match hits.len() {
        0 => Err(format!("keine Vorlage passt zu „{q}“ (list_templates)")),
        1 => Ok(ts.into_iter().nth(hits[0]).unwrap()),
        _ => Err(format!(
            "mehrdeutig: {} – bitte Nummer oder „Gruppe/Name“ angeben",
            hits.iter()
                .map(|&i| format!("{}. {}/{}", i + 1, ts[i].group, ts[i].name))
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

// ---------------------------------------------------------------------------
// Textausgabe (Unicode, für Sprachmodelle und Menschen gleichermaßen lesbar)
// ---------------------------------------------------------------------------

fn fmt_object(o: &Obj, b: &Built) -> String {
    let mut s = format!(
        "■ #{} „{}“ – {}{}\n",
        o.id,
        o.name,
        o.kind.type_name(),
        if o.visible { "" } else { " (ausgeblendet)" }
    );
    for e in &b.errors {
        let _ = writeln!(s, "  Fehler: {e}");
    }
    let has_sym = b.formulas.iter().any(|l| l.label.contains("Symmetrie"));
    if let (Some(sym), false) = (&b.sym, has_sym) {
        let _ = writeln!(s, "  Symmetrie: {} (Ordnung {})", sym.name_uni, sym.order);
    }
    for l in &b.formulas {
        let mut line = format!("  {}: {}", l.label, l.uni);
        if let Some(n) = l.num {
            if !l.uni.contains('≈') {
                let _ = write!(line, "   [≈ {n:.10}{}]", l.unit);
            }
        }
        s.push_str(&line);
        s.push('\n');
    }
    if !b.legend.is_empty() {
        let _ = writeln!(s, "  Zahlkörper: {}", b.legend.join("; "));
    }
    for w in &b.warnings {
        let _ = writeln!(s, "  Hinweis: {w}");
    }
    s
}

fn rel(t: &str) -> String {
    if t.starts_with('≈') {
        t.to_string()
    } else {
        format!("= {t}")
    }
}

fn fmt_sections(scene: &Scene, secs: &[SectionRes], profs: &[Profile]) -> String {
    let mut s = String::new();
    for (pi, sec) in scene.sections.iter().enumerate() {
        if !sec.visible {
            continue;
        }
        let _ = writeln!(
            s,
            "[{pi}] {}: {}",
            sec.name,
            analysis::plane_text(sec, false)
        );
        let rs: Vec<&SectionRes> = secs.iter().filter(|r| r.plane == pi).collect();
        if rs.is_empty() {
            let _ = writeln!(s, "  trifft kein sichtbares Objekt");
        }
        for r in rs {
            let shape = if r.exact {
                format!("exaktes {}-Eck", r.ngon)
            } else if r.loops > 1 {
                format!("{} Schnittkurven, numerisch", r.loops)
            } else {
                "numerisch".to_string()
            };
            let _ = writeln!(s, "  {} ({shape})", r.obj);
            if r.closed {
                let _ = writeln!(s, "    A {}", rel(&r.area_uni));
                let _ = writeln!(s, "    U {}", rel(&r.perim_uni));
            } else {
                let _ = writeln!(s, "    Länge der Schnittkurve {}", rel(&r.perim_uni));
            }
            if r.exact && !r.points.is_empty() {
                let pts: Vec<&str> = r.points.iter().take(12).map(|p| p.uni.as_str()).collect();
                let _ = writeln!(s, "    Ecken: {}", pts.join(", "));
                let _ = writeln!(
                    s,
                    "    gültig, solange die Ebene dieselben Kanten schneidet"
                );
            }
            if !r.note.is_empty() {
                let _ = writeln!(s, "    Hinweis: {}", r.note);
            }
        }
        for p in profs.iter().filter(|p| p.plane == pi) {
            let (hmax, amax) =
                p.samples
                    .iter()
                    .cloned()
                    .fold((0.0, f64::MIN), |m, x| if x.1 > m.1 { x } else { m });
            let dev = if p.volume > 0.0 {
                (p.integral - p.volume).abs() / p.volume * 100.0
            } else {
                0.0
            };
            let _ = writeln!(
                s,
                "  Cavalieri-Profil {}: h ∈ [{:.4}, {:.4}], max A ≈ {amax:.6} bei h ≈ {hmax:.4}; ∫A dh ≈ {:.6}, Volumen ≈ {:.6} (Abweichung {dev:.3} %)",
                p.obj,
                p.samples.first().map(|x| x.0).unwrap_or(0.0),
                p.samples.last().map(|x| x.0).unwrap_or(0.0),
                p.integral,
                p.volume
            );
        }
    }
    s
}

fn fmt_interactions(inters: &[InterRes]) -> String {
    let mut s = String::new();
    for r in inters {
        let _ = writeln!(
            s,
            "{} ↔ {} ({})",
            r.a,
            r.b,
            if r.exact {
                "exakt im gemeinsamen Zahlkörper geprüft"
            } else {
                "numerisch"
            }
        );
        if r.is_empty() {
            match &r.near {
                Some((d, a, b)) => {
                    let _ = writeln!(
                        s,
                        "  keine Berührung; kleinster Abstand ≈ {d:.6} ({a} / {b})"
                    );
                }
                None => {
                    let _ = writeln!(s, "  keine Berührung");
                }
            }
        }
        if !r.shared.is_empty() {
            let pts: Vec<&str> = r.shared.iter().take(12).map(|p| p.uni.as_str()).collect();
            let _ = writeln!(
                s,
                "  Gemeinsame Knoten ({}): {}",
                r.shared.len(),
                pts.join(", ")
            );
        }
        let (bundles, rest) = analysis::bundles(&r.crossings);
        let side = |v: &[(String, usize)]| {
            v.iter()
                .map(|(l, n)| format!("{n}× {l}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        for bu in &bundles {
            let _ = writeln!(
                s,
                "  Bündelpunkt {}: {} von {} und {} von {}; Paarwinkel ({} verschiedene):",
                bu.p.uni,
                side(&bu.a),
                r.a,
                side(&bu.b),
                r.b,
                bu.angles.len()
            );
            for (u, _, ang) in &bu.angles {
                let _ = writeln!(s, "    ∠ ≈ {ang:.4}°: |cos ∠| {}", rel(u));
            }
        }
        for g in analysis::crossing_groups(&rest) {
            let pts: Vec<&str> = g.6.iter().take(6).map(|p| p.uni.as_str()).collect();
            let _ = writeln!(
                s,
                "  {} {} × {} ({}×), ∠ ≈ {:.4}°, |cos ∠| {}: bei {}{}",
                g.2,
                g.0,
                g.1,
                g.6.len(),
                g.5,
                rel(&g.3),
                pts.join(", "),
                if g.6.len() > 6 { ", …" } else { "" }
            );
        }
        for p in &r.pierces {
            let each = p
                .each_uni
                .as_ref()
                .map(|e| format!(", je {e}"))
                .unwrap_or_default();
            let _ = writeln!(
                s,
                "  {} von {} in {}: {} ({}×{each}), Innenlänge gesamt {}",
                p.label,
                p.owner,
                p.body,
                p.kind,
                p.count,
                rel(&p.total_uni)
            );
        }
        for (a, b, l) in &r.overlaps {
            let _ = writeln!(s, "  Überlappung {a} / {b}: gemeinsame Länge ≈ {l:.6}");
        }
        if let (Some((d, a, b)), false) = (&r.near, r.is_empty()) {
            let _ = writeln!(
                s,
                "  kleinster Abstand nicht berührender Streben ≈ {d:.6} ({a} / {b})"
            );
        }
        if r.near_misses > 0 {
            let _ = writeln!(
                s,
                "  {} numerische Beinahe-Treffer, exakt widerlegt",
                r.near_misses
            );
        }
        for n in &r.notes {
            let _ = writeln!(s, "  Hinweis: {n}");
        }
    }
    s
}

// ---------------------------------------------------------------------------
// Werkzeugbeschreibungen
// ---------------------------------------------------------------------------

const INSTRUCTIONS: &str = "Strebwerk ist ein exaktes 3D-Geometriewerkzeug: Polyeder, freie Punktkonstruktionen, \
Flächen und Kurven mit Verstrebungen (Kanten, Diagonalen, Speichen …). Alle Formeln werden exakt in \
Zahlkörpern wie ℚ(φ), ℚ(√2), ℚ(ρ) (ρ³ = ρ + 1, plastische Zahl) berechnet und – wo möglich – als reine \
Potenz angegeben (z. B. √2·ρ^(5/2)). Grundsatz: Rechne Längen, Winkel, Flächen und Vereinfachungen NICHT \
selbst, sondern mit analyze, evaluate, distance, angle, sections und interactions – deren Ergebnisse sind \
bewiesen exakt, deine Kopfrechnung nicht. Ausdrücke: + − * / ^, sqrt, Konstanten phi, rho, sqrt2, sqrt3, \
sqrt5, pi; Parameter der Szene (Standard a, t, h) bleiben in Formeln Unbestimmte. Ablauf: scene_info → \
Objekte anlegen (add_template, add_points, add_object) → analyze / sections / interactions → bei Bedarf \
save_scene (dann wird nach jeder Änderung automatisch gespeichert, und die geöffnete Strebwerk-GUI lädt \
die Datei live nach) → export_tex.";

fn term_schema(desc: &str) -> Value {
    json!({ "type": ["string", "number"], "description": desc })
}

fn vec3_schema(desc: &str) -> Value {
    json!({
        "type": "array",
        "items": { "type": ["string", "number"] },
        "minItems": 3, "maxItems": 3,
        "description": desc
    })
}

fn layers_schema() -> Value {
    json!({
        "type": "object",
        "description": "Darstellungs- und Strebenebenen (nur angegebene Schlüssel werden geändert), z. B. \
    {\"space_diag\": true, \"face_diag\": true, \"spokes\": true, \"dual\": true, \"axes\": true, \"mirrors\": true, \
    \"spheres\": true, \"cone_tet\": true, \"all_pairs\": false, \"labels\": true, \"lattice\": true, \"tpms\": true, \
    \"chords\": true}. Speichen (spokes) und Kegeltetraeder (cone_tet) fügen den exakten Schwerpunkt als Ecke hinzu."
    })
}

fn tool(name: &str, desc: &str, props: Value, required: &[&str], read_only: bool) -> Value {
    json!({
        "name": name,
        "description": desc,
        "inputSchema": {
            "type": "object",
            "properties": props,
            "required": required,
            "additionalProperties": false
        },
        "annotations": { "readOnlyHint": read_only }
    })
}

pub fn tools() -> Vec<Value> {
    vec![
        tool("scene_info", "Überblick über die Szene: Parameter, Generatoren, Objekte (mit id und Definition), Schnittebenen, Arbeitsdatei.", json!({}), &[], true),
        tool("scene_json", "Vollständige Szene als JSON (Strebwerk-Format), z. B. um eine Definition exakt nachzulesen.", json!({}), &[], true),
        tool("new_scene", "Neue, leere Szene (Parameter a, t, h bleiben). example=true: Beispielszene mit Ikosaeder. Löst die Arbeitsdatei.",
            json!({ "example": { "type": "boolean" } }), &[], false),
        tool("load_scene", "Szene aus .strebwerk-Datei laden (Pfad relativ zum freigegebenen Ordner); sie wird Arbeitsdatei.",
            json!({ "path": { "type": "string" } }), &["path"], false),
        tool("save_scene", "Szene speichern und als Arbeitsdatei setzen: danach wird nach jeder Änderung automatisch gespeichert und die geöffnete GUI lädt live nach. Ohne path: in die aktuelle Arbeitsdatei.",
            json!({ "path": { "type": "string", "description": "z. B. \"szenen/kegel.strebwerk\"" } }), &[], false),
        tool("list_templates", "Alle Vorlagen (Platonische und archimedische Körper, Quader inkl. 1 × ρ × ρ³, Prismen, Antiprismen, Flächen, Kurven) mit Nummer.", json!({}), &[], true),
        tool("add_template", "Objekt aus Vorlage anlegen (Nummer, Name oder „Gruppe/Name“; Teilwort genügt, wenn eindeutig).",
            json!({
                "template": { "type": "string" },
                "name": { "type": "string" },
                "layers": layers_schema()
            }), &["template"], false),
        tool("add_points", "Freie Konstruktion aus exakten Punkten anlegen, z. B. points=[[0,0,0],[\"1\",\"rho\",\"1+rho\"],…]. hull=true (Standard): konvexe Hülle als Körper mit allen Kanten, Flächen, Volumen. struts: zusätzliche Streben als Indexpaare.",
            json!({
                "name": { "type": "string" },
                "points": { "type": "array", "items": vec3_schema("Punkt (x, y, z) als exakte Ausdrücke") },
                "hull": { "type": "boolean" },
                "struts": { "type": "array", "items": { "type": "array", "items": { "type": "integer", "minimum": 0 }, "minItems": 2, "maxItems": 2 } },
                "layers": layers_schema()
            }), &["name", "points"], false),
        tool("add_object", "Beliebiges Objekt im Strebwerk-Format anlegen. kind-Beispiele: \
{\"Polyhedron\":{\"base\":{\"Platonic\":{\"kind\":\"Cube\",\"a\":\"2+2*rho\"}},\"ops\":[]}} (kind: Tetra|Cube|Octa|Dodeca|Icosa); \
{\"Polyhedron\":{\"base\":{\"Cuboid\":{\"a\":\"1\",\"b\":\"rho\",\"c\":\"rho^3\"}},\"ops\":[\"Dual\",{\"Scale\":\"2\"},{\"Truncate\":\"1/3\"}]}}; \
{\"Polyhedron\":{\"base\":{\"Antiprism\":{\"n\":5,\"r\":\"1\",\"h\":\"h\"}},\"ops\":[]}} (ebenso Prism, Pyramid, Bipyramid); \
{\"ParamSurface\":{\"x\":\"(2+cos(v))*cos(u)\",\"y\":\"(2+cos(v))*sin(u)\",\"z\":\"sin(v)\",\"u0\":\"0\",\"u1\":\"2*pi\",\"v0\":\"0\",\"v1\":\"2*pi\",\"nu\":64,\"nv\":32}}; \
{\"Implicit\":{\"f\":\"x^4+y^4+z^4-1\",\"lo\":[\"-1.5\",\"-1.5\",\"-1.5\"],\"hi\":[\"1.5\",\"1.5\",\"1.5\"],\"res\":48}}; \
{\"Curve\":{\"x\":\"cos(t)\",\"y\":\"sin(t)\",\"z\":\"t/5\",\"t0\":\"0\",\"t1\":\"6*pi\",\"n\":300}}.",
            json!({
                "name": { "type": "string" },
                "kind": { "type": "object" },
                "layers": layers_schema()
            }), &["name", "kind"], false),
        tool("update_object", "Objekt ändern: patch wird in das Objekt-JSON eingemischt (Objekte rekursiv, Listen ersetzt). Schlüssel: name, visible, kind, offset ([x,y,z] als Ausdrücke), color ([r,g,b]), layers. Beispiel: {\"layers\":{\"spokes\":true}} oder {\"kind\":{\"Polyhedron\":{\"base\":{\"Platonic\":{\"a\":\"2\"}}}}}.",
            json!({ "id": { "type": "integer" }, "patch": { "type": "object" } }), &["id", "patch"], false),
        tool("remove_object", "Objekt entfernen.", json!({ "id": { "type": "integer" } }), &["id"], false),
        tool("set_param", "Parameter anlegen oder ändern. In exakten Formeln bleibt ein Parameter eine Unbestimmte (z. B. Lage s einer Schnittebene → A(s) als Polynom); value bestimmt nur Darstellung und Zahlwerte.",
            json!({
                "name": { "type": "string" },
                "value": { "type": "number" },
                "min": { "type": "number" },
                "max": { "type": "number" }
            }), &["name"], false),
        tool("add_generator", "Eigenen algebraischen Generator anlegen: Nullstelle eines irreduziblen Polynoms nahe approx, z. B. key=\"tau\", poly=\"x^3 - x^2 - x - 1\", approx=1.839 (Tribonacci).",
            json!({
                "key": { "type": "string" },
                "poly": { "type": "string" },
                "approx": { "type": "number" }
            }), &["key", "poly", "approx"], false),
        tool("add_section", "Schnittebene n·x = d anlegen (axis \"x\"/\"y\"/\"z\" oder normal [nx,ny,nz]) und sofort auswerten: exaktes Schnittpolygon, Fläche A, Umfang U, Cavalieri-Profil. Tipp: offset als Parameter (z. B. \"s\" nach set_param) liefert A(s) als Formel statt Dezimalbrüchen.",
            json!({
                "axis": { "type": "string", "enum": ["x", "y", "z"] },
                "normal": vec3_schema("Normalenvektor als Ausdrücke"),
                "offset": term_schema("d als Ausdruck, z. B. \"s\", \"rho/2\", 0"),
                "profile": { "type": "boolean", "description": "Cavalieri-Profil berechnen (Standard true)" },
                "name": { "type": "string" }
            }), &["offset"], false),
        tool("remove_section", "Schnittebene entfernen (Index aus scene_info).", json!({ "index": { "type": "integer", "minimum": 0 } }), &["index"], false),
        tool("analyze", "Exakte Formeln eines Objekts (oder aller sichtbaren): Kanten-/Diagonalklassen mit Anzahl, Flächen, Oberfläche, Volumen, Um-/In-/Kantenkugel, Diederwinkel, Symmetriegruppe, Topologie; jeweils mit Potenzform, falls vorhanden.",
            json!({ "id": { "type": "integer" } }), &[], true),
        tool("sections", "Alle Schnittebenen mit allen sichtbaren Objekten auswerten (exakt, wo möglich).", json!({}), &[], true),
        tool("interactions", "Wie die Streben verschiedener Objekte zusammenwirken: gemeinsame Knoten, Kreuzungen und Stöße mit exaktem Punkt und |cos ∠|, Bündelpunkte, Durchstoßungen mit exakter Innenlänge, Überlappungen, kleinste Abstände.", json!({}), &[], true),
        tool("evaluate", "Ausdruck exakt vereinfachen (inkl. Potenzform), z. B. \"rho^2 - 1\" → ρ^(-1), \"sqrt(2+2*rho+2*rho^2)\" → √2·ρ^(5/2), \"cos(2*pi/5)\".",
            json!({ "expr": { "type": "string" } }), &["expr"], true),
        tool("distance", "Exakter Abstand zweier Punkte (mit Potenzform).",
            json!({ "a": vec3_schema("Punkt P"), "b": vec3_schema("Punkt Q") }), &["a", "b"], true),
        tool("angle", "Exakter Winkel zwischen zwei Vektoren: u·v, cos²∠, cos∠ (mit Potenzform) und Gradmaß.",
            json!({ "u": vec3_schema("Vektor u"), "v": vec3_schema("Vektor v") }), &["u", "v"], true),
        tool("export_tex", "LaTeX-Dokument mit allen Formeln, Schnitten, Profilen und Interaktionen. Mit path (.tex, relativ zum freigegebenen Ordner) wird die Datei geschrieben, sonst der Quelltext zurückgegeben.",
            json!({ "path": { "type": "string" } }), &[], false),
        tool("undo", "Letzte Änderung rückgängig machen.", json!({}), &[], false),
    ]
}

// ---------------------------------------------------------------------------
// Protokoll
// ---------------------------------------------------------------------------

fn rpc_ok(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn rpc_err(id: &Value, code: i64, msg: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": msg } })
}

/// Eine JSON-RPC-Nachricht verarbeiten; `None` bei Benachrichtigungen.
pub fn handle(sess: &mut Session, msg: &Value) -> Option<Value> {
    let id = msg.get("id").cloned();
    let method = msg.get("method").and_then(|m| m.as_str());
    let Some(method) = method else {
        // Antworten des Clients (wir stellen keine Anfragen) ignorieren
        return id.map(|i| rpc_err(&i, -32600, "ungültige Anfrage"));
    };
    let id = id?; // Benachrichtigung (z. B. notifications/initialized)
    let params = msg.get("params");
    Some(match method {
        "initialize" => {
            let asked = params
                .and_then(|p| p.get("protocolVersion"))
                .and_then(|v| v.as_str())
                .unwrap_or(PROTOCOL_DEFAULT);
            let version = if PROTOCOL_VERSIONS.contains(&asked) {
                asked
            } else {
                PROTOCOL_DEFAULT
            };
            rpc_ok(
                &id,
                json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": "strebwerk", "title": "Strebwerk", "version": env!("CARGO_PKG_VERSION") },
                    "instructions": INSTRUCTIONS
                }),
            )
        }
        "ping" => rpc_ok(&id, json!({})),
        "tools/list" => rpc_ok(&id, json!({ "tools": tools() })),
        "tools/call" => {
            let name = params.and_then(|p| p.get("name")).and_then(|n| n.as_str());
            let Some(name) = name else {
                return Some(rpc_err(&id, -32602, "Werkzeugname fehlt"));
            };
            if !tools().iter().any(|t| t["name"] == name) {
                return Some(rpc_err(
                    &id,
                    -32602,
                    &format!("unbekanntes Werkzeug: {name}"),
                ));
            }
            let res = Command::from_call(name, params.and_then(|p| p.get("arguments")))
                .and_then(|c| sess.apply(c));
            let (text, is_err) = match res {
                Ok(t) => (t, false),
                Err(e) => (e, true),
            };
            rpc_ok(
                &id,
                json!({ "content": [{ "type": "text", "text": text }], "isError": is_err }),
            )
        }
        m if m.starts_with("notifications/") => return None,
        "resources/list" => rpc_ok(&id, json!({ "resources": [] })),
        "prompts/list" => rpc_ok(&id, json!({ "prompts": [] })),
        _ => rpc_err(&id, -32601, &format!("unbekannte Methode: {method}")),
    })
}

/// Protokollschleife: liest JSON-RPC-Zeilen, schreibt Antworten. Protokolle gehen nach stderr.
pub fn serve<R: BufRead, W: Write>(
    sess: &mut Session,
    input: R,
    mut out: W,
) -> std::io::Result<()> {
    for line in input.lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(line) {
            Err(e) => Some(rpc_err(&Value::Null, -32700, &format!("Parse-Fehler: {e}"))),
            Ok(Value::Array(batch)) => {
                let rs: Vec<Value> = batch.iter().filter_map(|m| handle(sess, m)).collect();
                if rs.is_empty() {
                    None
                } else {
                    Some(Value::Array(rs))
                }
            }
            Ok(m) => handle(sess, &m),
        };
        if let Some(r) = reply {
            writeln!(out, "{}", serde_json::to_string(&r).unwrap_or_default())?;
            out.flush()?;
        }
    }
    Ok(())
}

/// `strebwerk --mcp [--root ORDNER] [szene.strebwerk]`
pub fn main(args: &[String]) -> Result<(), String> {
    let mut root: Option<PathBuf> = None;
    let mut scene_file: Option<String> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--root" => {
                root = Some(PathBuf::from(
                    it.next().ok_or("--root braucht einen Ordner")?,
                ))
            }
            s if !s.starts_with("--") => scene_file = Some(s.to_string()),
            s => return Err(format!("unbekannte Option {s}")),
        }
    }
    let root = match root {
        Some(r) => r,
        None => std::env::current_dir().map_err(|e| e.to_string())?,
    };
    if !root.is_dir() {
        return Err(format!("{} ist kein Ordner", disp(&root)));
    }
    let mut sess = Session::new(root);
    if let Some(f) = scene_file {
        sess.apply(Command::LoadScene { path: f })?;
    }
    eprintln!("strebwerk MCP-Server bereit (Ordner {})", disp(&sess.root));
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    serve(&mut sess, stdin.lock(), stdout.lock()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sess() -> Session {
        Session::new(std::env::temp_dir())
    }

    fn call(s: &mut Session, name: &str, args: Value) -> Result<String, String> {
        Command::from_call(name, Some(&args)).and_then(|c| s.apply(c))
    }

    #[test]
    fn julians_tetraeder() {
        let mut s = sess();
        let out = call(
            &mut s,
            "add_points",
            json!({
                "name": "T",
                "points": [[0,0,0],["1","rho","rho^3"],["rho","rho^3","1"],["1","rho^3","rho"]]
            }),
        )
        .unwrap();
        assert!(out.contains("#1"), "{out}");
        let a = call(&mut s, "analyze", json!({})).unwrap();
        assert!(a.contains("ρ^(5/2)"), "{a}");
        assert!(a.contains("√2·ρ^(-4)"), "{a}");
        assert!(a.contains("1/3·ρ^(-1)"), "{a}");
        let d = call(
            &mut s,
            "distance",
            json!({ "a": ["1","rho","1+rho"], "b": ["1","1+rho","rho"] }),
        )
        .unwrap();
        assert!(d.contains("|PQ| = √2"), "{d}");
        let e = call(&mut s, "evaluate", json!({ "expr": "rho^2 - 1" })).unwrap();
        assert!(e.contains("ρ^(-1)"), "{e}");
        let w = call(
            &mut s,
            "angle",
            json!({ "u": ["1","rho","rho^3"], "v": [1,1,-1] }),
        )
        .unwrap();
        assert!(w.contains("cos²∠ = 0"), "{w}");
    }

    #[test]
    fn schnitt_mit_parameter() {
        let mut s = sess();
        call(&mut s, "add_template", json!({ "template": "Oktaeder" })).unwrap();
        call(
            &mut s,
            "set_param",
            json!({ "name": "s", "value": 0.3, "min": 0, "max": 1 }),
        )
        .unwrap();
        let out = call(&mut s, "add_section", json!({ "axis": "z", "offset": "s" })).unwrap();
        assert!(out.contains("exaktes 4-Eck"), "{out}");
        assert!(out.contains('s'), "{out}");
    }

    #[test]
    fn fehler_lassen_szene_unveraendert() {
        let mut s = sess();
        let e = call(
            &mut s,
            "add_points",
            json!({ "name": "X", "points": [["1+","0","0"]] }),
        );
        assert!(e.is_err());
        assert!(s.scene.objects.is_empty());
        let e = call(
            &mut s,
            "add_template",
            json!({ "template": "Würfel", "layers": { "spokez": true } }),
        );
        assert!(e.unwrap_err().contains("unbekannte Ebene"));
        assert!(s.scene.objects.is_empty());
        // Tippfehler im Argumentnamen fällt auf
        assert!(call(&mut s, "add_section", json!({ "axis": "z", "ofset": 1 })).is_err());
    }

    #[test]
    fn aendern_und_rueckgaengig() {
        let mut s = sess();
        call(
            &mut s,
            "add_template",
            json!({ "template": "Platonisch/Würfel" }),
        )
        .unwrap();
        let id = s.scene.objects[0].id;
        call(
            &mut s,
            "update_object",
            json!({ "id": id, "patch": { "name": "W", "layers": { "spokes": true },
                    "kind": { "Polyhedron": { "base": { "Platonic": { "a": "2" } } } } } }),
        )
        .unwrap();
        let o = &s.scene.objects[0];
        assert_eq!(o.name, "W");
        assert!(o.layers.spokes);
        assert!(
            matches!(&o.kind, Kind::Polyhedron { base: scene::PolyBase::Platonic { a, .. }, .. } if a == "2")
        );
        call(&mut s, "undo", json!({})).unwrap();
        assert_eq!(s.scene.objects[0].name, "Würfel");
        // Typwechsel ersetzt das kind vollständig
        call(
            &mut s,
            "update_object",
            json!({ "id": id, "patch": { "kind": { "Free": { "pts": [["0","0","0"],["1","0","0"]], "struts": [[0,1]], "hull": false } } } }),
        )
        .unwrap();
        assert!(matches!(s.scene.objects[0].kind, Kind::Free { .. }));
    }

    #[test]
    fn pfade_bleiben_im_ordner() {
        let dir = std::env::temp_dir().join(format!("strebwerk-mcp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut s = Session::new(dir.clone());
        assert!(call(
            &mut s,
            "save_scene",
            json!({ "path": "../flucht.strebwerk" })
        )
        .is_err());
        assert!(call(&mut s, "save_scene", json!({ "path": "x.txt" })).is_err());
        call(&mut s, "save_scene", json!({ "path": "a.strebwerk" })).unwrap();
        call(&mut s, "add_template", json!({ "template": "Tetraeder" })).unwrap();
        let txt = std::fs::read_to_string(dir.join("a.strebwerk")).unwrap();
        assert!(txt.contains("Tetra"), "Autospeichern");
        let mut t = Session::new(dir.clone());
        call(&mut t, "load_scene", json!({ "path": "a.strebwerk" })).unwrap();
        assert_eq!(t.scene.objects.len(), 1);
        // Änderung von außen (hier: zweite Sitzung statt GUI) wird vor dem nächsten Befehl übernommen
        std::thread::sleep(std::time::Duration::from_millis(20));
        call(&mut t, "add_template", json!({ "template": "Oktaeder" })).unwrap();
        let out = call(&mut s, "scene_info", json!({})).unwrap();
        assert!(
            out.contains("übernommen") && out.contains("Oktaeder"),
            "{out}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn protokoll() {
        let mut s = sess();
        let input = [
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"0"}}}),
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"evaluate","arguments":{"expr":"(1+sqrt5)/2"}}}),
            json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"gibtsnicht","arguments":{}}}),
            json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"evaluate","arguments":{}}}),
        ]
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join("\n");
        let mut out = Vec::new();
        serve(&mut s, input.as_bytes(), &mut out).unwrap();
        let lines: Vec<Value> = String::from_utf8(out)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 5, "keine Antwort auf Benachrichtigungen");
        assert_eq!(lines[0]["result"]["protocolVersion"], "2025-06-18");
        let n = lines[1]["result"]["tools"].as_array().unwrap().len();
        assert_eq!(n, tools().len());
        assert!(lines[2]["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("φ"));
        assert_eq!(lines[3]["error"]["code"], -32602);
        assert_eq!(lines[4]["result"]["isError"], true);
        // jedes Werkzeug lässt sich in einen Befehl übersetzen (Schema und Enum passen zusammen)
        for t in tools() {
            let name = t["name"].as_str().unwrap();
            let e = Command::from_call(name, Some(&json!({})))
                .err()
                .unwrap_or_default();
            assert!(!e.contains("unknown variant"), "{name}: {e}");
        }
    }
}

#[cfg(test)]
mod disp_tests {
    #[test]
    fn windows_praefix() {
        use std::path::Path;
        assert_eq!(
            super::disp(Path::new(r"\\?\C:\Users\x\a.tex")),
            r"C:\Users\x\a.tex"
        );
        assert_eq!(super::disp(Path::new("/tmp/a.tex")), "/tmp/a.tex");
    }
}
