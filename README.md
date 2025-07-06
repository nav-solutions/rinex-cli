RINEX-Cli
=========

[![Rust](https://github.com/rtk-rs/rinex-cli/actions/workflows/rust.yml/badge.svg)](https://github.com/rtk-rs/rinex-cli/actions/workflows/rust.yml)
[![Rust](https://github.com/rtk-rs/rinex-cli/actions/workflows/daily.yml/badge.svg)](https://github.com/rtk-rs/rinex-cli/actions/workflows/daily.yml)
[![crates.io](https://img.shields.io/crates/v/rinex-cli.svg)](https://crates.io/crates/rinex-cli)

[![MRSV](https://img.shields.io/badge/MSRV-1.82.0-orange?style=for-the-badge)](https://github.com/rust-lang/rust/releases/tag/1.82.0)
[![License](https://img.shields.io/badge/license-MPL_2.0-orange?style=for-the-badge&logo=mozilla)](https://github.com/rtk-rs/rinex-cli/blob/main/LICENSE)

`rinex-cli` is a command line tool to post-process RINEX and SP3 files.

Because RINEX and SP3 cover many applications, `rinex-cli` can be used for many applications. 
The most important being:

- File management
  - patching & reworking (for example: zero repair)
  - splitting: create a batch of files
  - transposing: to a single timescale, into a batch of timescales
  - reformat: export to CSV
- Analysis 
  - generate high level reports
- Synthesis
  - generate RINEX (and soon SP3) from provided products
-  post-processed navigation (on `ppp` feature only) because it integrates a complete
  - RINEX-Cli integrates a complete [P.V.T solution solver](https://github.com/rtk-rs/gnss-rtk)
  - absolute navigation with `ppp` command line
  - differential navigation with `rtk` command line, using one ground station
- CGGTTS solutions solver (`ppp --cggtts` mode) by combining the `ppp` **and** `cggtts` options

<div align="center">
    <p>
        Static surveying of a geodetic marker:
    </p>
    <a href=https://github.com/rtk-rs/rinex-cli/blob/main/plots/front-page/map.png>
        <img src=https://github.com/rtk-rs/rinex-cli/blob/main/plots/front-page/map.png alt="Plot">
    </a>
</div>

<div align="center">
    <p>
        Errors from the geodetic marker (CPP, Galileo E1+E5)
    </p>
    <a href=https://github.com/rtk-rs/rinex-cli/blob/main/plots/front-page/coordinates.png>
        <img src=https://github.com/rtk-rs/rinex-cli/blob/main/plots/front-page/coordinates.png alt="Plot">
    </a>
</div>

<div align="center">
    <p>
        `PPP` solver applied to a roaming session (pedestrian profile) around station `NYA100NOR`
    </p>
    <a href=https://github.com/rtk-rs/rinex-cli/blob/main/plots/front-page/roaming-ppp1.png>
        <img src=https://github.com/rtk-rs/rinex-cli/blob/main/plots/front-page/roaming-ppp1.png alt="Plot">
    </a>
</div>

<div align="center">
    <p>
        REFSYS resolved from PPP+CGGTTS (CPP, Galileo E1+E5)
    </p>
    <a href=https://github.com/rtk-rs/rinex-cli/blob/main/plots/front-page/refsys.png>
        <img src=https://github.com/rtk-rs/rinex-cli/blob/main/plots/front-page/refsys.png alt="Plot">
    </a>
</div>

## Download the tool

You can download the latest version from [the release portal](https://github.com/rtk-rs/rinex-cli/releases).

Two version of the toolbox are automatically released:

* the smallest is published under the name `rinex-cli` and is only compatible with file management
* the heaviest allows all features, including the NAV PVT Solver and NAV CGGTTS solutions solver, and is published
under the name `rinex-cli-ppp`

## Install from Cargo

You can directly install the tool from Cargo with internet access:

```bash
cargo install rinex-cli
```

## Build from sources

Download the version you are interested in:

```bash
git clone https://github.com/rtk-rs/rinex-cli
```

If you're interested in running one of the demos, you should enable all features
and should download our example data set:

```bash
git clone --recurse-submodules https://github.com/rtk-rs/rinex-cli
```

To install in the default location, in this example activating all features,
you should run this:

```bash
cargo install --all-features --path .
rinex-cli --version

which rinex-cli
${HOME}/.cargo/bin/rinex-cli
```

## File formats & revisions

`rinex-cli` supports 

- all formats & revisions supported by [the RINEX parser](https://github.com/rtk-rs/rinex)
- all revisions supported by [the SP3 parser](https://github.com/rtk-rs/sp3)

Summary:

| Format                 | File name restrictions            |    Support                         |
|------------------------|-----------------------------------|------------------------------------|
| RINEX                  | :heavy_minus_sign:                | :heavy_check_mark:                 |
| CRINEX                 | :heavy_minus_sign:                | :heavy_check_mark:                 | 
| gzip compressed RINEX  | Name must end with `.gz`          | :heavy_check_mark:                 | 
| gzip compressed CRINEX | Name must end with `.gz`          | :heavy_check_mark:                 | 
| .Z compressed RINEX    | Not supported                     | Not supported                      |
| DORIS RINEX            | :heavy_minus_sign:                | :construction: Work in progress    |
| gzip compressed DORIS  | Name must end with `.gz`          | 
| .Z compressed DORIS    | Not supported                     | Not supported                      |
| SP3                    | :heavy_minus_sign:                | :heavy_check_mark:                 | 
| gzip compressed SP3    | Name must end with `.gz`          | :heavy_check_mark:                 | 
| .Z compressed SP3      | Not supported                     | Not supported                      |
| BINEX                  | :heavy_minus_sign:                | :heavy_minus_sign:                 |
| UBX                    | :heavy_minus_sign:                | :heavy_minus_sign:                 |

:heavy_minus_sign: No restrictions: file names do not have to follow naming conventions.  

## Documentation

:warning: All our examples and demos are expected to be execute at the base of this repo.

Once you have installed the tool, read the first few steps:

- [File loading interface](./documentation/FileLoading.md): learn how to load data into the toolbox
- [The Preprocessor documentation](./documentation/Preprocessor.md) will teach you
how design a filter and deploy up to complex processing pipelines
- [The Input / Output page](./documentation/InputOutput.md) summarizes the output you can
generate, based on your input products

Then, continue your learning journey with:

- [`merge` mode](./documentation/Merge.md): to merge RINEX files together,
which is particularly useful in Data production context & files management

- [`split` mode](./documentation/Split.md): divide/split your input products at a specific point in time (`Epoch`)

- [`tbin` (time) binning mode](./documentation/TBin.md) create a batch (file series) of RINEX of equal duration

- [`cbin` (Constellation /Timescale) binning mode](./demos/CBIN.md) to split Multi-GNSS RINEX into individual
Constellations, with possible Timescale re-expression

- [`diff` mode](./demos/DIFF.md): create a special RINEX=RINEX(A)-RINEX(B)
by substracting two observation RINEX files together, per frequency and signal modulations

- [`filegen` mode](./documentation/Filegen.md): generate output products (RINEX, SP3, CSV..)
after a possible preprocessing pipeline. Use this to either reformat RINEX or perform a RINEX to CSV conversion.

## Post Processed Positioning

Dive into the world of precise navigation:

- [`ppp` opmode introduction](./documentation/PPP.md): resolve PVT solutions
- [`ppp` with special `--cggtts` option](./documentation/CGGTTS.md): resolve CGGTTS solutions

Examples
========

[Many examples](./examples/README.md) are provided for each mode individually.

Demos
=====

Our [Demo folder](./demos) hosts many illustrations and high level applications

Toolbox features
================

- `csv`: unlock CSV export (default)
- `ppp`: unlock P.V.T. solver (not available by default, because it is heavy)
- `ppp+cggtts`: unlock CGGTTS solver (not available by default, because it is heavy)

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
