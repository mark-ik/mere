/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Flip/navigation restore helpers for scrying-hosted web surfaces.

use mere::forme::GraphMemberId;
use inker::{Cookie, NavigationEvent, SameSite, WebSurface};
use verso_tile::scry::{NavSignal, ScryForward, ScrySurface};

use super::windows_pool::Tile;

/// The `verso-scry` `ScrySurface` seam over any inker web surface. This keeps
/// flip restore generic: cookies, navigation, and script restore no longer know
/// whether the tile came from scrying, weld, graft, or a future producer.
struct ProducerSurface<'a>(&'a mut dyn WebSurface);

impl ScrySurface for ProducerSurface<'_> {
    fn set_cookie(&mut self, cookie: &verso_tile::api::Cookie) -> Result<(), String> {
        self.0
            .set_cookie(&Cookie {
                name: cookie.name.clone(),
                value: cookie.value.clone(),
                domain: cookie.domain.clone(),
                path: cookie.path.clone(),
                secure: cookie.secure,
                http_only: cookie.http_only,
                same_site: cookie.same_site.map(map_same_site),
                expires: cookie.expires,
                partitioned: cookie.partitioned,
            })
            .map_err(|err| err.to_string())
    }

    fn navigate(&mut self, url: &str) -> Result<(), String> {
        self.0.navigate_to_url(url).map_err(|err| err.to_string())
    }

    fn run_script(&mut self, js: &str) -> Result<String, String> {
        self.0
            .execute_script_with_result(js)
            .map_err(|err| err.to_string())
    }
}

fn map_same_site(same_site: verso_tile::api::SameSite) -> SameSite {
    match same_site {
        verso_tile::api::SameSite::Strict => SameSite::Strict,
        verso_tile::api::SameSite::Lax => SameSite::Lax,
        verso_tile::api::SameSite::None => SameSite::None,
    }
}

/// Map an inker nav event to the signal the flip waits on.
fn nav_signal(event: &NavigationEvent) -> Option<NavSignal> {
    match event {
        NavigationEvent::Started { .. } => Some(NavSignal::Started),
        NavigationEvent::Finished { .. } => Some(NavSignal::Completed { success: true }),
        NavigationEvent::Failed { .. } => Some(NavSignal::Completed { success: false }),
        NavigationEvent::Committed { .. } => None,
    }
}

pub(super) fn drive_navigation(
    member: GraphMemberId,
    tile: &mut Tile,
    url: &str,
    width: u32,
    height: u32,
    pending_flip: Option<ScryForward>,
) {
    if tile.size != (width, height) {
        match tile.producer.resize(width, height) {
            Ok(()) => tile.size = (width, height),
            Err(err) => tile.last_error = Some(format!("resize: {err}")),
        }
    }

    if let Some(mut flip) = pending_flip {
        match tile.producer.as_web_surface() {
            Some(web) => {
                let mut surface = ProducerSurface(web);
                if let Err(err) = flip.begin(&mut surface) {
                    tile.last_error = Some(format!("flip begin: {err}"));
                }
                tile.shown_url = Some(url.to_string());
                if !flip.is_done() {
                    tile.flip = Some(flip);
                }
            }
            None => tile.last_error = Some("flip begin: surface has no web control".into()),
        }
    } else if tile.shown_url.as_deref() != Some(url) {
        match tile.producer.as_web_surface() {
            Some(web) => match web.navigate_to_url(url) {
                Ok(()) => tile.shown_url = Some(url.to_string()),
                Err(err) => tile.last_error = Some(format!("navigate: {err}")),
            },
            None => tile.last_error = Some("navigate: surface has no web control".into()),
        }
    }

    if tile.flip.is_some() {
        let mut signals = Vec::new();
        if let Some(web) = tile.producer.as_web_surface() {
            while let Some(event) = web.poll_navigation_event() {
                if let Some(sig) = nav_signal(&event) {
                    signals.push(sig);
                }
            }
        }
        if let Some(mut flip) = tile.flip.take() {
            if !signals.is_empty() {
                if let Some(web) = tile.producer.as_web_surface() {
                    let mut surface = ProducerSurface(web);
                    for sig in signals {
                        flip.on_nav(sig, &mut surface);
                    }
                } else {
                    tile.last_error = Some("flip restore: surface has no web control".into());
                }
            }
            if !flip.is_done() {
                tile.flip = Some(flip);
            }
        }
    } else {
        let _ = member;
    }
}
