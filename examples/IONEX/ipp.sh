#!/bin/sh
# Ionospheric Pierce Points of a receiver in Ny-Ålesund (Svalbard, 78.9° N), 450 km shell:
# regional and world maps of the pierce points, lines of sight, then the quality control
# of the observations. Add an IONEX covering the session (-f MAPS.INX) for the vertical TEC.
cargo run --release --features ionex,pdf -- \
    -f rover=data/OBS/V3/2024_09_20_10_17_06.obs.gz \
    -f data/NAV/V3/2024_09_20_10_17_06.nav.gz \
    --session ipp_pedestrian \
    --report html,pdf \
    ipp
