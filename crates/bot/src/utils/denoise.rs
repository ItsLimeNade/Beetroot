//! Smoothing of CGM traces for display.
//!
//! This is a Whittaker-Eilers smoother (penalized least squares): the
//! smoothed trace `z` minimizes
//!
//! ```text
//!   Σ wᵢ (yᵢ − zᵢ)²  +  λ ∫ z''(t)² dt
//! ```
//!
//! that is, it stays close to the readings `y` while being penalized for
//! bending. It was picked over the usual alternatives because of what a graph
//! of past glucose needs:
//!
//! - **No lag.** A graph is drawn after the fact, so the smoother can look
//!   both ways. Moving averages over past readings and exponential or Kalman
//!   *filters* shift every rise, fall and peak later in time.
//! - **Honest at the edges.** The newest reading is the one people look at.
//!   Centered moving averages and Savitzky-Golay need a full window either
//!   side and have to shrink, pad or extrapolate at the ends; the penalty
//!   formulation has no window and treats the last reading like any other.
//! - **Uneven sampling.** Sensors report every 1 or 5 minutes and drop
//!   readings. Window-based filters assume a fixed step. Here the bending is
//!   measured in real time between readings (divided differences), so missing
//!   or irregular readings need no resampling.
//! - **Shape is kept.** Penalizing the second derivative leaves any straight
//!   trend untouched and takes less off real peaks and troughs than averaging
//!   over the same span does.
//! - **One meaningful knob.** The strength is a cutoff period (see
//!   [`Strength`]), not a window size that means something different for each
//!   sensor.
//!
//! It is also exact and cheap: one banded linear system per trace, solved in
//! linear time.

use bonbon::prelude::GraphEntry;

/// How hard a trace is smoothed.
///
/// Each strength is a cutoff period: a wiggle repeating at that period keeps
/// half its height, faster ones less, slower ones more. Sensor jitter lives
/// in the 5 to 20 minute range while glucose itself rarely swings faster than
/// about an hour, so the cutoffs sit in between.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strength {
    /// Takes the jitter off and little else: a 20 minute wiggle keeps about
    /// a quarter, a 2 hour swing over 99%.
    Light,
    /// A 20 minute wiggle keeps about 5%, a 2 hour swing about 98%.
    Medium,
    /// A clean, rounded trace, at the cost of some height on sharp peaks: a
    /// 20 minute wiggle keeps about 2%, a 2 hour swing about 94%.
    Strong,
}

impl Strength {
    /// Reads the level stored for a user: 0 is off, then 1 to 3.
    pub fn from_level(level: i64) -> Option<Self> {
        match level {
            1 => Some(Self::Light),
            2 => Some(Self::Medium),
            3 => Some(Self::Strong),
            _ => None,
        }
    }

    pub fn level(self) -> i64 {
        match self {
            Self::Light => 1,
            Self::Medium => 2,
            Self::Strong => 3,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Light => "Light",
            Self::Medium => "Medium",
            Self::Strong => "Strong",
        }
    }

    fn cutoff_period_minutes(self) -> f64 {
        match self {
            Self::Light => 30.0,
            Self::Medium => 45.0,
            Self::Strong => 60.0,
        }
    }
}

/// The sampling step the smoothing strength is defined against. Readings are
/// weighted by how much time each stands for, so a 1 minute sensor is
/// smoothed like a 5 minute one rather than five times less.
const REFERENCE_STEP_MINUTES: f64 = 5.0;

/// Readings further apart than this are not smoothed across: each stretch of
/// continuous data is handled on its own, so a gap is never bridged.
const MAX_GAP_MINUTES: f64 = 15.0;

/// Smooths sensor noise out of a glucose trace.
///
/// Readings come back oldest first, with their timestamps unchanged and one
/// reading kept per timestamp.
pub fn denoise(mut entries: Vec<GraphEntry>, strength: Strength) -> Vec<GraphEntry> {
    entries.sort_by_key(|e| e.date);
    entries.dedup_by_key(|e| e.date);

    // Minutes since the first reading.
    let Some(origin) = entries.first().map(|e| e.date) else {
        return entries;
    };
    let times: Vec<f64> = entries
        .iter()
        .map(|e| (e.date - origin).num_milliseconds() as f64 / 60_000.0)
        .collect();

    let mut start = 0;
    for end in 1..=entries.len() {
        if end == entries.len() || times[end] - times[end - 1] > MAX_GAP_MINUTES {
            let values: Vec<f64> = entries[start..end].iter().map(|e| e.sgv as f64).collect();
            let smoothed = smooth(&times[start..end], &values, strength);
            for (entry, value) in entries[start..end].iter_mut().zip(smoothed) {
                entry.sgv = value as f32;
            }
            start = end;
        }
    }

    entries
}

/// Whittaker-Eilers smoothing of one continuous stretch. `t` is in minutes
/// and strictly increasing.
fn smooth(t: &[f64], y: &[f64], strength: Strength) -> Vec<f64> {
    let n = y.len();
    // Three readings are needed to measure any bending at all.
    if n < 3 {
        return y.to_vec();
    }

    // With the fit weighted per reference step, a wiggle of angular frequency
    // ω is scaled by 1 / (1 + step · λ · ω⁴). Half at the cutoff gives λ.
    let cutoff = std::f64::consts::TAU / strength.cutoff_period_minutes();
    let lambda = 1.0 / (REFERENCE_STEP_MINUTES * cutoff.powi(4));

    // The system (W + λ DᵀVD) z = W y, with D the second divided differences
    // and V the time each one spans. Symmetric with two bands above the
    // diagonal: `a0` is the diagonal, `a1` and `a2` the bands.
    let mut a0 = vec![0.0; n];
    let mut a1 = vec![0.0; n];
    let mut a2 = vec![0.0; n];
    let mut rhs = vec![0.0; n];

    for i in 0..n {
        // The time this reading stands for: half the gap to each neighbour.
        let before = if i > 0 { t[i] - t[i - 1] } else { t[1] - t[0] };
        let after = if i + 1 < n {
            t[i + 1] - t[i]
        } else {
            t[n - 1] - t[n - 2]
        };
        let weight = (before + after) / 2.0 / REFERENCE_STEP_MINUTES;
        a0[i] = weight;
        rhs[i] = weight * y[i];
    }

    for k in 1..n - 1 {
        let (left, right) = (t[k] - t[k - 1], t[k + 1] - t[k]);
        let span = left + right;
        // z'' at reading k from its two neighbours.
        let c = [
            2.0 / (left * span),
            -2.0 / (left * right),
            2.0 / (right * span),
        ];
        let scale = lambda * span / 2.0;
        let i = k - 1;
        a0[i] += scale * c[0] * c[0];
        a0[i + 1] += scale * c[1] * c[1];
        a0[i + 2] += scale * c[2] * c[2];
        a1[i] += scale * c[0] * c[1];
        a1[i + 1] += scale * c[1] * c[2];
        a2[i] += scale * c[0] * c[2];
    }

    solve_banded(&a0, &a1, &a2, rhs)
}

/// Solves a symmetric positive definite system with two bands either side of
/// the diagonal, by LDLᵀ factorization.
fn solve_banded(a0: &[f64], a1: &[f64], a2: &[f64], mut b: Vec<f64>) -> Vec<f64> {
    let n = a0.len();
    let mut d = vec![0.0; n];
    // l1[i] = L[i+1][i], l2[i] = L[i+2][i]
    let mut l1 = vec![0.0; n];
    let mut l2 = vec![0.0; n];

    for i in 0..n {
        let mut diagonal = a0[i];
        let mut below = a1[i];
        if i >= 1 {
            diagonal -= l1[i - 1] * l1[i - 1] * d[i - 1];
            below -= l1[i - 1] * l2[i - 1] * d[i - 1];
        }
        if i >= 2 {
            diagonal -= l2[i - 2] * l2[i - 2] * d[i - 2];
        }
        d[i] = diagonal;
        l1[i] = below / diagonal;
        l2[i] = a2[i] / diagonal;
    }

    // L u = b
    for i in 0..n {
        if i >= 1 {
            b[i] -= l1[i - 1] * b[i - 1];
        }
        if i >= 2 {
            b[i] -= l2[i - 2] * b[i - 2];
        }
    }
    // D v = u, then Lᵀ z = v
    for i in (0..n).rev() {
        b[i] /= d[i];
        if i + 1 < n {
            b[i] -= l1[i] * b[i + 1];
        }
        if i + 2 < n {
            b[i] -= l2[i] * b[i + 2];
        }
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Duration, Utc};
    use std::f64::consts::TAU;

    fn at(minutes: f64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_800_000_000, 0).unwrap()
            + Duration::milliseconds((minutes * 60_000.0) as i64)
    }

    /// A trace sampled every `step` minutes for `minutes`, following `f`.
    fn trace(minutes: f64, step: f64, f: impl Fn(f64) -> f64) -> Vec<GraphEntry> {
        (0..=(minutes / step) as usize)
            .map(|i| {
                let t = i as f64 * step;
                GraphEntry {
                    sgv: f(t) as f32,
                    date: at(t),
                }
            })
            .collect()
    }

    fn medium(entries: Vec<GraphEntry>) -> Vec<GraphEntry> {
        denoise(entries, Strength::Medium)
    }

    /// Height of a wave after smoothing, measured away from the edges.
    fn amplitude(entries: &[GraphEntry], base: f32) -> f32 {
        let inner = &entries[entries.len() / 4..entries.len() * 3 / 4];
        inner
            .iter()
            .map(|e| (e.sgv - base).abs())
            .fold(0.0, f32::max)
    }

    /// Repeatable noise in [-1, 1].
    fn noise(i: usize) -> f64 {
        let x = (i as f64 * 12.9898).sin() * 43_758.545;
        (x - x.floor()) * 2.0 - 1.0
    }

    #[test]
    fn leaves_flat_and_straight_traces_alone() {
        let flat = medium(trace(180.0, 5.0, |_| 120.0));
        assert!(flat.iter().all(|e| (e.sgv - 120.0).abs() < 1e-3));

        // A steady climb has no bending to penalize, even at the edges.
        let climb = medium(trace(180.0, 5.0, |t| 80.0 + t));
        for (i, e) in climb.iter().enumerate() {
            assert!((e.sgv - (80.0 + i as f32 * 5.0)).abs() < 1e-2);
        }
    }

    #[test]
    fn removes_fast_wiggles_and_keeps_slow_swings() {
        let wave = |period: f64| move |t: f64| 120.0 + 30.0 * (TAU * t / period).sin();

        let jitter = amplitude(&medium(trace(600.0, 5.0, wave(20.0))), 120.0);
        assert!(jitter < 30.0 * 0.1, "20 min wiggle kept {jitter}");

        let cutoff = amplitude(&medium(trace(900.0, 5.0, wave(45.0))), 120.0);
        assert!((cutoff - 15.0).abs() < 2.0, "45 min wave kept {cutoff}");

        let swing = amplitude(&medium(trace(1440.0, 5.0, wave(120.0))), 120.0);
        assert!(swing > 30.0 * 0.95, "2 h swing kept {swing}");
    }

    #[test]
    fn each_strength_halves_its_own_cutoff() {
        for (strength, period) in [
            (Strength::Light, 30.0),
            (Strength::Medium, 45.0),
            (Strength::Strong, 60.0),
        ] {
            let wave = move |t: f64| 120.0 + 30.0 * (TAU * t / period).sin();
            let kept = amplitude(&denoise(trace(1200.0, 5.0, wave), strength), 120.0);
            assert!((kept - 15.0).abs() < 2.5, "{strength:?} kept {kept}");
            assert_eq!(Strength::from_level(strength.level()), Some(strength));
        }
        assert_eq!(Strength::from_level(0), None);
    }

    #[test]
    fn smooths_one_and_five_minute_sensors_alike() {
        let wave = |t: f64| 120.0 + 30.0 * (TAU * t / 45.0).sin();
        let five = amplitude(&medium(trace(900.0, 5.0, wave)), 120.0);
        let one = amplitude(&medium(trace(900.0, 1.0, wave)), 120.0);
        assert!((five - one).abs() < 1.5, "5 min: {five}, 1 min: {one}");
    }

    #[test]
    fn recovers_a_noisy_meal_curve() {
        // A rise and fall over three hours, with ±8 mg/dL of jitter.
        let clean = |t: f64| 110.0 + 80.0 * (-((t - 90.0) / 45.0).powi(2)).exp();
        let noisy: Vec<GraphEntry> = trace(180.0, 5.0, clean)
            .into_iter()
            .enumerate()
            .map(|(i, e)| GraphEntry {
                sgv: e.sgv + 8.0 * noise(i) as f32,
                ..e
            })
            .collect();

        let error = |entries: &[GraphEntry]| {
            let sum: f64 = entries
                .iter()
                .enumerate()
                .map(|(i, e)| (e.sgv as f64 - clean(i as f64 * 5.0)).powi(2))
                .sum();
            (sum / entries.len() as f64).sqrt()
        };

        let before = error(&noisy);
        let smoothed = medium(noisy);
        let after = error(&smoothed);
        assert!(after < before * 0.6, "error {before} -> {after}");

        // The peak is still there, at the same time and nearly the same height.
        let peak = smoothed
            .iter()
            .max_by(|a, b| a.sgv.total_cmp(&b.sgv))
            .unwrap();
        assert!((peak.sgv - 190.0).abs() < 8.0, "peak {}", peak.sgv);
        assert!((peak.date - at(90.0)).num_minutes().abs() <= 5);
    }

    #[test]
    fn never_smooths_across_a_gap() {
        // An hour around 100, a 40 minute gap, then an hour around 200.
        let mut entries = trace(60.0, 5.0, |_| 100.0);
        entries.extend(trace(60.0, 5.0, |_| 200.0).into_iter().map(|e| GraphEntry {
            date: e.date + Duration::minutes(100),
            ..e
        }));

        let smoothed = medium(entries);
        assert!(smoothed[..13].iter().all(|e| (e.sgv - 100.0).abs() < 1e-3));
        assert!(smoothed[13..].iter().all(|e| (e.sgv - 200.0).abs() < 1e-3));
    }

    #[test]
    fn copes_with_dropped_and_odd_readings() {
        // A straight climb with readings missing and off the 5 minute beat.
        let minutes = [0.0, 5.0, 10.2, 20.0, 24.7, 30.0, 41.0, 45.0, 50.0];
        let entries: Vec<GraphEntry> = minutes
            .iter()
            .map(|&t| GraphEntry {
                sgv: (100.0 + 2.0 * t) as f32,
                date: at(t),
            })
            .collect();

        let smoothed = medium(entries);
        for (e, &t) in smoothed.iter().zip(&minutes) {
            assert!((e.sgv as f64 - (100.0 + 2.0 * t)).abs() < 1e-2);
            assert_eq!(e.date, at(t));
        }
    }

    #[test]
    fn handles_tiny_unsorted_and_duplicate_input() {
        assert!(medium(Vec::new()).is_empty());

        let two = medium(trace(5.0, 5.0, |t| 100.0 + t));
        assert_eq!(
            two.iter().map(|e| e.sgv).collect::<Vec<_>>(),
            [100.0, 105.0]
        );

        // Newest first, with one timestamp reported twice.
        let mut entries = trace(60.0, 5.0, |_| 140.0);
        entries.push(entries[3].clone());
        entries.reverse();
        let smoothed = medium(entries);
        assert_eq!(smoothed.len(), 13);
        assert!(smoothed.windows(2).all(|w| w[0].date < w[1].date));
        assert!(smoothed.iter().all(|e| (e.sgv - 140.0).abs() < 1e-3));
    }
}
