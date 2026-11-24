//! HTML rendition: a single, self-contained page (plotly.js is loaded from its CDN).
use maud::{html, Markup, PreEscaped, DOCTYPE};
use serde_json::{json, Value};

use crate::report::{time_tag, Block, Chart, ChartKind, Heatmap, HeatmapFrame, Report, XAxis};

/// Maximal number of points per series
const MAX_POINTS: usize = 2000;

const PLOTLY_JS: &str = "https://cdn.plot.ly/plotly-2.35.2.min.js";

const STYLE: &str = r#"
:root { --fg:#1d2733; --muted:#5b6b7c; --line:#dfe5ec; --bg:#f6f8fa; --accent:#1f6feb; --warn:#9a6700; --warnbg:#fff8c5; }
* { box-sizing: border-box; }
body { margin:0; font-family: -apple-system, "Segoe UI", Roboto, Helvetica, Arial, sans-serif; color:var(--fg); background:#fff; font-size:14px; line-height:1.45; }
header { padding:20px 32px; border-bottom:1px solid var(--line); background:var(--bg); }
header h1 { margin:0; font-size:22px; }
header .meta { color:var(--muted); margin-top:4px; }
.layout { display:flex; align-items:flex-start; }
nav { position:sticky; top:0; width:240px; flex-shrink:0; padding:16px; max-height:100vh; overflow-y:auto; border-right:1px solid var(--line); }
nav a { display:block; padding:4px 8px; color:var(--fg); text-decoration:none; border-radius:4px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
nav a:hover { background:var(--bg); color:var(--accent); }
main { flex:1; min-width:0; padding:8px 32px 48px; }
section { padding-top:12px; border-bottom:1px solid var(--line); padding-bottom:16px; }
h2 { font-size:19px; margin:16px 0 8px; }
h3 { font-size:15px; margin:18px 0 6px; color:var(--muted); }
table { border-collapse:collapse; margin:8px 0; font-size:13px; }
th, td { border:1px solid var(--line); padding:4px 8px; text-align:left; vertical-align:top; }
th { background:var(--bg); font-weight:600; position:sticky; top:0; }
.table-wrap { max-height:480px; overflow:auto; display:inline-block; max-width:100%; }
table.kv th { width:220px; position:static; }
.notice { background:var(--warnbg); color:var(--warn); padding:8px 12px; border-radius:6px; margin:8px 0; }
.chart { width:100%; height:440px; margin:8px 0; }
.chart.map { height:520px; }
.chart.heatmap { height:620px; }
footer { color:var(--muted); padding:16px 32px; }
@media print { nav { display:none; } .table-wrap { max-height:none; overflow:visible; } }
"#;

/// Lazy chart rendition: charts are drawn when they scroll into view
const SCRIPT: &str = r#"
function draw(el) {
  if (el.dataset.drawn) return;
  el.dataset.drawn = "1";
  const chart = CHARTS[el.id];
  Plotly.newPlot(el, chart.data, chart.layout, {responsive: true, displaylogo: false});
}
const observer = new IntersectionObserver((entries) => {
  entries.forEach((entry) => { if (entry.isIntersecting) draw(entry.target); });
}, {rootMargin: "400px"});
document.querySelectorAll(".chart").forEach((el) => observer.observe(el));
"#;

/// Inserts a null point (NaN) in the gaps larger than 10 typical steps
fn break_gaps(x: &[f64], y: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let mut steps = x
        .windows(2)
        .map(|w| w[1] - w[0])
        .filter(|dx| *dx > 0.0)
        .collect::<Vec<_>>();
    steps.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let typical = steps.get(steps.len() / 2).copied().unwrap_or(f64::MAX);

    let (mut xs, mut ys) = (Vec::with_capacity(x.len()), Vec::with_capacity(y.len()));
    for i in 0..x.len() {
        if i > 0 && x[i] - x[i - 1] > 10.0 * typical {
            xs.push((x[i] + x[i - 1]) / 2.0);
            ys.push(f64::NAN);
        }
        xs.push(x[i]);
        ys.push(y[i]);
    }
    (xs, ys)
}

fn chart_json(chart: &Chart) -> Value {
    let traces = chart
        .series
        .iter()
        .map(|series| {
            let series = series.decimated(MAX_POINTS);

            match chart.kind {
                ChartKind::Map => json!({
                    "type": "scattermap",
                    "name": series.name,
                    "lon": series.x,
                    "lat": series.y,
                    "mode": "markers",
                    "marker": {"size": 5},
                }),
                _ => {
                    // lines are interrupted over data gaps
                    let (xs, ys) = if chart.kind == ChartKind::Lines {
                        break_gaps(&series.x, &series.y)
                    } else {
                        (series.x.clone(), series.y.clone())
                    };

                    let x: Value = match chart.x_axis {
                        XAxis::Time(origin) => xs
                            .iter()
                            .map(|x| Value::String(time_tag(origin, *x)))
                            .collect(),
                        XAxis::Linear => json!(xs),
                    };
                    let mode = if chart.kind == ChartKind::Lines {
                        "lines"
                    } else {
                        "markers"
                    };
                    // SVG traces: browsers limit the number of WebGL contexts per page
                    json!({
                        "type": "scatter",
                        "name": series.name,
                        "x": x,
                        "y": ys,
                        "mode": mode,
                        "marker": {"size": 4},
                        "line": {"width": 1.5},
                    })
                },
            }
        })
        .collect::<Vec<_>>();

    let mut layout = json!({
        "title": {"text": chart.title, "font": {"size": 15}},
        "margin": {"l": 60, "r": 20, "t": 48, "b": 48},
        "legend": {"orientation": "v"},
        "hovermode": "closest",
    });

    if chart.kind == ChartKind::Map {
        // center and zoom from the extent of the points
        let (mut lon_min, mut lon_max, mut lat_min, mut lat_max) =
            (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
        for series in chart.series.iter() {
            for (lon, lat) in series.x.iter().zip(series.y.iter()) {
                lon_min = lon_min.min(*lon);
                lon_max = lon_max.max(*lon);
                lat_min = lat_min.min(*lat);
                lat_max = lat_max.max(*lat);
            }
        }
        let extent = (lon_max - lon_min).max(lat_max - lat_min).max(1.0E-5);
        let zoom = (360.0 / extent).log2().clamp(1.0, 19.0) - 1.0;
        layout["map"] = json!({
            "style": "open-street-map",
            "center": {"lon": (lon_min + lon_max) / 2.0, "lat": (lat_min + lat_max) / 2.0},
            "zoom": zoom,
        });
    } else {
        layout["xaxis"] = json!({"title": {"text": chart.x_label}});
        layout["yaxis"] = json!({"title": {"text": chart.y_label}});
        if chart.equal_axes {
            layout["yaxis"]["scaleanchor"] = json!("x");
        }
    }

    json!({ "data": traces, "layout": layout })
}

/// Heatmap: one trace, frames selected with a slider (same color scale)
fn heatmap_json(heatmap: &Heatmap) -> Value {
    let z = |frame: &HeatmapFrame| -> Value {
        frame
            .z
            .iter()
            .map(|row| {
                row.iter()
                    .map(|v| if v.is_finite() { json!(v) } else { Value::Null })
                    .collect::<Value>()
            })
            .collect()
    };

    let (zmin, zmax) = heatmap
        .frames
        .iter()
        .flat_map(|f| f.z.iter().flatten())
        .copied()
        .filter(|v| v.is_finite())
        .fold((f64::MAX, f64::MIN), |(a, b), v| (a.min(v), b.max(v)));

    let first = heatmap.frames.first();

    let trace = json!({
        "type": "heatmap",
        "x": heatmap.x,
        "y": heatmap.y,
        "z": first.map(z).unwrap_or(Value::Null),
        "zmin": zmin,
        "zmax": zmax,
        "colorscale": "Viridis",
        "colorbar": {"title": {"text": heatmap.z_label}},
        "hovertemplate": "%{x}°, %{y}°: %{z:.1f}<extra></extra>",
    });

    let steps = heatmap
        .frames
        .iter()
        .map(|frame| {
            json!({
                "label": frame.label,
                "method": "restyle",
                "args": [{"z": [z(frame)]}],
            })
        })
        .collect::<Vec<_>>();

    let mut layout = json!({
        "title": {"text": heatmap.title, "font": {"size": 15}},
        "margin": {"l": 60, "r": 20, "t": 48, "b": 48},
        "xaxis": {"title": {"text": heatmap.x_label}},
        "yaxis": {"title": {"text": heatmap.y_label}, "scaleanchor": "x", "scaleratio": 1},
    });

    if heatmap.frames.len() > 1 {
        layout["sliders"] = json!([{
            "active": 0,
            "currentvalue": {"prefix": "Map: "},
            "pad": {"t": 48},
            "steps": steps,
        }]);
        layout["margin"]["b"] = json!(110);
    }

    json!({ "data": [trace], "layout": layout })
}

fn table(headers: &[String], rows: &[Vec<String>]) -> Markup {
    html! {
        div class="table-wrap" {
            table {
                thead { tr { @for h in headers { th { (h) } } } }
                tbody {
                    @for row in rows {
                        tr { @for cell in row { td { (cell) } } }
                    }
                }
            }
        }
    }
}

pub fn render(report: &Report) -> String {
    let mut charts = serde_json::Map::new();
    let mut chart_index = 0;

    let body = html! {
        @for section in report.sections.iter() {
            section id=(section.id) {
                h2 { (section.title) }
                @for block in section.blocks.iter() {
                    @match block {
                        Block::Heading(text) => h3 { (text) },
                        Block::Text(text) => p { (text) },
                        Block::Notice(text) => div class="notice" { (text) },
                        Block::KeyValue(pairs) => {
                            table class="kv" {
                                @for (key, value) in pairs {
                                    tr { th { (key) } td { (value) } }
                                }
                            }
                        },
                        Block::Table(t) => (table(&t.headers, &t.rows)),
                        Block::Chart(chart) => {
                            @if !chart.is_empty() {
                                @let id = {
                                    chart_index += 1;
                                    format!("chart-{}", chart_index)
                                };
                                @let _ = charts.insert(id.clone(), chart_json(chart));
                                div id=(id) class={ "chart" @if chart.kind == ChartKind::Map { " map" } } {}
                            }
                        },
                        Block::Heatmap(heatmap) => {
                            @if !heatmap.frames.is_empty() {
                                @let id = {
                                    chart_index += 1;
                                    format!("chart-{}", chart_index)
                                };
                                @let _ = charts.insert(id.clone(), heatmap_json(heatmap));
                                div id=(id) class="chart heatmap" {}
                            }
                        },
                    }
                }
            }
        }
    };

    // the charts are embedded in a script element
    let charts = Value::Object(charts).to_string().replace("</", "<\\/");

    let page = html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (report.title) }
                style { (PreEscaped(STYLE)) }
                script src=(PLOTLY_JS) charset="utf-8" {}
            }
            body {
                header {
                    h1 { (report.title) }
                    div class="meta" { (report.subtitle) " · generated " (report.generated) }
                }
                div class="layout" {
                    nav {
                        @for section in report.sections.iter() {
                            a href={ "#" (section.id) } title=(section.title) { (section.title) }
                        }
                    }
                    main { (body) }
                }
                footer {
                    "rinex-cli v" (env!("CARGO_PKG_VERSION")) ", powered by GNSS-Qc (NAV-Solutions framework)"
                }
                script { (PreEscaped(format!("const CHARTS = {};", charts))) (PreEscaped(SCRIPT)) }
            }
        }
    };

    page.into_string()
}
