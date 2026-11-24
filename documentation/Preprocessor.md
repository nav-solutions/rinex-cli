Preprocessing
=============

The toolbox includes a powerful Filter Designer that you 
can operate with `-P`.

A filter description is made of an item and possibly an operand.
We support many items and many operands, as listed down below.
When the operand is omitted, it is the [Data Masking](#Data Masking) filter that is implicitely requested.

It is possible to stack as many filter descriptions as you need,
to create complex conditions. For this you have two options

1. either use `-P` as many times as you need
2. or use `;` as a filter separator in your description.

Example (1)

```bash
rinex-cli -q \
    -P Gal \
    -P "<= 2020-06-25T12:00:00 UTC" \
    --fp data/ESBC00DNK_R_20201770000_01D_30S_MO.crx.gz
```

Example (2)

```bash
rinex-cli -q \
    -P "Gal;<= 2020-06-25T12:00:00 UTC" \
    --fp data/ESBC00DNK_R_20201770000_01D_30S_MO.crx.gz
```

An invalid filter description will only result in a warning trace, it will not cause the application to crash. 

Any filter will apply to the entire dataset, we currently have no means to focus on one particular portion of the dataset. Let's imagine you have loaded one observation and one navigation files, applying a constellation filter will apply similarly to both.

## Filter Operand

Most filter operations support an Operand. When omitted, it is the Equality operand (`=`) that is implied.
The following operands are defined:

* `<` Lower Than 
* `<=` Lower Than or Equals 
* `>` Greater Than
* `>=` Greater Than or Equals
* `=` Equality
* `!=` Inequality

## Case sensitivity

The filter design is case insensitive and is very flexible in your operation description. For example,
these are all valid `Galileo` selection methods:

- `-P Gal`
- `-P gal`
- `-P galileo`

## Whitespace tolerance

The filter designer is very tolerant to whitespaces. We tolerate missing whitespace between Operands and Operations:

```bash
rinex-cli -P "Gal;<2024-08-24T10:00:00 UTC" [...]
```

## Data Masking

Data masking supports many items

1. Time frame

By definining a time frame, you can reduce data quantity and focus
on the time frame you are interested in:

```bash
rinex-cli -q \
    -P Gal \
    -P "<= 2020-06-25T10:00:00 UTC" \
    --fp data/ESBC00DNK_R_20201770000_01D_30S_MO.crx.gz
```

When the operand is omitted, we only retain a single Epoch.

2. By stacking two time frames, you can easily define a time window

```bash
rinex-cli -q \
    -P Gal \
    -P "> 2020-06-25T10:00:00 UTC" \
    -P "<= 2020-06-25T12:00:00 UTC" \
    --fp data/ESBC00DNK_R_20201770000_01D_30S_MO.crx.gz
```

3. GNSS Constellations

All supported constellations can be used as a data mask.
In this example, we retain Galileo + GPS only:

```bash
rinex-cli -q \
    -P Gal;GPS
    --fp data/ESBC00DNK_R_20201770000_01D_30S_MO.crx.gz
```

:warning: a constellation mask is not compatible with an Operand.

4. SV (Satellite vehicles)

Any valid SV description may serve as a data mask. 
In this example, we retain G05,G06,G07,G08,G09 only:

```bash
rinex-cli -q \
    -P G05,G06,G07,G08,G09 \
    --fp data/ESBC00DNK_R_20201770000_01D_30S_MO.crx.gz
```

When an operand is specified, we apply this PRN filter for that very constellation.
In this example, we retain PRN above 15 for all Galileo, and any GPS:

```bash
rinex-cli -q \
    -P >E15,GPS \
    --fp data/ESBC00DNK_R_20201770000_01D_30S_MO.crx.gz
```

# Resampling (decimation)

We support two kinds of decimation filters to resample your data (with reduction):

1. Decimate to fit an new sampling period, with `-P decim:dt`. Any valid
Duration description may apply. In this examplen, the new sampling period becomes 5 mins,
instead of 30 seconds. This does not involve interpolation at the moment, data is simply discarded. 

```bash
rinex-cli -q \
    -P GPS \
    -P "decim:5 min" \
    --fp data/ESBC00DNK_R_20201770000_01D_30S_MO.crx.gz
```


2. Modulo decimation. In this example, we reduce data quantity by 2, the sampling interval
is now 1 min.

```bash
rinex-cli -q \
    -P GPS \
    -P "decim:2" \
    --fp data/ESBC00DNK_R_20201770000_01D_30S_MO.crx.gz
```

# Aligning epochs (low cost receivers)

Decimation counts from the first epoch of each file, and keeps the time tags as they are.
Receivers that time tag their observations a few milliseconds off the second (u-blox: 08:17:50.989)
never share an epoch with a reference station (08:18:00.000), which prevents RTK and any
combination of receivers. Two global options fix this, for observation RINEX only:

- `--snap-epochs` moves each epoch to the nearest second and corrects the observations accordingly
(pseudo ranges C + c·Δt, phase ranges L + f·Δt), as a receiver clock jump would. The transmission
times, hence the geometry, are unchanged.
- `--align-sampling PERIOD` keeps only the epochs that are multiples of `PERIOD`, counted from the GPS
time origin, so that every file keeps the same instants (`30s`, `"30 s"`, `30`, `"0.5 min"`).

```bash
rinex-cli \
    -f rover=data/OBS/V3/2024_09_20_10_17_06.obs.gz \
    -f NYA2=data/CRNX/V3/NYA200NOR_R_20242640000_01D_30S_MO.crx.gz \
    --snap-epochs --align-sampling 30s
```

See [RTK](./RTK.md#aligning-the-rover-epochs).
