//! World state: units, combat, secret values, money, bags (docs/SIMULATOR.md
//! decision 5, SPEC-V1.md section 8). Owner: package S2.
//!
//! Empty in the groundwork. Round 7's combat and secret-value hooks
//! (`Sim::set_combat`, `Sim::set_secret_global`) move here; every setter
//! fires the events the client would, with payloads from the manifest.
