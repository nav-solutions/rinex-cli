#!/bin/sh
# Ionosphere monitoring: phase Geometry Free (leveled on the code) of every Galileo pair
# (E1/E5a, E1/E5b, E1/E5, E1/E6), ESBC00DNK, 2020-06-25. CSV output for further processing.
cargo run --release --features pdf -- \
    -f ESBC=data/CRNX/V3/ESBC00DNK_R_20201770000_01D_30S_MO.crx.gz \
    --session ionosphere_ESBC \
    --title "Galileo Geometry Free: ESBC00DNK, 2020-06-25" \
    --report html \
    --data csv \
    --no-qc \
    combinations --gf -c gal
