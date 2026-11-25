#!/bin/sh
# Signal combinations of ESBC00DNK (Septentrio PolaRx5), 2020-06-25, 24h, 30 s:
# Geometry Free, Ionosphere Free and code multipath, every pair of frequencies
cargo run --release --features pdf -- \
    -f ESBC=data/CRNX/V3/ESBC00DNK_R_20201770000_01D_30S_MO.crx.gz \
    -f data/NAV/V3/ESBC00DNK_R_20201770000_01D_MN.rnx.gz \
    --session combinations_ESBC \
    --title "Signal combinations: ESBC00DNK, 2020-06-25" \
    --report html,pdf \
    combinations
