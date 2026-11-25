#!/bin/sh
# 1. CKMG0080.09I: 13 maps (one every 2 hours) interpolated x2 (rotated maps):
#    WORKSPACE/CKMG0080_x2/IONEX/CKMG0080_01H.09I (25 maps, one every hour)
cargo run --release --features ionex,pdf -- \
    -f data/IONEX/V1/CKMG0080.09I.gz \
    --session CKMG0080_x2 \
    --report html,pdf \
    tec --upsample 2 --region "Europe:35,70,-10,30"

# 2. the synthesized IONEX, injected back: TEC analysis and maps
cargo run --release --features ionex,pdf -- \
    -f WORKSPACE/CKMG0080_x2/IONEX/CKMG0080_01H.09I \
    --session CKMG0080_01H_tec \
    --title "TEC maps: CKMG0080_01H.09I (CODE 2009-01-08, synthesized x2)" \
    --report html,pdf \
    tec --region "Europe:35,70,-10,30"
