#!/bin/sh
# CKMG0090.21I: 25 maps (one every hour), one map every 30 minutes (49 maps)
cargo run --release --features ionex,pdf -- \
    -f data/IONEX/V1/CKMG0090.21I.gz \
    --session CKMG0090_30min \
    --report html,pdf \
    tec --resample "30 min"
