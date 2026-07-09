/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Content-script tests.

use super::*;

fn find_element(dom: &ScriptedDom, id: NodeId, tag: &str) -> Option<NodeId> {
    if dom.kind(id) == NodeKind::Element
        && dom.element_name(id).map(|q| q.local.to_string()).as_deref() == Some(tag)
    {
        return Some(id);
    }
    for child in dom.dom_children(id) {
        if let Some(found) = find_element(dom, child, tag) {
            return Some(found);
        }
    }
    None
}

#[test]
fn mirror_preserves_elements_attributes_and_text() {
    let doc = StaticDocument::parse("<body><p class=\"intro\">Hello</p></body>");
    let dom = mirror_to_scripted_dom(&doc);

    let p = find_element(&dom, dom.document(), "p").expect("mirror keeps the <p>");
    assert_eq!(
        dom.attribute(
            p,
            &layout_dom_api::Namespace::from(""),
            &layout_dom_api::LocalName::from("class")
        ),
        Some("intro"),
        "the class attribute is carried into the mirror",
    );
    let text = dom
        .dom_children(p)
        .next()
        .expect("the <p> has a text child");
    assert_eq!(
        dom.text(text),
        Some("Hello"),
        "the text node is carried into the mirror"
    );
}

#[test]
fn grant_maps_resolved_permissions() {
    use mere::kernel::permissions::SettingScope;
    let allow = ResolvedPermission {
        effective: Permission::Allow,
        decided_by: None,
    };
    let deny = ResolvedPermission {
        effective: Permission::Deny,
        decided_by: Some(SettingScope::Surface),
    };
    let prompt = ResolvedPermission {
        effective: Permission::Prompt,
        decided_by: None,
    };

    let g = grant_from_resolved(allow, allow, allow);
    assert_eq!(g.log, CapPermission::Allow);
    assert_eq!(g.document, CapPermission::Allow);
    assert_eq!(g.net, CapPermission::Allow);

    // A denied document capability maps to Deny -> the import is omitted, so a
    // component that requires it fails to instantiate (the enforced boundary).
    assert_eq!(
        grant_from_resolved(allow, deny, allow).document,
        CapPermission::Deny
    );
    // Prompt is preserved (P2 omits it conservatively, like Deny, at link time).
    assert_eq!(
        grant_from_resolved(allow, prompt, allow).document,
        CapPermission::Prompt
    );
    // net maps independently (a denied net omits the import).
    assert_eq!(
        grant_from_resolved(allow, allow, deny).net,
        CapPermission::Deny
    );
}

#[test]
fn attach_permissions_resolve_with_narrowing() {
    // No Session override: log/document default Allow; net defaults Deny (powerful).
    let (log, document, net) = resolve_attach_permissions(ScriptCapPolicy::default());
    assert_eq!(log.effective, Permission::Allow);
    assert_eq!(document.effective, Permission::Allow);
    assert_eq!(
        net.effective,
        Permission::Deny,
        "net is denied unless granted"
    );

    // A Session-scope Deny on `document` narrows past the Allow baseline (the
    // narrowing rule); the grant then omits the document import — a session-wide
    // "no script may touch the page" switch.
    let policy = ScriptCapPolicy {
        document: Some(Permission::Deny),
        ..Default::default()
    };
    let (_log, document, _net) = resolve_attach_permissions(policy);
    assert_eq!(document.effective, Permission::Deny);
    assert_eq!(document.decided_by, Some(SettingScope::Session));

    // A Session-scope Allow on `net` GRANTS it over the Deny baseline — the
    // loop-closing case: a user opts a script into network egress. (This is why
    // the baseline is the resolve default, not an explicit App Deny — an App Deny
    // could never be granted back by a narrower scope.)
    let policy = ScriptCapPolicy {
        net: Some(Permission::Allow),
        ..Default::default()
    };
    let (log, _doc, net) = resolve_attach_permissions(policy);
    assert_eq!(
        net.effective,
        Permission::Allow,
        "a session grant flips net on"
    );
    assert_eq!(
        grant_from_resolved(log, _doc, net).net,
        CapPermission::Allow
    );
}

#[test]
fn origin_matching_handles_exact_host_and_suffix_glob() {
    assert!(origin_matches("https://example.com/path", "example.com"));
    assert!(!origin_matches("https://evil.com/", "example.com"));
    // `*.` matches the apex and any subdomain, not an unrelated host.
    assert!(origin_matches(
        "https://en.wikipedia.org/wiki/X",
        "*.wikipedia.org"
    ));
    assert!(origin_matches("https://wikipedia.org/", "*.wikipedia.org"));
    assert!(!origin_matches(
        "https://notwikipedia.org/",
        "*.wikipedia.org"
    ));
    // Host extraction drops scheme / path / query / fragment (any scheme).
    assert!(origin_matches(
        "gemini://example.com/cap?q#f",
        "example.com"
    ));
    // Userinfo + port + case are normalized away, so a non-default port or a
    // cased host still matches (and userinfo cannot smuggle a different host).
    assert!(origin_matches("https://example.com:8443/", "example.com"));
    assert!(origin_matches("https://EXAMPLE.com/", "example.com"));
    assert!(origin_matches(
        "https://x.wikipedia.org:8443/",
        "*.wikipedia.org"
    ));
    assert!(!origin_matches(
        "https://example.com@evil.com/",
        "example.com"
    ));
}

#[test]
fn ssrf_floor_blocks_loopback_and_private_hosts() {
    assert!(is_disallowed_fetch_host("http://localhost/"));
    assert!(is_disallowed_fetch_host("http://127.0.0.1/x"));
    assert!(is_disallowed_fetch_host("http://10.0.0.5/"));
    assert!(is_disallowed_fetch_host("http://192.168.1.1/"));
    assert!(is_disallowed_fetch_host("http://169.254.1.1/"));
    assert!(is_disallowed_fetch_host("http://[::1]/"));
    assert!(is_disallowed_fetch_host("http://100.64.0.1/"));
    // Public hosts (name or IP) are allowed through.
    assert!(!is_disallowed_fetch_host("https://example.com/"));
    assert!(!is_disallowed_fetch_host("https://1.1.1.1/"));
}

#[test]
fn binding_for_finds_the_matching_origin() {
    let allow = ResolvedPermission {
        effective: Permission::Allow,
        decided_by: None,
    };
    let bindings = vec![
        ResolvedScriptBinding {
            origin: "example.com".into(),
            component_path: PathBuf::from("a.wasm"),
            log: allow,
            document: allow,
            net: allow,
        },
        ResolvedScriptBinding {
            origin: "*.wikipedia.org".into(),
            component_path: PathBuf::from("w.wasm"),
            log: allow,
            document: allow,
            net: allow,
        },
    ];
    assert_eq!(
        binding_for("https://en.wikipedia.org/x", &bindings).map(|b| b.component_path.clone()),
        Some(PathBuf::from("w.wasm")),
    );
    assert!(binding_for("https://other.net/", &bindings).is_none());
}

#[test]
fn load_mod_bindings_derives_bindings_from_installed_mods() {
    let mere_root = tempfile::tempdir().expect("temp mere_root");
    let mods_dir = mere_root.path().join("mods");
    std::fs::create_dir_all(&mods_dir).expect("mods dir");
    // A valid component preamble (\0asm + version/layer), required by the
    // tightened validate_wasm_binary; a bare core-module header is rejected.
    let wasm = [0x00, 0x61, 0x73, 0x6d, 0x0d, 0x00, 0x01, 0x00];
    // A net-declaring mod (two origins), a document-only mod (no Network), and a
    // plain mod with no document-script declaration (contributes nothing).
    std::fs::write(mods_dir.join("weather.wasm"), wasm).unwrap();
    std::fs::write(
        mods_dir.join("weather.wasm.toml"),
        "mod_id = \"mod:weather\"\ncapabilities = [\"network\"]\ndocument_script_origins = [\"example.com\", \"*.weather.test\"]\n",
    )
    .unwrap();
    std::fs::write(mods_dir.join("reader.wasm"), wasm).unwrap();
    std::fs::write(
        mods_dir.join("reader.wasm.toml"),
        "mod_id = \"mod:reader\"\ndocument_script_origins = [\"reader.example\"]\n",
    )
    .unwrap();
    std::fs::write(mods_dir.join("plain.wasm"), wasm).unwrap();
    std::fs::write(mods_dir.join("plain.wasm.toml"), "mod_id = \"mod:plain\"\n").unwrap();

    // net allowed this session -> only a mod that *declares* Network may fetch.
    let prefs = ScriptPermissionPrefs {
        log: None,
        document: None,
        net: Some(Permission::Allow),
    };
    let bindings = load_mod_bindings(mere_root.path(), &prefs);

    assert_eq!(
        bindings.len(),
        3,
        "weather's 2 origins + reader's 1; plain adds none"
    );
    let example = bindings
        .iter()
        .find(|b| b.origin == "example.com")
        .expect("example.com bound");
    assert!(
        example.component_path.ends_with("weather.wasm"),
        "the binding points at the mod's own wasm component",
    );
    assert_eq!(
        example.net.effective,
        Permission::Allow,
        "weather declares Network + session allows -> net granted",
    );
    // The reader mod never declared Network, so net is denied even though the
    // session pref is Allow (per-mod least-privilege; the manifest is the ceiling).
    let reader = bindings
        .iter()
        .find(|b| b.origin == "reader.example")
        .expect("reader.example bound");
    assert_eq!(
        reader.net.effective,
        Permission::Deny,
        "a mod without a Network capability cannot fetch regardless of session prefs",
    );
    // A matching navigation routes to the mod's component via the shared lookup.
    assert_eq!(
        binding_for("https://sub.weather.test/x", &bindings).map(|b| b.component_path.clone()),
        Some(example.component_path.clone()),
    );
}

#[test]
fn load_mod_bindings_empty_without_a_mods_dir() {
    let mere_root = tempfile::tempdir().expect("temp mere_root");
    let prefs = ScriptPermissionPrefs::default();
    assert!(
        load_mod_bindings(mere_root.path(), &prefs).is_empty(),
        "no mods/ dir = no mod bindings",
    );
}
