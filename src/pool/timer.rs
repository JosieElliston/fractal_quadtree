use std::{ops, time::Duration};

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Timer {
    elapsed: Duration,
    count: u64,
}
impl Timer {
    pub(super) fn insert(&mut self, elapsed: Duration) {
        self.elapsed += elapsed;
        self.count += 1;
    }

    pub(crate) fn elapsed(&self) -> Duration {
        self.elapsed
    }

    pub(crate) fn count(&self) -> u64 {
        self.count
    }

    pub(crate) fn div_elapsed(&self, elapsed: Duration) -> f64 {
        self.elapsed.div_duration_f64(elapsed)
    }

    pub(crate) fn div_count(&self, count: u64) -> Option<Duration> {
        (self.elapsed.as_nanos() as u64)
            .checked_div(count)
            .map(Duration::from_nanos)
    }

    pub(crate) fn us_per_iter(&self) -> f64 {
        match self.div_count(self.count) {
            Some(elapsed) => elapsed.as_nanos() as f64 / 1000.0,
            None => f64::NAN,
        }
    }
}
impl ops::AddAssign for Timer {
    fn add_assign(&mut self, rhs: Self) {
        self.elapsed += rhs.elapsed;
        self.count += rhs.count;
    }
}
impl ops::Add for Timer {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            elapsed: self.elapsed + rhs.elapsed,
            count: self.count + rhs.count,
        }
    }
}

/// this exists for debugging / UX,
/// and is not needed for the main algorithm.
/// it needs to be `Copy` for [`egui::util::History`].
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct MultiTimer {
    pub(crate) render_ok: Timer,
    pub(crate) render_err: Timer,
    pub(crate) sample_ok: Timer,
    pub(crate) sample_err: Timer,
    pub(crate) insert_ok: Timer,
    pub(crate) insert_err: Timer,
    pub(crate) free_ok: Timer,
    pub(crate) free_err: Timer,
    pub(crate) retire_ok: Timer,
    pub(crate) retire_err: Timer,
    pub(crate) refine_ok: Timer,
    pub(crate) refine_err: Timer,
    pub(crate) idle: Timer,
}

const _: () =
    assert!(std::mem::size_of::<MultiTimer>() == MultiTimer::N * std::mem::size_of::<Timer>());

impl MultiTimer {
    const N: usize = 13;

    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    fn to_array(self) -> [Timer; MultiTimer::N] {
        [
            self.render_ok,
            self.render_err,
            self.sample_ok,
            self.sample_err,
            self.insert_ok,
            self.insert_err,
            self.free_ok,
            self.free_err,
            self.retire_ok,
            self.retire_err,
            self.refine_ok,
            self.refine_err,
            self.idle,
        ]
    }

    fn from_array(arr: [Timer; MultiTimer::N]) -> Self {
        Self {
            render_ok: arr[0],
            render_err: arr[1],
            sample_ok: arr[2],
            sample_err: arr[3],
            insert_ok: arr[4],
            insert_err: arr[5],
            free_ok: arr[6],
            free_err: arr[7],
            retire_ok: arr[8],
            retire_err: arr[9],
            refine_ok: arr[10],
            refine_err: arr[11],
            idle: arr[12],
        }
    }

    pub(crate) fn total(&self) -> Timer {
        self.to_array().into_iter().reduce(|a, b| a + b).unwrap()
    }
}
impl ops::AddAssign for MultiTimer {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}
impl ops::Add for MultiTimer {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        let lhs = self.to_array();
        let rhs = rhs.to_array();
        Self::from_array(std::array::from_fn(|i| lhs[i] + rhs[i]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timer_to_tuple(timer: Timer) -> (Duration, u64) {
        (timer.elapsed(), timer.count())
    }

    #[test]
    fn test_multitimer() {
        let arr = std::array::from_fn(|i| {
            let mut timer = Timer::default();
            timer.insert(Duration::from_millis(100 * (i as u64 + 1)));
            timer
        });

        assert_eq!(
            arr.map(timer_to_tuple),
            MultiTimer::from_array(arr).to_array().map(timer_to_tuple)
        );
    }
}
