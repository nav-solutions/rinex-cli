use csv::Writer;
use std::{fs::File, io::Write, path::Path};

use itertools::Itertools;

use crate::{
    cli::Context,
    fops::{custom_prod_attributes, output_filename},
    Error,
};

use gnss_qc::prelude::{ProductType, Rinex};

use clap::ArgMatches;

pub fn dump_context_csv(
    ctx: &Context,
    matches: &ArgMatches,
    submatches: &ArgMatches,
) -> Result<(), Error> {
    let ctx_data = &ctx.data;

    ctx.workspace.create_subdir("CSV");

    // OBS RINEX
    if let Some(rinex) = ctx_data.rinex(ProductType::Observation) {
        let prod = custom_prod_attributes(rinex, submatches);

        let output = ctx
            .workspace
            .root
            .join("CSV")
            .join(output_filename(rinex, matches, submatches, prod));

        write_obs_rinex(rinex, &output)?;

        info!(
            "{} dumped in {}",
            ProductType::Observation,
            output.display()
        );
    }

    // NAV RINEX
    if let Some(brdc) = ctx_data.rinex(ProductType::BroadcastNavigation) {
        ctx.workspace.create_subdir("BRDC");

        let prod = custom_prod_attributes(brdc, submatches);
        let output_name = output_filename(brdc, matches, submatches, prod);

        let output = ctx.workspace.root.join("BRDC").join(&output_name);

        write_raw_nav_rinex(brdc, &output)?;

        info!(
            "{} dumped in {}",
            ProductType::BroadcastNavigation,
            output.display()
        );

        if let Some(obs) = ctx_data.rinex(ProductType::Observation) {
            ctx.workspace.create_subdir("BRDC+OBS");

            let output = ctx.workspace.root.join("BRDC+OBS").join(&output_name);

            write_joint_nav_obs_rinex(brdc, obs, &output)?;

            info!(
                "{} dumped in {}",
                ProductType::BroadcastNavigation,
                output.display()
            );
        }
    }

    Ok(())
}

fn write_obs_rinex<P: AsRef<Path>>(rnx: &Rinex, path: P) -> Result<(), Error> {
    let mut w = Writer::from_path(path)?;
    w.write_record(&[
        "Epoch",
        "Flag",
        "Clock Offset [s]",
        "SV",
        "RINEX Code",
        "Value",
        "LLI",
        "SNR",
    ])?;

    for (k, v) in rnx.observations_iter() {
        let t = k.epoch.to_string();
        let flag = k.flag.to_string();

        let clk = if let Some(clk) = v.clock {
            format!("{:.15}E", clk.offset_s)
        } else {
            "None".to_string()
        };

        for signal in v.signals.iter() {
            let sv = signal.sv.to_string();
            let code = signal.observable.to_string();
            let value = format!("{:.15E}", signal.value);

            let lli = if let Some(lli) = signal.lli {
                format!("{:?}", lli)
            } else {
                "None".to_string()
            };

            let snr = if let Some(snr) = signal.snr {
                format!("{:?}", snr)
            } else {
                "None".to_string()
            };

            w.write_record(&[&t, &flag, &clk, &sv, &code, &value, &lli, &snr])?;
        }
    }
    Ok(())
}

fn write_joint_nav_obs_rinex(brdc: &Rinex, obs: &Rinex, path: &Path) -> Result<(), Error> {
    let mut orbit_w = Writer::from_path(path)?;
    orbit_w.write_record(&["Epoch", "SV", "x_ecef_km", "y_ecef_km", "z_ecef_km"])?;

    let parent = path.parent().unwrap();
    let stem = path.file_stem().unwrap().to_string_lossy().to_string();

    let clk_path = parent.join(&format!("{}-clock.csv", stem));
    let mut clk_w = Writer::from_path(clk_path)?;
    clk_w.write_record(&["Epoch", "SV", "correction"])?;

    for (k, v) in obs.observations_iter() {
        let t_str = k.epoch.to_string();

        for sv in v.signals.iter().map(|sig| sig.sv).unique() {
            let sv_str = sv.to_string();

            if let Some((toc, _, eph)) = brdc.nav_ephemeris_selection(sv, k.epoch) {
                if let Some(sv_orbit) = eph.kepler2position(sv, k.epoch) {
                    let sv_state = sv_orbit.to_cartesian_pos_vel();
                    let (x_km, y_km, z_km) = (sv_state[0], sv_state[1], sv_state[2]);

                    orbit_w.write_record(&[
                        &t_str,
                        &sv_str,
                        &format!("{:.15E}", x_km),
                        &format!("{:.15E}", y_km),
                        &format!("{:.15E}", z_km),
                    ])?;

                    if let Some(correction) = eph.clock_correction(toc, k.epoch, sv, 8) {
                        clk_w.write_record(&[
                            &t_str,
                            &sv_str,
                            &format!("{:.15E}", correction.to_seconds()),
                        ])?;
                    }
                }
            }
        }
    }
    Ok(())
}

fn write_raw_nav_rinex(brdc: &Rinex, path: &Path) -> Result<(), Error> {
    let mut fd = File::create(path)
        .unwrap_or_else(|e| panic!("Failed to create file: {} - {}", path.display(), e));

    let record = brdc.record.as_nav().unwrap();

    for (k, v) in record.iter() {
        let t = k.epoch;
        let sv = k.sv;
        let msgtype = k.msgtype;
        let frmtype = k.frmtype;

        if let Some(eph) = v.as_ephemeris() {
            let (bias, drift, drift_r) = eph.sv_clock();

            write!(fd, "{}, ", t)?;
            write!(fd, "{}, ", sv)?;
            write!(fd, "{}, ", msgtype)?;
            write!(fd, "{}, ", frmtype)?;

            write!(fd, "bias (s), {:.15E}, ", bias)?;
            write!(fd, "drift (s/s), {:.15E}, ", drift)?;
            write!(fd, "drift_r (s/s2), {:.15E}, ", drift_r)?;

            write!(fd, "healthy, {}, ", eph.sv_healthy())?;

            if let Some(tgd) = eph.tgd() {
                write!(fd, "tgd, {}, ", tgd)?;
            }

            let num_orbits = eph.orbits.len();

            for (num, (name, orbit)) in eph.orbits.iter().enumerate() {
                if name != "tgd" {
                    write!(fd, "{}, ", name)?;
                }

                match name.as_str() {
                    "health" => {
                        if let Some(health) = orbit.as_gps_qzss_l1l2l5_health_flag() {
                            write!(fd, "{:?}", health)?;
                        } else if let Some(health) = orbit.as_gps_qzss_l1c_health_flag() {
                            write!(fd, "{:?}", health)?;
                        } else if let Some(health) = orbit.as_glonass_health_flag() {
                            write!(fd, "{:?}", health)?;
                        } else if let Some(health) = orbit.as_galileo_health_flag() {
                            write!(fd, "{:?}", health)?;
                        } else if let Some(health) = orbit.as_bds_sat_h1_flag() {
                            write!(fd, "{:?}", health)?;
                        } else if let Some(health) = orbit.as_bds_health_flag() {
                            write!(fd, "{:?}", health)?;
                        } else {
                            write!(fd, "{:.14E}", orbit.as_f64())?;
                        }
                    },
                    "health2" => {
                        if let Some(health) = orbit.as_glonass_health2_flag() {
                            write!(fd, "{:?}", health)?;
                        } else {
                            write!(fd, "{:.14E}", orbit.as_f64())?;
                        }
                    },
                    "source" => {
                        write!(fd, "{:.14E}", orbit.as_f64())?;
                    },
                    "satType" => {
                        if let Some(sat_type) = orbit.as_bds_satellite_type() {
                            write!(fd, "{:?}", sat_type)?;
                        } else {
                            write!(fd, "{:.14E}", orbit.as_f64())?;
                        }
                    },
                    "integrity" => {
                        if let Some(integrity) = orbit.as_bds_b1c_integrity() {
                            write!(fd, "{:?}", integrity)?;
                        } else if let Some(integrity) = orbit.as_bds_b2a_b1c_integrity() {
                            write!(fd, "{:?}", integrity)?;
                        } else if let Some(integrity) = orbit.as_bds_b2b_integrity() {
                            write!(fd, "{:?}", integrity)?;
                        } else {
                            write!(fd, "{:.14E}", orbit.as_f64())?;
                        }
                    },
                    "status" => {
                        if let Some(status) = orbit.as_glonass_status_mask() {
                            write!(fd, "{:?}", status)?;
                        } else {
                            write!(fd, "{:.14E}", orbit.as_f64())?;
                        }
                    },
                    "tgd" => {},
                    _ => write!(fd, "{:.14E}", orbit.as_f64())?,
                }

                if num != num_orbits - 1 {
                    write!(fd, ", ")?;
                } else {
                    write!(fd, "\n")?;
                }
            }
        }
    }

    Ok(())
}
