//! Static charts (SVG), for the PDF rendition
use std::fmt::Write;

use crate::report::{time_tag, Chart, ChartKind, Heatmap, HeatmapFrame, XAxis};

const WIDTH: f64 = 720.0;
const HEIGHT: f64 = 340.0;
const MARGIN_LEFT: f64 = 72.0;
const MARGIN_RIGHT: f64 = 16.0;
const MARGIN_TOP: f64 = 16.0;
const MARGIN_BOTTOM: f64 = 44.0;

/// Maximal number of points per series
const MAX_POINTS: usize = 1500;

/// Series are listed in a legend up to this number
const MAX_LEGEND: usize = 12;

const PALETTE: [&str; 10] = [
    "#1f77b4", "#d62728", "#2ca02c", "#ff7f0e", "#9467bd", "#8c564b", "#e377c2", "#7f7f7f",
    "#bcbd22", "#17becf",
];

const FONT: &str = "DejaVu Sans Mono";

/// "Nice" tick values covering [min, max]
fn ticks(min: f64, max: f64, target: usize) -> Vec<f64> {
    let span = max - min;
    if !(span.is_finite()) || span <= 0.0 {
        return vec![min];
    }
    let raw = span / target as f64;
    let magnitude = 10f64.powf(raw.log10().floor());
    let step = [1.0, 2.0, 2.5, 5.0, 10.0]
        .iter()
        .map(|m| m * magnitude)
        .find(|step| span / step <= target as f64)
        .unwrap_or(10.0 * magnitude);
    let first = (min / step).ceil() * step;
    let mut values = Vec::new();
    let mut v = first;
    while v <= max + step * 1e-9 {
        values.push(if v.abs() < step * 1e-9 { 0.0 } else { v });
        v += step;
    }
    values
}

/// Time ticks: multiples of usual durations
fn time_ticks(min: f64, max: f64) -> Vec<f64> {
    let span = max - min;
    let step = [
        1.0, 5.0, 10.0, 30.0, 60.0, 300.0, 600.0, 1800.0, 3600.0, 7200.0, 10800.0, 21600.0,
        43200.0, 86400.0, 172800.0, 604800.0,
    ]
    .iter()
    .copied()
    .find(|step| span / step <= 8.0)
    .unwrap_or(604800.0);
    let mut values = Vec::new();
    let mut v = (min / step).ceil() * step;
    while v <= max {
        values.push(v);
        v += step;
    }
    values
}

fn format_tick(value: f64, step: f64) -> String {
    if value == 0.0 {
        return "0".to_string();
    }
    let magnitude = value.abs().max(step.abs());
    if !(1e-3..1e6).contains(&magnitude) {
        format!("{:.1e}", value)
    } else {
        let decimals = if step >= 1.0 {
            0
        } else {
            (-step.log10().floor()) as usize
        };
        format!("{:.*}", decimals.min(6), value)
    }
}

/// Coast line of a land ring: the ring outline, without its edges along the
/// antimeridian (where Natural Earth splits the land masses).
/// `project` returns the SVG coordinates of the visible points.
fn coast_path(ring: &[(f64, f64)], project: impl Fn(f64, f64) -> Option<(f64, f64)>) -> String {
    let on_antimeridian = |lon: f64| lon.abs() >= 179.99;
    let mut path = String::new();
    let mut previous: Option<f64> = None;
    for (lon, lat) in ring.iter().chain(ring.first()) {
        let cut = previous.is_some_and(|prev| on_antimeridian(prev) && on_antimeridian(*lon));
        match project(*lon, *lat) {
            Some((x, y)) => {
                let pen = previous.is_some() && !cut;
                let _ = write!(path, "{}{:.1},{:.1}", if pen { 'L' } else { 'M' }, x, y);
                previous = Some(*lon);
            },
            None => previous = None,
        }
    }
    path
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Renders a chart as SVG
pub fn render(chart: &Chart) -> String {
    match chart.kind {
        ChartKind::Map | ChartKind::Region => return render_map(chart),
        ChartKind::Globe => return render_globe(chart),
        _ => {},
    }

    let series = chart
        .series
        .iter()
        .map(|s| s.decimated(MAX_POINTS))
        .collect::<Vec<_>>();

    let finite = |v: &f64| v.is_finite();

    let mut x_min = series
        .iter()
        .flat_map(|s| s.x.iter())
        .copied()
        .filter(finite)
        .fold(f64::MAX, f64::min);
    let mut x_max = series
        .iter()
        .flat_map(|s| s.x.iter())
        .copied()
        .filter(finite)
        .fold(f64::MIN, f64::max);
    let mut y_min = series
        .iter()
        .flat_map(|s| s.y.iter())
        .copied()
        .filter(finite)
        .fold(f64::MAX, f64::min);
    let mut y_max = series
        .iter()
        .flat_map(|s| s.y.iter())
        .copied()
        .filter(finite)
        .fold(f64::MIN, f64::max);

    if x_min > x_max || y_min > y_max {
        return format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="40"></svg>"#,
            WIDTH
        );
    }

    // margins around the data (time axes are not padded)
    let pad_x = !matches!(chart.x_axis, XAxis::Time(_));
    for (min, max, pad) in [
        (&mut x_min, &mut x_max, pad_x),
        (&mut y_min, &mut y_max, true),
    ] {
        if (*max - *min).abs() < 1e-12 {
            *min -= 0.5_f64.max(min.abs() * 0.05);
            *max += 0.5_f64.max(max.abs() * 0.05);
        } else if pad {
            let margin = (*max - *min) * 0.04;
            *min -= margin;
            *max += margin;
        }
    }

    let plot_w = WIDTH - MARGIN_LEFT - MARGIN_RIGHT;
    let plot_h = HEIGHT - MARGIN_TOP - MARGIN_BOTTOM;

    let equal = chart.equal_axes || matches!(chart.kind, ChartKind::Map | ChartKind::Region);
    if equal {
        // same scale on both axes
        let (sx, sy) = ((x_max - x_min) / plot_w, (y_max - y_min) / plot_h);
        let scale = sx.max(sy);
        let (cx, cy) = ((x_min + x_max) / 2.0, (y_min + y_max) / 2.0);
        x_min = cx - scale * plot_w / 2.0;
        x_max = cx + scale * plot_w / 2.0;
        y_min = cy - scale * plot_h / 2.0;
        y_max = cy + scale * plot_h / 2.0;
    }

    let px = |x: f64| MARGIN_LEFT + (x - x_min) / (x_max - x_min) * plot_w;
    let py = |y: f64| MARGIN_TOP + plot_h - (y - y_min) / (y_max - y_min) * plot_h;

    // legend, below the chart: (x, row) of each entry
    let mut legend = Vec::<(f64, usize)>::new();
    if series.len() > 1 && series.len() <= MAX_LEGEND {
        let (mut x, mut row) = (MARGIN_LEFT, 0);
        for s in series.iter() {
            let width = 22.0 + 6.1 * s.name.chars().count() as f64;
            if x + width > WIDTH - MARGIN_RIGHT && x > MARGIN_LEFT {
                x = MARGIN_LEFT;
                row += 1;
            }
            legend.push((x, row));
            x += width;
        }
    }
    let legend_rows = legend.last().map(|(_, row)| row + 1).unwrap_or(0);
    let total_height = HEIGHT + 14.0 * legend_rows as f64;

    let mut svg = String::with_capacity(64 * 1024);
    let _ = write!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" font-family="{f}" font-size="10">"#,
        w = WIDTH,
        h = total_height,
        f = FONT
    );

    // frame
    let _ = write!(
        svg,
        r##"<rect x="{}" y="{}" width="{}" height="{}" fill="none" stroke="#9aa5b1" stroke-width="0.8"/>"##,
        MARGIN_LEFT, MARGIN_TOP, plot_w, plot_h
    );

    // y ticks and grid
    let y_ticks = ticks(y_min, y_max, 6);
    let y_step = if y_ticks.len() > 1 {
        y_ticks[1] - y_ticks[0]
    } else {
        1.0
    };
    for v in y_ticks.iter() {
        let y = py(*v);
        let _ = write!(
            svg,
            r##"<line x1="{x0}" y1="{y}" x2="{x1}" y2="{y}" stroke="#e5e9ee" stroke-width="0.6"/><text x="{tx}" y="{ty}" text-anchor="end" fill="#3d4b59">{label}</text>"##,
            x0 = MARGIN_LEFT,
            x1 = MARGIN_LEFT + plot_w,
            y = y,
            tx = MARGIN_LEFT - 5.0,
            ty = y + 3.5,
            label = format_tick(*v, y_step)
        );
    }

    // x ticks and grid
    let (x_ticks, x_label): (Vec<(f64, String)>, String) = match chart.x_axis {
        XAxis::Time(origin) => {
            let values = time_ticks(x_min, x_max);
            let span = x_max - x_min;
            let labels = values
                .iter()
                .map(|v| {
                    let tag = time_tag(origin, *v);
                    let label = if span <= 86400.0 {
                        tag[11..16].to_string()
                    } else {
                        tag[5..16].to_string()
                    };
                    (*v, label)
                })
                .collect();
            (
                labels,
                format!(
                    "{} (from {})",
                    chart.x_label,
                    &time_tag(origin, x_min)[..16]
                ),
            )
        },
        XAxis::Linear => {
            let values = ticks(x_min, x_max, 8);
            let step = if values.len() > 1 {
                values[1] - values[0]
            } else {
                1.0
            };
            (
                values.iter().map(|v| (*v, format_tick(*v, step))).collect(),
                chart.x_label.clone(),
            )
        },
    };

    for (v, label) in x_ticks.iter() {
        let x = px(*v);
        let _ = write!(
            svg,
            r##"<line x1="{x}" y1="{y0}" x2="{x}" y2="{y1}" stroke="#e5e9ee" stroke-width="0.6"/><text x="{x}" y="{ty}" text-anchor="middle" fill="#3d4b59">{label}</text>"##,
            x = x,
            y0 = MARGIN_TOP,
            y1 = MARGIN_TOP + plot_h,
            ty = MARGIN_TOP + plot_h + 14.0,
            label = escape(label)
        );
    }

    // axis labels
    let _ = write!(
        svg,
        r##"<text x="{}" y="{}" text-anchor="middle" fill="#1d2733">{}</text>"##,
        MARGIN_LEFT + plot_w / 2.0,
        HEIGHT - 8.0,
        escape(&x_label)
    );
    let _ = write!(
        svg,
        r##"<text x="14" y="{y}" text-anchor="middle" fill="#1d2733" transform="rotate(-90 14 {y})">{}</text>"##,
        escape(&chart.y_label),
        y = MARGIN_TOP + plot_h / 2.0
    );

    // clip the data to the plot area
    let _ = write!(
        svg,
        r#"<clipPath id="plot"><rect x="{}" y="{}" width="{}" height="{}"/></clipPath><g clip-path="url(#plot)">"#,
        MARGIN_LEFT, MARGIN_TOP, plot_w, plot_h
    );

    for (index, s) in series.iter().enumerate() {
        let color = PALETTE[index % PALETTE.len()];

        if s.connected {
            // segments (baselines..): every point connected, with markers
            let mut path = String::new();
            for (i, (x, y)) in s.x.iter().zip(s.y.iter()).enumerate() {
                let _ = write!(
                    path,
                    "{}{:.1},{:.1}",
                    if i == 0 { 'M' } else { 'L' },
                    px(*x),
                    py(*y)
                );
            }
            let _ = write!(
                svg,
                r#"<path d="{}" fill="none" stroke="{c}" stroke-width="2"/><g fill="{c}">"#,
                path,
                c = color
            );
            for (x, y) in s.x.iter().zip(s.y.iter()) {
                let _ = write!(
                    svg,
                    r#"<circle cx="{:.1}" cy="{:.1}" r="3.5"/>"#,
                    px(*x),
                    py(*y)
                );
            }
            svg.push_str("</g>");
        } else if chart.kind == ChartKind::Lines {
            // typical step, to break the lines over data gaps
            let mut steps =
                s.x.windows(2)
                    .map(|w| w[1] - w[0])
                    .filter(|dx| *dx > 0.0)
                    .collect::<Vec<_>>();
            steps.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let typical = steps.get(steps.len() / 2).copied().unwrap_or(f64::MAX);

            let mut path = String::new();
            let mut previous: Option<f64> = None;
            for (x, y) in s.x.iter().zip(s.y.iter()) {
                if !x.is_finite() || !y.is_finite() {
                    previous = None;
                    continue;
                }
                let command = match previous {
                    Some(prev) if x - prev <= 10.0 * typical => 'L',
                    _ => 'M',
                };
                let _ = write!(path, "{}{:.1},{:.1}", command, px(*x), py(*y));
                previous = Some(*x);
            }
            let _ = write!(
                svg,
                r#"<path d="{}" fill="none" stroke="{}" stroke-width="1"/>"#,
                path, color
            );
        } else {
            let _ = write!(svg, r#"<g fill="{}">"#, color);
            for (x, y) in s.x.iter().zip(s.y.iter()) {
                if x.is_finite() && y.is_finite() {
                    let _ = write!(
                        svg,
                        r#"<circle cx="{:.1}" cy="{:.1}" r="1.3"/>"#,
                        px(*x),
                        py(*y)
                    );
                }
            }
            let _ = write!(svg, "</g>");
        }
    }

    let _ = write!(svg, "</g>");

    // legend
    for (index, (s, (x, row))) in series.iter().zip(legend.iter()).enumerate() {
        let color = PALETTE[index % PALETTE.len()];
        let y = HEIGHT + 4.0 + 14.0 * *row as f64;
        let _ = write!(
            svg,
            r##"<rect x="{}" y="{}" width="9" height="9" fill="{}"/><text x="{}" y="{}" fill="#1d2733">{}</text>"##,
            x,
            y - 8.0,
            color,
            x + 12.0,
            y,
            escape(&s.name)
        );
    }

    svg.push_str("</svg>");
    svg
}

/// Viridis color scale (5 stops, linear interpolation)
fn viridis(t: f64) -> String {
    const STOPS: [(f64, (f64, f64, f64)); 5] = [
        (0.00, (68.0, 1.0, 84.0)),
        (0.25, (59.0, 82.0, 139.0)),
        (0.50, (33.0, 145.0, 140.0)),
        (0.75, (94.0, 201.0, 98.0)),
        (1.00, (253.0, 231.0, 37.0)),
    ];

    let t = if t.is_finite() {
        t.clamp(0.0, 1.0)
    } else {
        0.0
    };

    let i = STOPS
        .windows(2)
        .position(|w| t <= w[1].0)
        .unwrap_or(STOPS.len() - 2);

    let ((t0, c0), (t1, c1)) = (STOPS[i], STOPS[i + 1]);
    let w = (t - t0) / (t1 - t0);
    let mix = |a: f64, b: f64| (a + (b - a) * w).round() as u8;

    format!(
        "#{:02x}{:02x}{:02x}",
        mix(c0.0, c1.0),
        mix(c0.1, c1.1),
        mix(c0.2, c1.2)
    )
}

/// Renders one frame of a heatmap as SVG (color scale shared by all frames)
pub fn render_heatmap(heatmap: &Heatmap, frame: &HeatmapFrame) -> String {
    const COLORBAR: f64 = 70.0;

    let (zmin, zmax) = heatmap.range();
    let span = (zmax - zmin).max(1.0E-9);

    let (nx, ny) = (heatmap.x.len(), heatmap.y.len());

    if nx == 0 || ny == 0 {
        return format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="40"></svg>"#,
            WIDTH
        );
    }

    // cell size: grid spacing (nodes at the cell centers)
    let dx = if nx > 1 {
        (heatmap.x[nx - 1] - heatmap.x[0]) / (nx - 1) as f64
    } else {
        1.0
    };
    let dy = if ny > 1 {
        (heatmap.y[ny - 1] - heatmap.y[0]) / (ny - 1) as f64
    } else {
        1.0
    };

    let (x_min, x_max) = (heatmap.x[0] - dx / 2.0, heatmap.x[nx - 1] + dx / 2.0);
    let (y_min, y_max) = (heatmap.y[0] - dy / 2.0, heatmap.y[ny - 1] + dy / 2.0);

    // equal axes
    let plot_w_max = WIDTH - MARGIN_LEFT - MARGIN_RIGHT - COLORBAR;
    let plot_h_max = 2.0 * HEIGHT - MARGIN_TOP - MARGIN_BOTTOM;
    let scale = (plot_w_max / (x_max - x_min)).min(plot_h_max / (y_max - y_min));
    let (plot_w, plot_h) = ((x_max - x_min) * scale, (y_max - y_min) * scale);
    let height = plot_h + MARGIN_TOP + MARGIN_BOTTOM;

    let px = |x: f64| MARGIN_LEFT + (x - x_min) * scale;
    let py = |y: f64| MARGIN_TOP + plot_h - (y - y_min) * scale;

    let mut svg = String::with_capacity(512 * 1024);
    let _ = write!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" font-family="{f}" font-size="10">"#,
        w = WIDTH,
        h = height,
        f = FONT
    );

    // cells
    for (j, row) in frame.z.iter().enumerate().take(ny) {
        for (i, value) in row.iter().enumerate().take(nx) {
            if !value.is_finite() {
                continue;
            }
            let (x, y) = (heatmap.x[i], heatmap.y[j]);
            let _ = write!(
                svg,
                r#"<rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" fill="{}"/>"#,
                px(x - dx / 2.0),
                py(y + dy / 2.0),
                dx * scale + 0.3,
                dy * scale + 0.3,
                viridis((value - zmin) / span)
            );
        }
    }

    // frame and graticule (every 30°)
    let _ = write!(
        svg,
        r##"<rect x="{}" y="{}" width="{}" height="{}" fill="none" stroke="#3d4b59" stroke-width="0.8"/>"##,
        MARGIN_LEFT, MARGIN_TOP, plot_w, plot_h
    );

    for v in ticks(x_min, x_max, 8) {
        let x = px(v);
        let _ = write!(
            svg,
            r##"<line x1="{x}" y1="{y0}" x2="{x}" y2="{y1}" stroke="#ffffff" stroke-opacity="0.35" stroke-width="0.5"/><text x="{x}" y="{ty}" text-anchor="middle" fill="#3d4b59">{label}</text>"##,
            x = x,
            y0 = MARGIN_TOP,
            y1 = MARGIN_TOP + plot_h,
            ty = MARGIN_TOP + plot_h + 14.0,
            label = format_tick(v, 1.0)
        );
    }

    for v in ticks(y_min, y_max, 6) {
        let y = py(v);
        let _ = write!(
            svg,
            r##"<line x1="{x0}" y1="{y}" x2="{x1}" y2="{y}" stroke="#ffffff" stroke-opacity="0.35" stroke-width="0.5"/><text x="{tx}" y="{ty}" text-anchor="end" fill="#3d4b59">{label}</text>"##,
            x0 = MARGIN_LEFT,
            x1 = MARGIN_LEFT + plot_w,
            y = y,
            tx = MARGIN_LEFT - 5.0,
            ty = y + 3.5,
            label = format_tick(v, 1.0)
        );
    }

    // axis labels
    let _ = write!(
        svg,
        r##"<text x="{}" y="{}" text-anchor="middle" fill="#1d2733">{}</text>"##,
        MARGIN_LEFT + plot_w / 2.0,
        height - 8.0,
        escape(&heatmap.x_label)
    );
    let _ = write!(
        svg,
        r##"<text x="14" y="{y}" text-anchor="middle" fill="#1d2733" transform="rotate(-90 14 {y})">{}</text>"##,
        escape(&heatmap.y_label),
        y = MARGIN_TOP + plot_h / 2.0
    );

    // color bar
    let (bx, bw) = (MARGIN_LEFT + plot_w + 18.0, 12.0);
    let steps = 64;
    for k in 0..steps {
        let t = k as f64 / (steps - 1) as f64;
        let y = MARGIN_TOP + plot_h * (1.0 - (k + 1) as f64 / steps as f64);
        let _ = write!(
            svg,
            r#"<rect x="{:.1}" y="{:.2}" width="{}" height="{:.2}" fill="{}"/>"#,
            bx,
            y,
            bw,
            plot_h / steps as f64 + 0.3,
            viridis(t)
        );
    }

    let bar_ticks = ticks(zmin, zmax, 5);
    let bar_step = if bar_ticks.len() > 1 {
        bar_ticks[1] - bar_ticks[0]
    } else {
        1.0
    };

    for v in bar_ticks {
        let y = MARGIN_TOP + plot_h * (1.0 - (v - zmin) / span);
        let _ = write!(
            svg,
            r##"<text x="{}" y="{:.1}" fill="#3d4b59">{}</text>"##,
            bx + bw + 3.0,
            y + 3.5,
            format_tick(v, bar_step)
        );
    }

    let _ = write!(
        svg,
        r##"<text x="{}" y="{}" fill="#1d2733">{}</text>"##,
        bx - 4.0,
        MARGIN_TOP - 4.0,
        escape(&heatmap.z_label)
    );

    svg.push_str("</svg>");
    svg
}

/// Local map (PDF): the points are projected on a local plan (east, north in meters or
/// kilometers), north up, equal scales, with a scale bar. Remarkable points (stations) are
/// labeled, series colored by value carry a color scale.
pub fn render_map(chart: &Chart) -> String {
    use crate::report::geo::{land, local_plan, orthographic};

    // regional map: land masses and graticule
    let region = chart.kind == ChartKind::Region;

    const MAP_WIDTH: f64 = 720.0;
    const MAP_HEIGHT: f64 = 440.0;
    const LEFT: f64 = 70.0;
    const RIGHT: f64 = 90.0;
    const TOP: f64 = 16.0;
    const BOTTOM: f64 = 44.0;

    let series = chart
        .series
        .iter()
        .map(|s| s.decimated(MAX_POINTS))
        .collect::<Vec<_>>();

    // projection origin: center of the extent
    let points = series
        .iter()
        .flat_map(|s| s.x.iter().zip(s.y.iter()))
        .filter(|(x, y)| x.is_finite() && y.is_finite())
        .map(|(x, y)| (*x, *y))
        .collect::<Vec<_>>();

    if points.is_empty() {
        return format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="40"></svg>"#,
            MAP_WIDTH
        );
    }

    let (lon0, lat0) =
        crate::report::geographic_center(points.iter().copied()).unwrap_or(points[0]);

    let plan = series
        .iter()
        .map(|s| {
            s.x.iter()
                .zip(s.y.iter())
                .map(|(lon, lat)| local_plan(lon0, lat0, *lon, *lat))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    let all = plan
        .iter()
        .flatten()
        .filter(|(e, n)| e.is_finite() && n.is_finite());
    let (mut e_min, mut e_max, mut n_min, mut n_max) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for (e, n) in all {
        e_min = e_min.min(*e);
        e_max = e_max.max(*e);
        n_min = n_min.min(*n);
        n_max = n_max.max(*n);
    }

    let plot_w = MAP_WIDTH - LEFT - RIGHT;
    let plot_h = MAP_HEIGHT - TOP - BOTTOM;

    // equal scales, 6 % margin
    let span = ((e_max - e_min) / plot_w)
        .max((n_max - n_min) / plot_h)
        .max(1.0E-3)
        * 1.12;
    let (ce, cn) = ((e_min + e_max) / 2.0, (n_min + n_max) / 2.0);
    let (e_min, e_max) = (ce - span * plot_w / 2.0, ce + span * plot_w / 2.0);
    let (n_min, n_max) = (cn - span * plot_h / 2.0, cn + span * plot_h / 2.0);

    // kilometers above 5 km
    let km = (e_max - e_min) > 5000.0;
    let unit = if km { 1000.0 } else { 1.0 };

    let px = |e: f64| LEFT + (e - e_min) / (e_max - e_min) * plot_w;
    let py = |n: f64| TOP + plot_h - (n - n_min) / (n_max - n_min) * plot_h;

    let mut svg = String::with_capacity(64 * 1024);
    let _ = write!(
        svg,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" font-family="{f}" font-size="10"><rect x="{l}" y="{t}" width="{pw}" height="{ph}" fill="{bg}" stroke="#9aa5b1" stroke-width="0.8"/>"##,
        bg = if region { "#cfe3f3" } else { "#f4f7f9" },
        w = MAP_WIDTH,
        h = MAP_HEIGHT,
        f = FONT,
        l = LEFT,
        t = TOP,
        pw = plot_w,
        ph = plot_h
    );

    // grid
    for v in ticks(e_min / unit, e_max / unit, 7) {
        let x = px(v * unit);
        let _ = write!(
            svg,
            r##"<line x1="{x}" y1="{t}" x2="{x}" y2="{b}" stroke="#e1e6eb" stroke-width="0.6"/><text x="{x}" y="{ty}" text-anchor="middle" fill="#3d4b59">{v}</text>"##,
            x = x,
            t = TOP,
            b = TOP + plot_h,
            ty = TOP + plot_h + 14.0,
            v = format_tick(v, 0.1)
        );
    }
    for v in ticks(n_min / unit, n_max / unit, 5) {
        let y = py(v * unit);
        let _ = write!(
            svg,
            r##"<line x1="{l}" y1="{y}" x2="{r}" y2="{y}" stroke="#e1e6eb" stroke-width="0.6"/><text x="{tx}" y="{ty}" text-anchor="end" fill="#3d4b59">{v}</text>"##,
            l = LEFT,
            r = LEFT + plot_w,
            y = y,
            tx = LEFT - 5.0,
            ty = y + 3.5,
            v = format_tick(v, 0.1)
        );
    }

    let unit_label = if km { "km" } else { "m" };
    let _ = write!(
        svg,
        r##"<text x="{}" y="{}" text-anchor="middle" fill="#1d2733">East ({}) — origin {:.5}°, {:.5}°</text><text x="14" y="{y}" text-anchor="middle" fill="#1d2733" transform="rotate(-90 14 {y})">North ({})</text>"##,
        LEFT + plot_w / 2.0,
        MAP_HEIGHT - 8.0,
        unit_label,
        lat0,
        lon0,
        unit_label,
        y = TOP + plot_h / 2.0
    );

    // color scale range
    let color_values = series
        .iter()
        .filter_map(|s| s.colors.as_ref())
        .flatten()
        .copied()
        .filter(|v| v.is_finite())
        .collect::<Vec<_>>();
    let (c_min, c_max) = color_values
        .iter()
        .fold((f64::MAX, f64::MIN), |(a, b), v| (a.min(*v), b.max(*v)));

    let _ = write!(
        svg,
        r#"<clipPath id="map"><rect x="{}" y="{}" width="{}" height="{}"/></clipPath><g clip-path="url(#map)">"#,
        LEFT, TOP, plot_w, plot_h
    );

    if region {
        // visible points of a polyline (lon, lat) as an SVG path, broken behind the horizon
        let polyline = |points: &mut dyn Iterator<Item = (f64, f64)>| -> String {
            let mut path = String::new();
            let mut pen = false;
            for (lon, lat) in points {
                let (_, _, visible) = orthographic(lon0, lat0, lon, lat);
                if visible {
                    let (e, n) = local_plan(lon0, lat0, lon, lat);
                    let _ = write!(
                        path,
                        "{}{:.1},{:.1}",
                        if pen { 'L' } else { 'M' },
                        px(e),
                        py(n)
                    );
                }
                pen = visible;
            }
            path
        };

        // land masses (clipped to the frame)
        for ring in land() {
            if !ring
                .iter()
                .any(|(lon, lat)| orthographic(lon0, lat0, *lon, *lat).2)
            {
                continue;
            }
            // fill: one continuous outline, points behind the horizon are pushed onto it
            let mut path = String::new();
            for (k, (lon, lat)) in ring.iter().enumerate() {
                let (x, y, visible) = orthographic(lon0, lat0, *lon, *lat);
                let (e, n) = if visible {
                    local_plan(lon0, lat0, *lon, *lat)
                } else {
                    let rho = (x * x + y * y).sqrt().max(1.0E-12);
                    let horizon = 6_371_008.8 * std::f64::consts::FRAC_PI_2;
                    (horizon * x / rho, horizon * y / rho)
                };
                let _ = write!(
                    path,
                    "{}{:.1},{:.1}",
                    if k == 0 { 'M' } else { 'L' },
                    px(e),
                    py(n)
                );
            }
            if !path.is_empty() {
                let coast = coast_path(ring, |lon, lat| {
                    let (_, _, visible) = orthographic(lon0, lat0, lon, lat);
                    visible.then(|| {
                        let (e, n) = local_plan(lon0, lat0, lon, lat);
                        (px(e), py(n))
                    })
                });
                let _ = write!(
                    svg,
                    r##"<path d="{}Z" fill="#e3e7d3" stroke="none"/><path d="{}" fill="none" stroke="#6b7a89" stroke-width="0.6"/>"##,
                    path, coast
                );
            }
        }

        // graticule: 5° or 10° depending on the extent
        let step = if (e_max - e_min) > 2.0E6 { 10 } else { 5 };
        for lat in (-85..=85).filter(|lat| lat % step == 0) {
            let path = polyline(&mut (-360..=360).map(|k| (k as f64 * 0.5, lat as f64)));
            let _ = write!(
                svg,
                r##"<path d="{}" fill="none" stroke="#9fb3c4" stroke-width="0.4"/>"##,
                path
            );
        }
        for lon in (-180..180).filter(|lon| lon % step == 0) {
            let path = polyline(&mut (-178..=178).map(|k| (lon as f64, k as f64 * 0.5)));
            let _ = write!(
                svg,
                r##"<path d="{}" fill="none" stroke="#9fb3c4" stroke-width="0.4"/>"##,
                path
            );
        }
    }

    // lines and points first, stations on top
    let mut order = (0..series.len()).collect::<Vec<_>>();
    order.sort_by_key(|i| series[*i].anchor);

    for index in order {
        let (s, points) = (&series[index], &plan[index]);
        let color = PALETTE[index % PALETTE.len()];

        if s.anchor {
            for (e, n) in points.iter() {
                let (x, y) = (px(*e), py(*n));
                // star
                let mut path = String::new();
                for k in 0..10 {
                    let r = if k % 2 == 0 { 9.0 } else { 4.0 };
                    let a = std::f64::consts::PI / 5.0 * k as f64 - std::f64::consts::FRAC_PI_2;
                    let _ = write!(
                        path,
                        "{}{:.1},{:.1}",
                        if k == 0 { 'M' } else { 'L' },
                        x + r * a.cos(),
                        y + r * a.sin()
                    );
                }
                let _ = write!(
                    svg,
                    r##"<path d="{}Z" fill="#d62728" stroke="#ffffff" stroke-width="0.8"/><text x="{}" y="{}" fill="#1d2733" font-weight="bold">{}</text>"##,
                    path,
                    x + 11.0,
                    y - 6.0,
                    escape(&s.name)
                );
            }
        } else if let Some(track_color) = &s.color {
            // track: solid line, broken over the gaps (NaN), labeled at its end
            let mut path = String::new();
            let mut pen = false;
            for (e, n) in points.iter() {
                if e.is_finite() && n.is_finite() {
                    let _ = write!(
                        path,
                        "{}{:.1},{:.1}",
                        if pen { 'L' } else { 'M' },
                        px(*e),
                        py(*n)
                    );
                    pen = true;
                } else {
                    pen = false;
                }
            }
            let _ = write!(
                svg,
                r##"<path d="{}" fill="none" stroke="{}" stroke-width="1.4" stroke-linejoin="round"/>"##,
                path, track_color
            );
            if let Some((e, n)) = points
                .iter()
                .rev()
                .find(|(e, n)| e.is_finite() && n.is_finite())
            {
                let _ = write!(
                    svg,
                    r##"<circle cx="{x:.1}" cy="{y:.1}" r="2" fill="{c}"/><text x="{tx:.1}" y="{ty:.1}" font-size="8" fill="{c}">{label}</text>"##,
                    x = px(*e),
                    y = py(*n),
                    tx = px(*e) + 3.0,
                    ty = py(*n) - 3.0,
                    c = track_color,
                    label = escape(&s.name)
                );
            }
        } else if s.connected {
            let mut path = String::new();
            for (k, (e, n)) in points.iter().enumerate() {
                let _ = write!(
                    path,
                    "{}{:.1},{:.1}",
                    if k == 0 { 'M' } else { 'L' },
                    px(*e),
                    py(*n)
                );
            }
            let _ = write!(
                svg,
                r##"<path d="{}" fill="none" stroke="#555f6b" stroke-width="1.6" stroke-dasharray="6 3"/>"##,
                path
            );
        } else if let Some(colors) = &s.colors {
            // track: thin line (interrupted over the gaps), then points colored by value
            let mut path = String::new();
            let mut pen = false;
            for (e, n) in points.iter() {
                if e.is_finite() && n.is_finite() {
                    let _ = write!(
                        path,
                        "{}{:.1},{:.1}",
                        if pen { 'L' } else { 'M' },
                        px(*e),
                        py(*n)
                    );
                    pen = true;
                } else {
                    pen = false;
                }
            }
            let _ = write!(
                svg,
                r##"<path d="{}" fill="none" stroke="#9aa5b1" stroke-width="0.6"/>"##,
                path
            );
            for ((e, n), v) in points
                .iter()
                .zip(colors.iter())
                .filter(|((e, n), v)| e.is_finite() && n.is_finite() && v.is_finite())
            {
                let t = (v - c_min) / (c_max - c_min).max(1.0E-12);
                let _ = write!(
                    svg,
                    r#"<circle cx="{:.1}" cy="{:.1}" r="2.2" fill="{}"/>"#,
                    px(*e),
                    py(*n),
                    viridis(t)
                );
            }
        } else {
            let _ = write!(svg, r#"<g fill="{}">"#, color);
            for (e, n) in points.iter() {
                let _ = write!(
                    svg,
                    r#"<circle cx="{:.1}" cy="{:.1}" r="1.6"/>"#,
                    px(*e),
                    py(*n)
                );
            }
            let _ = write!(svg, "</g>");
        }
    }
    let _ = write!(svg, "</g>");

    // north arrow
    let (ax, ay) = (LEFT + plot_w - 22.0, TOP + 34.0);
    let _ = write!(
        svg,
        r##"<path d="M{ax},{y0} L{x1},{y1} L{ax},{y2} L{x2},{y1} Z" fill="#1d2733"/><text x="{ax}" y="{ty}" text-anchor="middle" font-weight="bold" fill="#1d2733">N</text>"##,
        ax = ax,
        y0 = ay - 16.0,
        x1 = ax + 6.0,
        y1 = ay + 6.0,
        y2 = ay + 1.0,
        x2 = ax - 6.0,
        ty = ay - 20.0
    );

    // scale bar: a round length close to 1/5 of the width
    let target = (e_max - e_min) / 5.0;
    let magnitude = 10f64.powf(target.log10().floor());
    let length = [1.0, 2.0, 5.0, 10.0]
        .iter()
        .map(|m| m * magnitude)
        .filter(|l| *l <= target)
        .fold(magnitude, f64::max);
    let bar = length / (e_max - e_min) * plot_w;
    let (bx, by) = (LEFT + 12.0, TOP + plot_h - 14.0);
    let _ = write!(
        svg,
        r##"<rect x="{bx}" y="{by}" width="{bar:.1}" height="5" fill="#1d2733"/><rect x="{bx}" y="{by}" width="{half:.1}" height="5" fill="#ffffff" stroke="#1d2733" stroke-width="0.6"/><text x="{bx}" y="{ty}" fill="#1d2733">{label}</text>"##,
        bx = bx,
        by = by,
        bar = bar,
        half = bar / 2.0,
        ty = by - 4.0,
        label = if length >= 1000.0 {
            format!("{} km", length / 1000.0)
        } else {
            format!("{} m", length)
        }
    );

    // color bar
    if !color_values.is_empty() {
        let (cx, cy, ch) = (LEFT + plot_w + 18.0, TOP + 20.0, plot_h - 40.0);
        for k in 0..50 {
            let t = 1.0 - k as f64 / 49.0;
            let _ = write!(
                svg,
                r#"<rect x="{}" y="{:.1}" width="12" height="{:.1}" fill="{}"/>"#,
                cx,
                cy + ch * k as f64 / 50.0,
                ch / 50.0 + 0.5,
                viridis(t)
            );
        }
        let _ = write!(
            svg,
            r##"<text x="{x}" y="{y0}" fill="#1d2733">{max}</text><text x="{x}" y="{y1}" fill="#1d2733">{min}</text><text x="{lx}" y="{ly}" fill="#1d2733">{label}</text>"##,
            x = cx + 16.0,
            y0 = cy + 8.0,
            y1 = cy + ch,
            max = format_tick(c_max, 1.0),
            min = format_tick(c_min, 1.0),
            lx = cx - 4.0,
            ly = cy - 6.0,
            label = escape(chart.color_label.as_deref().unwrap_or(""))
        );
    }

    svg.push_str("</svg>");
    svg
}

/// World map (PDF): orthographic projection centered on the points
/// (Natural Earth land masses), the points being marked.
pub fn render_globe(chart: &Chart) -> String {
    use crate::report::geo::{land, orthographic};

    const SIZE: f64 = 360.0;
    const R: f64 = 160.0;
    let (cx, cy) = (SIZE / 2.0, SIZE / 2.0);

    let points = chart
        .series
        .iter()
        .flat_map(|s| s.x.iter().zip(s.y.iter()))
        .filter(|(x, y)| x.is_finite() && y.is_finite())
        .map(|(x, y)| (*x, *y))
        .collect::<Vec<_>>();

    let (lon0, lat0) = if points.is_empty() {
        (0.0, 0.0)
    } else {
        (
            points.iter().map(|p| p.0).sum::<f64>() / points.len() as f64,
            points.iter().map(|p| p.1).sum::<f64>() / points.len() as f64,
        )
    };

    let project = |lon: f64, lat: f64| {
        let (x, y, visible) = orthographic(lon0, lat0, lon, lat);
        // points behind the globe are pushed onto the limb
        let (x, y) = if visible {
            (x, y)
        } else {
            let norm = (x * x + y * y).sqrt().max(1.0E-9);
            (x / norm, y / norm)
        };
        (cx + R * x, cy - R * y, visible)
    };

    let mut svg = String::with_capacity(64 * 1024);
    let _ = write!(
        svg,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{s}" height="{s}" viewBox="0 0 {s} {s}" font-family="{f}" font-size="10"><circle cx="{cx}" cy="{cy}" r="{r}" fill="#cfe3f3" stroke="#6b7a89" stroke-width="0.8"/>"##,
        s = SIZE,
        f = FONT,
        cx = cx,
        cy = cy,
        r = R
    );

    // land masses
    for ring in land() {
        let projected = ring
            .iter()
            .map(|(lon, lat)| project(*lon, *lat))
            .collect::<Vec<_>>();
        if !projected.iter().any(|(_, _, visible)| *visible) {
            continue;
        }
        let mut path = String::new();
        for (k, (x, y, _)) in projected.iter().enumerate() {
            let _ = write!(path, "{}{:.1},{:.1}", if k == 0 { 'M' } else { 'L' }, x, y);
        }
        let coast = coast_path(ring, |lon, lat| {
            let (x, y, visible) = project(lon, lat);
            visible.then_some((x, y))
        });
        let _ = write!(
            svg,
            r##"<path d="{}Z" fill="#e3e7d3" stroke="none"/><path d="{}" fill="none" stroke="#6b7a89" stroke-width="0.4"/>"##,
            path, coast
        );
    }

    // graticule (30°)
    for lat in (-60..=60).step_by(30) {
        let mut path = String::new();
        let mut pen = false;
        for lon in (-180..=180).step_by(5) {
            let (x, y, visible) = project(lon as f64, lat as f64);
            if visible {
                let _ = write!(path, "{}{:.1},{:.1}", if pen { 'L' } else { 'M' }, x, y);
            }
            pen = visible;
        }
        let _ = write!(
            svg,
            r##"<path d="{}" fill="none" stroke="#9fb3c4" stroke-width="0.4"/>"##,
            path
        );
    }
    for lon in (-180..180).step_by(30) {
        let mut path = String::new();
        let mut pen = false;
        for lat in (-90..=90).step_by(5) {
            let (x, y, visible) = project(lon as f64, lat as f64);
            if visible {
                let _ = write!(path, "{}{:.1},{:.1}", if pen { 'L' } else { 'M' }, x, y);
            }
            pen = visible;
        }
        let _ = write!(
            svg,
            r##"<path d="{}" fill="none" stroke="#9fb3c4" stroke-width="0.4"/>"##,
            path
        );
    }

    // tracks
    for s in chart
        .series
        .iter()
        .filter(|s| s.color.is_some() && !s.anchor)
    {
        let mut path = String::new();
        let mut pen = false;
        for (lon, lat) in s.x.iter().zip(s.y.iter()) {
            if !(lon.is_finite() && lat.is_finite()) {
                pen = false;
                continue;
            }
            let (x, y, visible) = project(*lon, *lat);
            if visible {
                let _ = write!(path, "{}{:.1},{:.1}", if pen { 'L' } else { 'M' }, x, y);
            }
            pen = visible;
        }
        let _ = write!(
            svg,
            r##"<path d="{}" fill="none" stroke="{}" stroke-width="1"/>"##,
            path,
            s.color.as_deref().unwrap_or("#1f77b4")
        );
    }

    // stations
    let anchors = chart.series.iter().filter(|s| s.anchor).collect::<Vec<_>>();
    for s in anchors.iter() {
        if let Some((x, y, _)) =
            s.x.first()
                .zip(s.y.first())
                .map(|(lon, lat)| project(*lon, *lat))
        {
            let _ = write!(
                svg,
                r##"<circle cx="{x:.1}" cy="{y:.1}" r="4" fill="#d62728" stroke="#ffffff" stroke-width="1"/><text x="{tx:.1}" y="{ty:.1}" fill="#1d2733" font-weight="bold">{label}</text>"##,
                x = x,
                y = y,
                tx = x + 7.0,
                ty = y - 6.0,
                label = escape(&s.name)
            );
        }
    }

    // otherwise (all series): red circle around the location
    if !anchors.is_empty() {
        svg.push_str("</svg>");
        return svg;
    }
    if let Some((x, y, _)) = points.first().map(|(lon, lat)| project(*lon, *lat)) {
        let _ = write!(
            svg,
            r##"<circle cx="{x:.1}" cy="{y:.1}" r="7" fill="none" stroke="#d62728" stroke-width="2"/><circle cx="{x:.1}" cy="{y:.1}" r="2.5" fill="#d62728"/><text x="{tx:.1}" y="{ty:.1}" fill="#1d2733" font-weight="bold">{label}</text>"##,
            x = x,
            y = y,
            tx = x + 10.0,
            ty = y - 8.0,
            label = escape(&format!("{:.3}°, {:.3}°", lat0, lon0))
        );
    }

    svg.push_str("</svg>");
    svg
}

/// Sky plot (PDF): polar projection, north up, azimuth clockwise, zenith at the center
pub fn render_skyplot(skyplot: &crate::report::sky::Skyplot) -> String {
    const SIZE: f64 = 560.0;
    const R: f64 = 230.0;
    let (cx, cy) = (SIZE / 2.0 - 40.0, SIZE / 2.0);

    let project = |az: f64, el: f64| {
        let r = R * (90.0 - el.clamp(0.0, 90.0)) / 90.0;
        let a = az.to_radians();
        (cx + r * a.sin(), cy - r * a.cos())
    };

    let mut svg = String::with_capacity(64 * 1024);
    let _ = write!(
        svg,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{s}" height="{s}" viewBox="0 0 {s} {s}" font-family="{f}" font-size="10">"##,
        s = SIZE,
        f = FONT
    );

    // elevation circles
    for el in [0.0, 15.0, 30.0, 45.0, 60.0, 75.0] {
        let r = R * (90.0 - el) / 90.0;
        let _ = write!(
            svg,
            r##"<circle cx="{cx}" cy="{cy}" r="{r:.1}" fill="{fill}" stroke="#c3ccd5" stroke-width="{w}"/><text x="{tx:.1}" y="{ty:.1}" fill="#6b7a89">{el}°</text>"##,
            cx = cx,
            cy = cy,
            r = r,
            fill = if el == 0.0 { "#f7f9fb" } else { "none" },
            w = if el == 0.0 { 1.0 } else { 0.5 },
            tx = cx + 3.0,
            ty = cy - r + 11.0,
            el = el
        );
    }

    // azimuth spokes
    for (az, label) in [
        (0.0, "N"),
        (45.0, "NE"),
        (90.0, "E"),
        (135.0, "SE"),
        (180.0, "S"),
        (225.0, "SW"),
        (270.0, "W"),
        (315.0, "NW"),
    ] {
        let (x, y) = project(az, 0.0);
        let (lx, ly) = {
            let a = f64::to_radians(az);
            (cx + (R + 14.0) * a.sin(), cy - (R + 14.0) * a.cos() + 4.0)
        };
        let _ = write!(
            svg,
            r##"<line x1="{cx}" y1="{cy}" x2="{x:.1}" y2="{y:.1}" stroke="#c3ccd5" stroke-width="0.5"/><text x="{lx:.1}" y="{ly:.1}" text-anchor="middle" font-weight="bold" fill="#1d2733">{label}</text>"##,
            cx = cx,
            cy = cy,
            x = x,
            y = y,
            lx = lx,
            ly = ly,
            label = label
        );
    }

    // tracks
    for track in skyplot.tracks.iter() {
        let mut path = String::new();
        let mut previous: Option<f64> = None;
        for (t, az, el) in track.points.iter() {
            let (x, y) = project(*az, *el);
            let command = match previous {
                Some(p) if t - p <= skyplot.gap_s => 'L',
                _ => 'M',
            };
            let _ = write!(path, "{}{:.1},{:.1}", command, x, y);
            previous = Some(*t);
        }
        let _ = write!(
            svg,
            r#"<path d="{}" fill="none" stroke="{}" stroke-width="1.6" stroke-linecap="round"/>"#,
            path, track.color
        );
        if let Some((_, az, el)) = track.points.last() {
            let (x, y) = project(*az, *el);
            let _ = write!(
                svg,
                r#"<circle cx="{x:.1}" cy="{y:.1}" r="2.6" fill="{c}"/><text x="{tx:.1}" y="{ty:.1}" fill="{c}" font-size="9">{label}</text>"#,
                x = x,
                y = y,
                c = track.color,
                tx = x + 4.0,
                ty = y - 3.0,
                label = escape(&track.label)
            );
        }
    }

    // legend: constellations
    let mut groups = Vec::<(&str, &str)>::new();
    for track in skyplot.tracks.iter() {
        if !groups.iter().any(|(g, _)| *g == track.group) {
            groups.push((&track.group, &track.group_color));
        }
    }
    for (k, (group, color)) in groups.iter().enumerate() {
        let y = 24.0 + 16.0 * k as f64;
        let _ = write!(
            svg,
            r##"<rect x="{x}" y="{y0}" width="10" height="10" fill="{c}"/><text x="{tx}" y="{y}" fill="#1d2733">{g}</text>"##,
            x = SIZE - 110.0,
            y0 = y - 9.0,
            c = color,
            tx = SIZE - 96.0,
            y = y,
            g = escape(group)
        );
    }

    svg.push_str("</svg>");
    svg
}

/// Visibility timeline (PDF): one row per satellite
pub fn render_timeline(timeline: &crate::report::sky::Timeline) -> String {
    const ROW: f64 = 9.0;
    const LEFT: f64 = 52.0;
    const RIGHT: f64 = 16.0;
    const TOP: f64 = 10.0;
    const BOTTOM: f64 = 36.0;

    let rows = timeline.rows.len().max(1);
    let height = TOP + BOTTOM + ROW * rows as f64;
    let plot_w = WIDTH - LEFT - RIGHT;
    let end = timeline.end_s.max(1.0);
    let px = |t: f64| LEFT + t.clamp(0.0, end) / end * plot_w;

    let mut svg = String::with_capacity(32 * 1024);
    let _ = write!(
        svg,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}" font-family="{f}" font-size="8">"##,
        w = WIDTH,
        h = height,
        f = FONT
    );

    // time grid
    let span = end;
    for v in time_ticks(0.0, end) {
        let x = px(v);
        let tag = time_tag(timeline.origin, v);
        let label = if span <= 86400.0 {
            &tag[11..16]
        } else {
            &tag[5..16]
        };
        let _ = write!(
            svg,
            r##"<line x1="{x}" y1="{t}" x2="{x}" y2="{b}" stroke="#e5e9ee" stroke-width="0.6"/><text x="{x}" y="{ty}" text-anchor="middle" fill="#3d4b59" font-size="9">{label}</text>"##,
            x = x,
            t = TOP,
            b = TOP + ROW * rows as f64,
            ty = TOP + ROW * rows as f64 + 13.0,
            label = label
        );
    }
    let _ = write!(
        svg,
        r##"<text x="{}" y="{}" text-anchor="middle" fill="#1d2733" font-size="10">{} (from {})</text>"##,
        LEFT + plot_w / 2.0,
        height - 6.0,
        timeline.origin.time_scale,
        &time_tag(timeline.origin, 0.0)[..16]
    );

    for (k, row) in timeline.rows.iter().enumerate() {
        let y = TOP + ROW * k as f64;
        if k % 2 == 0 {
            let _ = write!(
                svg,
                r##"<rect x="{}" y="{}" width="{}" height="{}" fill="#f5f7f9"/>"##,
                LEFT, y, plot_w, ROW
            );
        }
        let _ = write!(
            svg,
            r##"<text x="{}" y="{:.1}" text-anchor="end" fill="#1d2733">{}</text>"##,
            LEFT - 4.0,
            y + ROW - 1.5,
            escape(&row.label)
        );
        for (start, stop) in row.intervals.iter() {
            let (x0, x1) = (px(*start), px(*stop));
            let _ = write!(
                svg,
                r#"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" fill="{}"/>"#,
                x0,
                y + 1.5,
                (x1 - x0).max(0.8),
                ROW - 3.0,
                row.color
            );
        }
    }

    svg.push_str("</svg>");
    svg
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn nice_ticks() {
        assert_eq!(ticks(0.0, 10.0, 5), vec![0.0, 2.0, 4.0, 6.0, 8.0, 10.0]);
        // 3 h ticks over a day
        assert_eq!(time_ticks(0.0, 86400.0).len(), 9);
        assert_eq!(format_tick(2.5, 0.5), "2.5");
        assert_eq!(format_tick(1.0e-7, 1.0e-7), "1.0e-7");
    }
}
