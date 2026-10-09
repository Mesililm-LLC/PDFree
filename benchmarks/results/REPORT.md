# Prototype qualification report — 2026-10-09

The Windows x64 portable prototype builds, launches, and renders real PDF content. It is **not yet a fully qualified viewer release**. The native engine remains provisional, with the 64 MiB native-store target unmet and physical presentation/manual interaction qualification outstanding.

## Recorded evidence

- Formatting, strict Clippy, and 16 tests passed: eight viewport/cache/queue contracts, one clipped-pixel reference comparison, and seven native integration tests. See [validation output](validation.txt).
- The optimized default and optional WGPU configurations built. Native upstream compiler/linker warnings are retained in [release build output](release-build.txt), rather than silently suppressed.
- The packaged executable launched successfully. [Imported DLLs](portable-dependencies.txt) include Windows system libraries and the packaged release VC runtime; libclang/Python are build-only dependencies. A clean second-machine installation has not been tested.
- Seven actual app smoke runs produced before/after navigation/zoom captures: independent W3C and Mozilla documents, transparency, dense vectors, raster images, mixed sizes, and 1,000 pages. See [smoke manifest](corpus-ui-smoke.json), [fixture hashes/provenance](../../tests/corpus/manifest.json), and [manual boundaries](../../docs/manual-validation.md). The raster fixture is synthetic, not a scanned real-world document.

## Warm measurements

See the reproducible [aggregate table](summary-table.md) and [full aggregate values](summary.json). Each renderer run uses 20 fresh processes with filesystem/driver caches left warm, identical office.pdf bytes, 125% DPI, and the same navigation/zoom schedule. The runner requests a **hidden startup window**, which may affect compositor/driver scheduling. This is not a visible-window responsiveness qualification. Raw per-trial data and machine/executable/document hashes are included in each JSON report; local monotonic traces are retained under benchmarks/runs.

The original default build recorded median open-to-first-tile publication of **1,027.6 ms with FemtoVG** and **117.2 ms with software**. Native tile render medians were approximately 1 ms. FemtoVG window-show requests took a median 1,266.4 ms for office.pdf and 1,000.6 ms for the empty window; software's office-window request median was 45.2 ms.

These measurements suggest that graphics/window initialization and UI event delivery dominate native tile rasterization in this environment. Hidden startup and instrumented trace flushing are potential confounders. The measured FemtoVG publication time already exceeds the 250 ms small-PDF goal; software's lower publication time does **not** prove first usable presentation meets that goal. Cold-window performance is unmeasured. No advantage over another reader is claimed.

The separate WGPU-enabled build recorded median first-tile publication of **1,446.0 ms**, p95 2,560.1 ms, and median peak working set 185.1 MiB. It did not improve the publication result in this setup. Its empty-window request median was 1,180.4 ms. WGPU uses a separately hashed executable and the same document/interactions; the 20 fresh processes include first-use pipeline initialization rather than discarding the first run.

For a reproducible software experiment, set `SLINT_BACKEND=winit-software` before launching. The portable default keeps the requested Winit/FemtoVG baseline with software fallback. Renderer selection should be revisited after visible-window frame/presentation profiling.

## Memory and cancellation

Application raster-cache accounting is hard-bounded at 128 MiB; observed raster accounting in the office schedule stayed near 40.1 MiB. Process working set/private memory are sampled separately and include more than that cache. Neither those counters nor the raster counter isolate native-store, Slint-image, or GPU residency. MuPDF 0.8.0's safe API fixes its shared native-store threshold at **256 MiB**, not the requested 64 MiB; it is not a total native-memory cap.

The replacement probe records acknowledgement after vector display-list preparation and shutdown join duration. Worker start is not observed, and active native calls finish. This measures command replacement/cleanup, not a hard native abort or user-visible cancellation presentation.

Across [20 release probe trials](cancellation.json), replacement acknowledgement p50/p95 was **1.16/10.43 ms**, and shutdown join p50/p95 was **0.64/18.46 ms**. See [the generated summary](cancellation-summary.json).

## Unavailable qualification

| Evidence | Status |
|---|---|
| Cold trial reboot-01 | Pending a separate fresh reboot |
| Cold trial reboot-02 | Pending a separate fresh reboot |
| Cold trial reboot-03 | Pending a separate fresh reboot |
| Cold trial reboot-04 | Pending a separate fresh reboot |
| Cold trial reboot-05 | Pending a separate fresh reboot |
| Edge comparison | Edge 154.0.4258.62 installed; comparable presentation/interaction timing unavailable |
| SumatraPDF comparison | Not found on PATH or standard install locations; comparison unavailable |
| Physical presentation / first usable page | Unverified; pixel publication and render callbacks are different milestones |
| Sustained scrolling frame-time percentiles | Unavailable; recorded frame gaps include idle periods |
| Full pointer/keyboard/multi-monitor DPI qualification | Pending manual checks |
| Broad real-world scan fidelity | Synthetic raster fixture exercised; independent scan corpus still needed |

No reboots were performed, and no warm run was relabeled cold. Next qualification work should address visible-window graphics overhead, presentation/frame timing, independent-reader comparisons, and native allocation accounting before adding editing. PDFium remains the next adapter comparison if MuPDF's unresolved limits prevent acceptance.
