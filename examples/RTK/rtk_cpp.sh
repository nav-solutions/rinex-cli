#!/bin/sh
# RTK-CPP: ESBC (rover) relative to MOJN (base), 65 km baseline, 2020-06-25, 24h
# Dual frequency code (L1/L2 iono free): double differences of pseudo ranges, broadcast orbits
cargo run --release --features navigation,pdf -- \
    -f ESBC=data/CRNX/V3/ESBC00DNK_R_20201770000_01D_30S_MO.crx.gz \
    -f MOJN=data/CRNX/V3/MOJN00DNK_R_20201770000_01D_30S_MO.crx.gz \
    -f data/NAV/V3/ESBC00DNK_R_20201770000_01D_MN.rnx.gz \
    --session rtk_cpp_ESBC_MOJN \
    --title "RTK-CPP: ESBC (rover) / MOJN (base), GPS, 2020-06-25" \
    --report html,pdf \
    nav --method cpp --rover ESBC --base MOJN --static -c gps
