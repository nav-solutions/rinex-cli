TEC from the observations (obs2tec)
===================================

The `obs2tec` command (`ionex` feature) estimates the Total Electron Content (TEC) from the
observations of one receiver or of a network of receivers, then builds TEC maps:

- the vertical TEC at the ionospheric pierce point of every line of sight, epoch after epoch,
- one map every interval (`--interval`, one hour by default) over a latitude / longitude grid,
- projected on regions (`--region`) and points of interest (`--point`), map after map,
- formatted as a standard IONEX file with `--ionex`.

Not to be confused with the [`tec`](TEC.md) command, which analyzes the maps of existing IONEX
files (published by the IGS analysis centers for example). Both are complementary:
the IONEX formatted by `obs2tec` is a standard IONEX, that can be injected back into rinex-cli
and analyzed by `tec` like any other IONEX ([example](#example-a-network-of-two-receivers)).

```bash
cargo install rinex-cli --features ionex,pdf
```

> [!WARNING]
> **Uncalibrated TEC.** The differential code biases (DCB) of the satellites and of the
> receivers are not corrected (yet): every TEC estimate is biased by the DCB of its receiver
> and of its satellite, a few TECu to tens of TECu. Read [the limitations](#limitations-differential-code-biases)
> before using the maps.

## Requirements

- dual frequency code **and** phase observations (one or more receivers, Observation RINEX),
- orbits: broadcast ephemerides (Navigation RINEX) or precise orbits (SP3), see `--orbits`,
- the position of every receiver (RINEX header): the pierce points depend on it.

When IONEX files are loaded too, the vertical TEC of their maps is interpolated at every
pierce point, for comparison (`VTEC − IONEX` in the report, `map_vertical_tecu` in the CSV).

## Options

| Option | Description |
|--------|-------------|
| `--elevation-mask DEGREES` | Elevation mask (default: 15°) |
| `--shell-altitude KM` | Altitude of the single layer (default: 450 km) |
| `--orbits auto\|precise\|broadcast` | Orbital products preference (default: precise products whenever they cover the satellite) |
| `-c`, `--constellations` | Constellations, comma separated (default: all). See [the limitations](#limitations-differential-code-biases) |
| `--receivers` | Receivers of the network, comma separated (default: every signal source) |
| `--max-gap PERIOD` | A data gap longer than this ends a phase arc (default: 10 min) |
| `--slip-threshold METERS` | A jump of the geometry free phase larger than this ends a phase arc (default: 0.1 m) |
| `--interval PERIOD` | One map every PERIOD (default: `"1 h"`) |
| `--grid network\|world\|LAT0,LAT1,LON0,LON1` | Extent of the maps: bounding box of the pierce points (default), the world, or these bounds (degrees) |
| `--dlat DEGREES`, `--dlon DEGREES` | Grid spacing (default: 2.5° × 5°, like the IGS maps) |
| `--radius DEGREES` | Influence radius of a pierce point (default: the largest grid spacing) |
| `--min-samples COUNT` | Minimal number of pierce points of a grid node, empty node otherwise (default: 3) |
| `--no-rotation` | Earth-fixed maps (default: Sun-fixed frame, IONEX specifications) |
| `--region NAME:LAT_MIN,LAT_MAX,LON_MIN,LON_MAX` | Additional region of the statistics (repeat as needed). The whole grid is always analyzed |
| `--point NAME:LAT,LON` | Point of interest: TEC projected on this point, map after map (repeat as needed) |
| `--ionex` | Formats the TEC maps as IONEX, in `WORKSPACE/<session>/IONEX/` |
| `--gzip` | Gzip compressed IONEX |

## Principle

### 1. Slant TEC, per line of sight

The ionospheric delay of a signal is `I = 40.3 TEC / f²` (meters, TEC in electrons/m², f in Hz).
With two frequencies f1 and fj, the geometry free combinations measure the TEC along the
line of sight (slant TEC, STEC), in TECu (10¹⁶ electrons/m²):

```text
k = f1² fj² / (40.3 (f1² − fj²)) / 10¹⁶          (9.52 TECu/m for GPS L1/L2)

STEC(code)  = k (Cj − C1)       absolute, but noisy (multipath: meters)
STEC(phase) = k (L1 − Lj)       precise (millimeters), but ambiguous
```

The phase is leveled onto the code over each continuous phase arc:

```text
STEC = STEC(phase) + < STEC(code) − STEC(phase) >arc  − k c (DCB(sat) + DCB(rx))
                                                        └──────── 0 (not corrected) ───────┘
```

An arc ends on a loss of lock, a data gap (`--max-gap`), or a jump of the geometry free
phase (cycle slip, `--slip-threshold`). GNSS-Qc streams a running (real time) leveling, then
the exact leveling of the arc once it ended: obs2tec applies the exact leveling to every
estimate of the arc.

### 2. Vertical TEC, at the pierce point

The single layer (thin shell) model concentrates the ionosphere at `--shell-altitude`.
The line of sight crosses the shell at the ionospheric pierce point (IPP, see
[pierce points](IPP.md)), with a zenith angle z' (sin z' = R/(R + H) sin z):

```text
VTEC = STEC × cos(z')       (IONEX mapping function: COSZ)
```

### 3. TEC maps

One map every interval Tk (round UTC epochs: 00:00, 01:00, ..), gathering the pierce points of
`[Tk − interval/2, Tk + interval/2[`:

- Sun-fixed frame (default): the ionosphere follows the Sun far more than the Earth. A pierce
  point observed at t is moved to the longitude `λ + 360° (t − Tk) / 1 day` of the map Tk,
  exactly like the rotated interpolation of the IONEX specifications (see [TEC maps](TEC.md)).
- every node of the grid is the inverse distance weighted (1/d²) mean of the vertical TEC of the
  pierce points within `--radius`, its RMS their weighted dispersion,
- a node with less than `--min-samples` pierce points is left empty (`9999` in IONEX).

The points of interest (`--point`) are projected exactly like the nodes. The regions
(`--region`) present the statistics of their nodes, map after map, like the `tec` command.

## Outputs

```text
WORKSPACE/<session>/
├── IONEX/<SESSION>_01H.20I        --ionex: the TEC maps (standard IONEX, 9 characters prefix)
├── report.html                    estimates, maps (slider), regions and points of interest
├── summary.json
└── data/
    ├── tec_estimates.csv          every estimate: pierce point, code / phase / leveled slant TEC,
    │                              running leveling, vertical TEC, input IONEX (if any)
    └── tec_arcs.csv               every phase arc: exact leveling, code noise
```

The IONEX header describes the processing: mapping function `COSZ`, elevation cutoff,
base radius, single layer altitude, number of stations and satellites, and comments stating
that the maps are **uncalibrated** (DCB not corrected).

The report presents, per receiver, the median vertical TEC, the fraction of negative estimates
(a signature of the DCB) and, when IONEX files are loaded, the median of `VTEC − IONEX`:
a constant offset per receiver is mostly its DCB.

## Example: a network of two receivers

ESBC00DNK (Esbjerg) and MOJN00DNK (Mojn), two Danish stations 65 km apart, 2020-06-25
(30 s observations, broadcast ephemerides). See [examples/OBS2TEC](../examples/OBS2TEC).

1. TEC estimates of both receivers (GPS L1/L2), one map every hour over the network,
   projected on Esbjerg, formatted as IONEX:

```bash
rinex-cli \
    -f data/CRNX/V3/ESBC00DNK_R_20201770000_01D_30S_MO.crx.gz \
    -f data/CRNX/V3/MOJN00DNK_R_20201770000_01D_30S_MO.crx.gz \
    -f data/NAV/V3/ESBC00DNK_R_20201770000_01D_MN.rnx.gz \
    --session DNK_TEC --report html,pdf \
    obs2tec -c GPS --ionex --point "Esbjerg:55.5,8.45"
```

```text
WORKSPACE/DNK_TEC/
├── IONEX/DNKTEC_01H.20I     25 maps (2020-06-25 00:00 to 2020-06-26 00:00 UTC), 2.5° × 5°
├── report.html
└── data/tec_estimates.csv, tec_arcs.csv
```

2. The formatted IONEX is a standard IONEX: inject it back into rinex-cli, and analyze it
   like any other IONEX (statistics per region, maps, temporal interpolation, comparison with
   other products..):

```bash
rinex-cli \
    -f WORKSPACE/DNK_TEC/IONEX/DNKTEC_01H.20I \
    --session DNK_TEC_maps --report html,pdf \
    tec --region "Denmark:54.5,57.8,8,13" --resample "30 min"
```

It can also be loaded next to observations, for example as the reference of the `ipp` command
(vertical TEC at the pierce points of another receiver).

### What to expect: limitations of this example

The maps of this example are **uncalibrated**: the differential code biases are not corrected.

- The two receivers see the same ionosphere (65 km apart), yet their median vertical TEC differs
  by about 12 TECu over the day (GPS, about +6 TECu for ESBC, −6 TECu for MOJN): this is
  essentially the difference of their receiver DCB (about 19 TECu along the line of sight,
  6 to 7 ns). Where the pierce points of both receivers overlap, the gridding averages
  estimates biased differently.
- Every GPS satellite adds its own DCB (a few ns, ± 10 TECu): a node takes the bias of the
  satellites seen at that hour, so the maps jump from one hour to the next when the
  constellation changes, and negative TEC values are common, especially at the edges of the
  network (low elevations, few satellites).
- Mixing constellations or signals makes it worse (the receiver bias depends on the signal
  pair): on the same day, without `-c GPS`, the median vertical TEC of ESBC is −4 TECu with
  Galileo E1/E5a and −44 TECu with BeiDou B1I/B3I, while MOJN shows +8 and 0 TECu.
  Restrict the estimation to one constellation (`-c GPS`) until the DCB are corrected.
- Two receivers cover a few hundred kilometers: most of the grid is empty (`9999`), and the
  nodes at the edges are extrapolated from a handful of low elevation pierce points.

These maps are therefore suited to the relative variations of the TEC (day / night cycle,
disturbances, scintillation studies, traveling ionospheric disturbances), not to absolute TEC,
nor to the correction of single frequency positioning.

## Limitations: differential code biases

Satellites and receivers delay the signals differently on each frequency (hardware delays).
The geometry free code measures `k (Cj − C1)`, which includes the differential code bias
`DCB = bias(j) − bias(1)` of the satellite and of the receiver: `k c DCB`, 2.85 TECu per ns
for GPS L1/L2. Typical values: a few ns for the satellites, up to tens of ns for the receivers.
The phase leveling transfers these biases onto the leveled phase: obs2tec sets them to 0.

With a single receiver, the receiver DCB is a constant offset of all its estimates. With a
network, the problems add up:

1. **Every receiver has its own DCB**: neighbor receivers disagree by tens of TECu, and the
   maps show discontinuities where their coverages overlap, or a mean of biased values.
2. **Every satellite has its own DCB**: the map value depends on the satellites in view,
   it jumps when a satellite rises or sets.
3. **The DCB depends on the signal pair**: C1C/C2W and C1W/C2W differ (P1 − C1 bias), so do
   GPS L1/L2, Galileo E1/E5a, BeiDou B1I/B3I.. Receivers tracking different codes, or a mix of
   constellations, are not consistent with each other. GLONASS adds inter frequency biases (one
   per frequency channel).
4. **Negative TEC**: the uncalibrated TEC can be negative, which IONEX consumers may reject.

The cure is the calibration of the DCB:

- **calibrated sources**: satellite DCB published by the analysis centers (CODE monthly DCB
  files, Bias SINEX), the DCB auxiliary block of the IGS IONEX files, the receiver DCB of the
  IGS stations. Satellite DCB are well determined (stable, constrained to a zero mean); receiver
  DCB are only available for the stations of the analysis centers,
- **joint estimation**: the IGS analysis centers estimate the maps (spherical harmonics) together
  with one DCB per satellite and per receiver, over the whole network and a whole day, the
  satellite DCB being constrained to a zero sum.

Other limitations, independent of the DCB:

- the single layer model is a projection: the obliquity factor is inaccurate at low elevations
  (raise `--elevation-mask` to 20° or 30° to reduce it), horizontal gradients are ignored, and
  GNSS signals also cross the plasmasphere (above the shell),
- short phase arcs are poorly leveled (code multipath does not average out),
- inverse distance weighting does not extrapolate: nodes far from any pierce point are empty,
  and the density of the network shapes the maps,
- the antimeridian is not crossed by `network` grids (use `--grid world`).
