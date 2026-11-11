//! FSRS-5 memory mathematics and QUSOR's learning and review scheduling policy.
//!
//! Everything in this module is a pure function of its arguments: no clock is
//! read, no state is shared, and identical inputs produce bit-identical results.
//! Time crosses the boundary as Unix milliseconds supplied by the caller.

use std::fmt;

/// Lowest stability the scheduler will ever emit or consume, in days.
const S_MIN: f64 = 0.0001;
/// Highest stability the scheduler will ever emit, in days.
const S_MAX: f64 = 36_500.0;
/// Lowest difficulty on the FSRS scale.
const D_MIN: f64 = 1.0;
/// Highest difficulty on the FSRS scale.
const D_MAX: f64 = 10.0;
/// FSRS-5 decay term `w20`; the extended 19-parameter model keeps `w19 = 0`.
const DECAY: f64 = 0.5;
/// Milliseconds in one day.
const MS_PER_DAY: i64 = 86_400_000;
/// Milliseconds in one minute.
const MS_PER_MINUTE: i64 = 60_000;
/// Shortest review interval the policy may schedule, in days.
const MIN_INTERVAL_DAYS: f64 = 1.0;
/// Longest review interval the policy may schedule, in days.
const MAX_INTERVAL_DAYS: f64 = 36_500.0;
/// Exclusive lower bound of an accepted `desired_retention`.
const RETENTION_MIN: f64 = 0.0001;
/// Exclusive upper bound of an accepted `desired_retention`.
const RETENTION_MAX: f64 = 0.9999;

/// FSRS-5 default weights `w0..=w18`; `w19` is fixed at `0` and `w20` at `0.5`.
pub const DEFAULT_PARAMS: [f64; 19] = [
    0.40255, 1.18385, 3.173, 15.69105, 7.1949, 0.5345, 1.4604, 0.0046, 1.54575, 0.1192, 1.01925,
    1.9395, 0.11, 0.29605, 2.2698, 0.2315, 2.9898, 0.51655, 0.6621,
];

/// The learner's self-reported response quality for one review.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Rating {
    /// Complete failure to recall.
    Again,
    /// Recalled with serious difficulty.
    Hard,
    /// Recalled with some hesitation.
    Good,
    /// Effortless recall.
    Easy,
}

impl Rating {
    /// Numeric model grade in `1..=4`, as used by the FSRS formulas.
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            Self::Again => 1,
            Self::Hard => 2,
            Self::Good => 3,
            Self::Easy => 4,
        }
    }

    /// Inverse of [`Rating::code`]; `None` outside `1..=4`.
    #[must_use]
    pub fn from_code(code: u8) -> Option<Self> {
        match code {
            1 => Some(Self::Again),
            2 => Some(Self::Hard),
            3 => Some(Self::Good),
            4 => Some(Self::Easy),
            _ => None,
        }
    }
}

/// The scheduling phase a card is currently in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Stage {
    /// Same-day learning steps; no multi-day interval yet.
    Learning,
    /// Graduated card scheduled across whole days.
    Review,
}

/// The long-term memory estimates that FSRS-5 carries between reviews.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MemoryState {
    /// Stability in days: the interval at which recall probability reaches 90%.
    pub stability: f64,
    /// Difficulty on the `1.0..=10.0` scale; higher means harder to recall.
    pub difficulty: f64,
}

/// The card state presented to [`schedule`] before the rating is applied.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScheduleInput {
    /// `None` for a new card. When present, `last_review_at_ms` must also be
    /// present, and vice versa.
    pub memory: Option<MemoryState>,
    /// Current phase. Ignored for new cards.
    pub stage: Stage,
    /// Previous review timestamp in Unix milliseconds; must be positive and not
    /// after the supplied `now_ms`.
    pub last_review_at_ms: Option<i64>,
}

/// A card state after applying one rating.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scheduled {
    /// Updated long-term memory estimates.
    pub memory: MemoryState,
    /// Phase the card moves to.
    pub stage: Stage,
    /// Unix milliseconds at which the card becomes due.
    pub due_at_ms: i64,
    /// Whole minutes from `now_ms` to `due_at_ms`.
    pub interval_minutes: u64,
}

/// Scheduling configuration: FSRS-5 weights plus QUSOR's step and retention policy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Policy {
    /// FSRS-5 weights `w0..=w18`; every entry must be finite.
    pub params: [f64; 19],
    /// Target recall probability for review cards, inside `(0.0001, 0.9999)`.
    pub desired_retention: f64,
    /// Same-day learning step for [`Rating::Again`] in minutes; must be non-zero.
    pub again_step_minutes: u32,
    /// Same-day learning step for [`Rating::Hard`] in minutes; must be non-zero.
    pub hard_step_minutes: u32,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            params: DEFAULT_PARAMS,
            desired_retention: 0.9,
            again_step_minutes: 10,
            hard_step_minutes: 30,
        }
    }
}

/// The only failure mode of [`schedule`]: the inputs violate the contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchedulerError {
    /// A timestamp, memory value, or policy value is outside its accepted range.
    InvalidInput,
}

impl fmt::Display for SchedulerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput => formatter.write_str("invalid scheduler input"),
        }
    }
}

impl std::error::Error for SchedulerError {}

/// Applies `rating` to `input` at `now_ms` and returns the next card state.
///
/// The result is deterministic: repeating the call with the same arguments
/// yields bit-identical values.
///
/// # Errors
///
/// Returns [`SchedulerError::InvalidInput`] when a timestamp, the memory state,
/// or the policy violates the documented ranges, or when the computed due
/// timestamp cannot be represented in `i64` milliseconds. The function never
/// panics.
pub fn schedule(
    input: ScheduleInput,
    rating: Rating,
    now_ms: i64,
    policy: &Policy,
) -> Result<Scheduled, SchedulerError> {
    validate(&input, now_ms, policy)?;

    let Some(memory) = input.memory else {
        return schedule_new_card(rating, now_ms, policy);
    };
    let last_review_at_ms = input
        .last_review_at_ms
        .ok_or(SchedulerError::InvalidInput)?;
    let elapsed_days = (now_ms - last_review_at_ms) as f64 / 86_400_000.0;
    let recall = retrievability(elapsed_days, memory.stability);
    let stability = if elapsed_days.round() < 1.0 {
        short_term_stability(memory.stability, rating, &policy.params)
    } else if rating == Rating::Again {
        forget_stability(memory.stability, memory.difficulty, recall, &policy.params)
    } else {
        recall_stability(
            memory.stability,
            memory.difficulty,
            recall,
            rating,
            &policy.params,
        )
    };
    let next = MemoryState {
        stability: clamp_or_fallback(stability, memory.stability, S_MIN, S_MAX),
        difficulty: clamp_or_fallback(
            next_difficulty(memory.difficulty, rating, &policy.params),
            memory.difficulty,
            D_MIN,
            D_MAX,
        ),
    };

    match (input.stage, rating) {
        (Stage::Learning, Rating::Again) => learning_step(now_ms, policy.again_step_minutes, next),
        (Stage::Learning, Rating::Hard) => learning_step(now_ms, policy.hard_step_minutes, next),
        _ => review_interval(now_ms, policy.desired_retention, next),
    }
}

fn validate(input: &ScheduleInput, now_ms: i64, policy: &Policy) -> Result<(), SchedulerError> {
    if now_ms <= 0 {
        return Err(SchedulerError::InvalidInput);
    }
    match (input.memory, input.last_review_at_ms) {
        (None, Some(_)) | (Some(_), None) => return Err(SchedulerError::InvalidInput),
        (Some(memory), Some(last_review_at_ms)) => {
            if last_review_at_ms <= 0 || last_review_at_ms > now_ms {
                return Err(SchedulerError::InvalidInput);
            }
            if !memory.stability.is_finite() || memory.stability <= 0.0 {
                return Err(SchedulerError::InvalidInput);
            }
            if !memory.difficulty.is_finite() || !(D_MIN..=D_MAX).contains(&memory.difficulty) {
                return Err(SchedulerError::InvalidInput);
            }
        }
        (None, None) => {}
    }
    let retention = policy.desired_retention;
    if !retention.is_finite() || retention <= RETENTION_MIN || retention >= RETENTION_MAX {
        return Err(SchedulerError::InvalidInput);
    }
    if policy.again_step_minutes == 0 || policy.hard_step_minutes == 0 {
        return Err(SchedulerError::InvalidInput);
    }
    if !policy.params.iter().all(|weight| weight.is_finite()) {
        return Err(SchedulerError::InvalidInput);
    }
    Ok(())
}

fn schedule_new_card(
    rating: Rating,
    now_ms: i64,
    policy: &Policy,
) -> Result<Scheduled, SchedulerError> {
    let memory = MemoryState {
        stability: clamp_or_fallback(init_stability(rating, &policy.params), S_MIN, S_MIN, S_MAX),
        difficulty: init_difficulty(rating, &policy.params),
    };
    match rating {
        Rating::Again => learning_step(now_ms, policy.again_step_minutes, memory),
        Rating::Hard => learning_step(now_ms, policy.hard_step_minutes, memory),
        Rating::Good | Rating::Easy => review_interval(now_ms, policy.desired_retention, memory),
    }
}

fn learning_step(
    now_ms: i64,
    step_minutes: u32,
    memory: MemoryState,
) -> Result<Scheduled, SchedulerError> {
    let delta_ms = i128::from(step_minutes) * i128::from(MS_PER_MINUTE);
    build_schedule(now_ms, delta_ms, Stage::Learning, memory)
}

fn review_interval(
    now_ms: i64,
    desired_retention: f64,
    memory: MemoryState,
) -> Result<Scheduled, SchedulerError> {
    let days = interval_days(desired_retention, memory.stability);
    let delta_ms = i128::from(days) * i128::from(MS_PER_DAY);
    build_schedule(now_ms, delta_ms, Stage::Review, memory)
}

fn build_schedule(
    now_ms: i64,
    delta_ms: i128,
    stage: Stage,
    memory: MemoryState,
) -> Result<Scheduled, SchedulerError> {
    let due_at_ms =
        i64::try_from(i128::from(now_ms) + delta_ms).map_err(|_| SchedulerError::InvalidInput)?;
    let interval_minutes = u64::try_from(delta_ms / i128::from(MS_PER_MINUTE))
        .map_err(|_| SchedulerError::InvalidInput)?;
    Ok(Scheduled {
        memory,
        stage,
        due_at_ms,
        interval_minutes,
    })
}

/// Whole-day interval for a target retention, clamped to the policy bounds.
fn interval_days(desired_retention: f64, stability: f64) -> u64 {
    let raw = interval(desired_retention, stability);
    let days = raw.round().clamp(MIN_INTERVAL_DAYS, MAX_INTERVAL_DAYS);
    days as u64
}

/// FSRS-5 forgetting curve `R(t, s)`, with elapsed days rounded like the model.
fn retrievability(elapsed_days: f64, stability: f64) -> f64 {
    let elapsed = elapsed_days.max(0.0).round();
    let stability = stability.max(S_MIN);
    (1.0 + (elapsed / stability) * decay_factor()).powf(-DECAY)
}

/// FSRS-5 inverse forgetting curve `I(r, s)`.
fn interval(desired_retention: f64, stability: f64) -> f64 {
    let stability = stability.max(S_MIN);
    let growth = desired_retention.powf(1.0 / -DECAY) - 1.0;
    (stability / decay_factor() * growth).clamp(0.0, S_MAX)
}

/// `factor = 0.9^(1 / -w20) - 1`, i.e. `19 / 81` for the extended model.
fn decay_factor() -> f64 {
    0.9_f64.powf(1.0 / -DECAY) - 1.0
}

/// Initial stability `init_s(G) = w[G-1]`, kept inside the model bounds.
fn init_stability(rating: Rating, params: &[f64; 19]) -> f64 {
    params[usize::from(rating.code()) - 1].clamp(S_MIN, S_MAX)
}

/// Initial difficulty `init_d(G) = w4 - e^(w5 * (G - 1)) + 1`, clamped.
fn init_difficulty(rating: Rating, params: &[f64; 19]) -> f64 {
    let grade = f64::from(rating.code());
    (params[4] - (params[5] * (grade - 1.0)).exp() + 1.0).clamp(D_MIN, D_MAX)
}

/// FSRS-5 difficulty update; every rating updates difficulty.
fn next_difficulty(difficulty: f64, rating: Rating, params: &[f64; 19]) -> f64 {
    let grade = f64::from(rating.code());
    let delta = -params[6] * (grade - 3.0);
    let damped = difficulty + (10.0 - difficulty) / 9.0 * delta;
    let blended = params[7] * init_difficulty(Rating::Easy, params) + (1.0 - params[7]) * damped;
    clamp_or_fallback(blended, difficulty, D_MIN, D_MAX)
}

/// Success stability `s'` for `G >= 2` on a day-separated review.
fn recall_stability(
    stability: f64,
    difficulty: f64,
    recall: f64,
    rating: Rating,
    params: &[f64; 19],
) -> f64 {
    let hard_penalty = if rating == Rating::Hard {
        params[15]
    } else {
        1.0
    };
    let easy_bonus = if rating == Rating::Easy {
        params[16]
    } else {
        1.0
    };
    let gain = params[8].exp()
        * (11.0 - difficulty)
        * stability.powf(-params[9])
        * ((params[10] * (1.0 - recall)).exp() - 1.0)
        * hard_penalty
        * easy_bonus;
    stability * (gain + 1.0)
}

/// Forgetting stability for `G = 1` on a day-separated review.
fn forget_stability(stability: f64, difficulty: f64, recall: f64, params: &[f64; 19]) -> f64 {
    let ceiling = stability / (params[17] * params[18]).exp();
    let forgotten = params[11]
        * difficulty.powf(-params[12])
        * ((stability + 1.0).powf(params[13]) - 1.0)
        * (params[14] * (1.0 - recall)).exp();
    forgotten.min(ceiling)
}

/// Short-term stability used when two reviews land on the same rounded day.
fn short_term_stability(stability: f64, rating: Rating, params: &[f64; 19]) -> f64 {
    let grade = f64::from(rating.code());
    let increment = (params[17] * (grade - 3.0 + params[18])).exp();
    let growth = if rating.code() >= 2 {
        increment.max(1.0)
    } else {
        increment
    };
    stability * growth
}

/// Clamp into `[min, max]`, replacing a `NaN` produced by unusual weights with `fallback`.
fn clamp_or_fallback(value: f64, fallback: f64, min: f64, max: f64) -> f64 {
    if value.is_nan() {
        fallback.clamp(min, max)
    } else {
        value.clamp(min, max)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        D_MAX, D_MIN, DEFAULT_PARAMS, Rating, S_MAX, S_MIN, decay_factor, init_difficulty,
        interval, interval_days, next_difficulty, retrievability, short_term_stability,
    };

    fn close(actual: f64, expected: f64, tolerance: f64) -> bool {
        (actual - expected).abs() <= tolerance
    }

    #[test]
    fn decay_factor_matches_closed_form() {
        assert!(close(decay_factor(), 19.0 / 81.0, 1e-15));
    }

    #[test]
    fn retrievability_matches_reference_curve() {
        assert!(close(retrievability(0.0, 3.0), 1.0, 1e-12));
        assert!(close(retrievability(3.0, 3.0), 0.9, 1e-9));
        assert!(close(retrievability(-4.0, 3.0), 1.0, 1e-12));
        // Elapsed days are rounded: sub-day gaps count as day zero.
        assert!(close(retrievability(0.4, 3.0), 1.0, 1e-12));
        assert!(close(
            retrievability(2.6, 3.0),
            retrievability(3.0, 3.0),
            1e-12
        ));
        assert!(retrievability(10.0, 3.0) < retrievability(1.0, 3.0));
    }

    #[test]
    fn interval_inverts_retrievability() {
        for stability in [S_MIN, 0.5, 1.0, 3.173, 15.69105, S_MAX] {
            let expected = stability;
            assert!(
                close(interval(0.9, stability), expected, 1e-9),
                "stability {stability} did not round-trip"
            );
        }
        assert_eq!(interval(0.9, 1.0e12), S_MAX);
    }

    #[test]
    fn interval_days_bounds_are_enforced() {
        assert_eq!(interval_days(0.9, S_MIN), 1);
        assert_eq!(interval_days(0.9, 1.0e12), 36_500);
        assert_eq!(interval_days(0.9, DEFAULT_PARAMS[2]), 3);
        assert_eq!(interval_days(0.9, DEFAULT_PARAMS[3]), 16);
    }

    #[test]
    fn short_term_growth_matches_formula() {
        let increment = (DEFAULT_PARAMS[17] * DEFAULT_PARAMS[18]).exp();
        assert!(close(
            short_term_stability(1.0, Rating::Good, &DEFAULT_PARAMS),
            increment,
            1e-12
        ));
        let again_increment = (DEFAULT_PARAMS[17] * (1.0 - 3.0 + DEFAULT_PARAMS[18])).exp();
        assert!(close(
            short_term_stability(1.0, Rating::Again, &DEFAULT_PARAMS),
            again_increment,
            1e-12
        ));
        let hard_increment = (DEFAULT_PARAMS[17] * (2.0 - 3.0 + DEFAULT_PARAMS[18])).exp();
        assert!(close(
            short_term_stability(1.0, Rating::Hard, &DEFAULT_PARAMS),
            hard_increment.max(1.0),
            1e-12
        ));
    }

    #[test]
    fn difficulty_update_stays_inside_bounds() {
        for difficulty in [D_MIN, 5.0, D_MAX] {
            for rating in [Rating::Again, Rating::Hard, Rating::Good, Rating::Easy] {
                let updated = next_difficulty(difficulty, rating, &DEFAULT_PARAMS);
                assert!((D_MIN..=D_MAX).contains(&updated));
            }
        }
        let reference = init_difficulty(Rating::Good, &DEFAULT_PARAMS);
        assert!(
            next_difficulty(reference, Rating::Good, &DEFAULT_PARAMS)
                < next_difficulty(reference, Rating::Again, &DEFAULT_PARAMS)
        );
    }
}
