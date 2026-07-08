//! Persisting a log through [`muniment`].
//!
//! A [`Codicil`] is `Serialize`/`Deserialize`, so it stores as one muniment slot
//! value. These are the convenience methods for that.
//!
//! **Founding granularity:** the whole log is written as a single slot on each
//! `save`. That is correct and simple, and matches how strophe writes its
//! `ProjectBundle` today. The append-friendly form, writing each new entry as its
//! own content-addressed [`muniment::BlobStore`] blob and keeping only an ordered
//! index of hashes as the slot, is the roadmap (see the founding proposal): it
//! turns an append into one small write instead of a rewrite, and makes entries
//! immutable by hash.

use serde::{de::DeserializeOwned, Serialize};

use muniment::{Backend, Codec, SlotStore, StoreError};

use crate::log::Codicil;

impl<T> Codicil<T> {
    /// Write the whole log to the muniment slot at `key`, overwriting the
    /// previous value.
    pub async fn save<B, C>(&self, slots: &SlotStore<B, C>, key: &str) -> Result<(), StoreError>
    where
        B: Backend,
        C: Codec,
        T: Serialize,
    {
        slots.save(key, self).await
    }

    /// Load the log from the muniment slot at `key`, or an empty log if the slot
    /// is absent.
    pub async fn load<B, C>(slots: &SlotStore<B, C>, key: &str) -> Result<Self, StoreError>
    where
        B: Backend,
        C: Codec,
        T: DeserializeOwned,
    {
        Ok(slots.load(key).await?.unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use muniment::{JsonSlots, MemoryBackend};

    #[test]
    fn log_round_trips_through_a_muniment_slot() {
        pollster::block_on(async {
            let slots = JsonSlots::new(MemoryBackend::new());

            let mut log: Codicil<String> = Codicil::new();
            log.append("first edit".into());
            log.append("second edit".into());
            log.save(&slots, "history").await.unwrap();

            let restored: Codicil<String> = Codicil::load(&slots, "history").await.unwrap();
            assert_eq!(restored, log);
            assert_eq!(restored.len(), 2);
        });
    }

    #[test]
    fn absent_slot_loads_an_empty_log() {
        pollster::block_on(async {
            let slots = JsonSlots::new(MemoryBackend::new());
            let log: Codicil<u32> = Codicil::load(&slots, "never-saved").await.unwrap();
            assert!(log.is_empty());
        });
    }

    #[test]
    fn reload_then_append_continues_the_sequence() {
        pollster::block_on(async {
            let slots = JsonSlots::new(MemoryBackend::new());
            let mut log: Codicil<u32> = Codicil::new();
            log.append(1);
            log.save(&slots, "k").await.unwrap();

            let mut reloaded: Codicil<u32> = Codicil::load(&slots, "k").await.unwrap();
            let seq = reloaded.append(2);
            assert_eq!(seq, crate::Seq(1), "the sequence resumes after reload");
            assert_eq!(reloaded.entries(), &[1, 2]);
        });
    }
}
