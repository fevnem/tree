//! Smooth download speed, sampled by the engine once per second.

use std::time::Instant;

#[derive(Default, Clone)]
pub struct Speedometer {
    last_bytes: u64,
    ewma: f64,
    last_at: Option<Instant>,
}

impl Speedometer {
    /// Feed the cumulative byte count; returns an EWMA in bytes/second.
    pub fn sample(&mut self, bytes: u64) -> f64 {
        let now = Instant::now();
        let Some(t0) = self.last_at else {
            self.last_at = Some(now);
            self.last_bytes = bytes;
            return 0.0;
        };
        let dt = now.duration_since(t0).as_secs_f64();
        if dt < 0.2 {
            return self.ewma;
        }
        let instant = (bytes.saturating_sub(self.last_bytes)) as f64 / dt;
        self.ewma = if self.ewma <= 0.0 {
            instant
        } else {
            self.ewma * 0.65 + instant * 0.35
        };
        self.last_bytes = bytes;
        self.last_at = Some(now);
        self.ewma
    }
}

#[cfg(test)]
mod tests {
    use super::Speedometer;

    #[test]
    fn first_sample_is_zero_then_positive() {
        let mut s = Speedometer::default();
        assert_eq!(s.sample(0), 0.0);
        std::thread::sleep(std::time::Duration::from_millis(220));
        let v = s.sample(1024 * 1024);
        assert!(v > 0.0, "speed should be positive, got {v}");
    }
}
