//! Shared boilerplate for the concrete pipeline stage modules.
//!
//! Every `stages/<name>.rs` declares the same three items next to its own
//! `enumerate` + `process`: the [`Stage`] descriptor `STAGE`, its drain-job
//! [`Builtin`] `SPEC`, and a one-line `run` that hands the stage to the
//! dispatcher. Only a handful of literals differ between them, so the [`stage!`]
//! macro generates all three from those literals, leaving each module with just
//! its own enumeration/processing logic.
//!
//! [`Stage`]: crate::services::pipeline::stage::Stage
//! [`Builtin`]: crate::services::jobs::Builtin

/// Declare a pipeline stage's `STAGE` descriptor, its drain `SPEC` [`Builtin`],
/// and the `run` glue in one shot. The macro expands in the calling module, so it
/// binds to that module's own `enumerate` and `process` functions.
///
/// `key` is always `"pipeline.{short}"` and `category` is always
/// [`Category::Pipeline`]; only `short`, `subject_kind`, `concurrency`,
/// `pause_for_playback`, `schedule`, and `triggers` vary per stage.
///
/// [`Builtin`]: crate::services::jobs::Builtin
/// [`Category::Pipeline`]: crate::model::Category::Pipeline
macro_rules! stage {
    (
        short: $short:literal,
        subject_kind: $subject_kind:literal,
        concurrency: $concurrency:literal,
        pause_for_playback: $pause_for_playback:literal,
        schedule: $schedule:expr,
        triggers: $triggers:expr $(,)?
    ) => {
        pub const STAGE: $crate::services::pipeline::stage::Stage =
            $crate::services::pipeline::stage::Stage {
                short: $short,
                key: concat!("pipeline.", $short),
                subject_kind: $subject_kind,
                concurrency: $concurrency,
                pause_for_playback: $pause_for_playback,
                enumerate,
                process,
            };

        pub const SPEC: $crate::services::jobs::Builtin = $crate::services::jobs::Builtin {
            key: $crate::services::jobs::JobKey(concat!("pipeline.", $short)),
            category: $crate::model::Category::Pipeline,
            schedule: $schedule,
            triggers: $triggers,
            run,
        };

        fn run(ctx: &$crate::services::jobs::JobContext) -> ::anyhow::Result<()> {
            $crate::services::pipeline::dispatcher::run(&STAGE, ctx)
        }
    };
}

pub(crate) use stage;
