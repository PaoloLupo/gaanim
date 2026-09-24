//! Content fingerprints built from `Debug` output.
//!
//! Hot reload compares authored scene descriptions between script runs. Most
//! types print their complete content through `Debug`; opaque values such as
//! callbacks print only a summary. While [`with_identity_debug`] runs, those
//! values also print their allocation identity, so two different callbacks
//! never produce the same fingerprint.

use std::cell::Cell;
use std::fmt::{self, Debug, Write};
use std::hash::{DefaultHasher, Hasher};

thread_local! {
    static IDENTITY_DEBUG: Cell<bool> = const { Cell::new(false) };
}

/// Whether `Debug` implementations of opaque values must print their identity.
pub fn identity_debug() -> bool {
    IDENTITY_DEBUG.with(Cell::get)
}

/// Run `f` with identity-revealing `Debug` output on this thread.
pub fn with_identity_debug<R>(f: impl FnOnce() -> R) -> R {
    struct Reset(bool);
    impl Drop for Reset {
        fn drop(&mut self) {
            IDENTITY_DEBUG.with(|flag| flag.set(self.0));
        }
    }
    let _reset = Reset(IDENTITY_DEBUG.with(|flag| flag.replace(true)));
    f()
}

/// Address of a shared value, printable while [`identity_debug`] is active.
pub fn identity<T: ?Sized>(value: &T) -> Identity {
    Identity((value as *const T).cast::<()>() as usize)
}

/// Opaque allocation address; see [`identity`].
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Identity(usize);

impl Debug for Identity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "@{:#x}", self.0)
    }
}

/// Streams `Debug` output into a hasher without materializing the text.
#[derive(Default)]
pub struct DebugFingerprint {
    hasher: DefaultHasher,
    /// Set when a value could not print its content, e.g. a locked `Mutex`.
    opaque: bool,
}

impl DebugFingerprint {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one value, formatted with identity-revealing `Debug`.
    pub fn add(&mut self, value: &impl Debug) {
        with_identity_debug(|| {
            let _ = write!(self, "{value:?}");
        });
        // Separate consecutive values so their texts cannot run together.
        self.hasher.write_u8(0xff);
    }

    /// The fingerprint, or `None` when some content was not observable.
    pub fn finish(&self) -> Option<u64> {
        (!self.opaque).then(|| self.hasher.finish())
    }
}

impl Write for DebugFingerprint {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        // `Mutex` and `RwLock` print this placeholder while another borrow
        // holds them; the hidden content must not count as unchanged.
        if text.contains("<locked>") {
            self.opaque = true;
        }
        self.hasher.write(text.as_bytes());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct Callback(Arc<dyn Fn() + Send + Sync>);

    impl Debug for Callback {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let mut debug = f.debug_struct("Callback");
            if identity_debug() {
                debug.field("identity", &identity(&*self.0));
            }
            debug.finish_non_exhaustive()
        }
    }

    fn fingerprint(value: &impl Debug) -> Option<u64> {
        let mut fingerprint = DebugFingerprint::new();
        fingerprint.add(value);
        fingerprint.finish()
    }

    #[test]
    fn opaque_values_differ_by_identity_only_while_fingerprinting() {
        let first = Callback(Arc::new(|| {}));
        let second = Callback(Arc::new(|| {}));
        assert_eq!(format!("{first:?}"), format!("{second:?}"));
        assert_ne!(fingerprint(&first), fingerprint(&second));
        assert_eq!(fingerprint(&first), fingerprint(&first));
        assert!(!identity_debug());
    }

    #[test]
    fn content_changes_and_locked_values_are_never_equal() {
        assert_eq!(fingerprint(&vec![1.0, 2.0]), fingerprint(&vec![1.0, 2.0]));
        assert_ne!(fingerprint(&vec![1.0, 2.0]), fingerprint(&vec![1.0, 2.5]));
        let shared = Mutex::new(3);
        let _guard = shared.lock().unwrap();
        assert_eq!(fingerprint(&shared), None);
    }
}
