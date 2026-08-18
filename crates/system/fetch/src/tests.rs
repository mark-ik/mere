// Copyright 2026 Mark AB (markik)
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Fetch tests.

use super::*;

#[test]
fn fetchable_for_http_and_smolweb_schemes() {
    assert!(is_fetchable("http://example.com"));
    assert!(is_fetchable("https://example.com"));
    assert!(is_fetchable("gemini://capsule.example/"));
    assert!(is_fetchable("gopher://example.org/"));
    assert!(is_fetchable("finger://example.org/alice"));
    assert!(is_fetchable("spartan://example.org/"));
    assert!(is_fetchable("nex://nightfall.city/"));
    assert!(is_fetchable("guppy://mozz.us/"));
    assert!(is_fetchable("titan://capsule.example/page"));
    assert!(!is_fetchable("mere://welcome"));
    assert!(!is_fetchable("about:blank"));
}

#[test]
fn smolweb_content_type_tags_fixed_schemes_and_passes_others_through() {
    fn resp_for(url: &url::Url, mime: &str) -> errand::Response {
        errand::Response {
            url: url.clone(),
            status: errand::Status::Success,
            raw_status: None,
            meta: mime.to_string(),
            body: Vec::new(),
        }
    }

    let finger = url::Url::parse("finger://example.org/alice").unwrap();
    assert_eq!(
        smolweb_content_type(&finger, &resp_for(&finger, "text/plain")),
        "text/x-finger"
    );

    let nex = url::Url::parse("nex://nightfall.city/").unwrap();
    assert_eq!(
        smolweb_content_type(&nex, &resp_for(&nex, "")),
        "application/x-nex"
    );

    let guppy = url::Url::parse("guppy://mozz.us/").unwrap();
    assert_eq!(
        smolweb_content_type(&guppy, &resp_for(&guppy, "text/gemini")),
        "application/x-guppy"
    );

    let titan = url::Url::parse("titan://capsule.example/page").unwrap();
    assert_eq!(
        smolweb_content_type(&titan, &resp_for(&titan, "text/gemini")),
        "application/x-titan"
    );

    let gem = url::Url::parse("gemini://capsule.example/").unwrap();
    let gem_resp = errand::Response {
        url: gem.clone(),
        status: errand::Status::Success,
        raw_status: Some(20),
        meta: "text/gemini; charset=utf-8".into(),
        body: Vec::new(),
    };
    assert_eq!(smolweb_content_type(&gem, &gem_resp), "text/gemini");
}

#[test]
fn smolweb_input_preserves_prompt_target_and_sensitivity() {
    let target = url::Url::parse("gemini://capsule.example/search").unwrap();
    let response = |code| errand::Response {
        url: target.clone(),
        status: errand::Status::Input,
        raw_status: Some(code),
        meta: "Search the capsule".into(),
        body: Vec::new(),
    };

    assert_eq!(
        smolweb_input_failure(&target, &response(10)),
        FetchFailure::InputRequired {
            url: target.to_string(),
            prompt: "Search the capsule".into(),
            sensitive: false,
        }
    );
    assert_eq!(
        smolweb_input_failure(&target, &response(11)),
        FetchFailure::InputRequired {
            url: target.to_string(),
            prompt: "Search the capsule".into(),
            sensitive: true,
        }
    );
}

#[test]
fn smolweb_trace_address_omits_query_and_fragment() {
    assert_eq!(
        url_without_query("gemini://capsule.example/search?secret%20answer#part"),
        "gemini://capsule.example/search"
    );
}

#[test]
fn gemini_identity_is_scoped_to_one_capsule_origin() {
    let identity = GeminiClientIdentity::new(
        "gemini://Capsule.Example/account",
        vec![1, 2, 3],
        vec![4, 5, 6],
    )
    .unwrap();
    assert!(identity.applies_to(&url::Url::parse("gemini://capsule.example/private").unwrap()));
    assert!(identity.applies_to(&url::Url::parse("gemini://capsule.example:1965/other").unwrap()));
    assert!(!identity.applies_to(&url::Url::parse("gemini://other.example/private").unwrap()));
    assert!(
        !identity.applies_to(&url::Url::parse("gemini://capsule.example:1966/private").unwrap())
    );
    assert!(!identity.applies_to(&url::Url::parse("https://capsule.example/private").unwrap()));
    assert_eq!(identity.origin(), "gemini://capsule.example");
}

#[test]
fn state_tag_distinguishes_transitions() {
    let ready = ContentState::Ready(Fetched {
        content_type: None,
        body: String::new(),
    });
    assert_eq!(ContentState::tag(None), 0);
    assert_eq!(ContentState::tag(Some(&ContentState::Loading)), 1);
    assert_ne!(
        ContentState::tag(Some(&ContentState::Loading)),
        ContentState::tag(Some(&ready)),
        "Loading and Ready re-key the card",
    );
    assert_ne!(
        ContentState::tag(Some(&ready)),
        ContentState::tag(Some(&ContentState::Failed("x".into()))),
    );
}
