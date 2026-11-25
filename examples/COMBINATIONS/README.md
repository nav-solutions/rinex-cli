Signal combinations
===================

See [the documentation](../../documentation/Combinations.md).

| Script | Demonstrates |
|--------|--------------|
| [esbc.sh](./esbc.sh) | GF, IF and code multipath of a geodetic receiver (ESBC00DNK), every pair of frequencies |
| [multipath.sh](./multipath.sh) | Code multipath of two stations, same day: ESBC00DNK versus MOJN00DNK (choke ring antenna) |
| [glacier.sh](./glacier.sh) | Low cost station (u-blox, 1 s) on a glacier: code multipath and ionosphere |
| [ionosphere.sh](./ionosphere.sh) | Galileo phase GF (E1/E5a, E1/E5b, E1/E5, E1/E6), CSV output |

Code multipath RMS (arc mean removed):

| Signal source | GPS MP C1C | GPS MP C2W / C2X | Galileo MP C1C (L5Q) | Galileo MP C5Q (L1C) |
|---------------|------------|------------------|----------------------|----------------------|
| ESBC00DNK (ASH701945E_M) | 0.32 m | 0.31 m | 0.23 m | 0.32 m |
| MOJN00DNK (LEIAR25.R4, choke ring) | 0.27 m | 0.26 m | 0.22 m | 0.19 m |
| Glacier station (u-blox) | 0.44 m (L2X) | 0.43 m (C2X) | - | - |
