//! Strebwerk – interaktive 3D-Umgebung für exakte mathematische Konstruktionen.
#![allow(dead_code)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod algebra;
mod analysis;
mod app;
mod export;
mod expr;
mod mesh;
mod poly;
mod render;
mod scene;

/// `strebwerk --tex szene.strebwerk [ausgabe.tex]`: Formeln, Schnitte und Interaktionen ohne Fenster exportieren.
fn export_cli(args: &[String]) -> Result<String, String> {
    let src = args
        .first()
        .ok_or("Aufruf: strebwerk --tex szene.strebwerk [ausgabe.tex]")?;
    let txt = std::fs::read_to_string(src).map_err(|e| format!("{src}: {e}"))?;
    let f: app::SaveFile = serde_json::from_str(&txt).map_err(|e| format!("{src}: {e}"))?;
    let mut cache = scene::Cache::new();
    let built: Vec<(scene::Obj, scene::Built)> = f
        .scene
        .objects
        .iter()
        .filter(|o| o.visible)
        .map(|o| (o.clone(), scene::build(o, &f.scene, &mut cache)))
        .collect();
    let vis: Vec<(&scene::Obj, &scene::Built)> = built.iter().map(|(o, b)| (o, b)).collect();
    let (secs, profs, inters) = analysis::analyze_all(&f.scene, &vis, true);
    let tex = export::latex_document(&vis, &f.scene, &secs, &profs, &inters);
    let out = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| src.trim_end_matches(".strebwerk").to_string() + ".tex");
    std::fs::write(&out, tex).map_err(|e| format!("{out}: {e}"))?;
    Ok(out)
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(|a| a == "--tex").unwrap_or(false) {
        match export_cli(&args[2..]) {
            Ok(out) => println!("geschrieben: {out}"),
            Err(e) => {
                eprintln!("Fehler: {e}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1480.0, 900.0])
            .with_min_inner_size([900.0, 560.0])
            .with_title("Strebwerk"),
        renderer: eframe::Renderer::Wgpu,
        multisampling: 4,
        ..Default::default()
    };
    eframe::run_native(
        "Strebwerk",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}
