use crate::cli::Context;
use std::collections::BTreeMap;

use gnss_rtk::prelude::{
    ClockProfile, Config as NaviConfig, Duration, Epoch, Method as NaviMethod, PVTSolution,
    TimeScale, UserParameters, UserProfile, SV,
};

use gnss_qc::{
    plot::{MapboxStyle, NamedColor},
    prelude::{html, MarkerSymbol, Markup, Mode, Plot, QcExtraPage, Render},
};

use itertools::Itertools;

struct ReportTab {}

impl Render for ReportTab {
    fn render(&self) -> Markup {
        html! {
            a id="menu:ppp" {
                span class="icon" {
                    i class="fa-solid fa-location-crosshairs" {}
                }
                "PPP Solutions"
            }
        }
    }
}

struct Summary {
    method: NaviMethod,
    orbit: String,
    first_epoch: Epoch,
    last_epoch: Epoch,
    duration: Duration,
    satellites: Vec<SV>,
    timescale: TimeScale,
    user_profile: UserProfile,
    clock_profile: ClockProfile,
    final_err_m: Option<(f64, f64, f64)>,
    final_geo_ddeg_m: (f64, f64, f64),
    averaged_err_m: Option<(f64, f64, f64)>,
    averaged_latlongalt_ddeg_m: Option<(f64, f64, f64)>,
    surveyed_lat_long_alt_ddeg_ddeg_km: Option<(f64, f64, f64)>,
}

impl Render for Summary {
    fn render(&self) -> Markup {
        html! {
            div class="table-container" {
                table class="table is-bordered" {
                    tbody {
                        tr {
                            th class="is-info" {
                                "Session"
                            }
                            @ if self.user_profile == UserProfile::Static {
                                td {
                                    ("Static")
                                }
                            } @ else {
                                td {
                                    ("Kinematic")
                                }
                            }
                        }
                        tr {
                            th class="is-info" {
                                "User Profile"
                            }
                        }

                        tr {
                            th class="is-info" {
                                "Clock Profile"
                            }
                        }

                        tr {
                            th class="is-info" {
                                "Technique"
                            }
                            td {
                                (self.method.to_string())
                            }
                        }

                        tr {
                            th class="is-info" {
                                "Timescale"
                            }
                            td {
                                (self.timescale.to_string())
                            }
                        }

                        tr {
                            th class="is-info" {
                                "Orbit"
                            }
                            td {
                                (self.orbit)
                            }
                        }
                        tr {
                            th class="is-info" {
                                button aria-label="Satellites that contributed to the solutions" data-balloon-pos="right" {
                                    "Satellites"
                                }
                            }
                            td {
                                (self.satellites.iter().join(" ,"))
                            }
                        }

                        tr {
                            th class="is-info" {
                                "Time frame"
                            }
                            td {
                                table class="table is-bordered" {
                                    tr {
                                        th class="is-info" {
                                            "First solution"
                                        }
                                        td {
                                            (self.first_epoch.round(Duration::from_seconds(1.0)).to_string())
                                        }
                                    }
                                    tr {
                                        th class="is-info" {
                                            "Last solution"
                                        }
                                        td {
                                            (self.last_epoch.round(Duration::from_seconds(1.0)).to_string())
                                        }
                                    }
                                    tr {
                                        th class="is-info" {
                                            "Duration"
                                        }
                                        td {
                                            (self.duration.to_string())
                                        }
                                    }
                                }
                            }
                        }

                        @ match self.surveyed_lat_long_alt_ddeg_ddeg_km {
                            Some((lat0_ddeg, long0_ddeg, alt0_km)) => {
                                tr {
                                    th class="is-info" {
                                        "Surveyed"
                                    }
                                    td {
                                        table class="table is-bordered" {
                                            tr {
                                                th class="is-info" {
                                                    "GEO"
                                                }
                                                td {
                                                    (format!("lat={:.5}°", lat0_ddeg))
                                                }
                                                td {
                                                    (format!("long={:.5}°", long0_ddeg))
                                                }
                                                td {
                                                    (format!("alt={:.3}m", alt0_km * 1.0E3))
                                                }
                                            }
                                        }
                                     }
                                }
                            },
                            _ => {},
                        }

                        @ match self.final_err_m {
                            Some((final_err_x_m, final_err_y_m, final_err_z_m)) => {
                                tr {
                                    th class="is-info" {
                                        "Final"
                                    }
                                    td {
                                        table class="table is-bordered" {
                                            tr {
                                                th class="is-info" {
                                                    "GEO"
                                                }
                                                td {
                                                    (format!("lat={:.5}°", self.final_geo_ddeg_m.0))
                                                }
                                                td {
                                                    (format!("long={:.5}°", self.final_geo_ddeg_m.1))
                                                }
                                                td {
                                                    (format!("alt={:.3}m", self.final_geo_ddeg_m.2))
                                                }
                                            }
                                        }
                                    }
                                }

                                tr {
                                    th class="is-info" {
                                        "Final"
                                    }
                                    td {
                                        table class="table is-bordered" {
                                            tr {
                                                th class="is-info" {
                                                    "Error"
                                                }
                                                td {
                                                    (format!("x={:.6}m", final_err_x_m))
                                                }
                                                td {
                                                    (format!("y={:.6}m", final_err_y_m))
                                                }
                                                td {
                                                    (format!("z={:.6}m", final_err_z_m))
                                                }
                                            }
                                        }
                                    }
                                    @ if let Some((err_x, err_y, err_z)) = &self.averaged_err_m {
                                        td {
                                            table class="table is-bordered" {
                                                tr {
                                                    th class="is-info" {
                                                        "Averaged Error"
                                                    }
                                                    td {
                                                        (format!("{:.6}m", err_x))
                                                    }
                                                    td {
                                                        (format!("y={:.6}m", err_y))
                                                    }
                                                    td {
                                                        (format!("z={:.6}m", err_z))
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            },
                            _ => {
                                tr {
                                    th class="is-info" {
                                        "Final"
                                    }
                                    td {
                                        table class="table is-bordered" {
                                            tr {
                                                th class="is-info" {
                                                    "GEO"
                                                }
                                                td {
                                                    (format!("lat={:.3E}°", self.final_geo_ddeg_m.0))
                                                }
                                                td {
                                                    (format!("long={:.3E}°", self.final_geo_ddeg_m.1))
                                                }
                                                td {
                                                    (format!("alt={:.3E}m", self.final_geo_ddeg_m.2))
                                                }
                                            }
                                        }
                                    }
                                }
                            },
                        }

                        @ match self.averaged_latlongalt_ddeg_m {
                            Some((lat_ddeg, long_ddeg, alt_m)) => {
                                tr {
                                    th class="is-info" {
                                        "Averaged"
                                    }
                                    td {
                                        table class="table is-bordered" {
                                            tr {
                                                th class="is-info" {
                                                    "GEO"
                                                }
                                                td {
                                                    (format!("lat={:.5}°", lat_ddeg))
                                                }
                                                td {
                                                    (format!("lat={:.5}°", long_ddeg))
                                                }
                                                td {
                                                    (format!("alt={:.5}m", alt_m))
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            _ => {},
                        }

                    }
                }
            }
        }
    }
}

impl Summary {
    fn new(
        cfg: &NaviConfig,
        ctx: &Context,
        user_profile: UserProfile,
        clock_profile: ClockProfile,
        solutions: &BTreeMap<Epoch, PVTSolution>,
        x0_y0_z0_m: Option<(f64, f64, f64)>,
        lat0_long0_alt0_ddeg_ddeg_km: Option<(f64, f64, f64)>,
    ) -> Self {
        let mut timescale = TimeScale::default();

        let (mut first_epoch, mut last_epoch) = (Epoch::default(), Epoch::default());

        let mut final_err_m = Option::<(f64, f64, f64)>::None;
        let mut final_geo_ddeg_m = (0.0, 0.0, 0.0);

        let mut averaged_err_m = Option::<(f64, f64, f64)>::None;
        let mut averaged_latlongalt_ddeg_m = Option::<(f64, f64, f64)>::None;

        let num_solutions = solutions.len();

        let satellites = solutions
            .values()
            .map(|pvt_sol| pvt_sol.sv.iter().map(|contrib| contrib.sv))
            .fold(vec![], |mut list, svnn| {
                for sv in svnn {
                    list.push(sv);
                }
                list
            })
            .into_iter()
            .unique()
            .sorted()
            .collect::<Vec<_>>();

        for (index, (t, sol)) in solutions.iter().enumerate() {
            if index == 0 {
                first_epoch = *t;
            }

            let (x_m, y_m, z_m) = sol.pos_m;

            final_geo_ddeg_m = sol.lat_long_alt_deg_deg_m;

            if let Some((x0_m, y0_m, z0_m)) = x0_y0_z0_m {
                let (err_x, err_y, err_z) = (x_m - x0_m, y_m - y0_m, z_m - z0_m);

                final_err_m = Some((err_x, err_y, err_z));

                if let Some(averaged_err_m) = &mut averaged_err_m {
                    *averaged_err_m = (
                        averaged_err_m.0 + err_x,
                        averaged_err_m.1 + err_y,
                        averaged_err_m.2 + err_z,
                    );
                }
            }

            if user_profile == UserProfile::Static {
                // averaged coords
                if let Some(averaged_latlongalt_ddeg_m) = &mut averaged_latlongalt_ddeg_m {
                    *averaged_latlongalt_ddeg_m = (
                        averaged_latlongalt_ddeg_m.0 + sol.lat_long_alt_deg_deg_m.0,
                        averaged_latlongalt_ddeg_m.1 + sol.lat_long_alt_deg_deg_m.1,
                        averaged_latlongalt_ddeg_m.2 + sol.lat_long_alt_deg_deg_m.2,
                    );
                } else {
                    averaged_latlongalt_ddeg_m = Some(sol.lat_long_alt_deg_deg_m);
                }
            }

            last_epoch = *t;
            timescale = sol.timescale;
        }

        if let Some(averaged_err_m) = &mut averaged_err_m {
            *averaged_err_m = (
                averaged_err_m.0 / num_solutions as f64,
                averaged_err_m.1 / num_solutions as f64,
                averaged_err_m.2 / num_solutions as f64,
            );
        }

        if let Some(averaged_latlongalt_ddeg_m) = &mut averaged_latlongalt_ddeg_m {
            *averaged_latlongalt_ddeg_m = (
                averaged_latlongalt_ddeg_m.0 / num_solutions as f64,
                averaged_latlongalt_ddeg_m.1 / num_solutions as f64,
                averaged_latlongalt_ddeg_m.2 / num_solutions as f64,
            );
        }

        Self {
            first_epoch,
            last_epoch,
            timescale,
            satellites,
            user_profile,
            clock_profile,
            final_err_m,
            final_geo_ddeg_m,
            averaged_err_m,
            averaged_latlongalt_ddeg_m,
            orbit: {
                if ctx.data.has_sp3() {
                    "SP3".to_string()
                } else {
                    "Kepler".to_string()
                }
            },
            method: cfg.method,
            duration: last_epoch - first_epoch,
            surveyed_lat_long_alt_ddeg_ddeg_km: lat0_long0_alt0_ddeg_ddeg_km,
        }
    }
}

struct ReportContent {
    /// summary
    summary: Summary,

    /// sv_plot
    sv_plot: Plot,

    /// map_proj
    map_proj: Plot,

    /// clk_plot
    clk_plot: Plot,

    /// drift_plot
    drift_plot: Plot,

    /// ddeg_plot
    ddeg_plot: Plot,

    /// altitude_plot
    altitude_plot: Plot,

    /// coords_err
    coords_err_plot: Option<Plot>,

    /// 3d_plot
    coords_err3d_plot: Option<Plot>,

    /// velocity_plot
    vel_plot: Plot,

    /// DOP
    dop_plot: Plot,

    /// TDOP
    tdop_plot: Plot,

    // /// NAVI
    // navi_plot: Plot,
    /// tropod
    tropod_plot: Plot,

    /// ionod
    ionod_plot: Plot,
}

impl ReportContent {
    pub fn new(
        cfg: &NaviConfig,
        ctx: &Context,
        user_profile: UserProfile,
        clock_profile: ClockProfile,
        solutions: &BTreeMap<Epoch, PVTSolution>,
    ) -> Self {
        let nb_solutions = solutions.len();
        let epochs = solutions.keys().cloned().collect::<Vec<_>>();

        let (x0y0z0_m, lat0_long0_alt0_km) = if let Some(rx_orbit) = ctx.rx_orbit {
            let pos_vel = rx_orbit.to_cartesian_pos_vel() * 1.0E3;
            let (x0_m, y0_m, z0_m) = (pos_vel[0], pos_vel[1], pos_vel[2]);

            let (lat0_ddeg, long0_ddeg, alt0_km) = rx_orbit
                .latlongalt()
                .unwrap_or_else(|e| panic!("latlongalt() physical error: {}", e));

            (
                Some((x0_m, y0_m, z0_m)),
                Some((lat0_ddeg, long0_ddeg, alt0_km)),
            )
        } else {
            (None, None)
        };

        let summary = Summary::new(
            cfg,
            ctx,
            user_profile.clone(),
            clock_profile,
            solutions,
            x0y0z0_m,
            lat0_long0_alt0_km,
        );

        Self {
            map_proj: {
                let mut map_proj = if let Some((lat0_ddeg, long0_ddeg, _)) = lat0_long0_alt0_km {
                    Plot::world_map(
                        "map_proj",
                        "Map Projection",
                        MapboxStyle::OpenStreetMap,
                        (lat0_ddeg, long0_ddeg),
                        18,
                        true,
                    )
                } else {
                    Plot::world_map(
                        "map_proj",
                        "Map Projection",
                        MapboxStyle::OpenStreetMap,
                        (summary.final_geo_ddeg_m.0, summary.final_geo_ddeg_m.1),
                        18,
                        true,
                    )
                };

                if let Some((lat0_ddeg, long0_ddeg, _)) = lat0_long0_alt0_km {
                    let apriori = Plot::mapbox(
                        vec![lat0_ddeg],
                        vec![long0_ddeg],
                        "apriori",
                        3,
                        MarkerSymbol::Circle,
                        Some(NamedColor::Red),
                        1.0,
                        true,
                    );

                    map_proj.add_trace(apriori);
                }

                let mut prev_pct = 0;
                for (index, (_, sol_i)) in solutions.iter().enumerate() {
                    let (lat_ddeg, long_ddeg, _) = sol_i.lat_long_alt_deg_deg_m;

                    let modulo = if user_profile == UserProfile::Static {
                        10
                    } else {
                        1
                    };
                    let pct = index * 100 / nb_solutions;

                    if pct % modulo == 0 && index > 0 && pct != prev_pct
                        || index == nb_solutions - 1
                    {
                        let (name, visible) = if index == nb_solutions - 1 {
                            ("FINAL".to_string(), true)
                        } else {
                            (format!("Solver: {:02}%", pct), false)
                        };

                        let scatter = Plot::mapbox(
                            vec![lat_ddeg],
                            vec![long_ddeg],
                            &name,
                            3,
                            MarkerSymbol::Circle,
                            Some(NamedColor::Black),
                            1.0,
                            visible,
                        );

                        map_proj.add_trace(scatter);
                    }

                    prev_pct = pct;
                }
                map_proj
            },
            sv_plot: {
                let mut plot = Plot::timedomain_plot("sv_plot", "Contributions", "PRN #", true);
                for sv in summary.satellites.iter() {
                    let epochs = solutions
                        .iter()
                        .filter_map(|(t, sol)| {
                            let sv_list =
                                sol.sv.iter().map(|contrib| contrib.sv).collect::<Vec<_>>();
                            if sv_list.contains(sv) {
                                Some(*t)
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>();

                    let prn = epochs.iter().map(|_| sv.prn).collect::<Vec<_>>();

                    let trace = Plot::timedomain_chart(
                        &sv.to_string(),
                        Mode::Markers,
                        MarkerSymbol::Cross,
                        &epochs,
                        prn,
                        true,
                    );

                    plot.add_trace(trace);
                }
                plot
            },
            ddeg_plot: {
                let mut plot =
                    Plot::timedomain_plot("ddeg_plot", "Coordinates", "Coordinates [ddeg]", true);

                let ddeg = solutions
                    .iter()
                    .map(|(_, sol)| {
                        let (lat_ddeg, long_ddeg, _) = sol.lat_long_alt_deg_deg_m;
                        (lat_ddeg, long_ddeg)
                    })
                    .collect::<Vec<_>>();

                let lati = ddeg.iter().map(|ddeg| ddeg.0).collect::<Vec<_>>();
                let long = ddeg.iter().map(|ddeg| ddeg.1).collect::<Vec<_>>();

                let trace = Plot::timedomain_chart(
                    "latitude",
                    Mode::Markers,
                    MarkerSymbol::Cross,
                    &epochs,
                    lati,
                    true,
                );

                plot.add_trace(trace);

                let trace = Plot::timedomain_chart(
                    "longitude",
                    Mode::Markers,
                    MarkerSymbol::Cross,
                    &epochs,
                    long,
                    false,
                );

                plot.add_trace(trace);
                plot
            },
            altitude_plot: {
                let mut plot =
                    Plot::timedomain_plot("altitude_plot", "Altitude", "Altitude [m]", true);

                let alt_m = solutions
                    .iter()
                    .map(|(_, sol)| sol.lat_long_alt_deg_deg_m.2)
                    .collect::<Vec<_>>();

                let trace = Plot::timedomain_chart(
                    "altitude",
                    Mode::Markers,
                    MarkerSymbol::Cross,
                    &epochs,
                    alt_m,
                    true,
                );

                plot.add_trace(trace);
                plot
            },
            vel_plot: {
                let mut plot =
                    Plot::timedomain_plot("vel_plot", "Velocity", "Velocity [m/s]", true);

                // let vel_x = solutions
                //     .iter()
                //     .map(|(_, sol)| sol.vel_m_s.0)
                //     .collect::<Vec<_>>();

                // let vel_y = solutions
                //     .iter()
                //     .map(|(_, sol)| sol.vel_m_s.1)
                //     .collect::<Vec<_>>();

                // let vel_z = solutions
                //     .iter()
                //     .map(|(_, sol)| sol.vel_m_s.2)
                //     .collect::<Vec<_>>();

                // let trace = Plot::timedomain_chart(
                //     "vel_x",
                //     Mode::Markers,
                //     MarkerSymbol::Cross,
                //     &epochs,
                //     vel_x,
                //     true,
                // );

                // plot.add_trace(trace);

                // let trace = Plot::timedomain_chart(
                //     "vel_y",
                //     Mode::Markers,
                //     MarkerSymbol::Cross,
                //     &epochs,
                //     vel_y,
                //     true,
                // );

                // plot.add_trace(trace);

                // let trace = Plot::timedomain_chart(
                //     "vel_z",
                //     Mode::Markers,
                //     MarkerSymbol::Cross,
                //     &epochs,
                //     vel_z,
                //     true,
                // );

                // plot.add_trace(trace);
                plot
            },
            tropod_plot: {
                let mut plot =
                    Plot::timedomain_plot("tropo", "Troposphere Bias", "Error [m]", true);

                for sv in summary.satellites.iter() {
                    let x = solutions
                        .iter()
                        .filter_map(|(t, sol)| {
                            let sv_list =
                                sol.sv.iter().map(|contrib| contrib.sv).collect::<Vec<_>>();
                            if sv_list.contains(sv) {
                                Some(*t)
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>();

                    let y = solutions
                        .iter()
                        .filter_map(|(_, sol)| {
                            if let Some(value) = sol
                                .sv
                                .iter()
                                .filter(|contrib| contrib.sv == *sv)
                                .reduce(|k, _| k)
                            {
                                let bias = value.tropo_bias?;
                                Some(bias)
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>();

                    let trace = Plot::timedomain_chart(
                        &sv.to_string(),
                        Mode::Markers,
                        MarkerSymbol::Cross,
                        &x,
                        y,
                        true,
                    );
                    plot.add_trace(trace);
                }
                plot
            },
            ionod_plot: {
                let mut plot = Plot::timedomain_plot("iono", "Ionosphere Bias", "Error [m]", true);
                for sv in summary.satellites.iter() {
                    let x = solutions
                        .iter()
                        .filter_map(|(t, sol)| {
                            let sv_list =
                                sol.sv.iter().map(|contrib| contrib.sv).collect::<Vec<_>>();
                            if sv_list.contains(sv) {
                                Some(*t)
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>();

                    let y = solutions
                        .iter()
                        .filter_map(|(_, sol)| {
                            if let Some(value) = sol
                                .sv
                                .iter()
                                .filter(|contrib| contrib.sv == *sv)
                                .reduce(|k, _| k)
                            {
                                let bias = value.iono_bias?;
                                Some(bias)
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>();

                    let trace = Plot::timedomain_chart(
                        &sv.to_string(),
                        Mode::Markers,
                        MarkerSymbol::Cross,
                        &x,
                        y,
                        true,
                    );
                    plot.add_trace(trace);
                }
                plot
            },
            tdop_plot: {
                let mut plot = Plot::timedomain_plot(
                    "tdop",
                    "Temporal dillution of precision",
                    "Error [m]",
                    true,
                );
                let tdop = solutions
                    .iter()
                    .map(|(_, sol)| sol.tdop)
                    .collect::<Vec<_>>();

                let trace = Plot::timedomain_chart(
                    "tdop",
                    Mode::Markers,
                    MarkerSymbol::Cross,
                    &epochs,
                    tdop,
                    true,
                );
                plot.add_trace(trace);
                plot
            },
            dop_plot: {
                let mut plot =
                    Plot::timedomain_plot("dop", "Dillution of Precision", "Error [m]", true);

                let gdop = solutions
                    .iter()
                    .map(|(_, sol)| sol.gdop)
                    .collect::<Vec<_>>();

                let trace = Plot::timedomain_chart(
                    "gdop",
                    Mode::Markers,
                    MarkerSymbol::Cross,
                    &epochs,
                    gdop,
                    true,
                );

                plot.add_trace(trace);

                let vdop = solutions
                    .iter()
                    .map(|(_, sol)| sol.vdop)
                    .collect::<Vec<_>>();

                let trace = Plot::timedomain_chart(
                    "vdop",
                    Mode::Markers,
                    MarkerSymbol::Cross,
                    &epochs,
                    vdop,
                    true,
                );
                plot.add_trace(trace);

                let hdop = solutions
                    .iter()
                    .map(|(_, sol)| sol.hdop)
                    .collect::<Vec<_>>();

                let trace = Plot::timedomain_chart(
                    "hdop",
                    Mode::Markers,
                    MarkerSymbol::Cross,
                    &epochs,
                    hdop,
                    true,
                );
                plot.add_trace(trace);
                plot
            },
            clk_plot: {
                let mut plot =
                    Plot::timedomain_plot("clk_offset", "Clock Offset", "Offset [s]", true);

                let clock_offset = solutions
                    .iter()
                    .map(|(_, sol)| sol.clock_offset_s)
                    .collect::<Vec<_>>();

                let trace = Plot::timedomain_chart(
                    "offset",
                    Mode::Markers,
                    MarkerSymbol::Cross,
                    &epochs,
                    clock_offset,
                    true,
                );
                plot.add_trace(trace);
                plot
            },
            drift_plot: {
                let mut plot =
                    Plot::timedomain_plot("clk_drift", "Clock Drift", "Drift [s/s]", true);

                // let clock_drift = solutions
                //     .iter()
                //     .map(|(_, sol)| sol.clock_drift_s_s)
                //     .collect::<Vec<_>>();

                // let trace = Plot::timedomain_chart(
                //     "drift",
                //     Mode::Markers,
                //     MarkerSymbol::Cross,
                //     &epochs,
                //     clock_drift,
                //     true,
                // );
                // plot.add_trace(trace);
                plot
            },

            coords_err_plot: if let Some((x0_m, y0_m, z0_m)) = x0y0z0_m {
                let mut plot = Plot::timedomain_plot("xy_plot", "X/Y/Z Error", "Error [m]", true);

                let trace = Plot::timedomain_chart(
                    "x err",
                    Mode::Markers,
                    MarkerSymbol::Cross,
                    &epochs,
                    solutions.values().map(|sol| sol.pos_m.0 - x0_m).collect(),
                    true,
                );

                plot.add_trace(trace);

                let trace = Plot::timedomain_chart(
                    "y err",
                    Mode::Markers,
                    MarkerSymbol::Cross,
                    &epochs,
                    solutions.values().map(|sol| sol.pos_m.1 - y0_m).collect(),
                    true,
                );

                plot.add_trace(trace);

                let trace = Plot::timedomain_chart(
                    "z err",
                    Mode::Markers,
                    MarkerSymbol::Cross,
                    &epochs,
                    solutions.values().map(|sol| sol.pos_m.2 - z0_m).collect(),
                    true,
                );

                plot.add_trace(trace);

                Some(plot)
            } else {
                None
            },

            coords_err3d_plot: if let Some((x0_m, y0_m, z0_m)) = x0y0z0_m {
                let mut plot = Plot::plot_3d(
                    "3d_sphere",
                    "3D errors",
                    "X error [m]",
                    "Y Error [m]",
                    "Z Error [m]",
                    true,
                );

                let trace = Plot::chart_3d(
                    "Error",
                    Mode::Markers,
                    MarkerSymbol::Cross,
                    &epochs,
                    solutions.values().map(|sol| sol.pos_m.0 - x0_m).collect(),
                    solutions.values().map(|sol| sol.pos_m.1 - y0_m).collect(),
                    solutions.values().map(|sol| sol.pos_m.2 - z0_m).collect(),
                );

                plot.add_trace(trace);

                Some(plot)
            } else {
                None
            },

            summary,
        }
    }
}

impl Render for ReportContent {
    fn render(&self) -> Markup {
        html! {
            div class="table-container" {
                table class="table is-bordered" {
                    tbody {
                        tr {
                            th class="is-info" {
                                "Summary"
                            }
                            td {
                                (self.summary.render())
                            }
                        }
                        tr {
                            th class="is-info" {
                                "Map Proj"
                            }
                            td {
                                (self.map_proj.render())
                            }
                        }
                        tr {
                            th class="is-info" {
                                button aria-label="SV Contribution over time" data-balloon-pos="right" {
                                    "SV Plot"
                                }
                            }
                            td {
                                (self.sv_plot.render())
                            }
                        }
                        tr {
                            th class="is-info" {
                                button aria-label="Receiver Clock Offset with respected to Timescale" data-balloon-pos="right" {
                                    "Clock offset"
                                }
                            }
                            td {
                                (self.clk_plot.render())
                            }
                        }
                        tr {
                            th class="is-info" {
                                button aria-label="Receiver Clock drift from Timescale" data-balloon-pos="right" {
                                    "Clock drift"
                                }
                            }
                            td {
                                (self.drift_plot.render())
                            }
                        }
                        tr {
                            th class="is-info" {
                                "Coordinates"
                            }
                            td {
                                (self.ddeg_plot.render())
                            }
                        }
                        tr {
                            th class="is-info" {
                                "Altitude"
                            }
                            td {
                                (self.altitude_plot.render())
                            }
                        }

                        @ if let Some(plot) = &self.coords_err_plot {
                            tr {
                                th class="is-info" {
                                    button aria-label="3D errors (surveying applications only)" data-balloon-pos="right" {
                                        "Coordinates Error"
                                    }
                                }
                                td {
                                    table class="table is-bordered" {
                                        tr {
                                            th class="is-info" {
                                                "Errors"
                                            }
                                            td {
                                                (plot.render())
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        @ if let Some(plot) = &self.coords_err3d_plot {
                            tr {
                                th class="is-info" {
                                    button aria-label="3D errors (surveying applications only)" data-balloon-pos="right" {
                                        "Coordinates Error"
                                    }
                                }
                                td {
                                    table class="table is-bordered" {
                                        tr {
                                            th class="is-info" {
                                                "3D Errors"
                                            }
                                            td {
                                                (plot.render())
                                            }
                                        }
                                    }
                                }
                            }

                        }

                        tr {
                            th class="is-info" {
                                "Velocity"
                            }
                            td {
                                (self.vel_plot.render())
                            }
                        }

                        tr {
                            th class="is-info" {
                                "DOP"
                            }
                            td {
                                div class="table-container" {
                                    table class="table is-bordered" {
                                        tr {
                                            th class="is-info" {
                                                "Geometric DOP"
                                            }
                                            td {
                                                (self.dop_plot.render())
                                            }
                                        }
                                        tr {
                                            th class="is-info" {
                                                "Temporal DOP"
                                            }
                                            td {
                                                (self.tdop_plot.render())
                                            }
                                        }
                                        button aria-label="Geometric Dillution of Precision" data-balloon-pos="right" {
                                        }
                                    }
                                }
                            }
                        }
                        tr {
                            th class="is-info" {
                                button aria-label="Error due to Tropospheric delay" data-balloon-pos="right" {
                                    "Troposphere"
                                }
                            }
                            td {
                                (self.tropod_plot.render())
                            }
                        }
                        tr {
                            th class="is-info" {
                                button aria-label="Error due to Ionospheric delay" data-balloon-pos="right" {
                                    "Ionosphere"
                                }
                            }
                            td {
                                (self.ionod_plot.render())
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Solutions report
pub struct Report {
    tab: ReportTab,
    content: ReportContent,
}

impl Report {
    pub fn formalize(self) -> QcExtraPage {
        QcExtraPage {
            tab: Box::new(self.tab),
            html_id: "ppp".to_string(),
            content: Box::new(self.content),
        }
    }

    pub fn new(
        cfg: &NaviConfig,
        ctx: &Context,
        user_profile: UserProfile,
        clock_profile: ClockProfile,
        solutions: &BTreeMap<Epoch, PVTSolution>,
    ) -> Self {
        Self {
            tab: ReportTab {},
            content: ReportContent::new(cfg, ctx, user_profile, clock_profile, solutions),
        }
    }
}
