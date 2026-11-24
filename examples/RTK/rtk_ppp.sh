#!/bin/sh
# RTK-PPP: ESBC (rover) relative to MOJN (base), 65 km baseline, 2020-06-25, 24h
# Dual frequency code and phase (L1/L2 iono free): double differences of the phase
# (wide lane / narrow lane ambiguity rounding), GRG final orbits (SP3) and clocks (Clock RINEX)
cargo run --release --features navigation,pdf -- \
    -f ESBC=data/CRNX/V3/ESBC00DNK_R_20201770000_01D_30S_MO.crx.gz \
    -f MOJN=data/CRNX/V3/MOJN00DNK_R_20201770000_01D_30S_MO.crx.gz \
    -f data/NAV/V3/ESBC00DNK_R_20201770000_01D_MN.rnx.gz \
    -f data/SP3/C/GRG0MGXFIN_20201770000_01D_15M_ORB.SP3.gz \
    -f data/CLK/V3/GRG0MGXFIN_20201770000_01D_30S_CLK.CLK.gz \
    --session rtk_ppp_ESBC_MOJN \
    --title "RTK-PPP: ESBC (rover) / MOJN (base), GPS, 2020-06-25" \
    --report html,pdf \
    nav --method ppp --rover ESBC --base MOJN --static -c gps
