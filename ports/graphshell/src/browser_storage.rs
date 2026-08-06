//! What the browser has said about keeping this profile's stored graph.
//!
//! `navigator.storage.persist()` is a request, not a setting. A browser may
//! grant it, refuse it, decide silently on heuristics, or not offer the API at
//! all in an insecure context. The one thing a host must not do is assume.
//!
//! This module is the part of that with no browser in it: the states worth
//! distinguishing, and how they are said out loud. The wasm entry point does
//! the asking. Keeping them apart is what lets the wording and the durability
//! rule be tested, because `web.rs` has no test harness and cannot grow one
//! without a browser.

use std::fmt;

/// Whether stored bytes survive storage pressure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoragePersistence {
    /// The browser will not evict this origin under pressure.
    Granted,
    /// The browser may evict this origin. Ordinary, not an error: most
    /// browsers refuse until a profile looks established.
    Refused,
    /// Nobody answered. An insecure context, an older browser, or a call that
    /// failed. Deliberately distinct from [`Refused`](Self::Refused): "the
    /// browser said no" and "we could not ask" are different facts, and
    /// reporting the second as the first is a guess presented as knowledge.
    Unknown(String),
}

impl StoragePersistence {
    /// Whether this profile's bytes can be relied on to still be here later.
    ///
    /// Only [`Granted`](Self::Granted) qualifies. `Unknown` is not durable:
    /// treating an unanswered question as a yes is exactly the failure this
    /// type exists to prevent.
    pub fn is_durable(&self) -> bool {
        matches!(self, Self::Granted)
    }

    /// A stable token for automation and scenario checks, so a driver reads a
    /// state rather than parsing a sentence.
    pub fn token(&self) -> &'static str {
        match self {
            Self::Granted => "granted",
            Self::Refused => "refused",
            Self::Unknown(_) => "unknown",
        }
    }
}

impl fmt::Display for StoragePersistence {
    /// Phrased for someone deciding whether to trust this device with the only
    /// copy. Refusal names its consequence rather than its mechanism, because
    /// "not persisted" reads as a setting somebody forgot rather than as
    /// bytes that can vanish.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Granted => write!(formatter, "persistent"),
            Self::Refused => write!(formatter, "not persistent, may be evicted"),
            Self::Unknown(_) => write!(formatter, "persistence unknown"),
        }
    }
}

/// Decide the state from what the two storage calls returned.
///
/// `persisted` is the current answer; `requested` is the result of asking,
/// which a caller only performs when `persisted` came back false. Both are
/// `Result` because either call can be absent or fail.
///
/// Asking again once the answer is already yes is pointless, so a granted
/// `persisted` short-circuits.
pub fn decide(
    persisted: Result<bool, String>,
    requested: impl FnOnce() -> Result<bool, String>,
) -> StoragePersistence {
    match persisted {
        Ok(true) => StoragePersistence::Granted,
        Ok(false) => match requested() {
            Ok(true) => StoragePersistence::Granted,
            Ok(false) => StoragePersistence::Refused,
            // The browser answered the first question and not the second. That
            // is not a refusal; it is a gap, and it is named as one.
            Err(reason) => StoragePersistence::Unknown(reason),
        },
        Err(reason) => StoragePersistence::Unknown(reason),
    }
}

/// One line describing where this profile is stored and whether it will stay.
///
/// Both halves together, because either alone misleads: "IndexedDB reopened"
/// sounds durable and is not, and a persistence state with no store named does
/// not say what it is about.
pub fn status_line(store: &str, persistence: &StoragePersistence) -> String {
    format!("{store} · {persistence}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_granted_request_counts_as_durable() {
        assert!(StoragePersistence::Granted.is_durable());
        assert!(!StoragePersistence::Refused.is_durable());
        assert!(
            !StoragePersistence::Unknown("no storage manager".into()).is_durable(),
            "an unanswered question is not a yes"
        );
    }

    #[test]
    fn an_already_persistent_profile_is_not_asked_again() {
        let mut asked = false;
        let state = decide(Ok(true), || {
            asked = true;
            Ok(false)
        });
        assert_eq!(state, StoragePersistence::Granted);
        assert!(
            !asked,
            "requesting what is already granted is a wasted prompt"
        );
    }

    #[test]
    fn a_refusal_and_an_unanswerable_question_stay_distinct() {
        assert_eq!(decide(Ok(false), || Ok(true)), StoragePersistence::Granted);
        assert_eq!(decide(Ok(false), || Ok(false)), StoragePersistence::Refused);

        // Both of these could lazily be called "not persisted". They are not
        // the same, and the difference is what an operator needs.
        let no_api = decide(Err("no storage manager".into()), || Ok(true));
        let ask_failed = decide(Ok(false), || Err("persist() rejected".into()));
        assert_eq!(
            no_api,
            StoragePersistence::Unknown("no storage manager".into())
        );
        assert_eq!(
            ask_failed,
            StoragePersistence::Unknown("persist() rejected".into())
        );
        assert_ne!(no_api, StoragePersistence::Refused);
        assert_ne!(ask_failed, StoragePersistence::Refused);
    }

    #[test]
    fn the_status_line_never_reports_a_store_without_its_durability() {
        assert_eq!(
            status_line("IndexedDB reopened", &StoragePersistence::Granted),
            "IndexedDB reopened · persistent"
        );
        // The case the whole module is for: a reopened store reads as safe
        // until the second half says otherwise.
        assert_eq!(
            status_line("IndexedDB reopened", &StoragePersistence::Refused),
            "IndexedDB reopened · not persistent, may be evicted"
        );
        assert_eq!(
            status_line("IndexedDB seeded", &StoragePersistence::Unknown("x".into())),
            "IndexedDB seeded · persistence unknown"
        );
    }

    #[test]
    fn tokens_are_stable_for_automation() {
        assert_eq!(StoragePersistence::Granted.token(), "granted");
        assert_eq!(StoragePersistence::Refused.token(), "refused");
        assert_eq!(
            StoragePersistence::Unknown(String::new()).token(),
            "unknown"
        );
    }
}
