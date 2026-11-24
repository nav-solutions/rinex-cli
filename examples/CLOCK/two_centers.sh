#!/bin/sh
# Clock residuals between two Clock RINEX of distinct analysis centers, 2020-06-25:
# GRG - JPL. JPL0OPSFIN_20201770000_01D_30S_CLK.CLK.gz is available at
# https://sideshow.jpl.nasa.gov/pub/jpligsac/2111/
cargo run --release --features pdf -- \
    -f data/CLK/V3/GRG0MGXFIN_20201770000_01D_30S_CLK.CLK.gz \
    -f JPL0OPSFIN_20201770000_01D_30S_CLK.CLK.gz \
    --session clock_residuals_GRG_JPL \
    --title "Clock residuals: GRG - JPL, 2020-06-25" \
    --report html,pdf \
    residuals clock --lhs clk:agency:GRG --rhs clk:agency:JPL
