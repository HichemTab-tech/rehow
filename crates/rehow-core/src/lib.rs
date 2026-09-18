pub(crate) mod application;
pub mod domain;
pub mod git {}
pub mod markdown;
pub mod protocol;
pub mod search;
pub mod storage;

#[allow(
    dead_code,
    reason = "Phase 1C parser is consumed beginning in Phase 1D"
)]
pub(crate) mod parsing;
