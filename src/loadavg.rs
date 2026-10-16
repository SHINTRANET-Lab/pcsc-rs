//! Load average for Windows, which has none: the cores' busy time (the number of cores'
//! worth of work running) averaged over 1, 5 and 15 minutes the way Unix does, as an
//! exponentially weighted moving average. It is an approximation: it does not see tasks
//! waiting for a core or for a disk, so it stays near the core count at most.

use std::time::Instant;

const PERIODS: [f64; 3] = [60.0, 300.0, 900.0];

#[derive(Debug, Default)]
pub struct Loadavg {
    averages: [f64; 3],
    last: Option<Instant>,
}

impl Loadavg {
    /// Adds a sample: `running` is how many cores' worth of work is running now
    /// (CPU usage as a fraction × the number of logical cores).
    pub fn update(&mut self, running: f64, now: Instant) {
        let running = running.max(0.0);
        match self.last.replace(now) {
            // The first sample is where the averages start from
            None => self.averages = [running; 3],
            Some(before) => {
                let seconds = now.duration_since(before).as_secs_f64();
                for (average, period) in self.averages.iter_mut().zip(PERIODS) {
                    let keep = (-seconds / period).exp();
                    *average = *average * keep + running * (1.0 - keep);
                }
            }
        }
    }

    pub fn get(&self) -> [f64; 3] {
        self.averages
    }
}

#[cfg(test)]
mod tests {
    use super::Loadavg;
    use std::time::{Duration, Instant};

    #[test]
    fn follows_the_load() {
        let start = Instant::now();
        let mut load = Loadavg::default();
        load.update(0.0, start);
        assert_eq!(load.get(), [0.0; 3]);
        // A steady 2.0 for 10 minutes: the 1-minute average is there, the 15-minute one is not
        for second in 1..=600 {
            load.update(2.0, start + Duration::from_secs(second));
        }
        let [one, five, fifteen] = load.get();
        assert!((one - 2.0).abs() < 0.01, "{one}");
        assert!(five > 1.0 && five < 2.0, "{five}");
        assert!(fifteen > 0.5 && fifteen < five, "{fifteen}");
    }
}
