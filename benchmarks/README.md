# Reproducible measurements

See [recorded results](results/REPORT.md). The runner requests a hidden startup window to avoid disturbing desktop work; this may affect driver/compositor scheduling and is not a visible-window responsiveness qualification. All renderer comparisons below use that same request. UI pixel publication remains distinct from actual presentation.

The interaction schedule is relative to first tile publication, not process launch. Run `scripts/smoke-corpus.ps1` for functional captures, `scripts/benchmark-engine.ps1` for replacement/cleanup trials, and `python scripts/summarize-benchmarks.py` to regenerate aggregate tables. Do not run compilation concurrently with controlled timed trials.

Build `scripts/build.ps1 -Release` first, then run:

```powershell
./scripts/benchmark.ps1 -Trials 20 -Document tests/corpus/office.pdf -Output benchmarks/results/warm-office.json
./scripts/benchmark.ps1 -Empty -Trials 20 -Output benchmarks/results/warm-empty.json
```

Use identical executable, machine, renderer, window size/DPI, document bytes, and interaction schedule for comparisons. The built-in schedule opens a PDF, changes page at 600 ms, zooms at 1,000 ms, returns a page at 1,500 ms, and finishes after 2,200 ms. It exercises real UI callbacks and rendering but is not a sustained pointer-scroll workload. First pixels denotes image-model publication; actual presentation remains unverified. Main-entry clocks omit pre-main loader time. Trace flushing adds overhead, so results are instrumented runs.

The runner samples working set/private memory every 25 ms and preserves monotonic JSONL traces under ignored `benchmarks/runs/`. Summary reports include all per-trial render durations and cache counters, executable/document hashes, machine details, renderer request, and boot time. Renderer callbacks can be unavailable on software; frame gaps contain idle periods and must not be reported as scroll frame-time percentiles. Cache hits count internal lookups, not user navigations. Peak memory sampling can miss short spikes.

For software use `$env:SLINT_BACKEND='winit-software'`; for OpenGL FemtoVG use `winit-femtovg`. Build with `-Wgpu` and use `winit-femtovg-wgpu` for the optional WGPU comparison. Restore the environment variable after the experiment. Record requested renderer and any fallback in stderr.

Cold trials require five separate fresh reboots and one run per reboot:

```powershell
./scripts/benchmark.ps1 -Mode Cold -Trials 1 -ColdTrialId reboot-01 -Output benchmarks/results/cold-01.json
```

Repeat for reboot-02 through reboot-05, without opening the document or app first. Reboots are intentionally not automated. Never call an un-cleared cache cold. Post-reboot trials, independent-reader timing, and physical presentation measurements remain pending until explicitly collected.

For a visual check set `PDFREE_CAPTURE_DIR` to a local directory and launch `pdfree.exe document.pdf --benchmark-ui --trace trace.jsonl`. The app captures its own renderer output at the first-page and post-zoom stages; it does not capture the desktop or other apps.

Targets from the research are goals: cold window ≤300 ms p50, warm window ≤150 ms p50, small-PDF first usable page ≤250 ms p50. Request/publication timing cannot establish these presentation-based goals by itself.
