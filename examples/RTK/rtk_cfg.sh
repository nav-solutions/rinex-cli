#!/bin/sh
# RTK-PPP described by a (partial) navigation configuration file:
# technique (RTK, base MOJN), rover ESBC, method PPP. See examples/CONFIG/rtk_ppp.json.
# Command line options take precedence: here we restrict to GPS and select the static profile.
cargo run --release --features navigation,pdf -- \
    -f ESBC=data/CRNX/V3/ESBC00DNK_R_20201770000_01D_30S_MO.crx.gz \
    -f MOJN=data/CRNX/V3/MOJN00DNK_R_20201770000_01D_30S_MO.crx.gz \
    -f data/NAV/V3/ESBC00DNK_R_20201770000_01D_MN.rnx.gz \
    -f data/SP3/C/GRG0MGXFIN_20201770000_01D_15M_ORB.SP3.gz \
    -f data/CLK/V3/GRG0MGXFIN_20201770000_01D_30S_CLK.CLK.gz \
    --session rtk_cfg_ESBC_MOJN \
    --report html \
    nav --cfg examples/CONFIG/rtk_ppp.json --static -c gps
