"""Summarize recorded trials without treating publication as presentation."""
import json
import math
import statistics
from pathlib import Path

root = Path(__file__).resolve().parents[1]
reports = []
def metric(values):
    values = sorted(v for v in values if v is not None)
    return None if not values else {
        "p50": statistics.median(values),
        "p95": values[math.ceil(len(values) * .95) - 1],
        "max": max(values),
    }
for path in sorted((root / "benchmarks/results").glob("warm-*.json")):
    data = json.loads(path.read_text(encoding="utf-8-sig"))
    runs = data["runs"]
    reports.append({
        "report": path.name, "trials": len(runs),
        "executable_sha256": data["executable_sha256"],
        "window_show_request_ms": metric([r["window_show_request_ms"] for r in runs]),
        "open_to_first_tile_publication_ms": metric([r["open_to_pixels_ms"] for r in runs]),
        "tile_render_ms": metric([v for r in runs for v in r["tile_render_ms"]]),
        "peak_working_set_mib": metric([r["peak_working_set_bytes"] / 1048576 for r in runs]),
        "peak_private_mib": metric([r["peak_private_bytes"] / 1048576 for r in runs]),
        "max_raster_cache_mib": max(r["raster_cache_bytes"] for r in runs) / 1048576,
        "cache_hits": sum(r["cache_hits"] for r in runs),
        "cache_misses": sum(r["cache_misses"] for r in runs),
    })
(root / "benchmarks/results/summary.json").write_text(json.dumps(reports, indent=2) + "\n")
lines = ["| Run | N | Window request p50 / p95 ms | First tile p50 / p95 ms | Tile render p50 / p95 ms | Peak working set p50 MiB |", "|---|---:|---:|---:|---:|---:|"]
def pair(value):
    return "—" if value is None else f'{value["p50"]:.1f} / {value["p95"]:.1f}'
for report in reports:
    lines.append(f'| {report["report"]} | {report["trials"]} | {pair(report["window_show_request_ms"])} | {pair(report["open_to_first_tile_publication_ms"])} | {pair(report["tile_render_ms"])} | {report["peak_working_set_mib"]["p50"]:.1f} |')
(root / "benchmarks/results/summary-table.md").write_text("\n".join(lines) + "\n", encoding="utf-8")
print("\n".join(lines))
probe = root / "benchmarks/results/cancellation.json"
if probe.exists():
    data = json.loads(probe.read_text(encoding="utf-8-sig"))
    summary = {"trials": len(data["trials"]), "meaning": data["meaning"]}
    for name in ("replacement_ack_ms", "shutdown_join_ms"):
        summary[name] = metric([trial[name] for trial in data["trials"]])
    (probe.parent / "cancellation-summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary, indent=2))
