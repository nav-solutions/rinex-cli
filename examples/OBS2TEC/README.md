TEC from the observations (obs2tec)
===================================

See [the documentation](../../documentation/Obs2Tec.md).

- [network.sh](./network.sh): a network of two Danish receivers (ESBC00DNK, MOJN00DNK), one TEC
  map every hour formatted as IONEX, then the IONEX injected back into rinex-cli (`tec` command)
  and analyzed like any other IONEX
- [single.sh](./single.sh): a single receiver, GPS and Galileo, one map every 15 minutes over
  Northern Europe, points of interest, gzip compressed IONEX

> [!WARNING]
> The TEC is uncalibrated: the differential code biases (DCB) of the satellites and receivers
> are not corrected. Read [the limitations](../../documentation/Obs2Tec.md#limitations-differential-code-biases).
