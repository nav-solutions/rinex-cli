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
epochs (see [aligning the epochs](#aligning-the-rover-epochs)), navigation RINEX, and precise products
for RTK-PPP (SP3, Clock RINEX).
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

Aligning the rover epochs
=========================

The engine presents two receivers together **when they sample the same epoch**. Reference stations
sample on the second (08:18:00.000). Low cost receivers time tag their observations in their own time
base, a few milliseconds off: 08:17:50.989, 08:17:51.987.. Without alignment, the base is never
available at the rover epochs, and every epoch fails with `rtk base missing`.

| Option | Role |
|--------|------|
| `--snap-epochs` | Moves each observation epoch to the nearest second and corrects the observations (receiver clock jump): pseudo ranges C + c·Δt, phase ranges L + f·Δt (Glonass: frequency of the channel). Doppler and SSI are unchanged |
| `--align-sampling PERIOD` | Keeps the observation epochs that are multiples of `PERIOD` (`30s`, `"30 s"`, `30`), counted from the GPS time origin: a 1 Hz rover keeps the instants of a 30 s base |

The geometry is not modified: the transmission time (t_rx − C/c) is preserved, the receiver clock
estimate absorbs Δt, and the double differences cancel it (verified on absolute CPP: identical
positions, clock offsets shifted by exactly Δt). Unlike a decimation (`-P "decim:30 s"`, which counts
from the first epoch of each file), `--align-sampling` retains the same instants in every file.
`--align-sampling` is optional: without it, the rover epochs without base observations are reported
as `rtk base missing`.

Report
======

The navigation report of an RTK session presents, in addition to the usual errors (or displacements,
for a moving rover), DOP and satellites charts:

- the navigation context: technique (`RTK, base "MOJN"`) and the reference baseline length
- an **RTK baseline** section:
  - base and base position (ECEF and geodetic)
  - estimated baseline (mean of the last 50 % of the solutions) and reference baseline (reference
  position of the rover − base position): length, east, north, up (local frame of the base)
  - baseline error: length, east, north, up, ppm of the baseline
  - a map of the rover trajectory, colored by the time elapsed, with the base and the baseline
  (base → mean of the last 50 % of the solutions, or last solution for a moving rover). HTML:
  OpenStreetMap tiles, zoomed on the rover and the base. PDF: local plan (meters or kilometers,
  north up, scale bar)
  - a world map (orthographic globe) locating the rover and the base
  - the baseline in the local frame of the base (east, north in km, equal axes)
  - the baseline length error (estimated − reference) over time
- a sky plot and a visibility timeline of the satellites used by the solver
- no receiver clock chart (RTK does not resolve it)

A moving rover (`--profile pedestrian`, `car`..) has no reference position, unless `--reference` is
given: its solutions are presented as displacements relative to the first solution, with the
distance travelled, the speed and the altitude range, and the baseline length over time.

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

Example: pedestrian, NYA2, 6 to 8.5 km
======================================

A walk around Ny-Ålesund (Svalbard), recorded by a u-blox receiver (GPS L1 C/A + L2C, Galileo,
BeiDou, Glonass, 1 Hz), navigated relative to the IGS station NYA2 (Septentrio PolaRx5, 30 s),
2024-09-20, 08:18 → 11:00 GPST. See [examples/RTK](../examples/RTK): `pedestrian_rtk_{spp,cpp,ppp}.sh`.

```bash
rinex-cli \
    -f rover=data/OBS/V3/2024_09_20_10_17_06.obs.gz \
    -f NYA2=data/CRNX/V3/NYA200NOR_R_20242640000_01D_30S_MO.crx.gz \
    -f data/NAV/V3/2024_09_20_10_17_06.nav.gz \
    --snap-epochs --align-sampling 30s \
    --report html,pdf \
    nav --method ppp --rover rover --base NYA2 --profile pedestrian -c gps --gpx --kml
```

- the rover time tags are 10 to 13 ms early (08:17:50.989): `--snap-epochs` aligns its 9770 epochs, and
`--align-sampling 30s` keeps the 326 epochs of the base sampling
- broadcast ephemerides only (recorded by the rover): no precise product for that day
- the rover has no reference position: the report presents displacements, the trajectory map and the
baseline length over time (6.1 → 8.5 km). The three methods measure the same walk: 8.4 to 8.7 km,
0.9 m/s on average, from 33 to ≈ 200 m of altitude

GPS only (see [constellations](#constellations)), pedestrian profile. There is no ground truth: the methods
are compared with each other, and the smoothness of the track is measured by the second difference
of consecutive positions (30 s apart), which also contains the true changes of direction of the walker.

| | RTK-SPP | RTK-CPP | RTK-PPP |
|-|---------|---------|---------|
| Solutions | 325 / 326 (100 %) | 322 / 326 (99 %) | 216 / 326 (66 %) |
| Satellites (mean) | 8.6 | 7.9 | 7.6 |
| Track interruptions | 1 | 1 | 46 |
| Second difference, horizontal: median / 95 % | 2.7 / 8.2 m | 2.8 / 8.2 m | 3.3 / 58 m |
| Second difference, vertical: median / 95 % | 0.3 / 1.4 m | 0.6 / 4.8 m | 0.4 / 2.2 m |
| Horizontal difference with RTK-SPP: median / 95 % | | 0.8 / 2.8 m | 5.8 / 72 m |

What this tells us:

- **RTK-SPP and RTK-CPP agree within a meter** (median), and both track the walker at every epoch.
Over a few kilometers, the L1 double differences already cancel most of the ionosphere: RTK-SPP is
the most stable solution of this session. RTK-CPP amplifies the code noise (iono free combination),
mostly vertically.
- **RTK-PPP is the ultimate method, but not on this session.** It relies on the continuity of the phase:
every interruption reinitializes the ambiguities, and the solution needs minutes to converge again.
The walker's receiver loses the phase of one satellite or another every few minutes
(`not enough candidates match pre-fit criteria`: 97 epochs), the 30 s base sampling hides
the cycle slips, and the track is interrupted 46 times: after each interruption, the solution jumps
by tens of meters (up to 400 m) and converges back towards the code solutions in 5 to 10 minutes.
It never stays continuous long enough for the float ambiguities to settle (≈ 2.4 m from RTK-SPP after
5 minutes of continuity).
- To get the precision of the phase on such a session: a base sampled at 1 Hz (the rover keeps all its
epochs, cycle slips become detectable), an antenna with a ground plane (fewer losses of lock),
precise products, and ambiguity fixing (not supported yet, see [limitations](#limitations)).

Mixing GPS and Galileo is possible in RTK-SPP (`-c gps,gal`: 326 / 326 solutions, L1 and E1 share
the same frequency), not in RTK-CPP and RTK-PPP (the rover tracks E1 + E5b, GPS L1 + L2).

Limitations
===========

- the base must be static (its position is fixed), the rover may move (`--profile`)
- rover and base must be sampled at the same epochs: the base observations are not interpolated, use
`--snap-epochs` and `--align-sampling` for low cost receivers. The RTK sampling is that of the base
- RTK-PPP needs continuous phase tracking: every interruption reinitializes the ambiguities
- float solution only: the ambiguities are not fixed
- antenna phase center models (ANTEX) are not applied to the navigation yet
- RTK is not compatible with the `cggtts` command (CGGTTS requires the receiver clock)
