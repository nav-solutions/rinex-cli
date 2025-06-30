use crate::{
    cli::Context,
    positioning::{Buffer, CenteredDataPoints, CenteredSnapshot, EphemerisSource},
};

use std::{cell::RefCell, collections::HashMap};

use gnss_rtk::prelude::{Duration, Epoch, SatelliteClockCorrection, SV};

pub trait ClockStateProvider {
    fn next_clock_at(&mut self, t: Epoch, sv: SV) -> Option<ClockCorrection>;
}

impl CenteredDataPoints<f64> for f64 {
    fn zero() -> f64 {
        0.0
    }
}

pub struct Clock<'a, 'b> {
    eos: bool,
    has_precise: bool,
    sampling_period: Duration,
    sv_buffers: HashMap<SV, Buffer<f64>>,
    eph: &'a RefCell<EphemerisSource<'b>>,
    sv_snapshots: HashMap<SV, CenteredSnapshot<2, f64>>,
    iter: Box<dyn Iterator<Item = (Epoch, SV, f64)> + 'a>,
}

impl ClockStateProvider for Clock<'_, '_> {
    fn next_clock_at(&mut self, t: Epoch, sv: SV) -> Option<ClockCorrection> {
        if self.has_precise {
            let dt = self.next_precise_at(t, sv)?;
            let dt = Duration::from_seconds(dt);
            debug!("{} ({}) - clock correction: {}", t, sv, dt);
            return Some(ClockCorrection::without_relativistic_correction(dt));
        } else {
            let (toc, _, eph) = self.eph.borrow_mut().select(t, sv)?;
            let dt = eph.clock_correction(toc, t, sv, 8)?;
            debug!("{} ({}) - clock correction: {}", t, sv, dt);
            Some(ClockCorrection::without_relativistic_correction(dt))
        }
    }
}

impl<'a, 'b> Clock<'a, 'b> {
    pub fn new(ctx: &'a Context, eph: &'a RefCell<EphemerisSource<'b>>) -> Self {
        let has_precise = ctx.data.clock().is_some();

        let mut s = Self {
            eph,
            has_precise,
            eos: false,
            sampling_period: if let Some(clk) = ctx.data.clock() {
                let dt = clk
                    .dominant_sampling_interval()
                    .expect("Invalid clock RINEX: undefined sampling interval");
                dt
            } else {
                Duration::default()
            },
            sv_buffers: HashMap::with_capacity(16),
            sv_snapshots: HashMap::with_capacity(16),
            iter: if let Some(clk) = ctx.data.clock() {
                info!("Clock source created: operating with Precise Clocks.");
                Box::new(
                    clk.precise_sv_clock()
                        .map(|(t, sv, _, prof)| (t, sv, prof.bias)),
                )
            } else {
                if let Some(sp3) = ctx.data.sp3() {
                    if sp3.has_satellite_clock_offset() {
                        info!("Clock source created: operating with SP3 Clocks.");
                        Box::new(sp3.satellites_clock_offset_sec_iter())
                    } else {
                        warn!("Clock source created: operating without Precise Clock.");
                        Box::new([].into_iter())
                    }
                } else {
                    warn!("Clock source created: operating without Precise Clock.");
                    Box::new([].into_iter())
                }
            },
        };

        if s.has_precise {
            // fill in buffer
            s.consume_many(128);
        }

        s
    }

    fn consume_one(&mut self) {
        if let Some((t, sv, dt)) = self.iter.next() {
            if let Some(buf) = self.sv_buffers.get_mut(&sv) {
                buf.push(t, dt);
            } else {
                let mut buf = Buffer::<f64>::new(8);
                buf.push(t, dt);
                self.sv_buffers.insert(sv, buf);
            }
        } else {
            self.eos = true;
            info!("Consumed all precise clocks.");
        }
    }

    fn consume_many(&mut self, n: usize) {
        let mut i = 0;
        while !self.eos && i < n {
            self.consume_one();
            i += 1;
        }
    }

    fn next_precise_at(&mut self, t: Epoch, sv: SV) -> Option<f64> {
        let next_t = t + self.sampling_period;

        while !self.eos {
            self.consume_many(16);

            if let Some(buffer) = self.sv_buffers.get(&sv) {
                if buffer.last_t >= next_t {
                    break;
                }
            }
        }

        // centered buffer
        if self.sv_snapshots.get(&sv).is_none() {
            self.sv_snapshots.insert(sv, CenteredSnapshot::new());
        }

        let sv_snapshot = self.sv_snapshots.get_mut(&sv).unwrap();

        let sv_buffer = self.sv_buffers.get_mut(&sv)?;
        sv_buffer.centered_snapshot(t, self.sampling_period, sv_snapshot);

        if !sv_snapshot.centered(t, self.sampling_period) {
            return None;
        }

        let t = sv_snapshot.interpolate(|buf| {
            buf[0].1
            // let (t_0, dt_0) = buf[0];
            // let (t_1, dt_1) = buf[1];
            // let delta_s = (t_1 - t_0).to_seconds();
            // let mut dt = (t_1 - t).to_seconds() / delta_s * dt_0;
            // dt += (t - t_0).to_seconds() / delta_s * dt_1;
            // dt
        });

        Some(t)
    }
}
