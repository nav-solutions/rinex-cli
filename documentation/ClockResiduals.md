Clock residuals
===============

`residuals clock` compares the clocks described by two clock products: **A − B**,
for every clock described by both (satellite clocks, ground station clocks), at every epoch
(or every `--sampling` seconds). Typical use: comparing the clock solutions of two analysis
centers.

| Product | Designation | Clocks |
|---------|-------------|--------|
| Clock RINEX | `clk` (any), `clk:PRODUCT` | satellites and ground stations |
| SP3 clock states | `sp3` (any), `sp3:PRODUCT` | satellites |
| Broadcast clocks (NAV) | `nav` (any), `nav:PRODUCT` | satellites |

`PRODUCT` is the identity printed by the `index` command, for example `agency:GRG`.
Clock RINEX are identified by their analysis center (`ANALYSIS CENTER` header field):
`agency:GRG`, `agency:JPL`, `agency:COM`.. Use aliases (`-f grg=FILE`) to name them yourself.

## Clock datum

Each analysis center estimates its clocks relative to its own reference: the raw residuals
of two centers share a common offset at each epoch, of a few nanoseconds. GNSS-Qc estimates
this datum difference at each epoch (median of the satellite residuals) and removes it:

| `--alignment` | Datum |
|---------------|-------|
| `constellation` (default) | one per epoch and per constellation (inter system biases differ between centers). Stations use the GPS datum |
| `epoch` | one per epoch |
| `none` | raw residuals only |

## Example: two analysis centers

Two Clock RINEX of distinct analysis centers, for the same day:

```bash
rinex-cli \
    -f GRG0MGXFIN_20201770000_01D_30S_CLK.CLK.gz \
    -f JPL0OPSFIN_20201770000_01D_30S_CLK.CLK.gz \
    --report html,pdf \
    residuals clock --lhs clk:agency:GRG --rhs clk:agency:JPL
```

With the data of this repository (GRG Clock RINEX, IAC SP3 of the same day):

```bash
rinex-cli \
    -f data/CLK/V3/GRG0MGXFIN_20201770000_01D_30S_CLK.CLK.gz \
    -f data/SP3/D/Sta21114.sp3.gz \
    --session clock_residuals_GRG_IAC --report html,pdf \
    residuals clock --lhs clk:agency:GRG --rhs sp3:agency:IAC --sampling 900
```

```text
WORKSPACE/clock_residuals_GRG_IAC/
├── report.html
├── report.pdf
├── summary.json
└── data/clock_residuals.csv     epoch,clock,lhs_kind,lhs_source,rhs_kind,rhs_source,residual_s,datum_s,aligned_s
```

The report presents, per constellation (and for the stations): raw mean and dispersion, aligned
RMS and maximum; the clock datum difference over time; the aligned residuals of every clock;
the raw residuals; a table per clock.

| Constellation | Raw RMS | Aligned RMS |
|---------------|---------|-------------|
| GPS | 1.54 ns | 0.34 ns (10 cm) |
| Galileo | 2.55 ns | 0.23 ns (7 cm) |
| Glonass | 2.79 ns | 2.45 ns |

Once the datum is removed, the GPS and Galileo clocks of both centers agree at the 0.1 ns
level. Glonass clocks absorb satellite specific biases (FDMA inter frequency biases), that
differ between centers.

## Other comparisons

```bash
# Clock RINEX and SP3 of the same center: identical at the SP3 epochs (15'),
# interpolation error of the SP3 clocks in between (30")
rinex-cli -f GRG.CLK -f GRG.SP3 residuals clock --lhs clk --rhs sp3

# precise - broadcast clocks: quality of the radio messages
rinex-cli -f GRG.CLK -f BRDC.rnx residuals clock --lhs clk --rhs nav --sampling 900

# ground station clocks of two centers
rinex-cli -f a=A.CLK -f b=B.CLK residuals clock --lhs clk:a --rhs clk:b --stations-only
```
