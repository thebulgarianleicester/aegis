use aegis_classifier::{classify, stride::StrideTracker};

#[test]
fn constant_and_balanced_distributions() {
    let zeros = vec![0; 4096];
    let balanced: Vec<u8> = (0..4096).map(|i| (i % 256) as u8).collect();
    let f = classify(&zeros, &balanced, 0).unwrap();
    assert!((f.entropy_pre - 0.0).abs() < 1e-12);
    assert!((f.entropy_post - 8.0).abs() < 1e-12);
    assert!(f.chi2.unwrap().abs() < 1e-12);
    assert!((f.chi2_p.unwrap() - 1.0).abs() < 1e-12);
    assert!(f.kl_post_pre_bits > 0.0);
}
#[test]
fn upper_tail_is_small_for_biased_input() {
    let data = vec![0; 4096];
    let f = classify(&data, &data, 0).unwrap();
    assert!(f.chi2_p.unwrap() < 1e-10);
    assert!(f.kl_post_pre_bits.abs() < 1e-12);
    assert_eq!(f.serial_correlation_post, None);
}
#[test]
fn short_and_invalid_inputs_are_explicit() {
    assert!(classify(&[], &[], 0).is_err());
    assert!(classify(&[1], &[1, 2], 0).is_err());
    let f = classify(&[0; 40], &[1; 40], 0).unwrap();
    assert!(f.chi2_p.is_none());
}
#[test]
fn header_detection_only_at_zero() {
    let mut pre = vec![0; 4096];
    pre[..5].copy_from_slice(b"%PDF-");
    let post = vec![1; 4096];
    assert!(classify(&pre, &post, 0).unwrap().header_destroyed);
    assert!(!classify(&pre, &post, 4096).unwrap().header_destroyed);
    let mut other = vec![0; 4096];
    other[..4].copy_from_slice(b"PK\x03\x04");
    assert!(classify(&pre, &other, 0).unwrap().header_destroyed);
}
#[test]
fn stride_requires_three_gaps_and_resets_on_reordering() {
    let mut s = StrideTracker::new(4);
    for i in 0..4 {
        s.observe(i * 8192, 4096);
    }
    assert_eq!(s.coefficient_of_variation(), Some(0.0));
    s.observe(0, 4096);
    assert_eq!(s.coefficient_of_variation(), None);
}

#[test]
fn chi_square_survival_matches_independent_reference() {
    // Counts 12 for 128 bins and 20 for 128 bins => chi^2 = 256, df = 255.
    // Reference: scipy.stats.chi2.sf(256, 255) = 0.4706088676271671.
    let mut sample = Vec::new();
    for byte in 0..=255u8 {
        sample.extend(std::iter::repeat_n(byte, if byte < 128 { 12 } else { 20 }));
    }
    let f = classify(&sample, &sample, 0).unwrap();
    assert!((f.chi2.unwrap() - 256.0).abs() < 1e-12);
    assert!((f.chi2_p.unwrap() - 0.4706088676271671).abs() < 1e-11);
}
