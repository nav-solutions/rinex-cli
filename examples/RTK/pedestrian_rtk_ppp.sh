#!/bin/sh
# RTK-PPP: pedestrian (u-blox, 1 Hz) relative to NYA2 (Ny-Ålesund, Septentrio, 30 s),
# 2024-09-20, 08:18 -> 11:00 GPST, 6 to 8.5 km baseline.
# Dual frequency code and phase (L1/L2C iono free): double differences of the phase, ambiguities rounded.
# The rover time tags (08:17:50.989..) are snapped to the second, and the epochs of the
# base sampling (30 s) are retained: rover and base are presented together.
cargo run --release --features navigation,pdf -- \
    -f rover=data/OBS/V3/2024_09_20_10_17_06.obs.gz \
    -f NYA2=data/CRNX/V3/NYA200NOR_R_20242640000_01D_30S_MO.crx.gz \
    -f data/NAV/V3/2024_09_20_10_17_06.nav.gz \
    --snap-epochs --align-sampling 30s \
    --session rtk_ppp_pedestrian_NYA2 \
    --title "RTK-PPP: pedestrian (rover) / NYA2 (base), GPS, 2024-09-20" \
    --report html,pdf \
    nav --method ppp --rover rover --base NYA2 --profile pedestrian -c gps --gpx --kml
