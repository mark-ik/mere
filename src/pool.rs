//! A growable, reusing worker-thread pool.
//!
//! Why this exists: some actors are long-lived (a content actor lives as long as
//! its surface) and `thread::spawn`-per-actor leaks one OS thread's worth of
//! per-thread state on every spawn — notably a layout engine's *leaked* thread-local
//! sharing cache, which is never reclaimed. Opening / closing / respawning surfaces
//! would then leak unboundedly with churn.
//!
//! A pooled worker **loops**: it runs one job (an actor's whole lifetime), and when
//! that job returns (the actor's command channel closed) it waits for the next job
//! rather than dying. So the OS-thread count is bounded by *peak concurrent* jobs,
//! not the total ever submitted, and the leaked thread-local count is bounded with
//! it. The pool **grows on demand**: a submit with no idle worker spawns one.
//!
//! Concurrency without `crossbeam`: a `Mutex<State>` (the job queue + the idle-worker
//! count) plus a `Condvar`. The idle count is read under the same lock as the spawn
//! decision, so growth is race-free — a job is never stranded with every worker busy.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

/// A unit of work to run on a pooled worker. For an actor this is its whole run
/// loop, so it occupies a worker until the actor ends.
type Job = Box<dyn FnOnce() + Send + 'static>;

struct State {
    queue: VecDeque<Job>,
    /// Workers currently blocked waiting for a job. Read under the lock when
    /// deciding whether to grow, so the decision is consistent.
    idle: usize,
}

struct Inner {
    state: Mutex<State>,
    available: Condvar,
    /// Total workers spawned — the high-water mark of concurrent jobs.
    workers: AtomicUsize,
}

/// A growable, reusing worker-thread pool. Cheap to clone (a shared handle).
#[derive(Clone)]
pub struct Pool {
    inner: Arc<Inner>,
}

impl Default for Pool {
    fn default() -> Self {
        Self::new()
    }
}

impl Pool {
    /// An empty pool. Workers are spawned lazily on the first submit that finds no
    /// idle worker.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Inner {
                state: Mutex::new(State {
                    queue: VecDeque::new(),
                    idle: 0,
                }),
                available: Condvar::new(),
                workers: AtomicUsize::new(0),
            }),
        }
    }

    /// Run `job` on a pooled worker, growing the pool by one if every worker is
    /// busy. The job runs to completion on whichever worker takes it, then that
    /// worker is reused for the next job.
    pub fn submit(&self, job: Job) {
        let mut state = self.inner.state.lock().unwrap();
        // Grow only when no worker is waiting (all are busy on long-lived jobs).
        // Checked under the lock, so an idle worker cannot slip away between the
        // check and the push and strand this job.
        if state.idle == 0 {
            self.spawn_worker();
        }
        state.queue.push_back(job);
        drop(state);
        self.inner.available.notify_one();
    }

    /// Total worker threads spawned — the peak number of concurrent jobs the pool
    /// has had to serve. The bound on leaked per-thread state.
    pub fn workers(&self) -> usize {
        self.inner.workers.load(Ordering::SeqCst)
    }

    fn spawn_worker(&self) {
        self.inner.workers.fetch_add(1, Ordering::SeqCst);
        let inner = Arc::clone(&self.inner);
        thread::spawn(move || {
            loop {
                let job = {
                    let mut state = inner.state.lock().unwrap();
                    loop {
                        if let Some(job) = state.queue.pop_front() {
                            break job;
                        }
                        state.idle += 1;
                        state = inner.available.wait(state).unwrap();
                        state.idle -= 1;
                    }
                };
                job(); // the actor's whole lifetime; the worker is reused when it returns
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Barrier;
    use std::sync::mpsc;

    use super::*;

    #[test]
    fn runs_all_jobs_and_grows_for_concurrency() {
        let pool = Pool::new();
        let (tx, rx) = mpsc::channel();
        // Five jobs that all rendezvous on a barrier before reporting: they can only
        // pass if the pool grew to >= 5 concurrent workers, so this both proves every
        // job ran and that the pool grows (a non-growing pool would deadlock here).
        let barrier = Arc::new(Barrier::new(5));
        for i in 0..5 {
            let tx = tx.clone();
            let barrier = Arc::clone(&barrier);
            pool.submit(Box::new(move || {
                barrier.wait();
                tx.send(i).unwrap();
            }));
        }
        let mut got: Vec<i32> = (0..5).map(|_| rx.recv().unwrap()).collect();
        got.sort();
        assert_eq!(
            got,
            vec![0, 1, 2, 3, 4],
            "all five ran concurrently (the barrier released)"
        );
        assert!(
            pool.workers() >= 5,
            "the pool grew to serve five concurrent jobs"
        );
    }

    #[test]
    fn reuses_a_worker_for_a_sequential_job() {
        // One job, awaited to completion, then a second. The single worker loops
        // back to idle and serves the second, so the pool never grows past one.
        let pool = Pool::new();
        let (tx, rx) = mpsc::channel();
        pool.submit(Box::new(move || tx.send(()).unwrap()));
        rx.recv().unwrap();
        // The worker is finishing `job()` and looping back to wait; give it the
        // moment to register idle before the next submit (no public idle signal to
        // await deterministically, so a short sleep keeps the test honest about reuse).
        thread::sleep(std::time::Duration::from_millis(50));
        let (tx2, rx2) = mpsc::channel();
        pool.submit(Box::new(move || tx2.send(()).unwrap()));
        rx2.recv().unwrap();
        assert_eq!(
            pool.workers(),
            1,
            "the idle worker was reused, not a second spawned"
        );
    }
}
