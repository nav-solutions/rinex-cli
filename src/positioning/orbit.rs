use crate::{
    cli::Context,
    positioning::{Buffer, CenteredSnapshot, Coords3d, EphemerisBuffer, PreciseOrbits},
};

use anise::errors::AlmanacError;
use rinex::carrier::Carrier;

use gnss_rtk::prelude::{
    Almanac, Duration, Epoch, Frame, Orbit, OrbitSource, Rc, Vector3, EARTH_J2000, SUN_J2000, SV,
};

use std::{cell::RefCell, collections::HashMap};

pub struct Orbits<'a> {
    // eos_precise: bool,
    has_precise: bool,
    // has_precise: bool,
    // eph: &'a RefCell<EphemerisSource<'b>>,
    // precise: RefCell<PreciseOrbits<'a>>,
    ephemeris_buffer: Rc<RefCell<EphemerisBuffer<'a>>>,
}

impl<'a> Orbits<'a> {
    pub fn new(ctx: &'a Context, ephemeris_buffer: Rc<RefCell<EphemerisBuffer<'a>>>) -> Self {
        // let has_precise = ctx.data.has_sp3();
        // let precise = RefCell::new(PreciseOrbits::new(ctx));
        Self {
            ephemeris_buffer,
            has_precise: false,
        }
    }
}

impl OrbitSource for Orbits<'_> {
    fn state_at(&self, epoch: Epoch, sv: SV, frame: Frame) -> Option<Orbit> {
        if self.has_precise {
            panic!("not yet");
        } else {
            if let Some(frm) = self
                .ephemeris_buffer
                .borrow()
                .select_sv_ephemeris(epoch, sv)
            {
                if let Some(orbit) = frm.ephemeris.kepler2position(sv, epoch) {
                    let pos_vel_km = orbit.to_cartesian_pos_vel();
                    debug!(
                        "{}({}) - kepler state: x={}km y={}km z={}km",
                        epoch.round(Duration::from_milliseconds(1.0)),
                        sv,
                        pos_vel_km[0],
                        pos_vel_km[1],
                        pos_vel_km[2],
                    );

                    Some(orbit)
                } else {
                    error!("{}({}) - kepler solver failed", epoch, sv);
                    None
                }
            } else {
                error!("{}({}) - no ephemeris available", epoch, sv);
                None
            }
        }
        // if self.has_precise {
        //     let mut precise_orbits = self.precise.borrow_mut();
        //     let orbit = precise_orbits.next_precise_at(t, sv, frame)?;
        //     let state = orbit.to_cartesian_pos_vel();

        //     let (x_km, y_km, z_km) = (state[0], state[1], state[2]);

        //     debug!(
        //         "{} ({}) - precise state : x={}, y={}, z={} (km, ECEF)",
        //         t.round(Duration::from_milliseconds(1.0)),
        //         sv,
        //         x_km,
        //         y_km,
        //         z_km
        //     );

        //     Some(orbit)
        // } else {
    }
}
