use log::info;

use std::cell::RefCell;

use gnss_rtk::prelude::{BiasRuntime, Duration, Epoch, SatelliteClockCorrection, SpacebornBias};

use crate::positioning::EphemerisBuffer;

pub struct SpacebornBiases<'a, 'b> {
    buffer: &'a RefCell<EphemerisBuffer<'b>>,
}

impl<'a, 'b> SpacebornBias for SpacebornBiases<'a, 'b> {
    fn clock_bias(&self, rtm: &BiasRuntime) -> SatelliteClockCorrection {
        Default::default()
    }

    fn group_delay(&self, rtm: &BiasRuntime) -> Duration {
        Duration::ZERO
    }

    fn mw_bias(&self, _: &BiasRuntime) -> f64 {
        Default::default()
    }
}

impl<'a, 'b> SpacebornBiases<'a, 'b> {
    pub fn new(buffer: &'a RefCell<EphemerisBuffer<'b>>) -> Self {
        info!("spaceborn biases created & deployed");
        Self { buffer }
    }
}
