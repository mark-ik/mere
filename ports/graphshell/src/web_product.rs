// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! H3's daily graph operations and semantic form wiring.

use std::collections::HashMap;

use graphshell::product::{
    EditableRelation, ExportRequest, LocalFileMetadata, RelationFamilyFilter, SavedSceneV1,
    TransferScope,
};
use mere::canvas::{CameraView, Face, project_canvas_strategy_with_score_for_view};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::Closure;
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{Event, File, HtmlInputElement, HtmlSelectElement, HtmlTextAreaElement};

use super::{ActiveSession, BrowserHost, element, root, update_semantics};
use crate::web_view::ChromeModel;

const SAVED_SCENE_ADDRESS: &str = "mere://scene/graphshell-h3";
const DEFAULT_SPRITE: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=";

pub(super) fn install_product_events(
    state: &std::rc::Rc<std::cell::RefCell<BrowserHost>>,
) -> Result<(), String> {
    let input: HtmlInputElement = element_as("file-input")?;
    let change_input = input.clone();
    let file_state = state.clone();
    let change = Closure::<dyn FnMut(Event)>::new(move |_: Event| {
        let Some(file) = change_input.files().and_then(|files| files.item(0)) else {
            return;
        };
        let state = file_state.clone();
        spawn_local(async move {
            let result = read_file_metadata(&file).await.and_then(|metadata| {
                let mut host = state.borrow_mut();
                let id = host
                    .app
                    .host
                    .create_file_metadata(metadata)
                    .map_err(|error| error.to_string())?;
                host.sync_canvas(&[id], true)?;
                host.product_status = "File metadata added · content hash is portable".to_string();
                host.chrome_dirty = true;
                update_semantics(&mut host)
            });
            if let Err(error) = result {
                let mut host = state.borrow_mut();
                host.product_status = format!("File intake failed · {error}");
                host.chrome_dirty = true;
                let _ = update_semantics(&mut host);
            }
        });
    });
    input
        .add_event_listener_with_callback("change", change.as_ref().unchecked_ref())
        .map_err(|_| "could not attach file intake")?;
    change.forget();
    Ok(())
}

impl BrowserHost {
    pub(super) fn run_product_command(&mut self, command: &str) -> bool {
        let result = match command {
            "add-address" => self.add_address(),
            "save-metadata" => self.save_metadata(),
            "add-relation" => self.add_relation(),
            "select-pair" => self.select_pair(),
            "apply-filter" => self.apply_filter(),
            "clear-filter" => self.clear_filter(),
            "apply-arrangement" => self.apply_arrangement_from_form(),
            "toggle-physics" => self.toggle_physics(),
            "apply-face" => self.apply_face(),
            "save-scene" => self.save_scene(),
            "reopen-scene" => self.reopen_scene(),
            "export-codicil" | "export-engram" => self.export_codicil(),
            "open-codicil" | "open-engram" => self.open_codicil(),
            _ => return false,
        };
        self.product_status = match result {
            Ok(status) => status,
            Err(error) => format!("Failed · {error}"),
        };
        self.chrome_dirty = true;
        true
    }

    pub(super) fn product_chrome(&self) -> (String, String, bool) {
        (
            self.product_status.clone(),
            self.layout_id.clone(),
            self.canvas.physics_paused(),
        )
    }

    pub(super) fn sync_canvas(&mut self, selected: &[Uuid], fit: bool) -> Result<(), String> {
        self.arrangement_transition = None;
        let camera = self.canvas.camera();
        let previous_score = self.canvas.projection_score().cloned();
        let old = self.canvas.cartography_geometry();
        self.canvas.set_graph(self.app.host.graph().clone());
        self.canvas
            .apply_cartography_importance_metric(old.importance_metric());
        self.canvas.apply_cartography_sizing(
            old.size_iter(),
            old.size_by_degree(),
            old.size_by_importance(),
        );
        self.canvas.apply_cartography_sprites(old.sprite_iter());
        self.canvas
            .apply_cartography_sprite_hulls(old.sprite_hull_iter());
        self.canvas.apply_cartography_materials(old.material_iter());
        self.canvas.apply_cartography_faces(old.face_iter());

        let extents = self.canvas.strategy_extents();
        let projection = project_canvas_strategy_with_score_for_view(
            &self.layout_id,
            self.canvas.graph(),
            self.canvas.focused_key(),
            self.width,
            self.height,
            None,
            Some(&extents),
            true,
            camera.zoom,
            previous_score.as_ref(),
        );
        let old_positions: HashMap<_, _> = old.iter().collect();
        let positions: Vec<_> = projection
            .positions
            .into_iter()
            .map(|(key, position)| {
                let restored = self
                    .canvas
                    .graph()
                    .get_node(key)
                    .and_then(|node| old_positions.get(&node.id))
                    .map(|(x, y)| mere::kernel::geometry::PortablePoint::new(*x, *y))
                    .unwrap_or(position);
                (key, restored)
            })
            .collect();
        self.canvas
            .set_layout_strategy(Some(self.layout_id.clone()));
        self.canvas.set_projection_score(projection.score);
        self.canvas.apply_strategy_positions(&positions);
        self.canvas.note_strategy_computed(
            &self.layout_id,
            self.width,
            self.height,
            self.canvas.focused_key(),
        );
        self.canvas.set_physics_damping(self.physics_damping);
        self.canvas.set_physics_paused(self.physics_paused);
        self.canvas.set_selected_members(selected);
        self.primary_member = selected.first().copied().or_else(|| {
            self.primary_member
                .filter(|id| self.canvas.graph().get_node_by_id(*id).is_some())
        });
        if fit {
            self.canvas.fit_to_content();
        } else {
            self.canvas.set_camera(camera);
        }
        self.app.mount_local().map_err(|error| error.to_string())?;
        self.active = ActiveSession::Local;
        self.last_detail_member = None;
        Ok(())
    }

    pub(super) fn apply_saved_scene(&mut self, scene: SavedSceneV1) -> Result<(), String> {
        self.arrangement_transition = None;
        self.layout_id = scene
            .layout_strategy
            .clone()
            .unwrap_or_else(|| "phyllotaxis.default".to_string());
        self.physics_damping = scene.physics_damping;
        self.physics_paused = scene.physics_paused;
        self.handler_id = scene.default_handler.clone();
        self.canvas.set_graph(self.app.host.graph().clone());
        self.canvas
            .set_layout_strategy(Some(self.layout_id.clone()));
        let positions: Vec<_> = scene
            .cartography
            .iter()
            .filter_map(|(id, (x, y))| {
                self.canvas
                    .graph()
                    .get_node_key_by_id(id)
                    .map(|key| (key, mere::kernel::geometry::PortablePoint::new(x, y)))
            })
            .collect();
        self.canvas.apply_strategy_positions(&positions);
        self.canvas.note_strategy_computed(
            &self.layout_id,
            self.width,
            self.height,
            self.canvas.focused_key(),
        );
        self.canvas
            .apply_cartography_importance_metric(scene.cartography.importance_metric());
        self.canvas.apply_cartography_sizing(
            scene.cartography.size_iter(),
            scene.cartography.size_by_degree(),
            scene.cartography.size_by_importance(),
        );
        self.canvas
            .apply_cartography_sprites(scene.cartography.sprite_iter());
        self.canvas
            .apply_cartography_sprite_hulls(scene.cartography.sprite_hull_iter());
        self.canvas
            .apply_cartography_materials(scene.cartography.material_iter());
        self.canvas
            .apply_cartography_faces(scene.cartography.face_iter());
        self.canvas.set_arrangement_pull(scene.arrangement_pull);
        // The law, its overlays and the kind source ride the scene; an unknown id
        // (a scene from a newer catalog) falls back to the default rather than
        // failing the restore. (Physics catalog — P1.)
        self.canvas.set_physics_kind_source(
            mere::canvas::PhysicsKindSource::parse(&scene.physics_kind_source)
                .unwrap_or(mere::canvas::PhysicsKindSource::Site),
        );
        self.canvas.set_physics_mass_source(
            mere::canvas::PhysicsMassSource::parse(&scene.physics_mass_source)
                .unwrap_or(mere::canvas::PhysicsMassSource::Degree),
        );
        self.canvas.set_physics_depth_source(
            mere::canvas::PhysicsDepthSource::parse(&scene.physics_depth_source)
                .unwrap_or(mere::canvas::PhysicsDepthSource::Roots),
        );
        self.canvas.set_physics_overlays(
            scene
                .physics_overlays
                .iter()
                .filter_map(|id| mere::canvas::PhysicsOverlay::parse(id))
                .collect(),
        );
        self.canvas.set_physics_law(
            mere::canvas::PhysicsLaw::parse(&scene.physics_law)
                .unwrap_or(mere::canvas::PhysicsLaw::Springs),
        );
        self.canvas.set_physics_damping(scene.physics_damping);
        self.canvas.set_physics_paused(scene.physics_paused);
        self.canvas.set_selected_members(&scene.selected);
        self.primary_member = scene.selected.first().copied();
        self.canvas.set_camera(CameraView {
            offset: scene.camera_offset,
            zoom: scene.camera_zoom,
        });
        self.active = ActiveSession::Local;
        self.last_detail_member = None;
        Ok(())
    }

    fn add_address(&mut self) -> Result<String, String> {
        let address = input_value("address-input")?;
        let title = input_value("address-title")?;
        let id = self
            .app
            .host
            .create_address(address.trim(), title.trim())
            .map_err(|error| error.to_string())?;
        self.sync_canvas(&[id], true)?;
        self.detail_open = true;
        Ok("Address added to the local Mere graph".to_string())
    }

    fn save_metadata(&mut self) -> Result<String, String> {
        let id = self.focused_member()?;
        let title = input_value("edit-title")?;
        let tags = input_value("edit-tags")?
            .split(',')
            .map(str::trim)
            .filter(|tag| !tag.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        self.app
            .host
            .edit_node(id, &title, tags)
            .map_err(|error| error.to_string())?;
        let facet = input_value("facet-id")?;
        let value = textarea_value("facet-json")?;
        if !facet.trim().is_empty() && !value.trim().is_empty() {
            self.app
                .host
                .set_product_facet(id, &facet, &value)
                .map_err(|error| error.to_string())?;
        }
        self.sync_canvas(&[id], false)?;
        Ok("Title, tags, and facet saved".to_string())
    }

    fn add_relation(&mut self) -> Result<String, String> {
        let from = self.focused_member()?;
        let target = input_value("relation-target")?;
        let to = self
            .app
            .host
            .graph()
            .get_node_by_url(target.trim())
            .map(|(_, node)| node.id)
            .ok_or_else(|| format!("unknown relation target {}", target.trim()))?;
        let relation = EditableRelation::from_code(&select_value("relation-kind")?)
            .ok_or("unknown editable relation")?;
        self.app
            .host
            .assert_product_relation(from, to, relation)
            .map_err(|error| error.to_string())?;
        self.sync_canvas(&[from, to], false)?;
        Ok("Relation added and pair selected".to_string())
    }

    fn select_pair(&mut self) -> Result<String, String> {
        let from = self.focused_member()?;
        let target = input_value("relation-target")?;
        let to = self
            .app
            .host
            .graph()
            .get_node_by_url(target.trim())
            .map(|(_, node)| node.id)
            .ok_or_else(|| format!("unknown relation target {}", target.trim()))?;
        self.canvas.set_selected_members(&[from, to]);
        self.primary_member = Some(from);
        Ok("Relation pair selected".to_string())
    }

    fn apply_filter(&mut self) -> Result<String, String> {
        let family = RelationFamilyFilter::from_code(&select_value("relation-filter")?)
            .ok_or("unknown relation family")?;
        let query = input_value("search-input")?;
        let matches = self.app.host.matching_members(&query, family);
        self.filter_count = matches.len();
        self.relation_family = family;
        if query.trim().is_empty() && family == RelationFamilyFilter::All {
            self.canvas.clear_scope();
        } else {
            self.canvas.scope_to_members(matches.iter().copied());
        }
        Ok(format!("{} graph object(s) match", matches.len()))
    }

    fn clear_filter(&mut self) -> Result<String, String> {
        set_input_value("search-input", "")?;
        set_select_value("relation-filter", "all")?;
        self.canvas.clear_scope();
        self.filter_count = self.app.host.graph().node_count();
        self.relation_family = RelationFamilyFilter::All;
        Ok("Showing the whole graph".to_string())
    }

    fn apply_arrangement_from_form(&mut self) -> Result<String, String> {
        let next_layout_id = select_value("arrangement-select")?;
        let selected = self.canvas.selected_members();
        let previous_score = self.canvas.projection_score().cloned();
        let extents = self.canvas.strategy_extents();
        let projection = project_canvas_strategy_with_score_for_view(
            &next_layout_id,
            self.canvas.graph(),
            self.canvas.focused_key(),
            self.width,
            self.height,
            None,
            Some(&extents),
            true,
            self.canvas.camera().zoom,
            previous_score.as_ref(),
        );
        let transitioning = self.begin_arrangement_transition(&projection.positions)?;
        self.layout_id = next_layout_id;
        self.canvas
            .set_layout_strategy(Some(self.layout_id.clone()));
        self.canvas.set_projection_score(projection.score);
        if !transitioning {
            self.canvas.apply_strategy_positions(&projection.positions);
        }
        self.canvas.note_strategy_computed(
            &self.layout_id,
            self.width,
            self.height,
            self.canvas.focused_key(),
        );
        self.canvas.set_selected_members(&selected);
        if !transitioning {
            self.canvas.fit_to_content();
        }
        Ok(if transitioning {
            format!("Arrangement changing to {}", self.layout_id)
        } else {
            format!("Arrangement set to {}", self.layout_id)
        })
    }

    /// Re-evaluate only the Spiral score's representation slots after a view
    /// change. The canvas keeps its existing positions; the prior score gives
    /// the registry enough state to apply hysteresis at rung boundaries.
    pub(super) fn refresh_representation_score(&mut self) {
        if self.layout_id != "phyllotaxis.default" {
            return;
        }
        let previous_score = self.canvas.projection_score().cloned();
        let extents = self.canvas.strategy_extents();
        let projection = project_canvas_strategy_with_score_for_view(
            &self.layout_id,
            self.canvas.graph(),
            self.canvas.focused_key(),
            self.width,
            self.height,
            None,
            Some(&extents),
            true,
            self.canvas.camera().zoom,
            previous_score.as_ref(),
        );
        self.canvas.set_projection_score(projection.score);
    }

    fn toggle_physics(&mut self) -> Result<String, String> {
        self.physics_paused = !self.physics_paused;
        self.canvas.set_physics_paused(self.physics_paused);
        Ok(if self.physics_paused {
            "Physics paused".to_string()
        } else {
            "Physics running".to_string()
        })
    }

    fn apply_face(&mut self) -> Result<String, String> {
        let id = self.focused_member()?;
        let face = Face::from_code(&select_value("face-select")?);
        if face == Face::Sprite {
            self.canvas.set_node_sprite(id, DEFAULT_SPRITE.to_string());
        } else {
            self.canvas.set_node_face(id, face);
        }
        self.face = face.as_code().to_string();
        Ok(format!("Representation set to {}", face.as_code()))
    }

    fn save_scene(&mut self) -> Result<String, String> {
        let selected = {
            let selected = self.canvas.selected_members();
            if selected.is_empty() {
                vec![self.focused_member()?]
            } else {
                selected
            }
        };
        let camera = self.canvas.camera();
        let scene = SavedSceneV1 {
            name: "Graphshell working scene".to_string(),
            selected,
            layout_strategy: Some(self.layout_id.clone()),
            physics_paused: self.physics_paused,
            physics_damping: self.physics_damping,
            physics_law: self.canvas.physics_law().id().to_string(),
            physics_overlays: self
                .canvas
                .physics_overlays()
                .iter()
                .map(|overlay| overlay.id().to_string())
                .collect(),
            physics_kind_source: self.canvas.physics_kind_source().id().to_string(),
            physics_mass_source: self.canvas.physics_mass_source().id().to_string(),
            physics_depth_source: self.canvas.physics_depth_source().id().to_string(),
            arrangement_pull: self.canvas.arrangement_pull(),
            camera_offset: camera.offset,
            camera_zoom: camera.zoom,
            default_handler: select_value("handler-select")?,
            cartography: self.canvas.cartography_geometry(),
        };
        self.handler_id = scene.default_handler.clone();
        self.app
            .host
            .save_product_scene(SAVED_SCENE_ADDRESS, &scene)
            .map_err(|error| error.to_string())?;
        self.saved_scene = Some(scene.clone());
        self.sync_canvas(&scene.selected, false)?;
        self.apply_saved_scene(scene)?;
        Ok("Scene saved with selection, arrangement, physics, and representations".to_string())
    }

    fn reopen_scene(&mut self) -> Result<String, String> {
        let scene = self
            .app
            .host
            .product_scene(SAVED_SCENE_ADDRESS)
            .map_err(|error| error.to_string())?;
        self.saved_scene = Some(scene.clone());
        self.apply_saved_scene(scene)?;
        Ok("Saved scene reopened".to_string())
    }

    fn export_codicil(&mut self) -> Result<String, String> {
        let focused = self.focused_member()?;
        let scope = TransferScope::from_code(&select_value("transfer-scope")?)
            .ok_or("unknown transfer scope")?;
        let scene = if scope == TransferScope::SavedScene {
            Some(
                self.saved_scene
                    .clone()
                    .or_else(|| self.app.host.product_scene(SAVED_SCENE_ADDRESS).ok())
                    .ok_or("save a scene before exporting scene scope")?,
            )
        } else {
            self.saved_scene.clone()
        };
        let bytes = self
            .app
            .host
            .export_product_codicil(ExportRequest {
                focused,
                selected: self.canvas.selected_members(),
                scope,
                exported_at_ms: 1_700_000_000_000 + self.action_count as u64,
                include_local_file_locations: checkbox("include-local-file")?,
                scene,
            })
            .map_err(|error| error.to_string())?;
        self.last_export = String::from_utf8(bytes).map_err(|error| error.to_string())?;
        self.export_bytes = self.last_export.len();
        set_textarea_value("codicil-data", &self.last_export)?;
        Ok(format!(
            "Exported {} bytes · {}",
            self.export_bytes,
            scope.code()
        ))
    }

    fn open_codicil(&mut self) -> Result<String, String> {
        let data = textarea_value("codicil-data")?;
        let (receipt, scene) = self
            .app
            .host
            .replace_with_product_codicil(data.as_bytes())
            .map_err(|error| error.to_string())?;
        self.imported_nodes = receipt.nodes;
        let selected = scene
            .as_ref()
            .map(|scene| scene.selected.clone())
            .unwrap_or_default();
        self.sync_canvas(&selected, true)?;
        if let Some(scene) = scene {
            self.saved_scene = Some(scene.clone());
            self.apply_saved_scene(scene)?;
        }
        self.detail_open = false;
        Ok(format!(
            "Opened codicil · {} objects, {} relations, {} facets",
            receipt.nodes, receipt.relations, receipt.facets
        ))
    }

    fn focused_member(&self) -> Result<Uuid, String> {
        self.current_primary_member()
            .ok_or_else(|| "select one graph object first".to_string())
    }
}

pub(super) fn update_product_semantics(
    host: &mut BrowserHost,
    _model: &ChromeModel,
) -> Result<(), String> {
    let member = host.current_primary_member();
    if member != host.last_detail_member {
        if let Some(id) = member
            && let Some((_, node)) = host.app.host.graph().get_node_by_id(id)
        {
            set_input_value("edit-title", &node.title)?;
            let mut tags = node.tags.iter().cloned().collect::<Vec<_>>();
            tags.sort();
            set_input_value("edit-tags", &tags.join(", "))?;
            set_select_value("handler-select", &host.handler_id)?;
            set_select_value("arrangement-select", &host.layout_id)?;
            set_select_value("face-select", &host.face)?;
        }
        host.last_detail_member = member;
    }
    if let Ok(element) = element("product-status") {
        element.set_text_content(Some(&host.product_status));
    }
    let body = root()?;
    for (name, value) in [
        ("data-product-status", host.product_status.clone()),
        (
            "data-node-count",
            host.app.host.graph().node_count().to_string(),
        ),
        ("data-filter-count", host.filter_count.to_string()),
        ("data-layout", host.layout_id.clone()),
        ("data-physics-paused", host.physics_paused.to_string()),
        (
            "data-selected-count",
            host.canvas.selected_members().len().to_string(),
        ),
        ("data-export-bytes", host.export_bytes.to_string()),
        ("data-imported-nodes", host.imported_nodes.to_string()),
        (
            "data-relation-family",
            host.relation_family.code().to_string(),
        ),
        ("data-face", host.face.clone()),
    ] {
        body.set_attribute(name, &value)
            .map_err(|_| format!("could not expose {name}"))?;
    }
    Ok(())
}

async fn read_file_metadata(file: &File) -> Result<LocalFileMetadata, String> {
    let buffer = JsFuture::from(file.array_buffer())
        .await
        .map_err(|_| "the selected file could not be read")?;
    let bytes = js_sys::Uint8Array::new(&buffer).to_vec();
    let hash = Sha256::digest(bytes);
    let content_hash = hash.iter().map(|byte| format!("{byte:02x}")).collect();
    Ok(LocalFileMetadata {
        content_hash,
        name: file.name(),
        media_type: file.type_(),
        byte_len: file.size() as u64,
        last_modified_ms: file.last_modified() as u64,
    })
}

fn element_as<T: JsCast>(id: &str) -> Result<T, String> {
    element(id)?
        .dyn_into()
        .map_err(|_| format!("part {id} has the wrong element type"))
}

fn input_value(id: &str) -> Result<String, String> {
    Ok(element_as::<HtmlInputElement>(id)?.value())
}

fn set_input_value(id: &str, value: &str) -> Result<(), String> {
    element_as::<HtmlInputElement>(id)?.set_value(value);
    Ok(())
}

fn select_value(id: &str) -> Result<String, String> {
    Ok(element_as::<HtmlSelectElement>(id)?.value())
}

pub(super) fn selected_handler() -> Result<String, String> {
    select_value("handler-select")
}

fn set_select_value(id: &str, value: &str) -> Result<(), String> {
    element_as::<HtmlSelectElement>(id)?.set_value(value);
    Ok(())
}

fn textarea_value(id: &str) -> Result<String, String> {
    Ok(element_as::<HtmlTextAreaElement>(id)?.value())
}

fn set_textarea_value(id: &str, value: &str) -> Result<(), String> {
    element_as::<HtmlTextAreaElement>(id)?.set_value(value);
    Ok(())
}

fn checkbox(id: &str) -> Result<bool, String> {
    Ok(element_as::<HtmlInputElement>(id)?.checked())
}
