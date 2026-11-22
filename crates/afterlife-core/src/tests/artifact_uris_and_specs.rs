use crate::*;

use super::{agent_actor, seed_job_in_district};

#[test]
fn find_stage_for_job_prefers_pinned_spec_version_and_falls_back_to_active() {
    let mut engine = AfterlifeEngine::new();
    let human = engine.local_actor();
    let agent = agent_actor(&engine.owner_id, "spec-runner");
    let dist_id = DistrictId::from("DIST-001");
    let v1_job_id = seed_job_in_district(&mut engine, "V1 pinned job");

    let mut v2_stages = engine
        .find_district(&dist_id)
        .unwrap()
        .active_spec()
        .stages
        .clone();
    let design = v2_stages
        .iter_mut()
        .find(|s| s.id == StageId::from("design"))
        .unwrap();
    design.label = "Design V2".into();

    let prop = engine
        .apply(
            &agent,
            EngineCommand::ProposeSpecUpdate {
                district_id: dist_id.clone(),
                base_version: SpecVersion::new(1),
                stages: v2_stages,
                changelog: "Rename design stage in v2".into(),
                rationale: "Test pinned spec resolution".into(),
                source_feedback_ids: Vec::new(),
                at: 2_100_000_000_000,
            },
        )
        .unwrap();
    let prop_id = match &prop[0] {
        JobEvent::SpecProposed { proposal_id, .. } => proposal_id.clone(),
        _ => unreachable!(),
    };
    engine
        .apply(
            &human,
            EngineCommand::ApproveSpecProposal {
                proposal_id: prop_id,
                at: 2_100_000_001_000,
            },
        )
        .unwrap();

    let v2_job_id = seed_job_in_district(&mut engine, "V2 pinned job");
    let unpinned_job_id = engine.queue_preset_job("Unpinned queued job", Some(&dist_id));

    let v1_job = engine.find_job(&v1_job_id).unwrap();
    let v2_job = engine.find_job(&v2_job_id).unwrap();
    let unpinned_job = engine.find_job(&unpinned_job_id).unwrap();

    assert_eq!(
        engine
            .find_stage_for_job(v1_job, &dist_id, &StageId::from("design"))
            .unwrap()
            .label,
        "Design Whole"
    );
    assert_eq!(
        engine
            .find_stage_for_job(v2_job, &dist_id, &StageId::from("design"))
            .unwrap()
            .label,
        "Design V2"
    );
    assert_eq!(
        engine
            .find_stage_for_job(unpinned_job, &dist_id, &StageId::from("design"))
            .unwrap()
            .label,
        "Design V2"
    );
}

#[test]
fn external_openable_artifact_uris_accepted_on_attach() {
    let mut engine = AfterlifeEngine::new();
    let agent = agent_actor(&engine.owner_id, "uri-guard-runner");
    let job_id = seed_job_in_district(&mut engine, "External URI guard");
    let claim = engine
        .apply(
            &agent,
            EngineCommand::ClaimJob {
                job_id: job_id.clone(),
                runner_id: RunnerId::from("uri-guard-runner"),
                runner_label: "URI Guard Runner".into(),
                ttl_ms: 60_000,
                expected_revision: None,
                at: 2_200_000_000_000,
            },
        )
        .unwrap();
    let tok = match &claim[0] {
        JobEvent::JobClaimed { fencing_token, .. } => *fencing_token,
        _ => unreachable!(),
    };

    for (idx, uri) in [
        "https://github.com/org/repo/pull/1",
        "http://127.0.0.1:4319/v1/health",
        "linear://team/issue/ENG-42",
        "slack://channel?team=T1&id=C1",
    ]
    .iter()
    .enumerate()
    {
        engine
            .apply(
                &agent,
                EngineCommand::AttachArtifact {
                    job_id: job_id.clone(),
                    kind: ArtifactKind::Issue,
                    label: format!("ext-{idx}"),
                    uri: (*uri).to_string(),
                    sha256: None,
                    fencing_token: Some(tok),
                    at: 2_200_000_000_100 + idx as u64,
                },
            )
            .unwrap();
    }

    for bad in ["javascript:alert(1)", "data:text/plain,hi", "vbscript:x"] {
        let err = engine
            .apply(
                &agent,
                EngineCommand::AttachArtifact {
                    job_id: job_id.clone(),
                    kind: ArtifactKind::Spec,
                    label: "bad".into(),
                    uri: bad.into(),
                    sha256: None,
                    fencing_token: Some(tok),
                    at: 2_200_000_000_200,
                },
            )
            .unwrap_err();
        assert!(matches!(err, TransitionError::InvalidInput { .. }));
    }
}

#[test]
fn attach_artifact_rejects_fictional_uris_and_mismatched_blob_sha256() {
    let mut engine = AfterlifeEngine::new();
    let agent = agent_actor(&engine.owner_id, "uri-strict-runner");
    let job_id = seed_job_in_district(&mut engine, "Strict URI contract");
    let claim = engine
        .apply(
            &agent,
            EngineCommand::ClaimJob {
                job_id: job_id.clone(),
                runner_id: RunnerId::from("uri-strict-runner"),
                runner_label: "Strict URI Runner".into(),
                ttl_ms: 60_000,
                expected_revision: None,
                at: 2_300_000_000_000,
            },
        )
        .unwrap();
    let tok = match &claim[0] {
        JobEvent::JobClaimed { fencing_token, .. } => *fencing_token,
        _ => unreachable!(),
    };

    for fictional in [
        "evidence://job-001/before-baseline.json",
        "file:///etc/passwd",
        "s3://bucket/key.json",
        "queue-drop.json",
        "blob://local-owner/short-sha/report.json",
        "blob://local-owner/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef/../evil.json",
    ] {
        let err = engine
            .apply(
                &agent,
                EngineCommand::AttachArtifact {
                    job_id: job_id.clone(),
                    kind: ArtifactKind::Snapshot,
                    label: "fictional".into(),
                    uri: fictional.into(),
                    sha256: None,
                    fencing_token: Some(tok),
                    at: 2_300_000_000_100,
                },
            )
            .unwrap_err();
        assert!(
            matches!(err, TransitionError::InvalidInput { .. }),
            "expected InvalidInput for `{fictional}`, got {err:?}"
        );
    }

    let sha_a = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let sha_b = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
    let blob_uri = format_blob_uri(engine.owner_id.as_str(), sha_a, "report.json");

    let mismatch_err = engine
        .apply(
            &agent,
            EngineCommand::AttachArtifact {
                job_id: job_id.clone(),
                kind: ArtifactKind::VerificationReport,
                label: "report.json".into(),
                uri: blob_uri.clone(),
                sha256: Some(sha_b.into()),
                fencing_token: Some(tok),
                at: 2_300_000_000_200,
            },
        )
        .unwrap_err();
    assert!(matches!(mismatch_err, TransitionError::InvalidInput { .. }));

    engine
        .apply(
            &agent,
            EngineCommand::AttachArtifact {
                job_id: job_id.clone(),
                kind: ArtifactKind::VerificationReport,
                label: "report.json".into(),
                uri: format!("  {blob_uri}  "),
                sha256: Some(sha_a.into()),
                fencing_token: Some(tok),
                at: 2_300_000_000_300,
            },
        )
        .unwrap();
    let saved = engine
        .find_job(&job_id)
        .unwrap()
        .artifacts
        .last()
        .unwrap();
    assert_eq!(saved.uri, blob_uri);
}
