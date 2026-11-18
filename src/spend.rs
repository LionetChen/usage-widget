//! What this computer's CLI use in the current weekly window would have cost at
//! API prices. Kept apart from the percent estimators, with its own place in the
//! logs, so it works whether or not they are switched on.

use serde::{Deserialize, Serialize, de::DeserializeOwned};

const WEEK_SECS: i64 = 7 * 86_400;
/// Reset times wobble by a second between readings; windows are days apart.
const SAME_WINDOW_SECS: i64 = 3600;

/// The running total for one weekly window, with `scan`: where the provider's
/// log reader stopped.
#[derive(Default, Serialize, Deserialize)]
#[serde(default, bound(deserialize = "S: Default + DeserializeOwned"))]
pub struct WeekSpend<S> {
    /// The window being totalled, by its reset time.
    window: i64,
    dollars: f64,
    scan: S,
}

impl<S: Default + Serialize + DeserializeOwned> WeekSpend<S> {
    /// Dollars used so far in the weekly window that resets at `window`. `scan`
    /// returns the `(unix time, dollars)` of each response not yet read.
    pub fn update(
        file: &str,
        window: i64,
        scan: impl FnOnce(&mut S) -> Vec<(i64, f64)>,
    ) -> Option<f64> {
        let path = crate::paths::cache_dir()?.join("usage-widget").join(file);
        let mut state: Self = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let dollars = state.add(window, scan);
        if let Ok(bytes) = serde_json::to_vec(&state) {
            let _ = crate::paths::write_atomic(&path, &bytes);
        }
        Some(dollars)
    }

    fn add(&mut self, window: i64, scan: impl FnOnce(&mut S) -> Vec<(i64, f64)>) -> f64 {
        if window < self.window - SAME_WINDOW_SECS {
            return self.dollars; // a late reading from an earlier window
        }
        if window - self.window >= SAME_WINDOW_SECS {
            // A new week: forget where the reader stopped, so the logs are read
            // again from the start and use already in this window is counted.
            *self = Self {
                window,
                ..Self::default()
            };
        }
        self.dollars += scan(&mut self.scan)
            .into_iter()
            .filter(|(ts, _)| (window - WEEK_SECS..window).contains(ts))
            .map(|(_, dollars)| dollars)
            .sum::<f64>();
        self.dollars
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WEEK: i64 = 1_789_462_965;

    #[test]
    fn totals_one_window_and_starts_over_in_the_next() {
        let mut spend = WeekSpend::<u32>::default();
        // The first reading of a window rereads the logs, older use included.
        let total = spend.add(WEEK, |scans| {
            *scans += 1;
            vec![(WEEK - WEEK_SECS - 1, 50.0), (WEEK - 3600, 1.25)]
        });
        assert_eq!(total, 1.25);
        // The reset time wobbles; that is still this window.
        assert_eq!(spend.add(WEEK + 1, |_| vec![(WEEK - 60, 0.5)]), 1.75);
        // A stale reading from last week changes nothing and reads nothing.
        assert_eq!(spend.add(WEEK - WEEK_SECS, |_| unreachable!()), 1.75);
        assert_eq!(spend.scan, 1);

        let next = WEEK + WEEK_SECS;
        let total = spend.add(next, |scans| {
            assert_eq!(*scans, 0, "the reader starts over");
            vec![(WEEK - 60, 0.5), (WEEK + 60, 2.0)]
        });
        assert_eq!(total, 2.0);
    }
}
