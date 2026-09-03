// Copyright 2026 Mark Alan Boykin
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// SPDX-License-Identifier: MPL-2.0

//! Portable async-host boundary traits.
//!
//! `kernel` names the runtime/host seam in terms of a small,
//! object-safe task-spawning interface so native and wasm hosts can choose
//! different executor models without changing runtime code.

use std::any::Any;
use std::marker::PhantomData;

use crossbeam_channel::{Receiver, TryRecvError};
use futures_util::future::BoxFuture;

/// Boxed blocking-task result used at the object-safe trait boundary.
pub type ErasedBlockingResult = Box<dyn Any + Send>;

/// Boxed blocking-task work item used at the object-safe trait boundary.
pub type BoxedBlockingWork = Box<dyn FnOnce() -> ErasedBlockingResult + Send + 'static>;

/// Object-safe host-provided async spawner.
///
/// `spawn_blocking` is exposed as an erased operation on the trait so the
/// runtime can hold `Arc<dyn AsyncSpawner>`. Typed callers use the helper
/// [`spawn_blocking`] wrapper below, which downcasts the boxed result back to
/// the requested type.
pub trait AsyncSpawner: Send + Sync {
    /// Spawn a supervised async task.
    fn spawn_supervised(
        &self,
        label: &'static str,
        task: BoxFuture<'static, ()>,
    ) -> Result<(), SpawnError>;

    /// Spawn a blocking task and return an erased result receiver.
    fn spawn_blocking_erased(
        &self,
        label: &'static str,
        work: BoxedBlockingWork,
    ) -> Result<Receiver<ErasedBlockingResult>, SpawnError>;

    /// Broadcast cancellation to all supervised tasks.
    fn request_cancel(&self);

    /// Has cancellation already been requested?
    fn is_cancelled(&self) -> bool;

    /// Request cancellation and wait for supervised tasks to terminate.
    fn shutdown(&self) -> BoxFuture<'static, ()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnError {
    /// This host does not support the requested spawn kind.
    Unsupported,
    /// The runtime is shutting down and rejected the task.
    ShuttingDown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockingTryRecvError {
    Empty,
    Disconnected,
    TypeMismatch,
}

/// Typed wrapper over an erased blocking-task result channel.
pub struct BlockingTaskReceiver<T> {
    inner: Receiver<ErasedBlockingResult>,
    marker: PhantomData<T>,
}

impl<T> BlockingTaskReceiver<T> {
    pub fn new(inner: Receiver<ErasedBlockingResult>) -> Self {
        Self {
            inner,
            marker: PhantomData,
        }
    }
}

impl<T> BlockingTaskReceiver<T>
where
    T: Send + 'static,
{
    pub fn try_recv(&self) -> Result<T, BlockingTryRecvError> {
        match self.inner.try_recv() {
            Ok(value) => value
                .downcast::<T>()
                .map(|typed| *typed)
                .map_err(|_| BlockingTryRecvError::TypeMismatch),
            Err(TryRecvError::Empty) => Err(BlockingTryRecvError::Empty),
            Err(TryRecvError::Disconnected) => Err(BlockingTryRecvError::Disconnected),
        }
    }
}

/// Spawn a blocking task through an object-safe [`AsyncSpawner`] and recover
/// a typed result receiver on the caller side.
pub fn spawn_blocking<T, F>(
    spawner: &dyn AsyncSpawner,
    label: &'static str,
    work: F,
) -> Result<BlockingTaskReceiver<T>, SpawnError>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let rx = spawner.spawn_blocking_erased(label, Box::new(move || Box::new(work())))?;
    Ok(BlockingTaskReceiver::new(rx))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use crossbeam_channel::bounded;

    use super::{
        AsyncSpawner, BlockingTaskReceiver, BlockingTryRecvError, BoxedBlockingWork, SpawnError,
        spawn_blocking,
    };

    #[derive(Default)]
    struct MockAsyncSpawner {
        cancelled: Arc<AtomicBool>,
    }

    impl AsyncSpawner for MockAsyncSpawner {
        fn spawn_supervised(
            &self,
            _label: &'static str,
            _task: futures_util::future::BoxFuture<'static, ()>,
        ) -> Result<(), SpawnError> {
            if self.cancelled.load(Ordering::SeqCst) {
                return Err(SpawnError::ShuttingDown);
            }
            Ok(())
        }

        fn spawn_blocking_erased(
            &self,
            _label: &'static str,
            work: BoxedBlockingWork,
        ) -> Result<crossbeam_channel::Receiver<super::ErasedBlockingResult>, SpawnError> {
            if self.cancelled.load(Ordering::SeqCst) {
                return Err(SpawnError::ShuttingDown);
            }
            let (tx, rx) = bounded(1);
            let _ = tx.send(work());
            Ok(rx)
        }

        fn request_cancel(&self) {
            self.cancelled.store(true, Ordering::SeqCst);
        }

        fn is_cancelled(&self) -> bool {
            self.cancelled.load(Ordering::SeqCst)
        }

        fn shutdown(&self) -> futures_util::future::BoxFuture<'static, ()> {
            let cancelled = Arc::clone(&self.cancelled);
            Box::pin(async move {
                cancelled.store(true, Ordering::SeqCst);
            })
        }
    }

    #[test]
    fn spawn_blocking_returns_typed_value() {
        let spawner = MockAsyncSpawner::default();
        let rx = spawn_blocking(&spawner, "test_blocking", || 42usize)
            .expect("spawn_blocking should succeed");

        assert_eq!(rx.try_recv(), Ok(42));
    }

    #[test]
    fn blocking_task_receiver_reports_type_mismatch() {
        let (tx, rx) = bounded(1);
        tx.send(Box::new("wrong-type".to_string()) as super::ErasedBlockingResult)
            .expect("send should succeed");
        let rx = BlockingTaskReceiver::<usize>::new(rx);

        assert_eq!(rx.try_recv(), Err(BlockingTryRecvError::TypeMismatch));
    }

    #[test]
    fn spawn_blocking_rejects_new_work_after_cancel() {
        let spawner = MockAsyncSpawner::default();
        spawner.request_cancel();

        let result = spawn_blocking(&spawner, "test_blocking", || 42usize);

        assert!(matches!(result, Err(SpawnError::ShuttingDown)));
    }
}
