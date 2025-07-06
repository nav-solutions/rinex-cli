// Ephemeris interface is not used by this application

use gnss_rtk::prelude::{Ephemeris as RawEphemeris, EphemerisSource, Epoch, SV};

use rinex::navigation::Ephemeris;

use crate::cli::Context;

pub struct NullEphemerisSource {}

impl NullEphemerisSource {
    pub fn new() -> Self {
        Self {}
    }
}

impl EphemerisSource for NullEphemerisSource {
    // Ephemeris interface is not used by this application
    fn ephemeris_data(&self, _: Epoch, _: SV) -> Option<RawEphemeris> {
        None
    }
}

pub struct EphemerisFrame {
    /// [SV]
    pub sv: SV,

    /// ToC as [Epoch]
    pub toc: Epoch,

    /// ToE as [Epoch]
    pub toe: Epoch,

    /// [Ephemeris]
    pub ephemeris: Ephemeris,
}

pub struct EphemerisBuffer<'a> {
    /// End of ephemeris Stream
    eos: bool,

    /// buffer
    buffer: Vec<EphemerisFrame>,

    /// Iterator
    iter: Box<dyn Iterator<Item = EphemerisFrame> + 'a>,
}

impl<'a> EphemerisBuffer<'a> {
    pub fn new_epoch(&mut self, epoch: Epoch) {
        // iterate until EoS or frame becomes future + invalid
        if !self.eos {
            loop {
                if let Some(frame) = self.iter.next() {
                    if frame.toe > epoch && !frame.ephemeris.is_valid(frame.sv, epoch) {
                        self.buffer.push(frame);
                        break;
                    }

                    self.buffer.push(frame);
                } else {
                    self.eos = true;
                    break;
                }
            }
        }
    }

    pub fn new(ctx: &'a Context) -> Self {
        let brdc = ctx
            .data
            .brdc_navigation()
            .expect("Navigation RINEX is required by post-processed navigation");

        let s = Self {
            eos: false,
            buffer: Vec::with_capacity(16),
            iter: Box::new(brdc.nav_ephemeris_frames_iter().filter_map(|(k, v)| {
                if let Some(toe) = v.toe(k.sv) {
                    Some(EphemerisFrame {
                        toe,
                        sv: k.sv,
                        toc: k.epoch,
                        ephemeris: v.clone(),
                    })
                } else {
                    error!(
                        "{}({}) - ephemeris error (non supported constellation?)",
                        k.epoch, k.sv
                    );
                    None
                }
            })),
        };

        info!("Ephemeris buffer created");

        s
    }
}

// pub struct EphemerisFrame {
//     pub sv: SV,
// }
//
// pub struct EphemerisSource<'a> {
//     sv: SV,
//     eos: bool,
//     toc: Epoch,
//     buffer: HashMap<SV, Vec<(Epoch, Epoch, Ephemeris)>>,
//     iter: Box<dyn Iterator<Item = (SV, Epoch, Epoch, &'a Ephemeris)> + 'a>,
// }
//
// impl<'a> EphemerisSource<'a> {
//     /// Consume one entry from [Iterator]
//     fn consume_one(&mut self) {
//         if let Some((sv, toc, toe, eph)) = self.iter.next() {
//             if let Some(buffer) = self.buffer.get_mut(&sv) {
//                 buffer.push((toc, toe, eph.clone()));
//             } else {
//                 self.buffer.insert(sv, vec![(toc, toe, eph.clone())]);
//             }
//             self.sv = sv;
//             self.toc = toc;
//         } else {
//             if !self.eos {
//                 info!("{}({}): consumed all epochs", self.toc, self.sv);
//             }
//             self.eos = true;
//         }
//     }
//
//     /// Consume n entries from [Iterator]
//     fn consume_many(&mut self, n: usize) {
//         for _ in 0..n {
//             self.consume_one();
//         }
//     }
//
//     /// [Ephemeris] selection attempt, for [SV] at [Epoch]
//     fn try_select(&self, t: Epoch, sv: SV) -> Option<(Epoch, Epoch, &Ephemeris)> {
//         let buffer = self.buffer.get(&sv)?;
//
//         if sv.constellation.is_sbas() {
//             buffer
//                 .iter()
//                 .filter_map(|(toc_i, toe_i, eph_i)| {
//                     if t >= *toc_i {
//                         Some((*toc_i, *toe_i, eph_i))
//                     } else {
//                         None
//                     }
//                 })
//                 .min_by_key(|(toc_i, _, _)| (t - *toc_i).abs())
//         } else {
//             buffer
//                 .iter()
//                 .filter_map(|(toc_i, toe_i, eph_i)| {
//                     if eph_i.is_valid(sv, t) {
//                         Some((*toc_i, *toe_i, eph_i))
//                     } else {
//                         None
//                     }
//                 })
//                 .min_by_key(|(_, toe_i, _)| (t - *toe_i).abs())
//         }
//     }
//
//     /// [Ephemeris] selection at [Epoch] for [SV].
//     pub fn select(&mut self, t: Epoch, sv: SV) -> Option<(Epoch, Epoch, Ephemeris)> {
//         loop {
//             if let Some((toc_i, toe_i, eph_i)) = self.try_select(t, sv) {
//                 return Some((toc_i, toe_i, eph_i.clone()));
//             } else {
//                 self.consume_one();
//                 if self.eos {
//                     return None;
//                 }
//             }
//         }
//     }
// }
