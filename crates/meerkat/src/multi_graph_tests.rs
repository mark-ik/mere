/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */
    use super::*;

    fn temp_root(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "mere-mg-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn seeds_one_session_on_a_fresh_root() {
        let root = temp_root("seed");
        let (store, active) = bootstrap_sessions(&root);
        assert_eq!(store.len(), 1);
        assert!(store.get(active).is_some());
        let manifest = root
            .join("sessions")
            .join(active.as_uuid().to_string())
            .join(session_runtime::MANIFEST_FILE);
        assert!(manifest.exists(), "seeded session manifest written to disk");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn migrates_a_flat_graph_into_a_session() {
        let root = temp_root("migrate");
        // A pre-MG1 flat layout: graph.json + frame.json + views/ at the root.
        std::fs::write(
            root.join(session_graph_store::GRAPH_FILE),
            br#"{"flat":true}"#,
        )
        .unwrap();
        std::fs::write(root.join(frame_layout_store::FRAME_FILE), b"{}").unwrap();
        let flat_views = root.join(view_intent_store::VIEW_INTENT_DIR);
        std::fs::create_dir_all(&flat_views).unwrap();
        std::fs::write(flat_views.join("pane.json"), b"{}").unwrap();

        let (store, active) = bootstrap_sessions(&root);
        assert_eq!(store.len(), 1);
        let session_dir = root.join("sessions").join(active.as_uuid().to_string());
        // The session-scoped artefacts (graph + views) moved into the session dir,
        // and the bytes survived.
        assert!(session_dir.join(session_graph_store::GRAPH_FILE).exists());
        assert!(
            session_dir
                .join(view_intent_store::VIEW_INTENT_DIR)
                .join("pane.json")
                .exists()
        );
        assert!(
            !root.join(session_graph_store::GRAPH_FILE).exists(),
            "the flat graph was moved, not copied"
        );
        // The frame layout is window-scoped (MG5): it stays at the root, not the
        // session dir.
        assert!(
            root.join(frame_layout_store::FRAME_FILE).exists(),
            "the window-scoped frame stays at the root"
        );
        assert!(
            !session_dir.join(frame_layout_store::FRAME_FILE).exists(),
            "the frame is not pulled into the session"
        );
        let moved =
            std::fs::read_to_string(session_dir.join(session_graph_store::GRAPH_FILE)).unwrap();
        assert!(moved.contains("flat"), "no graph lost in the migration");
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn reuses_an_existing_session_instead_of_seeding() {
        let root = temp_root("reuse");
        let (_first, first) = bootstrap_sessions(&root);
        let (store, active) = bootstrap_sessions(&root);
        assert_eq!(store.len(), 1, "no duplicate session seeded");
        assert_eq!(active, first, "the existing session is reopened as active");
        std::fs::remove_dir_all(&root).ok();
    }
