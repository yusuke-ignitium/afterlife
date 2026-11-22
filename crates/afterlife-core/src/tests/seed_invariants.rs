use crate::*;

#[test]
fn default_workspace_passes_graph_validation() {
    let engine = AfterlifeEngine::new();
    assert!(engine.validate().is_ok());
    assert!(engine.jobs.is_empty());
    assert!(engine.queued_jobs().is_empty());
    assert!(engine.approval_jobs().is_empty());
    assert_eq!(engine.districts.len(), 5);
    assert_eq!(engine.template_order.len(), 7);
    let dist2 = engine
        .find_district(&DistrictId::from("DIST-002"))
        .expect("DIST-002 must exist");
    let dist2_stage_ids: Vec<&str> = dist2.stages().iter().map(|s| s.id.as_str()).collect();
    assert_eq!(
        dist2_stage_ids,
        vec!["recon", "options", "shape", "approve", "shaped"]
    );
    let shape_stage = dist2
        .stages()
        .iter()
        .find(|s| s.id.as_str() == "shape")
        .unwrap();
    assert_eq!(shape_stage.required_artifact_kinds, vec![ArtifactKind::Spec]);
    for district in &engine.districts {
        assert_eq!(
            district.active_philosophy().version,
            PhilosophyVersion::new(1)
        );
        let stages = district.stages();
        assert!(
            stages.iter().any(|s| s.is_gate()),
            "district {} must include a human approval gate",
            district.id
        );
        assert!(
            stages.iter().any(|s| s.is_terminal()),
            "district {} must include a terminal stage",
            district.id
        );
    }
    let serialized = format!("{:?}{:?}", engine.districts, engine.templates);
    for forbidden in [
        "gpu-perf-optimize",
        "AGENTS.md",
        ".cursor",
        ".agents",
        "Antigravity",
        "Cursor",
        "cargo",
        "Metal",
        "1 分で",
        "1分で",
    ] {
        assert!(
            !serialized.contains(forbidden),
            "generic workspace seed must not contain environment-specific or time-boxed token `{forbidden}`"
        );
    }
}

fn contains_cjk_or_fullwidth(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(
            c,
            '\u{3000}'..='\u{303F}'
                | '\u{3040}'..='\u{30FF}'
                | '\u{3400}'..='\u{4DBF}'
                | '\u{4E00}'..='\u{9FFF}'
                | '\u{FF00}'..='\u{FFEF}'
        )
    })
}

#[test]
fn builtin_workspace_defaults_and_step_fields_are_english_only() {
    assert!(contains_cjk_or_fullwidth("日本語"));
    assert!(!contains_cjk_or_fullwidth("English-only default text (v1)"));

    let engine = AfterlifeEngine::new();
    let districts_debug = format!("{:?}", engine.districts);
    let templates_debug = format!("{:?}", engine.templates);
    assert!(
        !contains_cjk_or_fullwidth(&districts_debug),
        "default districts must not contain Japanese or full-width characters"
    );
    assert!(
        !contains_cjk_or_fullwidth(&templates_debug),
        "default sub-machine templates must not contain Japanese or full-width characters"
    );

    let defaults = default_step_fields();
    assert_eq!(defaults.len(), 2);
    for field in &defaults {
        assert!(!field.key.trim().is_empty());
        assert!(!field.label.trim().is_empty());
        assert!(!field.prompt.trim().is_empty());
        assert!(
            !contains_cjk_or_fullwidth(&field.label)
                && !contains_cjk_or_fullwidth(&field.prompt),
            "default_step_fields entry `{}` must be English-only",
            field.key
        );
        assert_eq!(
            builtin_step_field_label(&field.key),
            Some(field.label.as_str())
        );
    }

    for key in ["decision", "execution", "note", "reason"] {
        let label = builtin_step_field_label(key)
            .unwrap_or_else(|| panic!("missing builtin label for `{key}`"));
        assert!(!label.trim().is_empty());
        assert!(
            !contains_cjk_or_fullwidth(label),
            "builtin fallback label for `{key}` must be English-only"
        );
    }
    assert_eq!(builtin_step_field_label("custom_user_key"), None);
}

#[test]
fn cannot_attach_sub_template_to_gate_or_terminal() {
    let mut engine = AfterlifeEngine::new();
    let actor = engine.local_actor();
    let err = engine
        .apply(
            &actor,
            EngineCommand::SetStageSubTemplate {
                district_id: DistrictId::from("DIST-001"),
                stage_id: StageId::from("approve"),
                template_id: Some(SubMachineTemplateId::from("tmpl-build-loop")),
                at: 1000,
            },
        )
        .unwrap_err();

    assert!(matches!(err, TransitionError::SubTemplateNotAllowed { .. }));
}
