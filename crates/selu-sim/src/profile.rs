//! Development-only startup measurements. Enable with SELU_SIM_PROFILE=1.
//! Durations are exclusive: nested owners are subtracted, never added twice.
#[cfg(debug_assertions)]
mod enabled {
    use std::{
        cell::RefCell,
        collections::BTreeMap,
        time::{Duration, Instant},
    };
    #[derive(Default)]
    struct Data {
        active: bool,
        stack: Vec<Duration>,
        totals: BTreeMap<&'static str, (Duration, u64)>,
        counts: BTreeMap<&'static str, u64>,
    }
    thread_local! {
        static DATA: RefCell<Data> = RefCell::new(Data {
            active: std::env::var_os("SELU_SIM_PROFILE").is_some(), ..Data::default()
        });
    }
    pub struct Span(Option<(Instant, &'static str)>);
    impl Span {
        pub fn new(name: &'static str) -> Self {
            Self(DATA.with(|data| {
                let mut data = data.borrow_mut();
                if !data.active {
                    return None;
                }
                data.stack.push(Duration::ZERO);
                Some((Instant::now(), name))
            }))
        }
    }
    impl Drop for Span {
        fn drop(&mut self) {
            let Some((start, name)) = self.0 else {
                return;
            };
            let elapsed = start.elapsed();
            DATA.with(|data| {
                let mut data = data.borrow_mut();
                let children = data.stack.pop().unwrap_or_default();
                if let Some(parent) = data.stack.last_mut() {
                    *parent += elapsed;
                }
                let entry = data.totals.entry(name).or_default();
                entry.0 += elapsed.saturating_sub(children);
                entry.1 += 1;
            });
        }
    }
    pub fn count(name: &'static str) {
        DATA.with(|data| {
            let mut data = data.borrow_mut();
            if data.active {
                *data.counts.entry(name).or_default() += 1;
            }
        });
    }
    pub fn report() {
        DATA.with(|data| {
            let data = data.borrow();
            if !data.active {
                return;
            }
            eprintln!(
                "SELU_SIM_PROFILE active; exclusive durations; {} spans still active",
                data.stack.len()
            );
            for (name, (duration, calls)) in &data.totals {
                eprintln!(
                    "PROFILE {name}: {:.6}s ({calls} calls)",
                    duration.as_secs_f64()
                );
            }
            for (name, count) in &data.counts {
                eprintln!("PROFILE_COUNT {name}: {count}");
            }
        });
    }
}
#[cfg(debug_assertions)]
pub use enabled::*;
#[cfg(not(debug_assertions))]
pub struct Span;
#[cfg(not(debug_assertions))]
impl Span {
    pub fn new(_: &'static str) -> Self {
        Self
    }
}
#[cfg(not(debug_assertions))]
pub fn count(_: &'static str) {}
#[cfg(not(debug_assertions))]
pub fn report() {}
