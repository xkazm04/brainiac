//! Library layer repository (docs/LIBRARY-PLAN.md LB0, migration 0028).
//!
//! The normative layer's data plane, one concern per module:
//! - [`standards`] — the rule is the atom: insert/read/adopt/deprecate + provenance
//! - [`bridge`]    — the L6 divergence → candidate bridge (the one function
//!   with product weight)
//! - [`skills`]    — versioned bundles; drafts are never served
//! - [`usage`]     — the vital signs; counted by team, never by person
//!
//! Two invariants these modules lean on but do NOT own — the schema owns
//! them, so no future code path can skip them:
//! - attribution: a standard cannot leave `proposed` without provenance rows
//!   or a named decree (deferred constraint trigger in 0028);
//! - no leaderboard: `library_usage_events` has no user column at all.
//!
//! Everything runs under the caller's RLS transaction; there is no unscoped
//! query path.

pub mod bridge;
pub mod proposals;
pub mod skills;
pub mod standards;
pub mod usage;

pub use bridge::{propose_from_divergence, ratify_divergence, slugify};
pub use proposals::{propose_standard, Proposal, ProposeOutcome, DEFAULT_PROPOSE_PER_HOUR};
pub use skills::{
    add_skill_version, current_published_version, get_skill_by_slug, insert_skill, list_skills,
    publish_skill_version, versions_of, NewSkill, NewSkillVersion,
};
pub use standards::{
    add_provenance, adopt_standard, deprecate_standard, get_standard, get_standard_by_slug,
    insert_standard, list_standards, mark_standards_pages_dirty, provenance, reject_standard,
    versions, NewStandard, StandardVersionRow,
};
pub use usage::{health_signals, record_usage, usage_by_team, usage_named, LibraryHealth};
