use crate::AppWindow;
use slint::ComponentHandle;
use std::{fs::File, io::BufWriter, path::PathBuf};

pub fn snapshot(ui: &AppWindow, name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let Some(directory) = std::env::var_os("PDFREE_CAPTURE_DIR") else {
        return Ok(());
    };
    let directory = PathBuf::from(directory);
    std::fs::create_dir_all(&directory)?;
    let buffer = ui.window().take_snapshot()?;
    let file = BufWriter::new(File::create(directory.join(format!("{name}.png")))?);
    let mut encoder = png::Encoder::new(file, buffer.width(), buffer.height());
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()?
        .write_image_data(buffer.as_bytes())?;
    Ok(())
}
