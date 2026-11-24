RINEX-Cli
=========

[![Rust](https://github.com/nav-solutions/rinex-cli/actions/workflows/rust.yml/badge.svg)](https://github.com/nav-solutions/rinex-cli/actions/workflows/rust.yml)
[![Rust](https://github.com/nav-solutions/rinex-cli/actions/workflows/daily.yml/badge.svg)](https://github.com/nav-solutions/rinex-cli/actions/workflows/daily.yml)
[![crates.io](https://img.shields.io/crates/v/rinex-cli.svg)](https://crates.io/crates/rinex-cli)

[![MRSV](https://img.shields.io/badge/MSRV-1.92.0-orange?style=for-the-badge)](https://github.com/rust-lang/rust/releases/tag/1.92.0)
[![License](https://img.shields.io/badge/license-MPL_2.0-orange?style=for-the-badge&logo=mozilla)](https://github.com/rtk-rs/rinex-cli/blob/main/LICENSE)

`rinex-cli` is the reference application of the [NAV-Solutions framework](https://github.com/nav-solutions):
it deploys the [GNSS-Qc](https://github.com/nav-solutions/gnss-qc) engine on your data
and presents the results as data files and reports.

```text
 RINEX, CRINEX, SP3 ─► indexing ─► GNSS-Qc engine ─► output stream ─┬─► data/*.csv, results.jsonl, stdout
 (gzip or not)        (receivers,   (synchronization,               ├─► CGGTTS, GPX, KML
                       agencies,     quality control,               ├─► summary.json
                       aliases)      navigation..)                  └─► report.html, report.pdf
```

- Quality control: sampling, data gaps, signal strength, lock losses, ephemeris coverage
- Post processed navigation: SPP, CPP, PPP, RTK (`navigation` feature)
- CGGTTS solutions: common view time transfer (`cggtts` feature)
- Residual analysis: receiver A − receiver B, orbital product A − product B
- Ionospheric Pierce Points (`ionex` feature)
- File operations: filegen, merge, split, tbin, cbin, diff

## Install

```bash
cargo install rinex-cli                        # quality control, residuals, file operations
cargo install rinex-cli --features cggtts,pdf  # + navigation, CGGTTS, PDF reports
cargo install rinex-cli --all-features
```

| Feature | Description |
|---------|-------------|
| `navigation` | `nav` command: P.V.T solutions (GNSS-RTK). The heaviest option. `ppp` is a legacy alias |
| `cggtts` | `cggtts` command (implies `navigation`) |
| `ionex` | `ipp` command |
| `embed_ephem` | Embeds the ANISE dataset at build time: no Internet access at runtime |
| `pdf` | PDF reports (Typst engine, embedded fonts) |

## Quick start

1. Verify how your files are identified (nothing is processed):

```bash
rinex-cli -d data/CRNX/V3 -f data/NAV/V3/ESBC00DNK_R_20201770000_01D_MN.rnx.gz index
```

2. Run a command, results are written in `WORKSPACE/<session>/`:

```bash
rinex-cli \
    -f data/CRNX/V3/ESBC00DNK_R_20201770000_01D_30S_MO.crx.gz \
    -f data/NAV/V3/ESBC00DNK_R_20201770000_01D_MN.rnx.gz \
    -f data/SP3/C/GRG0MGXFIN_20201770000_01D_15M_ORB.SP3.gz \
    --report html,pdf --open \
    nav --method ppp --static
```

```text
WORKSPACE/ESBC00DNK_R_20201770000_01D_30S_MO/
├── report.html          interactive report
├── report.pdf           static report (pdf feature)
├── summary.json         configuration, inputs, statistics, end of session reports
└── data/
    ├── solutions.csv    one file per kind of result
    └── results.jsonl    complete output stream (--data jsonl)
```

## Inputs and indexing

Files are loaded with `-f FILE` and `-d DIRECTORY` (recursive, `--depth`), as many times as needed.
RINEX, CRINEX and SP3 are supported, gzip compressed or not (detected from the content:
file names do not matter).

Every file is identified (*indexed*) by GNSS-Qc. Files that share the same identity and type
are merged. Default identities:

| Product | Identity |
|---------|----------|
| Observations | `receiver:MODEL #SERIAL`, otherwise antenna, operator, file name |
| Navigation, meteo, clock | `agency:NAME`, otherwise file name |
| SP3 | `agency:NAME`, otherwise file name |

Customize the indexing per product type with `--obs-index`, `--nav-index`, `--sp3-index`,
`--meteo-index`, `--clk-index` (`receiver`, `antenna`, `operator`, `agency`, `filename`, `auto`,
`alias:NAME`), or give an alias to a file or a directory: `-f ALIAS=FILE`, `-d ALIAS=DIR`.
`--input-cfg cfg.json` loads the preferences (`QcInputConfig`) from a file.

Typical situations:

```bash
# two stations equipped with the same receiver model and no serial number:
# without aliases they would be merged
rinex-cli -f rover=ROVER.crx -f base=BASE.crx -f BRDC.rnx index

# rapid and final products of the same agency: by default they would be merged
rinex-cli -f rap=RAP.SP3 -f fin=FIN.SP3 residuals orbit --lhs sp3:rap --rhs sp3:fin

# or one product per file
rinex-cli --sp3-index filename -f FIN.SP3 -f RAP.SP3 index
```

Products are designated by their identity, as printed by `index`
(`--rover "receiver:SEPT POLARX5 #3047937"`, `--lhs agency:GRGS`), or by name when unique.

Preprocessing (`-G -R -E -C -J -I -S --CG`, `-P FILTER`, `-z`) applies to each file before indexing.
`--start` and `--end` restrict the processing window.

## Outputs

| Option | Description |
|--------|-------------|
| `-o, --output DIR` | Output folder (`$RINEX_WORKSPACE`, default `WORKSPACE`) |
| `--session NAME` | Session subfolder (default: primary file name) |
| `--report html,pdf,none` | Report formats (default `html`) |
| `--data csv,jsonl,none` | Data formats (default `csv`) |
| `--raw` | Also export the raw signal observations |
| `--stdout` | Stream the results on stdout (JSON lines), logs go to stderr |
| `--title`, `--open`, `-q` | Report title, open the report, quiet |

The HTML report is a single interactive page (plotly.js is loaded from its CDN). The PDF report
is static: it is meant to be archived, printed or delivered. Both are built from the same report
model. The JSON lines deserialize into `gnss_qc::prelude::QcOutputItem` (serde feature).

## Commands

| Command | Description |
|---------|-------------|
| `index` | Lists the input products, as indexed (`--json`) |
| `qc` | Quality control of the signal sources |
| `nav` | P.V.T solutions: `--method spp\|cpp\|ppp`, `--rover`, `--base` (RTK), `--orbits auto\|precise\|broadcast`, `-c gps,gal`, `--profile`, `--static`, `--clock`, `--apriori`, `--reference`, `--cfg nav.json`, `--anise-dir`, `--gpx`, `--kml` |
| `cggtts` | CGGTTS solutions, same navigation options + `--station`, `--frame`, `--min-track`, `--solutions` |
| `residuals obs` | Receiver A − receiver B: `--lhs`, `--rhs`, `--all`, `--tolerance` |
| `residuals orbit` | Product A − product B: `--lhs`, `--rhs` (`sp3`, `sp3:PRODUCT`, `nav`, `nav:PRODUCT`), `--sampling`, `--max-age`, `-c` |
| `ipp` | Ionospheric Pierce Points: `--elevation-mask`, `--shell-altitude`, `--orbits`, `-c` |
| `filegen` | Writes the (preprocessed) products as RINEX / SP3 |
| `merge FILE` | Merges a file into the product of the same kind |
| `split EPOCH` | Splits the products at this epoch |
| `tbin DURATION` | Batch of files of this duration |
| `cbin` | One file per constellation (`--timescale`) |
| `diff FILE` | Observation RINEX (A) − FILE (B) |

`rinex-cli <command> --help` describes every option.

### ANISE dataset

Navigation requires the ANISE dataset (planetary ephemeris and constants). By default, it is
downloaded on first use and cached. Offline deployments: build with `embed_ephem`, or provide
local files with `--anise-dir DIR` (`$GNSS_QC_ANISE_DIR`): `de440s.bsp` and `pck11.pca`.

## Migrating from v0.13

| v0.13 | v0.14 |
|-------|-------|
| `--fp FILE` | `-f FILE` |
| `-f` (force report) | removed: reports are always generated |
| `-w FOLDER` | `-o FOLDER` |
| `-o NAME` (output name) | `--session NAME`, `--title` |
| `--sum`, `--jpl-bpc` | removed |
| `--rx-ecef x,y,z` (km) | `nav --apriori x,y,z` (m) or `--reference x,y,z` (m) |
| `ppp` | `nav --method spp\|cpp\|ppp` |
| `rtk --fp BASE` | `-f base=BASE nav --base base` |
| `ppp --cggtts` | `cggtts` |
| `ppp -c cfg.json` (GNSS-RTK `Config`) | `nav --cfg nav.json` (GNSS-Qc `QcNavigationConfig`) |
| `--nav healthy` | not supported yet |
| `filegen --csv` | `qc --raw --data csv` |

Clock RINEX, meteo RINEX and IONEX are loaded but not processed yet: GNSS-Qc does not
convert them into engine sources.

Special Thanks
==============

These tools would not exist without the great libraries written by C. Rabotin, 
[check out his work](https://github.com/nyx-space).  

Some features would not exist without the invaluable help of J. Lesouple, through
our countless discussions. Check out his 
[PhD manuscript (french)](http://perso.recherche.enac.fr/~julien.lesouple/fr/publication/thesis/THESIS.pdf?fbclid=IwAR3WlHm0eP7ygRzywbL07Ig-JawvsdCEdvz1umJJaRRXVO265J9cp931YyI)

## Licensing

This application is part of the [RTK-rs framework](https://github.com/rtk-rs) which
is delivered under the [Mozilla V2 Public](https://www.mozilla.org/en-US/MPL/2.0) license.
