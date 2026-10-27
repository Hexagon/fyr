//! Place index tests against real (hand-built) MBTiles and PMTiles maps.
//!
//! Jobs run synchronously with `run_job` wherever possible; the worker
//! thread test waits explicitly for each state it asserts.

use super::fixtures::{self, build_tiles, feature, write_mbtiles, write_pmtiles, PmtilesOptions};
use super::mvt::encode::{self, TestFeature};
use super::store::{self, PlaceRow};
use super::worker::{run_job, BuildOutcome, JobOutcome, PlaceJob, PlacesError, PlacesSettings};
use super::*;
use crate::fulltext::worker::CancelReason;
use crate::fulltext::Interrupt;
use crate::search::{self, Deadline, SearchKind, SearchQuery, SearchRoots};
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

struct Fixture {
    root: PathBuf,
    settings: PlacesSettings,
}

impl Fixture {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!("fyr-places-{label}-{}", uuid::Uuid::new_v4()));
        let maps = root.join("maps");
        std::fs::create_dir_all(&maps).unwrap();
        let settings = PlacesSettings {
            index_dir: root.join("index"),
            maps_dir: maps,
            enabled: true,
            auto_max_bytes: 64 * 1024 * 1024,
            min_free_bytes: 0,
            throttle: 0.0,
            chunk_tiles: 256,
            rescan_interval: Duration::from_millis(50),
        };
        Self { root, settings }
    }

    fn map(&self, name: &str) -> PathBuf {
        self.settings.maps_dir.join(name)
    }

    fn index(&self, name: &str) -> PathBuf {
        index_path(&self.settings.index_dir, name)
    }

    fn run(&self, name: &str) -> JobOutcome {
        run_job(&self.settings, name, &Arc::new(PlaceJob::new(true)))
    }

    fn reader(&self, name: &str) -> Option<PlaceReader> {
        open_reader(&self.settings.index_dir, name, &self.map(name))
    }

    fn write_omt_mbtiles(&self, name: &str) {
        write_mbtiles(&self.map(name), &build_tiles(&fixtures::omt_features()), "pbf");
    }

    fn write_protomaps_pmtiles(&self, name: &str, options: PmtilesOptions) {
        write_pmtiles(&self.map(name), &build_tiles(&fixtures::protomaps_features()), options);
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn far() -> Interrupt {
    Interrupt::at(Instant::now() + Duration::from_secs(30))
}

fn find(reader: &PlaceReader, text: &str) -> Vec<PlaceRow> {
    let expr = crate::fulltext::all_terms_expr(text).expect("terms");
    reader.search(&expr, 50, &far()).expect("search")
}

fn names(rows: &[PlaceRow]) -> Vec<&str> {
    rows.iter().map(|r| r.name.as_str()).collect()
}

fn roots(fx: &Fixture) -> SearchRoots {
    SearchRoots {
        books: fx.root.join("books"),
        maps: fx.settings.maps_dir.clone(),
        poi: fx.root.join("poi"),
        index: None,
        places: Some(fx.settings.index_dir.clone()),
    }
}

fn place_search(fx: &Fixture, text: &str) -> Vec<search::SearchHit> {
    let kinds: HashSet<SearchKind> = [SearchKind::Place].into_iter().collect();
    let outcome = search::run_search(
        &roots(fx),
        &SearchQuery::new(text).unwrap(),
        &kinds,
        &Deadline::after(Duration::from_secs(10)),
    );
    let (hits, _, _) = search::merge_results(outcome.hits, 40, 5);
    hits
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

#[test]
fn extraction_reads_place_layers_and_locates_features_in_both_hemispheres() {
    let tiles = build_tiles(&fixtures::omt_features());
    let mut found: Vec<Place> = Vec::new();
    for (&(z, x, y), data) in &tiles {
        for candidate in extract_candidates(data).unwrap() {
            found.extend(locate(&candidate, z, x, y));
        }
    }
    // One quantisation unit at z3 is ~1.2 km; at z14 well below 10 m.
    for (name, lon, lat) in [("Stockholm", 18.0686, 59.3293), ("Sydney", 151.2093, -33.8688), ("Santiago", -70.6693, -33.4489), ("Reykjavík", -21.9426, 64.1466)] {
        for zoom in [3u8, 14] {
            let place = found.iter().find(|p| p.name == name && p.zoom == zoom).unwrap_or_else(|| panic!("{name} z{zoom}"));
            let tolerance_km = if zoom == 3 { 2.0 } else { 0.02 };
            assert!(haversine_km(lon, lat, place.lon, place.lat) < tolerance_km, "{name} z{zoom}: {place:?}");
        }
    }
    let gbg = found.iter().find(|p| p.name == "Göteborg").unwrap();
    assert_eq!(gbg.alt, vec!["Gothenburg".to_string()]);
    assert_eq!((gbg.category, gbg.kind.as_str()), (Category::Place, "city"));
    let peak = found.iter().find(|p| p.name == "Kebnekaise").unwrap();
    assert_eq!((peak.category, peak.kind.as_str()), (Category::Natural, "peak"));
    let lake = found.iter().find(|p| p.name == "Vänern").unwrap();
    assert_eq!((lake.category, lake.kind.as_str()), (Category::Water, "lake"));
    // Unnamed features are skipped.
    assert!(!found.iter().any(|p| p.kind == "cafe"));
    // Importance: capital city > city > village; lower zooms add a bonus.
    let imp = |n: &str, z: u8| found.iter().find(|p| p.name == n && p.zoom == z).unwrap().importance;
    assert!(imp("Stockholm", 14) > imp("Malmö", 14));
    assert!(imp("Malmö", 14) > imp("Åre", 14));
    assert!(imp("Stockholm", 3) > imp("Stockholm", 14));
}

#[test]
fn buffer_features_lines_and_unknown_layers_are_handled() {
    let layer = encode::layer(
        "place",
        4096,
        &[
            TestFeature::point(-20, 100, &[("name", "Buffered".into()), ("class", "town".into())]),
            TestFeature::point(4096, 100, &[("name", "Edge".into())]),
            TestFeature::point(10, 10, &[("name", "  Inside \u{0007}Town ".into()), ("class", "Market Town".into())]),
            TestFeature::line(&[(0, 0), (10, 10)], &[("name", "Not a point".into())]),
        ],
    );
    let water = encode::layer(
        "water_name",
        4096,
        &[TestFeature::line(&[(100, 100), (200, 200), (300, 300)], &[("name", "Dalälven".into()), ("class", "river".into())])],
    );
    let unknown = encode::layer("roads", 4096, &[TestFeature::point(5, 5, &[("name", "E4".into())])]);
    let natural = encode::layer(
        "natural_label",
        4096,
        &[TestFeature::polygon(&[(1000, 1000), (1100, 1000), (1100, 1100), (1000, 1100)], &[("name", "Sarek".into()), ("class", "national_park".into())])],
    );
    let tile = encode::tile(&[layer, water, unknown, natural]);
    let candidates = extract_candidates(&tile).unwrap();
    let names: Vec<&str> = candidates.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["Inside Town", "Dalälven", "Sarek"]);
    assert_eq!((candidates[2].px, candidates[2].py), (1050.0, 1050.0));
    assert_eq!((candidates[2].category, candidates[2].kind.as_str()), (Category::Park, "national_park"));
    assert_eq!(candidates[0].kind, "market_town");
    assert_eq!(candidates[0].category, Category::Place);
    assert_eq!((candidates[1].px, candidates[1].py), (200.0, 200.0));
    assert_eq!(candidates[1].category, Category::Water);
}

#[test]
fn zoom_plan_and_link_zooms() {
    assert_eq!(zoom_plan(0, 14), vec![3, 6, 9, 12, 14]);
    assert_eq!(zoom_plan(0, 15), vec![3, 6, 9, 12, 14]);
    assert_eq!(zoom_plan(4, 12), vec![6, 9, 12]);
    assert_eq!(zoom_plan(0, 2), vec![2]);
    assert_eq!(zoom_plan(14, 14), vec![14]);
    assert_eq!(zoom_plan(15, 16), Vec::<u8>::new());
    assert_eq!(zoom_for("city", Category::Place), 11);
    assert_eq!(zoom_for("village", Category::Place), 14);
    assert_eq!(zoom_for("peak", Category::Natural), 14);
    assert_eq!(zoom_for("whatever", Category::Poi), 17);
}

#[test]
fn index_names_are_unique_and_path_free() {
    let a = index_file_name("sweden.pmtiles");
    let b = index_file_name("sweden.mbtiles");
    assert_ne!(a, b);
    assert!(a.starts_with("places-sweden-") && a.ends_with(".sqlite"));
    assert!(is_index_file_name(&a));
    assert!(!a.ends_with(".fts.sqlite"), "must not collide with full-text index cleanup");
    let odd = index_file_name("../../etc/x y.pmtiles");
    assert!(!odd.contains('/') && !odd.contains(' '));
    assert!(is_valid_map_name("a.PMTiles") && is_valid_map_name("b.mbtiles"));
    for bad in ["", ".hidden.pmtiles", "a/b.pmtiles", "a\\b.mbtiles", "notes.zim", "x.pmtiles\0"] {
        assert!(!is_valid_map_name(bad), "{bad:?}");
    }
}

// ---------------------------------------------------------------------------
// Indexing
// ---------------------------------------------------------------------------

#[test]
fn indexes_mbtiles_with_dedup_and_accent_insensitive_matching() {
    let fx = Fixture::new("mbtiles");
    fx.write_omt_mbtiles("sweden.mbtiles");
    let outcome = fx.run("sweden.mbtiles");
    let JobOutcome::Completed { places } = outcome else { panic!("{outcome:?}") };
    // 15 named features; the two Bergs are far apart and stay separate.
    assert_eq!(places, 15);
    assert_eq!(store::count_places(&fx.index("sweden.mbtiles")), Some(15));

    let reader = fx.reader("sweden.mbtiles").expect("reader");
    assert_eq!(reader.meta.state, crate::fulltext::IndexState::Ready);
    assert_eq!(reader.meta.zooms, vec![3, 6, 9, 12, 14]);
    assert_eq!(reader.meta.processed, reader.meta.total);
    // Stockholm appears in five zoom levels but is stored once, at the
    // precise z14 position.
    let stockholm = find(&reader, "stockholm");
    assert_eq!(names(&stockholm), vec!["Stockholm"]);
    assert!(haversine_km(18.0686, 59.3293, stockholm[0].lon, stockholm[0].lat) < 0.02);
    assert_eq!(names(&find(&reader, "malmo")), vec!["Malmö"]);
    assert_eq!(names(&find(&reader, "MALMÖ")), vec!["Malmö"]);
    assert_eq!(names(&find(&reader, "are")), vec!["Åre"]);
    assert_eq!(names(&find(&reader, "gothenburg")), vec!["Göteborg"]);
    assert_eq!(names(&find(&reader, "vaner")), vec!["Vänern"]);
    assert_eq!(names(&find(&reader, "kebne")), vec!["Kebnekaise"]);
    assert_eq!(names(&find(&reader, "reykjavik")), vec!["Reykjavík"]);
    let exact = crate::fulltext::exact_terms_expr("berg").unwrap();
    assert_eq!(reader.search(&exact, 50, &far()).unwrap().len(), 2, "two separate villages named Berg");
    assert_eq!(find(&reader, "berg").len(), 3, "prefix search adds Bergby");
    assert!(find(&reader, "atlantis").is_empty());
}

#[test]
fn indexes_pmtiles_with_leaf_directories_and_runs() {
    let fx = Fixture::new("pmtiles");
    // Leaf directories of two entries each, plus a run of identical tiles.
    let mut tiles = build_tiles(&fixtures::protomaps_features());
    let ocean = encode::tile(&[encode::layer("water", 4096, &[])]);
    for x in 0..6u32 {
        tiles.insert((12, 3000 + x, 1000), ocean.clone());
    }
    write_pmtiles(&fx.map("nordic.pmtiles"), &tiles, PmtilesOptions { leaf_size: 2, runs: true, tile_type: 1 });
    let outcome = fx.run("nordic.pmtiles");
    let JobOutcome::Completed { places } = outcome else { panic!("{outcome:?}") };
    assert_eq!(places, 5);
    let reader = fx.reader("nordic.pmtiles").unwrap();
    assert_eq!(reader.meta.zooms, vec![6, 9, 12]);
    let kiruna = find(&reader, "kiruna");
    assert_eq!(names(&kiruna), vec!["Kiruna"]);
    assert_eq!(kiruna[0].kind, "town");
    assert!(haversine_km(20.2253, 67.8558, kiruna[0].lon, kiruna[0].lat) < 0.05);
    let peak = find(&reader, "kebnekaise");
    assert_eq!((peak[0].category, peak[0].kind.as_str()), (Category::Natural, "peak"));
    let auckland = find(&reader, "auckland");
    assert!(haversine_km(174.7633, -36.8485, auckland[0].lon, auckland[0].lat) < 0.05);
    assert_eq!(names(&find(&reader, "vattern")), vec!["Vättern"]);

    // Same content without leaves or runs gives the same places.
    let fx2 = Fixture::new("pmtiles-flat");
    write_pmtiles(&fx2.map("nordic.pmtiles"), &tiles, PmtilesOptions { leaf_size: 0, runs: false, tile_type: 1 });
    assert_eq!(fx2.run("nordic.pmtiles"), JobOutcome::Completed { places: 5 });
}

#[test]
fn run_length_entries_place_features_in_every_tile_of_the_run() {
    let fx = Fixture::new("runs");
    // The same bytes at three consecutive tile IDs: the feature exists at
    // three different places on the globe.
    let tile = encode::tile(&[encode::layer("place", 4096, &[TestFeature::point(2048, 2048, &[("name", "Echo".into())])])]);
    let mut tiles = BTreeMap::new();
    for id in fixtures::first_id(2)..fixtures::first_id(2) + 3 {
        let (z, x, y) = tiles::tile_id_to_zxy(id).unwrap();
        tiles.insert((z, x, y), tile.clone());
    }
    write_pmtiles(&fx.map("echo.pmtiles"), &tiles, PmtilesOptions::default());
    assert_eq!(fx.run("echo.pmtiles"), JobOutcome::Completed { places: 3 });
}

#[test]
fn hostile_queries_never_error() {
    let fx = Fixture::new("hostile");
    fx.write_omt_mbtiles("sweden.mbtiles");
    fx.run("sweden.mbtiles");
    let reader = fx.reader("sweden.mbtiles").unwrap();
    for text in ["\"", "NEAR(a b)", "name:Stockholm", "*", "a OR b", "^", "stock\"holm", "-malmö", "( )", "Stockholm*"] {
        if let Some(expr) = crate::fulltext::all_terms_expr(text) {
            assert!(reader.search(&expr, 10, &far()).is_ok(), "{text}");
        }
        let _ = place_search(&fx, text);
    }
    assert_eq!(names(&find(&reader, "name:Stockholm")), Vec::<&str>::new(), "column filters are inert");
    let expired = Interrupt::at(Instant::now() - Duration::from_secs(1));
    assert!(matches!(
        reader.search("\"stockholm\"", 10, &expired),
        Err(crate::fulltext::store::QueryError::Interrupted)
    ));
}

#[test]
fn changed_maps_are_ignored_then_reindexed() {
    let fx = Fixture::new("reindex");
    fx.write_omt_mbtiles("sweden.mbtiles");
    fx.run("sweden.mbtiles");
    assert!(fx.reader("sweden.mbtiles").is_some());
    // Replace the map with different content (and size).
    let replacement = build_tiles(&[feature("place", 25.0, 65.0, &[10], &[("name", "Luleå".into()), ("class", "town".into())])]);
    write_mbtiles(&fx.map("sweden.mbtiles"), &replacement, "pbf");
    assert!(fx.reader("sweden.mbtiles").is_none(), "stale index must not be used");
    assert_eq!(fx.run("sweden.mbtiles"), JobOutcome::Completed { places: 1 });
    let reader = fx.reader("sweden.mbtiles").unwrap();
    assert_eq!(names(&find(&reader, "lulea")), vec!["Luleå"]);
    assert!(find(&reader, "stockholm").is_empty());
}

#[test]
fn interrupted_builds_resume_without_duplicates() {
    for format in ["mbtiles", "pmtiles"] {
        let fx = Fixture::new("resume");
        let name = format!("sweden.{format}");
        let tiles = build_tiles(&fixtures::omt_features());
        if format == "mbtiles" {
            write_mbtiles(&fx.map(&name), &tiles, "pbf");
        } else {
            write_pmtiles(&fx.map(&name), &tiles, PmtilesOptions { leaf_size: 3, ..Default::default() });
        }
        let mut settings = fx.settings.clone();
        settings.chunk_tiles = 2;
        let mut stops = 0;
        loop {
            let control = Arc::new(PlaceJob {
                max_chunks: Some(1),
                ..PlaceJob::new(true)
            });
            match run_job(&settings, &name, &control) {
                JobOutcome::Cancelled(CancelReason::Yield) => {
                    stops += 1;
                    let meta = store::inspect(&fx.index(&name)).unwrap();
                    assert_eq!(meta.state, crate::fulltext::IndexState::Building);
                    assert!(meta.processed > 0 && meta.processed <= meta.total);
                    // Partially built indexes are already searchable.
                    if meta.places > 0 {
                        assert!(fx.reader(&name).is_some());
                    }
                }
                JobOutcome::Completed { places } => {
                    assert_eq!(places, 15, "{format}");
                    break;
                }
                other => panic!("{format}: {other:?}"),
            }
            assert!(stops < 200, "no progress");
        }
        assert!(stops > 5, "{format}: expected many resumes, got {stops}");
        assert_eq!(store::count_places(&fx.index(&name)), Some(15));
        let reader = fx.reader(&name).unwrap();
        assert_eq!(find(&reader, "stockholm").len(), 1);
        assert_eq!(reader.meta.processed, reader.meta.total);
    }
}

#[test]
fn cancellation_rolls_back_the_open_chunk() {
    let fx = Fixture::new("cancel");
    fx.write_omt_mbtiles("sweden.mbtiles");
    let control = Arc::new(PlaceJob::new(false));
    control.cancel(CancelReason::Removed);
    assert_eq!(run_job(&fx.settings, "sweden.mbtiles", &control), JobOutcome::Cancelled(CancelReason::Removed));
    if let Some(meta) = store::inspect(&fx.index("sweden.mbtiles")) {
        assert_eq!(meta.places, 0);
        assert_eq!(store::count_places(&fx.index("sweden.mbtiles")), Some(0));
    }
}

#[test]
fn disk_space_guard_stops_indexing() {
    let mut fx = Fixture::new("disk");
    fx.write_omt_mbtiles("sweden.mbtiles");
    fx.settings.min_free_bytes = u64::MAX;
    assert_eq!(fx.run("sweden.mbtiles"), JobOutcome::LowDisk);
    assert!(fx.reader("sweden.mbtiles").is_none());
}

#[test]
fn raster_and_broken_maps_are_reported_not_indexed() {
    let fx = Fixture::new("raster");
    let mut png = BTreeMap::new();
    png.insert((0u8, 0u32, 0u32), b"\x89PNG\r\n\x1a\nfake".to_vec());
    write_mbtiles(&fx.map("aerial.mbtiles"), &png, "png");
    write_pmtiles(&fx.map("aerial.pmtiles"), &png, PmtilesOptions { tile_type: 2, ..Default::default() });
    std::fs::write(fx.map("broken.pmtiles"), b"PMTiles but not really").unwrap();
    std::fs::write(fx.map("broken.mbtiles"), b"not sqlite at all").unwrap();
    for name in ["aerial.mbtiles", "aerial.pmtiles", "broken.pmtiles", "broken.mbtiles"] {
        assert!(matches!(fx.run(name), JobOutcome::Unsupported(_)), "{name}");
        assert!(!fx.index(name).exists(), "{name}: no index file for unsupported maps");
    }
    let indexer = PlaceIndexer::new(fx.settings.clone());
    assert!(matches!(indexer.request_build("aerial.mbtiles"), Err(PlacesError::Unsupported(_))));
    let status = indexer.status();
    let aerial = status.maps.iter().find(|m| m.filename == "aerial.pmtiles").unwrap();
    assert_eq!((aerial.state, aerial.content), ("unsupported", "raster"));
    assert!(aerial.note.as_deref().unwrap().contains("Raster"));
    let broken = status.maps.iter().find(|m| m.filename == "broken.mbtiles").unwrap();
    assert_eq!(broken.state, "unsupported");
}

#[test]
fn corrupt_tiles_are_skipped_and_counted() {
    let fx = Fixture::new("corrupt");
    let mut tiles = build_tiles(&fixtures::omt_features());
    tiles.insert((14, 1, 1), fixtures::garbage_tile());
    tiles.insert((14, 2, 2), vec![0xff; 64]);
    write_mbtiles(&fx.map("mixed.mbtiles"), &tiles, "pbf");
    // A tile whose gzip stream is cut short.
    let conn = rusqlite::Connection::open(fx.map("mixed.mbtiles")).unwrap();
    conn.execute("INSERT INTO tiles VALUES (14, 3, 3, ?1)", [vec![0x1f, 0x8b, 0x08, 0x00, 0x01]]).unwrap();
    drop(conn);
    assert_eq!(fx.run("mixed.mbtiles"), JobOutcome::Completed { places: 15 });
    let meta = store::inspect(&fx.index("mixed.mbtiles")).unwrap();
    assert_eq!(meta.bad_tiles, 3);
    let indexer = PlaceIndexer::new(fx.settings.clone());
    let status = indexer.status();
    assert!(status.maps[0].note.as_deref().unwrap().contains("3 tiles"));
}

#[test]
fn damaged_pmtiles_directories_fail_cleanly() {
    let fx = Fixture::new("damaged");
    fx.write_protomaps_pmtiles("nordic.pmtiles", PmtilesOptions { leaf_size: 2, ..Default::default() });
    // Point the leaf directory section past the end of the file.
    let mut bytes = std::fs::read(fx.map("nordic.pmtiles")).unwrap();
    bytes[48..56].copy_from_slice(&1u64.to_le_bytes());
    std::fs::write(fx.map("nordic.pmtiles"), &bytes).unwrap();
    let outcome = fx.run("nordic.pmtiles");
    assert!(matches!(outcome, JobOutcome::Unsupported(_) | JobOutcome::Failed(_)), "{outcome:?}");
}

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

#[test]
fn global_search_ranks_by_match_quality_then_importance() {
    let fx = Fixture::new("search");
    fx.write_omt_mbtiles("sweden.mbtiles");
    fx.run("sweden.mbtiles");

    let hits = place_search(&fx, "malmo");
    assert_eq!(hits[0].title, "Malmö");
    assert_eq!(hits[0].kind, SearchKind::Place);
    assert_eq!(hits[0].place_kind.as_deref(), Some("city"));
    assert_eq!(hits[0].source, "sweden.mbtiles");
    assert!(hits[0].link.starts_with("/maps?map=sweden.mbtiles&lat=55.60"), "{}", hits[0].link);
    assert!(hits[0].link.ends_with("&zoom=11&label=Malm%C3%B6"), "{}", hits[0].link);

    // Exact "Berg" villages outrank the more important town "Bergby".
    let berg: Vec<String> = place_search(&fx, "Berg").into_iter().map(|h| h.title).collect();
    assert_eq!(berg, vec!["Berg", "Berg", "Bergby"]);
    // Same tier (word prefix): the big city wins on importance.
    let sankt: Vec<String> = place_search(&fx, "sankt").into_iter().map(|h| h.title).collect();
    assert_eq!(sankt, vec!["Sankt Petersburg", "Sankt Anna"]);
    // Alternative names match and are shown.
    let gbg = place_search(&fx, "Gothenburg");
    assert_eq!(gbg[0].title, "Göteborg");
    assert_eq!(gbg[0].subtitle.as_deref(), Some("City · Gothenburg"));
    // Peaks link at their own zoom.
    assert!(place_search(&fx, "Kebnekaise")[0].link.contains("&zoom=14&"));
    // Southern / western hemisphere coordinates survive the round trip.
    let santiago = &place_search(&fx, "santiago")[0];
    assert!((santiago.lat.unwrap() + 33.4489).abs() < 0.001 && (santiago.lng.unwrap() + 70.6693).abs() < 0.001);
}

#[test]
fn places_found_in_several_maps_are_merged() {
    let fx = Fixture::new("merge");
    fx.write_omt_mbtiles("sweden.mbtiles");
    fx.write_protomaps_pmtiles("nordic.pmtiles", PmtilesOptions::default());
    fx.run("sweden.mbtiles");
    fx.run("nordic.pmtiles");
    let stockholm: Vec<_> = place_search(&fx, "stockholm").into_iter().filter(|h| h.title == "Stockholm").collect();
    assert_eq!(stockholm.len(), 1, "{stockholm:?}");
    // A place only in one map links to that map.
    let kiruna = place_search(&fx, "kiruna");
    assert_eq!(kiruna[0].source, "nordic.pmtiles");
    assert!(kiruna[0].link.starts_with("/maps?map=nordic.pmtiles&"));
    // Disabled place search returns nothing.
    let off = SearchRoots { places: None, ..roots(&fx) };
    let kinds: HashSet<SearchKind> = SearchKind::ALL.into_iter().collect();
    let outcome = search::run_search(&off, &SearchQuery::new("kiruna").unwrap(), &kinds, &Deadline::after(Duration::from_secs(5)));
    assert!(outcome.hits.iter().all(|h| h.kind != SearchKind::Place));
}

#[test]
fn search_respects_the_time_budget() {
    let fx = Fixture::new("budget");
    fx.write_omt_mbtiles("sweden.mbtiles");
    fx.run("sweden.mbtiles");
    let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let deadline = Deadline::new(Duration::from_secs(10), cancelled);
    let (hits, partial) = search::search_places(&fx.settings.index_dir, &fx.settings.maps_dir, &SearchQuery::new("stockholm").unwrap(), &deadline);
    assert!(hits.is_empty());
    assert!(partial);
}

// ---------------------------------------------------------------------------
// Worker thread
// ---------------------------------------------------------------------------

fn wait_for(indexer: &PlaceIndexer, file: &str, state: &str) -> worker::MapPlacesStatus {
    let end = Instant::now() + Duration::from_secs(20);
    loop {
        let status = indexer.status();
        if let Some(entry) = status.maps.iter().find(|m| m.filename == file) {
            if entry.state == state {
                return entry.clone();
            }
        }
        assert!(Instant::now() < end, "{file} never reached {state}: {:?}", status.maps);
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn worker_indexes_automatically_and_obeys_admin_controls() {
    let mut fx = Fixture::new("worker");
    fx.write_omt_mbtiles("sweden.mbtiles");
    fx.write_protomaps_pmtiles("nordic.pmtiles", PmtilesOptions::default());
    // Only maps up to the size of nordic.pmtiles are automatic.
    let sweden = std::fs::metadata(fx.map("sweden.mbtiles")).unwrap().len();
    let nordic = std::fs::metadata(fx.map("nordic.pmtiles")).unwrap().len();
    assert!(nordic < sweden, "fixture sizes");
    fx.settings.auto_max_bytes = nordic - 1;
    let indexer = PlaceIndexer::new(fx.settings.clone());
    indexer.start();
    // Neither is automatic now (both are larger than the cap).
    std::thread::sleep(Duration::from_millis(200));
    let status = indexer.status();
    assert!(status.maps.iter().all(|m| m.state == "not_indexed" && !m.auto), "{:?}", status.maps);

    assert_eq!(indexer.request_build("nordic.pmtiles"), Ok(BuildOutcome::Queued));
    let ready = wait_for(&indexer, "nordic.pmtiles", "ready");
    assert_eq!((ready.places, ready.percent), (5, 100.0));
    assert!(ready.index_bytes > 0);
    assert_eq!(indexer.request_build("nordic.pmtiles"), Ok(BuildOutcome::AlreadyReady));
    assert_eq!(indexer.request_build("missing.pmtiles"), Err(PlacesError::NotFound));
    assert_eq!(indexer.request_build("../x.pmtiles"), Err(PlacesError::InvalidName));

    // Delete excludes the map from automatic indexing.
    indexer.remove("nordic.pmtiles", Duration::from_secs(10)).unwrap();
    assert!(!fx.index("nordic.pmtiles").exists());
    wait_for(&indexer, "nordic.pmtiles", "excluded");
    // A removed map's orphaned index is cleaned up.
    assert_eq!(indexer.request_build("sweden.mbtiles"), Ok(BuildOutcome::Queued));
    wait_for(&indexer, "sweden.mbtiles", "ready");
    indexer.forget_map("sweden.mbtiles", Duration::from_secs(10));
    assert!(!fx.index("sweden.mbtiles").exists());
    indexer.shutdown(Duration::from_secs(5));
    // Exclusions survive restarts.
    let again = PlaceIndexer::new(fx.settings.clone());
    assert!(again.status().maps.iter().any(|m| m.filename == "nordic.pmtiles" && m.state == "excluded"));
}

#[test]
fn shutdown_leaves_a_running_job_resumable() {
    let mut fx = Fixture::new("shutdown");
    fx.write_omt_mbtiles("sweden.mbtiles");
    fx.settings.chunk_tiles = 1;
    fx.settings.throttle = 50.0; // long pauses between chunks
    let indexer = PlaceIndexer::new(fx.settings.clone());
    indexer.start();
    wait_for(&indexer, "sweden.mbtiles", "indexing");
    indexer.shutdown(Duration::from_secs(10));
    let meta = store::inspect(&fx.index("sweden.mbtiles")).expect("index kept");
    assert_eq!(meta.state, crate::fulltext::IndexState::Building);
    fx.settings.throttle = 0.0;
    fx.settings.chunk_tiles = 256;
    assert_eq!(fx.run("sweden.mbtiles"), JobOutcome::Completed { places: 15 });
}

// ---------------------------------------------------------------------------
// Throughput (ignored): cargo test -p server --release -- --ignored places_throughput --nocapture
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn places_throughput() {
    let fx = Fixture::new("throughput");
    // A z14 grid of 96 × 96 tiles, each with 12 named places and road-like
    // filler, plus the lower zoom passes.
    let mut features = Vec::new();
    for i in 0..(96 * 12) {
        for j in 0..96 {
            let lon = 11.0 + (i as f64) * (96.0 * 360.0 / 16384.0) / (96.0 * 12.0) + 0.0001;
            let lat = 58.0 + (j as f64) * 0.0139;
            features.push(feature("place", lon, lat, &[9, 12, 14], &[("name", format!("Ort {i}-{j}").as_str().into()), ("class", "hamlet".into())]));
        }
    }
    let mut tiles = build_tiles(&features);
    let filler = encode::layer("transportation", 4096, &(0..200).map(|k| TestFeature::line(&[(k, 0), (k * 2, 4000), (4000, k)], &[("class", "minor".into())])).collect::<Vec<_>>());
    for value in tiles.values_mut() {
        let mut with_filler = value.clone();
        encode::bytes_field(3, &filler, &mut with_filler);
        *value = with_filler;
    }
    write_pmtiles(&fx.map("grid.pmtiles"), &tiles, PmtilesOptions { leaf_size: 512, ..Default::default() });
    let size = std::fs::metadata(fx.map("grid.pmtiles")).unwrap().len();
    let started = Instant::now();
    let outcome = fx.run("grid.pmtiles");
    let elapsed = started.elapsed();
    let JobOutcome::Completed { places } = outcome else { panic!("{outcome:?}") };
    let meta = store::inspect(&fx.index("grid.pmtiles")).unwrap();
    println!(
        "places_throughput: {} tiles, {:.1} MB map, {} places in {:.2?} → {:.0} tiles/s, {:.0} places/s, index {:.1} MB",
        meta.total,
        size as f64 / 1e6,
        places,
        elapsed,
        meta.total as f64 / elapsed.as_secs_f64(),
        places as f64 / elapsed.as_secs_f64(),
        crate::fulltext::index_size(&fx.index("grid.pmtiles")) as f64 / 1e6
    );
    let reader = fx.reader("grid.pmtiles").unwrap();
    let started = Instant::now();
    let hits = find(&reader, "ort 500");
    println!("places_throughput: query took {:.2?} ({} rows)", started.elapsed(), hits.len());
}
