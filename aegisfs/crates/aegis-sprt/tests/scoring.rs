use aegis_sprt::{Config, Decision, Evidence, Likelihood, Scorer, ScoringError};
use proptest::prelude::*;

fn config() -> Config {
    let l = Likelihood::new(0.2, 0.8).unwrap();
    Config {
        uniform: l,
        header_change: l,
        unlink_or_rename: l,
        directory_jump: l,
        canary: l,
        decay_per_second: 0.5,
        threshold: 2.0,
        max_groups: 2,
    }
}
#[test]
fn positive_negative_and_missing_features_are_distinct() {
    let mut scorer = Scorer::new(config()).unwrap();
    let first = scorer
        .observe(
            "group-1",
            1,
            Evidence {
                uniform: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
    assert!((first.snapshot.score - 4.0f64.ln()).abs() < 1e-12);
    assert_eq!(first.decision, Decision::Continue);
    let second = scorer
        .observe(
            "group-1",
            1,
            Evidence {
                uniform: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(second.decision, Decision::AboveThreshold);
    let third = scorer
        .observe(
            "group-1",
            1,
            Evidence {
                uniform: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
    assert!((third.snapshot.score - 4.0f64.ln()).abs() < 1e-12);
    let fourth = scorer.observe("group-1", 1, Evidence::default()).unwrap();
    assert!((fourth.snapshot.score - third.snapshot.score).abs() < 1e-12);
    assert!(fourth.snapshot.peak_score > fourth.snapshot.score);
}
#[test]
fn time_decay_reflection_and_ordering_are_explicit() {
    let mut scorer = Scorer::new(config()).unwrap();
    let e = Evidence {
        uniform: Some(true),
        ..Default::default()
    };
    scorer.observe("g", 10, e).unwrap();
    let u = scorer
        .observe("g", 2_000_000_010, Evidence::default())
        .unwrap();
    assert!((u.decay_factor - (-1.0f64).exp()).abs() < 1e-12);
    assert!(matches!(
        scorer.observe("g", 9, e),
        Err(ScoringError::TimeWentBackwards)
    ));
    assert_eq!(scorer.snapshot("g").unwrap().observations, 2);
    let negative = scorer
        .observe(
            "g",
            2_000_000_010,
            Evidence {
                uniform: Some(false),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(negative.snapshot.score, 0.0);
}
#[test]
fn groups_are_isolated_bounded_and_explicitly_retired() {
    let mut s = Scorer::new(config()).unwrap();
    let e = Evidence {
        canary: Some(true),
        ..Default::default()
    };
    let one = s.observe("one", 0, e).unwrap();
    let two = s.observe("two", 0, Evidence::default()).unwrap();
    assert!(one.snapshot.score > two.snapshot.score);
    assert_eq!(
        s.observe("three", 0, e).unwrap_err(),
        ScoringError::Capacity
    );
    assert_eq!(
        s.observe("bad/name", 0, e).unwrap_err(),
        ScoringError::InvalidGroup
    );
    assert!(s.forget("one"));
    assert!(!s.forget("one"));
    assert!(s.observe("three", 0, e).is_ok());
    assert_eq!(s.group_count(), 2);
}
#[test]
fn invalid_parameters_are_rejected_even_if_struct_constructed_directly() {
    assert!(Likelihood::new(0.0, 0.5).is_err());
    assert!(Likelihood::new(f64::NAN, 0.5).is_err());
    let mut c = config();
    c.uniform = Likelihood {
        benign: 0.0,
        suspicious: 0.5,
    };
    assert!(matches!(
        Scorer::new(c),
        Err(ScoringError::InvalidProbability)
    ));
    let mut c = config();
    c.threshold = f64::INFINITY;
    assert!(matches!(Scorer::new(c), Err(ScoringError::InvalidConfig)));
}
#[test]
fn extreme_likelihoods_do_not_overflow_ratios() {
    let mut c = config();
    c.uniform = Likelihood::new(f64::from_bits(1), 0.5).unwrap();
    let mut s = Scorer::new(c).unwrap();
    let u = s
        .observe(
            "g",
            0,
            Evidence {
                uniform: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(u.snapshot.score.is_finite());
}
proptest! {
    #[test]
    fn arbitrary_boolean_stream_stays_finite_and_nonnegative(events in proptest::collection::vec(any::<bool>(), 0..100)) {
        let mut s=Scorer::new(config()).unwrap();
        for (i,event) in events.into_iter().enumerate() {
            let u=s.observe("test", i as u64 * 1_000_000, Evidence{uniform:Some(event),..Default::default()}).unwrap();
            prop_assert!(u.snapshot.score.is_finite()); prop_assert!(u.snapshot.score>=0.0);
            prop_assert!(u.snapshot.peak_score>=u.snapshot.score);
        }
    }
}
