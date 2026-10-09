# Third-party notices

PDFree includes components maintained by their respective authors. Cargo.lock records exact versions and checksums. This notice is not a replacement for upstream license texts.

- MuPDF / Artifex and mupdf-rs / contributors: AGPL-3.0. The Rust wrapper statically links native MuPDF components, whose bundled third-party source contains additional notices. Sources: https://github.com/ArtifexSoftware/mupdf and https://github.com/messense/mupdf-rs.
- Slint / SixtyFPS GmbH and contributors: the selected open-source license is GPL-3.0-only; upstream also offers other licensing options. Source: https://github.com/slint-ui/slint.
- rfd / contributors: MIT. Source: https://github.com/PolyMeilex/rfd.
- Rust transitive dependencies retain the license expressions in their package metadata. `scripts/package.ps1` exports the complete resolved package/license inventory and includes available dependency license and copyright files.
- Microsoft VC runtime files, when packaged, retain Microsoft's applicable runtime redistribution terms; they are not covered by PDFree's AGPL. The packaging script uses only the installed Visual Studio redistributable runtime directory.
- W3C dummy PDF and Mozilla PDF.js's tracemonkey compatibility sample retain original document copyrights. URLs and hashes are recorded in the corpus manifest. They are test inputs, not application content.
- libclang, CMake, and pypdf are development/test prerequisites; their local installations are excluded from the portable application.
