/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::collections::{HashMap, VecDeque};

use super::*;
use crate::channels::*;

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingInvariantToken {
    start_channel: String,
    deadline_unix_ms: u64,
}

#[derive(Debug, Clone)]
pub struct ChannelConfig {
    pub enabled: bool,
    pub sample_rate: f32,
    pub retention_count: usize,
}

impl Default for ChannelConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sample_rate: 1.0,
            retention_count: 100,
        }
    }
}

pub struct DiagnosticsRegistry {
    channels: HashMap<String, RuntimeChannelDescriptor>,
    configs: HashMap<String, ChannelConfig>,
    sample_counters: HashMap<String, u64>,
    orphan_channels: HashMap<String, u64>,
    pub(crate) invariants: HashMap<String, DiagnosticsInvariant>,
    pending_invariants: HashMap<String, VecDeque<PendingInvariantToken>>,
}

impl Default for DiagnosticsRegistry {
    fn default() -> Self {
        let mut registry = Self {
            channels: HashMap::new(),
            configs: HashMap::new(),
            sample_counters: HashMap::new(),
            orphan_channels: HashMap::new(),
            invariants: HashMap::new(),
            pending_invariants: HashMap::new(),
        };

        registry.register_batch(phase0_required_channels());
        registry.register_batch(phase2_required_channels());
        registry.register_batch(phase3_required_channels());
        registry.register_batch(phase5_required_channels());
        registry.register_default_invariants();

        registry
    }
}

impl DiagnosticsRegistry {
    pub fn register(&mut self, descriptor: DiagnosticChannelDescriptor) {
        let runtime = RuntimeChannelDescriptor::from_contract(descriptor);
        self.configs.entry(runtime.channel_id.clone()).or_default();
        self.channels.insert(runtime.channel_id.clone(), runtime);
    }

    pub fn register_batch(&mut self, descriptors: &[DiagnosticChannelDescriptor]) {
        for descriptor in descriptors.iter().copied() {
            self.register(descriptor);
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn get_config(&self, channel_id: &str) -> ChannelConfig {
        self.configs
            .get(&super::normalize_channel_id(channel_id))
            .cloned()
            .unwrap_or_default()
    }

    pub fn set_config(&mut self, channel_id: &str, config: ChannelConfig) {
        self.configs.insert(
            super::normalize_channel_id(channel_id),
            super::normalize_channel_config(config),
        );
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn has_channel(&self, channel_id: &str) -> bool {
        self.channels
            .contains_key(&super::normalize_channel_id(channel_id))
    }

    pub fn list_channel_configs(&self) -> Vec<(RuntimeChannelDescriptor, ChannelConfig)> {
        self.channels
            .values()
            .cloned()
            .map(|descriptor| {
                let config = self
                    .configs
                    .get(&descriptor.channel_id)
                    .cloned()
                    .unwrap_or_default();
                (descriptor, config)
            })
            .collect()
    }

    pub fn list_orphan_channels(&self) -> Vec<(String, u64)> {
        let mut entries: Vec<(String, u64)> = self
            .orphan_channels
            .iter()
            .map(|(channel_id, count)| (channel_id.clone(), *count))
            .collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        entries
    }

    pub fn register_runtime_channel(
        &mut self,
        descriptor: RuntimeChannelDescriptor,
        policy: ChannelRegistrationPolicy,
    ) -> Result<bool, ChannelRegistrationError> {
        if descriptor.channel_id.trim().is_empty() {
            return Err(ChannelRegistrationError::InvalidChannelId);
        }

        let normalized_id = super::normalize_channel_id(&descriptor.channel_id);
        let mut normalized_descriptor = descriptor.clone();
        normalized_descriptor.channel_id = normalized_id.clone();

        super::validate_runtime_channel_ownership(&normalized_descriptor)?;
        super::validate_runtime_channel_schema(&normalized_descriptor)?;

        if let Some(existing) = self.channels.get(&normalized_id) {
            if existing.schema_version != normalized_descriptor.schema_version {
                match policy {
                    ChannelRegistrationPolicy::RejectConflict => {
                        return Err(ChannelRegistrationError::Conflict {
                            channel_id: normalized_id,
                            existing_schema_version: existing.schema_version,
                            requested_schema_version: normalized_descriptor.schema_version,
                        });
                    }
                    ChannelRegistrationPolicy::KeepExisting => return Ok(false),
                    ChannelRegistrationPolicy::ReplaceExisting => {}
                }
            } else if !matches!(policy, ChannelRegistrationPolicy::ReplaceExisting) {
                return Ok(false);
            }
        }

        self.channels.insert(
            normalized_descriptor.channel_id.clone(),
            normalized_descriptor.clone(),
        );
        self.configs
            .entry(normalized_descriptor.channel_id)
            .or_default();
        Ok(true)
    }

    pub fn register_mod_channel(
        &mut self,
        mod_id: &str,
        channel_id: &str,
        schema_version: u16,
        description: Option<String>,
        capabilities: &[DiagnosticsCapability],
    ) -> Result<bool, ChannelRegistrationError> {
        if !capabilities.contains(&DiagnosticsCapability::RegisterChannels) {
            return Err(ChannelRegistrationError::InvalidOwnership {
                channel_id: channel_id.to_string(),
                reason: "missing RegisterChannels capability".to_string(),
            });
        }

        self.register_runtime_channel(
            RuntimeChannelDescriptor::info(
                channel_id,
                schema_version,
                DiagnosticsChannelOwner::mod_owner(mod_id),
                description,
            ),
            ChannelRegistrationPolicy::RejectConflict,
        )
    }

    pub fn register_verse_channel(
        &mut self,
        peer_id: &str,
        channel_id: &str,
        schema_version: u16,
        description: Option<String>,
        capabilities: &[DiagnosticsCapability],
    ) -> Result<bool, ChannelRegistrationError> {
        if !capabilities.contains(&DiagnosticsCapability::RegisterChannels) {
            return Err(ChannelRegistrationError::InvalidOwnership {
                channel_id: channel_id.to_string(),
                reason: "missing RegisterChannels capability".to_string(),
            });
        }

        self.register_runtime_channel(
            RuntimeChannelDescriptor::info(
                channel_id,
                schema_version,
                DiagnosticsChannelOwner::verse_owner(peer_id),
                description,
            ),
            ChannelRegistrationPolicy::RejectConflict,
        )
    }

    pub fn should_emit_channel(&mut self, channel_id: &str) -> bool {
        let normalized = super::normalize_channel_id(channel_id);
        if !self.channels.contains_key(&normalized) {
            log::warn!("diagnostics: emit to unregistered channel '{normalized}'");
            let _ = self.register_runtime_channel(
                RuntimeChannelDescriptor::info(
                    normalized.clone(),
                    1,
                    DiagnosticsChannelOwner::runtime(),
                    Some("Auto-registered runtime channel".to_string()),
                ),
                ChannelRegistrationPolicy::KeepExisting,
            );
            *self.orphan_channels.entry(normalized.clone()).or_insert(0) += 1;
        }

        let config = self.configs.get(&normalized).cloned().unwrap_or_default();
        if !config.enabled {
            return false;
        }
        if config.sample_rate >= 1.0 {
            return true;
        }
        if config.sample_rate <= 0.0 {
            return false;
        }

        let counter = self.sample_counters.entry(normalized).or_insert(0);
        *counter = counter.saturating_add(1);
        let gate = (1.0f32 / config.sample_rate.max(0.0001)).ceil() as u64;
        gate <= 1 || (*counter % gate == 0)
    }

    pub fn register_invariant(
        &mut self,
        invariant: DiagnosticsInvariant,
        capabilities: &[DiagnosticsCapability],
    ) -> Result<bool, ChannelRegistrationError> {
        if !capabilities.contains(&DiagnosticsCapability::RegisterInvariants) {
            return Err(ChannelRegistrationError::InvalidOwnership {
                channel_id: invariant.start_channel,
                reason: "missing RegisterInvariants capability".to_string(),
            });
        }

        if invariant.invariant_id.trim().is_empty() || invariant.timeout_ms == 0 {
            return Err(ChannelRegistrationError::InvalidChannelId);
        }

        let invariant_id = invariant.invariant_id.trim().to_ascii_lowercase();
        if self.invariants.contains_key(&invariant_id) {
            return Ok(false);
        }

        self.invariants.insert(
            invariant_id,
            DiagnosticsInvariant {
                invariant_id: invariant.invariant_id.trim().to_ascii_lowercase(),
                start_channel: super::normalize_channel_id(&invariant.start_channel),
                terminal_channels: invariant
                    .terminal_channels
                    .iter()
                    .map(|entry| super::normalize_channel_id(entry))
                    .collect(),
                timeout_ms: invariant.timeout_ms,
                owner: invariant.owner,
                enabled: invariant.enabled,
            },
        );
        Ok(true)
    }

    pub fn observe_channel_event(
        &mut self,
        channel_id: &str,
        now_unix_ms: u64,
    ) -> Vec<DiagnosticsInvariantViolation> {
        let normalized = super::normalize_channel_id(channel_id);

        for invariant in self.invariants.values() {
            if !invariant.enabled {
                continue;
            }

            if invariant.start_channel == normalized {
                self.pending_invariants
                    .entry(invariant.invariant_id.clone())
                    .or_default()
                    .push_back(PendingInvariantToken {
                        start_channel: normalized.clone(),
                        deadline_unix_ms: now_unix_ms.saturating_add(invariant.timeout_ms),
                    });
            }

            if invariant
                .terminal_channels
                .iter()
                .any(|entry| entry == &normalized)
                && let Some(queue) = self.pending_invariants.get_mut(&invariant.invariant_id)
            {
                let _ = queue.pop_front();
            }
        }

        self.sweep_invariants(now_unix_ms)
    }

    pub fn sweep_invariants(&mut self, now_unix_ms: u64) -> Vec<DiagnosticsInvariantViolation> {
        let mut violations = Vec::new();

        for invariant in self.invariants.values() {
            if !invariant.enabled {
                continue;
            }

            let Some(queue) = self.pending_invariants.get_mut(&invariant.invariant_id) else {
                continue;
            };

            while let Some(front) = queue.front() {
                if front.deadline_unix_ms > now_unix_ms {
                    break;
                }
                let expired = queue.pop_front().expect("queue front just checked");
                violations.push(DiagnosticsInvariantViolation {
                    invariant_id: invariant.invariant_id.clone(),
                    start_channel: expired.start_channel,
                    deadline_unix_ms: expired.deadline_unix_ms,
                });
            }
        }

        violations
    }

    fn register_default_invariants(&mut self) {
        let _ = self.register_invariant(
            DiagnosticsInvariant {
                invariant_id: "invariant.registry.protocol.resolve_completes".to_string(),
                start_channel: CHANNEL_PROTOCOL_RESOLVE_STARTED.to_string(),
                terminal_channels: vec![
                    CHANNEL_PROTOCOL_RESOLVE_SUCCEEDED.to_string(),
                    CHANNEL_PROTOCOL_RESOLVE_FAILED.to_string(),
                ],
                timeout_ms: 500,
                owner: DiagnosticsChannelOwner::core(),
                enabled: true,
            },
            &[DiagnosticsCapability::RegisterInvariants],
        );

        let phase5_terminal_channels = vec![
            CHANNEL_VERSE_SYNC_INTENT_APPLIED.to_string(),
            CHANNEL_VERSE_SYNC_ACCESS_DENIED.to_string(),
            CHANNEL_VERSE_SYNC_CONNECTION_REJECTED.to_string(),
        ];

        let _ = self.register_invariant(
            DiagnosticsInvariant {
                invariant_id: INVARIANT_VERSE_SYNC_RECEIVED_COMPLETES.to_string(),
                start_channel: CHANNEL_VERSE_SYNC_UNIT_RECEIVED.to_string(),
                terminal_channels: phase5_terminal_channels.clone(),
                timeout_ms: 1_000,
                owner: DiagnosticsChannelOwner::core(),
                enabled: true,
            },
            &[DiagnosticsCapability::RegisterInvariants],
        );

        let _ = self.register_invariant(
            DiagnosticsInvariant {
                invariant_id: INVARIANT_VERSE_SYNC_SENT_COMPLETES.to_string(),
                start_channel: CHANNEL_VERSE_SYNC_UNIT_SENT.to_string(),
                terminal_channels: phase5_terminal_channels,
                timeout_ms: 2_000,
                owner: DiagnosticsChannelOwner::core(),
                enabled: true,
            },
            &[DiagnosticsCapability::RegisterInvariants],
        );
    }
}
