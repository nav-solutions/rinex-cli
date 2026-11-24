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

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Renders a chart as SVG
pub fn render(chart: &Chart) -> String {
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

    let equal = chart.equal_axes || chart.kind == ChartKind::Map;
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

        if chart.kind == ChartKind::Lines {
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
