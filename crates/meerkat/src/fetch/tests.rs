/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Fetch tests.

mod tests {
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
