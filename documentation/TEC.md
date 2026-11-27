TEC maps (IONEX)
================

The `tec` command (`ionex` feature) analyzes the TEC maps of IONEX files: the vertical
Total Electron Content (TECu, 10¹⁶ electrons/m²) over a latitude / longitude grid, map after
map. It also interpolates the maps in time, to synthesize new IONEX files.

To estimate TEC maps from observations (your own receivers), see the
[`obs2tec`](Obs2Tec.md) command: the IONEX files it formats are analyzed by `tec` like any
other IONEX.

```bash
cargo install rinex-cli --features ionex,pdf
```

| Option | Description |
|--------|-------------|
| `--region NAME:LAT_MIN,LAT_MAX,LON_MIN,LON_MAX` | Additional region of the statistics (repeat as needed). Default regions: global, North / South high latitudes (> 60°), North / South mid latitudes (30° to 60°), equatorial (± 30°) |
| `--upsample FACTOR` | FACTOR times as many maps (2: twice as many), intermediate maps interpolated in time |
| `--resample PERIOD` | One map every PERIOD (for example `"30 min"`) |
| `--interpolation rotated\|linear\|nearest` | Temporal interpolation (default: `rotated`, IONEX specifications) |
| `--no-ionex` | Do not synthesize the IONEX file of the resampled maps |
| `--gzip` | Gzip compressed IONEX output |

## Temporal interpolation

IONEX maps are quantized in time (one map every 1 or 2 hours) and in space (a 2.5° × 5° grid
for instance). Between two maps E0 (T0) and E1 (T1), the TEC at (φ, λ) is interpolated:

| `--interpolation` | Formula |
|-------------------|---------|
| `nearest` | E0 or E1, the nearest in time |
| `linear` | (T1 − t)/(T1 − T0) E0(φ, λ) + (t − T0)/(T1 − T0) E1(φ, λ) |
| `rotated` (default) | (T1 − t)/(T1 − T0) E0(φ, λ + Ω(t − T0)) + (t − T0)/(T1 − T0) E1(φ, λ + Ω(t − T1)), Ω = 360°/day |

The rotated strategy rotates both maps around the Earth axis to follow the Sun, which drives
the ionosphere: it is the strategy recommended by the IONEX specifications. Spatial
interpolation (rotated coordinates) is bilinear.

GNSS-Qc streams every node of every map (native and interpolated) as a point; rinex-cli
accumulates them and formats a new IONEX file, with the header of the original product and
the new temporal axis, in `WORKSPACE/<session>/IONEX/`.

## Example: interpolation ×2, then analysis of the synthesized IONEX

CKMG0080.09I (CODE, 2009-01-08) provides 13 worldwide maps, one every 2 hours.

1. Interpolate the maps ×2 (one map every hour), synthesize the new IONEX, report:

```bash
rinex-cli \
    -f data/IONEX/V1/CKMG0080.09I.gz \
    --session CKMG0080_x2 --report html,pdf \
    tec --upsample 2 --region "Europe:35,70,-10,30"
```

```text
WORKSPACE/CKMG0080_x2/
├── IONEX/CKMG0080_01H.09I   synthesized IONEX: 25 maps, one every hour
├── report.html              statistics, time series, maps (slider over the 25 maps)
├── report.pdf
├── summary.json
└── data/tec_maps.csv        statistics of every map, per region (tec_points.csv with --raw)
```

2. Inject the synthesized IONEX back into rinex-cli: TEC analysis and visualization.

```bash
rinex-cli \
    -f WORKSPACE/CKMG0080_x2/IONEX/CKMG0080_01H.09I \
    --session CKMG0080_01H_tec --report html,pdf \
    --title "TEC maps: CKMG0080_01H.09I (CODE 2009-01-08, synthesized x2)" \
    tec --region "Europe:35,70,-10,30"
```

Results:

- 25 maps (13 maps of the product, 12 interpolated), 5183 nodes each (71 latitudes × 73
  longitudes, 350 km);
- global mean TEC 10.6 TECu over the day (solar minimum), maximum 25.5 TECu at
  (−10°, −145°), at 00:00 UTC: the maximum follows the Sun westward, map after map;
- Europe (January night and winter): flat at 9.2 TECu, the floor of the CODE model;
- the synthesized IONEX is a valid IONEX file: injected back, it gives the same statistics
  (within 0.012 TECu: the IONEX format stores 0.1 TECu).

The same with CKMG0090.21I (2021-01-09, one map every hour), one map every 30 minutes:

```bash
rinex-cli -f data/IONEX/V1/CKMG0090.21I.gz tec --resample "30 min"
```

49 maps (24 interpolated), `IONEX/CKMG0090_30M.21I`; maximum 27.2 TECu at (−2.5°, −150°).

## Report

| Section | Content |
|---------|---------|
| Summary | maps (product, interpolated), interval, grid, layer altitude |
| Regions | mean TEC, dispersion, minimum, maximum (location and epoch) per region |
| Time series | mean and maximal TEC per region; global mean, maps of the product and interpolated maps; location of the maximal TEC |
| Maps | color maps of the vertical TEC over the world map (land masses, coast lines: Natural Earth 1:110m, embedded): every map in the HTML report (slider), one map out of N in the PDF report |
| Global statistics | map after map |

## Notes

- Interpolation only applies between two consecutive maps of the same product: files of
  distinct periods (CKMG0080.09I and CKMG0090.21I) are not interpolated with each other.
- 3D maps: the first altitude layer is presented.
