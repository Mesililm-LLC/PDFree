use crate::{constants, EngineError};
use crate::backend::DisplayListBackend;
use viewer_core::{PixelTile, TileKey};

pub(crate) fn render(list: &dyn DisplayListBackend, key: TileKey) -> Result<PixelTile, EngineError> {
    if key.width == 0 || key.height == 0 || key.width > constants::MAX_TILE_DIMENSION || key.height > constants::MAX_TILE_DIMENSION || key.rotation != 0
    {
        return Err(EngineError::Render(
            "Invalid tile dimensions or rotation".into(),
        ));
    }
    let bounds = list.bounds().map_err(|e| EngineError::Render(e))?;
    let scale = key.scale_milli as f32 / constants::SCALE_MILLI_FACTOR;
    let result = list.render(
        key.width,
        key.height,
        key.rotation,
        key.scale_milli,
        bounds.0,
        bounds.1,
    )?;
    Ok(PixelTile {
        width: key.width,
        height: key.height,
        rgba: result.rgba,
    })
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
                &mupdf::Matrix::new_scale(2.0, 2.0),
                &mupdf::Colorspace::device_rgb(),
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
        let backend_list: Box<dyn DisplayListBackend> =
            Box::new(crate::backend::mupdf::MupdfDisplayList { list });
        let tile = render(backend_list.as_ref(), key).unwrap();
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
