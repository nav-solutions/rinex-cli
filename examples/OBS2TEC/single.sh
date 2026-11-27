#!/bin/sh
# TEC estimated from the observations of a single receiver (ESBC00DNK, 2020-06-25),
# GPS and Galileo, 30° elevation mask (smaller mapping errors), one map every 15 minutes
# over Northern Europe, projected on Esbjerg and Copenhagen.
# UNCALIBRATED: the differential code biases (DCB) are not corrected,
# see documentation/Obs2Tec.md (limitations).
cargo run --release --features ionex,pdf -- \
    -f data/CRNX/V3/ESBC00DNK_R_20201770000_01D_30S_MO.crx.gz \
    -f data/NAV/V3/ESBC00DNK_R_20201770000_01D_MN.rnx.gz \
    --session ESBC_TEC \
    --report html,pdf \
    obs2tec -c GPS,GAL --elevation-mask 30 --interval "15 min" \
    --grid 45,65,-5,25 --dlat 1 --dlon 2 \
    --point "Esbjerg:55.5,8.45" --point "Copenhagen:55.7,12.6" \
    --ionex --gzip
