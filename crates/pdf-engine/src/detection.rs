//! Offline field recognition. Only owned pixels/text leave the coordinator.
use mupdf::{Colorspace, Matrix, TextPageFlags, drawing::DrawingItem, pdf::PdfDocument};
use std::{
    collections::VecDeque,
    fs::{self, File, OpenOptions},
    io::Read,
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use viewer_core::{DetectedField, DetectionConfidence, FieldKind, FieldSpec, TextRect};

type Result<T> = std::result::Result<T, String>;
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
#[derive(Clone, Debug)]
struct Word {
    text: String,
    rect: TextRect,
    confidence: f32,
}
struct PageInput {
    page: usize,
    width: u32,
    height: u32,
    gray: Vec<u8>,
    scale: f32,
    words: Vec<Word>,
    vectors: Vec<TextRect>,
    // Underscore runs printed as text (flat print-to-PDF forms). Each rect is
    // the baseline of one run; candidates expand it into a field above it.
    blanks: Vec<TextRect>,
    circles: Vec<TextRect>,
    blocked: Vec<TextRect>,
    name_prefix: String,
}
type ActiveWorker = (Receiver<Result<Vec<DetectedField>>>, JoinHandle<()>);
pub(crate) struct Detection {
    pub document: u64,
    pub revision: u64,
    pub operation: u64,
    pub generation: u64,
    pub total: usize,
    pub processed: usize,
    pages: VecDeque<usize>,
    name_seed: u128,
    active: Option<ActiveWorker>,
    cancel: Arc<AtomicBool>,
    pub found: Vec<DetectedField>,
}
impl Drop for Detection {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        if let Some((_, handle)) = self.active.take() {
            let _ = handle.join();
        }
    }
}
impl Detection {
    pub fn new(
        document: u64,
        revision: u64,
        operation: u64,
        generation: u64,
        pages: Vec<usize>,
    ) -> Result<Self> {
        if pages.is_empty() || pages.len() > 100 {
            return Err("Detect 1–100 pages per run; use page ranges for larger documents.".into());
        }
        Ok(Self {
            document,
            revision,
            operation,
            generation,
            total: pages.len(),
            processed: 0,
            pages: pages.into(),
            name_seed: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(err)?
                .as_nanos(),
            active: None,
            cancel: Arc::new(AtomicBool::new(false)),
            found: vec![],
        })
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }
    pub fn advance(&mut self, doc: &PdfDocument) -> Result<bool> {
        if self.cancel.load(Ordering::Acquire) {
            return Err("Field detection cancelled; no fields were created.".into());
        }
        if let Some((receiver, _)) = &self.active {
            match receiver.try_recv() {
                Ok(result) => {
                    let (_, handle) = self.active.take().unwrap();
                    let _ = handle.join();
                    self.found.extend(result?);
                    self.processed += 1;
                    if self.found.len() > 2_000 {
                        return Err(
                            "Detection exceeded 2,000 fields; narrow the page range.".into()
                        );
                    }
                }
                Err(mpsc::TryRecvError::Empty) => return Ok(false),
                Err(_) => return Err("OCR worker stopped unexpectedly.".into()),
            }
        }
        if let Some(page) = self.pages.pop_front() {
            let input = snapshot(
                doc,
                page,
                format!("PDFree_Detected_{}_{}", self.name_seed, page + 1),
            )?;
            let (sender, receiver) = mpsc::sync_channel(1);
            let cancel = self.cancel.clone();
            let handle = thread::Builder::new()
                .name("pdf-form-recognition".into())
                .spawn(move || {
                    let result = recognize(input, &cancel);
                    let _ = sender.send(result);
                })
                .map_err(err)?;
            self.active = Some((receiver, handle));
            return Ok(false);
        }
        Ok(true)
    }
}
fn snapshot(doc: &PdfDocument, page: usize, name_prefix: String) -> Result<PageInput> {
    let native = doc.load_page(page as i32).map_err(err)?;
    let bounds = native.bounds().map_err(err)?;
    let scale = (300. / 72_f32)
        .min((18_000_000. / (bounds.width() * bounds.height())).sqrt())
        .min(8192. / bounds.width().max(bounds.height()));
    if !scale.is_finite() || scale < 0.5 {
        return Err("Page dimensions exceed field-detection limits.".into());
    }
    let pix = native
        .to_pixmap(
            &Matrix::new_scale(scale, scale),
            &Colorspace::device_gray(),
            false,
            false,
        )
        .map_err(err)?;
    let (width, height) = (pix.width(), pix.height());
    let mut gray = Vec::with_capacity(width as usize * height as usize);
    for y in 0..height as usize {
        gray.extend_from_slice(&pix.samples()[y * pix.stride() as usize..][..width as usize]);
    }
    let mut words = vec![];
    let mut vectors = vec![];
    let mut blanks = vec![];
    let mut images = vec![];
    let text = native
        .to_text_page(TextPageFlags::PRESERVE_IMAGES)
        .map_err(err)?;
    for block in text.blocks() {
        if block.image().is_some() {
            let b = block.bounds();
            images.push(TextRect {
                left: b.x0 - bounds.x0,
                top: b.y0 - bounds.y0,
                right: b.x1 - bounds.x0,
                bottom: b.y1 - bounds.y0,
            });
            continue;
        }
        for line in block.lines() {
            if words.len() > 10_000 {
                break;
            }
            let chars = line
                .chars()
                .filter_map(|ch| {
                    let c = ch.char()?;
                    let q = ch.quad();
                    Some((
                        c,
                        TextRect {
                            left: q.ul.x.min(q.ll.x) - bounds.x0,
                            top: q.ul.y.min(q.ur.y) - bounds.y0,
                            right: q.ur.x.max(q.lr.x) - bounds.x0,
                            bottom: q.ll.y.max(q.lr.y) - bounds.y0,
                        },
                    ))
                })
                .collect::<Vec<_>>();
            collect_text_line(chars, &mut words, &mut blanks, &mut vectors);
        }
    }
    let mut circles = vec![];
    for drawing in native.drawings().map_err(err)?.into_iter().take(10_000) {
        if drawing
            .items
            .iter()
            .filter(|i| matches!(i, DrawingItem::Curve(..)))
            .count()
            >= 4
        {
            let r = drawing.rect;
            if (5.0..=22.0).contains(&r.width()) && (r.width() - r.height()).abs() < 3.0 {
                circles.push(TextRect {
                    left: r.x0 - bounds.x0,
                    top: r.y0 - bounds.y0,
                    right: r.x1 - bounds.x0,
                    bottom: r.y1 - bounds.y0,
                });
            }
        }
        for item in drawing.items.into_iter().take(1024) {
            match item {
                DrawingItem::Rect { rect, .. } => vectors.push(TextRect {
                    left: rect.x0 - bounds.x0,
                    top: rect.y0 - bounds.y0,
                    right: rect.x1 - bounds.x0,
                    bottom: rect.y1 - bounds.y0,
                }),
                DrawingItem::Line(a, b) if (a.y - b.y).abs() < 1. && (a.x - b.x).abs() > 35. => {
                    vectors.push(TextRect {
                        left: a.x.min(b.x) - bounds.x0,
                        top: a.y.min(b.y) - bounds.y0,
                        right: a.x.max(b.x) - bounds.x0,
                        bottom: a.y.max(b.y) - bounds.y0,
                    })
                }
                _ => {}
            }
        }
    }
    let mut blocked = vec![];
    for f in crate::forms::page(doc, page)? {
        blocked.push(f.spec.rect);
        if let Some(c) = f.detection {
            blocked.push(c.origin);
        }
    }
    blocked.extend(crate::forms::suppressed(doc, page)?);
    // Raster analysis turns artwork inside embedded images (seal
    // checkerboards, photo edges) into convincing little boxes. Exclude
    // small images from detection; a full-page image is a scan, whose
    // fields are exactly what raster detection exists to find.
    let page_area = bounds.width() * bounds.height();
    blocked.extend(images.into_iter().filter(|r| {
        let area = (r.right - r.left).max(0.) * (r.bottom - r.top).max(0.);
        area > 0. && area <= 0.20 * page_area
    }));
    Ok(PageInput {
        page,
        width,
        height,
        gray,
        scale,
        words,
        vectors,
        blanks,
        circles,
        blocked,
        name_prefix,
    })
}

/// Split one extracted text line into words, underscore blanks, and checkbox
/// glyphs. Merging a whole line into one word made its rect span every blank
/// on the line, so occupancy checks discarded those fields; printed forms
/// (Word "Print to PDF") have no vectors for the detector to fall back on.
fn collect_text_line(
    chars: Vec<(char, TextRect)>,
    words: &mut Vec<Word>,
    blanks: &mut Vec<TextRect>,
    vectors: &mut Vec<TextRect>,
) {
    fn flush_word(words: &mut Vec<Word>, value: &mut String, rect: &mut Option<TextRect>) {
        if let Some(rect) = rect.take()
            && !value.trim().is_empty()
            && words.len() <= 10_000
        {
            words.push(Word {
                text: std::mem::take(value).trim().into(),
                rect,
                confidence: 1.,
            });
        } else {
            value.clear();
        }
    }
    fn flush_blank(blanks: &mut Vec<TextRect>, run: &mut Option<TextRect>) {
        if let Some(rect) = run.take()
            && rect.right - rect.left >= 15.
        {
            blanks.push(TextRect {
                left: rect.left,
                top: rect.bottom,
                right: rect.right,
                bottom: rect.bottom,
            });
        }
    }
    // Wingdings/ZapfDingbats ballot boxes and Unicode empty squares.
    const CHECKBOX_GLYPHS: &[char] = &['\u{f0a8}', '\u{f071}', '☐', '□', '▢'];
    let mut value = String::new();
    let mut word_rect: Option<TextRect> = None;
    let mut run: Option<TextRect> = None;
    for (c, rect) in chars {
        if c == '_' {
            flush_word(words, &mut value, &mut word_rect);
            run = Some(run.map_or(rect, |r| r.union(rect)));
            continue;
        }
        flush_blank(blanks, &mut run);
        if CHECKBOX_GLYPHS.contains(&c) {
            flush_word(words, &mut value, &mut word_rect);
            vectors.push(rect);
        } else if c.is_whitespace() {
            flush_word(words, &mut value, &mut word_rect);
        } else {
            if value.len() > 4096 {
                flush_word(words, &mut value, &mut word_rect);
            }
            value.push(c);
            word_rect = Some(word_rect.map_or(rect, |r| r.union(rect)));
        }
    }
    flush_blank(blanks, &mut run);
    flush_word(words, &mut value, &mut word_rect);
}
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Result<Self> {
        for n in 0..100 {
            let path = std::env::temp_dir().join(format!(
                "pdfree-ocr-{}-{}-{n}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(err)?
                    .as_nanos()
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(err(e)),
            }
        }
        Err("Could not reserve OCR scratch space.".into())
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn ocr(input: &PageInput, factor: u32, psm: &str, cancel: &AtomicBool) -> Result<Vec<Word>> {
    let exe_dir = std::env::current_exe()
        .map_err(err)?
        .parent()
        .ok_or("Missing executable folder")?
        .join("ocr");
    let dir = std::env::var_os("PDFREE_OCR_DIR")
        .map(PathBuf::from)
        .unwrap_or(exe_dir);
    let exe = dir.join("tesseract.exe");
    if !exe.is_file() || !dir.join("tessdata/eng.traineddata").is_file() {
        return Err("Offline English OCR is unavailable. Use the complete portable package, or place fields manually.".into());
    }
    let scratch = Scratch::new()?;
    let image = scratch.0.join("page.png");
    let output = scratch.0.join("result");
    let w = input.width / factor;
    let h = input.height / factor;
    let mut pixels = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        for x in 0..w {
            pixels.push(input.gray[((y * factor) * input.width + x * factor) as usize]);
        }
    }
    {
        let f = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&image)
            .map_err(err)?;
        let mut encoder = png::Encoder::new(f, w, h);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .map_err(err)?
            .write_image_data(&pixels)
            .map_err(err)?;
    }
    let stdout = File::create(scratch.0.join("stdout")).map_err(err)?;
    let stderr = File::create(scratch.0.join("stderr")).map_err(err)?;
    let mut cmd = Command::new(exe);
    cmd.arg(&image)
        .arg(&output)
        .args([
            "-l",
            "eng",
            "--psm",
            psm,
            "--dpi",
            &((input.scale * 72. / factor as f32).round() as u32).to_string(),
            "--tessdata-dir",
        ])
        .arg(dir.join("tessdata"))
        .arg("tsv")
        .env("OMP_THREAD_LIMIT", "1")
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let mut child = cmd.spawn().map_err(err)?;
    let started = Instant::now();
    loop {
        if cancel.load(Ordering::Acquire) || started.elapsed() > Duration::from_secs(30) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("OCR cancelled or exceeded its 30-second page limit.".into());
        }
        if let Some(status) = child.try_wait().map_err(err)? {
            if !status.success() {
                return Err(
                    "Offline OCR could not recognize this page. Manual fields remain available."
                        .into(),
                );
            }
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    let mut text = String::new();
    File::open(output.with_extension("tsv"))
        .map_err(err)?
        .take(4 * 1024 * 1024 + 1)
        .read_to_string(&mut text)
        .map_err(err)?;
    if text.len() > 4 * 1024 * 1024 {
        return Err("OCR output exceeded the safe page limit.".into());
    }
    parse_tsv(&text, input.scale / factor as f32)
}
fn parse_tsv(text: &str, scale: f32) -> Result<Vec<Word>> {
    let mut words = vec![];
    for line in text.lines().skip(1).take(20_001) {
        let p = line.splitn(12, '\t').collect::<Vec<_>>();
        if p.len() != 12
            || p[0] != "5"
            || p[11].trim().is_empty()
            || p[11].chars().all(|c| matches!(c, '_' | '-' | '|'))
        {
            continue;
        }
        let n = |i: usize| {
            p[i].parse::<f32>()
                .map_err(|_| "Invalid OCR geometry".to_string())
        };
        let (x, y, w, h) = (n(6)? / scale, n(7)? / scale, n(8)? / scale, n(9)? / scale);
        let confidence = n(10)? / 100.;
        if ![x, y, w, h, confidence].iter().all(|v| v.is_finite()) || w <= 0. || h <= 0. {
            continue;
        }
        words.push(Word {
            text: p[11].into(),
            rect: TextRect {
                left: x,
                top: y,
                right: x + w,
                bottom: y + h,
            },
            confidence,
        });
    }
    if words.len() > 10_000 {
        return Err("OCR page exceeds 10,000 words.".into());
    }
    Ok(words)
}
fn overlap(a: TextRect, b: TextRect) -> f32 {
    let intersection = (a.right.min(b.right) - a.left.max(b.left)).max(0.)
        * (a.bottom.min(b.bottom) - a.top.max(b.top)).max(0.);
    intersection / ((a.right - a.left) * (a.bottom - a.top)).max(1.)
}
fn rotate_rect(r: TextRect, angle: f32, cx: f32, cy: f32) -> TextRect {
    let (s, c) = angle.sin_cos();
    let mut out = TextRect {
        left: f32::INFINITY,
        top: f32::INFINITY,
        right: f32::NEG_INFINITY,
        bottom: f32::NEG_INFINITY,
    };
    for (x, y) in [
        (r.left, r.top),
        (r.right, r.top),
        (r.right, r.bottom),
        (r.left, r.bottom),
    ] {
        let (x, y) = (x - cx, y - cy);
        let (x, y) = (c * x + s * y + cx, -s * x + c * y + cy);
        out.left = out.left.min(x);
        out.top = out.top.min(y);
        out.right = out.right.max(x);
        out.bottom = out.bottom.max(y);
    }
    out
}
fn deskew(input: &mut PageInput, cancel: &AtomicBool) -> Result<f32> {
    if !input.vectors.is_empty() {
        return Ok(0.);
    }
    let step = (input.width / 700).max(2) as usize;
    let w = input.width as usize;
    let h = input.height as usize;
    let mut points = vec![];
    for y in (0..h).step_by(step) {
        for x in (0..w).step_by(step) {
            let dark = (y..(y + step).min(h))
                .any(|yy| (x..(x + step).min(w)).any(|xx| input.gray[yy * w + xx] < 210));
            if dark {
                points.push((x as f32 / step as f32, y as f32 / step as f32));
            }
        }
    }
    if points.len() < 50 {
        return Ok(0.);
    }
    let mut best = (0_f32, 0_f64);
    for degree in -20..=20 {
        let angle = (degree as f32 * 0.25).to_radians();
        let (s, c) = angle.sin_cos();
        let mut rows = vec![0_u32; h / step + w / step + 256];
        for (x, y) in &points {
            let row = (y * c - x * s + (w / step) as f32 + 64.).round() as usize;
            if row < rows.len() {
                rows[row] += 1;
            }
        }
        let score = rows.iter().map(|n| f64::from(*n).powi(2)).sum::<f64>();
        if score > best.1 {
            best = (angle, score);
        }
    }
    let angle = best.0;
    if angle.abs() < 0.35_f32.to_radians() {
        return Ok(0.);
    }
    let (s, c) = angle.sin_cos();
    let (cx, cy) = (w as f32 / 2., h as f32 / 2.);
    let mut gray = vec![255; input.gray.len()];
    for y in 0..h {
        if cancel.load(Ordering::Acquire) {
            return Err("Field detection cancelled.".into());
        }
        for x in 0..w {
            let (dx, dy) = (x as f32 - cx, y as f32 - cy);
            let sx = (c * dx - s * dy + cx).round() as i32;
            let sy = (s * dx + c * dy + cy).round() as i32;
            if sx >= 0 && sy >= 0 && (sx as usize) < w && (sy as usize) < h {
                gray[y * w + x] = input.gray[sy as usize * w + sx as usize];
            }
        }
    }
    input.gray = gray;
    let (cx, cy) = (cx / input.scale, cy / input.scale);
    for word in &mut input.words {
        word.rect = rotate_rect(word.rect, angle, cx, cy);
    }
    for r in &mut input.blocked {
        *r = rotate_rect(*r, angle, cx, cy);
    }
    Ok(angle)
}
fn label(words: &[Word], r: TextRect, checkbox: bool) -> Option<(String, f32)> {
    let mut near = words
        .iter()
        .filter_map(|w| {
            let vertical = (w.rect.top - r.top).abs();
            let left = r.left - w.rect.right;
            let above = r.top - w.rect.bottom;
            let right = w.rect.left - r.right;
            let distance = if checkbox && (-2. ..180.).contains(&right) && vertical < 14. {
                right + vertical * 2.
            } else if (-3. ..300.).contains(&left) && vertical < 22. {
                left + vertical * 2.
            } else if (-3. ..28.).contains(&above)
                && w.rect.left < r.right
                && w.rect.right > r.left - 10.
            {
                above * 2. + (w.rect.left - r.left).abs() * 0.2
            } else {
                return None;
            };
            Some((distance, w))
        })
        .collect::<Vec<_>>();
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    let (_, first) = near.first()?;
    let line_y = first.rect.top;
    let mut same = near
        .iter()
        .filter(|(_, w)| (w.rect.top - line_y).abs() < 5.)
        .map(|(_, w)| *w)
        .collect::<Vec<_>>();
    same.sort_by(|a, b| a.rect.left.total_cmp(&b.rect.left));
    // On lines with several fields ("Date: ____  Taken by: ____"), only the
    // contiguous words ending nearest this field are its label; a wide gap
    // is where the previous field sits.
    if let Some(anchor) = same
        .iter()
        .rposition(|w| w.rect.right <= r.left + 3. && (w.rect.top - r.top).abs() < 22.)
    {
        let mut start = anchor;
        while start > 0 && same[start].rect.left - same[start - 1].rect.right < 18. {
            start -= 1;
        }
        same = same[start..=anchor].to_vec();
    }
    let text = same
        .iter()
        .map(|w| w.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let confidence = same.iter().map(|w| w.confidence).fold(1_f32, f32::min);
    Some((text.trim().trim_end_matches(':').to_string(), confidence))
}
#[derive(Clone, Copy)]
struct Shape {
    rect: TextRect,
    closed: bool,
    round: bool,
    native: bool,
    // Printed underscore blank: explicit field evidence, so it needs no
    // keyword label to be believed.
    blank: bool,
}
fn raster_shapes(input: &PageInput) -> Vec<Shape> {
    // Downsample to ~150 DPI. Group horizontal runs; pair box edges only when
    // both vertical sides are present, so letters and table text are excluded.
    let factor = 2_u32;
    let w = (input.width / factor) as usize;
    let h = (input.height / factor) as usize;
    let scale = input.scale / factor as f32;
    let mut hist = [0usize; 256];
    for p in input.gray.iter().step_by(4) {
        hist[*p as usize] += 1;
    }
    let total = hist.iter().sum::<usize>() as f64;
    let sum = hist
        .iter()
        .enumerate()
        .map(|(i, n)| i as f64 * (*n as f64))
        .sum::<f64>();
    let (mut weight, mut sum_bg, mut best, mut threshold) = (0_f64, 0_f64, 0_f64, 170_u8);
    for (i, count) in hist.iter().enumerate() {
        weight += *count as f64;
        if weight == 0. || weight == total {
            continue;
        }
        sum_bg += i as f64 * (*count as f64);
        let variance = weight
            * (total - weight)
            * (sum_bg / weight - (sum - sum_bg) / (total - weight)).powi(2);
        if variance > best {
            best = variance;
            threshold = (i + 24).clamp(180, 235) as u8;
        }
    }
    let dark = |x: usize, y: usize| {
        input.gray[(y * factor as usize) * input.width as usize + x * factor as usize] < threshold
    };
    let mut runs: Vec<(usize, usize, usize)> = vec![];
    for y in 1..h.saturating_sub(1) {
        let mut x = 1;
        while x < w.saturating_sub(1) {
            if !dark(x, y) {
                x += 1;
                continue;
            }
            let start = x;
            let mut end = x;
            let mut gap = 0;
            while x < w.saturating_sub(1) {
                if dark(x, y) {
                    end = x;
                    gap = 0;
                } else {
                    gap += 1;
                    if gap > 3 {
                        break;
                    }
                }
                x += 1;
            }
            if (end - start) as f32 / scale >= 5. {
                if let Some(last) = runs.iter_mut().rev().take(64).find(|(a, b, c)| {
                    y - *c <= 3 && start.abs_diff(*a) <= 3 && end.abs_diff(*b) <= 3
                }) {
                    last.2 = y;
                } else {
                    runs.push((start, end, y));
                }
            }
            x += 1;
        }
        if runs.len() > 10_000 {
            break;
        }
    }
    let mut shapes = vec![];
    let mut paired = HashSetLocal::new();
    for (i, (x1, x2, y)) in runs.iter().copied().enumerate() {
        let width = (x2 - x1) as f32 / scale;
        let mut closed = false;
        for (j, (a, b, bottom)) in runs.iter().copied().enumerate().skip(i + 1).take(256) {
            if bottom <= y {
                continue;
            }
            let height = (bottom - y) as f32 / scale;
            if height < 4. {
                continue;
            }
            if height > 60. {
                break;
            }
            if x1.abs_diff(a) > 4 || x2.abs_diff(b) > 4 {
                continue;
            }
            let sides = (y..=bottom)
                .filter(|yy| {
                    (x1.saturating_sub(2)..=(x1 + 2).min(w - 1)).any(|xx| dark(xx, *yy))
                        && (x2.saturating_sub(2)..=(x2 + 2).min(w - 1)).any(|xx| dark(xx, *yy))
                })
                .count() as f32
                / (bottom - y + 1) as f32;
            if sides > 0.75
                && ((width >= 35. && height >= 10.)
                    || ((width - height).abs() < 4. && width <= 22.))
            {
                shapes.push(Shape {
                    rect: TextRect {
                        left: x1 as f32 / scale,
                        top: y as f32 / scale,
                        right: x2 as f32 / scale,
                        bottom: bottom as f32 / scale,
                    },
                    closed: true,
                    round: false,
                    native: false,
                    blank: false,
                });
                paired.insert(i);
                paired.insert(j);
                closed = true;
                break;
            }
        }
        if !closed && !paired.contains(&i) && (35. ..=450.).contains(&width) {
            shapes.push(Shape {
                rect: TextRect {
                    left: x1 as f32 / scale,
                    top: (y as f32 / scale - 18.).max(0.),
                    right: x2 as f32 / scale,
                    bottom: (y as f32 / scale - 1.).max(3.),
                },
                closed: false,
                round: false,
                native: false,
                blank: false,
            });
        }
    }
    // Round isolated outlines require explicit grouping text before becoming
    // radio fields. OCR occupancy rejects letters and existing printed marks.
    let mut visited = vec![false; w * h];
    let limit = (24.0 * scale) as usize;
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let start = y * w + x;
            if visited[start] || !dark(x, y) {
                continue;
            }
            let mut queue = vec![(x, y)];
            visited[start] = true;
            let (mut l, mut t, mut r, mut b) = (x, y, x, y);
            let mut count = 0;
            while let Some((xx, yy)) = queue.pop() {
                count += 1;
                l = l.min(xx);
                t = t.min(yy);
                r = r.max(xx);
                b = b.max(yy);
                for (nx, ny) in [
                    (xx.saturating_sub(1), yy),
                    (xx + 1, yy),
                    (xx, yy.saturating_sub(1)),
                    (xx, yy + 1),
                ] {
                    if nx >= w || ny >= h {
                        continue;
                    }
                    let n = ny * w + nx;
                    if !visited[n] && dark(nx, ny) {
                        visited[n] = true;
                        queue.push((nx, ny));
                    }
                }
            }
            let (cw, ch) = (r - l + 1, b - t + 1);
            if cw > limit
                || ch > limit
                || cw < (5.0 * scale) as usize
                || ch < (5.0 * scale) as usize
                || cw.abs_diff(ch) > 3
            {
                continue;
            }
            let corners = [(l, t), (r, t), (l, b), (r, b)]
                .iter()
                .filter(|(xx, yy)| dark(*xx, *yy))
                .count();
            if corners == 0 && !dark((l + r) / 2, (t + b) / 2) && count * 100 / (cw * ch) < 65 {
                shapes.push(Shape {
                    rect: TextRect {
                        left: l as f32 / scale,
                        top: t as f32 / scale,
                        right: (r + 1) as f32 / scale,
                        bottom: (b + 1) as f32 / scale,
                    },
                    closed: true,
                    round: true,
                    native: false,
                    blank: false,
                });
            }
        }
    }
    shapes
}
type HashSetLocal = std::collections::HashSet<usize>;
fn candidates(input: &PageInput, words: &[Word]) -> Vec<DetectedField> {
    let mut rects = input
        .vectors
        .iter()
        .copied()
        .map(|mut r| {
            let closed = r.bottom - r.top >= 3.;
            if !closed {
                r.top = (r.top - 18.).max(0.);
                r.bottom -= 1.;
            }
            Shape {
                rect: r,
                closed,
                round: false,
                native: true,
                blank: false,
            }
        })
        .collect::<Vec<_>>();
    rects.extend(input.blanks.iter().map(|r| Shape {
        rect: TextRect {
            left: r.left,
            top: (r.top - 18.).max(0.),
            right: r.right,
            bottom: (r.bottom - 1.).max(3.),
        },
        closed: false,
        round: false,
        native: true,
        blank: true,
    }));
    rects.extend(input.circles.iter().map(|r| Shape {
        rect: *r,
        closed: true,
        round: true,
        native: true,
        blank: false,
    }));
    rects.extend(raster_shapes(input));
    rects.sort_by(|a, b| {
        a.rect
            .top
            .total_cmp(&b.rect.top)
            .then(a.rect.left.total_cmp(&b.rect.left))
    });
    let mut found: Vec<DetectedField> = vec![];
    for shape in rects.iter().copied() {
        let r = shape.rect;
        let (w, h) = (r.right - r.left, r.bottom - r.top);
        let check = (w - h).abs() < 5. && (5. ..=22.).contains(&w) && (5. ..=22.).contains(&h);
        let sized = ((35. ..=500.).contains(&w) && (9. ..=65.).contains(&h))
            || (shape.blank && (18. ..=500.).contains(&w) && (9. ..=65.).contains(&h));
        if !check && !sized {
            continue;
        }
        if input
            .blocked
            .iter()
            .any(|b| overlap(r, *b) > 0.2 || overlap(*b, r) > 0.5)
            || found
                .iter()
                .any(|f| overlap(r, f.spec.rect) > 0.6 || overlap(f.spec.rect, r) > 0.6)
        {
            continue;
        }
        if words.iter().any(|word| {
            !(check
                && word.text.chars().count() <= 2
                && overlap(r, word.rect) > 0.6
                && overlap(word.rect, r) > 0.8)
                && (overlap(word.rect, r) > 0.6 || overlap(r, word.rect) > 0.6)
        }) {
            continue;
        }
        let nearby = label(words, r, check);
        let (label, ocr_conf) = nearby.clone().unwrap_or(("Field".into(), 0.4));
        if !check && !shape.blank && w < 60. && (!shape.closed || nearby.is_none()) {
            continue;
        }
        let lower = label.to_ascii_lowercase();
        let same_rows = rects
            .iter()
            .map(|s| s.rect)
            .filter(|other| {
                (other.left - r.left).abs() < 3.
                    && (other.right - r.right).abs() < 3.
                    && (other.bottom - other.top - h).abs() < 3.
                    && (other.top - r.top).abs() > 3.
                    && (other.top - r.top).abs() < 100.
            })
            .collect::<Vec<_>>();
        let grid = same_rows.len() >= 2
            && same_rows
                .iter()
                .any(|other| (other.top - r.top).abs() < h * 1.5 + 3.);
        let useful_label = [
            "name",
            "address",
            "email",
            "phone",
            "date",
            "signature",
            "signed by",
            "amount",
            "quantity",
            "price",
            "total",
            "number of",
            "age",
            "notes",
            "comment",
            "description",
            "city",
            "state",
            "postal",
            "zip",
            "country",
            "account",
            "code",
            "company",
            "title",
            "first",
            "last",
            "birth",
            "gender",
            "reason",
            "response",
            "answer",
            "question",
            "other",
        ]
        .iter()
        .any(|s| lower.contains(s));
        let direct_label = words.iter().any(|word| {
            (-3. ..300.).contains(&(r.left - word.rect.right))
                && (word.rect.top - r.top).abs() < 12.
        });
        if !shape.blank
            && ((grid && !check && (!useful_label || !direct_label))
                || (!shape.closed && !useful_label))
        {
            continue;
        }
        let tokens = lower
            .split(|c: char| !c.is_ascii_alphanumeric())
            .collect::<Vec<_>>();
        let group = if check && shape.round {
            words
                .iter()
                .filter(|word| {
                    let header = word.text.to_ascii_lowercase();
                    (header.contains("select one")
                        || header.contains("choose one")
                        || ((header == "select" || header == "choose")
                            && words.iter().any(|next| {
                                next.text.eq_ignore_ascii_case("one")
                                    && (next.rect.top - word.rect.top).abs() < 5.0
                                    && next.rect.left >= word.rect.right
                                    && next.rect.left - word.rect.right < 30.0
                            })))
                        && word.rect.bottom <= r.top + 3.0
                        && r.top - word.rect.bottom < 180.0
                })
                .min_by(|a, b| (r.top - a.rect.bottom).total_cmp(&(r.top - b.rect.bottom)))
                .map(|word| {
                    format!(
                        "{}_group_{}",
                        input.name_prefix,
                        word.rect.top.round() as i32
                    )
                })
        } else {
            None
        };
        // An isolated round outline is not enough evidence for a form control.
        // In particular, letters in short words must not become checkboxes.
        if shape.round && group.is_none() {
            continue;
        }
        let kind = if group.is_some() {
            FieldKind::Radio
        } else if check {
            FieldKind::Checkbox
        } else if lower.contains("signature") || lower.contains("signed by") {
            FieldKind::VisualSignature
        } else if lower.contains("date")
            || tokens.contains(&"dob")
            || lower.contains("birth")
            || lower.contains("born on")
        {
            FieldKind::Date
        } else if ["amount", "quantity", "price", "total", "age"]
            .iter()
            .any(|s| tokens.contains(s))
            || lower.contains("number of")
        {
            FieldKind::Number
        } else {
            FieldKind::Text
        };
        let mut score = if nearby.is_some() {
            0.76 + 0.22 * ocr_conf.max(0.)
        } else {
            0.58
        };
        if !shape.closed && !shape.blank && (!shape.native || w < 90.) {
            score = score.min(0.86);
        }
        let spec = FieldSpec {
            page: input.page,
            name: format!("{}_{}", input.name_prefix, found.len() + 1),
            label,
            kind,
            rect: r,
            required: false,
            read_only: false,
            options: vec![],
            group: group.unwrap_or_default(),
            review: score < 0.9,
            date_format: "YYYY-MM-DD".into(),
            decimals: 2,
        };
        if spec.validate().is_ok() {
            found.push(DetectedField {
                spec,
                confidence: DetectionConfidence { score, origin: r },
            });
        }
        if found.len() >= 512 {
            break;
        }
    }
    let groups = found
        .iter()
        .filter(|f| f.spec.kind == FieldKind::Radio)
        .map(|f| f.spec.group.clone())
        .collect::<Vec<_>>();
    for field in &mut found {
        if field.spec.kind == FieldKind::Radio
            && groups.iter().filter(|g| **g == field.spec.group).count() < 2
        {
            field.spec.kind = FieldKind::Checkbox;
            field.spec.group.clear();
        }
    }
    found
}
fn recheck_crop(input: &PageInput, found: &[DetectedField]) -> (PageInput, (f32, f32)) {
    let uncertain = found
        .iter()
        .filter(|f| f.spec.review)
        .map(|f| f.spec.rect)
        .reduce(|a, b| a.union(b))
        .expect("uncertain candidate");
    let left = ((uncertain.left - 300.).max(0.) * input.scale)
        .floor()
        .min(input.width.saturating_sub(1) as f32) as u32;
    let top = ((uncertain.top - 50.).max(0.) * input.scale)
        .floor()
        .min(input.height.saturating_sub(1) as f32) as u32;
    let right = ((uncertain.right + 180.) * input.scale)
        .ceil()
        .clamp((left + 1) as f32, input.width as f32) as u32;
    let bottom = ((uncertain.bottom + 30.) * input.scale)
        .ceil()
        .clamp((top + 1) as f32, input.height as f32) as u32;
    let (width, height) = (right - left, bottom - top);
    let mut gray = Vec::with_capacity(width as usize * height as usize);
    for y in top..bottom {
        let begin = (y * input.width + left) as usize;
        gray.extend_from_slice(&input.gray[begin..begin + width as usize]);
    }
    (
        PageInput {
            page: input.page,
            width,
            height,
            gray,
            scale: input.scale,
            words: vec![],
            vectors: vec![],
            blanks: vec![],
            circles: vec![],
            blocked: vec![],
            name_prefix: input.name_prefix.clone(),
        },
        (left as f32 / input.scale, top as f32 / input.scale),
    )
}
fn recognize(mut input: PageInput, cancel: &AtomicBool) -> Result<Vec<DetectedField>> {
    let angle = deskew(&mut input, cancel)?;
    let words = if input.words.is_empty() {
        ocr(&input, 2, "3", cancel)?
    } else {
        input.words.clone()
    };
    let mut found = candidates(&input, &words);
    // The OCR recheck only refines uncertain fields. When the page already
    // has digital text, unavailable OCR must not discard first-pass fields.
    let recheck = if found.iter().any(|f| f.spec.review) {
        let (crop, offset) = recheck_crop(&input, &found);
        match ocr(&crop, 1, "11", cancel) {
            Ok(second) => Some((second, offset)),
            Err(error) if input.words.is_empty() => return Err(error),
            Err(_) => None,
        }
    } else {
        None
    };
    if let Some((mut second, offset)) = recheck {
        // Second independent segmentation pass at the full raster resolution.
        // Existing digital text remains the authoritative label source.
        for word in &mut second {
            word.rect.left += offset.0;
            word.rect.right += offset.0;
            word.rect.top += offset.1;
            word.rect.bottom += offset.1;
        }
        let improved = candidates(&input, &second);
        found.retain(|f| {
            !f.spec.review
                || improved
                    .iter()
                    .any(|other| overlap(f.spec.rect, other.spec.rect) > 0.7)
                || !second.iter().any(|word| {
                    !(f.spec.kind == FieldKind::Checkbox
                        && word.text.chars().count() <= 2
                        && overlap(f.spec.rect, word.rect) > 0.6
                        && overlap(word.rect, f.spec.rect) > 0.8)
                        && (overlap(word.rect, f.spec.rect) > 0.6
                            || overlap(f.spec.rect, word.rect) > 0.6)
                })
        });
        for field in &mut found {
            if !field.spec.review {
                continue;
            }
            if let Some(rechecked) = improved
                .iter()
                .filter(|f| overlap(field.spec.rect, f.spec.rect) > 0.7)
                .max_by(|a, b| a.confidence.score.total_cmp(&b.confidence.score))
                && rechecked.confidence.score > field.confidence.score
            {
                field.spec.label = rechecked.spec.label.clone();
                field.spec.kind = rechecked.spec.kind;
                field.confidence.score = rechecked.confidence.score;
                field.spec.review = field.confidence.score < 0.9;
            }
        }
    }
    if angle != 0. {
        let (cx, cy) = (
            input.width as f32 / input.scale / 2.,
            input.height as f32 / input.scale / 2.,
        );
        for field in &mut found {
            let mut r = rotate_rect(field.spec.rect, -angle, cx, cy);
            r.left = r.left.max(0.);
            r.top = r.top.max(0.);
            r.right = r.right.min(input.width as f32 / input.scale);
            r.bottom = r.bottom.min(input.height as f32 / input.scale);
            field.spec.rect = r;
            field.confidence.origin = r;
        }
    }
    if cancel.load(Ordering::Acquire) {
        return Err("Field detection cancelled; no fields were created.".into());
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn embedded_image_artwork_is_excluded_but_page_scans_are_not() {
        // The flat-form fixture embeds line-art squares (a stand-in for a
        // state seal) as an 80x80pt image at x 470..550, y 12..92.
        let seal = TextRect {
            left: 470.,
            top: 12.,
            right: 550.,
            bottom: 92.,
        };
        let doc = crate::files::open(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/corpus/flat-form.pdf"),
            None,
        )
        .unwrap();
        let input = snapshot(&doc, 0, "test".into()).unwrap();
        assert!(
            input
                .blocked
                .iter()
                .any(|b| overlap(*b, seal) > 0.9 && overlap(seal, *b) > 0.9),
            "small embedded image must be blocked: {:?}",
            input.blocked
        );
        let found = candidates(&input, &input.words);
        assert!(
            found.iter().all(|f| overlap(f.spec.rect, seal) < 0.5),
            "no field may be detected inside image artwork: {:?}",
            found
                .iter()
                .filter(|f| overlap(f.spec.rect, seal) >= 0.5)
                .map(|f| &f.spec.label)
                .collect::<Vec<_>>()
        );
        // A scanned page is one large image; excluding it would erase
        // scanned-form detection, so it must stay unblocked.
        let scan = crate::files::open(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus/scan.pdf"),
            None,
        )
        .unwrap();
        let input = snapshot(&scan, 0, "test".into()).unwrap();
        let page_area = 612. * 792.;
        assert!(
            input
                .blocked
                .iter()
                .all(|b| { (b.right - b.left) * (b.bottom - b.top) <= 0.20 * page_area }),
            "full-page scan image must not be blocked: {:?}",
            input.blocked
        );
    }
    #[test]
    fn labeled_blank_box_is_detected_and_existing_fields_are_excluded() {
        let r = TextRect {
            left: 280.,
            top: 40.,
            right: 380.,
            bottom: 62.,
        };
        let mut input = PageInput {
            page: 0,
            width: 800,
            height: 400,
            gray: vec![255; 800 * 400],
            scale: 2.,
            words: vec![Word {
                text: "Date:".into(),
                rect: TextRect {
                    left: 30.,
                    top: 42.,
                    right: 80.,
                    bottom: 56.,
                },
                confidence: 1.,
            }],
            vectors: vec![r],
            blanks: vec![],
            circles: vec![],
            blocked: vec![],
            name_prefix: "test".into(),
        };
        let result = candidates(&input, &input.words);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].spec.kind, FieldKind::Date);
        assert!(!result[0].spec.review);
        input.blocked.push(r);
        assert!(candidates(&input, &input.words).is_empty());
    }
    #[test]
    fn printed_underscore_blanks_and_checkbox_glyphs_become_fields() {
        // Lay out "Taken by: ______  M <box>" as extracted chars, the way a
        // Word print-to-PDF represents it: no vectors, all text glyphs.
        let mut chars = vec![];
        let mut x = 30_f32;
        for c in "Taken by: ".chars() {
            chars.push((
                c,
                TextRect {
                    left: x,
                    top: 42.,
                    right: x + 6.,
                    bottom: 56.,
                },
            ));
            x += 6.;
        }
        for _ in 0..20 {
            chars.push((
                '_',
                TextRect {
                    left: x,
                    top: 54.,
                    right: x + 6.,
                    bottom: 56.,
                },
            ));
            x += 6.;
        }
        x += 12.;
        for c in "M ".chars() {
            chars.push((
                c,
                TextRect {
                    left: x,
                    top: 42.,
                    right: x + 6.,
                    bottom: 56.,
                },
            ));
            x += 6.;
        }
        chars.push((
            '\u{f0a8}',
            TextRect {
                left: x,
                top: 44.,
                right: x + 10.,
                bottom: 54.,
            },
        ));
        let (mut words, mut blanks, mut vectors) = (vec![], vec![], vec![]);
        collect_text_line(chars, &mut words, &mut blanks, &mut vectors);
        assert_eq!(blanks.len(), 1);
        assert_eq!(vectors.len(), 1, "checkbox glyph becomes a closed rect");
        assert!(words.iter().any(|w| w.text == "Taken"));
        assert!(
            words
                .iter()
                .all(|w| w.rect.right <= blanks[0].left || w.rect.left >= blanks[0].right),
            "no word rect may span the blank"
        );
        let input = PageInput {
            page: 0,
            width: 800,
            height: 400,
            gray: vec![255; 800 * 400],
            scale: 2.,
            words: words.clone(),
            vectors,
            blanks,
            circles: vec![],
            blocked: vec![],
            name_prefix: "flat".into(),
        };
        let found = candidates(&input, &words);
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(
            found
                .iter()
                .any(|f| f.spec.kind == FieldKind::Text && f.spec.label.contains("Taken"))
        );
        assert!(found.iter().any(|f| f.spec.kind == FieldKind::Checkbox));
    }
    #[test]
    fn radio_groups_require_multiple_round_controls_and_explicit_grouping() {
        let rect = |x, y, w, h| TextRect {
            left: x,
            top: y,
            right: x + w,
            bottom: y + h,
        };
        let input = PageInput {
            page: 0,
            width: 800,
            height: 500,
            gray: vec![255; 800 * 500],
            scale: 2.,
            words: vec![
                Word {
                    text: "Select one".into(),
                    rect: rect(30., 20., 90., 12.),
                    confidence: 1.,
                },
                Word {
                    text: "Yes".into(),
                    rect: rect(120., 60., 30., 12.),
                    confidence: 1.,
                },
                Word {
                    text: "No".into(),
                    rect: rect(120., 95., 30., 12.),
                    confidence: 1.,
                },
            ],
            vectors: vec![],
            blanks: vec![],
            circles: vec![rect(100., 60., 12., 12.), rect(100., 95., 12., 12.)],
            blocked: vec![],
            name_prefix: "radios".into(),
        };
        let found = candidates(&input, &input.words);
        assert_eq!(found.len(), 2);
        assert!(found.iter().all(|f| f.spec.kind == FieldKind::Radio));
        assert_eq!(found[0].spec.group, found[1].spec.group);
        let words = input
            .words
            .iter()
            .filter(|w| w.text != "Select one")
            .cloned()
            .collect::<Vec<_>>();
        assert!(candidates(&input, &words).is_empty());
    }
    #[test]
    fn malformed_ocr_geometry_is_rejected() {
        assert!(parse_tsv("header\n5\t1\t1\t1\t1\t1\tbad\t1\t20\t10\t90\tName", 2.).is_err());
    }
}
