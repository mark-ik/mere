/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Initial diagnostics registry: channels + invariants seeded at startup.

use super::*;

pub(crate) fn initial_registry() -> DiagnosticsRegistry {
    let mut registry = DiagnosticsRegistry::default();
    for (channel, severity, description) in INITIAL_CHANNELS {
        let descriptor = match severity {
            Severity::Info => RuntimeChannelDescriptor::info(
                *channel,
                1,
                DiagnosticsChannelOwner::runtime(),
                Some((*description).to_string()),
            ),
            Severity::Warn => RuntimeChannelDescriptor::warn(
                *channel,
                1,
                DiagnosticsChannelOwner::runtime(),
                Some((*description).to_string()),
            ),
            Severity::Error => RuntimeChannelDescriptor::error(
                *channel,
                1,
                DiagnosticsChannelOwner::runtime(),
                Some((*description).to_string()),
            ),
        };
        let _ =
            registry.register_runtime_channel(descriptor, ChannelRegistrationPolicy::KeepExisting);
    }
    for invariant in INITIAL_INVARIANTS {
        let _ = registry.register_invariant(
            DiagnosticsInvariant {
                invariant_id: invariant.id.to_string(),
                start_channel: invariant.start.to_string(),
                terminal_channels: invariant
                    .terminal
                    .iter()
                    .map(|channel| (*channel).to_string())
                    .collect(),
                timeout_ms: invariant.timeout_ms,
                owner: DiagnosticsChannelOwner::runtime(),
                enabled: true,
            },
            &[DiagnosticsCapability::RegisterInvariants],
        );
    }
    registry
}

struct InitialInvariant {
    id: &'static str,
    start: &'static str,
    terminal: &'static [&'static str],
    timeout_ms: u64,
}

const INITIAL_CHANNELS: &[(&str, Severity, &str)] = &[
    (
        "meerkat.startup.started",
        Severity::Info,
        "Meerkat startup began",
    ),
    (
        "meerkat.startup.succeeded",
        Severity::Info,
        "Meerkat startup completed",
    ),
    (
        "meerkat.startup.failed",
        Severity::Error,
        "Meerkat startup failed",
    ),
    (
        "meerkat.frame.layout_changed",
        Severity::Info,
        "Frame layout changed",
    ),
    (
        "meerkat.frame.pane_summoned",
        Severity::Info,
        "Frame pane opened",
    ),
    (
        "meerkat.frame.pane_closed",
        Severity::Info,
        "Frame pane closed",
    ),
    (
        "meerkat.frame.divider_dragged",
        Severity::Info,
        "Frame divider moved",
    ),
    (
        "meerkat.ui.action_dispatched",
        Severity::Info,
        "UI action dispatched",
    ),
    (
        "meerkat.ui.surface_opened",
        Severity::Info,
        "UI surface opened",
    ),
    (
        "meerkat.ui.surface_dismissed",
        Severity::Info,
        "UI surface dismissed",
    ),
    (
        "meerkat.ui.focus_changed",
        Severity::Info,
        "UI focus changed",
    ),
    ("meerkat.theme.activated", Severity::Info, "Theme activated"),
    (
        "meerkat.actor.fetch.started",
        Severity::Info,
        "Fetch actor work started",
    ),
    (
        "meerkat.actor.fetch.succeeded",
        Severity::Info,
        "Fetch actor work succeeded",
    ),
    (
        "meerkat.actor.fetch.failed",
        Severity::Warn,
        "Fetch actor work failed",
    ),
    (
        "meerkat.actor.fetch.subresource",
        Severity::Info,
        "Fetch actor subresource completed",
    ),
    (
        "meerkat.actor.sync.status",
        Severity::Info,
        "Sync actor status changed",
    ),
    (
        "meerkat.actor.sync.started",
        Severity::Info,
        "Sync actor work started",
    ),
    (
        "meerkat.actor.sync.succeeded",
        Severity::Info,
        "Sync actor work succeeded",
    ),
    (
        "meerkat.actor.sync.failed",
        Severity::Warn,
        "Sync actor work failed",
    ),
    (
        "meerkat.actor.comms.inbox",
        Severity::Info,
        "Comms inbox updated",
    ),
    (
        "meerkat.actor.comms.thread",
        Severity::Info,
        "Comms thread updated",
    ),
    (
        "meerkat.actor.comms.sent",
        Severity::Info,
        "Comms draft sent",
    ),
    (
        "meerkat.actor.comms.send_outcome",
        Severity::Info,
        "Comms send outcome",
    ),
    (
        "meerkat.actor.comms.identity",
        Severity::Info,
        "Comms identity updated",
    ),
    (
        "meerkat.actor.comms.started",
        Severity::Info,
        "Comms actor work started",
    ),
    (
        "meerkat.actor.comms.succeeded",
        Severity::Info,
        "Comms actor work succeeded",
    ),
    (
        "meerkat.actor.comms.failed",
        Severity::Warn,
        "Comms actor work failed",
    ),
    (
        "meerkat.actor.content.respawned",
        Severity::Warn,
        "Content actor respawned",
    ),
    (
        "meerkat.actor.content.stopped",
        Severity::Info,
        "Content operation stopped",
    ),
    (
        "meerkat.actor.content.pinned",
        Severity::Info,
        "Content operation pinned",
    ),
    (
        "meerkat.actor.content.failed",
        Severity::Warn,
        "Content actor failed",
    ),
    (
        "meerkat.a11y.tree_built",
        Severity::Info,
        "Accessibility summary built",
    ),
    (
        "meerkat.a11y.bounds_missing",
        Severity::Warn,
        "Accessibility bounds missing",
    ),
    (
        "meerkat.a11y.label_missing",
        Severity::Warn,
        "Accessibility label missing",
    ),
    (
        "meerkat.a11y.focus_missing",
        Severity::Warn,
        "Accessibility focus missing",
    ),
    ("meerkat.probe.failed", Severity::Warn, "Probe failed"),
    ("meerkat.probe.degraded", Severity::Info, "Probe degraded"),
    (
        "meerkat.tracing.event",
        Severity::Info,
        "Tracing event bridged into Apparatus",
    ),
    ("meerkat.agent.spawned", Severity::Info, "Agent spawned"),
    (
        "meerkat.agent.intent_dropped",
        Severity::Warn,
        "Agent intent dropped",
    ),
    (
        "meerkat.agent.action_applied",
        Severity::Info,
        "Agent action applied",
    ),
    (
        "ux.roster_pane.opened",
        Severity::Info,
        "Roster pane opened",
    ),
    (
        "ux.roster_pane.dismissed",
        Severity::Info,
        "Roster pane dismissed",
    ),
    ("ux.gloss_pane.opened", Severity::Info, "Gloss pane opened"),
    (
        "ux.gloss_pane.dismissed",
        Severity::Info,
        "Gloss pane dismissed",
    ),
    (
        "ux.apparatus_pane.opened",
        Severity::Info,
        "Apparatus pane opened",
    ),
    (
        "ux.apparatus_pane.dismissed",
        Severity::Info,
        "Apparatus pane dismissed",
    ),
    ("ux.comms_pane.opened", Severity::Info, "Comms pane opened"),
    (
        "ux.comms_pane.dismissed",
        Severity::Info,
        "Comms pane dismissed",
    ),
    (
        "ux.workbench_pane.opened",
        Severity::Info,
        "Workbench pane opened",
    ),
    (
        "ux.workbench_pane.dismissed",
        Severity::Info,
        "Workbench pane dismissed",
    ),
];

const INITIAL_INVARIANTS: &[InitialInvariant] = &[
    InitialInvariant {
        id: "invariant.meerkat.startup.completes",
        start: "meerkat.startup.started",
        terminal: &["meerkat.startup.succeeded", "meerkat.startup.failed"],
        timeout_ms: 3_000,
    },
    InitialInvariant {
        id: "invariant.meerkat.fetch.completes",
        start: "meerkat.actor.fetch.started",
        terminal: &[
            "meerkat.actor.fetch.succeeded",
            "meerkat.actor.fetch.failed",
        ],
        timeout_ms: 10_000,
    },
    InitialInvariant {
        id: "invariant.meerkat.comms.completes",
        start: "meerkat.actor.comms.started",
        terminal: &[
            "meerkat.actor.comms.succeeded",
            "meerkat.actor.comms.failed",
        ],
        timeout_ms: 10_000,
    },
];
