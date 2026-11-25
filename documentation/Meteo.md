Meteo sensors
=============

`meteo` analyzes the meteo sensors described by Meteo RINEX (V2, V3): pressure, temperature,
humidity, zenith delays, wind, rain.. For each sensor and each station: statistics, time
series, and hour by hour statistics (mean, minimum, maximum).

```bash
rinex-cli \
    -f POTS=data/MET/V3/POTS00DEU_R_20232540000_01D_05M_MM.rnx.gz \
    --title "POTS00DEU meteo sensors, 2023-09-11" --report html,pdf \
    meteo
```

`--observables PR,TD` restricts the analysis to some sensors:

| Code | Sensor | Unit |
|------|--------|------|
| `PR` | Pressure | hPa |
| `TD` | Dry temperature | °C |
| `HR` | Relative humidity | % |
| `ZW`, `ZD`, `ZT` | Wet, dry, total zenith path delay | mm |
| `WD`, `WS` | Wind direction, speed | °, m/s |
| `RI` | Rain increment | 0.1 mm |
| `HI` | Hail indicator | - |

```text
WORKSPACE/<session>/
├── report.html
├── report.pdf
├── summary.json
└── data/meteo.csv      epoch,source,observable,value
```

POTS00DEU (Potsdam, GFZ), 2023-09-11, 288 observations per sensor (5'):

| Sensor | Mean | σ | Min | Max |
|--------|------|---|-----|-----|
| Pressure | 1003.31 hPa | 1.28 | 1001.7 hPa (22:45) | 1005.8 hPa (00:00) |
| Dry temperature | 24.88 °C | 4.53 | 16.8 °C (04:05) | 31.3 °C (13:20) |
| Relative humidity | 45.66 % | 17.59 | 27.4 % (15:00) | 85.0 % (04:10) |

A sunny late summer day: the temperature rises by 14 °C from dawn to the early afternoon,
while the relative humidity drops from 85 % to 27 %; the pressure slowly decreases (−4.1 hPa
over the day).

Several stations, or several days (merged), can be analyzed in a single session.
