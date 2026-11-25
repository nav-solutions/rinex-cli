#!/bin/sh
# Low cost station (u-blox, GPS L1 C/A + L2C, 1 s) on a glacier near Ny-Ålesund,
# 2024-05-06: code multipath and ionosphere (GF)
cargo run --release --features pdf -- \
    -f GLACIER=data/OBS/V3/240506_glacier_station.obs.gz \
    --session combinations_GLACIER \
    --title "Signal combinations: glacier station (u-blox), 2024-05-06" \
    --report html,pdf \
    combinations --gf --mp
