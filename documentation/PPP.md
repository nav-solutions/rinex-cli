Post Processed Positioning
==========================

The `nav` command (`navigation` feature) deploys the GNSS-RTK solver on your products and resolves
P.V.T solutions of one signal source (the *rover*). It is dedicated to post processing: the inputs are files
(RINEX, CRINEX, SP3, Clock RINEX..). It replaces the `ppp` mode of rinex-cli v0.13 (the `ppp` cargo feature remains as a legacy alias of `navigation`).

```bash
rinex-cli \
    -f data/CRNX/V3/ESBC00DNK_R_20201770000_01D_30S_MO.crx.gz \
    -f data/NAV/V3/ESBC00DNK_R_20201770000_01D_MN.rnx.gz \
    -f data/SP3/C/GRG0MGXFIN_20201770000_01D_15M_ORB.SP3.gz \
    -f data/CLK/V3/GRG0MGXFIN_20201770000_01D_30S_CLK.CLK.gz \
    --report html,pdf \
    nav --method ppp --static
```

Techniques
==========

- **Absolute** navigation (default): position and receiver clock are resolved from one signal source
- **RTK** (differential) navigation, relative to a base station: `--base PRODUCT`.
See the [RTK documentation](./RTK.md)

Methods
=======

| `--method` | Measurements | Typical use |
|------------|--------------|-------------|
| `spp` | one pseudo range (single frequency) | basic receivers; the ionosphere is modeled |
| `cpp` (default) | two pseudo ranges, iono free combination | geodetic receivers, broadcast products |
| `ppp` | two pseudo ranges and two phases, iono free combination | geodetic receivers, precise products |

The selected method must be compatible with the observed signals: CPP and PPP require two frequencies
per satellite. A satellite that does not provide them is not used.

Input products
==============

The input products define the scenario:

- with observation and navigation RINEX only, you are in the "real time" scenario: broadcast orbits and clocks
- add SP3 (precise orbits) and Clock RINEX (precise clocks) for the post processed scenario, which
gives the best results you can obtain without a reference station.
`--orbits auto|precise|broadcast` selects the orbital products (`auto`: precise whenever they cover the satellite)
- the processing window is defined by the products, and can be reduced with `--start` / `--end`
- the navigation RINEX should cover your observations: a satellite without ephemeris (or precise state)
cannot contribute
- any number of files may be stacked: two daily RINEX of the same station are merged (48 hours)

Several signal sources may be loaded (`index` lists them): `--rover` selects the one to navigate.

Options
=======

| Option | Role |
|--------|------|
| `--method spp\|cpp\|ppp` | Navigation method |
| `--rover PRODUCT` | Signal source to navigate. Default: the first one |
| `-c gps,gal` | Constellations (gps, gal, bds, qzss). Default: GPS and Galileo |
| `--orbits auto\|precise\|broadcast` | Orbital products preference |
| `--profile`, `--static` | User profile (dynamics of the navigation filter): static, pedestrian (default), car, airplane, rocket |
| `--clock` | Receiver clock profile: quartz (default), oscillator, atomic, hydrogen-maser |
| `--apriori x,y,z\|header` | A priori position. Default: none, the solver surveys from scratch |
| `--reference x,y,z` | Reference position to express the errors. Default: the RINEX header of the rover |
| `--base PRODUCT`, `--base-position x,y,z` | RTK navigation, see [RTK](./RTK.md) |
| `--cfg nav.json` | Navigation configuration (GNSS-Qc `QcNavigationConfig`), may be partial, see [examples/CONFIG](../examples/CONFIG) |
| `--anise-dir DIR` | ANISE dataset (`GNSS_QC_ANISE_DIR`) |
| `--gpx`, `--kml` | Also format the solutions as GPX / KML tracks |

The command line options take precedence over the configuration file.
The preprocessing options (`-G`, `-E`.., `-P` filters) apply before the navigation:
this is how you focus on specific satellites or signals.

Outputs
=======

- `data/solutions.csv`: one solution per epoch (ECEF and geodetic position, velocity, clock offset, DOPs, satellites)
- report: errors with respect to the reference position (east, north, up), trajectory, receiver clock,
DOPs, satellites, and the RTK baseline in RTK
- `summary.json`: navigation context and end of session statistics

Examples
========

- [Absolute navigation examples](../examples/PPP)
- [RTK examples](../examples/RTK): RTK-SPP, RTK-CPP, RTK-PPP compared on a 65 km baseline
- [CGGTTS examples](../examples/CGGTTS): the `cggtts` command shares the navigation options
