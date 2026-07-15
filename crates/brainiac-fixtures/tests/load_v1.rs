//! The fixture tree itself is under test: loading fixtures/v1 must succeed
//! (all referential + semantic integrity checks green), and the loaded
//! corpus must match the seed dimensions the eval profiles assume.

use brainiac_fixtures::load;

#[test]
fn fixtures_v1_load_and_validate() {
    let fx = load(brainiac_fixtures::loader::default_root()).expect("fixture tree must validate");

    assert_eq!(fx.org.teams.len(), 3, "Meridian has exactly 3 teams");
    assert_eq!(fx.merges.merge_sets.len(), 12, "12 collision sets");
    assert_eq!(fx.merges.negative_pairs.len(), 6, "6 near-miss traps");
    assert_eq!(fx.contradictions.cases.len(), 12, "12 contradiction cases");
    assert_eq!(fx.temporal.cases.len(), 14, "14 as-of cases");
    assert_eq!(fx.transcripts.len(), 9, "9 seed transcripts");
    assert!(
        fx.memories.memories.len() >= 80,
        "expanded corpus >= 80 gold memories"
    );
    assert!(fx.qa.queries.len() >= 54, "retrieval QA >= 54 queries");
    assert_eq!(fx.leak.queries.len(), 15, "15 RLS leak tests");
    // Guards the vacuous-pass regression: if pages.yaml stops loading, every
    // composition-gold check and the zero-tolerance leak gate would iterate zero
    // items and report green. Nothing else asserts this.
    assert_eq!(
        fx.documents.documents.len(),
        2,
        "2 composition-gold pages — an empty list means the docs+leak gates are vacuous"
    );

    // Every stratum represented.
    for stratum in [
        "semantic",
        "exact_identifier",
        "cross_team_graph",
        "temporal",
        "negative",
        "czech",
    ] {
        assert!(
            fx.qa.queries.iter().any(|q| q.stratum == stratum),
            "stratum {stratum} missing from seed QA"
        );
    }

    // All three visibility tiers exercised (leak tests depend on it).
    for vis in ["org", "team", "private"] {
        assert!(
            fx.memories.memories.iter().any(|m| m.visibility == vis),
            "no {vis}-visibility memory in the corpus"
        );
    }

    // Czech slice present.
    assert!(fx.memories.memories.iter().any(|m| m.language == "cs"));
}

#[test]
fn corrupted_tree_fails_validation() {
    // Copy the tree to a temp dir, break one reference, expect load() to fail.
    let src = brainiac_fixtures::loader::default_root();
    let dst = std::env::temp_dir().join(format!("brainiac-fx-{}", std::process::id()));
    copy_dir(&src, &dst).expect("copy fixture tree");

    let qa = dst.join("retrieval/qa.yaml");
    let broken = std::fs::read_to_string(&qa)
        .expect("read qa")
        .replace("mem-pay-0042", "mem-pay-9999");
    std::fs::write(&qa, broken).expect("write corrupted qa");

    let err = brainiac_fixtures::load(&dst).expect_err("dangling memory ref must fail");
    assert!(
        err.to_string().contains("mem-pay-9999"),
        "error should name the dangling id: {err}"
    );

    let _ = std::fs::remove_dir_all(&dst);
}

fn copy_dir(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let target = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}
