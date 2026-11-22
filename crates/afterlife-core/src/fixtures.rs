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
        name: "Evidence & Probe".into(),
        description: "一次仕様・既存実装トレースから隔離環境での実機検証、判断基準と前提の確定まで"
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
                id: SubStageId::from("lock_criteria"),
                label: "Lock Criteria".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                procedure:
                    "対象範囲・非対象・前提条件・評価基準・守るべき不変条件を簡潔に確定する。"
                        .into(),
                gate_checks: vec![
                    "対象範囲・前提条件・評価基準・守るべき不変条件が明記されている".into(),
                ],
            },
        ],
    };

    let tmpl_observe = SubMachineTemplate {
        id: SubMachineTemplateId::from("tmpl-observe"),
        name: "Observe & Instrument".into(),
        description: "対象経路の棚卸しから未観測区間の計測計装、再現可能な評価指標仕様の確定まで"
            .into(),
        philosophy_notes:
            "測れない区間は推測せず自律的に計測可能にしてからベースライン採取へ進む。".into(),
        stages: vec![
            SubStageDef {
                id: SubStageId::from("map_pipeline"),
                label: "Map Pipeline".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                procedure:
                    "対象ワークロードの実行経路・状態遷移・リソース消費ポイント（アクティブ時およびアイドル時）を一次実装から洗い出す。"
                        .into(),
                gate_checks: vec![
                    "対象経路と未観測区間が一次実装から特定されている".into(),
                ],
            },
            SubStageDef {
                id: SubStageId::from("add_instrumentation"),
                label: "Add Instrumentation".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                procedure:
                    "未観測のボトルネック区間があれば、本番挙動や品質を劣化させない再現可能な計測計装・計測ハーネスを整備する。"
                        .into(),
                gate_checks: vec![
                    "未観測区間が同一条件で繰り返し計測できる状態になっている".into(),
                ],
            },
            SubStageDef {
                id: SubStageId::from("lock_metric_spec"),
                label: "Lock Metric Spec".into(),
                runner: "Verifier".into(),
                runner_kind: RunnerKind::Deterministic,
                procedure:
                    "評価指標・計測ワークロード・環境条件・退行を許さない品質ラインを明文化して確定する。"
                        .into(),
                gate_checks: vec![
                    "評価指標・計測条件・品質退行禁止ラインが明記されている".into(),
                ],
            },
        ],
    };

    let tmpl_baseline_guard = SubMachineTemplate {
        id: SubMachineTemplateId::from("tmpl-baseline-guard"),
        name: "Baseline & Guard".into(),
        description: "メイン変更に着手する前のBeforeベースライン採取とデグレガード先行固定"
            .into(),
        philosophy_notes:
            "メイン変更より先にBeforeベースラインとデグレ検知ガードを固定し、後付けの甘い検証を構造的に防ぐ。"
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
                    "メインスコープ変更前に、守るべき既存品質・不変条件・境界条件の決定論的デグレガード（自動テスト／検証器）を先行実装し、現行状態で通過することを確認する。"
                        .into(),
                gate_checks: vec![
                    "メイン変更より先にデグレガードが実装され現行状態で通過している".into(),
                    "意図的な退行や境界違反を検知できる検証内容になっている".into(),
                ],
            },
        ],
    };

    let tmpl_red_repro = SubMachineTemplate {
        id: SubMachineTemplateId::from("tmpl-red-repro"),
        name: "Red Repro & Guard".into(),
        description: "修正前に障害を確実に再現するRedテスト固定と周辺退行ガード確認".into(),
        philosophy_notes:
            "再現できないバグは直せない。修正コードに触れる前に失敗する決定論的テストと周辺ガードを固定する。"
                .into(),
        stages: vec![
            SubStageDef {
                id: SubStageId::from("capture_failure"),
                label: "Capture Failure".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                procedure:
                    "障害の発生条件・入力・環境前提・観測される異常挙動（Before）を最小条件で再現し記録する。"
                        .into(),
                gate_checks: vec![
                    "障害の最小再現条件と現行の失敗挙動が記録されている".into(),
                ],
            },
            SubStageDef {
                id: SubStageId::from("lock_red_test"),
                label: "Lock Red Test".into(),
                runner: "Verifier".into(),
                runner_kind: RunnerKind::Deterministic,
                procedure:
                    "修正前に必ず失敗（Red）する決定論的な自動再現テストを先行実装し、期待通りの理由で失敗することを確認する。"
                        .into(),
                gate_checks: vec![
                    "修正前の現行コードで再現テストが意図通り失敗（Red）している".into(),
                ],
            },
            SubStageDef {
                id: SubStageId::from("lock_boundary_guards"),
                label: "Boundary Guards".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                procedure:
                    "修正によって壊れやすい周辺の正常系・互換性境界のガードを確認し、不足があれば先行追加して現行通過を固定する。"
                        .into(),
                gate_checks: vec![
                    "周辺正常系・互換性の既存ガードが現行状態で通過している".into(),
                ],
            },
        ],
    };

    let tmpl_build_loop = SubMachineTemplate {
        id: SubMachineTemplateId::from("tmpl-build-loop"),
        name: "Build & Hygiene".into(),
        description: "先行ガード下での土台整合変更、境界・異常系テスト補強、警告ゼロ検証ループ"
            .into(),
        philosophy_notes:
            "先行固定したガードを破らずに土台から整え、警告ゼロ・全緑になるまで妥協しない。"
                .into(),
        stages: vec![
            SubStageDef {
                id: SubStageId::from("apply_change"),
                label: "Apply Change".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                procedure:
                    "設計・診断・削減計画に従い、既存アーキテクチャ・トークン・永続化互換と整合する形でメインスコープを本質的に変更する。"
                        .into(),
                gate_checks: vec![
                    "パッチワークや場当たり上書きがなく全体と整合している".into(),
                    "ローカル固有パスや機密情報が混入していない".into(),
                ],
            },
            SubStageDef {
                id: SubStageId::from("edge_guard_tests"),
                label: "Edge & Guard Tests".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                procedure:
                    "変更範囲の正常系・異常系・境界条件を検証するテストを補強し、先行固定した再現テストや退行ガードが改変なしで成立することを確認する。"
                        .into(),
                gate_checks: vec![
                    "変更範囲の正常系・異常系・境界条件を検証する自動テストが揃っている".into(),
                    "先行固定した再現テスト・退行ガードが改変なしで通過している".into(),
                ],
            },
            SubStageDef {
                id: SubStageId::from("hygiene_gate"),
                label: "Hygiene Gate".into(),
                runner: "Build & Test".into(),
                runner_kind: RunnerKind::Deterministic,
                procedure:
                    "先行ガードを含む全テスト・ビルド・lint・規約チェックを実行し、警告ゼロ・全緑になるまで修正ループを回す。"
                        .into(),
                gate_checks: vec![
                    "先行ガードを含む全テスト・ビルド・lint が警告ゼロで通過している".into(),
                ],
            },
        ],
    };

    let tmpl_perf_loop = SubMachineTemplate {
        id: SubMachineTemplateId::from("tmpl-perf-loop"),
        name: "Measure & Optimize".into(),
        description: "実測プロファイル駆動のボトルネック特定、原則的改善、同一条件再計測ループ"
            .into(),
        philosophy_notes:
            "推測チューニングや品質を削る近道を禁じ、支配的ボトルネックへの構造改善と同一条件再計測を反復する。"
                .into(),
        stages: vec![
            SubStageDef {
                id: SubStageId::from("profile_bottleneck"),
                label: "Profile Hotspots".into(),
                runner: "Verifier".into(),
                runner_kind: RunnerKind::Deterministic,
                procedure:
                    "実測プロファイルと計装データから支配的ボトルネックを特定し、改善対象と期待効果の仮説を明文化する。"
                        .into(),
                gate_checks: vec![
                    "支配的ボトルネックが実測データで特定されている".into(),
                ],
            },
            SubStageDef {
                id: SubStageId::from("apply_principled_change"),
                label: "Optimize Core".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                procedure:
                    "精度・品質・UX・安全性を一切劣化させない構造的・アルゴリズム的改善を実装する。"
                        .into(),
                gate_checks: vec![
                    "品質・機能・正確性を削るアドホックな近道がない".into(),
                    "ローカル固有パスや計測用ゴミコードが混入していない".into(),
                ],
            },
            SubStageDef {
                id: SubStageId::from("remeasure_and_guard"),
                label: "Remeasure & Guard".into(),
                runner: "Build & Test".into(),
                runner_kind: RunnerKind::Deterministic,
                procedure:
                    "先行固定した品質ガード・全テスト・lint を警告ゼロで通過させたうえで、Before と同一条件で再計測し改善幅を確認する。"
                        .into(),
                gate_checks: vec![
                    "先行品質ガード・全テスト・ビルド・lint が警告ゼロで通過している".into(),
                    "Before と同一条件での再計測結果が記録されている".into(),
                ],
            },
        ],
    };

    let tmpl_verify_ship = SubMachineTemplate {
        id: SubMachineTemplateId::from("tmpl-verify-ship"),
        name: "Verify & Diff".into(),
        description: "実機・本番相当環境検証、Before/After前後比較、証跡とWhyの整理".into(),
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
                    "先行ステージで記録したBefore基準と変更後（After）を同一条件で比較し、品質・性能・互換性の退行ゼロを実証する。"
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
        tmpl_observe.id.clone(),
        tmpl_baseline_guard.id.clone(),
        tmpl_red_repro.id.clone(),
        tmpl_build_loop.id.clone(),
        tmpl_perf_loop.id.clone(),
        tmpl_verify_ship.id.clone(),
    ];
    templates.insert(tmpl_evidence.id.clone(), tmpl_evidence);
    templates.insert(tmpl_observe.id.clone(), tmpl_observe);
    templates.insert(tmpl_baseline_guard.id.clone(), tmpl_baseline_guard);
    templates.insert(tmpl_red_repro.id.clone(), tmpl_red_repro);
    templates.insert(tmpl_build_loop.id.clone(), tmpl_build_loop);
    templates.insert(tmpl_perf_loop.id.clone(), tmpl_perf_loop);
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
        updated_by: seed_actor.clone(),
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

    let spike_philosophy = Philosophy {
        version: PhilosophyVersion::new(1),
        summary:
            "一次情報と隔離環境での実機検証のみを根拠に、技術選定・互換性検証・PoC の意思決定コストを最小化する調査ディストリクト。"
                .into(),
        principles: vec![
            Principle {
                id: "P-SPK-1".into(),
                title: "Primary Sources Only".into(),
                statement:
                    "推測や一般論で比較表を埋めず、公式仕様・一次実装・実測値のみを根拠にする。取得不能な情報はその場で明示する。"
                        .into(),
                rationale:
                    "誤った前提や伝聞に基づく技術選定は実装フェーズでの大規模な手戻りを招くため。"
                        .into(),
            },
            Principle {
                id: "P-SPK-2".into(),
                title: "Empirical Isolation Before Opinion".into(),
                statement:
                    "挙動・互換性・制約が未確定な論点は、本番環境を汚さない隔離環境で実機検証して事実を確定させてから結論を出す。"
                        .into(),
                rationale:
                    "ドキュメントの行間や未保証仕様は机上議論では解決できず、最小再現の実行結果だけが確かな判断材料になるため。"
                        .into(),
            },
            Principle {
                id: "P-SPK-3".into(),
                title: "Decision-Ready Tradeoffs".into(),
                statement:
                    "選択肢を 2〜3 案に絞り、評価軸・推奨案・棄却理由・撤退条件を明示して次の実装へ即着手できるブリーフにする。"
                        .into(),
                rationale:
                    "事実の羅列だけでは意思決定が止まり、判断責任の所在とリスク境界が曖昧になるため。"
                        .into(),
            },
            Principle {
                id: "P-SPK-4".into(),
                title: "Clean Teardown & Zero Residue".into(),
                statement:
                    "調査用の一時ディレクトリ・実験設定・検証スクリプトは検証完了後に確実に撤去し、本番構成に実験の残骸を残さない。"
                        .into(),
                rationale:
                    "調査の副産物が残ると後続開発の前提を汚染し、再現性や移植性を損なうため。"
                        .into(),
            },
        ],
        anti_patterns: vec![
            "一次ソースや実測の裏付けなしに「動くはず」と推測で結論づける".into(),
            "本番ワークスペースやユーザー環境を汚しながら検証する".into(),
            "推奨案も撤退条件もない長文レポートを提出する".into(),
            "未検証の前提や検証失敗の事実を伏せて都合のよい案だけ並べる".into(),
        ],
        decision_heuristics: vec![
            "公式仕様と実機挙動が食い違う可能性がある論点は必ず隔離環境でカナリア検証を回して白黒をつける"
                .into(),
            "将来の保守コストとドリフトリスクを初期実装の手軽さより重く評価する".into(),
            "未解決のリスクや外部依存の未保証領域は隠さず前提条件として切り出す".into(),
        ],
        source_feedback_ids: vec![],
        updated_by: seed_actor.clone(),
        updated_at: SEED_TS,
    };

    let spike_spec = ProcedureSpec {
        version: SpecVersion::new(1),
        changelog: "Initial general-purpose spike and decision procedure spec".into(),
        source_feedback_ids: vec![],
        updated_by: seed_actor.clone(),
        updated_at: SEED_TS,
        stages: vec![
            StageDef {
                id: StageId::from("investigate"),
                label: "Investigate & Probe".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure:
                    "公式ドキュメント・一次実装を調査し、未確定の挙動や互換性は隔離環境で実機検証して一次事実を収集する。検証後は一時環境を撤去する。"
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "すべての主張に一次ソースまたは隔離実機検証の根拠がある".into(),
                        "検証用の一時ファイルや実験環境が撤去され本番環境が汚れていない".into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: Some(SubMachineTemplateId::from("tmpl-evidence")),
                next: vec![StageId::from("brief")],
                rework_to: None,
            },
            StageDef {
                id: StageId::from("brief"),
                label: "Decision Brief".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure:
                    "収集した一次事実と実測結果を 2〜3 案に構造化し、トレードオフ・推奨案・棄却理由・撤退条件をまとめた意思決定ブリーフを Artifact として添付する。"
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "候補が 2〜3 案に整理され推奨案と棄却理由が明記されている".into(),
                        "リスク・制約・撤退条件および次フェーズの着手条件が明確である".into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: None,
                next: vec![StageId::from("approve")],
                rework_to: Some(StageId::from("investigate")),
            },
            StageDef {
                id: StageId::from("approve"),
                label: "Human Gate".into(),
                runner: "You".into(),
                runner_kind: RunnerKind::Human,
                kind: StageKind::ApprovalGate,
                procedure:
                    "一次根拠の十分性と推奨案の妥当性を人間オーナーが最終確認し、方針を確定または追加検証へ差し戻す。"
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "一次情報と実機検証に基づく納得感ある根拠が揃っている".into(),
                        "選定方針で次フェーズへ進める明確さがある".into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: true,
                }),
                sub_template_id: None,
                next: vec![StageId::from("decided")],
                rework_to: Some(StageId::from("brief")),
            },
            StageDef {
                id: StageId::from("decided"),
                label: "Decided".into(),
                runner: "Done".into(),
                runner_kind: RunnerKind::Deterministic,
                kind: StageKind::Terminal,
                procedure:
                    "意思決定確定済み。根拠ブリーフと制約条件を後続の実装ディストリクトへ引き渡し可能な状態。"
                        .into(),
                gate_rubric: None,
                sub_template_id: None,
                next: vec![],
                rework_to: None,
            },
        ],
    };

    let root_cause_philosophy = Philosophy {
        version: PhilosophyVersion::new(1),
        summary:
            "修正前に失敗する再現テスト（Red）を先行固定し、対症療法を排して根本原因の解決と再発防止を実証する障害修正ディストリクト。"
                .into(),
        principles: vec![
            Principle {
                id: "P-FIX-1".into(),
                title: "Failing Reproduction First (Red Before Green)".into(),
                statement:
                    "修正コードに触れる前に、障害を再現して必ず失敗する決定論的テストと発生条件（Before）を先行固定する。"
                        .into(),
                rationale:
                    "再現テストなしの修正は原因を取り違えても気づけず、再発防止の保証にもならないため。"
                        .into(),
            },
            Principle {
                id: "P-FIX-2".into(),
                title: "Fix the Root Cause, Never Mask Symptoms".into(),
                statement:
                    "局所的な条件分岐や場当たりガードで表面の症状だけ隠さず、破綻した不変条件・状態遷移・データ境界の根本から直す。"
                        .into(),
                rationale:
                    "対症療法は別の入力経路で同じ障害を再発させ、コードベースに説明不能な例外分岐を蓄積させるため。"
                        .into(),
            },
            Principle {
                id: "P-FIX-3".into(),
                title: "Zero Collateral Regression".into(),
                statement:
                    "修正に伴う副作用を許さない。新規再現テストの Green 化と同時に、周辺の既存挙動ガードと全テスト・lint が警告ゼロで通過することを必須にする。"
                        .into(),
                rationale:
                    "一つの障害修正が別の境界や互換性を破壊すれば、システム全体の信頼性が低下するため。"
                        .into(),
            },
            Principle {
                id: "P-FIX-4".into(),
                title: "Minimal Blast Radius & Durable Why".into(),
                statement:
                    "変更範囲を根本解決に必要な最小限に保ち、障害の発生メカニズム・再発防止策・修正理由（Why）を証跡とコミットに残す。"
                        .into(),
                rationale:
                    "関係のない変更の混入を防ぎ、将来の変更者が同じ落とし穴を踏まないようにするため。"
                        .into(),
            },
        ],
        anti_patterns: vec![
            "失敗再現テスト（Red）を固定しないまま勘で修正を始める".into(),
            "例外の握りつぶしや局所的な if 分岐で症状だけ隠す".into(),
            "再現テストを修正後の挙動に合わせて緩める".into(),
            "周辺退行やビルド・lint 警告を確認せず承認へ回す".into(),
        ],
        decision_heuristics: vec![
            "再現テストが修正前に意図通りの失敗理由で落ちることを確認するまで実装フェーズへ進まない"
                .into(),
            "同じ原因が他の呼び出し経路や永続化層にも潜んでいないか必ず横展開で点検する"
                .into(),
            "修正後は Before（Red）と After（Green）の対比を明確な証跡として提示する".into(),
        ],
        source_feedback_ids: vec![],
        updated_by: seed_actor.clone(),
        updated_at: SEED_TS,
    };

    let root_cause_spec = ProcedureSpec {
        version: SpecVersion::new(1),
        changelog: "Initial general-purpose root-cause bug fix procedure spec".into(),
        source_feedback_ids: vec![],
        updated_by: seed_actor.clone(),
        updated_at: SEED_TS,
        stages: vec![
            StageDef {
                id: StageId::from("reproduce"),
                label: "Reproduce (Red)".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure:
                    "修正コードに触れる前に、障害の最小再現条件を記録し、現行状態で必ず失敗（Red）する決定論的再現テストと周辺境界ガードを先行固定する。"
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "障害の最小再現条件と現行の失敗挙動（Before）が記録されている".into(),
                        "修正前の現行コードで再現テストが意図通り失敗（Red）している".into(),
                        "周辺の既存挙動ガードが現行状態で通過している".into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: Some(SubMachineTemplateId::from("tmpl-red-repro")),
                next: vec![StageId::from("diagnose")],
                rework_to: None,
            },
            StageDef {
                id: StageId::from("diagnose"),
                label: "Root Cause".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure:
                    "症状ではなく破綻した不変条件・状態遷移・境界条件の根本原因を特定し、類似箇所への横展開影響と対症療法を避けた修正設計を確定する。"
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "症状ではなく根本原因と発生メカニズムが一次実装から特定されている".into(),
                        "対症療法を避けた修正方針と類似箇所への影響範囲が明確である".into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: None,
                next: vec![StageId::from("fix")],
                rework_to: Some(StageId::from("reproduce")),
            },
            StageDef {
                id: StageId::from("fix"),
                label: "Fix & Lock (Green)".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure:
                    "根本原因を最小かつ本質的な変更で修正し、先行固定した再現テストを Green に転じさせる。境界ケーステスト追加と全テスト・ビルド・lint 警告ゼロを通過させる。"
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "先行固定した再現テストが改変なしで通過（Green）している".into(),
                        "全テスト・ビルド・lint が警告ゼロで通過している".into(),
                        "ローカル固有情報・機密・関係ない変更が混入していない".into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: Some(SubMachineTemplateId::from("tmpl-build-loop")),
                next: vec![StageId::from("verify")],
                rework_to: Some(StageId::from("diagnose")),
            },
            StageDef {
                id: StageId::from("verify"),
                label: "Verify & Diff".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure:
                    "実機・本番相当環境で障害解消を確認し、Before（Red）と After（Green）および周辺挙動の退行ゼロを前後比較して証跡 Artifact と Why 要約を揃える。"
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "実機または本番相当環境で障害解消が確認済みである".into(),
                        "Before/After 比較で障害解消と周辺退行ゼロが実証されている".into(),
                        "証跡 Artifact と原因・修正 Why 要約が揃っている".into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: Some(SubMachineTemplateId::from("tmpl-verify-ship")),
                next: vec![StageId::from("approve")],
                rework_to: Some(StageId::from("fix")),
            },
            StageDef {
                id: StageId::from("approve"),
                label: "Human Gate".into(),
                runner: "You".into(),
                runner_kind: RunnerKind::Human,
                kind: StageKind::ApprovalGate,
                procedure:
                    "根本原因分析・再現テスト・前後比較証跡の妥当性を人間オーナーが確認し、Approve または理由付き Rework を行う。"
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "対症療法ではなく根本原因が解消されている".into(),
                        "影響範囲とリリースリスクが許容範囲内である".into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: true,
                }),
                sub_template_id: None,
                next: vec![StageId::from("resolved")],
                rework_to: Some(StageId::from("fix")),
            },
            StageDef {
                id: StageId::from("resolved"),
                label: "Resolved".into(),
                runner: "Done".into(),
                runner_kind: RunnerKind::Deterministic,
                kind: StageKind::Terminal,
                procedure:
                    "障害修正・再発防止テスト固定・Why 記録済みコミット完了。再発防止の知見は Feedback として還元可能な状態。"
                        .into(),
                gate_rubric: None,
                sub_template_id: None,
                next: vec![],
                rework_to: None,
            },
        ],
    };

    let perf_philosophy = Philosophy {
        version: PhilosophyVersion::new(1),
        summary:
            "未観測区間の計装と Before ベースライン・品質退行ゼロガードの先行固定を起点に、原則的改善と同一条件の前後比較で勝つ汎用性能最適化ディストリクト。"
                .into(),
        principles: vec![
            Principle {
                id: "P-PRF-1".into(),
                title: "Make Unobservable Stages Observable First".into(),
                statement:
                    "測れない領域を勘でチューニングしない。未観測のボトルネックは自律的に計測計装を整備して可視化してから着手する。"
                        .into(),
                rationale:
                    "可視化なしの最適化は支配的でない箇所に複雑さを増やすだけで、実効性能の改善に繋がらないため。"
                        .into(),
            },
            Principle {
                id: "P-PRF-2".into(),
                title: "Baseline & Quality Guard Before Tuning".into(),
                statement:
                    "最適化に着手する前に、同一条件で再現可能な Before 定量ベースラインと、正確性・視認性・UX の退行を検知する決定論的ガードを先行固定する。"
                        .into(),
                rationale:
                    "品質を壊したことに気づかない最適化や比較不能なベンチマークは、製品価値を毀損するため。"
                        .into(),
            },
            Principle {
                id: "P-PRF-3".into(),
                title: "Zero Quality Degradation".into(),
                statement:
                    "精度・応答性・視認性・機能・安全性を削って数値を稼ぐ近道を禁止し、データ構造・計算量・I/O・キャッシュ・アイドル停止など構造的改善で勝つ。"
                        .into(),
                rationale:
                    "体験や正確性を犠牲にした高速化は劣化であり、商用製品としての基準を満たさないため。"
                        .into(),
            },
            Principle {
                id: "P-PRF-4".into(),
                title: "Same-Condition Before/After Proof".into(),
                statement:
                    "すべての改善効果は同一入力・同一環境・同一設定下での Before/After 統計比較で実証し、再現手順と実測値を証跡として残す。"
                        .into(),
                rationale:
                    "条件の異なる計測値や単発の好結果では真の改善とノイズを区別できないため。"
                        .into(),
            },
        ],
        anti_patterns: vec![
            "計測やプロファイルなしに勘で最適化を入れる".into(),
            "Before ベースラインや品質ガードを固定せずにチューニングを始める".into(),
            "品質・精度・機能・アイドル時の省電力性を犠牲にして見かけの数値を稼ぐ".into(),
            "環境や負荷条件が違う Before/After を並べて改善と主張する".into(),
            "改善効果が立証できない複雑化コードを残したままにする".into(),
        ],
        decision_heuristics: vec![
            "アイドル時の無駄な処理・再計算・ポーリングの根絶をマイクロ最適化より優先する"
                .into(),
            "改善幅がノイズ範囲に留まる変更は潔く捨て、支配的ボトルネックへ集中する".into(),
            "計測ハーネスと品質回帰ガードは一度きりの使い捨てにせず、再実行可能な形で残す"
                .into(),
        ],
        source_feedback_ids: vec![],
        updated_by: seed_actor.clone(),
        updated_at: SEED_TS,
    };

    let perf_spec = ProcedureSpec {
        version: SpecVersion::new(1),
        changelog: "Initial general-purpose performance and efficiency procedure spec".into(),
        source_feedback_ids: vec![],
        updated_by: seed_actor.clone(),
        updated_at: SEED_TS,
        stages: vec![
            StageDef {
                id: StageId::from("instrument"),
                label: "Observe & Scope".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure:
                    "対象ワークロード・評価指標（レイテンシ・スループット・メモリ・CPU/電力等）を定義し、未観測区間があれば再現可能な計測計装を整備する。"
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "対象ワークロード・評価指標・退行禁止ラインが明文化されている".into(),
                        "未観測区間が再現可能な形で計測できる状態になっている".into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: Some(SubMachineTemplateId::from("tmpl-observe")),
                next: vec![StageId::from("baseline_guard")],
                rework_to: None,
            },
            StageDef {
                id: StageId::from("baseline_guard"),
                label: "Baseline & Quality Guard".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure:
                    "最適化実装前に、現行状態の Before 定量ベースラインを記録し、正確性・出力品質・UX の退行を自動検知する決定論的ガードを先行実装・固定する。"
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "再現可能な同一条件の Before 定量ベースラインが記録されている".into(),
                        "品質・正確性の退行を検知する決定論的ガードが先行固定され現行通過している"
                            .into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: Some(SubMachineTemplateId::from("tmpl-baseline-guard")),
                next: vec![StageId::from("optimize")],
                rework_to: Some(StageId::from("instrument")),
            },
            StageDef {
                id: StageId::from("optimize"),
                label: "Optimize Loop".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure:
                    "プロファイルに基づく支配的ボトルネックに対し、品質・機能を一切劣化させない原則的改善と同一条件再計測・品質ガード検証を反復する。"
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "品質・正確性を損なわない構造的改善になっている".into(),
                        "先行品質ガード・全テスト・ビルド・lint が警告ゼロで通過している".into(),
                        "改善効果のない複雑化コードやローカル固有パスが残っていない".into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: Some(SubMachineTemplateId::from("tmpl-perf-loop")),
                next: vec![StageId::from("verify")],
                rework_to: Some(StageId::from("baseline_guard")),
            },
            StageDef {
                id: StageId::from("verify"),
                label: "Before/After Proof".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure:
                    "実機・本番相当ビルドで動作とアイドル時挙動を確認し、Before ベースラインとの同一条件定量比較テーブル・証跡 Artifact・Why 要約を揃える。"
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "実機・本番相当ビルドでの動作確認（アイドル時含む）が完了している".into(),
                        "同一条件の Before/After 定量比較で改善と品質退行ゼロが実証されている"
                            .into(),
                        "計測証跡 Artifact と Why 要約が揃っている".into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: Some(SubMachineTemplateId::from("tmpl-verify-ship")),
                next: vec![StageId::from("approve")],
                rework_to: Some(StageId::from("optimize")),
            },
            StageDef {
                id: StageId::from("approve"),
                label: "Human Gate".into(),
                runner: "You".into(),
                runner_kind: RunnerKind::Human,
                kind: StageKind::ApprovalGate,
                procedure:
                    "Before/After 計測値・品質ガード結果・コード複雑度とのバランスを人間オーナーが最終確認し、Approve または理由付き Rework を行う。"
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "品質・可読性・保守性を損なわずに意味のある性能改善が得られている"
                            .into(),
                        "退行リスクと複雑度コストが許容範囲内である".into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: true,
                }),
                sub_template_id: None,
                next: vec![StageId::from("shipped")],
                rework_to: Some(StageId::from("optimize")),
            },
            StageDef {
                id: StageId::from("shipped"),
                label: "Shipped".into(),
                runner: "Done".into(),
                runner_kind: RunnerKind::Deterministic,
                kind: StageKind::Terminal,
                procedure:
                    "最適化承認・Before/After 証跡付きコミット完了。ガードと計測基盤が維持された状態。"
                        .into(),
                gate_rubric: None,
                sub_template_id: None,
                next: vec![],
                rework_to: None,
            },
        ],
    };

    let prune_philosophy = Philosophy {
        version: PhilosophyVersion::new(1),
        summary:
            "既存挙動ガードの先行固定のもとで重複・肥大・契約ドリフト・環境依存を削ぎ落とし、単一ソースと完全な移植性へ磨き上げる簡素化・堅牢化ディストリクト。"
                .into(),
        principles: vec![
            Principle {
                id: "P-PRN-1".into(),
                title: "Subtract Before Adding".into(),
                statement:
                    "複雑さの解消は新しい層や手順書を足すのではなく、不要な分岐・重複・死んだコード・冗長設定を削ることから始める。"
                        .into(),
                rationale:
                    "足し算の整理は保守面積を増やすだけで、本質的な見通しの良さと堅牢性は削減によって得られるため。"
                        .into(),
            },
            Principle {
                id: "P-PRN-2".into(),
                title: "Lock Behavior Guards Before Deletion".into(),
                statement:
                    "削除・統合・リファクタリングに着手する前に、維持すべき外部挙動・永続化互換・契約のベースラインと自動ガードを先行固定する。"
                        .into(),
                rationale:
                    "ガードなしの削除は隠れた依存や後方互換の破壊を見逃すため。"
                        .into(),
            },
            Principle {
                id: "P-PRN-3".into(),
                title: "Single Source of Truth, Zero Drift".into(),
                statement:
                    "仕様・スキーマ・設計トークン・ルール・実装の二重管理を解消し、単一ソースと決定論的な同期検証でドリフトを構造的に防ぐ。"
                        .into(),
                rationale:
                    "並存する設定や重複定義は必ず時間とともに乖離し、バグと運用コストの温床になるため。"
                        .into(),
            },
            Principle {
                id: "P-PRN-4".into(),
                title: "Clean Portability & Strict Hygiene".into(),
                statement:
                    "ローカル固有パス・ホスト依存・秘密情報・環境依存テストを排除し、どのマシンやディレクトリでも同一にビルド・検証・動作する状態へ整える。"
                        .into(),
                rationale:
                    "環境依存の混入はチーム展開・CI・クリーン再構築の再現性を壊すため。"
                        .into(),
            },
        ],
        anti_patterns: vec![
            "挙動ガードを固定しないまま大規模な削除や統合を行う".into(),
            "整理のつもりで新しいラッパー・冗長なドキュメント・二重設定を増やす".into(),
            "後方互換を黙って壊しながらリファクタリングする".into(),
            "機能追加と大規模な削減・整形を 1 つの曖昧な変更に混ぜる".into(),
        ],
        decision_heuristics: vec![
            "外部挙動ゼロ変化（ガード全緑）× 複雑さ・重複・依存の純減を成功基準にする".into(),
            "人間やエージェントが毎回気をつける必要がある約束事は、単一ソース化と自動同期テストへ置き換える"
                .into(),
            "使われていない設定・分岐・互換コードは影響範囲を一次確認したうえで迷わず撤去する"
                .into(),
        ],
        source_feedback_ids: vec![],
        updated_by: seed_actor.clone(),
        updated_at: SEED_TS,
    };

    let prune_spec = ProcedureSpec {
        version: SpecVersion::new(1),
        changelog: "Initial general-purpose prune and hardening procedure spec".into(),
        source_feedback_ids: vec![],
        updated_by: seed_actor,
        updated_at: SEED_TS,
        stages: vec![
            StageDef {
                id: StageId::from("audit"),
                label: "Audit Bloat & Drift".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure:
                    "重複定義・契約ドリフト・不要分岐・環境依存・冗長設定を全量棚卸しし、削減対象と維持すべき外部互換契約を確定する。"
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "削減・統合対象と参照箇所が全量トレースされている".into(),
                        "維持すべき外部挙動・後方互換・単一ソース方針が明確である".into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: Some(SubMachineTemplateId::from("tmpl-evidence")),
                next: vec![StageId::from("baseline_guard")],
                rework_to: None,
            },
            StageDef {
                id: StageId::from("baseline_guard"),
                label: "Lock Behavior Guards".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure:
                    "削除・統合に着手する前に、維持すべき外部挙動・互換フォーマット・契約同期の Before ベースラインと決定論的ガードを先行実装・固定する。"
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "変更前（Before）の外部挙動・契約・複雑度メトリクスが記録されている"
                            .into(),
                        "外部挙動と契約同期を守る自動ガードが先行固定され現行通過している".into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: Some(SubMachineTemplateId::from("tmpl-baseline-guard")),
                next: vec![StageId::from("simplify")],
                rework_to: Some(StageId::from("audit")),
            },
            StageDef {
                id: StageId::from("simplify"),
                label: "Prune & Unify".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure:
                    "先行ガードを破らずに重複・不要分岐・環境依存を削除し単一ソースへ統合する。全テスト・ビルド・lint 警告ゼロを通過させる。"
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "新たな重複やラッパーを増やさず複雑さ・重複が純減している".into(),
                        "先行ガードを含む全テスト・ビルド・lint が警告ゼロで通過している".into(),
                        "ローカル固有パス・環境依存・秘密情報がゼロである".into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: Some(SubMachineTemplateId::from("tmpl-build-loop")),
                next: vec![StageId::from("verify")],
                rework_to: Some(StageId::from("audit")),
            },
            StageDef {
                id: StageId::from("verify"),
                label: "Verify Zero Diff".into(),
                runner: "AI Agent".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure:
                    "クリーン環境・実機で動作確認し、Before 基準と比較して外部挙動ゼロ変化・契約ドリフトゼロ・純減結果を証跡 Artifact と Why 要約にまとめる。"
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "実機またはクリーン環境での動作確認が完了している".into(),
                        "Before/After 比較で外部挙動ゼロ変化と複雑さ・重複の削減が確認済みである"
                            .into(),
                        "証跡 Artifact と Why 要約が揃っている".into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: Some(SubMachineTemplateId::from("tmpl-verify-ship")),
                next: vec![StageId::from("approve")],
                rework_to: Some(StageId::from("simplify")),
            },
            StageDef {
                id: StageId::from("approve"),
                label: "Human Gate".into(),
                runner: "You".into(),
                runner_kind: RunnerKind::Human,
                kind: StageKind::ApprovalGate,
                procedure:
                    "削減内容・互換性維持・単一ソース化の妥当性を人間オーナーが最終確認し、Approve または理由付き Rework を行う。"
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "必要な挙動・後方互換を一切損なわず簡素化されている".into(),
                        "単一ソースと移植性の原則に合致している".into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: true,
                }),
                sub_template_id: None,
                next: vec![StageId::from("hardened")],
                rework_to: Some(StageId::from("simplify")),
            },
            StageDef {
                id: StageId::from("hardened"),
                label: "Hardened".into(),
                runner: "Done".into(),
                runner_kind: RunnerKind::Deterministic,
                kind: StageKind::Terminal,
                procedure:
                    "削減・単一ソース化・ガード固定・Why 記録済みコミット完了。ドリフトのないクリーンな状態。"
                        .into(),
                gate_rubric: None,
                sub_template_id: None,
                next: vec![],
                rework_to: None,
            },
        ],
    };

    let districts = vec![
        District {
            id: DistrictId::from("DIST-001"),
            owner_id: owner_id.clone(),
            name: "DISTRICT 01 - Product Delivery".into(),
            setup_by: "General Blueprint".into(),
            active_philosophy_version: PhilosophyVersion::new(1),
            philosophy_history: vec![delivery_philosophy],
            active_spec_version: SpecVersion::new(1),
            spec_history: vec![delivery_spec],
        },
        District {
            id: DistrictId::from("DIST-002"),
            owner_id: owner_id.clone(),
            name: "DISTRICT 02 - Spike & Decision".into(),
            setup_by: "General Blueprint".into(),
            active_philosophy_version: PhilosophyVersion::new(1),
            philosophy_history: vec![spike_philosophy],
            active_spec_version: SpecVersion::new(1),
            spec_history: vec![spike_spec],
        },
        District {
            id: DistrictId::from("DIST-003"),
            owner_id: owner_id.clone(),
            name: "DISTRICT 03 - Root-Cause Fix".into(),
            setup_by: "General Blueprint".into(),
            active_philosophy_version: PhilosophyVersion::new(1),
            philosophy_history: vec![root_cause_philosophy],
            active_spec_version: SpecVersion::new(1),
            spec_history: vec![root_cause_spec],
        },
        District {
            id: DistrictId::from("DIST-004"),
            owner_id: owner_id.clone(),
            name: "DISTRICT 04 - Perf & Efficiency".into(),
            setup_by: "General Blueprint".into(),
            active_philosophy_version: PhilosophyVersion::new(1),
            philosophy_history: vec![perf_philosophy],
            active_spec_version: SpecVersion::new(1),
            spec_history: vec![perf_spec],
        },
        District {
            id: DistrictId::from("DIST-005"),
            owner_id: owner_id.clone(),
            name: "DISTRICT 05 - Prune & Hardening".into(),
            setup_by: "General Blueprint".into(),
            active_philosophy_version: PhilosophyVersion::new(1),
            philosophy_history: vec![prune_philosophy],
            active_spec_version: SpecVersion::new(1),
            spec_history: vec![prune_spec],
        },
    ];

    WorkspaceSeed {
        templates,
        template_order,
        districts,
        jobs: Vec::new(),
        next_job_seq: 1,
        next_fencing_token: 1,
    }
}
