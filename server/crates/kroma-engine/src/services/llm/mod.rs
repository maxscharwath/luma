//! LLM-facing application services **connectors** that expose KROMA's data to a
//! model as callable tools.
//!
//! Today this is the catalog connector ([`CatalogTools`]); it sits on the
//! vendor-neutral [`ToolBox`](crate::infra::llm::ToolBox) foundation, so the same
//! tools back any tool-driven feature editorial curation now, library chat /
//! tool-driven personalize next.

mod suggest;
mod tools;

pub use suggest::suggest_for;
pub use tools::CatalogTools;
