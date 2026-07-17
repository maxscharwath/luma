//! Intro / credits segment detection by audio fingerprinting. The chapter-based
//! source lives in the probe pass (`infra::probe`); this module is the heavier
//! fingerprint pass that fills the gaps, run as a background job.

pub mod fingerprint;
pub mod job;
