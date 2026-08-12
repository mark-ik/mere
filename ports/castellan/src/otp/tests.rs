// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The published vectors, which are the whole reason to start here: the
//! implementation is checked against the RFCs rather than against itself.

use super::*;

/// RFC 4226 Appendix D uses this ASCII string as the shared secret.
const RFC4226_SECRET: &[u8] = b"12345678901234567890";

/// RFC 6238 Appendix B's table looks like it shares one seed, but each
/// algorithm actually uses a seed of its own hash length. Reusing the 20-byte
/// SHA-1 seed for SHA-256 and SHA-512 is the classic way to "fail" these
/// vectors while the implementation is correct.
const RFC6238_SHA1_SECRET: &[u8] = b"12345678901234567890";
const RFC6238_SHA256_SECRET: &[u8] = b"12345678901234567890123456789012";
const RFC6238_SHA512_SECRET: &[u8] =
    b"1234567890123456789012345678901234567890123456789012345678901234";

#[test]
fn rfc4226_appendix_d_hotp_values() {
    let expected = [
        "755224", "287082", "359152", "969429", "338314", "254676", "287922", "162583", "399871",
        "520489",
    ];
    let otp = Otp::hotp(RFC4226_SECRET.to_vec(), 0);
    for (counter, want) in expected.iter().enumerate() {
        assert_eq!(
            &otp.code_for_counter(counter as u64).unwrap(),
            want,
            "RFC 4226 counter {counter}"
        );
    }
}

#[test]
fn rfc6238_appendix_b_totp_values() {
    // (unix time, sha1, sha256, sha512)
    let cases: &[(u64, &str, &str, &str)] = &[
        (59, "94287082", "46119246", "90693936"),
        (1_111_111_109, "07081804", "68084774", "25091201"),
        (1_111_111_111, "14050471", "67062674", "99943326"),
        (1_234_567_890, "89005924", "91819424", "93441116"),
        (2_000_000_000, "69279037", "90698825", "38618901"),
        (20_000_000_000, "65353130", "77737706", "47863826"),
    ];

    for &(time, sha1, sha256, sha512) in cases {
        let variants = [
            (OtpAlgorithm::Sha1, RFC6238_SHA1_SECRET, sha1),
            (OtpAlgorithm::Sha256, RFC6238_SHA256_SECRET, sha256),
            (OtpAlgorithm::Sha512, RFC6238_SHA512_SECRET, sha512),
        ];
        for (algorithm, secret, want) in variants {
            let otp = Otp::totp(secret.to_vec())
                .with_algorithm(algorithm)
                .with_digits(8)
                .unwrap();
            assert_eq!(
                &otp.code_at_unix_time(time).unwrap(),
                want,
                "RFC 6238 {algorithm} at t={time}"
            );
        }
    }
}

#[test]
fn totp_counter_is_the_step_count_since_t0() {
    let otp = Otp::totp(RFC4226_SECRET.to_vec());
    assert_eq!(otp.counter_at_unix_time(0).unwrap(), 0);
    assert_eq!(otp.counter_at_unix_time(29).unwrap(), 0);
    assert_eq!(otp.counter_at_unix_time(30).unwrap(), 1);
    // RFC 6238 §4 states this one explicitly.
    assert_eq!(otp.counter_at_unix_time(59).unwrap(), 1);
    assert_eq!(otp.counter_at_unix_time(1_111_111_109).unwrap(), 0x023523EC);
}

#[test]
fn seconds_remaining_counts_down_within_the_step() {
    let otp = Otp::totp(RFC4226_SECRET.to_vec());
    assert_eq!(otp.seconds_remaining_at(0), Some(30));
    assert_eq!(otp.seconds_remaining_at(1), Some(29));
    assert_eq!(otp.seconds_remaining_at(29), Some(1));
    assert_eq!(otp.seconds_remaining_at(30), Some(30));
    // Counter-based codes do not expire, so there is nothing to count down.
    assert_eq!(
        Otp::hotp(RFC4226_SECRET.to_vec(), 0).seconds_remaining_at(0),
        None
    );
}

#[test]
fn verification_accepts_the_adjacent_step_when_skew_is_allowed() {
    let otp = Otp::totp(RFC6238_SHA1_SECRET.to_vec())
        .with_digits(8)
        .unwrap();
    let previous = otp.code_at_unix_time(29).unwrap();
    let current = otp.code_at_unix_time(59).unwrap();

    assert!(otp.verify_at_unix_time(&current, 59, 0).unwrap());
    // The previous step's code is refused with no skew and accepted with one.
    assert!(!otp.verify_at_unix_time(&previous, 59, 0).unwrap());
    assert!(otp.verify_at_unix_time(&previous, 59, 1).unwrap());
    assert!(!otp.verify_at_unix_time("00000000", 59, 1).unwrap());
}

#[test]
fn digit_counts_outside_the_representable_range_are_refused() {
    let secret = RFC4226_SECRET.to_vec();
    assert_eq!(
        Otp::totp(secret.clone()).with_digits(5).unwrap_err(),
        OtpError::UnsupportedDigits(5)
    );
    // Dynamic truncation yields 31 bits, so 10 digits is the ceiling.
    assert!(Otp::totp(secret.clone()).with_digits(10).is_ok());
    assert_eq!(
        Otp::totp(secret).with_digits(11).unwrap_err(),
        OtpError::UnsupportedDigits(11)
    );
}

#[test]
fn an_empty_secret_is_refused_rather_than_producing_a_code() {
    assert_eq!(
        Otp::totp(Vec::new()).code_at_unix_time(59).unwrap_err(),
        OtpError::EmptySecret
    );
}

#[test]
fn a_zero_period_is_refused() {
    assert_eq!(
        Otp::totp(RFC4226_SECRET.to_vec())
            .with_period(0)
            .unwrap_err(),
        OtpError::ZeroPeriod
    );
}

#[test]
fn debug_does_not_print_the_secret() {
    let otp = Otp::totp(b"super-secret-value".to_vec());
    let rendered = format!("{otp:?}");
    assert!(!rendered.contains("super-secret-value"), "{rendered}");
    assert!(rendered.contains("redacted"), "{rendered}");
}

// ── otpauth:// URIs ──────────────────────────────────────────────────────────

#[test]
fn a_typical_authenticator_uri_parses() {
    let (otp, meta) = parse_otpauth_uri(
        "otpauth://totp/ACME%20Co:john.doe@email.com\
         ?secret=HXDMVJECJJWSRB3HWIZR4IFUGFTMXBOZ&issuer=ACME%20Co&algorithm=SHA1\
         &digits=6&period=30",
    )
    .unwrap();
    assert_eq!(meta.account, "john.doe@email.com");
    assert_eq!(meta.issuer.as_deref(), Some("ACME Co"));
    assert_eq!(otp.digits(), 6);
    assert_eq!(otp.algorithm(), OtpAlgorithm::Sha1);
    assert_eq!(otp.kind(), OtpKind::Totp { period: 30, t0: 0 });
}

#[test]
fn the_issuer_parameter_wins_over_a_stale_label_prefix() {
    let (_, meta) =
        parse_otpauth_uri("otpauth://totp/OldName:alice?secret=MZXW6YTBOI&issuer=NewName").unwrap();
    assert_eq!(meta.issuer.as_deref(), Some("NewName"));
    assert_eq!(meta.account, "alice");
}

#[test]
fn the_label_prefix_is_used_when_no_issuer_parameter_is_given() {
    let (_, meta) = parse_otpauth_uri("otpauth://totp/GitHub:alice?secret=MZXW6YTBOI").unwrap();
    assert_eq!(meta.issuer.as_deref(), Some("GitHub"));
    assert_eq!(meta.account, "alice");
}

#[test]
fn defaults_apply_when_optional_parameters_are_absent() {
    let (otp, meta) = parse_otpauth_uri("otpauth://totp/alice?secret=MZXW6YTBOI").unwrap();
    assert_eq!(otp.digits(), DEFAULT_DIGITS);
    assert_eq!(otp.algorithm(), OtpAlgorithm::Sha1);
    assert_eq!(
        otp.kind(),
        OtpKind::Totp {
            period: DEFAULT_PERIOD_SECS,
            t0: 0
        }
    );
    assert_eq!(meta.issuer, None);
}

#[test]
fn an_hotp_uri_carries_its_counter() {
    let (otp, _) = parse_otpauth_uri("otpauth://hotp/alice?secret=MZXW6YTBOI&counter=7").unwrap();
    assert_eq!(otp.kind(), OtpKind::Hotp { counter: 7 });
}

#[test]
fn an_hotp_uri_without_a_counter_is_refused() {
    assert_eq!(
        parse_otpauth_uri("otpauth://hotp/alice?secret=MZXW6YTBOI").unwrap_err(),
        OtpUriError::MissingCounter
    );
}

#[test]
fn unknown_parameters_are_ignored_rather_than_rejected() {
    // Issuers add their own; a credential that is otherwise valid must import.
    let (otp, _) = parse_otpauth_uri(
        "otpauth://totp/alice?secret=MZXW6YTBOI&lock=true&image=https://example.com/i.png",
    )
    .unwrap();
    assert_eq!(otp.digits(), DEFAULT_DIGITS);
}

#[test]
fn an_unpadded_secret_parses_the_way_qr_codes_write_it() {
    let (padded, _) = parse_otpauth_uri("otpauth://totp/a?secret=MZXW6===").unwrap();
    let (unpadded, _) = parse_otpauth_uri("otpauth://totp/a?secret=MZXW6").unwrap();
    assert_eq!(
        padded.code_at_unix_time(59).unwrap(),
        unpadded.code_at_unix_time(59).unwrap()
    );
}

#[test]
fn the_scheme_is_matched_case_insensitively() {
    assert!(parse_otpauth_uri("OTPAUTH://TOTP/alice?secret=MZXW6YTBOI").is_ok());
}

#[test]
fn malformed_uris_are_refused_with_a_reason() {
    assert_eq!(
        parse_otpauth_uri("https://example.com").unwrap_err(),
        OtpUriError::NotOtpauth
    );
    assert_eq!(
        parse_otpauth_uri("otpauth://totp/alice").unwrap_err(),
        OtpUriError::MissingSecret
    );
    assert_eq!(
        parse_otpauth_uri("otpauth://wat/alice?secret=MZXW6YTBOI").unwrap_err(),
        OtpUriError::UnknownType("wat".into())
    );
    assert!(matches!(
        parse_otpauth_uri("otpauth://totp/alice?secret=MZXW6YTBOI&digits=many").unwrap_err(),
        OtpUriError::InvalidNumber {
            parameter: "digits",
            ..
        }
    ));
    assert!(matches!(
        parse_otpauth_uri("otpauth://totp/alice?secret=!!!").unwrap_err(),
        OtpUriError::Secret(_)
    ));
}

/// A URI carrying a known RFC seed must produce the RFC's codes, which ties
/// the two halves of this slice together end to end.
#[test]
fn a_parsed_uri_reproduces_the_rfc_vector() {
    // base32("12345678901234567890") = GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ
    let (otp, _) =
        parse_otpauth_uri("otpauth://totp/rfc?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&digits=8")
            .unwrap();
    assert_eq!(otp.code_at_unix_time(59).unwrap(), "94287082");
    assert_eq!(otp.code_at_unix_time(1_234_567_890).unwrap(), "89005924");
}
