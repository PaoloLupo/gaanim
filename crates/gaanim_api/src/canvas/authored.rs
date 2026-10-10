//! Shared authoring state that counts its own writes.
//!
//! A scene's operations, object declarations and camera constraints live
//! behind [`Authored`] locks that share one revision counter. Every mutable
//! access advances it, so an unchanged revision proves that nothing the
//! compilation reads changed, without comparing the content.

use std::fmt;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LockResult, Mutex, MutexGuard, PoisonError};

/// A `Mutex` over authored state whose writes advance a scene revision.
pub(crate) struct Authored<T> {
    value: Mutex<T>,
    revision: Arc<AtomicU64>,
}

impl<T> Authored<T> {
    /// Guard `value` with a revision counter of its own.
    pub fn new(value: T) -> Self {
        Self::with_revision(value, Arc::default())
    }

    /// Guard `value` with the revision counter it shares with `other`.
    pub fn sharing<U>(value: T, other: &Authored<U>) -> Self {
        Self::with_revision(value, other.revision.clone())
    }

    fn with_revision(value: T, revision: Arc<AtomicU64>) -> Self {
        Self {
            value: Mutex::new(value),
            revision,
        }
    }

    /// Lock the value; writing through the guard advances the revision.
    pub fn lock(&self) -> LockResult<AuthoredGuard<'_, T>> {
        let revision = &self.revision;
        self.value
            .lock()
            .map(|guard| AuthoredGuard { guard, revision })
            .map_err(|poisoned| {
                PoisonError::new(AuthoredGuard {
                    guard: poisoned.into_inner(),
                    revision,
                })
            })
    }

    /// The number of writes so far to every value sharing this counter.
    pub fn revision(&self) -> u64 {
        self.revision.load(Ordering::Acquire)
    }
}

/// Prints like the `Mutex` it wraps, so content fingerprints are unchanged.
impl<T: fmt::Debug> fmt::Debug for Authored<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.value.fmt(f)
    }
}

pub(crate) struct AuthoredGuard<'a, T> {
    guard: MutexGuard<'a, T>,
    revision: &'a AtomicU64,
}

impl<T> Deref for AuthoredGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.guard
    }
}

impl<T> DerefMut for AuthoredGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.revision.fetch_add(1, Ordering::AcqRel);
        &mut self.guard
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_advance_the_shared_revision_and_reads_do_not() {
        let state = Authored::new(vec![1]);
        let spec = Authored::sharing(String::new(), &state);
        let start = state.revision();

        assert_eq!(state.lock().unwrap().len(), 1);
        assert_eq!(spec.lock().unwrap().len(), 0);
        assert_eq!(state.revision(), start);

        state.lock().unwrap().push(2);
        assert_eq!(spec.revision(), start + 1);
        spec.lock().unwrap().push('a');
        assert_eq!(state.revision(), start + 2);
        assert_eq!(Authored::new(()).revision(), 0);
    }

    #[test]
    fn prints_like_a_mutex() {
        assert_eq!(
            format!("{:?}", Authored::new(3)),
            format!("{:?}", Mutex::new(3))
        );
    }
}
