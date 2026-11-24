RTK: differential navigation
============================

RTK (differential) navigation positions a *rover* relative to a *base*: a second signal source,
static, whose position is known. Both receivers observe the same satellites at the same epochs.

- single differences (satellite − pivot satellite, per site) cancel the receiver clocks
- double differences (rover − base) cancel the satellite clocks and orbital errors, and most of the
ionospheric and tropospheric delays, as long as the two sites are close

The solver estimates the **baseline** (base → rover vector) and the rover position is the base
position + the baseline. Consequences:

- RTK resolves the position only: there is no receiver clock (it is not presented in the report, and the
`clock_offset_s` column of `solutions.csv` is zero)
- the solutions are expressed in the frame of the base coordinates: an error on the base position is
transferred as is to the rover
- the residual errors grow with the baseline length (atmosphere, orbits): a few ppm

Requirements: the `navigation` feature, two observation RINEX (rover and base) sampled at the same
epochs, navigation RINEX, and precise products for RTK-PPP (SP3, Clock RINEX).
See also [PPP](./PPP.md) for the common navigation options.

Command line
============

1. Index your products, with an alias per station so that they are easy to designate:

```bash
rinex-cli \
    -f ESBC=data/CRNX/V3/ESBC00DNK_R_20201770000_01D_30S_MO.crx.gz \
    -f MOJN=data/CRNX/V3/MOJN00DNK_R_20201770000_01D_30S_MO.crx.gz \
    -f data/NAV/V3/ESBC00DNK_R_20201770000_01D_MN.rnx.gz \
    index
```

2. Navigate the rover, relative to the base:

```bash
rinex-cli \
    -f ESBC=data/CRNX/V3/ESBC00DNK_R_20201770000_01D_30S_MO.crx.gz \
    -f MOJN=data/CRNX/V3/MOJN00DNK_R_20201770000_01D_30S_MO.crx.gz \
    -f data/NAV/V3/ESBC00DNK_R_20201770000_01D_MN.rnx.gz \
    -f data/SP3/C/GRG0MGXFIN_20201770000_01D_15M_ORB.SP3.gz \
    -f data/CLK/V3/GRG0MGXFIN_20201770000_01D_30S_CLK.CLK.gz \
    --title "RTK-PPP ESBC/MOJN" --report html,pdf \
    nav --method ppp --rover ESBC --base MOJN --static -c gps
```

| Option | Role |
|--------|------|
| `--base PRODUCT` | Enables RTK, relative to this signal source (identity or alias, as listed by `index`) |
| `--base-position x,y,z` | Position of the base (ECEF, m). Default: its RINEX header (`APPROX POSITION XYZ` + antenna height) |
| `--rover PRODUCT` | Signal source to navigate. Default: the first signal source that is not the base |
| `--method spp\|cpp\|ppp` | RTK-SPP, RTK-CPP or RTK-PPP (see below) |
| `--reference x,y,z` | Reference position of the rover, to express the errors (ECEF, m). Default: its RINEX header |
| `--static`, `--profile` | User profile of the rover (dynamics of the navigation filter) |
| `-c gps` | Constellations. Prefer one constellation in RTK-CPP/PPP, see [below](#constellations) |
| `--cfg nav.json` | Configuration file: the technique may be described there, see [rtk_ppp.json](../examples/CONFIG/rtk_ppp.json) |

The base position matters: it is the origin of the rover solutions. The RINEX header position is an
*approximate* position: when you know survey grade coordinates of the base, pass them with `--base-position`.

Methods
=======

| Method | Double differences of | Ionosphere | Inputs |
|--------|-----------------------|------------|--------|
| RTK-SPP | one pseudo range (L1) | differential cancellation only | OBS ×2, NAV |
| RTK-CPP | iono free pseudo range (two frequencies) | cancelled | OBS ×2, NAV |
| RTK-PPP | iono free phase (two frequencies), ambiguities from wide lane / narrow lane rounding | cancelled | OBS ×2, NAV, SP3 + CLK recommended |

RTK-PPP uses the phase of the signal, whose noise is millimetric (versus decimetric for the code):
it is the most precise method. The ambiguities are rounded (float solution), they are not fixed with
a search algorithm (LAMBDA): do not expect the centimetric "fixed" RTK of real-time receivers on long
baselines.

Constellations
--------------

GNSS-RTK elects a single pivot satellite (the highest one) for all constellations, and only
differences measurements of identical carriers. In RTK-SPP, L1 and E1 share the same frequency,
so GPS and Galileo can be mixed. In RTK-CPP and RTK-PPP, the second frequency differs
(L2 versus E5a): the satellites of the other constellation than the pivot's cannot be differenced and are
dropped (`missing code-if combination`, `missing phase-if combination` errors). Over one day
ESBC/MOJN, GPS + Galileo RTK-PPP only uses 6.5 satellites on average, GPS alone 7.3.
`rinex-cli` warns about it: prefer one constellation (`-c gps`) for RTK-CPP/PPP.

Report
======

The navigation report of an RTK session presents, in addition to the usual errors, trajectory,
DOP and satellites charts:

- the navigation context: technique (`RTK, base "MOJN"`) and the reference baseline length
- an **RTK baseline** section:
  - base and base position (ECEF and geodetic)
  - estimated baseline (mean of the last 50 % of the solutions) and reference baseline (reference
  position of the rover − base position): length, east, north, up (local frame of the base)
  - baseline error: length, east, north, up, ppm of the baseline
  - a map of the baseline (base → mean rover solution) and of the rover solutions
  - the baseline in the local frame of the base (east, north in km, equal axes)
  - the baseline length error (estimated − reference) over time
- no receiver clock chart (RTK does not resolve it)

Example: ESBC/MOJN, 65 km
=========================

See [examples/RTK](../examples/RTK): the three methods are compared over 24 hours.
In short (GPS, after the first hour):

| | RTK-SPP | RTK-CPP | RTK-PPP |
|-|---------|---------|---------|
| Dispersion (1σ) east / north / up | 1.0 / 0.7 / 2.3 cm | 2.7 / 1.9 / 8.6 cm | 0.3 / 1.0 / 2.0 cm |
| Converged within 10 cm after | 32 min | 9h20 | 13 min |
| RMS 3D (versus the header position) | 24.5 cm | 23.7 cm | 27.9 cm |

RTK-PPP is the most precise and converges the fastest. The three methods share a ~2 dm offset with respect
to the RINEX header positions, which limits the accuracy assessment with these references.

Limitations
===========

- the base must be static (its position is fixed), the rover may move (`--profile`)
- rover and base should be sampled at the same epochs: the base observations are not interpolated
- float solution only: the ambiguities are not fixed
- antenna phase center models (ANTEX) are not applied to the navigation yet
- RTK is not compatible with the `cggtts` command (CGGTTS requires the receiver clock)
