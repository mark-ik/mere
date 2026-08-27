// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! The embeddable contact Ledger: a product surface over shared projection,
//! selection, and citation records.

use std::collections::{BTreeMap, BTreeSet};

use chirograph::{CoordinatedSelection, SelectionTarget};
use incipit::{ShelfmarkAuthorityV1, ShelfmarkInputV1, ShelfmarkV1};
use sceno::SourceRef;
use serde::{Deserialize, Serialize};

pub const LEDGER_SCHEMA: &str = "gazette.ledger/v1";
pub const CONTACT_ADAPTER: &str = "gazette.contact/v1";
pub const FACET_ADAPTER: &str = "gazette.contact-facet/v1";

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Contact {
    pub id: String,
    pub name: String,
    pub handle: String,
    pub trust: String,
    pub freshness: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct ContactFacet {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct LedgerAxisSource {
    pub source: SourceRef,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct LedgerContributor {
    pub authority: String,
    pub source: SourceRef,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct LedgerCell {
    pub contact: SourceRef,
    pub facet: SourceRef,
    pub value: String,
    pub contributors: Vec<LedgerContributor>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct LedgerInstanceAddress {
    pub view: String,
    pub source: SourceRef,
    pub facet: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct LedgerInstanceDelta {
    pub instance: LedgerInstanceAddress,
    pub visible: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct LedgerProjection {
    pub schema: String,
    pub rows: Vec<LedgerAxisSource>,
    pub columns: Vec<LedgerAxisSource>,
    pub cells: Vec<LedgerCell>,
    pub appearances: Vec<LedgerInstanceAddress>,
    pub selection: CoordinatedSelection,
    pub accessible_html: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct LedgerCitationInputs {
    pub contacts_record: String,
    pub contacts_generation: String,
    pub facets_record: String,
    pub facets_generation: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct LedgerCitationReceipt {
    pub input_generations: BTreeMap<String, String>,
    pub honored_instance_deltas: usize,
}

pub fn project_ledger(
    contacts: &[Contact],
    facets: &[ContactFacet],
    selection: CoordinatedSelection,
) -> Result<LedgerProjection, String> {
    if contacts.is_empty() || facets.is_empty() {
        return Err("Ledger needs at least one contact and one facet".into());
    }
    let contact_ids = contacts
        .iter()
        .map(|contact| contact.id.as_str())
        .collect::<BTreeSet<_>>();
    let facet_ids = facets
        .iter()
        .map(|facet| facet.id.as_str())
        .collect::<BTreeSet<_>>();
    if contact_ids.len() != contacts.len() || facet_ids.len() != facets.len() {
        return Err("Ledger axis source ids must be unique".into());
    }

    let rows = contacts
        .iter()
        .map(|contact| LedgerAxisSource {
            source: SourceRef::new(CONTACT_ADAPTER, &contact.id),
            label: contact.name.clone(),
        })
        .collect::<Vec<_>>();
    let columns = facets
        .iter()
        .map(|facet| LedgerAxisSource {
            source: SourceRef::new(FACET_ADAPTER, &facet.id),
            label: facet.label.clone(),
        })
        .collect::<Vec<_>>();
    let mut cells = Vec::with_capacity(contacts.len() * facets.len());
    for contact in contacts {
        for facet in facets {
            let contact_source = SourceRef::new(CONTACT_ADAPTER, &contact.id);
            let facet_source = SourceRef::new(FACET_ADAPTER, &facet.id);
            cells.push(LedgerCell {
                contact: contact_source.clone(),
                facet: facet_source.clone(),
                value: facet_value(contact, &facet.id)?,
                contributors: vec![
                    LedgerContributor {
                        authority: "contacts".into(),
                        source: contact_source,
                    },
                    LedgerContributor {
                        authority: "facets".into(),
                        source: facet_source,
                    },
                ],
            });
        }
    }
    let appearances = contacts
        .iter()
        .flat_map(|contact| {
            [
                ("recipient-picker", "row"),
                ("ledger", "row-heading"),
                ("contact-detail", "card"),
            ]
            .map(|(view, facet)| LedgerInstanceAddress {
                view: view.into(),
                source: SourceRef::new(CONTACT_ADAPTER, &contact.id),
                facet: facet.into(),
            })
        })
        .collect();
    let accessible_html = ledger_html(&rows, &columns, &cells);
    Ok(LedgerProjection {
        schema: LEDGER_SCHEMA.into(),
        rows,
        columns,
        cells,
        appearances,
        selection,
        accessible_html,
    })
}

impl LedgerProjection {
    pub fn visible_contacts(&self, consumer: &str) -> Vec<&LedgerAxisSource> {
        let targets = self.selection.targets_for(consumer);
        self.rows
            .iter()
            .filter(|row| {
                targets.as_ref().is_none_or(|targets| {
                    targets.contains(&SelectionTarget {
                        kind: "contact".into(),
                        id: row.source.id.clone(),
                    })
                })
            })
            .collect()
    }

    pub fn shelfmark(
        &self,
        inputs: LedgerCitationInputs,
        instance_deltas: &[LedgerInstanceDelta],
    ) -> Result<ShelfmarkV1, String> {
        let mut shelfmark = ShelfmarkV1::new("ledger");
        shelfmark.inputs.insert(
            "rows".into(),
            ShelfmarkInputV1 {
                authority: ShelfmarkAuthorityV1 {
                    adapter: "gazette.contacts/v1".into(),
                    record: inputs.contacts_record,
                },
                reading: "contacts".into(),
                reading_parameters: None,
                arrangement: None,
                expects_generation: inputs.contacts_generation,
            },
        );
        shelfmark.inputs.insert(
            "columns".into(),
            ShelfmarkInputV1 {
                authority: ShelfmarkAuthorityV1 {
                    adapter: "gazette.facets/v1".into(),
                    record: inputs.facets_record,
                },
                reading: "contact-facets".into(),
                reading_parameters: None,
                arrangement: None,
                expects_generation: inputs.facets_generation,
            },
        );
        shelfmark.delta.insert(
            "selection".into(),
            serde_json::to_string(&self.selection)
                .map_err(|error| format!("could not cite Ledger selection: {error}"))?,
        );
        shelfmark.delta.insert(
            "gazette.instances".into(),
            serde_json::to_string(instance_deltas)
                .map_err(|error| format!("could not cite Ledger instances: {error}"))?,
        );
        shelfmark
            .validate()
            .map_err(|error| format!("invalid Ledger shelfmark: {error:?}"))?;
        Ok(shelfmark)
    }
}

pub fn resolve_ledger_shelfmark(
    shelfmark: &ShelfmarkV1,
    found_generations: &BTreeMap<String, String>,
) -> Result<LedgerCitationReceipt, String> {
    shelfmark
        .validate()
        .map_err(|error| format!("invalid Ledger shelfmark: {error:?}"))?;
    if shelfmark.projection != "ledger" {
        return Err("citation is not a Ledger projection".into());
    }
    let mut verified = BTreeMap::new();
    for role in ["rows", "columns"] {
        let input = shelfmark
            .inputs
            .get(role)
            .ok_or_else(|| format!("Ledger citation lacks {role} input"))?;
        let found = found_generations
            .get(role)
            .ok_or_else(|| format!("Ledger {role} authority is unavailable"))?;
        if found != &input.expects_generation {
            return Err(format!(
                "Ledger {role} authority moved: expected {}, found {found}",
                input.expects_generation
            ));
        }
        verified.insert(role.into(), found.clone());
    }
    let instance_deltas: Vec<LedgerInstanceDelta> = serde_json::from_str(
        shelfmark
            .delta
            .get("gazette.instances")
            .ok_or_else(|| "Ledger citation lacks instance state".to_owned())?,
    )
    .map_err(|error| format!("invalid Ledger instance state: {error}"))?;
    Ok(LedgerCitationReceipt {
        input_generations: verified,
        honored_instance_deltas: instance_deltas.len(),
    })
}

fn facet_value(contact: &Contact, facet: &str) -> Result<String, String> {
    match facet {
        "handle" => Ok(contact.handle.clone()),
        "trust" => Ok(contact.trust.clone()),
        "freshness" => Ok(contact.freshness.clone()),
        _ => Err(format!("unsupported Ledger facet {facet}")),
    }
}

fn ledger_html(
    rows: &[LedgerAxisSource],
    columns: &[LedgerAxisSource],
    cells: &[LedgerCell],
) -> String {
    let mut html = String::from(
        "<table data-projection-family=\"ledger\"><caption>Contacts by facet</caption><thead><tr><th scope=\"col\">Contact</th>",
    );
    for column in columns {
        html.push_str("<th scope=\"col\">");
        html.push_str(&escape_html(&column.label));
        html.push_str("</th>");
    }
    html.push_str("</tr></thead><tbody>");
    for (row_index, row) in rows.iter().enumerate() {
        html.push_str("<tr><th scope=\"row\">");
        html.push_str(&escape_html(&row.label));
        html.push_str("</th>");
        for column_index in 0..columns.len() {
            html.push_str("<td>");
            html.push_str(&escape_html(
                &cells[row_index * columns.len() + column_index].value,
            ));
            html.push_str("</td>");
        }
        html.push_str("</tr>");
    }
    html.push_str("</tbody></table>");
    html
}

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use chirograph::{Selection, SelectionResolution, SelectionRole};

    fn contacts() -> Vec<Contact> {
        vec![
            Contact {
                id: "ada".into(),
                name: "Ada".into(),
                handle: "@ada@example.net".into(),
                trust: "vouched".into(),
                freshness: "today".into(),
            },
            Contact {
                id: "mina".into(),
                name: "Mina".into(),
                handle: "@mina@example.org".into(),
                trust: "known".into(),
                freshness: "yesterday".into(),
            },
        ]
    }

    fn facets() -> Vec<ContactFacet> {
        ["handle", "trust", "freshness"]
            .into_iter()
            .map(|id| ContactFacet {
                id: id.into(),
                label: id.into(),
            })
            .collect()
    }

    #[test]
    fn ledger_replays_matrix_and_repeated_instance_receipts() {
        let ledger = project_ledger(
            &contacts(),
            &facets(),
            CoordinatedSelection::new(SelectionResolution::Crossfilter),
        )
        .expect("Ledger projection");
        assert_eq!(ledger.cells.len(), 6);
        assert!(ledger.cells.iter().all(|cell| cell.contributors.len() == 2));
        assert!(ledger.accessible_html.contains("<caption>"));
        assert!(ledger.accessible_html.contains("scope=\"row\""));
        assert!(ledger.accessible_html.contains("scope=\"col\""));

        let ada = SourceRef::new(CONTACT_ADAPTER, "ada");
        let appearances = ledger
            .appearances
            .iter()
            .filter(|appearance| appearance.source == ada)
            .collect::<Vec<_>>();
        assert_eq!(appearances.len(), 3);
        assert_eq!(
            appearances
                .iter()
                .map(|appearance| appearance.view.as_str())
                .collect::<BTreeSet<_>>()
                .len(),
            3
        );
    }

    #[test]
    fn ledger_and_recipient_picker_crossfilter_without_private_coordination() {
        let mut selection = CoordinatedSelection::new(SelectionResolution::Crossfilter);
        selection.set(
            SelectionRole::Brush,
            Selection::one("ledger", "contact", "ada"),
        );
        selection.set(
            SelectionRole::Filter,
            Selection::one("recipient-picker", "contact", "mina"),
        );
        let mut ledger = project_ledger(&contacts(), &facets(), selection).expect("Ledger");
        assert_eq!(ledger.visible_contacts("ledger")[0].source.id, "mina");
        assert_eq!(
            ledger.visible_contacts("recipient-picker")[0].source.id,
            "ada"
        );
        assert!(ledger.selection.remove("recipient-picker"));
        assert_eq!(ledger.visible_contacts("ledger").len(), contacts().len());
    }

    #[test]
    fn ledger_shelfmark_checks_both_authorities_and_instance_delta() {
        let ledger = project_ledger(
            &contacts(),
            &facets(),
            CoordinatedSelection::new(SelectionResolution::Crossfilter),
        )
        .expect("Ledger");
        let shelfmark = ledger
            .shelfmark(
                LedgerCitationInputs {
                    contacts_record: "resident:contacts".into(),
                    contacts_generation: "contacts-7".into(),
                    facets_record: "gazette:facet-catalog".into(),
                    facets_generation: "facets-2".into(),
                },
                &[LedgerInstanceDelta {
                    instance: LedgerInstanceAddress {
                        view: "contact-detail".into(),
                        source: SourceRef::new(CONTACT_ADAPTER, "ada"),
                        facet: "card".into(),
                    },
                    visible: false,
                }],
            )
            .expect("Ledger shelfmark");
        let receipt = resolve_ledger_shelfmark(
            &shelfmark,
            &BTreeMap::from([
                ("columns".into(), "facets-2".into()),
                ("rows".into(), "contacts-7".into()),
            ]),
        )
        .expect("Ledger shelfmark resolves");
        assert_eq!(receipt.input_generations.len(), 2);
        assert_eq!(receipt.honored_instance_deltas, 1);

        let wire = serde_json::to_string(&shelfmark).expect("stable wire");
        assert_eq!(
            serde_json::from_str::<ShelfmarkV1>(&wire).expect("decode"),
            shelfmark
        );
    }
}
