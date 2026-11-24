//! Sky views: sky plot (polar projection of the satellites as seen by a receiver)
//! and visibility timeline (one row per satellite, segments where it is seen).
//!
//! Satellites are colored by constellation (hue), then by satellite (shade).
use std::collections::BTreeMap;

use gnss_qc::prelude::{Constellation, Epoch, SV};

use crate::report::{Block, Section};

/// Minimal interval between two points of a sky plot track (s)
const SKYPLOT_STEP_S: f64 = 60.0;

/// Sky plot track of one satellite
#[derive(Debug, Clone)]
pub struct SkyTrack {
    /// Satellite
    pub label: String,
    /// Constellation (legend group)
    pub group: String,
    /// Color (#rrggbb)
    pub color: String,
    /// Color of the constellation (#rrggbb)
    #[cfg_attr(not(feature = "pdf"), allow(dead_code))]
    pub group_color: String,
    /// (seconds since origin, azimuth °, elevation °)
    pub points: Vec<(f64, f64, f64)>,
}

/// Sky plot
#[derive(Debug, Clone)]
pub struct Skyplot {
    pub title: String,
    pub origin: Epoch,
    /// Tracks are interrupted when two points are further apart (s)
    pub gap_s: f64,
    pub tracks: Vec<SkyTrack>,
}

/// One row of a [Timeline]
#[derive(Debug, Clone)]
pub struct TimelineRow {
    /// Satellite
    pub label: String,
    /// Constellation
    pub group: String,
    /// Color (#rrggbb)
    pub color: String,
    /// Visibility intervals (seconds since origin)
    pub intervals: Vec<(f64, f64)>,
}

/// Visibility timeline
#[derive(Debug, Clone)]
pub struct Timeline {
    pub title: String,
    pub origin: Epoch,
    /// End of the time axis (seconds since origin)
    #[cfg_attr(not(feature = "pdf"), allow(dead_code))]
    pub end_s: f64,
    pub rows: Vec<TimelineRow>,
}

/// Hue of a constellation (degrees), None: gray
fn constellation_hue(constellation: Constellation) -> Option<f64> {
    match constellation {
        Constellation::GPS => Some(212.0),
        Constellation::Galileo => Some(140.0),
        Constellation::Glonass => Some(355.0),
        Constellation::BeiDou => Some(32.0),
        Constellation::QZSS => Some(280.0),
        Constellation::IRNSS => Some(320.0),
        _ => None,
    }
}

fn hsl_to_hex(h: f64, s: f64, l: f64) -> String {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = (h.rem_euclid(360.0)) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let to = |v: f64| ((v + m).clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", to(r), to(g), to(b))
}

/// Color of a constellation (legends)
pub fn constellation_color(constellation: Constellation) -> String {
    match constellation_hue(constellation) {
        Some(hue) => hsl_to_hex(hue, 0.75, 0.45),
        None => "#7f7f7f".to_string(),
    }
}

/// Color of a satellite: hue of its constellation, shade of its PRN
pub fn sv_color(sv: SV) -> String {
    // 6 shades, alternating dark / light so that neighbors differ
    let shade = [0.30, 0.55, 0.40, 0.65, 0.35, 0.50][(sv.prn as usize) % 6];
    match constellation_hue(sv.constellation) {
        Some(hue) => {
            // small hue spread as well
            let hue = hue + ((sv.prn as f64 * 7.0) % 24.0) - 12.0;
            hsl_to_hex(hue, 0.70, shade)
        },
        None => hsl_to_hex(0.0, 0.0, shade),
    }
}

/// Constellation label, satellites sorted by constellation then PRN
fn sort_key(sv: &SV) -> (String, u8) {
    (sv.constellation.to_string(), sv.prn)
}

#[derive(Debug, Default)]
struct SatelliteSky {
    /// Epochs the satellite was seen (seconds since origin)
    seen: Vec<f64>,
    /// (seconds since origin, azimuth, elevation)
    points: Vec<(f64, f64, f64)>,
}

/// Accumulates the sky of one receiver
#[derive(Debug, Default)]
pub struct SkyAccumulator {
    origin: Option<Epoch>,
    last: Option<Epoch>,
    satellites: BTreeMap<SV, SatelliteSky>,
}

impl SkyAccumulator {
    fn elapsed(&mut self, t: Epoch) -> f64 {
        let origin = *self.origin.get_or_insert(t);
        self.last = Some(match self.last {
            Some(last) if last > t => last,
            _ => t,
        });
        (t - origin).to_seconds()
    }

    /// The satellite is seen at this epoch
    pub fn seen(&mut self, sv: SV, t: Epoch) {
        let x = self.elapsed(t);
        let sat = self.satellites.entry(sv).or_default();
        if sat.seen.last() != Some(&x) {
            sat.seen.push(x);
        }
    }

    /// Position of the satellite in the sky of the receiver, at this epoch
    pub fn position(&mut self, sv: SV, t: Epoch, azimuth_deg: f64, elevation_deg: f64) {
        if !azimuth_deg.is_finite() || !elevation_deg.is_finite() || elevation_deg < 0.0 {
            return;
        }
        let x = self.elapsed(t);
        let sat = self.satellites.entry(sv).or_default();
        let decimated = sat
            .points
            .last()
            .is_some_and(|(last, _, _)| x - last < SKYPLOT_STEP_S);
        if !decimated {
            sat.points
                .push((x, azimuth_deg.rem_euclid(360.0), elevation_deg));
        }
    }

    /// True if a position of this satellite is expected at this epoch
    /// (sky plot tracks are decimated)
    pub fn needs_position(&self, sv: SV, t: Epoch) -> bool {
        let Some(origin) = self.origin else {
            return true;
        };
        let x = (t - origin).to_seconds();
        !self
            .satellites
            .get(&sv)
            .and_then(|sat| sat.points.last())
            .is_some_and(|(last, _, _)| x - last < SKYPLOT_STEP_S)
    }

    /// True when a position was presented
    pub fn has_positions(&self) -> bool {
        self.satellites.values().any(|sat| !sat.points.is_empty())
    }

    /// Dominant interval between two epochs a satellite is seen (s)
    fn sampling_s(&self) -> f64 {
        let mut steps = self
            .satellites
            .values()
            .flat_map(|sat| sat.seen.windows(2).map(|w| w[1] - w[0]))
            .filter(|dt| *dt > 0.0)
            .map(|dt| (dt * 1000.0).round() as i64)
            .collect::<Vec<_>>();
        if steps.is_empty() {
            return 30.0;
        }
        steps.sort();
        // most frequent value
        let mut best = (steps[0], 0);
        let mut current = (steps[0], 0);
        for step in steps {
            if step == current.0 {
                current.1 += 1;
            } else {
                current = (step, 1);
            }
            if current.1 > best.1 {
                best = current;
            }
        }
        best.0 as f64 / 1000.0
    }

    /// Sky plot block (None without positions)
    pub fn skyplot(&self, title: &str) -> Option<Block> {
        let origin = self.origin?;
        if !self.has_positions() {
            return None;
        }

        let tracks = self
            .satellites
            .iter()
            .filter(|(_, sat)| !sat.points.is_empty())
            .map(|(sv, sat)| SkyTrack {
                label: sv.to_string(),
                group: sv.constellation.to_string(),
                color: sv_color(*sv),
                group_color: constellation_color(sv.constellation),
                points: sat.points.clone(),
            })
            .collect::<Vec<_>>();

        let mut tracks = tracks;
        tracks.sort_by(|a, b| a.group.cmp(&b.group).then(a.label.cmp(&b.label)));

        Some(Block::Skyplot(Skyplot {
            title: title.to_string(),
            origin,
            gap_s: (5.0 * self.sampling_s()).max(3.0 * SKYPLOT_STEP_S),
            tracks,
        }))
    }

    /// Visibility timeline block (None if nothing was seen)
    pub fn timeline(&self, title: &str) -> Option<Block> {
        let origin = self.origin?;
        let end_s = (self.last? - origin).to_seconds();

        let sampling = self.sampling_s();
        let gap = 2.5 * sampling;

        let mut svs = self
            .satellites
            .iter()
            .filter(|(_, sat)| !sat.seen.is_empty())
            .map(|(sv, _)| *sv)
            .collect::<Vec<_>>();
        svs.sort_by_key(sort_key);

        let rows = svs
            .iter()
            .map(|sv| {
                let seen = &self.satellites[sv].seen;
                let mut intervals = Vec::<(f64, f64)>::new();
                for x in seen.iter() {
                    match intervals.last_mut() {
                        Some((_, end)) if x - *end <= gap => *end = *x,
                        _ => intervals.push((*x, *x)),
                    }
                }
                // a single epoch is presented as one sampling interval
                for (start, end) in intervals.iter_mut() {
                    *end = end.max(*start + sampling).min(end_s.max(*start));
                }
                TimelineRow {
                    label: sv.to_string(),
                    group: sv.constellation.to_string(),
                    color: sv_color(*sv),
                    intervals,
                }
            })
            .collect::<Vec<_>>();

        if rows.is_empty() {
            return None;
        }

        Some(Block::Timeline(Timeline {
            title: title.to_string(),
            origin,
            end_s,
            rows,
        }))
    }

    /// Adds the sky plot and the visibility timeline to this section
    pub fn push_blocks(&self, section: &mut Section, skyplot_title: &str, timeline_title: &str) {
        if let Some(block) = self.skyplot(skyplot_title) {
            section.push(block);
        }
        if let Some(block) = self.timeline(timeline_title) {
            section.push(block);
        }
    }
}

/// Azimuth and elevation (degrees) of a satellite, seen from a receiver (ECEF m)
pub fn azimuth_elevation(rx_ecef_m: (f64, f64, f64), sv_ecef_m: (f64, f64, f64)) -> (f64, f64) {
    let (e, n, u) = gnss_qc::geodesy::ecef_to_enu_vector(
        rx_ecef_m,
        (
            sv_ecef_m.0 - rx_ecef_m.0,
            sv_ecef_m.1 - rx_ecef_m.1,
            sv_ecef_m.2 - rx_ecef_m.2,
        ),
    );
    let azimuth = e.atan2(n).to_degrees().rem_euclid(360.0);
    let elevation = u.atan2((e * e + n * n).sqrt()).to_degrees();
    (azimuth, elevation)
}

#[cfg(test)]
mod test {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn colors() {
        let g01 = SV::from_str("G01").unwrap();
        let g02 = SV::from_str("G02").unwrap();
        let e01 = SV::from_str("E01").unwrap();
        assert_ne!(sv_color(g01), sv_color(g02));
        assert_ne!(sv_color(g01), sv_color(e01));
        assert_eq!(sv_color(g01).len(), 7);
        assert_eq!(hsl_to_hex(0.0, 1.0, 0.5), "#ff0000");
        assert_eq!(hsl_to_hex(120.0, 1.0, 0.5), "#00ff00");
    }

    #[test]
    fn sky_geometry() {
        // receiver on the equator (lon 0), satellite straight up
        let rx = (6378137.0, 0.0, 0.0);
        let (_, el) = azimuth_elevation(rx, (26_000_000.0, 0.0, 0.0));
        assert!((el - 90.0).abs() < 1e-6);
        // satellite towards the north pole: azimuth 0
        let (az, el) = azimuth_elevation(rx, (20_000_000.0, 0.0, 20_000_000.0));
        assert!(az.abs() < 1e-6 || (az - 360.0).abs() < 1e-6);
        assert!(el > 30.0 && el < 40.0);
        // tangent to the horizon: elevation 0
        let (_, el) = azimuth_elevation(rx, (6378137.0, 0.0, 20_000_000.0));
        assert!(el.abs() < 1e-6);
        // towards the east: azimuth 90
        let (az, _) = azimuth_elevation(rx, (6378137.0, 20_000_000.0, 0.0));
        assert!((az - 90.0).abs() < 1e-6);
    }

    #[test]
    fn timeline_intervals() {
        let t0 = Epoch::from_str("2024-09-20T08:00:00 GPST").unwrap();
        let sv = SV::from_str("G01").unwrap();
        let mut sky = SkyAccumulator::default();
        for i in 0..10 {
            sky.seen(
                sv,
                t0 + gnss_qc::prelude::Duration::from_seconds(30.0 * i as f64),
            );
        }
        for i in 20..25 {
            sky.seen(
                sv,
                t0 + gnss_qc::prelude::Duration::from_seconds(30.0 * i as f64),
            );
        }
        let Some(Block::Timeline(timeline)) = sky.timeline("t") else {
            panic!("no timeline");
        };
        assert_eq!(
            timeline.rows[0].intervals,
            vec![(0.0, 270.0), (600.0, 720.0)]
        );
    }
}
