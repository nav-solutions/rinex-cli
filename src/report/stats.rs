//! Streamed statistics
/// Running statistics (Welford): count, mean, standard deviation, RMS, extrema
#[derive(Debug, Clone, Copy, Default)]
pub struct Stats {
    pub n: usize,
    mean: f64,
    m2: f64,
    sum_sq: f64,
    pub min: f64,
    pub max: f64,
}

impl Stats {
    pub fn push(&mut self, x: f64) {
        if !x.is_finite() {
            return;
        }
        if self.n == 0 {
            self.min = x;
            self.max = x;
        } else {
            self.min = self.min.min(x);
            self.max = self.max.max(x);
        }
        self.n += 1;
        let delta = x - self.mean;
        self.mean += delta / self.n as f64;
        self.m2 += delta * (x - self.mean);
        self.sum_sq += x * x;
    }

    pub fn mean(&self) -> f64 {
        if self.n == 0 {
            f64::NAN
        } else {
            self.mean
        }
    }

    /// Standard deviation (population)
    pub fn std(&self) -> f64 {
        if self.n == 0 {
            f64::NAN
        } else {
            (self.m2 / self.n as f64).sqrt()
        }
    }

    pub fn rms(&self) -> f64 {
        if self.n == 0 {
            f64::NAN
        } else {
            (self.sum_sq / self.n as f64).sqrt()
        }
    }

    /// Maximal absolute value
    pub fn max_abs(&self) -> f64 {
        if self.n == 0 {
            f64::NAN
        } else {
            self.min.abs().max(self.max.abs())
        }
    }
}

/// Formats a value with a fixed number of decimals ("-" when not defined)
pub fn fmt(x: f64, decimals: usize) -> String {
    if x.is_finite() {
        format!("{:.*}", decimals, x)
    } else {
        "-".to_string()
    }
}

#[cfg(test)]
mod test {
    use super::Stats;

    #[test]
    fn statistics() {
        let mut stats = Stats::default();
        for x in [1.0, 2.0, 3.0, 4.0, f64::NAN] {
            stats.push(x);
        }
        assert_eq!(stats.n, 4);
        assert!((stats.mean() - 2.5).abs() < 1e-12);
        assert!((stats.std() - 1.118033988749895).abs() < 1e-12);
        assert!((stats.rms() - 7.5_f64.sqrt()).abs() < 1e-12);
        assert_eq!(stats.min, 1.0);
        assert_eq!(stats.max_abs(), 4.0);
    }
}
