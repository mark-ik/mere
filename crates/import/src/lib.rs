// Copyright 2026 Mark Boykin
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Browser-data import for Mere.
//!
//! Portable models and parsers for ingesting another browser's bookmarks,
//! history, and sessions into Mere. The unit of output is an
//! [`ImportedPageSeed`] (a canonicalised URL + title), wrapped in a
//! [`BrowserImportPayload`] (bookmark / history-visit / session-snapshot)
//! and batched with a [`BrowserImportRun`] that records provenance (source
//! browser family, mode, observation time).
//!
//! This crate is the *ingest* half only: it has no graph dependency and
//! performs no mutation. A consumer maps each page seed onto a kernel
//! address claim and seeds `node-lineage` from history visits / session
//! navigation, but that mapping lives at the call site, not here.
//!
//! Bookmark file parsing covers the two interchange formats browsers
//! export: Chrome/Chromium JSON (`Bookmarks` file) and the legacy
//! Netscape bookmark HTML (`<DL>`/`<DT>`/`<H3>`/`<A>`), detected by
//! [`detect_bookmark_file_format`]. The HTML path uses a small built-in
//! tokenizer rather than a full HTML engine, since the export grammar is
//! narrow and well-defined.

use serde::{Deserialize, Serialize};

pub mod web_clip;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrowserImportPayload {
    Bookmark(ImportedBookmarkItem),
    HistoryVisit(ImportedHistoryVisitItem),
    SessionSnapshot(ImportedBrowserSessionItem),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserImportBatch {
    pub run: BrowserImportRun,
    pub items: Vec<BrowserImportPayload>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserImportRun {
    pub import_id: String,
    pub source: BrowserImportSource,
    pub mode: BrowserImportMode,
    pub observed_at_unix_secs: i64,
    pub user_visible_label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrowserImportMode {
    OneShotFile,
    OneShotProfileRead,
    SnapshotBridge,
    IncrementalBridge,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserImportSource {
    pub browser_family: BrowserFamily,
    pub profile_hint: Option<String>,
    pub source_kind: BrowserImportSourceKind,
    pub stable_source_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrowserFamily {
    Chrome,
    Chromium,
    Edge,
    Brave,
    Arc,
    Firefox,
    Safari,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrowserImportSourceKind {
    BookmarkFile,
    HistoryDatabase,
    SessionFile,
    NativeProfileReader,
    ExtensionBridge,
    NativeMessagingBridge,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportedPageSeed {
    pub canonical_url: String,
    pub normalized_title: Option<String>,
    pub raw_url: Option<String>,
    pub raw_title: Option<String>,
    pub favicon_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportedBookmarkItem {
    pub page: ImportedPageSeed,
    pub bookmark_id: Option<String>,
    pub folder_path: Vec<ImportedFolderSegment>,
    pub location: BookmarkLocation,
    pub created_at_unix_secs: Option<i64>,
    pub modified_at_unix_secs: Option<i64>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportedFolderSegment {
    pub stable_id: Option<String>,
    pub label: String,
    pub position: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BookmarkLocation {
    Toolbar,
    Menu,
    Other,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportedHistoryVisitItem {
    pub page: ImportedPageSeed,
    pub visit_id: Option<String>,
    pub visited_at_unix_secs: i64,
    pub visit_count_hint: Option<u32>,
    pub transition: Option<HistoryTransitionKind>,
    pub referring_url: Option<String>,
    pub session_context: Option<ExternalSessionContext>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HistoryTransitionKind {
    Link,
    Typed,
    AutoBookmark,
    AutoSubframe,
    Reload,
    Redirect,
    Generated,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalSessionContext {
    pub external_window_id: Option<String>,
    pub external_tab_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportedBrowserSessionItem {
    pub snapshot_id: String,
    pub observed_at_unix_secs: i64,
    pub windows: Vec<ImportedBrowserWindow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportedBrowserWindow {
    pub external_window_id: Option<String>,
    pub ordinal: usize,
    pub tabs: Vec<ImportedBrowserTab>,
    pub focused: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportedBrowserTab {
    pub page: ImportedPageSeed,
    pub external_tab_id: Option<String>,
    pub ordinal: usize,
    pub active: bool,
    pub pinned: bool,
    pub audible: bool,
    pub opener_url: Option<String>,
    pub navigation: Vec<ImportedNavigationEntry>,
    pub active_navigation_index: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportedNavigationEntry {
    pub url: String,
    pub title: Option<String>,
    pub ordinal: usize,
    pub visited_at_unix_secs: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BookmarkFileFormat {
    ChromeJson,
    NetscapeHtml,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BookmarkImportError {
    UnsupportedFormat,
    InvalidJson(String),
}

impl std::fmt::Display for BookmarkImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedFormat => write!(f, "unsupported bookmark file format"),
            Self::InvalidJson(message) => write!(f, "invalid bookmark JSON: {message}"),
        }
    }
}

impl std::error::Error for BookmarkImportError {}

pub fn detect_bookmark_file_format(contents: &str) -> Option<BookmarkFileFormat> {
    let trimmed = contents.trim_start();
    if trimmed.starts_with('{') {
        return Some(BookmarkFileFormat::ChromeJson);
    }
    if trimmed.starts_with("<!DOCTYPE NETSCAPE-Bookmark-file-1")
        || trimmed.starts_with("<DL")
        || trimmed.starts_with("<dl")
        || trimmed.contains("NETSCAPE-Bookmark-file-1")
    {
        return Some(BookmarkFileFormat::NetscapeHtml);
    }
    None
}

pub fn parse_bookmark_file_to_batch(
    contents: &str,
    run: BrowserImportRun,
) -> Result<BrowserImportBatch, BookmarkImportError> {
    let items = parse_bookmark_items(contents)?
        .into_iter()
        .map(BrowserImportPayload::Bookmark)
        .collect();
    Ok(BrowserImportBatch { run, items })
}

pub fn parse_bookmark_items(
    contents: &str,
) -> Result<Vec<ImportedBookmarkItem>, BookmarkImportError> {
    match detect_bookmark_file_format(contents) {
        Some(BookmarkFileFormat::ChromeJson) => parse_chrome_bookmark_json(contents),
        Some(BookmarkFileFormat::NetscapeHtml) => Ok(parse_netscape_bookmark_html(contents)),
        None => Err(BookmarkImportError::UnsupportedFormat),
    }
}

mod parser;
use parser::{parse_chrome_bookmark_json, parse_netscape_bookmark_html};

#[cfg(test)]
mod tests;
