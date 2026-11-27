#!/bin/sh
# TEC estimated from the observations of a network of two receivers:
# ESBC00DNK and MOJN00DNK (Denmark, 65 km apart), 2020-06-25, GPS L1/L2.
# UNCALIBRATED: the differential code biases (DCB) are not corrected,
# see documentation/Obs2Tec.md (limitations).

# 1. vertical TEC at the pierce points of both receivers, one map every hour
#    over the network, projected on Esbjerg, formatted as IONEX:
#    WORKSPACE/DNK_TEC/IONEX/DNKTEC_01H.20I (25 maps)
cargo run --release --features ionex,pdf -- \
    -f data/CRNX/V3/ESBC00DNK_R_20201770000_01D_30S_MO.crx.gz \
    -f data/CRNX/V3/MOJN00DNK_R_20201770000_01D_30S_MO.crx.gz \
    -f data/NAV/V3/ESBC00DNK_R_20201770000_01D_MN.rnx.gz \
    --session DNK_TEC \
    --report html,pdf \
    obs2tec -c GPS --ionex --point "Esbjerg:55.5,8.45"

# 2. the formatted IONEX is a standard IONEX: injected back into rinex-cli,
#    analyzed like any other IONEX (statistics over Denmark, one map every 30 minutes)
cargo run --release --features ionex,pdf -- \
    -f WORKSPACE/DNK_TEC/IONEX/DNKTEC_01H.20I \
    --session DNK_TEC_maps \
    --report html,pdf \
    tec --region "Denmark:54.5,57.8,8,13" --resample "30 min"
