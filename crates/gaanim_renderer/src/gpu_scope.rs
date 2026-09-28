//! Validation error scopes checked without blocking.
//!
//! Native wgpu resolves a popped error scope at once, so it is awaited in
//! place. WebGPU resolves it asynchronously and the web player has no thread
//! to block on (blocking panics there), so the scope is polled once per frame
//! until it resolves, and the object it guards waits unused until then.

use vello::wgpu;

/// What a validation error scope caught.
pub(crate) enum ScopeCheck {
    Valid,
    Invalid(wgpu::Error),
    /// Not resolved yet (WebGPU only); poll it on a later frame.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    Pending(PendingScope),
}

/// Pop `scope` and return what it caught, or [`ScopeCheck::Pending`] while
/// WebGPU has not resolved it.
pub(crate) fn check(scope: wgpu::ErrorScopeGuard) -> ScopeCheck {
    let future = scope.pop();
    #[cfg(not(target_arch = "wasm32"))]
    {
        outcome(pollster::block_on(future))
    }
    #[cfg(target_arch = "wasm32")]
    {
        let mut future: web::ScopeFuture = Box::pin(future);
        match web::poll(&mut future) {
            Some(error) => outcome(error),
            None => ScopeCheck::Pending(PendingScope(web::store(future))),
        }
    }
}

fn outcome(error: Option<wgpu::Error>) -> ScopeCheck {
    match error {
        None => ScopeCheck::Valid,
        Some(error) => ScopeCheck::Invalid(error),
    }
}

/// An error scope WebGPU has not resolved yet. The future lives on the one
/// thread of the web player, so this key keeps its holders `Send`.
pub(crate) struct PendingScope(#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))] u64);

impl PendingScope {
    /// [`ScopeCheck::Valid`] or [`ScopeCheck::Invalid`] once the scope
    /// resolved; `None` while it is pending.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub(crate) fn poll(&self) -> Option<ScopeCheck> {
        #[cfg(target_arch = "wasm32")]
        {
            web::poll_stored(self.0).map(outcome)
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            Some(ScopeCheck::Valid)
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Drop for PendingScope {
    fn drop(&mut self) {
        web::forget(self.0);
    }
}

#[cfg(target_arch = "wasm32")]
mod web {
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Poll, Waker};

    use vello::wgpu;

    pub(super) type ScopeFuture = Pin<Box<dyn Future<Output = Option<wgpu::Error>>>>;

    thread_local! {
        static PENDING: RefCell<HashMap<u64, ScopeFuture>> = RefCell::new(HashMap::new());
        static NEXT: Cell<u64> = const { Cell::new(0) };
    }

    /// Poll without a waker: the scope's promise records its result when it
    /// settles, and the next frame polls again.
    pub(super) fn poll(future: &mut ScopeFuture) -> Option<Option<wgpu::Error>> {
        match future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            Poll::Ready(error) => Some(error),
            Poll::Pending => None,
        }
    }

    pub(super) fn store(future: ScopeFuture) -> u64 {
        let key = NEXT.replace(NEXT.get() + 1);
        PENDING.with_borrow_mut(|pending| pending.insert(key, future));
        key
    }

    pub(super) fn poll_stored(key: u64) -> Option<Option<wgpu::Error>> {
        PENDING.with_borrow_mut(|pending| {
            let error = poll(pending.get_mut(&key)?)?;
            pending.remove(&key);
            Some(error)
        })
    }

    pub(super) fn forget(key: u64) {
        PENDING.with_borrow_mut(|pending| pending.remove(&key));
    }
}
