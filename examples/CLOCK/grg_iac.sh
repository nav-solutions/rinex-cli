#!/bin/sh
# Clock residuals between two analysis centers, 2020-06-25:
# GRG (Clock RINEX, 30") - IAC (SP3 clock states, 15'), every 15'
cargo run --release --features pdf -- \
    -f data/CLK/V3/GRG0MGXFIN_20201770000_01D_30S_CLK.CLK.gz \
    -f data/SP3/D/Sta21114.sp3.gz \
    --session clock_residuals_GRG_IAC \
    --title "Clock residuals: GRG (CLK) - IAC (SP3), 2020-06-25" \
    --report html,pdf \
    residuals clock --lhs clk:agency:GRG --rhs sp3:agency:IAC --sampling 900
