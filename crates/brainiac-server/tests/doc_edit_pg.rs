//! KB4: a human edits a page, and the truth does not fork.
//!
//! This is the asymmetry (KB-PLAN D1) as a product experience rather than an
//! architecture diagram, driven through the real HTTP surface:
//!
//! - Editing a **pinned** section SAVES. It is the human's own prose and
//!   regeneration never touches it.
//! - Editing a **composed** section does NOT save. It is a projection of the
//!   org's memories, so writing the text into the page would fork the truth: the
//!   page would say one thing and the memory layer another, and the next
//!   recompose would silently revert the human — the single most infuriating
//!   thing a wiki can do to someone who took the time to fix it. The edit goes
//!   through EXTRACTION instead, becomes proposed knowledge, and the section
//!   regenerates once a maintainer approves it.
//!
//! The response says "captured", not "saved", and the test asserts that
//! distinction — a tool that says "saved" when it means "queued for someone
//! else's approval" has lied to the person most likely to notice.

use brainiac_core::embed::DeterministicEmbedder;
use brainiac_core::{DocKind, SectionBinding, SectionMode, Visibility};
use brainiac_store::documents::{NewDocument, NewSection};
use brainiac_store::Store;
use uuid::Uuid;

#[tokio::test]
async fn a_human_edit_is_captured_as_knowledge_not_written_into_the_page() {
    let Some(url) = std::env::var("DATABASE_URL").ok() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };
    // Cross-binary + in-process serialization: see brainiac_store::test_support.
    let _guard = brainiac_store::test_support::serial_guard(&url).await;
    brainiac_store::migrate(&url).await.expect("migrate");
    let admin = sqlx::PgPool::connect(&url).await.expect("admin");
    sqlx::query(
        "TRUNCATE document_reads, document_publications, publish_targets, document_dependencies,
                  document_revisions, document_sections, documents,
                  memory_entities, memory_embeddings, entity_links, edges, contradictions,
                  promotions, memories, canonical_entities, entities, provenance, sources,
                  team_members, users, teams, orgs, pipeline_runs, queue.jobs, queue.archive
         CASCADE",
    )
    .execute(&admin)
    .await
    .expect("truncate");

    let store = Store::connect(&url).await.expect("connect");
    let (org, team, user) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());

    let p = brainiac_pipeline::pipeline_principal(org);
    let mut tx = store.scoped_tx(&p).await.expect("tx");
    brainiac_store::orgs::upsert_org(&mut tx, org, "meridian")
        .await
        .expect("org");
    brainiac_store::orgs::upsert_team(&mut tx, team, org, "payments")
        .await
        .expect("team");
    brainiac_store::orgs::upsert_user(&mut tx, user, org, "lead@meridian.test")
        .await
        .expect("user");
    brainiac_store::orgs::upsert_member(&mut tx, team, user, "maintainer")
        .await
        .expect("member");

    let doc_id = Uuid::new_v4();
    let composed_id = Uuid::new_v4();
    let pinned_id = Uuid::new_v4();
    brainiac_store::documents::insert_document(
        &mut tx,
        &NewDocument {
            id: doc_id,
            org_id: org,
            team_id: Some(team),
            slug: "retry-policy".into(),
            title: "Retry policy".into(),
            visibility: Visibility::Org,
            doc_kind: DocKind::TopicPage,
        },
    )
    .await
    .expect("doc");
    brainiac_store::documents::insert_section(
        &mut tx,
        &NewSection {
            id: composed_id,
            document_id: doc_id,
            org_id: org,
            position: 0,
            heading: "What we know".into(),
            mode: SectionMode::Composed,
            binding: Some(SectionBinding {
                query: "retry".into(),
                max_items: 5,
                ..Default::default()
            }),
            pinned_content: None,
        },
    )
    .await
    .expect("composed section");
    brainiac_store::documents::insert_section(
        &mut tx,
        &NewSection {
            id: pinned_id,
            document_id: doc_id,
            org_id: org,
            position: 1,
            heading: "Ownership".into(),
            mode: SectionMode::Pinned,
            binding: None,
            pinned_content: Some("Owned by payments.".into()),
        },
    )
    .await
    .expect("pinned section");
    tx.commit().await.expect("commit");

    let tokens = serde_json::json!({
        "tok_lead": {"org": org, "user": user, "teams": [team]},
    });
    std::env::set_var("BRAINIAC_TOKENS", tokens.to_string());

    let app = brainiac_server::http::router(
        store.clone(),
        std::sync::Arc::new(DeterministicEmbedder::default()),
        None,
    )
    .await
    .expect("router");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });
    let base = format!("http://{addr}");
    let http = reqwest::Client::new();

    // ── the composed section: captured, NOT saved ───────────────────────
    let r = http
        .post(format!("{base}/v1/docs/retry-policy/edit"))
        .bearer_auth("tok_lead")
        .json(&serde_json::json!({
            "section_id": composed_id,
            "content": "The refund-worker retry cap is 45 seconds, not 30 — we raised it again after the June incident.",
            "note": "the page is out of date"
        }))
        .send()
        .await
        .expect("edit composed");
    assert_eq!(r.status(), reqwest::StatusCode::OK);
    let body: serde_json::Value = r.json().await.expect("json");

    assert_eq!(
        body["outcome"], "captured",
        "a composed-section edit must NOT be written into the page: {body}"
    );
    assert!(
        body["message"]
            .as_str()
            .expect("message")
            .contains("review"),
        "the editor must be told their change goes through the review gate: {body}"
    );
    let source_id: Uuid = body["source_id"]
        .as_str()
        .expect("source_id")
        .parse()
        .expect("uuid");
    assert!(
        body["job_id"].as_i64().is_some(),
        "the edit must be queued for extraction"
    );

    // The edit exists as an INGEST SOURCE — the same door a session transcript
    // comes through — and carries the human's reason, which is the part a diff
    // could never recover.
    let mut tx = store.scoped_tx(&p).await.expect("tx");
    let (_, raw) = brainiac_store::governance::get_source_text(&mut tx, source_id)
        .await
        .expect("source")
        .expect("exists");
    assert!(raw.contains("45 seconds"));
    assert!(
        raw.contains("the page is out of date"),
        "the reason was dropped"
    );

    // And the section itself is untouched: still a binding, still composed.
    let sections = brainiac_store::documents::sections(&mut tx, doc_id)
        .await
        .expect("sections");
    let composed = sections
        .iter()
        .find(|s| s.id == composed_id)
        .expect("section");
    assert_eq!(composed.mode, SectionMode::Composed);
    assert!(
        composed.pinned_content.is_none(),
        "the human's text was written into a composed section — the truth just forked"
    );
    tx.commit().await.expect("commit");

    // ── the pinned section: saved, because it is theirs ─────────────────
    let r = http
        .post(format!("{base}/v1/docs/retry-policy/edit"))
        .bearer_auth("tok_lead")
        .json(&serde_json::json!({
            "section_id": pinned_id,
            "content": "Owned by payments. Page #pay-oncall before changing retry behaviour."
        }))
        .send()
        .await
        .expect("edit pinned");
    assert_eq!(r.status(), reqwest::StatusCode::OK);
    let body: serde_json::Value = r.json().await.expect("json");
    assert_eq!(body["outcome"], "saved");

    let mut tx = store.scoped_tx(&p).await.expect("tx");
    let sections = brainiac_store::documents::sections(&mut tx, doc_id)
        .await
        .expect("sections");
    let pinned = sections
        .iter()
        .find(|s| s.id == pinned_id)
        .expect("section");
    assert!(pinned
        .pinned_content
        .as_deref()
        .expect("content")
        .contains("#pay-oncall"));

    // The page is dirty, so the human's prose reaches the published markdown on
    // the next compose — a pinned edit that never lands in a revision is
    // invisible, which is its own kind of lie.
    let doc = brainiac_store::documents::get_document(&mut tx, doc_id)
        .await
        .expect("q")
        .expect("doc");
    assert!(
        doc.dirty_at.is_some(),
        "the pinned edit never queued a recompose"
    );
    tx.commit().await.expect("commit");

    // ── read analytics (0025): no revision yet = nothing was consumed ──────
    // The page has sections but no composed revision at this point. The GET
    // succeeds (skeleton + metadata), but no CONTENT was served, so recording
    // a "read" would inflate the consumption numbers with page-loads that
    // taught the reader nothing.
    let r = http
        .get(format!("{base}/v1/docs/retry-policy"))
        .bearer_auth("tok_lead")
        .send()
        .await
        .expect("read revision-less page");
    assert_eq!(r.status(), reqwest::StatusCode::OK);
    let mut tx = store.scoped_tx(&p).await.expect("tx");
    let reads: Vec<(String, bool)> = sqlx::query_as(
        "SELECT via, was_dirty FROM document_reads WHERE document_id = $1 ORDER BY read_at",
    )
    .bind(doc_id)
    .fetch_all(&mut *tx)
    .await
    .expect("reads");
    tx.commit().await.expect("commit");
    assert!(
        reads.is_empty(),
        "a page view that served no revision content must not count as a read: {reads:?}"
    );

    // ── the propagation SLA, end to end (KB4) ───────────────────────────
    // The product's promise is that knowledge changing reaches every page BY
    // ITSELF. Measure it: recompose, and assert the page came back clean. A
    // page that stays dirty after a compose pass is the promise failing, and
    // Knowledge Health now reports exactly that (`pages_dirty`,
    // `oldest_dirty_secs`) so it can go red in front of a leader.
    let embedder = DeterministicEmbedder::default();
    let providers = brainiac_gateway::ProviderRouter::single(std::sync::Arc::new(
        brainiac_gateway::MockProvider::new(|_| "(no knowledge captured yet)".to_string()),
    ));
    let mut tx = store.scoped_tx(&p).await.expect("tx");
    let version = brainiac_store::memories::ensure_embedding_version(
        &mut tx,
        brainiac_core::embed::Embedder::model_name(&embedder),
        brainiac_core::embed::Embedder::dim(&embedder) as i32,
    )
    .await
    .expect("version");
    tx.commit().await.expect("commit");

    let started = std::time::Instant::now();
    let stats =
        brainiac_pipeline::worker::compose_tick(&store, &providers, &embedder, version, org, 10)
            .await
            .expect("compose");
    let elapsed = started.elapsed();
    assert_eq!(stats.composed, 1, "the dirty page did not recompose");

    let mut tx = store.scoped_tx(&p).await.expect("tx");
    let doc = brainiac_store::documents::get_document(&mut tx, doc_id)
        .await
        .expect("q")
        .expect("doc");
    let rev = brainiac_store::documents::revisions(&mut tx, doc_id, 1)
        .await
        .expect("revs")
        .into_iter()
        .next()
        .expect("a revision");
    tx.commit().await.expect("commit");

    assert!(
        doc.dirty_at.is_none(),
        "the page is still dirty after a compose pass — propagation is broken, \
         which is exactly what oldest_dirty_secs exists to make visible"
    );
    // The human's pinned prose reached the published markdown. A pinned edit
    // that never lands in a revision is invisible — its own kind of lie.
    assert!(
        rev.content_md.contains("#pay-oncall"),
        "the human's prose never reached the page:\n{}",
        rev.content_md
    );
    eprintln!("propagation latency (one page, mock composer): {elapsed:?}");

    // ── read analytics (0025): the dirty flag is a property of the MOMENT ──
    // A read of the freshly composed page records clean; the same page read
    // after its memories move on again records dirty. `was_dirty` is what lets
    // Knowledge Health rank rot that is being consumed above rot nobody opens.
    let r = http
        .get(format!("{base}/v1/docs/retry-policy"))
        .bearer_auth("tok_lead")
        .send()
        .await
        .expect("read clean page");
    assert_eq!(r.status(), reqwest::StatusCode::OK);

    let mut tx = store.scoped_tx(&p).await.expect("tx");
    brainiac_store::documents::mark_dirty(&mut tx, doc_id)
        .await
        .expect("mark dirty");
    tx.commit().await.expect("commit");
    let r = http
        .get(format!("{base}/v1/docs/retry-policy"))
        .bearer_auth("tok_lead")
        .send()
        .await
        .expect("read dirty page");
    assert_eq!(r.status(), reqwest::StatusCode::OK);

    let mut tx = store.scoped_tx(&p).await.expect("tx");
    let reads: Vec<(String, bool)> = sqlx::query_as(
        "SELECT via, was_dirty FROM document_reads WHERE document_id = $1 ORDER BY read_at",
    )
    .bind(doc_id)
    .fetch_all(&mut *tx)
    .await
    .expect("reads");
    tx.commit().await.expect("commit");
    assert_eq!(
        reads,
        vec![("http".to_string(), false), ("http".to_string(), true)],
        "clean read then dirty read, each recording the page's state at that moment"
    );
}
