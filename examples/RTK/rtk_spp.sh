#!/bin/sh
# RTK-SPP: ESBC (rover) relative to MOJN (base), 65 km baseline, 2020-06-25, 24h
# Single frequency code (L1 C/A): double differences of one pseudo range, broadcast orbits
cargo run --release --features navigation,pdf -- \
    -f ESBC=data/CRNX/V3/ESBC00DNK_R_20201770000_01D_30S_MO.crx.gz \
    -f MOJN=data/CRNX/V3/MOJN00DNK_R_20201770000_01D_30S_MO.crx.gz \
    -f data/NAV/V3/ESBC00DNK_R_20201770000_01D_MN.rnx.gz \
    --session rtk_spp_ESBC_MOJN \
    --title "RTK-SPP: ESBC (rover) / MOJN (base), GPS, 2020-06-25" \
    --report html,pdf \
    nav --method spp --rover ESBC --base MOJN --static -c gps
