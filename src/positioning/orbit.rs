use crate::{
    cli::Context,
    positioning::{Buffer, CenteredSnapshot, Coords3d, EphemerisSource, PreciseOrbits},
};

use anise::errors::AlmanacError;
use rinex::carrier::Carrier;

use gnss_rtk::prelude::{
    Almanac, Duration, Epoch, Frame, Orbit, OrbitSource, Vector3, EARTH_J2000, SUN_J2000, SV,
};

use std::{cell::RefCell, collections::HashMap};

pub struct Orbits<'a, 'b> {
    eos: bool,
    has_precise: bool,
    eph: &'a RefCell<EphemerisSource<'b>>,
    precise: RefCell<PreciseOrbits<'a>>,
}

impl<'a, 'b> Orbits<'a, 'b> {
    pub fn new(ctx: &'a Context, eph: &'a RefCell<EphemerisSource<'b>>) -> Self {
        let has_precise = ctx.data.has_sp3();
        let precise = RefCell::new(PreciseOrbits::new(ctx));

        Self {
            eph,
            precise,
            eos: false,
            has_precise,
        }
    }
}

impl OrbitSource for Orbits<'_, '_> {
    fn state_at(&self, t: Epoch, sv: SV, frame: Frame) -> Option<Orbit> {
        if self.has_precise {
            let mut precise_orbits = self.precise.borrow_mut();
            let orbit = precise_orbits.next_precise_at(t, sv, frame)?;
            let state = orbit.to_cartesian_pos_vel();

            let (x_km, y_km, z_km) = (state[0], state[1], state[2]);

            debug!(
                "{} ({}) - precise state : x={}, y={}, z={} (km, ECEF)",
                t.round(Duration::from_milliseconds(1.0)),
                sv,
                x_km,
                y_km,
                z_km
            );

            Some(orbit)
        } else {
            let (toc, _, eph) = self.eph.borrow_mut().select(t, sv)?;
            let orbit = eph.kepler2position(sv, t)?;
            let state = orbit.to_cartesian_pos_vel();
            let (x_km, y_km, z_km) = (state[0], state[1], state[2]);

            debug!(
                "{} ({}) - keplerian state : x={}, y={}, z={} (km, ECEF)",
                t.round(Duration::from_milliseconds(1.0)),
                sv,
                x_km,
                y_km,
                z_km
            );

            Some(orbit)
        }
    }
}
