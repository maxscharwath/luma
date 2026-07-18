//! The acquisition background jobs (search / import / match), moved out of the
//! core kroma-engine job roster so the core names no module crate. Each handler
//! file owns its `pub const SPEC` + `pub fn run`; the crate root gathers the
//! three SPECs into [`crate::JOBS`], which the binary hands to `AppState::new`
//! for registration.

pub mod import;
pub mod match_;
pub mod search;

use kroma_module_sdk::engine::services::jobs::JobContext;
use kroma_module_sdk::host::HostCtx;

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

/// Declare one acquisition background job in a single place: its [`Builtin`]
/// `SPEC` (always `Category::Acquisition`) plus a `run` handler that short-
/// circuits to `Ok(())` when the module is disabled. Every acquisition job
/// shares that descriptor + guard scaffolding; each handler file supplies only
/// what differs (key, schedule, triggers, body). `$ctx` binds the [`JobContext`]
/// in scope for the body.
macro_rules! acquisition_job {
    (
        key: $key:literal,
        schedule: $schedule:expr,
        triggers: $triggers:expr,
        run: |$ctx:ident| $body:block $(,)?
    ) => {
        pub const SPEC: kroma_module_sdk::engine::services::jobs::Builtin =
            kroma_module_sdk::engine::services::jobs::Builtin {
                key: kroma_module_sdk::engine::services::jobs::JobKey($key),
                category: kroma_module_sdk::engine::model::Category::Acquisition,
                schedule: $schedule,
                triggers: $triggers,
                run,
            };

        pub fn run(
            $ctx: &kroma_module_sdk::engine::services::jobs::JobContext,
        ) -> anyhow::Result<()> {
            if $crate::jobs::acquisition_disabled($ctx) {
                return Ok(());
            }
            $body
        }
    };
}
pub(crate) use acquisition_job;
