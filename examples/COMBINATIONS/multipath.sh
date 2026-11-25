#!/bin/sh
# Code multipath of two geodetic stations, same day: ESBC00DNK (ASH701945E_M)
# and MOJN00DNK (LEIAR25.R4 choke ring), GPS and Galileo
cargo run --release --features pdf -- \
    -f ESBC=data/CRNX/V3/ESBC00DNK_R_20201770000_01D_30S_MO.crx.gz \
    -f MOJN=data/CRNX/V3/MOJN00DNK_R_20201770000_01D_30S_MO.crx.gz \
    --session multipath_ESBC_MOJN \
    --title "Code multipath: ESBC00DNK versus MOJN00DNK, 2020-06-25" \
    --report html,pdf \
    --no-qc \
    combinations --mp -c gps,gal
