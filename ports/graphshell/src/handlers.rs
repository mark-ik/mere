//! User-configurable offers for handing an address to another application.

use chirograph::{AdvertisedAction, IntentEffect, IntentReference, PresentationSemantics};
use kernel::address::AddressKind;
use serde::{Deserialize, Serialize};

pub const OPEN_ADDRESS_INTENT: &str = "graphshell.address.open";
pub const OPEN_ADDRESS_SCHEMA: &str = "graphshell.address.open/v1";

/// The typed payload accepted by Graphshell's local open intent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenAddressV1 {
    pub address: String,
    pub handler: String,
}

/// A handler the host is willing to offer for selected address kinds.
///
/// This is a semantic offer, not an executable command line. Native launch
/// authority remains outside the portable Graphshell/Mere cone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandlerOffer {
    pub id: String,
    pub label: String,
    pub explanation: String,
    pub address_kinds: Vec<AddressKind>,
}

impl HandlerOffer {
    pub fn supports(&self, kind: AddressKind) -> bool {
        self.address_kinds.contains(&kind)
    }

    pub fn intent_id(&self) -> String {
        intent_id(&self.id)
    }

    pub fn advertised_action(&self) -> AdvertisedAction {
        AdvertisedAction {
            intent: IntentReference(self.intent_id()),
            label: self.label.clone(),
            explanation: self.explanation.clone(),
            payload_schema: OPEN_ADDRESS_SCHEMA.to_string(),
            input_form: None,
            effect: IntentEffect::ExternalEffect,
        }
    }
}

/// Ordered handler policy. Hosts replace or reorder this list from settings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HandlerRegistry {
    offers: Vec<HandlerOffer>,
}

impl HandlerRegistry {
    pub fn new(offers: Vec<HandlerOffer>) -> Self {
        Self { offers }
    }

    pub fn offers_for(&self, kind: AddressKind) -> impl Iterator<Item = &HandlerOffer> {
        self.offers.iter().filter(move |offer| offer.supports(kind))
    }

    pub fn get(&self, id: &str) -> Option<&HandlerOffer> {
        self.offers.iter().find(|offer| offer.id == id)
    }

    pub fn actions_for(&self, kind: AddressKind) -> Vec<AdvertisedAction> {
        self.offers_for(kind)
            .map(HandlerOffer::advertised_action)
            .collect()
    }

    pub fn attach_actions(&self, semantics: &mut PresentationSemantics, kind: AddressKind) {
        semantics.actions.extend(self.actions_for(kind));
    }
}

pub fn intent_id(handler: &str) -> String {
    format!("{OPEN_ADDRESS_INTENT}/{handler}")
}

pub fn handler_from_intent(intent: &str) -> Option<&str> {
    intent
        .strip_prefix(OPEN_ADDRESS_INTENT)
        .and_then(|suffix| suffix.strip_prefix('/'))
        .filter(|handler| !handler.is_empty())
}
