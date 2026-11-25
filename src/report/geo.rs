//! Geography for the maps: land masses and projections.
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

/// Land masses as SVG paths, in an equirectangular frame: `project` maps
/// (longitude, latitude) to the drawing coordinates.
/// Returns (fill: closed rings, coast: outlines without the antimeridian cuts).
pub fn land_paths(project: impl Fn(f64, f64) -> (f64, f64)) -> (String, String) {
    use std::fmt::Write;
    let on_antimeridian = |lon: f64| lon.abs() >= 179.99;
    let (mut fill, mut coast) = (String::new(), String::new());
    for ring in land() {
        for (k, (lon, lat)) in ring.iter().enumerate() {
            let (x, y) = project(*lon, *lat);
            let _ = write!(fill, "{}{:.2},{:.2}", if k == 0 { 'M' } else { 'L' }, x, y);
        }
        fill.push('Z');
        let mut previous: Option<f64> = None;
        for (lon, lat) in ring.iter().chain(ring.first()) {
            let cut = previous.is_some_and(|prev| on_antimeridian(prev) && on_antimeridian(*lon));
            let (x, y) = project(*lon, *lat);
            let pen = previous.is_some() && !cut;
            let _ = write!(coast, "{}{:.2},{:.2}", if pen { 'L' } else { 'M' }, x, y);
            previous = Some(*lon);
        }
    }
    (fill, coast)
}

/// Orthographic projection centered on (lon0, lat0), degrees.
#[cfg_attr(not(feature = "pdf"), allow(dead_code))]
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
/// azimuthal equidistant projection on the mean sphere (local and regional maps,
/// valid at the poles). Points behind the horizon are meaningless, see [orthographic].
#[cfg_attr(not(feature = "pdf"), allow(dead_code))]
pub fn local_plan(lon0: f64, lat0: f64, lon: f64, lat: f64) -> (f64, f64) {
    const EARTH_RADIUS_M: f64 = 6_371_008.8;
    let (l0, p0, l, p) = (
        lon0.to_radians(),
        lat0.to_radians(),
        lon.to_radians(),
        lat.to_radians(),
    );
    let cos_c = p0.sin() * p.sin() + p0.cos() * p.cos() * (l - l0).cos();
    let x = p.cos() * (l - l0).sin();
    let y = p0.cos() * p.sin() - p0.sin() * p.cos() * (l - l0).cos();
    // angular distance, accurate for close points (sin c = |(x, y)|)
    let rho = (x * x + y * y).sqrt();
    let k = if rho > 0.0 {
        rho.atan2(cos_c) / rho
    } else {
        1.0
    };
    (EARTH_RADIUS_M * k * x, EARTH_RADIUS_M * k * y)
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
        assert!((n - 111.195).abs() < 1e-3 && e.abs() < 1e-9);
        // equidistant: 10° of arc along a meridian, across the pole
        let (_, n) = local_plan(0.0, 85.0, 180.0, 85.0);
        assert!((n - 10.0f64.to_radians() * 6_371_008.8).abs() < 1.0);
    }
}
