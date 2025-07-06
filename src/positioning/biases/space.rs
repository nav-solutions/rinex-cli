use log::info;

use std::cell::RefCell;

use gnss_rtk::prelude::{BiasRuntime, Duration, Epoch, SatelliteClockCorrection, SpacebornBias};

use crate::positioning::EphemerisBuffer;

pub struct SpacebornBiases<'a, 'b> {
    buffer: &'a RefCell<&'a EphemerisBuffer<'b>>,
}

impl<'a, 'b> SpacebornBias for SpacebornBiases<'a, 'b> {
    fn clock_bias(&self, rtm: &BiasRuntime) -> SatelliteClockCorrection {
        if let Some(frm) = self.buffer.borrow().select_sv_ephemeris(rtm.epoch, rtm.sv) {
            if let Some(dt) = frm
                .ephemeris
                .clock_correction(frm.toc, rtm.epoch, rtm.sv, 10)
            {
                SatelliteClockCorrection::without_relativistic_correction(dt)
            } else {
                Default::default()
            }
        } else {
            Default::default()
        }
    }

    fn group_delay(&self, rtm: &BiasRuntime) -> Duration {
        if let Some(frame) = self.buffer.borrow().select_sv_ephemeris(rtm.epoch, rtm.sv) {
            if let Some(tgd) = frame.ephemeris.tgd() {
                tgd
            } else {
                Duration::ZERO
            }
        } else {
            Duration::ZERO
        }
    }

    fn mw_bias(&self, _: &BiasRuntime) -> f64 {
        // MW bias not supplied nor supported yet
        Default::default()
    }
}

impl<'a, 'b> SpacebornBiases<'a, 'b> {
    pub fn new(buffer: &'a RefCell<&'a EphemerisBuffer<'b>>) -> Self {
        info!("spaceborn biases created & deployed");
        Self { buffer }
    }
}
