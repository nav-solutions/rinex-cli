use log::info;

use std::cell::RefCell;
use std::rc::Rc;

use gnss_rtk::prelude::{BiasRuntime, Duration, Epoch, SatelliteClockCorrection, SpacebornBias};

use crate::positioning::EphemerisBuffer;

pub struct SpacebornBiases<'a> {
    buffer: Rc<RefCell<EphemerisBuffer<'a>>>,
}

impl<'a> SpacebornBias for SpacebornBiases<'a> {
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

impl<'a> SpacebornBiases<'a> {
    pub fn new(buffer: Rc<RefCell<EphemerisBuffer<'a>>>) -> Self {
        info!("spaceborn biases created & deployed");
        Self { buffer }
    }
}
