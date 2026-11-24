#!/bin/sh
# Meteo sensors of POTS00DEU (Potsdam), 2023-09-11, 5' sampling
cargo run --release --features pdf -- \
    -f POTS=data/MET/V3/POTS00DEU_R_20232540000_01D_05M_MM.rnx.gz \
    --session meteo_POTS \
    --title "POTS00DEU meteo sensors, 2023-09-11" \
    --report html,pdf \
    meteo
