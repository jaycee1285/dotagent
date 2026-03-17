mod app;
mod scanner;
mod sync;
mod syntax;
mod theme;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_title("dotagent — Skills & Hooks Explorer"),
        ..Default::default()
    };

    eframe::run_native(
        "dotagent",
        options,
        Box::new(|cc| Ok(Box::new(app::DotagentApp::new(cc)))),
    )
}
