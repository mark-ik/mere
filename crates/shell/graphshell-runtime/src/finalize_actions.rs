/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Portable queued payloads consumed by runtime finalize-action helpers.
//!
//! These shapes intentionally exclude app-owned side effects such as audit-log
//! persistence. The heavy `graphshell` crate may wrap them with additional
//! metadata, but the payloads themselves are host-neutral and cheap to share.

use crate::ports::{RuntimeClipboardPort, RuntimeToastPort};
use graphshell_core::graph::NodeKey;
use graphshell_core::shell_state::frame_model::ToastSeverity;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardCopyKind {
    Url,
    Title,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClipboardCopyRequest {
    pub key: NodeKey,
    pub kind: ClipboardCopyKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardCopySource {
    pub visible_url: String,
    pub visible_title: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiNotificationLevel {
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeStatusNotice {
    pub key: NodeKey,
    pub level: UiNotificationLevel,
    pub message: String,
}

pub const CLIPBOARD_STATUS_SUCCESS_URL_TEXT: &str = "Copied URL";
pub const CLIPBOARD_STATUS_SUCCESS_TITLE_TEXT: &str = "Copied title";
pub const CLIPBOARD_STATUS_UNAVAILABLE_TEXT: &str = "Clipboard unavailable";
pub const CLIPBOARD_STATUS_EMPTY_TEXT: &str = "Nothing to copy";
pub const CLIPBOARD_STATUS_FAILURE_PREFIX: &str = "Copy failed";
pub const CLIPBOARD_STATUS_MISSING_NODE_SUGGESTION_TEXT: &str = "select a node and try again";

pub trait RuntimeClipboardCopyState {
    fn take_pending_clipboard_copy(&mut self) -> Option<ClipboardCopyRequest>;

    fn clipboard_copy_source(&self, key: NodeKey) -> Option<ClipboardCopySource>;
}

pub fn drain_pending_clipboard_copy_requests<S, P, F>(
    state: &mut S,
    ports: &mut P,
    mut on_write_failure: F,
) where
    S: RuntimeClipboardCopyState + ?Sized,
    P: RuntimeClipboardPort + RuntimeToastPort + ?Sized,
    F: FnMut(usize),
{
    while let Some(ClipboardCopyRequest { key, kind }) = state.take_pending_clipboard_copy() {
        handle_pending_clipboard_copy_request(state, ports, key, kind, &mut on_write_failure);
    }
}

/// Host-neutral queue/audit seam for runtime-owned node-status notices.
pub trait RuntimeNodeStatusNoticeState {
    type AuditEvent;

    fn take_pending_node_status_notice(
        &mut self,
    ) -> Option<(NodeStatusNotice, Option<Self::AuditEvent>)>;

    fn log_node_status_audit_event(&mut self, key: NodeKey, event: Self::AuditEvent);
}

pub fn drain_pending_node_status_notices<S, P>(state: &mut S, port: &mut P)
where
    S: RuntimeNodeStatusNoticeState + ?Sized,
    P: RuntimeToastPort + ?Sized,
{
    while let Some((notice, audit_event)) = state.take_pending_node_status_notice() {
        emit_node_status_toast(port, notice.level, &notice.message);

        let Some(event) = audit_event else {
            continue;
        };
        state.log_node_status_audit_event(notice.key, event);
    }
}

pub fn port_error<P>(port: &mut P, message: impl Into<String>)
where
    P: RuntimeToastPort + ?Sized,
{
    port.enqueue_message(ToastSeverity::Error, message, None);
}

pub fn emit_node_status_toast<P>(port: &mut P, level: UiNotificationLevel, message: &str)
where
    P: RuntimeToastPort + ?Sized,
{
    let severity = match level {
        UiNotificationLevel::Success => ToastSeverity::Success,
        UiNotificationLevel::Warning => ToastSeverity::Warning,
        UiNotificationLevel::Error => ToastSeverity::Error,
    };
    port.enqueue_message(severity, message.to_string(), None);
}

fn handle_pending_clipboard_copy_request<S, P, F>(
    state: &S,
    ports: &mut P,
    key: NodeKey,
    kind: ClipboardCopyKind,
    on_write_failure: &mut F,
) where
    S: RuntimeClipboardCopyState + ?Sized,
    P: RuntimeClipboardPort + RuntimeToastPort + ?Sized,
    F: FnMut(usize),
{
    let Some(value) = clipboard_copy_value_for_node(state, key, kind, ports) else {
        return;
    };

    match ports.set_text(&value) {
        Ok(()) => emit_clipboard_copy_success_toast(ports, kind),
        Err(error) => {
            on_write_failure(error.len());
            let failure_text = if error == "clipboard unavailable" {
                CLIPBOARD_STATUS_UNAVAILABLE_TEXT.to_string()
            } else {
                clipboard_copy_failure_text(&error)
            };
            port_error(ports, failure_text);
        }
    }
}

fn clipboard_copy_value_for_node<S, P>(
    state: &S,
    key: NodeKey,
    kind: ClipboardCopyKind,
    port: &mut P,
) -> Option<String>
where
    S: RuntimeClipboardCopyState + ?Sized,
    P: RuntimeToastPort + ?Sized,
{
    let Some(ClipboardCopySource {
        visible_url,
        visible_title,
    }) = state.clipboard_copy_source(key)
    else {
        port_error(port, clipboard_copy_missing_node_failure_text());
        return None;
    };

    let value = match kind {
        ClipboardCopyKind::Url => visible_url,
        ClipboardCopyKind::Title => {
            clipboard_title_or_url(visible_title.as_str(), visible_url.as_str())
        }
    };

    if value.trim().is_empty() {
        port.enqueue_message(ToastSeverity::Warning, CLIPBOARD_STATUS_EMPTY_TEXT, None);
        return None;
    }

    Some(value)
}

fn clipboard_title_or_url(title: &str, url: &str) -> String {
    if title.is_empty() {
        url.to_owned()
    } else {
        title.to_owned()
    }
}

fn emit_clipboard_copy_success_toast<P>(port: &mut P, kind: ClipboardCopyKind)
where
    P: RuntimeToastPort + ?Sized,
{
    port.enqueue_message(
        ToastSeverity::Success,
        clipboard_copy_success_text(kind).to_string(),
        None,
    );
}

pub fn clipboard_copy_success_text(kind: ClipboardCopyKind) -> &'static str {
    match kind {
        ClipboardCopyKind::Url => CLIPBOARD_STATUS_SUCCESS_URL_TEXT,
        ClipboardCopyKind::Title => CLIPBOARD_STATUS_SUCCESS_TITLE_TEXT,
    }
}

pub fn clipboard_copy_failure_text(detail: &str) -> String {
    format!("{CLIPBOARD_STATUS_FAILURE_PREFIX}: {detail}")
}

pub fn clipboard_copy_missing_node_failure_text() -> String {
    clipboard_copy_failure_text(
        format!("node no longer exists; {CLIPBOARD_STATUS_MISSING_NODE_SUGGESTION_TEXT}").as_str(),
    )
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use graphshell_core::shell_state::frame_model::ToastSpec;

    use super::*;

    #[derive(Default)]
    struct TestToastPort {
        toasts: Vec<ToastSpec>,
    }

    impl RuntimeToastPort for TestToastPort {
        fn enqueue(&mut self, toast: ToastSpec) {
            self.toasts.push(toast);
        }
    }

    #[derive(Default)]
    struct TestClipboardPort {
        text: Option<String>,
        failure: Option<String>,
    }

    impl RuntimeClipboardPort for TestClipboardPort {
        fn get_text(&mut self) -> Option<String> {
            self.text.clone()
        }

        fn set_text(&mut self, text: &str) -> Result<(), String> {
            if let Some(error) = &self.failure {
                return Err(error.clone());
            }
            self.text = Some(text.to_string());
            Ok(())
        }
    }

    #[derive(Default)]
    struct TestPorts {
        clipboard: TestClipboardPort,
        toasts: TestToastPort,
    }

    impl RuntimeClipboardPort for TestPorts {
        fn get_text(&mut self) -> Option<String> {
            self.clipboard.get_text()
        }

        fn set_text(&mut self, text: &str) -> Result<(), String> {
            self.clipboard.set_text(text)
        }
    }

    impl RuntimeToastPort for TestPorts {
        fn enqueue(&mut self, toast: ToastSpec) {
            self.toasts.enqueue(toast);
        }
    }

    #[derive(Default)]
    struct TestNoticeState {
        pending: VecDeque<(NodeStatusNotice, Option<&'static str>)>,
        logged: Vec<(NodeKey, &'static str)>,
    }

    impl RuntimeNodeStatusNoticeState for TestNoticeState {
        type AuditEvent = &'static str;

        fn take_pending_node_status_notice(
            &mut self,
        ) -> Option<(NodeStatusNotice, Option<Self::AuditEvent>)> {
            self.pending.pop_front()
        }

        fn log_node_status_audit_event(&mut self, key: NodeKey, event: Self::AuditEvent) {
            self.logged.push((key, event));
        }
    }

    #[derive(Default)]
    struct TestClipboardState {
        pending: VecDeque<ClipboardCopyRequest>,
        source: Option<ClipboardCopySource>,
    }

    impl RuntimeClipboardCopyState for TestClipboardState {
        fn take_pending_clipboard_copy(&mut self) -> Option<ClipboardCopyRequest> {
            self.pending.pop_front()
        }

        fn clipboard_copy_source(&self, _key: NodeKey) -> Option<ClipboardCopySource> {
            self.source.clone()
        }
    }

    #[test]
    fn drain_pending_node_status_notices_emits_toasts_and_logs_audit_events() {
        let key = NodeKey::new(7);
        let mut state = TestNoticeState {
            pending: VecDeque::from([
                (
                    NodeStatusNotice {
                        key,
                        level: UiNotificationLevel::Success,
                        message: "done".to_string(),
                    },
                    Some("audit"),
                ),
                (
                    NodeStatusNotice {
                        key,
                        level: UiNotificationLevel::Warning,
                        message: "careful".to_string(),
                    },
                    None,
                ),
            ]),
            logged: Vec::new(),
        };
        let mut port = TestToastPort::default();

        drain_pending_node_status_notices(&mut state, &mut port);

        assert_eq!(port.toasts.len(), 2);
        assert_eq!(port.toasts[0].severity, ToastSeverity::Success);
        assert_eq!(port.toasts[0].message, "done");
        assert_eq!(port.toasts[1].severity, ToastSeverity::Warning);
        assert_eq!(port.toasts[1].message, "careful");
        assert_eq!(state.logged, vec![(key, "audit")]);
        assert!(state.pending.is_empty());
    }

    #[test]
    fn port_error_uses_error_severity() {
        let mut port = TestToastPort::default();

        port_error(&mut port, "broken");

        assert_eq!(port.toasts.len(), 1);
        assert_eq!(port.toasts[0].severity, ToastSeverity::Error);
        assert_eq!(port.toasts[0].message, "broken");
    }

    #[test]
    fn drain_pending_clipboard_copy_requests_copies_value_and_emits_success_toast() {
        let key = NodeKey::new(9);
        let mut state = TestClipboardState {
            pending: VecDeque::from([ClipboardCopyRequest {
                key,
                kind: ClipboardCopyKind::Title,
            }]),
            source: Some(ClipboardCopySource {
                visible_url: "https://example.test".to_string(),
                visible_title: "Example".to_string(),
            }),
        };
        let mut ports = TestPorts::default();
        let mut reported_failures = Vec::new();

        drain_pending_clipboard_copy_requests(&mut state, &mut ports, |byte_len| {
            reported_failures.push(byte_len)
        });

        assert_eq!(ports.clipboard.text.as_deref(), Some("Example"));
        assert_eq!(ports.toasts.toasts.len(), 1);
        assert_eq!(ports.toasts.toasts[0].severity, ToastSeverity::Success);
        assert_eq!(
            ports.toasts.toasts[0].message,
            CLIPBOARD_STATUS_SUCCESS_TITLE_TEXT
        );
        assert!(reported_failures.is_empty());
        assert!(state.pending.is_empty());
    }

    #[test]
    fn drain_pending_clipboard_copy_requests_reports_missing_node_explicitly() {
        let key = NodeKey::new(11);
        let mut state = TestClipboardState {
            pending: VecDeque::from([ClipboardCopyRequest {
                key,
                kind: ClipboardCopyKind::Url,
            }]),
            source: None,
        };
        let mut ports = TestPorts::default();

        drain_pending_clipboard_copy_requests(&mut state, &mut ports, |_| unreachable!());

        assert_eq!(ports.clipboard.text, None);
        assert_eq!(ports.toasts.toasts.len(), 1);
        assert_eq!(ports.toasts.toasts[0].severity, ToastSeverity::Error);
        assert!(
            ports.toasts.toasts[0]
                .message
                .contains("node no longer exists")
        );
    }

    #[test]
    fn drain_pending_clipboard_copy_requests_reports_clipboard_write_failure() {
        let key = NodeKey::new(13);
        let mut state = TestClipboardState {
            pending: VecDeque::from([ClipboardCopyRequest {
                key,
                kind: ClipboardCopyKind::Url,
            }]),
            source: Some(ClipboardCopySource {
                visible_url: "https://example.test".to_string(),
                visible_title: "".to_string(),
            }),
        };
        let mut ports = TestPorts::default();
        ports.clipboard.failure = Some("clipboard unavailable".to_string());
        let mut reported_failures = Vec::new();

        drain_pending_clipboard_copy_requests(&mut state, &mut ports, |byte_len| {
            reported_failures.push(byte_len)
        });

        assert_eq!(reported_failures, vec!["clipboard unavailable".len()]);
        assert_eq!(ports.toasts.toasts.len(), 1);
        assert_eq!(ports.toasts.toasts[0].severity, ToastSeverity::Error);
        assert_eq!(
            ports.toasts.toasts[0].message,
            CLIPBOARD_STATUS_UNAVAILABLE_TEXT
        );
    }
}
