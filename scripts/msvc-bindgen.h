/* MSVC's C headers omit max_align_t; its C++ cstddef defines it as double.
 * mupdf 0.8.0 requires this type in its native-device alignment assertion.
 * An allowlisted alias ensures bindgen retains the otherwise unreferenced type.
 * This header affects binding generation only, not native source compilation. */
#if defined(_MSC_VER)
typedef double max_align_t;
typedef max_align_t mupdf_pdfree_max_align_t;
#endif
