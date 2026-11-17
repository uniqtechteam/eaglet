"""Draw docs/img/bench-axum.svg from bench/results.json (no plotting library needed).

    python3 bench/plot.py

Two panels with their own scales: requests per second over HTTP (higher is better), and the
framework's own cost per request in memory (lower is better).
"""
import json
import pathlib

root = pathlib.Path(__file__).resolve().parent.parent
data = json.loads((root / "bench" / "results.json").read_text())
rows, machine = data["summary"], data["machine"]

COLOURS = {"axum": "#2a78d6", "eaglet": "#eb6834"}
DARK = {"axum": "#3987e5", "eaglet": "#d95926"}
LABELS = {"axum": "plain Axum", "eaglet": "eaglet"}
bar_h, pair_gap, left, width = 18, 18, 190, 380


def panel(y: float, title: str, key: str, fmt) -> tuple[list[str], float]:
    longest = max(r[f"{f}_{key}"] for r in rows for f in COLOURS)
    scale = width / longest
    parts = [f'<text x="0" y="{y}" class="v" font-weight="600">{title}</text>']
    y += 14
    for r in rows:
        parts.append(f'<text x="0" y="{y + bar_h + 4}" class="v">{r["case"]}</text>')
        for framework in COLOURS:
            value = r[f"{framework}_{key}"]
            w = max(2.0, value * scale)
            parts.append(
                f'<rect class="{framework}" x="{left}" y="{y}" width="{w:.1f}" height="{bar_h - 2}" '
                f'rx="4"><title>{LABELS[framework]}, {r["case"]}: {fmt(value)}</title></rect>'
                f'<text x="{left + w + 6:.1f}" y="{y + bar_h - 5}" class="v">{fmt(value)}</text>'
            )
            y += bar_h
        y += pair_gap
    return parts, y


legend_y = 44
parts = [
    f'<style>text{{fill:#52514e}}.v{{fill:#0b0b0b}}'
    f'.axum{{fill:{COLOURS["axum"]}}}.eaglet{{fill:{COLOURS["eaglet"]}}}'
    f'@media (prefers-color-scheme: dark){{text{{fill:#c3c2b7}}.v{{fill:#f0efec}}'
    f'.axum{{fill:{DARK["axum"]}}}.eaglet{{fill:{DARK["eaglet"]}}}}}</style>',
    '<text x="0" y="18" class="v" font-size="15" font-weight="600">'
    "The same three routes in plain Axum and in eaglet</text>",
    f'<rect class="axum" x="0" y="{legend_y - 11}" width="12" height="12" rx="2"/>'
    f'<text x="18" y="{legend_y}">plain Axum</text>'
    f'<rect class="eaglet" x="110" y="{legend_y - 11}" width="12" height="12" rx="2"/>'
    f'<text x="128" y="{legend_y}">eaglet</text>',
]
top, y = panel(78, "Requests per second over HTTP (higher is better)", "rps", lambda v: f"{v:,.0f}")
parts += top
bottom, y = panel(
    y + 22, "Framework cost per request, in memory (lower is better)", "inproc_ns",
    lambda v: f"{v:,.0f} ns",
)
parts += bottom
note = (
    f'{machine["cpu"]}, {machine["os"]}, {machine["rustc"].split(" (")[0]}, {machine["oha"]}, '
    f'{machine["connections"]} connections, median of {machine["rounds"]} rounds, {machine["date"]}'
)
parts.append(f'<text x="0" y="{y + 10}" font-size="11">{note}</text>')
height = y + 22
svg = (
    f'<svg xmlns="http://www.w3.org/2000/svg" width="{left + width + 110}" height="{height}" '
    f'viewBox="0 0 {left + width + 110} {height}" font-family="system-ui, sans-serif" '
    f'font-size="13" role="img" aria-label="Requests per second and cost per request, plain Axum '
    f'against eaglet">\n' + "\n".join(parts) + "\n</svg>\n"
)
out = root / "docs" / "img" / "bench-axum.svg"
out.parent.mkdir(parents=True, exist_ok=True)
out.write_text(svg)
print(f"wrote {out.relative_to(root)}")
