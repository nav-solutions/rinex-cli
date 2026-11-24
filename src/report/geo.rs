//! Geography for the static (PDF) maps: land masses and projections.
//!
//! Land masses: Natural Earth 1:110m land polygons (public domain,
//! <https://www.naturalearthdata.com>), outer rings, coordinates rounded to 0.01°.
use std::sync::OnceLock;

/// Natural Earth 1:110m land: one ring per line, "lon,lat" pairs separated by spaces
const LAND_110M: &str = include_str!("land110.txt");

/// Land masses: rings of (longitude, latitude) in degrees
pub fn land() -> &'static [Vec<(f64, f64)>] {
    static LAND: OnceLock<Vec<Vec<(f64, f64)>>> = OnceLock::new();
    LAND.get_or_init(|| {
        LAND_110M
            .lines()
            .map(|line| {
                line.split_whitespace()
                    .filter_map(|pair| {
                        let (lon, lat) = pair.split_once(',')?;
                        Some((lon.parse().ok()?, lat.parse().ok()?))
                    })
                    .collect::<Vec<_>>()
            })
            .filter(|ring| ring.len() > 2)
            .collect()
    })
}

/// Orthographic projection centered on (lon0, lat0), degrees.
/// Returns (x, y) on the unit disk and whether the point faces the viewer.
pub fn orthographic(lon0: f64, lat0: f64, lon: f64, lat: f64) -> (f64, f64, bool) {
    let (l0, p0, l, p) = (
        lon0.to_radians(),
        lat0.to_radians(),
        lon.to_radians(),
        lat.to_radians(),
    );
    let cos_c = p0.sin() * p.sin() + p0.cos() * p.cos() * (l - l0).cos();
    let x = p.cos() * (l - l0).sin();
    let y = p0.cos() * p.sin() - p0.sin() * p.cos() * (l - l0).cos();
    (x, y, cos_c >= 0.0)
}

/// Local plan around (lon0, lat0): east and north offsets (m) of (lon, lat),
/// equirectangular approximation (local maps, a few tens of kilometers)
pub fn local_plan(lon0: f64, lat0: f64, lon: f64, lat: f64) -> (f64, f64) {
    const METERS_PER_DEGREE: f64 = 111_320.0;
    let east = (lon - lon0) * METERS_PER_DEGREE * lat0.to_radians().cos();
    let north = (lat - lat0) * 110_574.0;
    (east, north)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn land_masses() {
        let land = land();
        assert!(land.len() > 100);
        // Svalbard is part of the 1:110m land
        assert!(land.iter().any(|ring| ring
            .iter()
            .any(|(lon, lat)| (10.0..30.0).contains(lon) && (76.0..81.0).contains(lat))));
    }

    #[test]
    fn projections() {
        let (x, y, visible) = orthographic(12.0, 79.0, 12.0, 79.0);
        assert!(x.abs() < 1e-12 && y.abs() < 1e-12 && visible);
        let (_, _, visible) = orthographic(12.0, 79.0, -168.0, -79.0);
        assert!(!visible);
        let (e, n) = local_plan(12.0, 79.0, 12.0, 79.001);
        assert!((n - 110.574).abs() < 1e-3 && e.abs() < 1e-9);
    }
}
