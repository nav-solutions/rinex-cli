RTK: ESBC (rover) relative to MOJN (base)
==========================================

See [the RTK documentation](../../documentation/RTK.md).

Two static Danish stations, observed on 2020-06-25 (24h, 30s sampling):

| Station | Role | Receiver antenna | Reference position (RINEX header + antenna height) |
|---------|------|------------------|-----------------------------------------------------|
| ESBC (Esbjerg) | rover | ASH701945E_M | 55.493563°N, 8.456821°E |
| MOJN (Mojn) | base | LEIAR25.R4 | 54.944315°N, 8.805378°E |

Baseline: **65.046 km** (MOJN → ESBC: 22.03 km west, 61.20 km north, −0.33 km up).
Both stations being static, the header position of the rover is used as reference to measure
the accuracy, even though the navigation filter knows nothing about it (no `--apriori`).

| Script | Technique | Inputs |
|--------|-----------|--------|
| [rtk_spp.sh](./rtk_spp.sh) | RTK-SPP: double differences of one pseudo range (L1 C/A) | OBS ×2, NAV |
| [rtk_cpp.sh](./rtk_cpp.sh) | RTK-CPP: double differences of the iono free pseudo range (L1/L2) | OBS ×2, NAV |
| [rtk_ppp.sh](./rtk_ppp.sh) | RTK-PPP: double differences of the iono free phase (L1/L2), ambiguities rounded | OBS ×2, NAV, SP3, Clock RINEX (GRG final) |
| [rtk_cfg.sh](./rtk_cfg.sh) | RTK-PPP described by a configuration file ([rtk_ppp.json](../CONFIG/rtk_ppp.json)) | idem |

Each script writes `WORKSPACE/rtk_<method>_ESBC_MOJN/` (report.html, report.pdf, data/solutions.csv, summary.json).
The navigation report gains an *RTK baseline* section:

- base, base position, estimated and reference baselines (length, east/north/up), baseline error (m, ppm)
- map of the baseline (base → rover) and of the rover solutions
- baseline in the local frame of the base (east/north, km, equal axes)
- baseline length error (estimated − reference) over time

The report then presents the signal combinations of the rover and of the base (Geometry Free,
Ionosphere Free, code multipath, per constellation, see [signal combinations](../../documentation/Combinations.md)).

The receiver clock chart is not presented: RTK does not resolve the receiver clock (it cancels out in the double differences).

RTK-SPP versus RTK-PPP
======================

GPS only (see [why](../../documentation/RTK.md#constellations)), static profile,
24h (2858 solutions), statistics after the first hour. Errors are expressed in the local frame of the
reference position of the rover.

| | RTK-SPP | RTK-CPP | RTK-PPP |
|-|---------|---------|---------|
| Measurement | L1 C/A code | L1/L2 code (iono free) | L1/L2 phase (iono free) |
| Orbits & clocks | broadcast | broadcast | GRG final (SP3 + CLK) |
| Satellites (mean) | 7.3 | 7.3 | 7.3 |
| Mean error east / north / up | −21.2 / +5.8 / −10.5 cm | −20.5 / −4.0 / −6.2 cm | −18.9 / +11.0 / −17.2 cm |
| Dispersion (1σ) east / north / up | 1.0 / 0.7 / 2.3 cm | 2.7 / 1.9 / 8.6 cm | **0.3 / 1.0 / 2.0 cm** |
| RMS 2D / 3D | 22.0 / 24.5 cm | 21.2 / 23.7 cm | 21.9 / 27.9 cm |
| Max 3D error (24h, convergence included) | 0.44 m | 0.78 m | 0.85 m |
| Converged within 10 cm of the final solution after | 32 min | 9h20 | **13 min** |
| Converged within 5 cm of the final solution after | 3h27 | 10h51 | 4h44 |
| Baseline length error | +12.6 ± 0.5 cm (1.9 ppm) | +3.2 ± 2.3 cm (0.5 ppm) | +16.7 ± 0.9 cm (2.6 ppm) |

What this tells us:

- **Precision**: RTK-PPP is the most stable solution: 3 mm (east) and 1 cm (north) of dispersion
over 23 hours, and the fastest to settle (13 minutes to stay within 10 cm). The phase is two orders of
magnitude less noisy than the code: this is what RTK-PPP brings.
- RTK-SPP is surprisingly good here: the static filter averages the L1 code over hours, and over
65 km the double differences already cancel most of the ionosphere. RTK-CPP is the noisiest:
the iono free combination amplifies the code noise (×3) and multipath.
- **Accuracy**: the three methods agree on a common offset of about 2 dm (mostly east),
that no measurement type removes. It is therefore not a measurement noise but a systematic common to the
three solutions: the reference positions themselves (RINEX header `APPROX POSITION XYZ`,
which is not a geodetic coordinate), the antenna phase centers (two different antennas,
not modeled) and the differential troposphere over 65 km. With these references, the accuracy
cannot be assessed below 2 dm: use survey grade coordinates (`--base-position`, `--reference`)
to go further.
- 65 km is a *long* baseline for RTK: ambiguity resolution is not attempted (float solution,
ambiguities rounded), real-time "fixed" centimetric RTK is out of reach on such a baseline.

Pedestrian rover (NYA2, 2024-09-20)
===================================

A walk around Ny-Ålesund, recorded by a u-blox receiver (1 Hz), relative to NYA2 (Septentrio, 30 s),
6 to 8.5 km baseline, broadcast ephemerides. See the [RTK documentation](../../documentation/RTK.md#example-pedestrian-nya2-6-to-85-km)
for the results.

| Script | Technique |
|--------|-----------|
| [pedestrian_rtk_spp.sh](./pedestrian_rtk_spp.sh) | RTK-SPP: L1 C/A code |
| [pedestrian_rtk_cpp.sh](./pedestrian_rtk_cpp.sh) | RTK-CPP: L1/L2C code (iono free) |
| [pedestrian_rtk_ppp.sh](./pedestrian_rtk_ppp.sh) | RTK-PPP: L1/L2C phase (iono free) |

The rover epochs are aligned on those of the base: `--snap-epochs --align-sampling 30s`.
Each script writes `WORKSPACE/rtk_<method>_pedestrian_NYA2/`: the report presents the trajectory on a map
(colored by time, with the base and the baseline), a world map, the displacements, the baseline length
over time, the sky plot and the visibility timeline of the satellites used. `data/rover.gpx` and
`data/rover.kml` open in any mapping application.
