# Implementation log

2026-10-09: User explicitly approved implementation of the full prototype plan.
Ruling: implement directly in the requested empty workspace and initialize local Git; no existing checkout/worktree requires isolation.
Ruling: the supplied approved plan is the binding spec; do not repeat approval handoffs from generic skills.
Ruling: post-reboot trials cannot be performed autonomously without disrupting the user's machine. Supply a cold-trial procedure and mark results pending rather than rebooting or relabeling warm trials.
Pre-flight: viewer-core owns geometry/request contracts; pdf-engine consumes these and returns plain pixel buffers; pdfree consumes both and owns Slint images.
Task 1: workspace, pinned dependencies and lockfile created; local libclang 18.1.1 and CMake 3.31.6 installed.
Task 2: six core contracts observed failing before implementation, then all six passed.
Ruling: mupdf-sys 0.8.0 misdetects Visual Studio 18 as v142. Set its supported MUPDF_MSVC_PLATFORM_TOOLSET override to the installed v145 toolset rather than installing an obsolete compiler.
Ruling: published mupdf 0.8.0 fixes the shared native store at 256 MiB and exposes no public setter. Keep safe published bindings and report this deviation from the 64 MiB target explicitly; application raster cache remains hard bounded. Native store is not a total process-memory cap.
Native build finding: mupdf 0.8.0 references max_align_t, missing from generated MSVC C bindings. Installed MSVC cstddef defines it as double. Supply a binding-generation-only header with that same definition and an allowlisted alias, without changing dependency source or bypassing the alignment check.
Task 3: five native integration tests passed, including real clipped rendering, independent embedded fonts, encrypted detection, malformed recovery, and repeated thousand-page opens.
Task 4: debug executable built and rendered an actual PDF; first app-only screenshots inspected. Fixed light-shell/dark-system control contrast, page-entry overwriting while typing, and benchmark interactions beginning too early on slow first launch.
Reading-anchor regression: observed missing-method failure, implemented page-relative anchoring, all seven viewer-core tests then passed.
Ruling: do not invalidate in-flight page pixels solely because distant page geometry changed; keys already encode raster dimensions and viewport changes still advance generation.
Final validation: formatting, strict Clippy, and all 16 tests passed. Default and WGPU optimized builds completed; native upstream warnings are recorded. Portable default executable launched successfully with packaged release VC runtimes and native license notices.
Performance evidence: 20 warm office trials per FemtoVG/software/WGPU renderer, 20 empty-window trials for FemtoVG and WGPU, and 20 release engine replacement/cleanup probes collected. Timing semantics and outstanding cold/presentation/comparative qualification are recorded in benchmarks/results/REPORT.md.
Fresh review: fixed empty benchmark completion, persistent render error reporting/retry suppression, external document report paths, and PowerShell argument-array concatenation. A focused tooling review also required fresh per-run screenshot directories and validation of both current captures to prevent stale-image false passes.
Preservation: all eleven corpus hashes verified; staged PDFs and the original research match their working bytes exactly. PDF files are explicitly binary in Git to preserve cross-reference offsets on Windows checkouts.
