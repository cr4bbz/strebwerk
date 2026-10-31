//! Oberfläche.

use crate::analysis::{self, InterRes, Profile, SectionRes};
use crate::export;
use crate::expr;
use crate::mesh::{LatticeKind, Tpms};
use crate::render::{self, Camera, Pick, ViewOpts};
use crate::scene::{
    self, Built, Cache, CustomGen, Kind, Obj, Param, Platonic, PolyBase, PolyOp, Scene,
};
use eframe::egui::{self, Color32, RichText, Sense};
use glam::DVec3;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::hash::{Hash, Hasher};

#[derive(Serialize, Deserialize)]
pub struct SaveFile {
    pub format: String,
    pub scene: Scene,
    pub camera: Camera,
}

#[derive(PartialEq, Clone, Copy)]
enum Tool {
    Orbit,
    AddPoint,
    AddStrut,
}

pub struct App {
    scene: Scene,
    cam: Camera,
    built: HashMap<u64, (u64, Built)>,
    cache: Cache,
    selected: Option<u64>,
    grid: bool,
    axes: bool,
    inter_on: bool,
    analysis_key: u64,
    sections_res: Vec<SectionRes>,
    profiles: Vec<Profile>,
    inters: Vec<InterRes>,
    analysis_ms: f64,
    tool: Tool,
    strut_first: Option<usize>,
    quick: String,
    quick_err: String,
    new_param: String,
    status: String,
    show_help: bool,
    stl_mm: f64,
    stl_radius: f64,
    tikz_width: f64,
    fit_pending: bool,
    last_items: Vec<render::Item>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut fonts = egui::FontDefinitions::default();
        fonts.font_data.insert(
            "dejavu".into(),
            std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
                "../assets/DejaVuSans.ttf"
            ))),
        );
        for fam in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts.families.entry(fam).or_default().push("dejavu".into());
        }
        cc.egui_ctx.set_fonts(fonts);
        let mut vis = egui::Visuals::dark();
        vis.panel_fill = Color32::from_rgb(24, 26, 32);
        vis.window_fill = Color32::from_rgb(28, 30, 38);
        vis.selection.bg_fill = Color32::from_rgb(60, 90, 150);
        cc.egui_ctx.set_visuals(vis);
        let mut scene = scene::default_scene();
        let mut cam = Camera::default();
        let mut status = "Willkommen bei Strebwerk. Links ziehen: drehen · rechts/mittel ziehen: verschieben · Rad: zoomen".to_string();
        let mut fit_pending = true;
        if let Some(path) = std::env::args().nth(1) {
            match std::fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|s| serde_json::from_str::<SaveFile>(&s).map_err(|e| e.to_string()))
            {
                Ok(f) => {
                    scene = f.scene;
                    cam = f.camera;
                    fit_pending = false;
                    status = format!("Geöffnet: {path}");
                }
                Err(e) => status = format!("Konnte {path} nicht öffnen: {e}"),
            }
        }
        let selected = scene.objects.first().map(|o| o.id);
        App {
            scene,
            cam,
            built: HashMap::new(),
            cache: Cache::new(),
            selected,
            grid: true,
            axes: true,
            inter_on: true,
            analysis_key: 0,
            sections_res: vec![],
            profiles: vec![],
            inters: vec![],
            analysis_ms: 0.0,
            tool: Tool::Orbit,
            strut_first: None,
            quick: String::new(),
            quick_err: String::new(),
            new_param: String::new(),
            status,
            show_help: false,
            stl_mm: 20.0,
            stl_radius: 0.8,
            tikz_width: 12.0,
            fit_pending,
            last_items: vec![],
        }
    }

    fn obj_key(&self, o: &Obj) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        let js = serde_json::to_string(o).unwrap_or_default();
        js.hash(&mut h);
        // Nur Parameter, die das Objekt erwähnt, lösen einen Neubau aus (Namen gehen immer ein,
        // weil sie den Ring bestimmen).
        for p in &self.scene.params {
            p.name.hash(&mut h);
            p.min.to_bits().hash(&mut h);
            if mentions(&js, &p.name) {
                p.value.to_bits().hash(&mut h);
            }
        }
        serde_json::to_string(&self.scene.gens)
            .unwrap_or_default()
            .hash(&mut h);
        h.finish()
    }

    fn rebuild(&mut self) {
        let ids: BTreeSet<u64> = self.scene.objects.iter().map(|o| o.id).collect();
        self.built.retain(|k, _| ids.contains(k));
        for i in 0..self.scene.objects.len() {
            let o = &self.scene.objects[i];
            let key = self.obj_key(o);
            if self.built.get(&o.id).map(|(k, _)| *k) != Some(key) {
                let b = scene::build(o, &self.scene, &mut self.cache);
                self.built.insert(o.id, (key, b));
            }
        }
        if self.cache.len() > 400 {
            self.cache.clear();
        }
    }

    fn scene_extent(&self) -> (DVec3, f64) {
        let vis: Vec<&Built> = self
            .scene
            .objects
            .iter()
            .filter(|o| o.visible)
            .filter_map(|o| self.built.get(&o.id).map(|x| &x.1))
            .filter(|b| b.radius > 0.0)
            .collect();
        if vis.is_empty() {
            return (DVec3::ZERO, 1.0);
        }
        let c = vis.iter().map(|b| b.center).sum::<DVec3>() / vis.len() as f64;
        let r = vis
            .iter()
            .map(|b| (b.center - c).length() + b.radius)
            .fold(0.0, f64::max);
        (c, r.max(0.1))
    }

    fn fit(&mut self) {
        let (c, r) = self.scene_extent();
        self.cam.target = c;
        self.cam.dist = r / (self.cam.fov * 0.5).sin() * 1.15;
    }

    fn animate(&mut self, dt: f64) -> bool {
        let mut any = false;
        for p in &mut self.scene.params {
            if p.animate {
                any = true;
                let span = (p.max - p.min).max(1e-9);
                p.value += p.dir * p.speed * span * dt;
                if p.value > p.max {
                    p.value = p.max;
                    p.dir = -1.0;
                }
                if p.value < p.min {
                    p.value = p.min;
                    p.dir = 1.0;
                }
            }
        }
        any
    }

    fn add_kind(&mut self, name: &str, kind: Kind) {
        let id = self.scene.add(name, kind);
        self.selected = Some(id);
        self.fit_pending = true;
        self.tool = Tool::Orbit;
    }

    fn selected_obj(&mut self) -> Option<&mut Obj> {
        let id = self.selected?;
        self.scene.objects.iter_mut().find(|o| o.id == id)
    }

    // ------------------------------------------------------------------ Datei

    fn save(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Strebwerk-Szene", &["strebwerk", "json"])
            .set_file_name("szene.strebwerk")
            .save_file()
        else {
            return;
        };
        let f = SaveFile {
            format: "strebwerk-1".into(),
            scene: self.scene.clone(),
            camera: self.cam.clone(),
        };
        match serde_json::to_string_pretty(&f)
            .map_err(|e| e.to_string())
            .and_then(|s| std::fs::write(&path, s).map_err(|e| e.to_string()))
        {
            Ok(()) => self.status = format!("Gespeichert: {}", path.display()),
            Err(e) => self.status = format!("Speichern fehlgeschlagen: {e}"),
        }
    }

    fn open(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Strebwerk-Szene", &["strebwerk", "json"])
            .pick_file()
        else {
            return;
        };
        let res: Result<SaveFile, String> = std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|s| serde_json::from_str(&s).map_err(|e| e.to_string()));
        match res {
            Ok(f) => {
                self.scene = f.scene;
                self.cam = f.camera;
                self.selected = self.scene.objects.first().map(|o| o.id);
                self.built.clear();
                self.status = format!("Geöffnet: {}", path.display());
            }
            Err(e) => self.status = format!("Öffnen fehlgeschlagen: {e}"),
        }
    }

    fn write_file(&mut self, what: &str, ext: &str, default: &str, content: String) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter(what, &[ext])
            .set_file_name(default)
            .save_file()
        else {
            return;
        };
        match std::fs::write(&path, content) {
            Ok(()) => self.status = format!("{what} geschrieben: {}", path.display()),
            Err(e) => self.status = format!("Schreiben fehlgeschlagen: {e}"),
        }
    }

    fn latex_all(&self) -> String {
        let pairs: Vec<(&Obj, &Built)> = self
            .scene
            .objects
            .iter()
            .filter(|o| o.visible)
            .filter_map(|o| self.built.get(&o.id).map(|b| (o, &b.1)))
            .collect();
        export::latex_document(
            &pairs,
            &self.scene,
            &self.sections_res,
            &self.profiles,
            &self.inters,
        )
    }

    /// Schnittebenen und Interaktionen neu auswerten, wenn sich etwas geändert hat.
    fn reanalyze(&mut self) {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for o in self.scene.objects.iter().filter(|o| o.visible) {
            o.id.hash(&mut h);
            self.built.get(&o.id).map(|b| b.0).hash(&mut h);
        }
        serde_json::to_string(&self.scene.sections)
            .unwrap_or_default()
            .hash(&mut h);
        for p in &self.scene.params {
            p.value.to_bits().hash(&mut h);
        }
        self.inter_on.hash(&mut h);
        let key = h.finish();
        if key == self.analysis_key {
            return;
        }
        self.analysis_key = key;
        let t0 = std::time::Instant::now();
        let vis: Vec<(&Obj, &Built)> = self
            .scene
            .objects
            .iter()
            .filter(|o| o.visible)
            .filter_map(|o| self.built.get(&o.id).map(|b| (o, &b.1)))
            .collect();
        let (secs, profs, inters) = analysis::analyze_all(&self.scene, &vis, self.inter_on);
        self.sections_res = secs;
        self.profiles = profs;
        self.inters = inters;
        self.analysis_ms = t0.elapsed().as_secs_f64() * 1000.0;
    }

    // ------------------------------------------------------------------ UI

    fn menu(&mut self, ui: &mut egui::Ui) {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("Datei", |ui| {
                if ui.button("Neue Szene").clicked() {
                    self.scene = scene::default_scene();
                    self.selected = self.scene.objects.first().map(|o| o.id);
                    self.built.clear();
                    self.fit_pending = true;
                    ui.close_menu();
                }
                if ui.button("Öffnen …").clicked() {
                    ui.close_menu();
                    self.open();
                }
                if ui.button("Speichern unter …").clicked() {
                    ui.close_menu();
                    self.save();
                }
                ui.separator();
                if ui.button("Formeln als LaTeX exportieren …").clicked() {
                    ui.close_menu();
                    let s = self.latex_all();
                    self.write_file("LaTeX", "tex", "strebwerk-formeln.tex", s);
                }
                if ui.button("Formeln als LaTeX kopieren").clicked() {
                    ui.ctx().copy_text(self.latex_all());
                    self.status = "LaTeX in die Zwischenablage kopiert".into();
                    ui.close_menu();
                }
                ui.horizontal(|ui| {
                    if ui.button("Ansicht als TikZ exportieren …").clicked() {
                        ui.close_menu();
                        let s = export::tikz(&self.last_items, self.tikz_width);
                        self.write_file("TikZ", "tex", "strebwerk-ansicht.tex", s);
                    }
                    ui.add(
                        egui::DragValue::new(&mut self.tikz_width)
                            .range(4.0..=40.0)
                            .suffix(" cm"),
                    );
                });
                ui.horizontal(|ui| {
                    if ui.button("Gerüst des Objekts als STL …").clicked() {
                        ui.close_menu();
                        if let Some(id) = self.selected {
                            if let Some((_, b)) = self.built.get(&id) {
                                let segs: Vec<(DVec3, DVec3)> =
                                    b.lines.iter().map(|l| (l.a, l.b)).collect();
                                let name = self
                                    .scene
                                    .objects
                                    .iter()
                                    .find(|o| o.id == id)
                                    .map(|o| o.name.clone())
                                    .unwrap_or_default();
                                let s = export::stl_struts(
                                    &name.replace(' ', "_"),
                                    &segs,
                                    self.stl_radius,
                                    self.stl_mm,
                                );
                                self.write_file("STL", "stl", "strebwerk-geruest.stl", s);
                            }
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("   Maßstab");
                    ui.add(
                        egui::DragValue::new(&mut self.stl_mm)
                            .range(1.0..=500.0)
                            .suffix(" mm/E"),
                    );
                    ui.label("Strebenradius");
                    ui.add(
                        egui::DragValue::new(&mut self.stl_radius)
                            .range(0.1..=10.0)
                            .speed(0.05)
                            .suffix(" mm"),
                    );
                });
            });
            ui.menu_button("Hinzufügen", |ui| {
                let mut groups: Vec<&'static str> = vec![];
                for t in scene::templates() {
                    if !groups.contains(&t.group) {
                        groups.push(t.group);
                    }
                }
                for g in groups {
                    ui.menu_button(g, |ui| {
                        for t in scene::templates().into_iter().filter(|t| t.group == g) {
                            if ui.button(t.name).clicked() {
                                self.add_kind(t.name, (t.make)());
                                ui.close_menu();
                            }
                        }
                    });
                }
            });
            ui.menu_button("Ansicht", |ui| {
                ui.checkbox(&mut self.grid, "Bodengitter");
                ui.checkbox(&mut self.axes, "Koordinatenachsen");
                ui.checkbox(&mut self.cam.ortho, "Orthogonalprojektion");
                if ui.button("Szene einpassen").clicked() {
                    self.fit();
                    ui.close_menu();
                }
                ui.separator();
                for (name, yaw, pitch) in [
                    ("Von vorn", -std::f64::consts::FRAC_PI_2, 0.0),
                    ("Von rechts", 0.0, 0.0),
                    ("Von oben", -std::f64::consts::FRAC_PI_2, 1.5707),
                    ("Isometrisch", 0.785398, 0.615_48),
                ] {
                    if ui.button(name).clicked() {
                        self.cam.yaw = yaw;
                        self.cam.pitch = pitch;
                        ui.close_menu();
                    }
                }
            });
            if ui.button("Hilfe").clicked() {
                self.show_help = !self.show_help;
            }
        });
    }

    fn left_panel(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
            ui.heading("Szene");
            let mut delete = None;
            let mut dup = None;
            for o in &mut self.scene.objects {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut o.visible, "");
                    let c = Color32::from_rgb(o.color[0], o.color[1], o.color[2]);
                    let (r, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), Sense::hover());
                    ui.painter().rect_filled(r, 2.0, c);
                    let errs = self.built.get(&o.id).map(|b| !b.1.errors.is_empty()).unwrap_or(false);
                    let label = if errs { RichText::new(format!("{} ⚠", o.name)).color(Color32::from_rgb(255, 150, 120)) } else { RichText::new(&o.name) };
                    if ui.selectable_label(self.selected == Some(o.id), label).on_hover_text(o.kind.type_name()).clicked() {
                        self.selected = Some(o.id);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("✕").on_hover_text("Löschen").clicked() {
                            delete = Some(o.id);
                        }
                        if ui.small_button("⊞").on_hover_text("Duplizieren").clicked() {
                            dup = Some(o.id);
                        }
                    });
                });
            }
            if let Some(id) = delete {
                self.scene.objects.retain(|o| o.id != id);
                if self.selected == Some(id) {
                    self.selected = self.scene.objects.first().map(|o| o.id);
                }
            }
            if let Some(id) = dup {
                if let Some(o) = self.scene.objects.iter().find(|o| o.id == id).cloned() {
                    self.scene.next_id += 1;
                    let mut n = o;
                    n.id = self.scene.next_id;
                    n.name = format!("{} (Kopie)", n.name);
                    self.selected = Some(n.id);
                    self.scene.objects.push(n);
                }
            }
            if self.scene.objects.is_empty() {
                ui.label(RichText::new("Leer – über „Hinzufügen“ oder die Schnelleingabe ein Objekt anlegen.").weak());
            }

            ui.add_space(8.0);
            ui.separator();
            ui.heading("Formel → Objekt");
            ui.label(RichText::new("f(x,y,z) = …   ·   (x(u,v), y(u,v), z(u,v))   ·   (x(t), y(t), z(t))").small().weak());
            let resp = ui.add(egui::TextEdit::singleline(&mut self.quick).hint_text("z. B. x^2 + y^2 - z^2 = 0.3").desired_width(f32::INFINITY));
            let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui.button("Erzeugen").clicked() || enter {
                match quick_kind(&self.quick) {
                    Ok((name, kind)) => {
                        self.quick_err.clear();
                        self.add_kind(&name, kind);
                    }
                    Err(e) => self.quick_err = e,
                }
            }
            if !self.quick_err.is_empty() {
                ui.colored_label(Color32::from_rgb(255, 140, 120), &self.quick_err);
            }

            ui.add_space(8.0);
            ui.separator();
            ui.heading("Parameter");
            ui.label(RichText::new("Symbole in allen Ausdrücken; Formeln bleiben symbolisch in ihnen.").small().weak());
            let mut remove = None;
            for (i, p) in self.scene.params.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&p.name).strong().monospace());
                    ui.add(egui::Slider::new(&mut p.value, p.min..=p.max).max_decimals(4));
                    let play = if p.animate { "⏸" } else { "▶" };
                    if ui.small_button(play).on_hover_text("Animieren (pendelt im Bereich)").clicked() {
                        p.animate = !p.animate;
                    }
                    if ui.small_button("✕").clicked() {
                        remove = Some(i);
                    }
                });
                ui.horizontal(|ui| {
                    ui.add_space(18.0);
                    ui.label(RichText::new("von").small().weak());
                    ui.add(egui::DragValue::new(&mut p.min).speed(0.05));
                    ui.label(RichText::new("bis").small().weak());
                    ui.add(egui::DragValue::new(&mut p.max).speed(0.05));
                    ui.label(RichText::new("Tempo").small().weak());
                    ui.add(egui::DragValue::new(&mut p.speed).speed(0.01).range(0.01..=5.0));
                });
                if p.max < p.min {
                    p.max = p.min;
                }
            }
            if let Some(i) = remove {
                self.scene.params.remove(i);
            }
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.new_param).hint_text("Name").desired_width(80.0));
                if ui.button("+ Parameter").clicked() {
                    let n = self.new_param.trim().to_string();
                    let valid = !n.is_empty()
                        && n.chars().next().unwrap().is_ascii_alphabetic()
                        && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                        && !expr::RESERVED.contains(&n.as_str())
                        && !expr::FUNCS.contains(&n.as_str())
                        && !self.scene.params.iter().any(|p| p.name == n);
                    if valid {
                        self.scene.params.push(Param { name: n, value: 1.0, min: 0.0, max: 2.0, animate: false, speed: 0.3, dir: 1.0 });
                        self.new_param.clear();
                    } else {
                        self.status = "Parametername ungültig, reserviert (x, y, z, u, v, pi, phi, rho …) oder schon vergeben".into();
                    }
                }
            });

            ui.add_space(8.0);
            ui.separator();
            ui.heading("Zahlkörper");
            ui.label(
                RichText::new("Eingebaut: φ (phi), √2 (sqrt2), √3, √5, ρ (rho, plastische Zahl); cos/sin(rationales Vielfaches von π) exakt über γₙ = 2cos(2π/n).")
                    .small()
                    .weak(),
            );
            let mut rm = None;
            for (i, g) in self.scene.gens.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut g.key).desired_width(50.0));
                    ui.label("Nullstelle von");
                    ui.add(egui::TextEdit::singleline(&mut g.poly).desired_width(90.0).font(egui::TextStyle::Monospace));
                    ui.label("≈");
                    ui.add(egui::DragValue::new(&mut g.approx).speed(0.01));
                    if ui.small_button("✕").clicked() {
                        rm = Some(i);
                    }
                });
            }
            if let Some(i) = rm {
                self.scene.gens.remove(i);
            }
            let (defs, errs) = self.scene.custom_gendefs();
            for d in defs {
                ui.label(RichText::new(&d.meaning).small().weak());
            }
            for e in errs {
                ui.colored_label(Color32::from_rgb(255, 140, 120), e);
            }
            if ui.button("+ eigener Generator").on_hover_text("Algebraische Zahl über ihr Minimalpolynom (in x) und eine Näherung der gewünschten reellen Nullstelle. Das Polynom muss irreduzibel über ℚ sein.").clicked() {
                let k = ["alpha", "beta", "delta", "kappa", "lambda", "mu"].iter().find(|k| !self.scene.gens.iter().any(|g| &g.key == *k)).unwrap_or(&"xi");
                self.scene.gens.push(CustomGen { key: k.to_string(), poly: "x^5 - x - 1".into(), approx: 1.17 });
            }

            ui.add_space(8.0);
            ui.separator();
            ui.heading("Schnittebenen");
            ui.label(RichText::new("Ebene n·x = d. Normale und Lage sind Ausdrücke – mit Parametern wird die Schnittfläche bei Polyedern symbolisch.").small().weak());
            let (c, r) = self.scene_extent();
            let env = {
                let (g, _) = self.scene.custom_gendefs();
                self.scene.env(&g)
            };
            let mut rm = None;
            let nsec = self.scene.sections.len();
            for pi in 0..nsec {
                let col = plane_color(pi);
                let sec = &mut self.scene.sections[pi];
                ui.horizontal(|ui| {
                    ui.checkbox(&mut sec.visible, "");
                    let (rr, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), Sense::hover());
                    ui.painter().rect_filled(rr, 2.0, col);
                    ui.add(egui::TextEdit::singleline(&mut sec.name).desired_width(70.0));
                    for (k, n) in ["x", "y", "z"].iter().enumerate() {
                        if ui.small_button(*n).on_hover_text(format!("Normale = {n}-Achse")).clicked() {
                            sec.normal = ["0".into(), "0".into(), "0".into()];
                            sec.normal[k] = "1".into();
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("✕").clicked() {
                            rm = Some(pi);
                        }
                    });
                });
                ui.horizontal(|ui| {
                    ui.add_space(18.0);
                    ui.label("n =");
                    for k in 0..3 {
                        ui.add(egui::TextEdit::singleline(&mut sec.normal[k]).desired_width(38.0).font(egui::TextStyle::Monospace));
                    }
                    ui.label("d =");
                    ui.add(egui::TextEdit::singleline(&mut sec.offset).desired_width(60.0).font(egui::TextStyle::Monospace));
                });
                // Schieberegler, solange d eine Zahl ist
                let nraw: Option<DVec3> = (0..3).map(|k| expr::num(&sec.normal[k], &env).ok()).collect::<Option<Vec<f64>>>().map(|v| DVec3::new(v[0], v[1], v[2]));
                if let (Ok(mut d), Some(n)) = (sec.offset.trim().parse::<f64>(), nraw) {
                    if n.length() > 1e-12 {
                        let mid = n.dot(c);
                        let span = r * n.length();
                        ui.horizontal(|ui| {
                            ui.add_space(18.0);
                            if ui.add(egui::Slider::new(&mut d, (mid - span)..=(mid + span)).text("d")).changed() {
                                let t = format!("{d:.3}");
                                let t = t.trim_end_matches('0').trim_end_matches('.').to_string();
                                sec.offset = if t == "-0" || t.is_empty() { "0".into() } else { t };
                            }
                        });
                    }
                } else {
                    ui.label(RichText::new("   d ist ein Ausdruck – über die Parameter steuern").small().weak());
                }
                ui.horizontal(|ui| {
                    ui.add_space(18.0);
                    ui.checkbox(&mut sec.profile, "Cavalieri-Profil A(h)");
                });
                for res in self.sections_res.iter().filter(|x| x.plane == pi) {
                    let shape = if res.exact { format!("{}-Eck, exakt", res.ngon) } else if res.loops > 1 { format!("{} Kurven", res.loops) } else { "numerisch".into() };
                    ui.label(RichText::new(format!("   {} · {shape}", res.obj)).small().color(Color32::from_gray(160)));
                    ui.horizontal_wrapped(|ui| {
                        ui.add_space(18.0);
                        let rel = |t: &str| if t.starts_with('≈') { t.to_string() } else { format!("= {t}") };
                        if res.closed {
                            ui.add(egui::Label::new(RichText::new(format!("A {}", rel(&res.area_uni))).size(14.0)).selectable(true));
                            ui.add(egui::Label::new(RichText::new(format!("U {}", rel(&res.perim_uni))).size(14.0)).selectable(true));
                        } else {
                            ui.add(egui::Label::new(RichText::new(format!("Kurvenlänge {}", rel(&res.perim_uni))).size(14.0)).selectable(true));
                        }
                    });
                    if res.exact {
                        ui.label(RichText::new(format!("   ≈ {:.8}", res.area)).small().monospace().color(Color32::from_gray(130)));
                    }
                    if !res.note.is_empty() {
                        ui.label(RichText::new(format!("   {}", res.note)).small().color(Color32::from_rgb(240, 200, 110)));
                    }
                }
                for pr in self.profiles.iter().filter(|x| x.plane == pi) {
                    ui.horizontal(|ui| {
                        ui.add_space(18.0);
                        sparkline(ui, &pr.samples, col);
                        ui.label(RichText::new(format!("{}\n∫A dh ≈ {:.5}\nV ≈ {:.5}", pr.obj, pr.integral, pr.volume)).small());
                    });
                }
                ui.add_space(4.0);
            }
            if let Some(i) = rm {
                self.scene.sections.remove(i);
            }
            if ui.button("+ Schnittebene").clicked() {
                let n = self.scene.sections.len() + 1;
                let mut sec = scene::Section::new(&format!("E{n}"), 2, "0");
                let t = format!("{:.3}", c.z);
                sec.offset = t.trim_end_matches('0').trim_end_matches('.').to_string();
                if sec.offset.is_empty() || sec.offset == "-0" {
                    sec.offset = "0".into();
                }
                self.scene.sections.push(sec);
            }

            ui.add_space(8.0);
            ui.separator();
            ui.heading("Interaktionen");
            ui.checkbox(&mut self.inter_on, "Verstrebungen der Objekte untereinander auswerten");
            if self.inter_on {
                ui.label(RichText::new("◉ gemeinsamer Knoten  ● Kreuzung/Stoß  • Durchstoßpunkt  ▬ Abschnitt im anderen Körper").small().weak());
                if self.inters.is_empty() {
                    ui.label(RichText::new("Mindestens zwei sichtbare Objekte mit Streben nötig.").small().weak());
                }
                for r in &self.inters {
                    let title = format!("{} ↔ {}{}", r.a, r.b, if r.exact { "  (exakt)" } else { "" });
                    egui::CollapsingHeader::new(title).id_salt(format!("ia{}{}", r.a, r.b)).default_open(!r.is_empty()).show(ui, |ui| {
                        inter_ui(ui, r);
                    });
                }
                ui.label(RichText::new(format!("Auswertung {:.1} ms", self.analysis_ms)).small().weak());
            }
        });
    }

    fn right_panel(&mut self, ui: &mut egui::Ui) {
        let Some(id) = self.selected else {
            ui.label("Kein Objekt gewählt.");
            return;
        };
        let built = self.built.get(&id).map(|b| b.1.clone());
        let tool = &mut self.tool;
        let strut_first = &mut self.strut_first;
        let Some(o) = self.scene.objects.iter_mut().find(|o| o.id == id) else {
            return;
        };
        let mut copy: Option<String> = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.color_edit_button_srgb(&mut o.color);
                    ui.add(
                        egui::TextEdit::singleline(&mut o.name)
                            .font(egui::TextStyle::Heading)
                            .desired_width(f32::INFINITY),
                    );
                });
                ui.label(RichText::new(o.kind.type_name()).weak());
                if let Some(b) = &built {
                    for e in &b.errors {
                        ui.colored_label(Color32::from_rgb(255, 130, 110), format!("⚠ {e}"));
                    }
                    for w in &b.warnings {
                        ui.colored_label(Color32::from_rgb(240, 200, 110), format!("• {w}"));
                    }
                }
                egui::CollapsingHeader::new("Definition")
                    .default_open(true)
                    .show(ui, |ui| {
                        definition_ui(ui, o, tool, strut_first);
                        ui.horizontal(|ui| {
                            ui.label("Verschiebung");
                            for k in 0..3 {
                                ui.add(
                                    egui::TextEdit::singleline(&mut o.offset[k])
                                        .desired_width(46.0),
                                );
                            }
                        });
                    });
                egui::CollapsingHeader::new("Verstrebungen & Ebenen")
                    .default_open(true)
                    .show(ui, |ui| {
                        layers_ui(ui, o);
                    });
                if let Some(b) = &built {
                    egui::CollapsingHeader::new("Formeln")
                        .default_open(true)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                if ui.button("LaTeX kopieren").clicked() {
                                    copy = Some(export::latex_object(o, b));
                                }
                                ui.label(
                                    RichText::new("exakt in K, symbolisch in den Parametern")
                                        .small()
                                        .weak(),
                                );
                            });
                            ui.add_space(4.0);
                            for l in &b.formulas {
                                ui.horizontal_wrapped(|ui| {
                                    if let Some(ci) = l.class {
                                        let col = b
                                            .class_colors
                                            .get(ci)
                                            .cloned()
                                            .unwrap_or(Color32::WHITE);
                                        let (r, _) = ui.allocate_exact_size(
                                            egui::vec2(12.0, 12.0),
                                            Sense::hover(),
                                        );
                                        ui.painter().rect_filled(r, 2.0, col);
                                        let mut vis = !o.hidden_classes.contains(&ci);
                                        if ui
                                            .checkbox(&mut vis, "")
                                            .on_hover_text("Klasse ein-/ausblenden")
                                            .changed()
                                        {
                                            if vis {
                                                o.hidden_classes.retain(|x| *x != ci);
                                            } else {
                                                o.hidden_classes.push(ci);
                                            }
                                        }
                                    }
                                    ui.label(
                                        RichText::new(&l.label)
                                            .small()
                                            .color(Color32::from_gray(150)),
                                    );
                                });
                                ui.horizontal_wrapped(|ui| {
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(&l.uni)
                                                .size(15.5)
                                                .color(Color32::from_gray(235)),
                                        )
                                        .selectable(true),
                                    );
                                    if !l.tex.is_empty()
                                        && ui.small_button("TeX").on_hover_text(&l.tex).clicked()
                                    {
                                        copy = Some(
                                            l.tex
                                                .replace("&=", "=")
                                                .replace("&\\approx", "\\approx"),
                                        );
                                    }
                                });
                                if let Some(n) = l.num {
                                    ui.label(
                                        RichText::new(format!("≈ {n:.10}{}", l.unit))
                                            .small()
                                            .monospace()
                                            .color(Color32::from_gray(130)),
                                    );
                                }
                                ui.add_space(3.0);
                            }
                            if !b.legend.is_empty() {
                                ui.separator();
                                ui.label(RichText::new("Zahlkörper").small().weak());
                                for g in &b.legend {
                                    ui.label(RichText::new(g).small());
                                }
                            }
                        });
                }
            });
        if let Some(s) = copy {
            ui.ctx().copy_text(s);
            self.status = "In die Zwischenablage kopiert".into();
        }
    }

    fn viewport(&mut self, ui: &mut egui::Ui) {
        let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let rect = resp.rect;
        painter.rect_filled(rect, 0.0, Color32::from_rgb(14, 16, 21));
        // Steuerung
        let shift = ui.input(|i| i.modifiers.shift);
        if resp.dragged_by(egui::PointerButton::Primary) && !shift {
            let d = resp.drag_delta();
            self.cam.yaw -= d.x as f64 * 0.008;
            self.cam.pitch = (self.cam.pitch + d.y as f64 * 0.008).clamp(-1.55, 1.55);
        }
        if resp.dragged_by(egui::PointerButton::Secondary)
            || resp.dragged_by(egui::PointerButton::Middle)
            || (resp.dragged_by(egui::PointerButton::Primary) && shift)
        {
            let d = resp.drag_delta();
            let b = self.cam.basis();
            let k = self.cam.dist / self.cam.focal(rect);
            self.cam.target += (-b.right * d.x as f64 + b.up * d.y as f64) * k;
        }
        if resp.hovered() {
            let s = ui.input(|i| i.smooth_scroll_delta.y);
            if s != 0.0 {
                self.cam.dist *= (-s as f64 * 0.0018).exp();
                self.cam.dist = self.cam.dist.clamp(1e-3, 1e5);
            }
        }
        if resp.double_clicked() {
            self.fit();
        }
        // Zeichnen
        let objs: Vec<(u64, &Built)> = self
            .scene
            .objects
            .iter()
            .filter(|o| o.visible)
            .filter_map(|o| self.built.get(&o.id).map(|b| (o.id, &b.1)))
            .collect();
        let (_, r) = self.scene_extent();
        let sections: Vec<(DVec3, f64, Color32)> = self
            .scene
            .sections
            .iter()
            .enumerate()
            .filter(|(_, e)| e.visible)
            .filter_map(|(i, e)| {
                analysis::plane_num(e, &self.scene)
                    .ok()
                    .map(|(n, d)| (n, d, plane_color(i)))
            })
            .collect();
        let (center, _) = self.scene_extent();
        let (marks, highlights) = if self.inter_on {
            (
                self.inters
                    .iter()
                    .flat_map(|x| x.marks.iter().cloned())
                    .collect(),
                self.inters
                    .iter()
                    .flat_map(|x| x.inside.iter().cloned())
                    .collect(),
            )
        } else {
            (vec![], vec![])
        };
        let opts = ViewOpts {
            grid: self.grid,
            axes: self.axes,
            sections,
            scene_radius: r,
            scene_center: center,
            marks,
            highlights,
        };
        let mut picks: Vec<Pick> = vec![];
        let items = render::collect(&self.cam, rect, &objs, &opts, &mut picks);
        render::paint(&painter.with_clip_rect(rect), &items);
        self.last_items = items;
        // Werkzeuge
        if self.tool != Tool::Orbit {
            let txt = match self.tool {
                Tool::AddPoint => {
                    "Klick setzt Punkt auf die Ebene z = 0 (Raster 0,05) · Esc beendet"
                }
                _ => {
                    if self.strut_first.is_some() {
                        "zweiten Punkt anklicken · Esc beendet"
                    } else {
                        "ersten Punkt der Strebe anklicken · Esc beendet"
                    }
                }
            };
            painter.text(
                rect.left_top() + egui::vec2(12.0, 12.0),
                egui::Align2::LEFT_TOP,
                txt,
                egui::FontId::proportional(14.0),
                Color32::from_rgb(255, 220, 120),
            );
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.tool = Tool::Orbit;
                self.strut_first = None;
            }
        }
        if resp.clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                self.click(pos, rect, &picks);
            }
        }
        // Formeln als Overlay
        if let Some(id) = self.selected {
            if let Some((_, b)) = self.built.get(&id) {
                if let Some(s) = &b.sym {
                    if s.order > 0 {
                        painter.text(
                            rect.right_bottom() - egui::vec2(12.0, 10.0),
                            egui::Align2::RIGHT_BOTTOM,
                            format!("Symmetrie {}  ·  |G| = {}", s.name_uni, s.order),
                            egui::FontId::proportional(14.0),
                            Color32::from_gray(170),
                        );
                    }
                }
            }
        }
    }

    fn click(&mut self, pos: egui::Pos2, rect: egui::Rect, picks: &[Pick]) {
        let Some(id) = self.selected else { return };
        match self.tool {
            Tool::AddPoint => {
                let b = self.cam.basis();
                let (o, d) = self.cam.ray(&b, rect, pos);
                if d.z.abs() < 1e-9 {
                    return;
                }
                let t = -o.z / d.z;
                if t <= 0.0 {
                    return;
                }
                let p = o + d * t;
                let fmt = |x: f64| {
                    let q = (x * 20.0).round() / 20.0;
                    let s = format!("{q:.2}");
                    let s = s.trim_end_matches('0').trim_end_matches('.').to_string();
                    if s == "-0" {
                        "0".to_string()
                    } else {
                        s
                    }
                };
                if let Some(o) = self.selected_obj() {
                    if let Kind::Free { pts, .. } = &mut o.kind {
                        pts.push([fmt(p.x), fmt(p.y), "0".into()]);
                    }
                }
            }
            Tool::AddStrut => {
                let hit = picks.iter().filter(|p| p.obj == id).min_by(|a, b| {
                    a.pos
                        .distance(pos)
                        .partial_cmp(&b.pos.distance(pos))
                        .unwrap()
                });
                if let Some(h) = hit {
                    if h.pos.distance(pos) < 14.0 {
                        match self.strut_first.take() {
                            None => self.strut_first = Some(h.idx),
                            Some(a) => {
                                if a != h.idx {
                                    if let Some(o) = self.selected_obj() {
                                        if let Kind::Free { struts, .. } = &mut o.kind {
                                            let e = (a.min(h.idx), a.max(h.idx));
                                            if !struts.contains(&e) {
                                                struts.push(e);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Tool::Orbit => {
                // Objekt per Klick auf einen Eckpunkt wählen
                if let Some(h) = picks.iter().min_by(|a, b| {
                    a.pos
                        .distance(pos)
                        .partial_cmp(&b.pos.distance(pos))
                        .unwrap()
                }) {
                    if h.pos.distance(pos) < 10.0 {
                        self.selected = Some(h.obj);
                    }
                }
            }
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let dt = ctx.input(|i| i.stable_dt).min(0.1) as f64;
        if self.animate(dt) {
            ctx.request_repaint();
        }
        self.rebuild();
        self.reanalyze();
        if self.fit_pending {
            self.fit();
            self.fit_pending = false;
        }
        egui::TopBottomPanel::top("menu").show(ctx, |ui| self.menu(ui));
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&self.status).small());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new("Strebwerk 0.1 · Rust + egui/wgpu")
                            .small()
                            .weak(),
                    );
                });
            });
        });
        egui::SidePanel::left("left")
            .resizable(true)
            .default_width(300.0)
            .show(ctx, |ui| self.left_panel(ui));
        egui::SidePanel::right("right")
            .resizable(true)
            .default_width(400.0)
            .show(ctx, |ui| self.right_panel(ui));
        egui::CentralPanel::default()
            .frame(egui::Frame::default())
            .show(ctx, |ui| self.viewport(ui));
        let mut open = self.show_help;
        egui::Window::new("Hilfe")
            .open(&mut open)
            .default_width(520.0)
            .show(ctx, |ui| {
                ui.label(RichText::new(HELP).size(13.5));
            });
        self.show_help = open;
    }
}

fn plane_color(i: usize) -> Color32 {
    [
        Color32::from_rgb(255, 235, 80),
        Color32::from_rgb(255, 150, 70),
        Color32::from_rgb(160, 240, 110),
        Color32::from_rgb(120, 200, 255),
        Color32::from_rgb(240, 130, 240),
    ][i % 5]
}

fn sparkline(ui: &mut egui::Ui, pts: &[(f64, f64)], col: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(120.0, 42.0), Sense::hover());
    let p = ui.painter();
    p.rect_stroke(
        rect,
        2.0,
        egui::Stroke::new(1.0_f32, Color32::from_gray(60)),
        egui::StrokeKind::Inside,
    );
    if pts.len() < 2 {
        return;
    }
    let (h0, h1) = (pts[0].0, pts[pts.len() - 1].0);
    let amax = pts.iter().map(|x| x.1).fold(1e-12, f64::max);
    let line: Vec<egui::Pos2> = pts
        .iter()
        .map(|(h, a)| {
            egui::pos2(
                rect.left() + 3.0 + (rect.width() - 6.0) * ((h - h0) / (h1 - h0).max(1e-12)) as f32,
                rect.bottom() - 3.0 - (rect.height() - 6.0) * (a / amax) as f32,
            )
        })
        .collect();
    p.add(egui::Shape::line(line, egui::Stroke::new(1.5_f32, col)));
}

fn inter_ui(ui: &mut egui::Ui, r: &InterRes) {
    let row = |ui: &mut egui::Ui, head: &str, body: String| {
        ui.label(RichText::new(head).small().color(Color32::from_gray(150)));
        ui.add(egui::Label::new(RichText::new(body).size(13.5)).selectable(true));
    };
    if r.is_empty() {
        let t = match &r.near {
            Some((d, a, b)) => format!("keine Berührung; kleinster Abstand ≈ {d:.6} ({a} / {b})"),
            None => "keine Berührung".into(),
        };
        ui.label(RichText::new(t).small());
    }
    if !r.shared.is_empty() {
        let pts: Vec<&str> = r.shared.iter().take(6).map(|p| p.uni.as_str()).collect();
        row(
            ui,
            &format!("Gemeinsame Knoten ({})", r.shared.len()),
            pts.join(", "),
        );
    }
    for g in analysis::crossing_groups(&r.crossings) {
        let pts: Vec<&str> = g.6.iter().take(3).map(|p| p.uni.as_str()).collect();
        row(
            ui,
            &format!("{}: {} × {} ({}×)", g.2, g.0, g.1, g.6.len()),
            format!(
                "|cos∠| {}  (∠ ≈ {:.4}°)\nbei {}{}",
                if g.3.starts_with('≈') {
                    g.3.clone()
                } else {
                    format!("= {}", g.3)
                },
                g.5,
                pts.join(", "),
                if g.6.len() > 3 { ", …" } else { "" }
            ),
        );
    }
    for p in &r.pierces {
        let each = p
            .each_uni
            .as_ref()
            .map(|e| format!(", je {e}"))
            .unwrap_or_default();
        row(
            ui,
            &format!("{} von {} in {} – {}", p.label, p.owner, p.body, p.kind),
            format!("{}×{each}, gesamt {}", p.count, p.total_uni),
        );
    }
    for (a, b, l) in &r.overlaps {
        row(
            ui,
            &format!("Überlappung {a} / {b}"),
            format!("gemeinsame Länge ≈ {l:.6}"),
        );
    }
    if let (Some((d, a, b)), false) = (&r.near, r.is_empty()) {
        ui.label(
            RichText::new(format!("kleinster Abstand sonst ≈ {d:.6} ({a} / {b})"))
                .small()
                .weak(),
        );
    }
    if r.near_misses > 0 {
        ui.label(
            RichText::new(format!(
                "{} numerische Beinahe-Treffer exakt widerlegt",
                r.near_misses
            ))
            .small()
            .color(Color32::from_rgb(240, 200, 110)),
        );
    }
    for n in &r.notes {
        ui.label(
            RichText::new(n)
                .small()
                .color(Color32::from_rgb(240, 200, 110)),
        );
    }
}

const HELP: &str = "Steuerung
  Linke Maustaste ziehen: drehen   ·   rechte/mittlere (oder Shift+links): verschieben
  Mausrad: zoomen   ·   Doppelklick: Szene einpassen

Ausdrücke
  + − · / ^, implizite Multiplikation (2a, 3 sin u), Klammern
  Funktionen: sin cos tan asin acos atan sinh cosh tanh exp ln sqrt cbrt abs min max atan2
  Konstanten: pi, e – nur numerisch
  Exakt: rationale Zahlen (0.25 = 1/4), Parameter, phi (φ), sqrt2, sqrt3, sqrt5 = 2φ − 1,
         rho (ρ, x³ = x + 1), eigene Generatoren, sqrt(8) = 2√2,
         cos(2pi/7), sin(pi/5) usw. – über γₙ = 2cos(2π/n) und σₙ = 2sin(2π/n)

Exaktheit
  Polyeder-Koordinaten liegen in einem Zahlkörperturm K. Abstände, Flächen, Volumen,
  Kugelradien und Diederwinkel werden in K[Parameter] gerechnet; gleiche Längen werden exakt
  (nicht numerisch) zu Klassen zusammengefasst. Die Kombinatorik (welche Punkte eine Fläche
  bilden) kommt aus der numerischen Hülle beim aktuellen Parameterwert – die Formeln gelten
  für den ganzen Bereich mit gleichem kombinatorischem Typ.

Verstrebungen
  Polyeder: Flächen- und Raumdiagonalen (exakt klassifiziert), Dual, Speichen,
  Symmetrieachsen (nach Zähligkeit gefärbt), Spiegelebenen, Um-/Inkugel,
  Kegel-Tetraedrisierung, Gitter-Füllung (kubisch, krz, Oktett), TPMS-Füllung (Gyroid …)
  Flächen: Parameterlinien, Sehnen durch das Innere, Normalen, Höhenlinien, Füllungen
  Kurven: begleitendes Dreibein, Schmiegkreise, Sehnen

Freie Konstruktion
  Punkte im Inspektor oder per Werkzeug „Punkt setzen“ anlegen, Streben per Klick ziehen;
  „konvexe Hülle“ macht daraus einen Körper. Ohne Hülle gibt es eine Maxwell-Zählung.

Schnittebenen
  Beliebig viele Ebenen n·x = d; n und d sind Ausdrücke. Bei Polyedern wird das
  Schnittpolygon exakt berechnet (Ecken, Umfang, Fläche) – mit einem Parameter in d
  (z. B. d = s) bleibt A(s) symbolisch. Cavalieri-Profil: A(h) über die ganze
  Ausdehnung, ∫A dh wird mit dem Volumen verglichen.

Interaktionen
  Für jedes Paar sichtbarer Objekte: gemeinsame Knoten, Kreuzungen und Stöße von Streben
  (exakter Schnittpunkt und Winkel), Durchstoßungen mit Innenlänge im anderen Körper,
  Überlappungen und kleinster Abstand. Bei exakten Objekten zählt ein Treffer nur, wenn er
  exakt nachgewiesen ist; numerische Beinahe-Treffer werden als widerlegt gemeldet.

Export
  Datei → LaTeX (Formeln, Schnittebenen, Profile, Interaktionen), TikZ (aktuelle Ansicht),
  STL (Gerüst zum 3D-Druck). Ohne Fenster: strebwerk --tex szene.strebwerk [ausgabe.tex]";

// ---------------------------------------------------------------------------
// Inspektor-Teile
// ---------------------------------------------------------------------------

fn expr_field(ui: &mut egui::Ui, label: &str, s: &mut String) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add(
            egui::TextEdit::singleline(s)
                .font(egui::TextStyle::Monospace)
                .desired_width(f32::INFINITY),
        );
    });
}

fn definition_ui(ui: &mut egui::Ui, o: &mut Obj, tool: &mut Tool, strut_first: &mut Option<usize>) {
    match &mut o.kind {
        Kind::Polyhedron { base, ops } => {
            let cur = match base {
                PolyBase::Platonic { .. } => 0,
                PolyBase::Cuboid { .. } => 1,
                PolyBase::Prism { .. } => 2,
                PolyBase::Antiprism { .. } => 3,
                PolyBase::Pyramid { .. } => 4,
                PolyBase::Bipyramid { .. } => 5,
            };
            let names = [
                "Platonischer Körper",
                "Quader",
                "Prisma",
                "Antiprisma",
                "Pyramide",
                "Bipyramide",
            ];
            let mut sel = cur;
            egui::ComboBox::from_id_salt("base")
                .selected_text(names[cur])
                .show_ui(ui, |ui| {
                    for (i, n) in names.iter().enumerate() {
                        ui.selectable_value(&mut sel, i, *n);
                    }
                });
            if sel != cur {
                *base = match sel {
                    0 => PolyBase::Platonic {
                        kind: Platonic::Cube,
                        a: "a".into(),
                    },
                    1 => PolyBase::Cuboid {
                        a: "a".into(),
                        b: "a".into(),
                        c: "h".into(),
                    },
                    2 => PolyBase::Prism {
                        n: 6,
                        r: "1".into(),
                        h: "h".into(),
                    },
                    3 => PolyBase::Antiprism {
                        n: 6,
                        r: "1".into(),
                        h: "h".into(),
                    },
                    4 => PolyBase::Pyramid {
                        n: 4,
                        r: "1".into(),
                        h: "h".into(),
                    },
                    _ => PolyBase::Bipyramid {
                        n: 5,
                        r: "1".into(),
                        h: "h".into(),
                    },
                };
            }
            match base {
                PolyBase::Platonic { kind, a } => {
                    egui::ComboBox::from_id_salt("plat")
                        .selected_text(kind.name())
                        .show_ui(ui, |ui| {
                            for k in [
                                Platonic::Tetra,
                                Platonic::Cube,
                                Platonic::Octa,
                                Platonic::Dodeca,
                                Platonic::Icosa,
                            ] {
                                ui.selectable_value(kind, k, k.name());
                            }
                        });
                    expr_field(ui, "Kantenlänge", a);
                }
                PolyBase::Cuboid { a, b, c } => {
                    expr_field(ui, "a", a);
                    expr_field(ui, "b", b);
                    expr_field(ui, "c", c);
                }
                PolyBase::Prism { n, r, h }
                | PolyBase::Antiprism { n, r, h }
                | PolyBase::Pyramid { n, r, h }
                | PolyBase::Bipyramid { n, r, h } => {
                    ui.horizontal(|ui| {
                        ui.label("n");
                        ui.add(egui::DragValue::new(n).range(3..=60));
                    });
                    expr_field(ui, "Umkreisradius", r);
                    expr_field(ui, "Höhe", h);
                }
            }
            ui.label(RichText::new("Operationen (der Reihe nach)").small().weak());
            let mut rm = None;
            for (i, op) in ops.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    match op {
                        PolyOp::Truncate(t) => {
                            ui.label("Abstumpfen t =");
                            ui.add(
                                egui::TextEdit::singleline(t)
                                    .font(egui::TextStyle::Monospace)
                                    .desired_width(140.0),
                            );
                        }
                        PolyOp::Dual => {
                            ui.label("Dual (Flächenmitten)");
                        }
                        PolyOp::Scale(s) => {
                            ui.label("Skalieren ×");
                            ui.add(
                                egui::TextEdit::singleline(s)
                                    .font(egui::TextStyle::Monospace)
                                    .desired_width(140.0),
                            );
                        }
                    }
                    if ui.small_button("✕").clicked() {
                        rm = Some(i);
                    }
                });
            }
            if let Some(i) = rm {
                ops.remove(i);
            }
            ui.horizontal(|ui| {
                if ui.small_button("+ Abstumpfen").clicked() {
                    ops.push(PolyOp::Truncate("t".into()));
                }
                if ui.small_button("+ Dual").clicked() {
                    ops.push(PolyOp::Dual);
                }
                if ui.small_button("+ Skalieren").clicked() {
                    ops.push(PolyOp::Scale("1".into()));
                }
            });
        }
        Kind::Free { pts, struts, hull } => {
            ui.checkbox(hull, "konvexe Hülle bilden (→ Körper mit allen Formeln)");
            let mut rm = None;
            egui::Grid::new("pts")
                .num_columns(5)
                .spacing([4.0, 2.0])
                .show(ui, |ui| {
                    for (i, p) in pts.iter_mut().enumerate() {
                        ui.label(RichText::new(format!("P{i}")).monospace());
                        for c in p.iter_mut() {
                            ui.add(
                                egui::TextEdit::singleline(c)
                                    .font(egui::TextStyle::Monospace)
                                    .desired_width(62.0),
                            );
                        }
                        if ui.small_button("✕").clicked() {
                            rm = Some(i);
                        }
                        ui.end_row();
                    }
                });
            if let Some(i) = rm {
                pts.remove(i);
                struts.retain(|(a, b)| *a != i && *b != i);
                for s in struts.iter_mut() {
                    if s.0 > i {
                        s.0 -= 1;
                    }
                    if s.1 > i {
                        s.1 -= 1;
                    }
                }
            }
            ui.horizontal(|ui| {
                if ui.button("+ Punkt").clicked() {
                    pts.push(["0".into(), "0".into(), "0".into()]);
                }
                if ui
                    .selectable_label(*tool == Tool::AddPoint, "Punkt setzen (Klick)")
                    .clicked()
                {
                    *tool = if *tool == Tool::AddPoint {
                        Tool::Orbit
                    } else {
                        Tool::AddPoint
                    };
                }
                if ui
                    .selectable_label(*tool == Tool::AddStrut, "Strebe ziehen (Klick)")
                    .clicked()
                {
                    *tool = if *tool == Tool::AddStrut {
                        Tool::Orbit
                    } else {
                        Tool::AddStrut
                    };
                    *strut_first = None;
                }
            });
            if !struts.is_empty() {
                ui.label(RichText::new("Streben").small().weak());
                let mut rm = None;
                ui.horizontal_wrapped(|ui| {
                    for (i, (a, b)) in struts.iter().enumerate() {
                        if ui.small_button(format!("P{a}–P{b} ✕")).clicked() {
                            rm = Some(i);
                        }
                    }
                });
                if let Some(i) = rm {
                    struts.remove(i);
                }
            }
        }
        Kind::ParamSurface {
            x,
            y,
            z,
            u0,
            u1,
            v0,
            v1,
            nu,
            nv,
        } => {
            expr_field(ui, "x(u,v)", x);
            expr_field(ui, "y(u,v)", y);
            expr_field(ui, "z(u,v)", z);
            ui.horizontal(|ui| {
                ui.label("u ∈");
                ui.add(egui::TextEdit::singleline(u0).desired_width(56.0));
                ui.add(egui::TextEdit::singleline(u1).desired_width(56.0));
                ui.label("v ∈");
                ui.add(egui::TextEdit::singleline(v0).desired_width(56.0));
                ui.add(egui::TextEdit::singleline(v1).desired_width(56.0));
            });
            ui.horizontal(|ui| {
                ui.label("Auflösung");
                ui.add(egui::DragValue::new(nu).range(4..=400));
                ui.add(egui::DragValue::new(nv).range(4..=400));
            });
        }
        Kind::Implicit { f, lo, hi, res } => {
            expr_field(ui, "F(x,y,z) = 0:", f);
            for (k, n) in ["x", "y", "z"].iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("{n} ∈"));
                    ui.add(egui::TextEdit::singleline(&mut lo[k]).desired_width(70.0));
                    ui.add(egui::TextEdit::singleline(&mut hi[k]).desired_width(70.0));
                });
            }
            ui.horizontal(|ui| {
                ui.label("Auflösung");
                ui.add(egui::DragValue::new(res).range(8..=160));
            });
        }
        Kind::Curve { x, y, z, t0, t1, n } => {
            expr_field(ui, "x(t)", x);
            expr_field(ui, "y(t)", y);
            expr_field(ui, "z(t)", z);
            ui.horizontal(|ui| {
                ui.label("t ∈");
                ui.add(egui::TextEdit::singleline(t0).desired_width(70.0));
                ui.add(egui::TextEdit::singleline(t1).desired_width(70.0));
                ui.label("Stützstellen");
                ui.add(egui::DragValue::new(n).range(8..=20000));
            });
        }
    }
}

fn layers_ui(ui: &mut egui::Ui, o: &mut Obj) {
    let l = &mut o.layers;
    let is_poly = matches!(
        o.kind,
        Kind::Polyhedron { .. } | Kind::Free { hull: true, .. }
    );
    let is_free = matches!(o.kind, Kind::Free { hull: false, .. });
    let is_surf = matches!(o.kind, Kind::ParamSurface { .. });
    let is_imp = matches!(o.kind, Kind::Implicit { .. });
    let is_curve = matches!(o.kind, Kind::Curve { .. });
    if !is_curve {
        ui.horizontal(|ui| {
            ui.checkbox(&mut l.faces, "Flächen");
            ui.add(egui::Slider::new(&mut l.alpha, 0.0..=1.0).text("Deckkraft"));
        });
    }
    if is_poly {
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut l.edges, "Kanten");
            ui.checkbox(&mut l.vertices, "Ecken");
            ui.checkbox(&mut l.labels, "Nummern");
        });
        ui.label(RichText::new("durch den Körper").small().weak());
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut l.face_diag, "Flächendiagonalen");
            ui.checkbox(&mut l.space_diag, "Raumdiagonalen");
            ui.checkbox(&mut l.dual, "Dual");
            ui.checkbox(&mut l.spokes, "Speichen");
            ui.checkbox(&mut l.cone_tet, "Tetraedrisierung");
        });
        ui.label(RichText::new("Symmetrie & Kugeln").small().weak());
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut l.axes, "Drehachsen");
            ui.checkbox(&mut l.mirrors, "Spiegelebenen");
            ui.checkbox(&mut l.spheres, "Um-/Inkugel");
        });
    }
    if is_free {
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut l.all_pairs, "alle Punktpaare (vollständiger Graph)");
            ui.checkbox(&mut l.axes, "Drehachsen");
            ui.checkbox(&mut l.mirrors, "Spiegelebenen");
        });
    }
    if is_poly || is_imp {
        ui.label(RichText::new("Füllung").small().weak());
        ui.horizontal(|ui| {
            ui.checkbox(&mut l.lattice, "Gitter");
            egui::ComboBox::from_id_salt("lat")
                .selected_text(l.lattice_kind.name())
                .show_ui(ui, |ui| {
                    for k in [LatticeKind::Cubic, LatticeKind::Bcc, LatticeKind::Octet] {
                        ui.selectable_value(&mut l.lattice_kind, k, k.name());
                    }
                });
            ui.add(
                egui::DragValue::new(&mut l.lattice_spacing)
                    .speed(0.01)
                    .range(0.02..=10.0)
                    .prefix("Abstand "),
            );
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut l.tpms, "TPMS");
            egui::ComboBox::from_id_salt("tpms")
                .selected_text(l.tpms_kind.name())
                .show_ui(ui, |ui| {
                    for k in [Tpms::Gyroid, Tpms::SchwarzP, Tpms::Diamond] {
                        ui.selectable_value(&mut l.tpms_kind, k, k.name());
                    }
                });
            ui.add(
                egui::DragValue::new(&mut l.tpms_period)
                    .speed(0.01)
                    .range(0.05..=10.0)
                    .prefix("Periode "),
            );
        });
    }
    if is_surf {
        ui.horizontal(|ui| {
            ui.checkbox(&mut l.iso, "Parameterlinien");
            ui.add(
                egui::DragValue::new(&mut l.iso_u)
                    .range(1..=200)
                    .prefix("u: "),
            );
            ui.add(
                egui::DragValue::new(&mut l.iso_v)
                    .range(1..=200)
                    .prefix("v: "),
            );
        });
    }
    if is_surf || is_curve {
        ui.horizontal(|ui| {
            ui.checkbox(&mut l.chords, "Sehnen");
            ui.add(
                egui::DragValue::new(&mut l.chord_du)
                    .speed(0.005)
                    .range(-1.0..=1.0)
                    .prefix("Δu "),
            );
            if is_surf {
                ui.add(
                    egui::DragValue::new(&mut l.chord_dv)
                        .speed(0.005)
                        .range(-1.0..=1.0)
                        .prefix("Δv "),
                );
            }
            ui.add(
                egui::DragValue::new(&mut l.chord_n)
                    .range(1..=400)
                    .prefix("Anzahl "),
            );
        });
    }
    if is_surf || is_imp {
        ui.horizontal(|ui| {
            ui.checkbox(&mut l.normals, "Normalen");
            ui.add(
                egui::DragValue::new(&mut l.normal_len)
                    .speed(0.005)
                    .range(0.01..=5.0)
                    .prefix("Länge "),
            );
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut l.contours, "Höhenlinien");
            ui.add(egui::DragValue::new(&mut l.contour_n).range(1..=80));
        });
    }
    if is_curve {
        ui.horizontal(|ui| {
            ui.checkbox(&mut l.frenet, "Begleitendes Dreibein (T, N, B)");
            ui.add(egui::DragValue::new(&mut l.frenet_n).range(1..=200));
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut l.osc, "Schmiegkreise");
            ui.add(egui::DragValue::new(&mut l.osc_n).range(1..=60));
        });
    }
}

// ---------------------------------------------------------------------------
// Schnelleingabe
// ---------------------------------------------------------------------------

/// Kommt `name` als eigenständiges Wort in `hay` vor?
fn mentions(hay: &str, name: &str) -> bool {
    let b = hay.as_bytes();
    let isw = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let mut start = 0;
    while let Some(i) = hay[start..].find(name) {
        let s = start + i;
        let e = s + name.len();
        if (s == 0 || !isw(b[s - 1])) && (e >= b.len() || !isw(b[e])) {
            return true;
        }
        start = s + 1;
    }
    false
}

fn split_top(s: &str) -> Vec<String> {
    let mut out = vec![];
    let mut depth = 0i32;
    let mut cur = String::new();
    for c in s.chars() {
        match c {
            '(' | '[' => {
                depth += 1;
                cur.push(c)
            }
            ')' | ']' => {
                depth -= 1;
                cur.push(c)
            }
            ',' | ';' if depth == 0 => {
                out.push(cur.trim().to_string());
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    out.push(cur.trim().to_string());
    out
}

fn is_fn_head(s: &str) -> bool {
    let s = s.trim();
    let Some(open) = s.find('(') else {
        return false;
    };
    if !s.ends_with(')') {
        return false;
    }
    let name = &s[..open];
    let args = &s[open + 1..s.len() - 1];
    !name.is_empty()
        && name.chars().all(|c| c.is_alphanumeric() || c == '_')
        && args
            .split(',')
            .all(|a| a.trim().chars().all(|c| c.is_alphabetic()))
}

pub fn quick_kind(input: &str) -> Result<(String, Kind), String> {
    let mut s = input.trim().to_string();
    if s.is_empty() {
        return Err("Eingabe leer".into());
    }
    if let Some(eq) = s.find('=') {
        if is_fn_head(&s[..eq]) {
            s = s[eq + 1..].trim().to_string();
        }
    }
    let tuple = s.starts_with('(') && s.ends_with(')') && {
        let parts = split_top(&s[1..s.len() - 1]);
        parts.len() == 3
    };
    if tuple {
        let parts = split_top(&s[1..s.len() - 1]);
        let mut vars = BTreeSet::new();
        for p in &parts {
            expr::vars_of(&expr::parse(p)?, &mut vars);
        }
        if vars.contains("u") || vars.contains("v") {
            return Ok((
                "Parameterfläche".into(),
                Kind::ParamSurface {
                    x: parts[0].clone(),
                    y: parts[1].clone(),
                    z: parts[2].clone(),
                    u0: "0".into(),
                    u1: "2pi".into(),
                    v0: "0".into(),
                    v1: "2pi".into(),
                    nu: 96,
                    nv: 48,
                },
            ));
        }
        if vars.contains("t") {
            return Ok((
                "Raumkurve".into(),
                Kind::Curve {
                    x: parts[0].clone(),
                    y: parts[1].clone(),
                    z: parts[2].clone(),
                    t0: "0".into(),
                    t1: "2pi".into(),
                    n: 800,
                },
            ));
        }
        return Err(
            "Tupel ohne u, v oder t – für eine Fläche (… u, v …), für eine Kurve (… t …)".into(),
        );
    }
    let f = match s.find('=') {
        Some(eq) => format!("({}) - ({})", &s[..eq], &s[eq + 1..]),
        None => s.clone(),
    };
    let e = expr::parse(&f)?;
    let mut vars = BTreeSet::new();
    expr::vars_of(&e, &mut vars);
    if !(vars.contains("x") || vars.contains("y") || vars.contains("z")) {
        return Err("implizite Fläche braucht x, y oder z".into());
    }
    let b = |v: &str| [v.to_string(), v.to_string(), v.to_string()];
    Ok((
        "Implizite Fläche".into(),
        Kind::Implicit {
            f,
            lo: b("-2"),
            hi: b("2"),
            res: 56,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mention() {
        assert!(mentions("\"a\":\"2a + h\"", "h"));
        assert!(!mentions("\"x\":\"theta + phi\"", "h"));
        assert!(!mentions("height", "h"));
    }

    #[test]
    fn quick() {
        assert!(matches!(
            quick_kind("x^2 + y^2 - z^2 = 0.3").unwrap().1,
            Kind::Implicit { .. }
        ));
        assert!(matches!(
            quick_kind("r(u,v) = (cos u, sin u, v)").unwrap().1,
            Kind::ParamSurface { .. }
        ));
        assert!(matches!(
            quick_kind("(cos t, sin t, t/5)").unwrap().1,
            Kind::Curve { .. }
        ));
        assert!(matches!(
            quick_kind("f(x,y,z) = x^4 + y^4 + z^4 - 1").unwrap().1,
            Kind::Implicit { .. }
        ));
    }
}
