// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

use super::*;

fn sample_run() -> BrowserImportRun {
    BrowserImportRun {
        import_id: "import-1".to_string(),
        source: BrowserImportSource {
            browser_family: BrowserFamily::Chrome,
            profile_hint: Some("Default".to_string()),
            source_kind: BrowserImportSourceKind::BookmarkFile,
            stable_source_id: Some("chrome:default".to_string()),
        },
        mode: BrowserImportMode::OneShotFile,
        observed_at_unix_secs: 1_744_329_600,
        user_visible_label: "Chrome bookmarks".to_string(),
    }
}

#[test]
fn detects_chrome_json_bookmark_file() {
    assert_eq!(
        detect_bookmark_file_format(" { \"roots\": {} } "),
        Some(BookmarkFileFormat::ChromeJson)
    );
}

#[test]
fn parses_chrome_json_bookmarks_into_folder_paths() {
    let json = r#"
        {
          "roots": {
            "bookmark_bar": {
              "children": [
                {
                  "type": "folder",
                  "name": "Rust",
                  "id": "10",
                  "children": [
                    {
                      "type": "url",
                      "id": "11",
                      "name": "Docs",
                      "url": "https://docs.rs"
                    }
                  ]
                }
              ],
              "name": "Bookmarks Bar",
              "type": "folder"
            }
          }
        }
        "#;

    let batch = parse_bookmark_file_to_batch(json, sample_run()).unwrap();
    assert_eq!(batch.items.len(), 1);
    let BrowserImportPayload::Bookmark(item) = &batch.items[0] else {
        panic!("expected bookmark payload");
    };
    assert_eq!(item.page.canonical_url, "https://docs.rs/");
    assert_eq!(item.location, BookmarkLocation::Toolbar);
    assert_eq!(item.folder_path.len(), 1);
    assert_eq!(item.folder_path[0].label, "Rust");
}

#[test]
fn parses_netscape_bookmarks_into_nested_folder_paths() {
    let html = r#"
        <!DOCTYPE NETSCAPE-Bookmark-file-1>
        <DL><p>
          <DT><H3>Research</H3>
          <DL><p>
            <DT><A HREF="https://example.com/article">Example Article</A>
          </DL><p>
        </DL><p>
        "#;

    let items = parse_bookmark_items(html).unwrap();
    assert_eq!(items.len(), 1);
    let item = &items[0];
    assert_eq!(item.page.canonical_url, "https://example.com/article");
    assert_eq!(item.folder_path.len(), 1);
    assert_eq!(item.folder_path[0].label, "Research");
    assert_eq!(
        item.page.normalized_title.as_deref(),
        Some("Example Article")
    );
}
