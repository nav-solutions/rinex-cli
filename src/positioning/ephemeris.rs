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

#[derive(Clone)]
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
        // discard past invalid
        self.buffer.retain(|frm| {
            if frm.toe < epoch {
                frm.ephemeris.is_valid(frm.sv, epoch)
            } else {
                true
            }
        });

        // iterate until EoS or frame becomes future + invalid
        loop {
            if self.eos {
                break;
            }

            if let Some(frame) = self.iter.next() {
                if frame.toe > epoch && !frame.ephemeris.is_valid(frame.sv, epoch) {
                    self.buffer.push(frame);
                    break;
                }

                self.buffer.push(frame);
            } else {
                self.eos = true;
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

    pub fn select_sv_ephemeris(&self, epoch: Epoch, sv: SV) -> Option<EphemerisFrame> {
        if sv.constellation.is_sbas() {
            error!("{} - sbas not supported yet!", epoch);
            None
        } else {
            self.buffer
                .iter()
                .filter(|frm| frm.sv == sv && frm.ephemeris.is_valid(sv, epoch))
                .min_by_key(|frm| (epoch - frm.toe).abs())
                .cloned()
        }
    }
}
