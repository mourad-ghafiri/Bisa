//! The registry gathers every cache's stats and clears them all at once — the
//! shape the admin `/cache/stats` and `/cache/clear` surface reads.
//!
//! Each test owns a `Registry`, so a `clear` in one test cannot empty a cache
//! another test is counting; the last test is the one that touches the
//! process-wide registry, and only to read it.

use std::time::Duration;

use bisa_cache::{all_stats, Registry, TtlCache, TtlCell};

#[test]
fn a_registry_reports_each_registered_cache() {
    let reg = Registry::new();
    let cell = TtlCell::in_registry("regtest.cell", &reg);
    let keyed: TtlCache<i32, i32> = TtlCache::in_registry("regtest.keyed", 0, &reg);
    cell.set(1);
    keyed.insert(9, 9);
    assert_eq!(cell.get(Duration::from_secs(60)), Some(1)); // one hit
    assert_eq!(keyed.get(Duration::from_secs(60), &9), Some(9)); // one hit

    let stats = reg.stats();
    assert_eq!(
        stats.iter().map(|s| s.name).collect::<Vec<_>>(),
        ["regtest.cell", "regtest.keyed"],
        "registration order, nothing else's"
    );
    assert_eq!(
        (stats[0].entries, stats[0].hits, stats[0].misses),
        (1, 1, 0)
    );
    assert_eq!(
        (stats[1].entries, stats[1].hits, stats[1].misses),
        (1, 1, 0)
    );
}

#[test]
fn clear_empties_every_cache_in_the_registry_but_keeps_counters_and_touches_no_other() {
    let reg = Registry::new();
    let other = Registry::new();
    let cell = TtlCell::in_registry("regtest.clear.cell", &reg);
    let keyed: TtlCache<i32, i32> = TtlCache::in_registry("regtest.clear.keyed", 0, &reg);
    let bystander = TtlCell::in_registry("regtest.bystander", &other);
    cell.set(1);
    keyed.insert(1, 1);
    bystander.set(7);
    assert_eq!(cell.get(Duration::from_secs(60)), Some(1));

    reg.clear();

    assert_eq!(cell.get(Duration::from_secs(60)), None);
    assert_eq!(keyed.get(Duration::from_secs(60), &1), None);
    assert_eq!(
        bystander.get(Duration::from_secs(60)),
        Some(7),
        "another registry's cache is nobody's to clear"
    );
    let hits = reg
        .stats()
        .iter()
        .find(|s| s.name == "regtest.clear.cell")
        .map(|s| s.hits)
        .unwrap_or(0);
    assert!(hits >= 1, "clear leaves counters standing");
}

#[test]
fn a_dropped_cache_falls_out_of_the_registry() {
    let reg = Registry::new();
    {
        let _tmp: TtlCell<i32> = TtlCell::in_registry("regtest.dropped", &reg);
        assert!(reg.stats().iter().any(|s| s.name == "regtest.dropped"));
    }
    // After the walk prunes dead weak handles, the dropped cache is gone.
    assert!(!reg.stats().iter().any(|s| s.name == "regtest.dropped"));
    assert!(reg.stats().is_empty());
}

#[test]
fn a_plain_constructor_joins_the_process_wide_registry() {
    // Read only: another test may be counting its own caches in the global
    // registry, so nothing here clears it.
    let cell = TtlCell::new("regtest.global.cell");
    cell.set(3);
    assert!(
        all_stats()
            .iter()
            .any(|s| s.name == "regtest.global.cell" && s.entries == 1),
        "`new` registers with `Registry::global`, which `all_stats` reads"
    );
    assert!(
        Registry::global()
            .stats()
            .iter()
            .any(|s| s.name == "regtest.global.cell"),
        "the same registry by name"
    );
}
