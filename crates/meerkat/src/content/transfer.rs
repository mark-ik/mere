/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Native <-> transport adapters for the content worker message seam.

use armillary::{NavGeneration, ViewportGeneration};
use content_contract::{
    ContentCommandMessage, ContentStateMessage, ContentUpdateMessage, FetchedMessage,
    LinkHitMessage, SceneTransferDecoder, SceneTransferEncoder, TransferBuffer, TransferError,
};

use super::{ContentCommand, ContentState, ContentUpdate};
use crate::card::LinkHit;
use crate::fetch::Fetched;

fn fetched_to_message(fetched: Fetched) -> FetchedMessage {
    FetchedMessage {
        content_type: fetched.content_type,
        body: fetched.body,
    }
}

fn fetched_from_message(fetched: FetchedMessage) -> Fetched {
    Fetched {
        content_type: fetched.content_type,
        body: fetched.body,
    }
}

fn content_state_to_message(state: ContentState) -> ContentStateMessage {
    match state {
        ContentState::Loading => ContentStateMessage::Loading,
        ContentState::Ready(fetched) => ContentStateMessage::Ready(fetched_to_message(fetched)),
        ContentState::Failed(reason) => ContentStateMessage::Failed(reason),
    }
}

fn content_state_from_message(state: ContentStateMessage) -> ContentState {
    match state {
        ContentStateMessage::Loading => ContentState::Loading,
        ContentStateMessage::Ready(fetched) => ContentState::Ready(fetched_from_message(fetched)),
        ContentStateMessage::Failed(reason) => ContentState::Failed(reason),
    }
}

fn link_hit_to_message(hit: LinkHit) -> LinkHitMessage {
    LinkHitMessage {
        rect: hit.rect,
        url: hit.url,
    }
}

fn link_hit_from_message(hit: LinkHitMessage) -> LinkHit {
    LinkHit {
        rect: hit.rect,
        url: hit.url,
    }
}

impl From<ContentCommand> for ContentCommandMessage {
    fn from(command: ContentCommand) -> Self {
        match command {
            ContentCommand::Show {
                url,
                state,
                engine,
                viewport,
                nav,
                viewport_gen,
                sheet,
            } => Self::Show {
                url,
                state: state.map(content_state_to_message),
                engine,
                viewport,
                nav: nav.0,
                viewport_gen: viewport_gen.0,
                sheet,
            },
            ContentCommand::Resize {
                viewport,
                viewport_gen,
            } => Self::Resize {
                viewport,
                viewport_gen: viewport_gen.0,
            },
            ContentCommand::Retheme {
                sheet,
                viewport_gen,
            } => Self::Retheme {
                sheet,
                viewport_gen: viewport_gen.0,
            },
            ContentCommand::Resource { url, bytes } => Self::Resource { url, bytes },
            ContentCommand::SetLifecycle { hidden, frozen } => {
                Self::SetLifecycle { hidden, frozen }
            }
            ContentCommand::Scroll {
                band_y,
                band_h,
                viewport_gen,
            } => Self::Scroll {
                band_y,
                band_h,
                viewport_gen: viewport_gen.0,
            },
            ContentCommand::Find {
                query,
                viewport_gen,
            } => Self::Find {
                query,
                viewport_gen: viewport_gen.0,
            },
            ContentCommand::SelectText {
                anchor,
                focus,
                viewport_gen,
            } => Self::SelectText {
                anchor,
                focus,
                viewport_gen: viewport_gen.0,
            },
            ContentCommand::AttachScript {
                component_path,
                log,
                document,
                net,
                viewport_gen,
            } => Self::AttachScript {
                component_path,
                log,
                document,
                net,
                viewport_gen: viewport_gen.0,
            },
            ContentCommand::DeliverEvent {
                kind,
                payload,
                viewport_gen,
            } => Self::DeliverEvent {
                kind,
                payload,
                viewport_gen: viewport_gen.0,
            },
            ContentCommand::DetachScript { viewport_gen } => Self::DetachScript {
                viewport_gen: viewport_gen.0,
            },
            ContentCommand::MaterializeLinks { viewport_gen } => Self::MaterializeLinks {
                viewport_gen: viewport_gen.0,
            },
            #[cfg(feature = "scripted")]
            ContentCommand::ScriptedClick { x, y, viewport_gen } => Self::ScriptedClick {
                x,
                y,
                viewport_gen: viewport_gen.0,
            },
        }
    }
}

impl From<ContentCommandMessage> for ContentCommand {
    fn from(command: ContentCommandMessage) -> Self {
        match command {
            ContentCommandMessage::Show {
                url,
                state,
                engine,
                viewport,
                nav,
                viewport_gen,
                sheet,
            } => Self::Show {
                url,
                state: state.map(content_state_from_message),
                engine,
                viewport,
                nav: NavGeneration(nav),
                viewport_gen: ViewportGeneration(viewport_gen),
                sheet,
            },
            ContentCommandMessage::Resize {
                viewport,
                viewport_gen,
            } => Self::Resize {
                viewport,
                viewport_gen: ViewportGeneration(viewport_gen),
            },
            ContentCommandMessage::Retheme {
                sheet,
                viewport_gen,
            } => Self::Retheme {
                sheet,
                viewport_gen: ViewportGeneration(viewport_gen),
            },
            ContentCommandMessage::Resource { url, bytes } => Self::Resource { url, bytes },
            ContentCommandMessage::SetLifecycle { hidden, frozen } => {
                Self::SetLifecycle { hidden, frozen }
            }
            ContentCommandMessage::Scroll {
                band_y,
                band_h,
                viewport_gen,
            } => Self::Scroll {
                band_y,
                band_h,
                viewport_gen: ViewportGeneration(viewport_gen),
            },
            ContentCommandMessage::Find {
                query,
                viewport_gen,
            } => Self::Find {
                query,
                viewport_gen: ViewportGeneration(viewport_gen),
            },
            ContentCommandMessage::SelectText {
                anchor,
                focus,
                viewport_gen,
            } => Self::SelectText {
                anchor,
                focus,
                viewport_gen: ViewportGeneration(viewport_gen),
            },
            ContentCommandMessage::AttachScript {
                component_path,
                log,
                document,
                net,
                viewport_gen,
            } => Self::AttachScript {
                component_path,
                log,
                document,
                net,
                viewport_gen: ViewportGeneration(viewport_gen),
            },
            ContentCommandMessage::DeliverEvent {
                kind,
                payload,
                viewport_gen,
            } => Self::DeliverEvent {
                kind,
                payload,
                viewport_gen: ViewportGeneration(viewport_gen),
            },
            ContentCommandMessage::DetachScript { viewport_gen } => Self::DetachScript {
                viewport_gen: ViewportGeneration(viewport_gen),
            },
            ContentCommandMessage::MaterializeLinks { viewport_gen } => Self::MaterializeLinks {
                viewport_gen: ViewportGeneration(viewport_gen),
            },
            #[cfg(feature = "scripted")]
            ContentCommandMessage::ScriptedClick { x, y, viewport_gen } => Self::ScriptedClick {
                x,
                y,
                viewport_gen: ViewportGeneration(viewport_gen),
            },
        }
    }
}

impl From<ContentUpdate> for ContentUpdateMessage {
    fn from(update: ContentUpdate) -> Self {
        match update {
            ContentUpdate::Document {
                nav,
                viewport_gen,
                packet,
                fonts,
                content_height,
            } => Self::Document {
                nav: nav.0,
                viewport_gen: viewport_gen.0,
                packet,
                fonts,
                content_height,
            },
            ContentUpdate::Scene {
                nav,
                viewport_gen,
                scene,
                stats,
                content_height,
                band_y,
                band_h,
                links,
                masks,
            } => Self::Scene {
                nav: nav.0,
                viewport_gen: viewport_gen.0,
                scene,
                stats,
                content_height,
                band_y,
                band_h,
                links: links.into_iter().map(link_hit_to_message).collect(),
                masks,
            },
            ContentUpdate::Wanted { nav, urls } => Self::Wanted { nav: nav.0, urls },
            ContentUpdate::Contribution { contributions } => Self::Contribution { contributions },
            ContentUpdate::FindMatches {
                nav,
                viewport_gen,
                matches,
            } => Self::FindMatches {
                nav: nav.0,
                viewport_gen: viewport_gen.0,
                matches,
            },
            ContentUpdate::TextSelection {
                nav,
                viewport_gen,
                selection,
            } => Self::TextSelection {
                nav: nav.0,
                viewport_gen: viewport_gen.0,
                selection,
            },
            ContentUpdate::EngineStats {
                nav,
                viewport_gen,
                dom,
                layout,
            } => Self::EngineStats {
                nav: nav.0,
                viewport_gen: viewport_gen.0,
                dom,
                layout,
            },
            ContentUpdate::ScriptOutcome { nav, outcome } => Self::ScriptOutcome {
                nav: nav.0,
                outcome,
            },
            ContentUpdate::TransportError { reason } => Self::TransportError { reason },
        }
    }
}

impl From<ContentUpdateMessage> for ContentUpdate {
    fn from(update: ContentUpdateMessage) -> Self {
        match update {
            ContentUpdateMessage::Document {
                nav,
                viewport_gen,
                packet,
                fonts,
                content_height,
            } => Self::Document {
                nav: NavGeneration(nav),
                viewport_gen: ViewportGeneration(viewport_gen),
                packet,
                fonts,
                content_height,
            },
            ContentUpdateMessage::Scene {
                nav,
                viewport_gen,
                scene,
                stats,
                content_height,
                band_y,
                band_h,
                links,
                masks,
            } => Self::Scene {
                nav: NavGeneration(nav),
                viewport_gen: ViewportGeneration(viewport_gen),
                scene,
                stats,
                content_height,
                band_y,
                band_h,
                links: links.into_iter().map(link_hit_from_message).collect(),
                masks,
            },
            ContentUpdateMessage::Wanted { nav, urls } => Self::Wanted {
                nav: NavGeneration(nav),
                urls,
            },
            ContentUpdateMessage::Contribution { contributions } => {
                Self::Contribution { contributions }
            }
            ContentUpdateMessage::FindMatches {
                nav,
                viewport_gen,
                matches,
            } => Self::FindMatches {
                nav: NavGeneration(nav),
                viewport_gen: ViewportGeneration(viewport_gen),
                matches,
            },
            ContentUpdateMessage::TextSelection {
                nav,
                viewport_gen,
                selection,
            } => Self::TextSelection {
                nav: NavGeneration(nav),
                viewport_gen: ViewportGeneration(viewport_gen),
                selection,
            },
            ContentUpdateMessage::EngineStats {
                nav,
                viewport_gen,
                dom,
                layout,
            } => Self::EngineStats {
                nav: NavGeneration(nav),
                viewport_gen: ViewportGeneration(viewport_gen),
                dom,
                layout,
            },
            ContentUpdateMessage::ScriptOutcome { nav, outcome } => Self::ScriptOutcome {
                nav: NavGeneration(nav),
                outcome,
            },
            ContentUpdateMessage::TransportError { reason } => Self::TransportError { reason },
        }
    }
}

impl ContentUpdate {
    pub(crate) fn into_transfer_buffer(
        self,
        encoder: &mut SceneTransferEncoder,
    ) -> Result<TransferBuffer, TransferError> {
        ContentUpdateMessage::from(self).into_transfer_buffer(encoder)
    }

    pub(crate) fn from_transfer_buffer(
        bytes: &[u8],
        decoder: &mut SceneTransferDecoder,
    ) -> Result<Self, TransferError> {
        Ok(ContentUpdateMessage::from_transfer_buffer(bytes, decoder)?.into())
    }
}

impl ContentCommand {
    pub(crate) fn into_transfer_buffer(self) -> Result<TransferBuffer, TransferError> {
        ContentCommandMessage::from(self).into_transfer_buffer()
    }

    pub(crate) fn from_transfer_buffer(bytes: &[u8]) -> Result<Self, TransferError> {
        Ok(ContentCommandMessage::from_transfer_buffer(bytes)?.into())
    }
}
