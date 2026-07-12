//! The acquisition background jobs (search / import / match), moved out of the
//! core luma-engine job roster so the core names no module crate. Each handler
//! file owns its `pub const SPEC` + `pub fn run`; the crate root gathers the
//! three SPECs into [`crate::JOBS`], which the binary hands to `AppState::new`
//! for registration.

pub mod import;
pub mod match_;
pub mod search;

use luma_engine::services::jobs::JobContext;
use luma_module_host::HostCtx;

/// The acquisition jobs belong to the Acquisition module: they grab + import
/// torrents. When that module is disabled these jobs no-op (a disabled module
/// does no background work). Returns true (and logs) when the caller should
/// skip. Resolves the enabled-state through the `HostCtx` seam.
fn acquisition_disabled(ctx: &JobContext) -> bool {
    if ctx.state.module_enabled(crate::MODULE_ID) {
        return false;
    }
    ctx.info("Acquisition module disabled; skipping.");
    true
}
