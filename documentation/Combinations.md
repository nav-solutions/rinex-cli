Signal combinations
===================

`combinations` (alias `comb`) combines the signals of each satellite, for each signal source
(Observation RINEX, CRINEX), powered by the GNSS-Qc `QcCombinationProcessor`:

| Combination | Formula (m) | Measures |
|-------------|-------------|----------|
| Geometry Free (GF), phase | `Lj − L1` | Ionospheric delay difference `I1 − Ij` + biases + ambiguities: ionosphere, cycle slips, scintillation |
| Geometry Free (GF), code | `C1 − Cj` | `I1 − Ij` + inter-frequency code biases |
| Ionosphere Free (IF), code and phase | `(f1² X1 − fj² Xj) / (f1² − fj²)` | Range without the first order ionosphere: what CPP and PPP navigate with |
| Code multipath (MP) | `MPi = Ci − Li − 2 (Li − Lj) / (αij − 1)`, `αij = (fi / fj)²` | Code multipath and noise: antenna environment, receiver |

```bash
rinex-cli \
    -f data/CRNX/V3/ESBC00DNK_R_20201770000_01D_30S_MO.crx.gz \
    -f data/NAV/V3/ESBC00DNK_R_20201770000_01D_MN.rnx.gz \
    --title "Signal combinations: ESBC00DNK, 2020-06-25" --report html,pdf \
    combinations
```

The signal combinations are also part of the **navigation reports** (`nav`: absolute
navigation SPP, CPP, PPP, and RTK) and of the **CGGTTS reports** (`cggtts`):
see [Navigation reports](#navigation-reports).

Only observations are required. The navigation RINEX is optional: it completes the quality
control of the observations that ends the report (sky view, ephemeris coverage, `--no-qc`
to skip it).

- [Signals](#signals)
- [Options](#options)
- [Report](#report)
- [Navigation reports](#navigation-reports)
- [Output files](#output-files)
- [Examples](#examples)
- [Limitations](#limitations)

## Signals

For each satellite, the signals are sorted per frequency band (RINEX band number). On each band,
one code and one phase are selected, civilian first (attributes `C`, `W`, `P`, `Y`, `X`, `Q`, `I`,
`L`..). The reference (signal 1) is the lowest band (L1, E1, B1C, or B1I for BeiDou), and
**every other band** is combined with it: GPS L1/L2 and L1/L5, Galileo E1/E5a, E1/E5b, E1/E5
(AltBOC), E1/E6, BeiDou B1I/B3I and B1I/B2I, QZSS, SBAS..

The GF conventions (phase `Lj − L1`, code `C1 − Cj`) are opposite because the ionosphere delays
the code and advances the phase: both measure `I1 − Ij`, negative and proportional to the slant
TEC (`I = 40.3 TEC / f²`), so they can be compared directly.

The code multipath is computed for **every code** (`C1C` and `C1W`, `C2W` and `C2L`..), with the
phase of its band and the phase of the reference band (or the next band, for the reference
band itself): `MP C1C (L2W)`, `MP C2W (L1C)`, `MP C5Q (L1C)`.. It contains a constant bias
(ambiguities) per continuous phase arc: an arc ends on a loss of lock (LLI), a data gap
(`--max-gap`) or a jump (`--jump`, undetected cycle slip). The report removes the mean of each
arc, and the RMS around it is the code multipath + noise.

Glonass (FDMA: each satellite has its own frequencies) is not supported.

## Options

| Option | Default | Description |
|--------|---------|-------------|
| `--gf`, `--if`, `--mp` | all of them | Combinations to compute |
| `--code-only`, `--phase-only` | code and phase | GF and IF: pseudo ranges or phase ranges only |
| `-c gps,gal` | all | Constellations |
| `--receivers ESBC,MOJN` | all | Signal sources (identity or alias, see `index`) |
| `--max-gap 600` | 600 s | Multipath: a longer data gap ends the phase arc |
| `--jump 5` | 5 m | Multipath: a larger jump ends the phase arc |

The global options apply as usual: time window (`--start`, `--end`), preprocessing
(`-P`: satellites, signals), outputs (`--data csv,jsonl`, `--report html,pdf`).

## Report

One section per signal source, starting with the formulas and their interpretation, then one
sub-section per constellation (`GPS (US): Geometry Free`, `GPS (US): Ionosphere Free`,
`GPS (US): code multipath`, `Galileo (EU): Geometry Free`..), so that the charts of one
constellation are kept together:

- **Geometry Free**: one table row and one chart per pair of frequencies.
  The phase GF is *leveled* on the code GF, pass after pass (median of code − phase over the
  pass): the curves combine the precision of the phase and the absolute level of the code, they
  describe `I1 − Ij` (ionosphere + inter-frequency biases) satellite by satellite. A pass ends on a
  10 minutes gap or a jump of the phase GF larger than 0.1 m (cycle slip). The table gives the
  dispersion of GF code − GF phase (code noise and multipath of the GF combination) and the
  range of the leveled GF.
- **Ionosphere Free**: IF code − IF phase, mean removed per pass: noise and multipath of the IF
  code, about three times the noise of the individual codes for L1/L2.
- **Code multipath**: per code / phase pair, satellites, samples, arcs, RMS, best and worst
  satellites (30 samples at least); a table of the RMS of each satellite for each code; charts of
  the multipath (arc mean removed) satellite by satellite.

Pairs observed by few satellites (incomplete tracking: L2/L5 when L1 is missing..) are listed in
the tables but not charted.

## Navigation reports

The `nav` and `cggtts` commands deploy the same analysis on the navigated signal sources, for
the constellations of the navigation (`-c`):

| Command | Signal combinations |
|---------|---------------------|
| `nav`, absolute (SPP, CPP, PPP) | the rover: `Signal combinations: ESBC (rover)` |
| `nav`, RTK (`--base`) | the rover and the base: `Signal combinations: ESBC (rover)`, `Signal combinations: MOJN (base)` |
| `cggtts` | the receiver: `Signal combinations: ESBC (receiver)` |

The sections follow the navigation results and precede the quality control of the observations.
They help to interpret the solutions: the Ionosphere Free combination is what CPP and PPP
navigate with (its code noise drives the convergence), the Geometry Free combination reveals
the ionospheric activity and the cycle slips that reset the phase ambiguities, the code multipath
qualifies the antenna environment of each receiver. CGGTTS tracks are just as sensitive: the
code multipath and the IF code noise are averaged by the 13 minutes tracks but bias the
time transfer of the low elevation tracks, and the GF combination tells the ionospheric
conditions of the session. `--no-combinations` removes them.

```bash
# RTK: combinations of the rover and of the base
rinex-cli \
    -f ESBC=data/CRNX/V3/ESBC00DNK_R_20201770000_01D_30S_MO.crx.gz \
    -f MOJN=data/CRNX/V3/MOJN00DNK_R_20201770000_01D_30S_MO.crx.gz \
    -f data/NAV/V3/ESBC00DNK_R_20201770000_01D_MN.rnx.gz \
    --report html,pdf \
    nav --method cpp --rover ESBC --base MOJN --static -c gps
```

## Output files

```text
WORKSPACE/<session>/
├── report.html
├── report.pdf
├── summary.json
└── data/
    ├── combinations.csv      epoch,receiver,sv,combination,lhs,rhs,value_m,arc,debiased_m
    └── multipath_arcs.csv    receiver,sv,code,phase,arc,start,end,samples,mean_m,rms_m
```

- `combinations.csv`: one line per epoch, satellite and combination (`GF`, `IF`, `MP`):
  `value_m = lhs − rhs` for GF, the IF combination of `lhs` (signal 1) and `rhs` (signal j),
  the raw multipath of code `lhs` with phase `rhs`, its arc number and its value minus the mean
  of its arc *so far* (real-time debiasing);
- `multipath_arcs.csv`: statistics of each multipath arc, when it ends. Subtract `mean_m` from the
  raw values of the arc for an exact debiasing.

## Examples

See [examples/COMBINATIONS](../examples/COMBINATIONS).

ESBC00DNK (Septentrio PolaRx5, 2020-06-25, 24h, 30 s), GPS L1/L2:

| | Satellites | Samples | Result |
|-|------------|---------|--------|
| GF L2W-L1C, C1C-C2W | 31 | 65871 | leveled GF from −7.3 to +2.6 m, GF code − GF phase σ 0.42 m |
| IF L1C-L2W, C1C-C2W | 31 | 65871 | IF code − IF phase σ 0.98 m |
| MP C1C (L2W) | 31 | 32773 | RMS 0.32 m, 83 arcs |
| MP C2W (L1C) | 31 | 32773 | RMS 0.31 m, 85 arcs |
| MP C5Q (L1C) | 14 | 14527 | RMS 0.27 m |

Code multipath RMS, three receivers:

| Signal source | Antenna, receiver | GPS MP C1C | GPS MP C2W / C2X | Galileo MP C1C (L5Q) | Galileo MP C5Q (L1C) |
|---------------|-------------------|------------|------------------|----------------------|----------------------|
| ESBC00DNK (2020-06-25, 30 s) | ASH701945E_M, PolaRx5 | 0.32 m | 0.31 m | 0.23 m | 0.32 m |
| MOJN00DNK (2020-06-25, 30 s) | LEIAR25.R4, PolaRx5 | 0.27 m | 0.26 m | 0.22 m | 0.19 m |
| Glacier station (2024-05-06, 1 s) | u-blox (low cost) | 0.44 m (L2X) | 0.43 m (C2X) | - | - |

The choke ring antenna of MOJN (LEIAR25) shows less code multipath than ESBC, the low cost
station on the glacier about 40 % more than the geodetic stations.

## Limitations

- Glonass is not supported (FDMA frequencies).
- Second order ionospheric effects are not modeled: the IF combination removes the first order only.
- The multipath is not presented as a function of the elevation (requires the satellite positions).
