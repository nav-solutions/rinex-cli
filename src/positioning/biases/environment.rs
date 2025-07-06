use log::info;

use gnss_rtk::prelude::{BiasRuntime, EnvironmentalBias, TroposphereModel};

pub struct EnvironmentalBiases {}

impl EnvironmentalBias for EnvironmentalBiases {
    fn ionosphere_bias_m(&self, _: &BiasRuntime) -> f64 {
        // TODO
        0.0
    }

    fn troposphere_bias_m(&self, rtm: &BiasRuntime) -> f64 {
        TroposphereModel::Niel.bias_m(rtm)
    }
}

//pub fn tropo_components(meteo: Option<&Rinex>, t: Epoch, lat_ddeg: f64) -> Option<(f64, f64)> {
//    const MAX_LATDDEG_DELTA: f64 = 15.0;
//    let max_dt = Duration::from_hours(24.0);
//    let rnx = meteo?;
//    let meteo = rnx.header.meteo.as_ref().unwrap();
//
//    let delays: Vec<(Observable, f64)> = meteo
//        .sensors
//        .iter()
//        .filter_map(|s| match s.observable {
//            Observable::ZenithDryDelay => {
//                let (x, y, z, _) = s.position?;
//                let (lat, _, _) = ecef2geodetic(x, y, z, Ellipsoid::WGS84);
//                let lat = rad2deg(lat);
//                if (lat - lat_ddeg).abs() < MAX_LATDDEG_DELTA {
//                    let value = rnx
//                        .zenith_dry_delay()
//                        .filter(|(t_sens, _)| (*t_sens - t).abs() < max_dt)
//                        .min_by_key(|(t_sens, _)| (*t_sens - t).abs());
//                    let (_, value) = value?;
//                    debug!("{:?} lat={} zdd {}", t, lat_ddeg, value);
//                    Some((s.observable.clone(), value))
//                } else {
//                    None
//                }
//            },
//            Observable::ZenithWetDelay => {
//                let (x, y, z, _) = s.position?;
//                let (mut lat, _, _) = ecef2geodetic(x, y, z, Ellipsoid::WGS84);
//                lat = rad2deg(lat);
//                if (lat - lat_ddeg).abs() < MAX_LATDDEG_DELTA {
//                    let value = rnx
//                        .zenith_wet_delay()
//                        .filter(|(t_sens, _)| (*t_sens - t).abs() < max_dt)
//                        .min_by_key(|(t_sens, _)| (*t_sens - t).abs());
//                    let (_, value) = value?;
//                    debug!("{:?} lat={} zdd {}", t, lat_ddeg, value);
//                    Some((s.observable.clone(), value))
//                } else {
//                    None
//                }
//            },
//            _ => None,
//        })
//        .collect();
//
//    if delays.len() < 2 {
//        None
//    } else {
//        let zdd = delays
//            .iter()
//            .filter_map(|(obs, value)| {
//                if obs == &Observable::ZenithDryDelay {
//                    Some(*value)
//                } else {
//                    None
//                }
//            })
//            .reduce(|k, _| k)
//            .unwrap();
//
//        let zwd = delays
//            .iter()
//            .filter_map(|(obs, value)| {
//                if obs == &Observable::ZenithWetDelay {
//                    Some(*value)
//                } else {
//                    None
//                }
//            })
//            .reduce(|k, _| k)
//            .unwrap();
//
//        Some((zwd, zdd))
//    }
//}

// /*
//  * Grabs nearest BD model (in time)
//  */
// pub fn bd_model(nav: &Rinex, t: Epoch) -> Option<BdModel> {
//     let (_, model) = nav
//         .nav_bdgim_models_iter()
//         .min_by_key(|(k_i, _)| (k_i.epoch - t).abs())?;

//     Some(BdModel { alpha: model.alpha })
// }

// /*
//  * Grabs nearest NG model (in time)
//  */
// pub fn ng_model(nav: &Rinex, t: Epoch) -> Option<NgModel> {
//     let (_, model) = nav
//         .nav_nequickg_models_iter()
//         .min_by_key(|(k_i, _)| (k_i.epoch - t).abs())?;

//     Some(NgModel { a: model.a })
// }

// /// Returns a [KbModel]
// pub fn kb_model(nav: &Rinex, t: Epoch) -> Option<KbModel> {
//     let (nav_key, model) = nav
//         .nav_klobuchar_models_iter()
//         .min_by_key(|(k_i, _)| (k_i.epoch - t).abs())?;
//
//     Some(KbModel {
//         h_km: {
//             match nav_key.sv.constellation {
//                 Constellation::BeiDou => 375.0,
//                 // we only expect GPS or BDS here,
//                 // badly formed RINEX will generate errors in the solutions
//                 _ => 350.0,
//             }
//         },
//         alpha: model.alpha,
//         beta: model.beta,
//     })
// }

impl EnvironmentalBiases {
    pub fn new() -> Self {
        info!("environemental biases model created & deployed");
        Self {}
    }
}
