//! HTML rendition: a single, self-contained page (plotly.js is loaded from its CDN).
use maud::{html, Markup, PreEscaped, DOCTYPE};
use serde_json::{json, Value};

use crate::report::{
    sky::{Skyplot, Timeline},
    time_tag, Block, Chart, ChartKind, Heatmap, HeatmapFrame, Report, XAxis,
};

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
.chart.map { height:560px; }
.chart.globe { height:420px; max-width:520px; }
.chart.region { height:640px; }
.chart.skyplot { height:640px; max-width:820px; }
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

/// Extent of the points of a map: (lon min, lon max, lat min, lat max)
fn extent(chart: &Chart) -> (f64, f64, f64, f64) {
    let (mut lon_min, mut lon_max, mut lat_min, mut lat_max) =
        (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for series in chart.series.iter() {
        for (lon, lat) in series.x.iter().zip(series.y.iter()) {
            if lon.is_finite() && lat.is_finite() {
                lon_min = lon_min.min(*lon);
                lon_max = lon_max.max(*lon);
                lat_min = lat_min.min(*lat);
                lat_max = lat_max.max(*lat);
            }
        }
    }
    if lon_min > lon_max {
        return (0.0, 0.0, 0.0, 0.0);
    }
    (lon_min, lon_max, lat_min, lat_max)
}

/// Web Mercator zoom level (MapLibre: 512 px tiles) fitting this extent in the map frame
/// (≈ 760 × 480 px, color bar and legend excluded)
fn map_zoom(lon_min: f64, lon_max: f64, lat_min: f64, lat_max: f64) -> f64 {
    const WIDTH_PX: f64 = 760.0;
    const HEIGHT_PX: f64 = 480.0;
    let mercator = |lat: f64| {
        let lat = lat.clamp(-85.0, 85.0).to_radians();
        (std::f64::consts::FRAC_PI_4 + lat / 2.0).tan().ln()
    };
    let dlon = (lon_max - lon_min).max(1.0E-6);
    let dy = (mercator(lat_max) - mercator(lat_min)).max(1.0E-8);
    let zoom_lon = (360.0 * WIDTH_PX / 512.0 / dlon).log2();
    let zoom_lat = (2.0 * std::f64::consts::PI * HEIGHT_PX / 512.0 / dy).log2();
    // margin around the points
    (zoom_lon.min(zoom_lat) - 0.6).clamp(0.5, 18.0)
}

/// Sky plot: polar projection, north up, azimuth clockwise, zenith at the center
fn skyplot_json(skyplot: &Skyplot) -> Value {
    let mut traces = Vec::new();
    let mut groups = std::collections::BTreeSet::new();

    for track in skyplot.tracks.iter() {
        let (mut r, mut theta, mut text) = (Vec::new(), Vec::new(), Vec::new());
        let mut previous: Option<f64> = None;
        for (t, az, el) in track.points.iter() {
            if previous.is_some_and(|p| t - p > skyplot.gap_s) {
                r.push(Value::Null);
                theta.push(Value::Null);
                text.push(Value::Null);
            }
            r.push(json!(90.0 - el));
            theta.push(json!(az));
            text.push(json!(time_tag(skyplot.origin, *t)));
            previous = Some(*t);
        }

        let first_of_group = groups.insert(track.group.clone());

        traces.push(json!({
            "type": "scatterpolar",
            "name": track.label,
            "legendgroup": track.group,
            "legendgrouptitle": if first_of_group { json!({"text": track.group}) } else { Value::Null },
            "r": r,
            "theta": theta,
            "text": text,
            "mode": "lines",
            "line": {"color": track.color, "width": 2},
            "hovertemplate": format!("{} %{{text}}<br>azimuth %{{theta:.1f}}°, elevation %{{customdata:.1f}}°<extra></extra>", track.label),
            "customdata": track.points.iter().map(|(_, _, el)| json!(el)).collect::<Vec<_>>(),
        }));

        // label at the end of the track
        if let Some((_, az, el)) = track.points.last() {
            traces.push(json!({
                "type": "scatterpolar",
                "r": [90.0 - el],
                "theta": [az],
                "mode": "markers+text",
                "text": [track.label],
                "textposition": "top center",
                "textfont": {"size": 10, "color": track.color},
                "marker": {"size": 6, "color": track.color},
                "legendgroup": track.group,
                "showlegend": false,
                "hoverinfo": "skip",
            }));
        }
    }

    let layout = json!({
        "title": {"text": skyplot.title, "font": {"size": 15}},
        "margin": {"l": 40, "r": 40, "t": 60, "b": 30},
        "legend": {"groupclick": "toggleitem"},
        "polar": {
            "angularaxis": {
                "rotation": 90,
                "direction": "clockwise",
                "tickmode": "array",
                "tickvals": [0, 45, 90, 135, 180, 225, 270, 315],
                "ticktext": ["N", "NE", "E", "SE", "S", "SW", "W", "NW"],
            },
            "radialaxis": {
                "range": [0, 90],
                "tickmode": "array",
                "tickvals": [0, 15, 30, 45, 60, 75, 90],
                "ticktext": ["90°", "75°", "60°", "45°", "30°", "15°", "0°"],
                "angle": 90,
                "tickangle": 90,
            },
        },
    });

    json!({ "data": traces, "layout": layout })
}

/// Visibility timeline: one row per satellite (top: first satellite)
fn timeline_json(timeline: &Timeline) -> Value {
    let mut traces = Vec::new();
    let mut groups = std::collections::BTreeSet::new();

    for row in timeline.rows.iter() {
        let (mut x, mut y) = (Vec::new(), Vec::new());
        for (start, end) in row.intervals.iter() {
            x.push(json!(time_tag(timeline.origin, *start)));
            x.push(json!(time_tag(timeline.origin, *end)));
            x.push(Value::Null);
            y.push(json!(row.label));
            y.push(json!(row.label));
            y.push(Value::Null);
        }
        let first_of_group = groups.insert(row.group.clone());
        traces.push(json!({
            "type": "scatter",
            "name": row.label,
            "legendgroup": row.group,
            "legendgrouptitle": if first_of_group { json!({"text": row.group}) } else { Value::Null },
            "showlegend": false,
            "x": x,
            "y": y,
            "mode": "lines",
            "line": {"color": row.color, "width": 7},
            "hovertemplate": format!("{} %{{x}}<extra></extra>", row.label),
        }));
    }

    let labels = timeline
        .rows
        .iter()
        .map(|row| json!(row.label))
        .rev()
        .collect::<Vec<_>>();

    let height = (18 * timeline.rows.len() + 140).max(300);

    let layout = json!({
        "title": {"text": timeline.title, "font": {"size": 15}},
        "height": height,
        "margin": {"l": 60, "r": 20, "t": 48, "b": 48},
        "hovermode": "closest",
        "xaxis": {"title": {"text": format!("{}", timeline.origin.time_scale)}},
        "yaxis": {
            "type": "category",
            "categoryorder": "array",
            "categoryarray": labels,
            "tickfont": {"size": 10},
            "automargin": true,
        },
    });

    json!({ "data": traces, "layout": layout })
}

fn chart_json(chart: &Chart) -> Value {
    // series colored by value: one shared color scale
    let first_colored = chart
        .series
        .iter()
        .position(|s| s.colors.is_some() && !s.anchor);
    let color_range = chart
        .series
        .iter()
        .filter_map(|s| s.colors.as_ref())
        .flatten()
        .filter(|v| v.is_finite())
        .fold((f64::MAX, f64::MIN), |(a, b), v| (a.min(*v), b.max(*v)));

    let traces = chart
        .series
        .iter()
        .enumerate()
        .map(|(index, series)| {
            let series = series.decimated(MAX_POINTS);

            match chart.kind {
                ChartKind::Map | ChartKind::Globe | ChartKind::Region => {
                    let kind = if chart.kind == ChartKind::Map {
                        "scattermap"
                    } else {
                        "scattergeo"
                    };
                    let mut trace = json!({
                        "type": kind,
                        "name": series.name,
                        "lon": series.x,
                        "lat": series.y,
                        "mode": if series.connected { "lines+markers" } else { "markers" },
                        "marker": {"size": if series.connected { 9 } else { 6 }},
                        "line": {"width": 3},
                    });
                    if series.anchor {
                        // stations: large marker, labeled
                        trace["mode"] = json!("markers+text");
                        trace["marker"] = json!({"size": 16, "color": "#d62728"});
                        trace["text"] = json!(vec![series.name.clone(); series.x.len()]);
                        trace["textposition"] = json!("top right");
                        trace["textfont"] = json!({"size": 13, "color": "#111"});
                    } else if let Some(color) = &series.color {
                        // track: solid line, labeled at its last point
                        let last = series
                            .x
                            .iter()
                            .zip(series.y.iter())
                            .rposition(|(x, y)| x.is_finite() && y.is_finite());
                        let text = (0..series.x.len())
                            .map(|i| {
                                if Some(i) == last {
                                    series.name.clone()
                                } else {
                                    String::new()
                                }
                            })
                            .collect::<Vec<_>>();
                        trace["mode"] = json!("lines+text");
                        trace["line"] = json!({"width": 2, "color": color});
                        trace["text"] = json!(text);
                        trace["textposition"] = json!("top right");
                        trace["textfont"] = json!({"size": 10, "color": color});
                    } else if let Some(colors) = &series.colors {
                        // one color scale for all the series, presented once
                        let first = Some(index) == first_colored;
                        trace["marker"] = json!({
                            "size": 7,
                            "color": colors,
                            "colorscale": "Viridis",
                            "cmin": color_range.0,
                            "cmax": color_range.1,
                            "showscale": first,
                            "colorbar": {
                                "title": {"text": chart.color_label.clone().unwrap_or_default()},
                                "thickness": 14,
                            },
                        });
                    }
                    trace
                },
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
                    let mode = if series.connected {
                        "lines+markers"
                    } else if chart.kind == ChartKind::Lines {
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

    if matches!(
        chart.kind,
        ChartKind::Map | ChartKind::Globe | ChartKind::Region
    ) {
        let (lon_min, lon_max, lat_min, lat_max) = extent(chart);
        let center = json!({"lon": (lon_min + lon_max) / 2.0, "lat": (lat_min + lat_max) / 2.0});

        if chart.kind == ChartKind::Map {
            layout["map"] = json!({
                "style": "open-street-map",
                "center": center,
                "zoom": map_zoom(lon_min, lon_max, lat_min, lat_max),
            });
            layout["legend"] = json!({"orientation": "h", "y": -0.02});
        } else if chart.kind == ChartKind::Region {
            // azimuthal projection centered on the points, fitted on them
            let (lon0, lat0) = crate::report::geographic_center(
                chart
                    .series
                    .iter()
                    .flat_map(|s| s.x.iter().copied().zip(s.y.iter().copied())),
            )
            .unwrap_or_default();
            layout["geo"] = json!({
                "projection": {
                    "type": "azimuthal equidistant",
                    "rotation": {"lon": lon0, "lat": lat0},
                },
                "fitbounds": "locations",
                "resolution": 50,
                "showland": true,
                "landcolor": "#e3e7d3",
                "showocean": true,
                "oceancolor": "#cfe3f3",
                "showlakes": true,
                "lakecolor": "#cfe3f3",
                "showcountries": true,
                "countrycolor": "#9aa5b1",
                "showcoastlines": true,
                "coastlinecolor": "#6b7a89",
                "lataxis": {"showgrid": true, "gridcolor": "#bcc6d0", "dtick": 5},
                "lonaxis": {"showgrid": true, "gridcolor": "#bcc6d0", "dtick": 10},
            });
            layout["legend"] = json!({"orientation": "v"});
        } else {
            layout["geo"] = json!({
                "projection": {
                    "type": "orthographic",
                    "rotation": {"lon": center["lon"], "lat": center["lat"]},
                },
                "showland": true,
                "landcolor": "#e3e7d3",
                "showocean": true,
                "oceancolor": "#cfe3f3",
                "showcountries": true,
                "countrycolor": "#9aa5b1",
                "showcoastlines": true,
                "coastlinecolor": "#6b7a89",
                "lataxis": {"showgrid": true, "gridcolor": "#bcc6d0", "dtick": 30},
                "lonaxis": {"showgrid": true, "gridcolor": "#bcc6d0", "dtick": 30},
            });
        }
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
        "opacity": if heatmap.geographic { 0.75 } else { 1.0 },
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

    // geographic grid: world map behind the colors, coast lines above
    if heatmap.geographic {
        let (fill, coast) = crate::report::geo::land_paths(|lon, lat| (lon, lat));
        layout["plot_bgcolor"] = json!("#cfe3f3");
        layout["shapes"] = json!([
            {
                "type": "path",
                "path": fill,
                "xref": "x",
                "yref": "y",
                "layer": "below",
                "fillcolor": "#e3e7d3",
                "line": {"width": 0},
            },
            {
                "type": "path",
                "path": coast,
                "xref": "x",
                "yref": "y",
                "layer": "above",
                "line": {"color": "rgba(29, 39, 51, 0.75)", "width": 1},
            },
        ]);
        // the cells are centered on the grid nodes
        let range = |v: &[f64], limit: f64| match (v.first(), v.last()) {
            (Some(first), Some(last)) if v.len() > 1 => {
                let half = (last - first) / (v.len() - 1) as f64 / 2.0;
                json!([(first - half).max(-limit), (last + half).min(limit)])
            },
            _ => Value::Null,
        };
        layout["xaxis"]["range"] = range(&heatmap.x, 180.0);
        layout["yaxis"]["range"] = range(&heatmap.y, 90.0);
        // equal scales: the frame shrinks, not the extent
        layout["xaxis"]["constrain"] = json!("domain");
        layout["yaxis"]["constrain"] = json!("domain");
        layout["xaxis"]["showgrid"] = json!(false);
        layout["yaxis"]["showgrid"] = json!(false);
    }

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
                                div id=(id) class={ "chart" @if chart.kind == ChartKind::Map { " map" } @if chart.kind == ChartKind::Globe { " globe" } @if chart.kind == ChartKind::Region { " region" } } {}
                            }
                        },
                        Block::Skyplot(skyplot) => {
                            @let id = {
                                chart_index += 1;
                                format!("chart-{}", chart_index)
                            };
                            @let _ = charts.insert(id.clone(), skyplot_json(skyplot));
                            div id=(id) class="chart skyplot" {}
                        },
                        Block::Timeline(timeline) => {
                            @let id = {
                                chart_index += 1;
                                format!("chart-{}", chart_index)
                            };
                            @let _ = charts.insert(id.clone(), timeline_json(timeline));
                            @let height = format!("height: {}px", (18 * timeline.rows.len() + 140).max(300));
                            div id=(id) class="chart timeline" style=(height) {}
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
