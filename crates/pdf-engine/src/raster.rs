use crate::EngineError;
use mupdf::{Colorspace, Device, DisplayList, IRect, Matrix, Pixmap, Rect};
use viewer_core::{PixelTile, TileKey};

pub(crate) fn render(list: &DisplayList, key: TileKey) -> Result<PixelTile, EngineError> {
    if key.width == 0 || key.height == 0 || key.width > 512 || key.height > 512 || key.rotation != 0
    {
        return Err(EngineError::Render(
            "Invalid tile dimensions or rotation".into(),
        ));
    }
    let raster = || -> Result<PixelTile, mupdf::Error> {
        let cs = Colorspace::device_rgb();
        let mut pixmap = Pixmap::new_with_w_h(&cs, key.width as i32, key.height as i32, false)?;
        pixmap.clear_with(255)?;
        let device = Device::from_pixmap_with_clip(
            &pixmap,
            IRect::new(0, 0, key.width as i32, key.height as i32),
        )?;
        let bounds = list.bounds();
        let scale = key.scale_milli as f32 / 1000.0;
        let matrix = Matrix::new(
            scale,
            0.0,
            0.0,
            scale,
            -bounds.x0 * scale - key.x as f32,
            -bounds.y0 * scale - key.y as f32,
        );
        list.run(
            &device,
            &matrix,
            Rect::new(0.0, 0.0, key.width as f32, key.height as f32),
        )?;
        drop(device);
        let stride = pixmap.stride() as usize;
        let mut rgba = Vec::with_capacity((key.width * key.height * 4) as usize);
        for row in pixmap.samples().chunks(stride).take(key.height as usize) {
            for pixel in row[..key.width as usize * 3].chunks_exact(3) {
                rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
            }
        }
        Ok(PixelTile {
            width: key.width,
            height: key.height,
            rgba,
        })
    };
    raster().map_err(|error| EngineError::Render(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clipped_hidpi_render_matches_the_same_region_of_a_full_page() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/corpus/transparency.pdf");
        let doc = mupdf::Document::open(path.to_str().unwrap()).unwrap();
        let page = doc.load_page(0).unwrap();
        let list = page.to_display_list(true).unwrap();
        let full = page
            .to_pixmap(
                &Matrix::new_scale(2.0, 2.0),
                &Colorspace::device_rgb(),
                false,
                true,
            )
            .unwrap();
        let key = TileKey {
            document: 1,
            page: 0,
            scale_milli: 2000,
            rotation: 0,
            x: 512,
            y: 512,
            width: 100,
            height: 100,
        };
        let tile = render(&list, key).unwrap();
        for y in 0..100usize {
            for x in 0..100usize {
                let index = (y + 512) * full.stride() as usize + (x + 512) * 3;
                assert_eq!(
                    &tile.rgba[(y * 100 + x) * 4..][..3],
                    &full.samples()[index..index + 3]
                );
            }
        }
    }
}
