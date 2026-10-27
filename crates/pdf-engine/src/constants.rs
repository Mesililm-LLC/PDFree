//! Centralized magic numbers and limits used across the pdf-engine crate.
//! All limits are documented with their rationale.

/// Maximum number of pages a document may have before being rejected.
pub const MAX_PAGES: usize = 1_000_000;

/// Maximum number of pages requested in a single forms query.
pub const MAX_FORM_PAGES: usize = 16;

/// Maximum number of pages requested in a single links query.
pub const MAX_LINK_PAGES: usize = 16;

/// Maximum number of pages requested in a single text query.
pub const MAX_TEXT_PAGES: usize = 16;

/// Maximum number of pages requested in a single notes query.
pub const MAX_NOTES_PAGES: usize = 16;

/// Maximum number of pages requested in a single thumbnails query.
pub const MAX_THUMBNAIL_PAGES: usize = 32;

/// Capacity of the render queue used for tile dispatch.
pub const RENDER_QUEUE_CAPACITY: usize = 128;

/// Maximum tile width or height in pixels.
pub const MAX_TILE_DIMENSION: u32 = 512;

/// Channel send timeout in milliseconds.
pub const CHANNEL_SEND_TIMEOUT_MILLIS: u64 = 10;

/// Channel receive timeout in milliseconds.
pub const CHANNEL_RECV_TIMEOUT_MILLIS: u64 = 10;

/// Wake channel receive timeout in milliseconds.
pub const WAKE_RECV_TIMEOUT_MILLIS: u64 = 2;

/// Priority assigned to thumbnail render requests.
pub const THUMBNAIL_PRIORITY: u32 = u32::MAX;

/// Maximum width for thumbnail pages in PDF points.
pub const THUMBNAIL_WIDTH_MAX: f32 = 140.0;

/// Maximum height for thumbnail pages in PDF points.
pub const THUMBNAIL_HEIGHT_MAX: f32 = 160.0;

/// Minimum scale factor applied to thumbnails.
pub const THUMBNAIL_SCALE_MIN: f32 = 1.0;

/// Scale numerator used when computing thumbnail scale from width.
pub const THUMBNAIL_WIDTH_SCALE: f32 = 140.0;

/// Scale numerator used when computing thumbnail scale from height.
pub const THUMBNAIL_HEIGHT_SCALE: f32 = 160.0;

/// Factor used to convert scale_milli to a floating-point scale.
pub const SCALE_MILLI_FACTOR: f32 = 1000.0;

/// Value used to clear a new pixmap (white).
pub const CLEAR_COLOR: u8 = 255;

/// Alpha value for fully opaque pixels.
pub const ALPHA_OPAQUE: u8 = 255;

/// Maximum number of annotations processed per page before truncation.
pub const MAX_ANNOTATIONS_PER_PAGE: usize = 10_000;

/// Maximum number of annotations returned per page.
pub const MAX_ANNOTATIONS_RETURNED: usize = 256;

/// Maximum total bytes of annotation data per page.
pub const MAX_ANNOTATION_BYTES: usize = 256 * 1024;

/// Maximum text length for note annotations.
pub const MAX_TEXT_LENGTH: usize = 16_384;

/// Maximum quad count for highlight annotations.
pub const MAX_QUAD_COUNT: usize = 1000;

/// Maximum pages supported for editing operations.
pub const MAX_EDIT_PAGES: usize = 100_000;

/// Maximum number of PDF files that can be inserted in one operation.
pub const MAX_INSERT_PATHS: usize = 16;

/// Maximum nesting depth for imported PDF object graphs.
pub const MAX_IMPORT_DEPTH: usize = 64;

/// Maximum number of nodes in an imported object graph.
pub const MAX_IMPORT_NODES: usize = 100_000;

/// Maximum size of a single imported stream in bytes.
pub const MAX_STREAM_BYTES: usize = 64 * 1024 * 1024;

/// Maximum total size of all imported streams in bytes.
pub const MAX_TOTAL_STREAM_BYTES: usize = 128 * 1024 * 1024;

/// Maximum array length in imported objects.
pub const MAX_IMPORT_ARRAY_LEN: usize = 10_000;

/// Maximum dictionary length in imported objects.
pub const MAX_IMPORT_DICT_LEN: usize = 2000;

/// Maximum number of outline entries before truncation.
pub const MAX_OUTLINE_ENTRIES: usize = 4096;

/// Maximum characters in an outline title.
pub const MAX_OUTLINE_TITLE_CHARS: usize = 512;

/// Maximum outline tree depth.
pub const MAX_OUTLINE_DEPTH: usize = 32;

/// Maximum number of links per page before truncation.
pub const MAX_LINKS: usize = 1024;

/// Maximum URI characters before treating as unsupported.
pub const MAX_URI_CHARS: usize = 8192;

/// Maximum characters in a truncated URI.
pub const MAX_TRUNCATED_URI_CHARS: usize = 256;

/// Maximum search query characters.
pub const MAX_SEARCH_QUERY_CHARS: usize = 512;

/// Maximum glyphs extracted per page.
pub const MAX_GLYPHS: usize = 100_000;

/// Maximum search hits before truncation.
pub const MAX_HITS: usize = 10_000;

/// Maximum number of detection pages per run.
pub const MAX_DETECTION_PAGES: usize = 100;

/// Maximum number of detected fields before aborting.
pub const MAX_DETECTION_FIELDS: usize = 2_000;

/// Maximum number of detected fields in candidate output.
pub const MAX_CANDIDATE_FIELDS: usize = 512;

/// OCR timeout in seconds per page.
pub const OCR_TIMEOUT_SECS: u64 = 30;

/// OCR poll interval in milliseconds.
pub const OCR_POLL_MILLIS: u64 = 20;

/// OCR output size limit in bytes.
pub const OCR_OUTPUT_LIMIT: usize = 4 * 1024 * 1024 + 1;

/// Maximum TSV lines parsed from OCR output.
pub const MAX_TSV_LINES: usize = 20_001;

/// Maximum OCR words per page.
pub const MAX_OCR_WORDS: usize = 10_000;

/// Dark pixel threshold for deskew (0-255).
pub const DARK_PIXEL_THRESHOLD: u8 = 210;

/// Minimum points required for deskew analysis.
pub const MIN_POINTS_FOR_DESKEW: usize = 50;

/// Deskew angle range in degrees (symmetric).
pub const DESKEW_ANGLE_MAX_DEGREES: i32 = 20;

/// Deskew angle step in degrees.
pub const DESKEW_ANGLE_STEP_DEGREES: f32 = 0.25;

/// Minimum angle (radians) to consider deskewing.
pub const DESKEW_MIN_ANGLE_RADIANS: f32 = 0.35_f32.to_radians();

/// Base threshold for binarization.
pub const THRESHOLD_BASE: u8 = 170;

/// Offset added to Otsu threshold.
pub const THRESHOLD_OFFSET: i32 = 24;

/// Minimum threshold after clamping.
pub const THRESHOLD_MIN: u8 = 180;

/// Maximum threshold after clamping.
pub const THRESHOLD_MAX: u8 = 235;

/// Downsample factor for raster shape analysis.
pub const RASTER_DOWNSCALE_FACTOR: u32 = 2;

/// Maximum lookback when pairing raster runs.
pub const MAX_RUNS_LOOKBACK: usize = 64;

/// Maximum runs before stopping raster analysis.
pub const MAX_RUNS: usize = 10_000;

/// Maximum lookahead when pairing runs.
pub const MAX_PAIR_LOOKAHEAD: usize = 256;

/// Minimum shape height in PDF points.
pub const MIN_SHAPE_HEIGHT: f32 = 4.0;

/// Maximum shape height in PDF points.
pub const MAX_SHAPE_HEIGHT: f32 = 60.0;

/// Width range for shape detection (PDF points).
pub const SHAPE_WIDTH_RANGE: (f32, f32) = (35.0, 450.0);

/// Factor for round shape detection limit.
pub const ROUND_LIMIT_FACTOR: f32 = 24.0;

/// Minimum dimension for round shapes.
pub const MIN_ROUND_DIMENSION: f32 = 5.0;

/// Maximum corner coordinate difference for pairing.
pub const MAX_CORNER_DIFF: usize = 3;

/// Minimum fill ratio for round shape acceptance.
pub const MIN_FILL_RATIO: usize = 65;

/// Crop top offset in PDF points.
pub const CROP_TOP_OFFSET: f32 = 18.0;

/// Crop top padding in PDF points.
pub const CROP_TOP_PAD: f32 = 50.0;

/// Crop right padding in PDF points.
pub const CROP_RIGHT_PAD: f32 = 180.0;

/// Crop bottom padding in PDF points.
pub const CROP_BOTTOM_PAD: f32 = 30.0;

/// Crop left offset in PDF points.
pub const CROP_LEFT_OFFSET: f32 = 300.0;

/// OCR downsample factor for the primary pass.
pub const OCR_FACTOR_PRIMARY: u32 = 2;

/// OCR downsample factor for the secondary pass.
pub const OCR_FACTOR_SECONDARY: u32 = 1;

/// OCR page segmentation mode for the primary pass.
pub const OCR_PSM_PRIMARY: &str = "3";

/// OCR page segmentation mode for the secondary pass.
pub const OCR_PSM_SECONDARY: &str = "11";

/// Base confidence score for detected fields with a label.
pub const SCORE_WITH_LABEL: f32 = 0.76;

/// Base confidence score for detected fields without a label.
pub const SCORE_WITHOUT_LABEL: f32 = 0.58;

/// OCR confidence weight added to the base score.
pub const SCORE_OCR_WEIGHT: f32 = 0.22;

/// Maximum score for unclosed shapes without native confirmation.
pub const SCORE_MAX_UNCLOSED: f32 = 0.86;

/// Score threshold below which a field is marked for review.
pub const REVIEW_THRESHOLD: f32 = 0.9;

/// Overlap threshold for blocked-field exclusion.
pub const BLOCKED_OVERLAP_THRESHOLD: f32 = 0.2;

/// Reverse overlap threshold for blocked-field exclusion.
pub const BLOCKED_OVERLAP_REVERSE_THRESHOLD: f32 = 0.5;

/// Overlap threshold between found fields.
pub const FOUND_OVERLAP_THRESHOLD: f32 = 0.6;

/// Overlap threshold between word and shape rects.
pub const WORD_OVERLAP_THRESHOLD: f32 = 0.6;

/// Reverse word overlap threshold.
pub const WORD_OVERLAP_REVERSE_THRESHOLD: f32 = 0.8;

/// Minimum width for open (non-closed) shapes.
pub const MIN_OPEN_SHAPE_WIDTH: f32 = 60.0;

/// Minimum word character count for overlap matching.
pub const MIN_WORD_CHARS: usize = 2;

/// Grid detection left/right tolerance in PDF points.
pub const GRID_TOLERANCE: f32 = 3.0;

/// Grid detection height tolerance in PDF points.
pub const GRID_HEIGHT_TOLERANCE: f32 = 3.0;

/// Grid detection top range (min, max) in PDF points.
pub const GRID_TOP_RANGE: (f32, f32) = (3.0, 100.0);

/// Grid height factor for grouping.
pub const GRID_HEIGHT_FACTOR: f32 = 1.5;

/// Minimum rows for grid detection.
pub const GRID_MIN_ROWS: usize = 2;

/// Checkbox vertical tolerance in PDF points.
pub const CHECKBOX_VERTICAL_TOLERANCE: f32 = 14.0;

/// Checkbox right-side maximum in PDF points.
pub const CHECKBOX_RIGHT_MAX: f32 = 180.0;

/// Checkbox left-side maximum in PDF points.
pub const CHECKBOX_LEFT_MAX: f32 = 300.0;

/// Checkbox above maximum in PDF points.
pub const CHECKBOX_ABOVE_MAX: f32 = 22.0;

/// Checkbox above maximum (alternate) in PDF points.
pub const CHECKBOX_ABOVE_MAX_ALT: f32 = 28.0;

/// Checkbox left minimum in PDF points.
pub const CHECKBOX_LEFT_MIN: f32 = 10.0;

/// Same-line vertical tolerance in PDF points.
pub const SAME_LINE_TOLERANCE: f32 = 5.0;

/// Direct label vertical tolerance in PDF points.
pub const DIRECT_LABEL_VERTICAL_TOLERANCE: f32 = 12.0;

/// Direct label left maximum in PDF points.
pub const DIRECT_LABEL_LEFT_MAX: f32 = 300.0;

/// Radio group top maximum in PDF points.
pub const RADIO_GROUP_TOP_MAX: f32 = 180.0;

/// Radio group top minimum in PDF points.
pub const RADIO_GROUP_TOP_MIN: f32 = 180.0;

/// Radio group left maximum in PDF points.
pub const RADIO_GROUP_LEFT_MAX: f32 = 30.0;

/// Radio group top difference threshold in PDF points.
pub const RADIO_GROUP_TOP_DIFF: f32 = 5.0;

/// Maximum options per form field.
pub const MAX_FIELD_OPTIONS: usize = 256;

/// Maximum suppressed fields list length.
pub const MAX_SUPPRESSED_FIELDS: usize = 2_000;

/// Maximum widgets per page preview.
pub const MAX_WIDGETS_PER_PAGE: usize = 512;

/// Maximum selection values parsed.
pub const MAX_SELECTION_VALUES: usize = 256;

/// Maximum decimals for form fields.
pub const MAX_DECIMALS: u8 = 12;

/// Default decimals for form fields.
pub const DEFAULT_DECIMALS: u8 = 2;

/// Default date format for form fields.
pub const DEFAULT_DATE_FORMAT: &str = "YYYY-MM-DD";

/// Maximum radio kids per group.
pub const MAX_RADIO_KIDS: usize = 2_000;

/// Maximum button group kids.
pub const MAX_BUTTON_KIDS: usize = 10_000;

/// Maximum fields created per operation.
pub const MAX_CREATE_FIELDS: usize = 2_000;

/// OCR source DPI for rendering pages.
pub const OCR_SOURCE_DPI: f32 = 300.0;

/// PDF standard DPI.
pub const PDF_STANDARD_DPI: f32 = 72.0;

/// Maximum total pixels for detection rendering.
pub const DETECTION_MAX_PIXELS: f64 = 18_000_000.0;

/// Maximum dimension for detection rendering.
pub const DETECTION_MAX_DIMENSION: f32 = 8192.0;

/// Minimum scale for detection rendering.
pub const DETECTION_MIN_SCALE: f32 = 0.5;

/// Maximum words per page during detection.
pub const MAX_WORDS: usize = 10_000;

/// Maximum value length for detected words.
pub const MAX_VALUE_LEN: usize = 4096;

/// Maximum drawings per page during detection.
pub const MAX_DRAWINGS: usize = 10_000;

/// Minimum curve count for circle detection.
pub const MIN_CURVE_COUNT: usize = 4;

/// Circle width range in PDF points.
pub const CIRCLE_WIDTH_RANGE: (f32, f32) = (5.0, 22.0);

/// Maximum width-height difference for circles.
pub const MAX_CIRCLE_DIFF: f32 = 3.0;

/// Maximum items per drawing.
pub const MAX_ITEMS_PER_DRAWING: usize = 1024;

/// Minimum line length in PDF points.
pub const MIN_LINE_LENGTH: f32 = 35.0;

/// Maximum scratch directory attempts.
pub const MAX_SCRATCH_ATTEMPTS: usize = 100;
