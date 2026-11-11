use learning_domain::{
    DEFAULT_PARAMS, MemoryState, Policy, Rating, ScheduleInput, SchedulerError, Stage, schedule,
};

const DAY_MS: i64 = 86_400_000;
const MINUTE_MS: i64 = 60_000;
const NOW: i64 = 1_700_000_000_000;
const DECAY: f64 = 0.5;
const S_MIN: f64 = 0.0001;

fn close(actual: f64, expected: f64, tolerance: f64) -> bool {
    (actual - expected).abs() <= tolerance
}

fn new_card() -> ScheduleInput {
    ScheduleInput {
        memory: None,
        stage: Stage::Learning,
        last_review_at_ms: None,
    }
}

fn review_card(memory: MemoryState, last_review_at_ms: i64) -> ScheduleInput {
    ScheduleInput {
        memory: Some(memory),
        stage: Stage::Review,
        last_review_at_ms: Some(last_review_at_ms),
    }
}

fn learning_card(memory: MemoryState, last_review_at_ms: i64) -> ScheduleInput {
    ScheduleInput {
        memory: Some(memory),
        stage: Stage::Learning,
        last_review_at_ms: Some(last_review_at_ms),
    }
}

fn init_stability(rating: Rating) -> f64 {
    DEFAULT_PARAMS[usize::from(rating.code()) - 1]
}

fn init_difficulty(rating: Rating) -> f64 {
    let grade = f64::from(rating.code());
    (DEFAULT_PARAMS[4] - (DEFAULT_PARAMS[5] * (grade - 1.0)).exp() + 1.0).clamp(1.0, 10.0)
}

/// Independent re-implementation of `I(desired_retention, stability)` and the day clamp.
fn day_offset(days: u64) -> i64 {
    i64::try_from(days).expect("interval days always fit in i64")
}

fn expected_interval_days(desired_retention: f64, stability: f64) -> u64 {
    let factor = 0.9_f64.powf(1.0 / -DECAY) - 1.0;
    let growth = desired_retention.powf(1.0 / -DECAY) - 1.0;
    let raw = (stability.max(S_MIN) / factor * growth).clamp(0.0, 36_500.0);
    raw.round().clamp(1.0, 36_500.0) as u64
}

#[test]
fn new_card_uses_initial_memory_and_documented_steps() {
    let policy = Policy::default();
    assert_eq!(policy.params, DEFAULT_PARAMS);
    assert!(close(policy.desired_retention, 0.9, 1e-15));
    assert_eq!(policy.again_step_minutes, 10);
    assert_eq!(policy.hard_step_minutes, 30);

    let again = schedule(new_card(), Rating::Again, NOW, &policy).unwrap();
    assert_eq!(again.stage, Stage::Learning);
    assert_eq!(again.due_at_ms, NOW + 10 * MINUTE_MS);
    assert_eq!(again.interval_minutes, 10);
    assert!(close(
        again.memory.stability,
        init_stability(Rating::Again),
        1e-12
    ));
    assert!(close(
        again.memory.difficulty,
        init_difficulty(Rating::Again),
        1e-12
    ));

    let hard = schedule(new_card(), Rating::Hard, NOW, &policy).unwrap();
    assert_eq!(hard.stage, Stage::Learning);
    assert_eq!(hard.due_at_ms, NOW + 30 * MINUTE_MS);
    assert_eq!(hard.interval_minutes, 30);
    assert!(close(
        hard.memory.stability,
        init_stability(Rating::Hard),
        1e-12
    ));
    assert!(close(
        hard.memory.difficulty,
        init_difficulty(Rating::Hard),
        1e-12
    ));

    let good = schedule(new_card(), Rating::Good, NOW, &policy).unwrap();
    let good_days = expected_interval_days(0.9, DEFAULT_PARAMS[2]);
    assert_eq!(good_days, 3);
    assert_eq!(good.stage, Stage::Review);
    assert_eq!(good.due_at_ms, NOW + day_offset(good_days) * DAY_MS);
    assert_eq!(good.interval_minutes, good_days * 1_440);
    assert!(close(good.memory.stability, DEFAULT_PARAMS[2], 1e-12));
    assert!(close(
        good.memory.difficulty,
        init_difficulty(Rating::Good),
        1e-12
    ));

    let easy = schedule(new_card(), Rating::Easy, NOW, &policy).unwrap();
    let easy_days = expected_interval_days(0.9, DEFAULT_PARAMS[3]);
    assert_eq!(easy_days, 16);
    assert_eq!(easy.stage, Stage::Review);
    assert_eq!(easy.due_at_ms, NOW + day_offset(easy_days) * DAY_MS);
    assert_eq!(easy.interval_minutes, easy_days * 1_440);
    assert!(close(easy.memory.stability, DEFAULT_PARAMS[3], 1e-12));
    assert!(close(
        easy.memory.difficulty,
        init_difficulty(Rating::Easy),
        1e-12
    ));

    // A new card ignores the supplied stage.
    let ignored_stage = ScheduleInput {
        memory: None,
        stage: Stage::Review,
        last_review_at_ms: None,
    };
    let same = schedule(ignored_stage, Rating::Good, NOW, &policy).unwrap();
    assert_eq!(same, good);
}

#[test]
fn desired_retention_and_steps_drive_scheduling() {
    let policy = Policy {
        desired_retention: 0.8,
        again_step_minutes: 5,
        hard_step_minutes: 45,
        ..Policy::default()
    };

    let good = schedule(new_card(), Rating::Good, NOW, &policy).unwrap();
    let days = expected_interval_days(0.8, DEFAULT_PARAMS[2]);
    assert_eq!(days, 8);
    assert_eq!(good.interval_minutes, days * 1_440);
    assert_eq!(good.due_at_ms, NOW + day_offset(days) * DAY_MS);

    let again = schedule(new_card(), Rating::Again, NOW, &policy).unwrap();
    assert_eq!(again.interval_minutes, 5);
    assert_eq!(again.due_at_ms, NOW + 5 * MINUTE_MS);

    let hard = schedule(new_card(), Rating::Hard, NOW, &policy).unwrap();
    assert_eq!(hard.interval_minutes, 45);
    assert_eq!(hard.due_at_ms, NOW + 45 * MINUTE_MS);
}

#[test]
fn success_stability_orders_hard_good_easy_and_forgetting_shrinks() {
    let policy = Policy::default();
    let memory = MemoryState {
        stability: DEFAULT_PARAMS[2],
        difficulty: init_difficulty(Rating::Good),
    };
    let last = NOW - 3 * DAY_MS;

    let again = schedule(review_card(memory, last), Rating::Again, NOW, &policy).unwrap();
    let hard = schedule(review_card(memory, last), Rating::Hard, NOW, &policy).unwrap();
    let good = schedule(review_card(memory, last), Rating::Good, NOW, &policy).unwrap();
    let easy = schedule(review_card(memory, last), Rating::Easy, NOW, &policy).unwrap();

    assert!(again.memory.stability < memory.stability);
    assert!(hard.memory.stability >= memory.stability);
    assert!(good.memory.stability >= memory.stability);
    assert!(easy.memory.stability >= memory.stability);
    assert!(again.memory.stability < hard.memory.stability);
    assert!(hard.memory.stability < good.memory.stability);
    assert!(good.memory.stability < easy.memory.stability);

    for result in [again, hard, good, easy] {
        assert_eq!(result.stage, Stage::Review);
    }
}

#[test]
fn difficulty_stays_in_bounds_and_reacts_to_rating() {
    let policy = Policy::default();
    let last = NOW - DAY_MS;
    let memory = MemoryState {
        stability: DEFAULT_PARAMS[2],
        difficulty: init_difficulty(Rating::Good),
    };

    let again = schedule(review_card(memory, last), Rating::Again, NOW, &policy).unwrap();
    let good = schedule(review_card(memory, last), Rating::Good, NOW, &policy).unwrap();
    assert!(good.memory.difficulty < again.memory.difficulty);

    for difficulty in [1.0, 5.0, 10.0] {
        let memory = MemoryState {
            stability: DEFAULT_PARAMS[2],
            difficulty,
        };
        for rating in [Rating::Again, Rating::Hard, Rating::Good, Rating::Easy] {
            let result = schedule(review_card(memory, last), rating, NOW, &policy).unwrap();
            assert!(
                (1.0..=10.0).contains(&result.memory.difficulty),
                "difficulty {difficulty} with {rating:?} left the scale"
            );
        }
    }
}

#[test]
fn same_day_review_uses_short_term_stability() {
    let policy = Policy::default();
    let memory = MemoryState {
        stability: 1.0,
        difficulty: 5.0,
    };
    let last = NOW - 1_000;
    let increment = (DEFAULT_PARAMS[17] * DEFAULT_PARAMS[18]).exp();

    let good = schedule(learning_card(memory, last), Rating::Good, NOW, &policy).unwrap();
    assert!(close(good.memory.stability, increment, 1e-12));
    assert_eq!(good.stage, Stage::Review);
    assert_eq!(good.interval_minutes, 1_440);
    assert_eq!(good.due_at_ms, NOW + DAY_MS);

    let hard = schedule(learning_card(memory, last), Rating::Hard, NOW, &policy).unwrap();
    assert!(close(hard.memory.stability, 1.0, 1e-12));
    assert_eq!(hard.stage, Stage::Learning);
    assert_eq!(hard.interval_minutes, 30);

    let again_increment = (DEFAULT_PARAMS[17] * (1.0 - 3.0 + DEFAULT_PARAMS[18])).exp();
    let again = schedule(learning_card(memory, last), Rating::Again, NOW, &policy).unwrap();
    assert!(close(again.memory.stability, again_increment, 1e-12));
    assert!(again.memory.stability < memory.stability);
    assert_eq!(again.stage, Stage::Learning);
    assert_eq!(again.interval_minutes, 10);
}

#[test]
fn same_day_boundary_follows_rounded_elapsed_days() {
    let policy = Policy::default();
    let memory = MemoryState {
        stability: 1.0,
        difficulty: 5.0,
    };
    let increment = (DEFAULT_PARAMS[17] * DEFAULT_PARAMS[18]).exp();

    // 0.497 days rounds to day zero: short-term update.
    let under = NOW - 43_000_000;
    let short = schedule(learning_card(memory, under), Rating::Good, NOW, &policy).unwrap();
    assert!(close(short.memory.stability, increment, 1e-12));

    // 0.501 days rounds to one day: regular success update.
    let over = NOW - 43_300_000;
    let long = schedule(learning_card(memory, over), Rating::Good, NOW, &policy).unwrap();
    assert!(!close(long.memory.stability, increment, 1e-6));
    assert!(long.memory.stability > memory.stability);
}

#[test]
fn review_again_remains_review_and_schedules_at_least_one_day() {
    let policy = Policy::default();
    let memory = MemoryState {
        stability: 10.0,
        difficulty: 5.0,
    };

    let result = schedule(
        review_card(memory, NOW - 30 * DAY_MS),
        Rating::Again,
        NOW,
        &policy,
    )
    .expect("valid review input");

    assert_eq!(result.stage, Stage::Review);
    assert!(result.memory.stability < memory.stability);
    assert!(result.interval_minutes >= 1_440);
    assert!(result.due_at_ms - NOW >= DAY_MS);

    let expected_days = expected_interval_days(0.9, result.memory.stability);
    assert_eq!(result.interval_minutes, expected_days * 1_440);
    assert_eq!(result.due_at_ms, NOW + day_offset(expected_days) * DAY_MS);
}

#[test]
fn review_intervals_are_clamped_to_one_and_36500_days() {
    let policy = Policy::default();

    let huge = schedule(
        review_card(
            MemoryState {
                stability: 1.0e7,
                difficulty: 5.0,
            },
            NOW - DAY_MS,
        ),
        Rating::Good,
        NOW,
        &policy,
    )
    .unwrap();
    assert_eq!(huge.stage, Stage::Review);
    assert!(close(huge.memory.stability, 36_500.0, 1e-9));
    assert_eq!(huge.interval_minutes, 36_500 * 1_440);
    assert_eq!(huge.due_at_ms - NOW, 36_500 * DAY_MS);

    let tiny = schedule(
        review_card(
            MemoryState {
                stability: S_MIN,
                difficulty: 5.0,
            },
            NOW - DAY_MS,
        ),
        Rating::Again,
        NOW,
        &policy,
    )
    .unwrap();
    assert_eq!(tiny.stage, Stage::Review);
    assert_eq!(tiny.memory.stability, S_MIN);
    assert_eq!(tiny.interval_minutes, 1_440);
    assert_eq!(tiny.due_at_ms - NOW, DAY_MS);
}

#[test]
fn scheduling_is_deterministic() {
    let policy = Policy::default();
    let memory = MemoryState {
        stability: 4.25,
        difficulty: 6.5,
    };
    let input = review_card(memory, NOW - 5 * DAY_MS);

    let first = schedule(input, Rating::Good, NOW, &policy).unwrap();
    let second = schedule(input, Rating::Good, NOW, &policy).unwrap();
    let third = schedule(input, Rating::Good, NOW, &Policy::default()).unwrap();

    assert_eq!(
        first.memory.stability.to_bits(),
        second.memory.stability.to_bits()
    );
    assert_eq!(
        first.memory.stability.to_bits(),
        third.memory.stability.to_bits()
    );
    assert_eq!(
        first.memory.difficulty.to_bits(),
        second.memory.difficulty.to_bits()
    );
    assert_eq!(
        first.memory.difficulty.to_bits(),
        third.memory.difficulty.to_bits()
    );
    assert_eq!(first.stage, second.stage);
    assert_eq!(first.stage, third.stage);
    assert_eq!(first.due_at_ms, second.due_at_ms);
    assert_eq!(first.due_at_ms, third.due_at_ms);
    assert_eq!(first.interval_minutes, second.interval_minutes);
    assert_eq!(first.interval_minutes, third.interval_minutes);
}

#[test]
fn invalid_inputs_return_errors_instead_of_panicking() {
    let valid_memory = MemoryState {
        stability: 3.0,
        difficulty: 5.0,
    };
    let valid_last = NOW - DAY_MS;
    let default_policy = Policy::default();

    let high_retention = Policy {
        desired_retention: 1.0,
        ..Policy::default()
    };
    let low_retention = Policy {
        desired_retention: 0.0,
        ..Policy::default()
    };
    let boundary_low_retention = Policy {
        desired_retention: 0.0001,
        ..Policy::default()
    };
    let boundary_high_retention = Policy {
        desired_retention: 0.9999,
        ..Policy::default()
    };
    let nan_retention = Policy {
        desired_retention: f64::NAN,
        ..Policy::default()
    };
    let zero_again_step = Policy {
        again_step_minutes: 0,
        ..Policy::default()
    };
    let zero_hard_step = Policy {
        hard_step_minutes: 0,
        ..Policy::default()
    };
    let nan_weight = Policy {
        params: {
            let mut params = DEFAULT_PARAMS;
            params[3] = f64::NAN;
            params
        },
        ..Policy::default()
    };
    let infinite_weight = Policy {
        params: {
            let mut params = DEFAULT_PARAMS;
            params[0] = f64::INFINITY;
            params
        },
        ..Policy::default()
    };

    let cases = [
        (
            "missing last review timestamp",
            ScheduleInput {
                memory: Some(valid_memory),
                stage: Stage::Review,
                last_review_at_ms: None,
            },
            NOW,
            default_policy,
        ),
        (
            "unexpected last review timestamp",
            ScheduleInput {
                memory: None,
                stage: Stage::Learning,
                last_review_at_ms: Some(valid_last),
            },
            NOW,
            default_policy,
        ),
        ("zero now", new_card(), 0, default_policy),
        ("negative now", new_card(), -1, default_policy),
        (
            "zero last review timestamp",
            review_card(valid_memory, 0),
            NOW,
            default_policy,
        ),
        (
            "last review after now",
            review_card(valid_memory, NOW + 1),
            NOW,
            default_policy,
        ),
        (
            "zero stability",
            review_card(
                MemoryState {
                    stability: 0.0,
                    difficulty: 5.0,
                },
                valid_last,
            ),
            NOW,
            default_policy,
        ),
        (
            "negative stability",
            review_card(
                MemoryState {
                    stability: -1.0,
                    difficulty: 5.0,
                },
                valid_last,
            ),
            NOW,
            default_policy,
        ),
        (
            "NaN stability",
            review_card(
                MemoryState {
                    stability: f64::NAN,
                    difficulty: 5.0,
                },
                valid_last,
            ),
            NOW,
            default_policy,
        ),
        (
            "infinite stability",
            review_card(
                MemoryState {
                    stability: f64::INFINITY,
                    difficulty: 5.0,
                },
                valid_last,
            ),
            NOW,
            default_policy,
        ),
        (
            "difficulty below scale",
            review_card(
                MemoryState {
                    stability: 3.0,
                    difficulty: 0.5,
                },
                valid_last,
            ),
            NOW,
            default_policy,
        ),
        (
            "difficulty above scale",
            review_card(
                MemoryState {
                    stability: 3.0,
                    difficulty: 10.5,
                },
                valid_last,
            ),
            NOW,
            default_policy,
        ),
        (
            "NaN difficulty",
            review_card(
                MemoryState {
                    stability: 3.0,
                    difficulty: f64::NAN,
                },
                valid_last,
            ),
            NOW,
            default_policy,
        ),
        ("zero retention", new_card(), NOW, low_retention),
        ("unit retention", new_card(), NOW, high_retention),
        (
            "retention at lower boundary",
            new_card(),
            NOW,
            boundary_low_retention,
        ),
        (
            "retention at upper boundary",
            new_card(),
            NOW,
            boundary_high_retention,
        ),
        ("NaN retention", new_card(), NOW, nan_retention),
        ("zero again step", new_card(), NOW, zero_again_step),
        ("zero hard step", new_card(), NOW, zero_hard_step),
        ("NaN weight", new_card(), NOW, nan_weight),
        ("infinite weight", new_card(), NOW, infinite_weight),
    ];

    for (label, input, now_ms, policy) in cases {
        assert_eq!(
            schedule(input, Rating::Good, now_ms, &policy),
            Err(SchedulerError::InvalidInput),
            "{label}"
        );
    }

    for rating in [Rating::Again, Rating::Hard, Rating::Good, Rating::Easy] {
        assert_eq!(
            schedule(new_card(), rating, 0, &default_policy),
            Err(SchedulerError::InvalidInput),
            "zero now with {rating:?}"
        );
    }
}

#[test]
fn unrepresentable_due_timestamp_fails_safely() {
    let policy = Policy::default();
    assert_eq!(
        schedule(new_card(), Rating::Again, i64::MAX, &policy),
        Err(SchedulerError::InvalidInput)
    );
}

#[test]
fn rating_codes_round_trip() {
    assert_eq!(Rating::Again.code(), 1);
    assert_eq!(Rating::Hard.code(), 2);
    assert_eq!(Rating::Good.code(), 3);
    assert_eq!(Rating::Easy.code(), 4);

    for rating in [Rating::Again, Rating::Hard, Rating::Good, Rating::Easy] {
        assert_eq!(Rating::from_code(rating.code()), Some(rating));
    }
    assert_eq!(Rating::from_code(0), None);
    assert_eq!(Rating::from_code(5), None);
    assert_eq!(Rating::from_code(u8::MAX), None);
}

#[test]
fn scheduler_error_implements_display_and_error() {
    let error = SchedulerError::InvalidInput;
    assert_eq!(error.to_string(), "invalid scheduler input");
    let dynamic: &dyn std::error::Error = &error;
    assert!(!dynamic.to_string().is_empty());
}

#[test]
fn public_api_is_reachable_through_module_path() {
    let policy = learning_domain::fsrs::Policy::default();
    let result = learning_domain::fsrs::schedule(
        learning_domain::fsrs::ScheduleInput {
            memory: None,
            stage: learning_domain::fsrs::Stage::Learning,
            last_review_at_ms: None,
        },
        learning_domain::fsrs::Rating::Again,
        NOW,
        &policy,
    );
    assert!(result.is_ok());
}
