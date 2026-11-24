Configuration presets
=====================

Navigation configuration examples (`nav --cfg FILE.json`, `cggtts --cfg FILE.json`).
They describe a GNSS-Qc `QcNavigationConfig` and may be *partial*: every field that is not
described keeps its default value (CPP, GPS + Galileo, automatic orbits and clocks, pedestrian profile).
The command line options (`--method`, `-c`, `--orbits`, `--static`, `--base`..) take precedence
over the configuration file.

```json
{
    "technique": { "Rtk": { "base": { "Custom": "MOJN" }, "position_ecef_m": null } },
    "rover": { "Custom": "ESBC" },
    "constellations": ["GPS", "Galileo"],
    "solver": { "method": "PPP", "timescale": "GPST" }
}
```

- `technique`: `"Absolute"` (default) or `{ "Rtk": { "base": PRODUCT, "position_ecef_m": [x, y, z] | null } }`
- `rover`, `base`: signal sources, as listed by the `index` command
(`{ "Custom": "ALIAS" }`, `{ "Receiver": "..." }`, `{ "Antenna": "..." }`..)
- `solver`: GNSS-RTK solver configuration (`method`, `timescale`, `min_sv_elev`, modeling..)
- [rtk_ppp.json](./rtk_ppp.json): RTK-PPP, rover ESBC and base MOJN (see the [RTK examples](../RTK))

Timescale
=========

The selected timescale will have PVT solutions expressed with respect to that timescale.  
Timescale selection has many impacts, that a simple readme and a few lines of text will not
suffice to describe and are out of scope here. The examples and demonstrations provided here
will emphasize many applications & scenarios. The input context must allow the desired timescale
to be selected, otherwise the configuration is not valid.

## GPST presets

Is the basic use case, compatible with basic general public RINEX files

## GST presets

`rinex-cli` (and `rtk-rs` more broadly) is capable to fulfill all requirements to navigate in GST.
This is direcly compatible with a GST RINEX file.

## BDT presets

`rinex-cli` (and `rtk-rs` more broadly) is capable to fulfill all requirements to navigate in BDT.
This is direcly compatible with a BDT RINEX file. 

## UTC presets

`rinex-cli` (and `rtk-rs` more broadly) is capable to fulfill all requirements to allow solving
in UTC. UTC RINEX files do not exist or are now part of general public RINEX files. 
We use those presets to demonstrate UTC is a valid timescale for our PVT solutions,
and in other exotic scenarios.

Navigation technique
====================

## SPP presets

In SPP, we navigate using a single signal in sight. 
With this framework, it [does not have to L1, as demonstrated in this example]().

SPP is dedicated to reduced navigation capabilities and will exhibit poorer results
than others. Yet, it is possible to obtain very good results (~ 1m) in static applications,
and if you have high quality sampling.

Because SPP is very limited, it does not make sense to compensate many perturbations. 
If you compare these presets to others, they will appear much more simple. It is one
of the reasons why we recommend starting with `SPP` navigation technique/strategy, before
diving into CPP and later on, PPP.

As learning experience, you can activate advanced compensations while still using the
SPP technique, to see that it has no impact, because your accuracy is mostly limited by the
ionospheric bias.

## CPP preset

Dual pseudo range navigation technique. Gives much higher accuracy as comparsed to SPP.

## PPP preset

Dual pseudo range + phase range navigation technique. Gives the highest accuracy.
