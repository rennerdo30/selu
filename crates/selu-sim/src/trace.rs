//! Trace recording (docs/SIMULATOR.md decision 7). Owner: package S2.
//!
//! Groundwork (S0): the record types and the ring buffer live in
//! `core.rs` ([`crate::core::Trace`]), and `print` and `Sim::fire` already
//! record `Print` and `Event` entries. S2 adds the remaining sources here:
//! script calls (`OnUpdate` aggregated per second), hooks, timer callbacks,
//! protected calls and their deferral, and source positions from
//! `debug.getinfo`.
