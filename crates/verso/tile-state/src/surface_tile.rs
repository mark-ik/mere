/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Surface-tile state — long-lived producer-backed tiles, analog of
//! [`crate::tiles::TileState`] for the [`inker::SurfaceEngine`] dispatch path.
//!
//! Document tiles cache per-URL [`inker::EngineDocument`] values; surface
//! tiles hold a single live [`inker::SurfaceProducer`] that owns the
//! underlying WebView control for the tile's whole lifetime. The tile-side
//! history vec still exists so the tile-strip switcher can label entries by
//! URL, but the producer is the source of truth for back/forward navigation.
//!
//! Not `Send`: surface producers may be STA-bound (Windows WebView2) or
//! main-thread-only (macOS WKWebView). The host drives each tile from a
//! single thread per tile.

use std::time::SystemTime;

use inker::{
    CursorShape, NavigationEvent, SurfaceError, SurfaceFrame, SurfaceProducer, WebMessage,
};

use crate::tiles::HistoryEntry;

/// Per-tile mutable state for a surface tile.
pub struct SurfaceTileState {
    /// Engine ID that produced this tile's producer (e.g. `"scrying.web"`).
    /// Mirrors [`inker::SurfaceEngine::engine_id`] of the registered engine.
    pub engine_id: String,
    producer: Box<dyn SurfaceProducer>,
    /// Within-tile navigation history. Same shape as `TileState::history` so
    /// the tile-strip switcher can treat both flavors uniformly.
    pub history: Vec<HistoryEntry>,
    pub history_cursor: usize,
    last_frame: Option<SurfaceFrame>,
    /// Last cursor shape the producer asked the host to display.
    last_cursor: CursorShape,
    /// Wall-clock of the most recent successful `acquire_frame`. `None` if
    /// no frame has landed yet.
    last_frame_at: Option<SystemTime>,
}

/// Outcome of one [`SurfaceTileState::step`] tick. The host re-renders /
/// dispatches based on the flags + drained events.
#[derive(Default)]
pub struct SurfaceTileStep {
    /// `true` when `acquire_frame` produced a new frame this tick.
    pub new_frame: bool,
    /// Drained navigation events from the producer this tick.
    pub nav_events: Vec<NavigationEvent>,
    /// Most recent cursor shape the producer asked for (latest wins).
    pub cursor: Option<CursorShape>,
    /// JS-side messages drained from the producer this tick.
    pub messages: Vec<WebMessage>,
}

impl SurfaceTileState {
    /// Wrap a freshly-spawned producer as a surface tile, seeded with the
    /// anchor URL as the first history entry.
    pub fn spawned(
        engine_id: impl Into<String>,
        producer: Box<dyn SurfaceProducer>,
        anchor_url: String,
    ) -> Self {
        Self {
            engine_id: engine_id.into(),
            producer,
            history: vec![HistoryEntry::new(anchor_url)],
            history_cursor: 0,
            last_frame: None,
            last_cursor: CursorShape::Default,
            last_frame_at: None,
        }
    }

    pub fn current(&self) -> Option<&HistoryEntry> {
        self.history.get(self.history_cursor)
    }

    pub fn current_url(&self) -> Option<&str> {
        self.current().map(|e| e.url.as_str())
    }

    pub fn last_frame(&self) -> Option<&SurfaceFrame> {
        self.last_frame.as_ref()
    }

    pub fn last_cursor(&self) -> CursorShape {
        self.last_cursor
    }

    pub fn last_frame_at(&self) -> Option<SystemTime> {
        self.last_frame_at
    }

    pub fn can_go_back(&self) -> bool {
        self.history_cursor > 0
    }

    pub fn can_go_forward(&self) -> bool {
        self.history_cursor + 1 < self.history.len()
    }

    /// Tell the producer to navigate to `url`, recording it in the
    /// within-tile history. If the URL matches the current entry this is
    /// treated as a reload (no history mutation, producer.reload() instead).
    pub fn navigate_to(&mut self, url: &str) -> Result<(), SurfaceError> {
        if self.current_url() == Some(url) {
            return self.producer.reload();
        }
        self.producer.navigate_to_url(url)?;
        self.push_history(url.to_string());
        Ok(())
    }

    pub fn reload(&mut self) -> Result<(), SurfaceError> {
        self.producer.reload()
    }

    pub fn stop(&mut self) -> Result<(), SurfaceError> {
        self.producer.stop()
    }

    /// Walk back in history. Drives the producer's own back stack so its
    /// view follows; falls back to the producer's `Err` if it disagrees on
    /// whether back is possible. Returns the new current URL on success.
    pub fn go_back(&mut self) -> Result<Option<String>, SurfaceError> {
        if !self.can_go_back() {
            return Ok(None);
        }
        self.producer.go_back()?;
        self.history_cursor -= 1;
        Ok(self.current_url().map(str::to_owned))
    }

    pub fn go_forward(&mut self) -> Result<Option<String>, SurfaceError> {
        if !self.can_go_forward() {
            return Ok(None);
        }
        self.producer.go_forward()?;
        self.history_cursor += 1;
        Ok(self.current_url().map(str::to_owned))
    }

    pub fn resize(&mut self, width: u32, height: u32) -> Result<(), SurfaceError> {
        self.producer.resize(width, height)
    }

    pub fn set_offset(&mut self, x: i32, y: i32) -> Result<(), SurfaceError> {
        self.producer.set_offset(x, y)
    }

    /// Borrow the underlying producer for input forwarding and other
    /// methods the surface-tile vocabulary doesn't expose directly.
    pub fn producer_mut(&mut self) -> &mut dyn SurfaceProducer {
        &mut *self.producer
    }

    /// One host-tick poll: drain frames + nav events + cursor shape + JS
    /// messages from the producer and fold them into the tile state.
    ///
    /// Producer-side commits update the within-tile history so the
    /// tile-strip switcher's URL labels follow link clicks, redirects, and
    /// `pushState` / `replaceState` without the host having to listen
    /// separately.
    pub fn step(&mut self) -> Result<SurfaceTileStep, SurfaceError> {
        let mut step = SurfaceTileStep::default();

        if let Some(frame) = self.producer.acquire_frame()? {
            self.last_frame = Some(frame);
            self.last_frame_at = Some(SystemTime::now());
            step.new_frame = true;
        }

        while let Some(ev) = self.producer.poll_navigation_event() {
            if let NavigationEvent::Committed { url } = &ev {
                if self.current_url() != Some(url.as_str()) {
                    self.push_history(url.clone());
                }
            }
            step.nav_events.push(ev);
        }

        while let Some(shape) = self.producer.poll_cursor_shape() {
            self.last_cursor = shape;
            step.cursor = Some(shape);
        }

        while let Some(msg) = self.producer.poll_web_message() {
            step.messages.push(msg);
        }

        Ok(step)
    }

    fn push_history(&mut self, url: String) {
        // Truncate any forward branch first, matching browser semantics:
        // "go back, then navigate somewhere new" loses the old forward path.
        if self.history_cursor + 1 < self.history.len() {
            self.history.truncate(self.history_cursor + 1);
        }
        self.history.push(HistoryEntry::new(url));
        self.history_cursor = self.history.len() - 1;
    }
}

// Surface-tile lookup methods live on [`crate::TileManager`] itself —
// see `tiles.rs` for the parallel `surface: HashMap<TileId, SurfaceTileState>`
// map and the `*_surface*` accessors.

#[cfg(test)]
mod tests {
    use super::*;
    use inker::{FocusReason, KeyboardEvent, MouseEvent, PointerEvent, SurfaceSettings};
    use std::collections::VecDeque;

    /// Driveable producer mock. The test populates the queues; the
    /// `SurfaceTileState` drains them through its trait methods.
    #[derive(Default)]
    struct MockProducer {
        nav_events: VecDeque<NavigationEvent>,
        cursor_shapes: VecDeque<CursorShape>,
        messages: VecDeque<WebMessage>,
        next_frame: Option<SurfaceFrame>,
        last_url: Option<String>,
        last_reload: bool,
        back_calls: u32,
        forward_calls: u32,
        back_available: bool,
        forward_available: bool,
    }

    impl SurfaceProducer for MockProducer {
        fn resize(&mut self, _: u32, _: u32) -> Result<(), SurfaceError> {
            Ok(())
        }
        fn set_offset(&mut self, _: i32, _: i32) -> Result<(), SurfaceError> {
            Ok(())
        }
        fn acquire_frame(&mut self) -> Result<Option<SurfaceFrame>, SurfaceError> {
            Ok(self.next_frame.take())
        }
        fn navigate_to_url(&mut self, url: &str) -> Result<(), SurfaceError> {
            self.last_url = Some(url.to_string());
            Ok(())
        }
        fn navigate_to_string(&mut self, _: &str) -> Result<(), SurfaceError> {
            Ok(())
        }
        fn reload(&mut self) -> Result<(), SurfaceError> {
            self.last_reload = true;
            Ok(())
        }
        fn stop(&mut self) -> Result<(), SurfaceError> {
            Ok(())
        }
        fn go_back(&mut self) -> Result<(), SurfaceError> {
            self.back_calls += 1;
            Ok(())
        }
        fn go_forward(&mut self) -> Result<(), SurfaceError> {
            self.forward_calls += 1;
            Ok(())
        }
        fn can_go_back(&self) -> bool {
            self.back_available
        }
        fn can_go_forward(&self) -> bool {
            self.forward_available
        }
        fn send_mouse_input(&mut self, _: MouseEvent) -> Result<(), SurfaceError> {
            Ok(())
        }
        fn send_pointer_input(&mut self, _: PointerEvent) -> Result<(), SurfaceError> {
            Ok(())
        }
        fn send_keyboard_input(&mut self, _: KeyboardEvent) -> Result<(), SurfaceError> {
            Ok(())
        }
        fn move_focus(&mut self, _: FocusReason) -> Result<(), SurfaceError> {
            Ok(())
        }
        fn poll_navigation_event(&mut self) -> Option<NavigationEvent> {
            self.nav_events.pop_front()
        }
        fn poll_cursor_shape(&mut self) -> Option<CursorShape> {
            self.cursor_shapes.pop_front()
        }
        fn poll_web_message(&mut self) -> Option<WebMessage> {
            self.messages.pop_front()
        }
        fn apply_settings(&mut self, _: &SurfaceSettings) -> Result<(), SurfaceError> {
            Ok(())
        }
        fn capture_snapshot_png(&mut self) -> Result<Vec<u8>, SurfaceError> {
            Err(SurfaceError::Unsupported("test".into()))
        }
    }

    fn spawn_tile() -> (SurfaceTileState, *mut MockProducer) {
        let mut producer = Box::new(MockProducer::default());
        let raw: *mut MockProducer = &mut *producer;
        let tile = SurfaceTileState::spawned(
            "scrying.web",
            producer as Box<dyn SurfaceProducer>,
            "https://a.example".into(),
        );
        (tile, raw)
    }

    #[test]
    fn seeded_history_starts_at_anchor() {
        let (tile, _) = spawn_tile();
        assert_eq!(tile.engine_id, "scrying.web");
        assert_eq!(tile.history.len(), 1);
        assert_eq!(tile.current_url(), Some("https://a.example"));
        assert!(!tile.can_go_back());
        assert!(!tile.can_go_forward());
    }

    #[test]
    fn navigate_to_pushes_history_and_calls_producer() {
        let (mut tile, raw) = spawn_tile();
        tile.navigate_to("https://b.example").expect("nav");
        assert_eq!(tile.history.len(), 2);
        assert_eq!(tile.current_url(), Some("https://b.example"));
        // Producer received the navigate call.
        unsafe { assert_eq!((*raw).last_url.as_deref(), Some("https://b.example")) };
    }

    #[test]
    fn navigate_to_same_url_reloads_without_history_push() {
        let (mut tile, raw) = spawn_tile();
        tile.navigate_to("https://a.example").expect("nav");
        assert_eq!(tile.history.len(), 1);
        unsafe { assert!((*raw).last_reload) };
    }

    #[test]
    fn go_back_walks_history_and_calls_producer() {
        let (mut tile, raw) = spawn_tile();
        tile.navigate_to("https://b.example").expect("nav");
        tile.navigate_to("https://c.example").expect("nav");
        let prev = tile.go_back().expect("ok").expect("some");
        assert_eq!(prev, "https://b.example");
        assert_eq!(tile.history_cursor, 1);
        unsafe { assert_eq!((*raw).back_calls, 1) };
    }

    #[test]
    fn navigate_after_back_truncates_forward_branch() {
        let (mut tile, _) = spawn_tile();
        tile.navigate_to("b").unwrap();
        tile.navigate_to("c").unwrap();
        tile.go_back().unwrap();
        tile.navigate_to("d").unwrap();
        let urls: Vec<&str> = tile.history.iter().map(|e| e.url.as_str()).collect();
        assert_eq!(urls, vec!["https://a.example", "b", "d"]);
        assert_eq!(tile.history_cursor, 2);
    }

    #[test]
    fn step_drains_nav_events_and_updates_history_on_committed() {
        let (mut tile, raw) = spawn_tile();
        unsafe {
            (*raw).nav_events.push_back(NavigationEvent::Started {
                url: "https://b.example".into(),
            });
            (*raw).nav_events.push_back(NavigationEvent::Committed {
                url: "https://b.example".into(),
            });
        }
        let step = tile.step().expect("step");
        assert_eq!(step.nav_events.len(), 2);
        // Committed URL pushed onto the history because the producer drove
        // a producer-side navigation (link click / redirect).
        assert_eq!(tile.current_url(), Some("https://b.example"));
        assert_eq!(tile.history.len(), 2);
    }

    #[test]
    fn step_drains_cursor_and_messages() {
        let (mut tile, raw) = spawn_tile();
        unsafe {
            (*raw).cursor_shapes.push_back(CursorShape::Text);
            (*raw).cursor_shapes.push_back(CursorShape::Pointer);
            (*raw).messages.push_back(WebMessage {
                tag: String::new(),
                payload: "hello".into(),
            });
        }
        let step = tile.step().expect("step");
        // Latest cursor wins; both messages drained.
        assert_eq!(step.cursor, Some(CursorShape::Pointer));
        assert_eq!(tile.last_cursor(), CursorShape::Pointer);
        assert_eq!(step.messages.len(), 1);
    }
}
