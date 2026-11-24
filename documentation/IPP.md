Ionospheric Pierce Points
=========================

The `ipp` command (`ionex` feature) resolves, at each epoch, the point where the line of sight
receiver → satellite crosses the ionospheric shell (single layer model). The vertical TEC of an
ionosphere map, multiplied by the obliquity factor of that point, gives the slant TEC that delays
the signal.

Requirements: observation RINEX (the receiver position is read from the header), and orbital products
(navigation RINEX or SP3). With IONEX maps covering the session, the vertical TEC at each pierce
point is interpolated in the maps (rotated maps).

```bash
rinex-cli \
    -f rover=data/OBS/V3/2024_09_20_10_17_06.obs.gz \
    -f data/NAV/V3/2024_09_20_10_17_06.nav.gz \
    --session ipp_pedestrian \
    --report html,pdf \
    ipp
```

| Option | Description |
|--------|-------------|
| `--elevation-mask DEG` | Lines of sight below this elevation are discarded (default 10°) |
| `--shell-altitude KM` | Altitude of the ionospheric shell (default 450 km, IGS maps) |
| `--orbits auto\|precise\|broadcast` | Orbital products preference |
| `-c gps,gal` | Constellations (default: all) |

Outputs
=======

- `data/ipp.csv`: one pierce point per epoch and satellite (elevation, azimuth, latitude,
longitude, shell altitude, obliquity factor, orbital product)
- report, per receiver:
  - summary: pierce points, satellites, orbital states, extent of the pierce points and largest
  distance to the receiver
  - **regional map** of the pierce points: one track per satellite, colored by constellation (hue)
  then satellite (shade), labeled at its last point, and the receiver. HTML: azimuthal projection
  fitted on the points (coast lines, borders, graticule). PDF: azimuthal equidistant plan in
  kilometers, north up, land masses (Natural Earth 1:110m), graticule and scale bar.
  The azimuthal projection remains valid around the poles, where the pierce points of high latitude
  stations spread
  - **vertical TEC map** (IONEX): pierce points colored by the vertical TEC (TECu), and the vertical TEC
  over time, per satellite
  - **world map** (globe) locating the receiver and its pierce points
  - **lines of sight**: sky plot (azimuth, elevation) and timeline of the pierce points
  - table per satellite: points, elevation, mean obliquity factor, latitude and longitude ranges,
  mean vertical TEC
  - then the quality control of the observations (`--no-qc` removes it)

On the example above (Ny-Ålesund, Svalbard, 78.9° N), 52 satellites (GPS, Galileo, BeiDou, QZSS)
pierce the 450 km shell between 66° N and 89° N, up to 1450 km from the receiver: the tracks of the
northern satellites converge towards the pole.
