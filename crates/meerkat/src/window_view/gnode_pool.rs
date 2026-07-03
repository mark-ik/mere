/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Host-owned retained DOM pool for the focused orrery's `.gnode` elements.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use layout_dom_api::{LayoutDom, LayoutDomMut, Namespace};
use serval_scripted_dom::{NodeId, ScriptedDom};
use xilem_serval::{attr_qual, html_qual};

use super::*;

pub(crate) const ORRERY_GNODE_POOL_ID: &str = "orrery-gnodes";
pub(crate) const ORRERY_GNODE_POOL_CLASS: &str = "orrery-gnode-pool";

const GNODE_LABEL_CAP: usize = 24;

#[derive(Default)]
pub(crate) struct GnodePool {
    root: Option<NodeId>,
    bound_graph: Option<GraphId>,
    entries: HashMap<GraphMemberId, GnodeEntry>,
    stable_cache: HashMap<GraphMemberId, StableMemberCache>,
}

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GnodePoolStats {
    pub(crate) structural_inserts: usize,
    pub(crate) structural_removes: usize,
    pub(crate) hot_attr_writes: usize,
    pub(crate) stable_attr_writes: usize,
}

#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GnodeBuildStats {
    pub(crate) favicon_encodes: usize,
}

struct GnodeEntry {
    root: NodeId,
    image: NodeId,
    label: NodeId,
    label_text: NodeId,
    wash: NodeId,
    hot: Option<GnodeHotRow>,
    stable: Option<GnodeStableRow>,
}

#[derive(Clone, PartialEq)]
pub(crate) struct GnodeSnapshot {
    pub(crate) member: GraphMemberId,
    pub(crate) hot: GnodeHotRow,
    pub(crate) stable: GnodeStableRow,
}

#[derive(Clone, PartialEq)]
pub(crate) struct GnodeHotRow {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) color: &'static str,
    pub(crate) selected: bool,
    pub(crate) hovered: bool,
    pub(crate) size: f32,
}

#[derive(Clone, PartialEq)]
pub(crate) struct GnodeStableRow {
    pub(crate) label: Arc<str>,
    pub(crate) radius: &'static str,
    pub(crate) image_uri: Option<Arc<str>>,
    pub(crate) image_cover: bool,
    pub(crate) show_label: bool,
    pub(crate) hull: Arc<[(f32, f32)]>,
}

#[derive(Default)]
struct StableMemberCache {
    label_source: String,
    label_value: Arc<str>,
    favicon: Option<CachedFavicon>,
    sprite_source: Option<String>,
    sprite_value: Option<Arc<str>>,
    hull_source: Vec<(f32, f32)>,
    hull_value: Arc<[(f32, f32)]>,
}

struct CachedFavicon {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    uri: Arc<str>,
}

#[derive(Clone, Copy)]
enum WriteClass {
    Hot,
    Stable,
}

impl GnodePool {
    pub(crate) fn cached_label(&mut self, member: GraphMemberId, title: &str) -> Arc<str> {
        let cache = self.stable_cache.entry(member).or_default();
        if cache.label_source == title {
            return Arc::clone(&cache.label_value);
        }
        cache.label_source.clear();
        cache.label_source.push_str(title);
        cache.label_value = Arc::<str>::from(derive_label(title));
        Arc::clone(&cache.label_value)
    }

    pub(crate) fn cached_favicon(
        &mut self,
        member: GraphMemberId,
        rgba: Option<&[u8]>,
        width: u32,
        height: u32,
        build: &mut GnodeBuildStats,
    ) -> Option<Arc<str>> {
        let cache = self.stable_cache.entry(member).or_default();
        let Some(rgba) = rgba else {
            cache.favicon = None;
            return None;
        };
        if let Some(cached) = cache.favicon.as_ref() {
            if cached.width == width && cached.height == height && cached.rgba.as_slice() == rgba {
                return Some(Arc::clone(&cached.uri));
            }
        }
        let uri = crate::render::favicon_data_uri(rgba, width, height)?;
        build.favicon_encodes += 1;
        let uri = Arc::<str>::from(uri);
        cache.favicon = Some(CachedFavicon {
            rgba: rgba.to_vec(),
            width,
            height,
            uri: Arc::clone(&uri),
        });
        Some(uri)
    }

    pub(crate) fn cached_sprite(
        &mut self,
        member: GraphMemberId,
        sprite: Option<&str>,
    ) -> Option<Arc<str>> {
        let cache = self.stable_cache.entry(member).or_default();
        let Some(sprite) = sprite else {
            cache.sprite_source = None;
            cache.sprite_value = None;
            return None;
        };
        if cache.sprite_source.as_deref() == Some(sprite) {
            return cache.sprite_value.as_ref().map(Arc::clone);
        }
        cache.sprite_source = Some(sprite.to_string());
        let sprite = Arc::<str>::from(sprite);
        cache.sprite_value = Some(Arc::clone(&sprite));
        Some(sprite)
    }

    pub(crate) fn cached_hull(
        &mut self,
        member: GraphMemberId,
        hull: Option<&[(f32, f32)]>,
    ) -> Arc<[(f32, f32)]> {
        let cache = self.stable_cache.entry(member).or_default();
        let hull = hull.unwrap_or(&[]);
        if cache.hull_source.as_slice() == hull {
            return Arc::clone(&cache.hull_value);
        }
        cache.hull_source.clear();
        cache.hull_source.extend_from_slice(hull);
        cache.hull_value = Arc::from(cache.hull_source.as_slice());
        Arc::clone(&cache.hull_value)
    }

    pub(crate) fn reconcile<I>(
        &mut self,
        dom: &Rc<RefCell<ScriptedDom>>,
        graph: GraphId,
        nodes: I,
    ) -> GnodePoolStats
    where
        I: IntoIterator<Item = GnodeSnapshot>,
    {
        let root = {
            let d = dom.borrow();
            if self
                .root
                .is_some_and(|node| d.is_live(node) && d.has_class(node, ORRERY_GNODE_POOL_CLASS))
            {
                self.root
            } else {
                crate::first_with_class(&d, d.document(), ORRERY_GNODE_POOL_CLASS)
            }
        };
        let Some(root) = root else {
            self.root = None;
            self.bound_graph = None;
            self.entries.clear();
            self.stable_cache.clear();
            return GnodePoolStats::default();
        };

        if self.root != Some(root) || self.bound_graph != Some(graph) {
            let mut d = dom.borrow_mut();
            self.clear_entries(&mut d);
            self.root = Some(root);
            self.bound_graph = Some(graph);
        }

        let mut d = dom.borrow_mut();
        let mut stats = GnodePoolStats::default();
        let mut seen = HashSet::new();
        for node in nodes {
            seen.insert(node.member);
            let entry = self.entries.entry(node.member).or_insert_with(|| {
                stats.structural_inserts += 1;
                Self::create_entry(&mut d, root, node.member)
            });
            Self::update_entry(&mut d, entry, &node, &mut stats);
        }

        let stale: Vec<_> = self
            .entries
            .keys()
            .copied()
            .filter(|member| !seen.contains(member))
            .collect();
        for member in stale {
            self.stable_cache.remove(&member);
            if let Some(entry) = self.entries.remove(&member) {
                if d.is_live(entry.root) {
                    d.remove(entry.root);
                    stats.structural_removes += 1;
                }
            }
        }
        stats
    }

    fn clear_entries(&mut self, dom: &mut ScriptedDom) {
        for (_, entry) in self.entries.drain() {
            if dom.is_live(entry.root) {
                dom.remove(entry.root);
            }
        }
    }

    fn create_entry(dom: &mut ScriptedDom, parent: NodeId, member: GraphMemberId) -> GnodeEntry {
        let root = dom.create_element(html_qual("div"));
        dom.set_attribute(root, attr_qual("class"), "gnode");
        dom.set_attribute(root, attr_qual("data-member"), &member.to_string());

        let image = dom.create_element(html_qual("img"));
        let label = dom.create_element(html_qual("span"));
        let label_text = dom.create_text("");
        dom.append_child(label, label_text);
        let wash = dom.create_element(html_qual("div"));

        dom.append_child(root, image);
        dom.append_child(root, label);
        dom.append_child(root, wash);
        dom.append_child(parent, root);

        GnodeEntry {
            root,
            image,
            label,
            label_text,
            wash,
            hot: None,
            stable: None,
        }
    }

    fn update_entry(
        dom: &mut ScriptedDom,
        entry: &mut GnodeEntry,
        node: &GnodeSnapshot,
        stats: &mut GnodePoolStats,
    ) {
        let prev_hot = entry.hot.as_ref();
        let prev_stable = entry.stable.as_ref();
        let root_changed = prev_hot != Some(&node.hot)
            || match prev_stable {
                None => true,
                Some(prev) => prev.radius != node.stable.radius || prev.hull != node.stable.hull,
            };
        if root_changed {
            Self::set_attribute_if_changed(
                dom,
                entry.root,
                "style",
                &root_style(&node.hot, &node.stable),
                WriteClass::Hot,
                stats,
            );
        }

        let image_changed = match prev_hot {
            None => true,
            Some(prev) => prev.size != node.hot.size,
        } || match prev_stable {
            None => true,
            Some(prev) => {
                prev.radius != node.stable.radius
                    || prev.image_uri != node.stable.image_uri
                    || prev.image_cover != node.stable.image_cover
            }
        };
        if image_changed {
            match node.stable.image_uri.as_ref() {
                Some(uri) => {
                    Self::set_attribute_if_changed(
                        dom,
                        entry.image,
                        "src",
                        uri,
                        WriteClass::Stable,
                        stats,
                    );
                    Self::set_attribute_if_changed(
                        dom,
                        entry.image,
                        "style",
                        &image_style(&node.hot, &node.stable),
                        WriteClass::Stable,
                        stats,
                    );
                }
                None => {
                    Self::remove_attribute_if_present(
                        dom,
                        entry.image,
                        "src",
                        WriteClass::Stable,
                        stats,
                    );
                    Self::set_attribute_if_changed(
                        dom,
                        entry.image,
                        "style",
                        "display:none",
                        WriteClass::Stable,
                        stats,
                    );
                }
            }
        }

        if match prev_stable {
            None => true,
            Some(prev) => prev.label != node.stable.label,
        } {
            Self::set_text_if_changed(
                dom,
                entry.label_text,
                &node.stable.label,
                WriteClass::Stable,
                stats,
            );
        }
        let label_changed = match prev_hot {
            None => true,
            Some(prev) => prev.size != node.hot.size,
        } || match prev_stable {
            None => true,
            Some(prev) => prev.show_label != node.stable.show_label,
        };
        if label_changed {
            Self::set_attribute_if_changed(
                dom,
                entry.label,
                "style",
                &label_style(&node.hot, &node.stable),
                WriteClass::Hot,
                stats,
            );
        }

        let wash_changed = match prev_hot {
            None => true,
            Some(prev) => prev.size != node.hot.size || prev.hovered != node.hot.hovered,
        } || match prev_stable {
            None => true,
            Some(prev) => prev.radius != node.stable.radius,
        };
        if wash_changed {
            Self::set_attribute_if_changed(
                dom,
                entry.wash,
                "style",
                &wash_style(&node.hot, &node.stable),
                WriteClass::Hot,
                stats,
            );
        }

        entry.hot = Some(node.hot.clone());
        entry.stable = Some(node.stable.clone());
    }

    fn set_attribute_if_changed(
        dom: &mut ScriptedDom,
        node: NodeId,
        name: &str,
        value: &str,
        class: WriteClass,
        stats: &mut GnodePoolStats,
    ) {
        if dom.attribute(node, &Namespace::from(""), &name.into()) == Some(value) {
            return;
        }
        dom.set_attribute(node, attr_qual(name), value);
        match class {
            WriteClass::Hot => stats.hot_attr_writes += 1,
            WriteClass::Stable => stats.stable_attr_writes += 1,
        }
    }

    fn remove_attribute_if_present(
        dom: &mut ScriptedDom,
        node: NodeId,
        name: &str,
        class: WriteClass,
        stats: &mut GnodePoolStats,
    ) {
        if dom
            .attribute(node, &Namespace::from(""), &name.into())
            .is_none()
        {
            return;
        }
        dom.remove_attribute(node, attr_qual(name));
        match class {
            WriteClass::Hot => stats.hot_attr_writes += 1,
            WriteClass::Stable => stats.stable_attr_writes += 1,
        }
    }

    fn set_text_if_changed(
        dom: &mut ScriptedDom,
        node: NodeId,
        value: &str,
        class: WriteClass,
        stats: &mut GnodePoolStats,
    ) {
        if dom.text(node) == Some(value) {
            return;
        }
        dom.set_text(node, value);
        match class {
            WriteClass::Hot => stats.hot_attr_writes += 1,
            WriteClass::Stable => stats.stable_attr_writes += 1,
        }
    }
}

fn derive_label(title: &str) -> String {
    let raw = title.trim_end_matches('/');
    let base = if raw.contains("://") {
        match raw.rsplit('/').next() {
            Some(slug) if !slug.is_empty() => slug,
            _ => raw,
        }
    } else {
        raw
    };
    if base.chars().count() <= GNODE_LABEL_CAP {
        base.to_string()
    } else {
        base.chars()
            .take(GNODE_LABEL_CAP - 1)
            .chain(['\u{2026}'])
            .collect()
    }
}

fn root_style(hot: &GnodeHotRow, stable: &GnodeStableRow) -> String {
    let face = hot.size;
    const LIFT: f32 = 4.0;
    let size = if hot.selected { face + LIFT } else { face };
    let half = size / 2.0;
    let (cx, cy) = (hot.x - half, hot.y - half);
    let ring = if hot.selected {
        "box-shadow:0 0 0 2px #ffffff,0 3px 10px rgba(0,0,0,0.6);"
    } else {
        "box-shadow:0 1px 3px rgba(0,0,0,0.45);"
    };
    let shape_style = if stable.hull.len() >= 3 {
        let pts: Vec<String> = stable
            .hull
            .iter()
            .map(|&(nx, ny)| format!("{:.2}% {:.2}%", (nx + 0.5) * 100.0, (ny + 0.5) * 100.0))
            .collect();
        format!("clip-path:polygon({});{ring}", pts.join(", "))
    } else {
        format!("border-radius:{};{ring}", stable.radius)
    };
    format!(
        "position:absolute;left:0;top:0;transform:translate({cx}px,{cy}px);width:{size}px;\
         height:{size}px;box-sizing:border-box;background-color:{};{shape_style}",
        hot.color
    )
}

fn image_style(hot: &GnodeHotRow, stable: &GnodeStableRow) -> String {
    let size = if hot.selected {
        hot.size + 4.0
    } else {
        hot.size
    };
    let fit = if stable.image_cover {
        "object-fit:cover;"
    } else {
        ""
    };
    format!(
        "position:absolute;left:0;top:0;width:{size}px;height:{size}px;border-radius:{};{fit}display:block",
        stable.radius
    )
}

fn label_style(hot: &GnodeHotRow, stable: &GnodeStableRow) -> String {
    let size = if hot.selected {
        hot.size + 4.0
    } else {
        hot.size
    };
    if stable.show_label {
        format!(
            "position:absolute;left:{}px;top:{}px;white-space:nowrap;color:#d8deea;\
             font-size:14px;font-weight:500;display:block",
            size + 6.0,
            (size / 2.0 - 8.0).max(0.0)
        )
    } else {
        "display:none".to_string()
    }
}

fn wash_style(hot: &GnodeHotRow, stable: &GnodeStableRow) -> String {
    let size = if hot.selected {
        hot.size + 4.0
    } else {
        hot.size
    };
    if hot.hovered {
        format!(
            "position:absolute;left:0;top:0;width:{size}px;height:{size}px;border-radius:{};\
             background-color:rgba(255,255,255,0.16);pointer-events:none;display:block",
            stable.radius
        )
    } else {
        "display:none".to_string()
    }
}

#[cfg(test)]
mod tests {
    use layout_dom_api::{Namespace, NodeKind};

    use super::*;

    fn attr<'a>(dom: &'a ScriptedDom, node: NodeId, name: &str) -> Option<&'a str> {
        dom.attribute(node, &Namespace::from(""), &name.into())
    }

    fn snapshot(
        member: GraphMemberId,
        label: &str,
        x: f32,
        y: f32,
        color: &'static str,
        selected: bool,
        hovered: bool,
        size: f32,
        radius: &'static str,
        image_uri: Option<&str>,
        image_cover: bool,
        show_label: bool,
        hull: &[(f32, f32)],
    ) -> GnodeSnapshot {
        GnodeSnapshot {
            member,
            hot: GnodeHotRow {
                x,
                y,
                color,
                selected,
                hovered,
                size,
            },
            stable: GnodeStableRow {
                label: Arc::from(label),
                radius,
                image_uri: image_uri.map(Arc::from),
                image_cover,
                show_label,
                hull: Arc::from(hull),
            },
        }
    }

    #[test]
    fn reconcile_reuses_existing_gnode_dom_nodes() {
        let dom = Rc::new(RefCell::new(ScriptedDom::new()));
        let pool_root = {
            let mut d = dom.borrow_mut();
            let root = d.create_element(html_qual("div"));
            d.set_attribute(root, attr_qual("class"), ORRERY_GNODE_POOL_CLASS);
            let doc = d.document();
            d.append_child(doc, root);
            root
        };
        let mut pool = GnodePool::default();
        let graph = GraphId::new();
        let member = uuid::Uuid::new_v4();

        let first_stats = pool.reconcile(
            &dom,
            graph,
            [snapshot(
                member,
                "Bird",
                10.0,
                20.0,
                "#5a8fc8",
                false,
                false,
                36.0,
                "50%",
                None,
                false,
                true,
                &[],
            )],
        );
        assert_eq!(first_stats.structural_inserts, 1);

        let second_stats = pool.reconcile(
            &dom,
            graph,
            [snapshot(
                member,
                "Bird 2",
                30.0,
                40.0,
                "#5fb878",
                true,
                true,
                40.0,
                "9px",
                Some("data:image/png;base64,AA=="),
                true,
                true,
                &[],
            )],
        );

        let first_root = pool.entries.get(&member).expect("entry").root;
        let d = dom.borrow();
        let pool_children: Vec<_> = d.dom_children(pool_root).collect();
        assert_eq!(
            pool_children,
            vec![first_root],
            "the same gnode root is retained"
        );
        let label = pool.entries.get(&member).expect("entry").label;
        let label_text = d
            .dom_children(label)
            .find(|&child| d.kind(child) == NodeKind::Text)
            .expect("label text node");
        assert_eq!(d.text(label_text), Some("Bird 2"));
        assert!(
            attr(&d, first_root, "style")
                .is_some_and(|style| style.contains("translate(8px,18px)")),
            "the retained node gets its new transform"
        );
        assert!(
            attr(&d, pool.entries.get(&member).expect("entry").image, "src").is_some(),
            "the retained image node picks up the new face image",
        );
        assert!(
            second_stats.hot_attr_writes > 0 || second_stats.stable_attr_writes > 0,
            "changing the retained row emits writes"
        );
    }

    #[test]
    fn reconcile_suppresses_unchanged_writes() {
        let dom = Rc::new(RefCell::new(ScriptedDom::new()));
        {
            let mut d = dom.borrow_mut();
            let root = d.create_element(html_qual("div"));
            d.set_attribute(root, attr_qual("class"), ORRERY_GNODE_POOL_CLASS);
            let doc = d.document();
            d.append_child(doc, root);
        }
        let mut pool = GnodePool::default();
        let graph = GraphId::new();
        let member = uuid::Uuid::new_v4();
        let node = snapshot(
            member,
            "Bird",
            10.0,
            20.0,
            "#5a8fc8",
            false,
            false,
            36.0,
            "50%",
            Some("data:image/png;base64,AA=="),
            true,
            true,
            &[],
        );

        let first = pool.reconcile(&dom, graph, [node.clone()]);
        let second = pool.reconcile(&dom, graph, [node]);
        assert_eq!(first.structural_inserts, 1);
        assert_eq!(second.structural_inserts, 0);
        assert_eq!(second.structural_removes, 0);
        assert_eq!(second.hot_attr_writes, 0);
        assert_eq!(second.stable_attr_writes, 0);
    }

    #[test]
    fn stable_caches_reuse_label_favicon_sprite_and_hull() {
        let mut pool = GnodePool::default();
        let member = uuid::Uuid::new_v4();
        let mut build = GnodeBuildStats::default();

        let label_1 = pool.cached_label(member, "https://example.test/path/to/article");
        let label_2 = pool.cached_label(member, "https://example.test/path/to/article");
        assert!(Arc::ptr_eq(&label_1, &label_2));

        let favicon_rgba = vec![0xff, 0x00, 0x00, 0xff];
        let fav_1 = pool
            .cached_favicon(member, Some(&favicon_rgba), 1, 1, &mut build)
            .expect("favicon");
        let fav_2 = pool
            .cached_favicon(member, Some(&favicon_rgba), 1, 1, &mut build)
            .expect("favicon");
        assert!(Arc::ptr_eq(&fav_1, &fav_2));
        assert_eq!(build.favicon_encodes, 1, "favicon encode is cached");

        let sprite_1 = pool
            .cached_sprite(member, Some("data:image/png;base64,AAAA"))
            .expect("sprite");
        let sprite_2 = pool
            .cached_sprite(member, Some("data:image/png;base64,AAAA"))
            .expect("sprite");
        assert!(Arc::ptr_eq(&sprite_1, &sprite_2));

        let hull = [(-0.4, -0.4), (0.4, -0.4), (0.0, 0.4)];
        let hull_1 = pool.cached_hull(member, Some(&hull));
        let hull_2 = pool.cached_hull(member, Some(&hull));
        assert!(Arc::ptr_eq(&hull_1, &hull_2));
    }
}
