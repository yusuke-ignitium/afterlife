use std::collections::HashMap;

use crate::{
    ids::{
        DistrictId, OwnerId, PhilosophyVersion, SpecVersion, StageId, SubMachineTemplateId,
        SubStageId,
    },
    model::{
        Actor, District, GateRubric, Job, Philosophy, Principle, ProcedureSpec, RunnerKind,
        StageDef, StageKind, SubMachineTemplate, SubStageDef,
    },
};

pub const LOCAL_OWNER_ID: &str = "local-owner";
const SEED_TS: u64 = 1_763_600_000_000;

pub struct WorkspaceSeed {
    pub templates: HashMap<SubMachineTemplateId, SubMachineTemplate>,
    pub template_order: Vec<SubMachineTemplateId>,
    pub districts: Vec<District>,
    pub jobs: Vec<Job>,
    pub next_job_seq: u32,
    pub next_fencing_token: u64,
}

pub fn default_workspace(owner_id: &OwnerId) -> WorkspaceSeed {
    let seed_actor = Actor::local_human(owner_id.clone());

    let mut templates = HashMap::new();

    let tmpl_evidence = SubMachineTemplate {
        id: SubMachineTemplateId::from("tmpl-evidence"),
        name: "Evidence & Spike".into(),
        description: "一次仕様・既存実装トレースから隔離環境での実機検証、スコープ確定まで"
            .into(),
        philosophy_notes:
            "推測で設計しない。一次ソースと隔離再現で事実を確定させてから着手する。".into(),
        stages: vec![
            SubStageDef {
                id: SubStageId::from("trace_sources"),
                label: "Trace Sources".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                procedure:
                    "既存実装・関連シンボル・公式ドキュメント・制約条件を一次情報から網羅的に確認する。"
                        .into(),
                gate_checks: vec![
                    "関連シンボルと影響範囲の一次確認が完了している".into(),
                    "外部仕様や依存制約に一次ソースの裏付けがある".into(),
                ],
            },
            SubStageDef {
                id: SubStageId::from("isolate_probe"),
                label: "Isolate Probe".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                procedure:
                    "未確定の挙動・互換性・性能仮説は一時環境や最小再現で実機検証し、推測を排除する。"
                        .into(),
                gate_checks: vec![
                    "未確定事項が実機検証または実測で確認済みである".into(),
                ],
            },
            SubStageDef {
                id: SubStageId::from("lock_scope"),
                label: "Lock Scope".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                procedure:
                    "変更対象・非対象・後方互換条件・守るべき不変条件を簡潔に確定する。"
                        .into(),
                gate_checks: vec![
                    "変更対象・非対象・守るべき不変条件が明記されている".into(),
                ],
            },
        ],
    };

    let tmpl_baseline_guard = SubMachineTemplate {
        id: SubMachineTemplateId::from("tmpl-baseline-guard"),
        name: "Baseline & Guard".into(),
        description: "メイン実装に着手する前のBeforeベースライン採取とデグレガード先行固定"
            .into(),
        philosophy_notes:
            "メイン実装より先にBeforeベースラインとデグレ検知ガードを固定し、後付けの甘い検証を構造的に防ぐ。"
                .into(),
        stages: vec![
            SubStageDef {
                id: SubStageId::from("capture_baseline"),
                label: "Capture Baseline".into(),
                runner: "Verifier".into(),
                runner_kind: RunnerKind::Deterministic,
                procedure:
                    "変更前の現行状態について、再現手順・品質指標・性能や出力のBeforeベースラインを同一条件で再測定可能な形で記録する。"
                        .into(),
                gate_checks: vec![
                    "変更前（Before）の比較基準と再現条件が記録されている".into(),
                ],
            },
            SubStageDef {
                id: SubStageId::from("lock_regression_guards"),
                label: "Lock Guards".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                procedure:
                    "メインスコープ実装前に、守るべき既存品質・不変条件・境界条件の決定論的デグレガード（自動テスト／検証器）を先行実装し、現行状態で通過することを確認する。"
                        .into(),
                gate_checks: vec![
                    "メイン実装より先にデグレガードが実装され現行状態で通過している".into(),
                    "意図的な退行や境界違反を検知できる検証内容になっている".into(),
                ],
            },
        ],
    };

    let tmpl_build_loop = SubMachineTemplate {
        id: SubMachineTemplateId::from("tmpl-build-loop"),
        name: "Build & Hygiene".into(),
        description: "先行ガード下での土台整合実装、新機能テスト追加、警告ゼロ検証ループ".into(),
        philosophy_notes:
            "先行固定したデグレガードを破らずに土台から整え、警告ゼロ・全緑になるまで妥協しない。"
                .into(),
        stages: vec![
            SubStageDef {
                id: SubStageId::from("implement_core"),
                label: "Implement Core".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                procedure:
                    "設計に従い、既存アーキテクチャ・トークン・永続化互換と整合する形でメインスコープを本質的に実装する。"
                        .into(),
                gate_checks: vec![
                    "パッチワークや場当たり上書きがなく全体と整合している".into(),
                    "ローカル固有パスや機密情報が混入していない".into(),
                ],
            },
            SubStageDef {
                id: SubStageId::from("add_feature_tests"),
                label: "Feature Tests".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                procedure:
                    "追加・変更した仕様の正常系・異常系・境界条件を検証する決定論的テストを整備する。"
                        .into(),
                gate_checks: vec![
                    "新仕様と異常系を網羅する自動テストが追加または更新されている".into(),
                ],
            },
            SubStageDef {
                id: SubStageId::from("hygiene_gate"),
                label: "Hygiene Gate".into(),
                runner: "Build & Test".into(),
                runner_kind: RunnerKind::Deterministic,
                procedure:
                    "先行デグレガードを含む全テスト・ビルド・lint・規約チェックを実行し、警告ゼロ・全緑になるまで修正ループを回す。"
                        .into(),
                gate_checks: vec![
                    "先行デグレガードを含む全テスト・ビルド・lint が警告ゼロで通過している"
                        .into(),
                ],
            },
        ],
    };

    let tmpl_verify_ship = SubMachineTemplate {
        id: SubMachineTemplateId::from("tmpl-verify-ship"),
        name: "Verify & Diff".into(),
        description: "実機・本番相当ビルド検証、Before/After前後比較、証跡とWhyの整理".into(),
        philosophy_notes:
            "動いたつもりで出荷しない。Beforeベースラインとの同一条件比較と実機確認で退行ゼロを実証し、人間レビューに必要な証跡だけを渡す。"
                .into(),
        stages: vec![
            SubStageDef {
                id: SubStageId::from("live_verify"),
                label: "Live Run".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                procedure:
                    "実機・本番相当ビルド・E2E パスで実際のユーザー操作と永続化／再起動挙動を検証する。"
                        .into(),
                gate_checks: vec![
                    "実機または本番相当環境での動作確認が完了している".into(),
                ],
            },
            SubStageDef {
                id: SubStageId::from("compare_baseline"),
                label: "Before/After Diff".into(),
                runner: "Verifier".into(),
                runner_kind: RunnerKind::Deterministic,
                procedure:
                    "Baseline & Guard で記録したBefore基準と変更後（After）を同一条件で比較し、品質・性能・互換性の退行ゼロを実証する。"
                        .into(),
                gate_checks: vec![
                    "Before/After の同一条件比較で品質・性能・互換性の退行ゼロが確認済みである"
                        .into(),
                ],
            },
            SubStageDef {
                id: SubStageId::from("bundle_evidence"),
                label: "Bundle Evidence".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                procedure:
                    "前後比較結果・証跡 Artifact・変更の Why（設計判断・互換性・不変条件）を整理し人間ゲートへ引き渡す。"
                        .into(),
                gate_checks: vec![
                    "人間承認に必要な前後比較証跡と Why 要約が揃っている".into(),
                ],
            },
        ],
    };

    let template_order = vec![
        tmpl_evidence.id.clone(),
        tmpl_baseline_guard.id.clone(),
        tmpl_build_loop.id.clone(),
        tmpl_verify_ship.id.clone(),
    ];
    templates.insert(tmpl_evidence.id.clone(), tmpl_evidence);
    templates.insert(tmpl_baseline_guard.id.clone(), tmpl_baseline_guard);
    templates.insert(tmpl_build_loop.id.clone(), tmpl_build_loop);
    templates.insert(tmpl_verify_ship.id.clone(), tmpl_verify_ship);

    let delivery_philosophy = Philosophy {
        version: PhilosophyVersion::new(1),
        summary:
            "一次情報と実機検証を起点に、実装前ベースラインとデグレガード先行固定でパッチワークを排し、人間承認境界で常に商用完成度を保つ汎用プロダクト開発ディストリクト。"
                .into(),
        principles: vec![
            Principle {
                id: "P-DEV-1".into(),
                title: "Ship Every State as a Whole".into(),
                statement:
                    "応急処置や場当たりハックを積まず、要件変更時は土台から整合させて常に商用完成品として保つ。"
                        .into(),
                rationale:
                    "局所パッチの積み上げは設計破綻と保守不能な分岐を生み、品質と開発速度を同時に落とすため。"
                        .into(),
            },
            Principle {
                id: "P-DEV-2".into(),
                title: "Primary Evidence & Live Probe First".into(),
                statement:
                    "推測で進めない。公式仕様・既存実装の一次確認と、隔離環境での実機検証・定量計測を先行させる。"
                        .into(),
                rationale:
                    "誤った前提に基づく設計や勘による最適化を防ぎ、手戻りと隠れた退行を根絶するため。"
                        .into(),
            },
            Principle {
                id: "P-DEV-3".into(),
                title: "Baseline & Regression Guards Before Implementation".into(),
                statement:
                    "メインスコープの実装に着手する前に、Beforeベースラインの記録と決定論的デグレガードの固定を完了させる。"
                        .into(),
                rationale:
                    "実装後の後付けテストは現行挙動への追従になりやすく、前後比較と退行検知の客観性を損なうため。"
                        .into(),
            },
            Principle {
                id: "P-DEV-4".into(),
                title: "Minimal Invariants, Single Source of Truth".into(),
                statement:
                    "冗長な手順書や二重管理を排し、単一ソースに最小の不変原則だけを残して仕様・実装・契約のドリフトを防ぐ。"
                        .into(),
                rationale:
                    "肥大化したドキュメントや重複設定は即座に陳腐化し、エージェントと人間の判断を狂わせるため。"
                        .into(),
            },
            Principle {
                id: "P-DEV-5".into(),
                title: "Fail-Closed Governance & Resilient State".into(),
                statement:
                    "実行は AI に積極委譲しつつ最終承認・破壊的判断は人間ゲートで止め、永続化はアトミック書き込み・サニタイズ・後方互換を徹底する。"
                        .into(),
                rationale:
                    "自律実行の速度を最大化しながら、データ破損・権限逸脱・ローカル固有情報や機密の混入を構造的に防ぐため。"
                        .into(),
            },
        ],
        anti_patterns: vec![
            "症状だけを隠すパッチワークやアドホックな上書きを残す".into(),
            "一次ソースや実機・実測の裏付けなしに設計や最適化を決める".into(),
            "Beforeベースラインやデグレガードを固定しないままメイン実装へ着手する".into(),
            "ビルド・テスト・lint・前後比較を通さず次ステージや承認へ回す".into(),
            "人間承認ゲートをエージェント自身の判断で迂回する".into(),
            "試行錯誤のログ・ローカル固有パス・秘密情報を成果物やコミットに残す".into(),
        ],
        decision_heuristics: vec![
            "仕様や挙動が曖昧なら推測で進めず、隔離検証（一時ディレクトリ・最小再現）で事実を確定させてから進む"
                .into(),
            "メイン実装より先にBeforeベースラインと決定論的デグレガードを固定し、完了後に同一条件で前後比較して退行ゼロを実証する"
                .into(),
            "要件が揺れたら表面を繕わず、共通の土台・トークン・抽象境界を一段整理してから実装する"
                .into(),
            "変更は論理単位で刻み、コミットには diff から読めない Why（設計判断・互換性・不変条件）を残す"
                .into(),
        ],
        source_feedback_ids: vec![],
        updated_by: seed_actor.clone(),
        updated_at: SEED_TS,
    };

    let delivery_spec = ProcedureSpec {
        version: SpecVersion::new(1),
        changelog: "Initial general-purpose product delivery procedure spec".into(),
        source_feedback_ids: vec![],
        updated_by: seed_actor,
        updated_at: SEED_TS,
        stages: vec![
            StageDef {
                id: StageId::from("discover"),
                label: "Discover & Probe".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure:
                    "既存実装・一次仕様・影響範囲をトレースし、未確定事項は隔離環境で実機検証してからスコープと守るべき不変条件を確定する。"
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "関連シンボル・影響範囲と一次仕様が確認済みである".into(),
                        "変更対象・非対象・守るべき不変条件が明確である".into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: Some(SubMachineTemplateId::from("tmpl-evidence")),
                next: vec![StageId::from("design")],
                rework_to: None,
            },
            StageDef {
                id: StageId::from("design"),
                label: "Design Whole".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure:
                    "場当たりハックを避け、プロダクト全体の設計・型・境界条件・後方互換・UX 整合にフィットするアプローチと、前後比較・デグレガード計画を定める。"
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "パッチワークではなく全体整合した設計になっている".into(),
                        "永続化・後方互換・異常系・境界条件の方針がある".into(),
                        "Beforeベースライン計測と先行デグレガードの計画が具体化されている"
                            .into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: None,
                next: vec![StageId::from("baseline_guard")],
                rework_to: Some(StageId::from("discover")),
            },
            StageDef {
                id: StageId::from("baseline_guard"),
                label: "Baseline & Guard".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure:
                    "メインスコープ実装前に、変更前（Before）の比較ベースラインを記録し、既存品質・不変条件を守る決定論的デグレガードを先行実装・固定する。"
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "変更前（Before）の比較ベースラインと再現条件が記録されている".into(),
                        "メイン実装に先立つデグレガードが実装され現行状態で通過している".into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: Some(SubMachineTemplateId::from("tmpl-baseline-guard")),
                next: vec![StageId::from("build")],
                rework_to: Some(StageId::from("design")),
            },
            StageDef {
                id: StageId::from("build"),
                label: "Build Core".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure:
                    "先行固定したデグレガードを破らずにメインスコープを土台整合で実装し、新仕様テストを追加。ビルド・全テスト・lint 警告ゼロと移植性を満たす。"
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "先行デグレガードと新仕様テストがすべて通過している".into(),
                        "ビルド・全テスト・lint が警告ゼロで通過している".into(),
                        "ローカル固有情報・機密・不要な差分が混入していない".into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: Some(SubMachineTemplateId::from("tmpl-build-loop")),
                next: vec![StageId::from("verify")],
                rework_to: Some(StageId::from("design")),
            },
            StageDef {
                id: StageId::from("verify"),
                label: "Verify & Diff".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure:
                    "実機・本番相当ビルド・E2E で動作確認し、Baseline & Guard で記録した Before 基準と同一条件で前後比較して退行ゼロを実証する。証跡 Artifact と Why 要約を揃える。"
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "実機または本番相当環境での動作確認証跡がある".into(),
                        "Before/After の同一条件比較で品質・性能・互換性の退行ゼロが確認済みである"
                            .into(),
                        "証跡 Artifact と Why 要約が揃っている".into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: Some(SubMachineTemplateId::from("tmpl-verify-ship")),
                next: vec![StageId::from("approve")],
                rework_to: Some(StageId::from("build")),
            },
            StageDef {
                id: StageId::from("approve"),
                label: "Human Gate".into(),
                runner: "You".into(),
                runner_kind: RunnerKind::Human,
                kind: StageKind::ApprovalGate,
                procedure:
                    "前後比較結果・証跡 Artifact・変更の妥当性を人間オーナーが最終確認し、Approve または理由付き Rework を行う。"
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "District Philosophy の原則と商用完成度を満たしている".into(),
                        "リリース・公開・破壊的変更のリスクが許容範囲内である".into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: true,
                }),
                sub_template_id: None,
                next: vec![StageId::from("shipped")],
                rework_to: Some(StageId::from("build")),
            },
            StageDef {
                id: StageId::from("shipped"),
                label: "Shipped".into(),
                runner: "Done".into(),
                runner_kind: RunnerKind::Deterministic,
                kind: StageKind::Terminal,
                procedure:
                    "人間承認・Why を記したクリーンなコミット・出荷完了。学びは Feedback として次版へ還元可能な状態。"
                        .into(),
                gate_rubric: None,
                sub_template_id: None,
                next: vec![],
                rework_to: None,
            },
        ],
    };

    let districts = vec![District {
        id: DistrictId::from("DIST-001"),
        owner_id: owner_id.clone(),
        name: "DISTRICT 01 - Product Delivery".into(),
        setup_by: "General Delivery Blueprint".into(),
        active_philosophy_version: PhilosophyVersion::new(1),
        philosophy_history: vec![delivery_philosophy],
        active_spec_version: SpecVersion::new(1),
        spec_history: vec![delivery_spec],
    }];

    WorkspaceSeed {
        templates,
        template_order,
        districts,
        jobs: Vec::new(),
        next_job_seq: 1,
        next_fencing_token: 1,
    }
}
