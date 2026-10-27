//! PDF backend trait abstraction.
//! Defines the operations the coordinator needs from a PDF engine, allowing
//! the backend to be swapped without changing the coordinator or subsystems.
use std::any::Any;
use std::path::Path;
use viewer_core::{DocumentInfo, OutlineEntry, PageSize};

/// A page handle returned by the backend.
pub trait PageBackend: Send + Sync {
    /// Returns the page's width and height in PDF points.
    fn bounds(&self) -> Result<PageSize, String>;

    /// Builds a display list for this page.
    fn to_display_list(&self, build: bool) -> Result<Box<dyn DisplayListBackend>, String>;

    /// Downcast escape hatch for backends that need access to concrete types.
    fn as_any(&self) -> &dyn Any;
}

/// A display list handle.
pub trait DisplayListBackend: Send + Sync {
    /// Returns the display list's bounding box as (x0, y0, x1, y1).
    fn bounds(&self) -> Result<(f32, f32, f32, f32), String>;

    /// Renders the display list to a pixel tile.
    fn render(
        &self,
        width: u32,
        height: u32,
        rotation: u32,
        scale_milli: u32,
        x: f32,
        y: f32,
    ) -> Result<viewer_core::PixelTile, String>;

    /// Downcast escape hatch for backends that need access to concrete types.
    fn as_any(&self) -> &dyn Any;
}

/// A document handle returned by the backend.
pub trait DocumentBackend: Send + Sync {
    /// Returns the number of pages in the document.
    fn page_count(&self) -> Result<usize, String>;

    /// Returns whether the document requires a password.
    fn needs_password(&self) -> Result<bool, String>;

    /// Authenticates with the given password.
    fn authenticate(&self, password: &str) -> Result<bool, String>;

    /// Loads a page by index.
    fn load_page(&self, page: i32) -> Result<Box<dyn PageBackend>, String>;

    /// Returns document info.
    fn info(&self) -> Result<DocumentInfo, String>;

    /// Returns the document's outline entries.
    fn outlines(&self) -> Result<(Vec<OutlineEntry>, bool), String>;

    /// Downcast escape hatch for backends that need access to concrete types.
    fn as_any(&self) -> &dyn Any;
}

/// Open a PDF document from a file path using the MuPDF backend.
pub fn open_document(
    path: &Path,
    password: Option<&str>,
) -> Result<Box<dyn DocumentBackend>, String> {
    crate::backend::mupdf::MupdfDocument::open(path, password)
}

/// MuPDF implementation of the backend traits.
pub mod mupdf {
    use super::*;
    use mupdf::pdf::PdfDocument;
    use mupdf::{Colorspace, Device, DisplayList, IRect, Matrix, Page, Pixmap, Rect};
    use std::path::Path;

    pub struct MupdfDocument {
        doc: PdfDocument,
    }

    impl MupdfDocument {
        pub fn open(
            path: &Path,
            password: Option<&str>,
        ) -> Result<Box<dyn DocumentBackend>, String> {
            let path_str = path.to_str().ok_or("Path is not valid Unicode")?;
            let doc = Document::open(path_str)
                .and_then(PdfDocument::try_from)
                .map_err(|e| e.to_string())?;
            if doc.needs_password().map_err(|e| e.to_string())? {
                let password = password.ok_or("This PDF requires a password.")?;
                if !doc.authenticate(password).map_err(|e| e.to_string())? {
                    return Err("The password was not accepted. Try again.".into());
                }
            }
            Ok(Box::new(MupdfDocument { doc }))
        }

        /// Returns the inner MuPDF document for subsystems that need concrete types.
        pub fn doc(&self) -> &PdfDocument {
            &self.doc
        }
    }

    impl DocumentBackend for MupdfDocument {
        fn page_count(&self) -> Result<usize, String> {
            self.doc
                .page_count()
                .map_err(|e| e.to_string())
                .map(|c| c as usize)
        }

        fn needs_password(&self) -> Result<bool, String> {
            self.doc.needs_password().map_err(|e| e.to_string())
        }

        fn authenticate(&self, password: &str) -> Result<bool, String> {
            self.doc.authenticate(password).map_err(|e| e.to_string())
        }

        fn load_page(&self, page: i32) -> Result<Box<dyn PageBackend>, String> {
            let page = self.doc.load_page(page).map_err(|e| e.to_string())?;
            Ok(Box::new(MupdfPage { page }))
        }

        fn info(&self) -> Result<DocumentInfo, String> {
            crate::forms::security(&self.doc)
                .map(|(signed, xfa, _)| {
                    let writable = !signed && !xfa;
                    let permissions = crate::forms::permissions(&self.doc);
                    DocumentInfo {
                        can_form: writable && permissions.contains(mupdf::pdf::Permission::FORM),
                        can_edit: writable
                            && !xfa
                            && permissions.contains(
                                mupdf::pdf::Permission::MODIFY | mupdf::pdf::Permission::ANNOTATE,
                            ),
                        signed,
                        can_copy: permissions.contains(mupdf::pdf::Permission::COPY),
                        can_annotate: writable
                            && permissions.contains(mupdf::pdf::Permission::ANNOTATE),
                        can_assemble: writable
                            && permissions.contains(mupdf::pdf::Permission::ASSEMBLE),
                        pages: self.page_count().unwrap_or(0),
                        first_page: PageSize::new(0., 0.),
                    }
                })
                .map_err(|e| e.to_string())
        }

        fn outlines(&self) -> Result<(Vec<OutlineEntry>, bool), String> {
            crate::navigation::outlines_mupdf(&self.doc)
        }

        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    pub struct MupdfPage {
        page: Page,
    }

    impl MupdfPage {
        /// Returns the inner MuPDF page for subsystems that need concrete types.
        pub fn page(&self) -> &Page {
            &self.page
        }
    }

    impl PageBackend for MupdfPage {
        fn bounds(&self) -> Result<PageSize, String> {
            let b = self.page.bounds().map_err(|e| e.to_string())?;
            Ok(PageSize::new(b.width(), b.height()))
        }

        fn to_display_list(&self, build: bool) -> Result<Box<dyn DisplayListBackend>, String> {
            let list = self
                .page
                .to_display_list(build)
                .map_err(|e| e.to_string())?;
            Ok(Box::new(MupdfDisplayList { list }))
        }

        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    pub struct MupdfDisplayList {
        list: DisplayList,
    }

    impl DisplayListBackend for MupdfDisplayList {
        fn bounds(&self) -> Result<(f32, f32, f32, f32), String> {
            let b = self.list.bounds();
            Ok((b.x0, b.y0, b.x1, b.y1))
        }

        fn render(
            &self,
            width: u32,
            height: u32,
            _rotation: u32,
            scale_milli: u32,
            x: f32,
            y: f32,
        ) -> Result<viewer_core::PixelTile, String> {
            let (x0, y0, x1, y1) = self.bounds()?;
            let cs = Colorspace::device_rgb();
            let mut pixmap = Pixmap::new_with_w_h(&cs, width as i32, height as i32, false)?;
            pixmap.clear_with(255)?;
            let device = Device::from_pixmap_with_clip(
                &pixmap,
                IRect::new(0, 0, width as i32, height as i32),
            )?;
            let scale = scale_milli as f32 / 1000.0;
            let matrix = Matrix::new(scale, 0.0, 0.0, scale, -x0 * scale - x, -y0 * scale - y);
            self.list.run(
                &device,
                &matrix,
                Rect::new(0.0, 0.0, width as f32, height as f32),
            )?;
            drop(device);
            let stride = pixmap.stride() as usize;
            let mut rgba = Vec::with_capacity((width * height * 4) as usize);
            for row in pixmap.samples().chunks(stride).take(height as usize) {
                for pixel in row[..width as usize * 3].chunks_exact(3) {
                    rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
                }
            }
            Ok(viewer_core::PixelTile {
                width,
                height,
                rgba,
            })
        }

        fn as_any(&self) -> &dyn Any {
            self
        }
    }
}
