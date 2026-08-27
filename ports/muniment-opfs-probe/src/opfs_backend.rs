// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! redb's [`StorageBackend`] over one OPFS `FileSystemSyncAccessHandle`,
//! kept worker-local.
//!
//! The honest answer to redb's `Send + Sync` bound: the value redb owns holds
//! no JS value at all. It carries a **realm-qualified token** into a
//! `thread_local!` registry where the sync-access handle lives, so
//! [`OpfsBackend`] is `Send + Sync` by construction, with no `unsafe impl`.
//!
//! The realm qualifier is what makes the claim fail-closed rather than merely
//! true-today. A bare index would be ambiguous across threads: thread B's
//! registry has its own vector, and slot `n` there is a *different* file, so a
//! backend that migrated to B would silently address the wrong database. Each
//! thread's registry draws a distinct id from a process-global counter at
//! first use, the token carries it, and a lookup whose realm does not match
//! the calling thread's returns [`ErrorKind::NotConnected`]. That holds under
//! any build, including a future `+atomics` one where real threads exist.
//!
//! wasm-bindgen itself declares `JsValue: Send + Sync` under
//! `cfg(not(target_feature = "atomics"))`, so a struct holding the handle
//! directly would also compile on today's target. The registry is chosen so
//! the claim is ours by construction rather than upstream's by assertion, and
//! so nothing above this file can reach the browser handle: the only browser
//! authority redb ever sees is six storage calls.

use std::cell::RefCell;
use std::fmt;
use std::io::{self, ErrorKind};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use redb::StorageBackend;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    DomException, FileSystemDirectoryHandle, FileSystemFileHandle, FileSystemGetDirectoryOptions,
    FileSystemGetFileOptions, FileSystemReadWriteOptions, FileSystemRemoveOptions,
    FileSystemSyncAccessHandle, WorkerGlobalScope,
};

use crate::IoStats;

/// `FileSystemSyncAccessHandle` takes and returns `double`, so every offset
/// and length crossing the JS boundary must be exactly representable. Beyond
/// this a conversion would silently round, which on a *write offset* means
/// writing to the wrong place. redb never reaches it (a 9 PB database is not
/// a browser concern), so this is a guard, not a limit anyone will meet.
const JS_SAFE_INTEGER: u64 = (1u64 << 53) - 1;

/// A u64 that JS can hold exactly, or an error naming what overflowed.
fn exact_f64(what: &str, value: u64) -> io::Result<f64> {
    if value > JS_SAFE_INTEGER {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            format!("{what} {value} exceeds JavaScript's safe integer range ({JS_SAFE_INTEGER})"),
        ));
    }
    Ok(value as f64)
}

/// A JS `double` back to a u64, refusing anything not a whole number in range.
fn exact_u64(what: &str, value: f64) -> io::Result<u64> {
    if !value.is_finite() || value < 0.0 || value.fract() != 0.0 || value > JS_SAFE_INTEGER as f64 {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            format!("{what} returned {value}, which is not an exact non-negative integer"),
        ));
    }
    Ok(value as u64)
}

/// Distinct id per thread-local registry instantiation.
static NEXT_REALM: AtomicU64 = AtomicU64::new(1);

struct Registry {
    /// This thread's realm. Tokens minted here carry it; a token presented on
    /// another thread carries a different one and is refused.
    realm: u64,
    handles: Vec<Option<FileSystemSyncAccessHandle>>,
}

impl Registry {
    fn new() -> Self {
        Self {
            realm: NEXT_REALM.fetch_add(1, Ordering::Relaxed),
            handles: Vec::new(),
        }
    }
}

thread_local! {
    /// Sync-access handles opened on this thread. A handle never leaves this
    /// table; an `OpfsBackend` holds only a realm-qualified index.
    static REGISTRY: RefCell<Registry> = RefCell::new(Registry::new());
}

/// Which registry slot, in which thread's registry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Token {
    realm: u64,
    index: usize,
}

/// Storage-call counters, shared with whoever opened the backend so they can
/// be read after redb has taken ownership.
#[derive(Debug, Default)]
pub struct IoCounters {
    reads: AtomicU64,
    writes: AtomicU64,
    set_lens: AtomicU64,
    syncs: AtomicU64,
    bytes_read: AtomicU64,
    bytes_written: AtomicU64,
}

impl IoCounters {
    pub fn snapshot(&self) -> IoStats {
        IoStats {
            reads: self.reads.load(Ordering::Relaxed),
            writes: self.writes.load(Ordering::Relaxed),
            set_lens: self.set_lens.load(Ordering::Relaxed),
            syncs: self.syncs.load(Ordering::Relaxed),
            bytes_read: self.bytes_read.load(Ordering::Relaxed),
            bytes_written: self.bytes_written.load(Ordering::Relaxed),
        }
    }
}

/// The value redb owns: a realm-qualified token, a path for diagnostics, and
/// counters. No JS value, no `unsafe`.
pub struct OpfsBackend {
    token: Token,
    path: String,
    counters: Arc<IoCounters>,
}

// The bound redb demands, met without an unsafe impl: every field is plain
// data. If this stops compiling, something browser-owned has leaked into the
// struct.
const _: () = {
    const fn assert_send_sync<T: Send + Sync + 'static>() {}
    assert_send_sync::<OpfsBackend>();
};

impl fmt::Debug for OpfsBackend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OpfsBackend")
            .field("path", &self.path)
            .field("realm", &self.token.realm)
            .field("index", &self.token.index)
            .finish()
    }
}

/// A browser error, carried inside an `io::Error` so callers can recover the
/// DOMException name.
#[derive(Debug)]
pub struct JsError {
    pub context: String,
    pub name: String,
    pub message: String,
}

impl fmt::Display for JsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.name.is_empty() {
            write!(f, "{}: {}", self.context, self.message)
        } else {
            write!(f, "{}: {}: {}", self.context, self.name, self.message)
        }
    }
}

impl std::error::Error for JsError {}

/// Map a JS error to an `io::Error`, keeping the DOMException name.
pub fn js_io(context: &str, err: JsValue) -> io::Error {
    let (name, message) = match err.dyn_ref::<DomException>() {
        Some(exception) => (exception.name(), exception.message()),
        None => (
            String::new(),
            err.as_string().unwrap_or_else(|| format!("{err:?}")),
        ),
    };
    let kind = match name.as_str() {
        "QuotaExceededError" => ErrorKind::QuotaExceeded,
        "NotFoundError" => ErrorKind::NotFound,
        // Another sync-access handle is open on the file: the browser's own
        // exclusivity, the refusal a second writer must get.
        "NoModificationAllowedError" => ErrorKind::WouldBlock,
        "InvalidStateError" => ErrorKind::BrokenPipe,
        "NotAllowedError" | "SecurityError" => ErrorKind::PermissionDenied,
        "TypeMismatchError" => ErrorKind::InvalidInput,
        _ => ErrorKind::Other,
    };
    io::Error::new(
        kind,
        JsError {
            context: context.to_string(),
            name,
            message,
        },
    )
}

/// The DOMException name inside an error produced by [`js_io`], if any.
pub fn dom_exception_name(err: &io::Error) -> Option<String> {
    err.get_ref()?
        .downcast_ref::<JsError>()
        .map(|e| e.name.clone())
        .filter(|name| !name.is_empty())
}

fn scope() -> WorkerGlobalScope {
    js_sys::global().unchecked_into()
}

async fn root() -> io::Result<FileSystemDirectoryHandle> {
    let value = JsFuture::from(scope().navigator().storage().get_directory())
        .await
        .map_err(|e| js_io("navigator.storage.getDirectory", e))?;
    Ok(value.unchecked_into())
}

fn split(path: &str) -> io::Result<(Vec<&str>, &str)> {
    let mut segments: Vec<&str> = path.split('/').collect();
    let name = segments
        .pop()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| {
            io::Error::new(
                ErrorKind::InvalidInput,
                format!("empty file name in {path:?}"),
            )
        })?;
    if segments.iter().any(|segment| segment.is_empty()) {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            format!("empty directory segment in {path:?}"),
        ));
    }
    Ok((segments, name))
}

async fn directory(segments: &[&str], create: bool) -> io::Result<FileSystemDirectoryHandle> {
    let mut dir = root().await?;
    for segment in segments {
        let options = FileSystemGetDirectoryOptions::new();
        options.set_create(create);
        let value = JsFuture::from(dir.get_directory_handle_with_options(segment, &options))
            .await
            .map_err(|e| js_io(&format!("getDirectoryHandle({segment})"), e))?;
        dir = value.unchecked_into();
    }
    Ok(dir)
}

async fn file_handle(
    path: &str,
    create: bool,
) -> io::Result<(FileSystemDirectoryHandle, FileSystemFileHandle, String)> {
    let (segments, name) = split(path)?;
    let dir = directory(&segments, create).await?;
    let options = FileSystemGetFileOptions::new();
    options.set_create(create);
    let value = JsFuture::from(dir.get_file_handle_with_options(name, &options))
        .await
        .map_err(|e| js_io(&format!("getFileHandle({name})"), e))?;
    Ok((dir, value.unchecked_into(), name.to_string()))
}

async fn sync_handle(path: &str, create: bool) -> io::Result<FileSystemSyncAccessHandle> {
    let (_, file, _) = file_handle(path, create).await?;
    let value = JsFuture::from(file.create_sync_access_handle())
        .await
        .map_err(|e| js_io("createSyncAccessHandle", e))?;
    Ok(value.unchecked_into())
}

impl OpfsBackend {
    /// Open the file at `path` (creating it and its directories if `create`)
    /// and take its exclusive sync-access handle. Refused with `WouldBlock`
    /// (DOMException `NoModificationAllowedError`) while another worker or
    /// tab holds the handle.
    pub async fn open(path: &str, create: bool) -> io::Result<Self> {
        let handle = sync_handle(path, create).await?;
        let token = REGISTRY.with(|registry| {
            let mut registry = registry.borrow_mut();
            registry.handles.push(Some(handle));
            Token {
                realm: registry.realm,
                index: registry.handles.len() - 1,
            }
        });
        Ok(Self {
            token,
            path: path.to_string(),
            counters: Arc::new(IoCounters::default()),
        })
    }

    pub fn counters(&self) -> Arc<IoCounters> {
        self.counters.clone()
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    /// Run `f` against this backend's handle, refusing if the calling thread
    /// is not the one that opened it (a different realm) or the handle has
    /// been closed.
    fn with<T>(
        &self,
        what: &str,
        f: impl FnOnce(&FileSystemSyncAccessHandle) -> io::Result<T>,
    ) -> io::Result<T> {
        REGISTRY.with(|registry| {
            let registry = registry.borrow();
            if registry.realm != self.token.realm {
                return Err(io::Error::new(
                    ErrorKind::NotConnected,
                    format!(
                        "{what}: the OPFS handle for {:?} belongs to realm {}, not this thread's realm {}",
                        self.path, self.token.realm, registry.realm
                    ),
                ));
            }
            match registry
                .handles
                .get(self.token.index)
                .and_then(|slot| slot.as_ref())
            {
                Some(handle) => f(handle),
                None => Err(io::Error::new(
                    ErrorKind::NotConnected,
                    format!(
                        "{what}: the OPFS handle for {:?} is closed",
                        self.path
                    ),
                )),
            }
        })
    }
}

impl StorageBackend for OpfsBackend {
    fn len(&self) -> io::Result<u64> {
        self.with("len", |handle| {
            let size = handle.get_size().map_err(|e| js_io("getSize", e))?;
            exact_u64("getSize", size)
        })
    }

    fn read(&self, offset: u64, out: &mut [u8]) -> io::Result<()> {
        self.counters.reads.fetch_add(1, Ordering::Relaxed);
        self.with("read", |handle| {
            let at = exact_f64("read offset", offset)?;
            let options = FileSystemReadWriteOptions::new();
            options.set_at(at);
            let read = handle
                .read_with_u8_array_and_options(out, &options)
                .map_err(|e| js_io("read", e))?;
            let n = exact_u64("read", read)? as usize;
            self.counters
                .bytes_read
                .fetch_add(n as u64, Ordering::Relaxed);
            if n < out.len() {
                return Err(io::Error::new(
                    ErrorKind::UnexpectedEof,
                    format!("read at {offset}: {n} of {} bytes", out.len()),
                ));
            }
            Ok(())
        })
    }

    fn set_len(&self, len: u64) -> io::Result<()> {
        self.counters.set_lens.fetch_add(1, Ordering::Relaxed);
        self.with("set_len", |handle| {
            let new_size = exact_f64("set_len", len)?;
            handle
                .truncate_with_f64(new_size)
                .map_err(|e| js_io("truncate", e))
        })
    }

    fn sync_data(&self) -> io::Result<()> {
        self.counters.syncs.fetch_add(1, Ordering::Relaxed);
        self.with("sync_data", |handle| {
            handle.flush().map_err(|e| js_io("flush", e))
        })
    }

    fn write(&self, offset: u64, data: &[u8]) -> io::Result<()> {
        self.counters.writes.fetch_add(1, Ordering::Relaxed);
        self.with("write", |handle| {
            let at = exact_f64("write offset", offset)?;
            let options = FileSystemReadWriteOptions::new();
            options.set_at(at);
            let written = handle
                .write_with_u8_array_and_options(data, &options)
                .map_err(|e| js_io("write", e))?;
            let n = exact_u64("write", written)? as usize;
            self.counters
                .bytes_written
                .fetch_add(n as u64, Ordering::Relaxed);
            if n < data.len() {
                return Err(io::Error::new(
                    ErrorKind::WriteZero,
                    format!("short write at {offset}: {n} of {} bytes", data.len()),
                ));
            }
            Ok(())
        })
    }

    /// Release the handle. redb calls this exactly once, including when an
    /// open fails, so the browser-side exclusivity ends with the database.
    ///
    /// Fails **closed** on a realm mismatch. An earlier version returned
    /// `Ok(())` there, which was the one place the registry still lied: the
    /// exclusive handle stays open on its owning thread, the file stays
    /// locked, and the caller was told the release succeeded. Today's
    /// single-threaded build cannot reach it, but "the guard holds under a
    /// future threaded build" is only true if this path reports the leak.
    fn close(&self) -> io::Result<()> {
        REGISTRY.with(|registry| {
            let mut registry = registry.borrow_mut();
            if registry.realm != self.token.realm {
                return Err(io::Error::new(
                    ErrorKind::NotConnected,
                    format!(
                        "close: the OPFS handle for {:?} belongs to realm {}, not this thread's realm {}; \
                         it is still open and the file is still locked",
                        self.path, self.token.realm, registry.realm
                    ),
                ));
            }
            if let Some(slot) = registry.handles.get_mut(self.token.index)
                && let Some(handle) = slot.take()
            {
                handle.close();
            }
            Ok(())
        })
    }
}

/// Delete the file at `path`. Returns whether it existed.
pub async fn remove(path: &str) -> io::Result<bool> {
    let (dir, _, name) = match file_handle(path, false).await {
        Ok(found) => found,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(err),
    };
    let options = FileSystemRemoveOptions::new();
    options.set_recursive(false);
    JsFuture::from(dir.remove_entry_with_options(&name, &options))
        .await
        .map_err(|e| js_io(&format!("removeEntry({name})"), e))?;
    Ok(true)
}

/// Whether the file at `path` exists.
pub async fn exists(path: &str) -> io::Result<bool> {
    match file_handle(path, false).await {
        Ok(_) => Ok(true),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err),
    }
}

/// The file's length, through a fresh handle that is closed again.
pub async fn size(path: &str) -> io::Result<u64> {
    let handle = sync_handle(path, false).await?;
    let size = handle.get_size().map_err(|e| js_io("getSize", e));
    handle.close();
    exact_u64("getSize", size?)
}

/// Every byte of the file at `path`.
pub async fn read_all(path: &str) -> io::Result<Vec<u8>> {
    let handle = sync_handle(path, false).await?;
    let outcome = (|| {
        let size = exact_u64(
            "getSize",
            handle.get_size().map_err(|e| js_io("getSize", e))?,
        )? as usize;
        let mut bytes = vec![0u8; size];
        let options = FileSystemReadWriteOptions::new();
        options.set_at(0.0);
        let n = exact_u64(
            "read",
            handle
                .read_with_u8_array_and_options(&mut bytes, &options)
                .map_err(|e| js_io("read", e))?,
        )? as usize;
        bytes.truncate(n);
        Ok(bytes)
    })();
    handle.close();
    outcome
}

/// Replace the file at `path` with `bytes`, creating it if absent.
pub async fn write_all(path: &str, bytes: &[u8]) -> io::Result<()> {
    let handle = sync_handle(path, true).await?;
    let outcome = (|| {
        handle
            .truncate_with_f64(0.0)
            .map_err(|e| js_io("truncate", e))?;
        let options = FileSystemReadWriteOptions::new();
        options.set_at(0.0);
        let n = exact_u64(
            "write",
            handle
                .write_with_u8_array_and_options(bytes, &options)
                .map_err(|e| js_io("write", e))?,
        )? as usize;
        if n < bytes.len() {
            return Err(io::Error::new(
                ErrorKind::WriteZero,
                format!("short write: {n} of {}", bytes.len()),
            ));
        }
        handle.flush().map_err(|e| js_io("flush", e))
    })();
    handle.close();
    outcome
}

// ── progress side file ───────────────────────────────────────────────────────

/// A worker-local side file the churn writes its progress to with
/// synchronous, flushed writes: 8 bytes `committed`, 8 bytes `committing`.
/// A page cannot learn where a forced kill landed from `postMessage` (it
/// stops delivering at `terminate()`) nor from a `BroadcastChannel` (nothing
/// posted by a never-yielding worker arrived), but it can read this file
/// afterwards. Never touched by redb; it is probe evidence only.
pub struct ProgressFile {
    handle: FileSystemSyncAccessHandle,
}

impl ProgressFile {
    /// The side file's path for a database path.
    pub fn path_for(database: &str) -> String {
        format!("{database}.progress")
    }

    pub async fn open(database: &str) -> io::Result<Self> {
        let handle = sync_handle(&Self::path_for(database), true).await?;
        Ok(Self { handle })
    }

    pub fn record(&self, committed: u64, committing: u64) -> io::Result<()> {
        let mut bytes = [0u8; 16];
        bytes[..8].copy_from_slice(&committed.to_le_bytes());
        bytes[8..].copy_from_slice(&committing.to_le_bytes());
        let options = FileSystemReadWriteOptions::new();
        options.set_at(0.0);
        let n = exact_u64(
            "progress write",
            self.handle
                .write_with_u8_array_and_options(&bytes, &options)
                .map_err(|e| js_io("progress write", e))?,
        )? as usize;
        if n < bytes.len() {
            return Err(io::Error::new(ErrorKind::WriteZero, "short progress write"));
        }
        self.handle.flush().map_err(|e| js_io("progress flush", e))
    }

    /// Read the side file through a fresh handle that is closed again.
    pub async fn read(database: &str) -> io::Result<Option<(u64, u64)>> {
        let bytes = match read_all(&Self::path_for(database)).await {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err),
        };
        if bytes.len() < 16 {
            return Ok(None);
        }
        let committed = u64::from_le_bytes(bytes[..8].try_into().unwrap());
        let committing = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
        Ok(Some((committed, committing)))
    }
}

impl Drop for ProgressFile {
    fn drop(&mut self) {
        self.handle.close();
    }
}

// ── staged creation ──────────────────────────────────────────────────────────

/// Whether this browser exposes `FileSystemFileHandle.move()`. It is a
/// vendor-implemented extension, not in the core WHATWG File System IDL, and
/// web-sys 0.3.103 has no binding for it, so it is called through `Reflect`
/// and feature-detected.
pub async fn move_supported() -> bool {
    let Ok((_, file, _)) = file_handle("muniment-probe/.move-probe", true).await else {
        return false;
    };
    let supported = js_sys::Reflect::get(&file, &JsValue::from_str("move"))
        .map(|value| value.is_function())
        .unwrap_or(false);
    let _ = remove("muniment-probe/.move-probe").await;
    supported
}

/// Rename `from` onto `to` within the same directory, atomically where the
/// browser implements `move()`. Returns whether the atomic path was used;
/// `false` means the byte-copy fallback ran, which is NOT atomic and is
/// recorded as such in the receipt.
pub async fn promote(from: &str, to: &str) -> io::Result<bool> {
    let (_, file, _) = file_handle(from, false).await?;
    let (to_segments, to_name) = split(to)?;
    let to_name = to_name.to_string();
    let target_dir = directory(&to_segments, true).await?;
    let mover = js_sys::Reflect::get(&file, &JsValue::from_str("move"))
        .map_err(|e| js_io("Reflect.get(move)", e))?;
    if mover.is_function() {
        let mover: js_sys::Function = mover.unchecked_into();
        let promise = mover
            .call2(&file, &target_dir, &JsValue::from_str(&to_name))
            .map_err(|e| js_io("FileSystemFileHandle.move", e))?;
        JsFuture::from(js_sys::Promise::from(promise))
            .await
            .map_err(|e| js_io("FileSystemFileHandle.move", e))?;
        return Ok(true);
    }
    // Fallback: copy then delete. Not atomic; a crash between the two leaves
    // the staging file, which the next open discards.
    let bytes = read_all(from).await?;
    write_all(to, &bytes).await?;
    remove(from).await?;
    Ok(false)
}
