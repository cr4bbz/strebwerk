//! Strebwerk – interaktive 3D-Umgebung für exakte mathematische Konstruktionen.
#![allow(dead_code)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod algebra;
mod app;
mod export;
mod expr;
mod mesh;
mod poly;
mod render;
mod scene;

fn main() -> eframe::Result<()> {
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
