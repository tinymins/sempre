use std::time::Duration;

use crate::TransparentError;

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[path = "route_events/unix.rs"]
mod native;
#[cfg(target_os = "windows")]
#[path = "route_events/windows.rs"]
mod native;
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
mod native {
    pub(super) struct Watcher;

    impl Watcher {
        #[allow(clippy::unnecessary_wraps)] // Shared platform contract.
        pub(super) fn open() -> Result<Self, crate::TransparentError> {
            Ok(Self)
        }

        pub(super) async fn next(&mut self) -> Result<(), crate::TransparentError> {
            std::future::pending().await
        }
    }
}

// Sempre's own route batches arrive as bursts; one pass follows each burst.
const QUIET: Duration = Duration::from_millis(500);
const SETTLE_LIMIT: Duration = Duration::from_secs(5);

/// OS route, interface and address change notifications. Idle systems never wake.
pub struct RouteEvents(native::Watcher);

impl RouteEvents {
    pub fn open() -> Result<Self, TransparentError> {
        native::Watcher::open().map(Self)
    }

    /// Resolves once a change was reported and the burst has settled.
    pub async fn changed(&mut self) -> Result<(), TransparentError> {
        self.0.next().await?;
        let deadline = tokio::time::Instant::now() + SETTLE_LIMIT;
        while tokio::time::Instant::now() < deadline {
            match tokio::time::timeout(QUIET, self.0.next()).await {
                Ok(result) => result?,
                Err(_) => break,
            }
        }
        Ok(())
    }
}
