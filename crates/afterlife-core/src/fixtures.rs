use std::collections::HashMap;

use crate::{
    ids::{
        DistrictId, FencingToken, JobId, LeaseId, OwnerId, PhilosophyVersion, RunnerId,
        SpecVersion, StageId, SubMachineTemplateId, SubStageId,
    },
    model::{
        Actor, Artifact, ArtifactKind, District, GateRubric, Job, JobLease, JobLocation,
        Philosophy, Principle, ProcedureSpec, RunnerKind, StageDef, StageKind, SubMachineTemplate,
        SubStageDef,
    },
};

pub const LOCAL_OWNER_ID: &str = "local-owner";
const SEED_TS: u64 = 1_763_600_000_000;
const SEED_TTL: u64 = 300_000;

fn seed_lease(seq: u64, runner_id: &str, runner_label: &str) -> JobLease {
    JobLease {
        lease_id: LeaseId::new(format!("lease-seed-{seq}")),
        fencing_token: FencingToken::new(seq),
        runner_id: RunnerId::from(runner_id),
        runner_label: runner_label.into(),
        holder_client_id: None,
        acquired_at: SEED_TS,
        expires_at: SEED_TS + SEED_TTL,
    }
}

pub fn sample_workspace(
    owner_id: &OwnerId,
) -> (
    HashMap<SubMachineTemplateId, SubMachineTemplate>,
    Vec<SubMachineTemplateId>,
    Vec<District>,
    Vec<Job>,
    u32,
    u64,
) {
    let seed_actor = Actor::local_human(owner_id.clone());

    let mut templates = HashMap::new();
    let tmpl_pr_ci = SubMachineTemplate {
        id: SubMachineTemplateId::from("tmpl-pr-ci"),
        name: "PR CI Watch".into(),
        description: "PR作成からCI監視、失敗時の自動修正ループ".into(),
        philosophy_notes: "CIが赤のまま人間レビューに回さない。差分は小さく自己説明的に保つ。"
            .into(),
        stages: vec![
            SubStageDef {
                id: SubStageId::from("create_pr"),
                label: "Create PR".into(),
                runner: "Dots (Git)".into(),
                runner_kind: RunnerKind::Ai,
                procedure: "変更差分を要約し、検証コマンド・ロールバック手順付きでPRを作成する。"
                    .into(),
                gate_checks: vec![
                    "PR本文に検証ログがある".into(),
                    "不要な差分が混入していない".into(),
                ],
            },
            SubStageDef {
                id: SubStageId::from("watch_ci"),
                label: "Watch CI".into(),
                runner: "CI Poller".into(),
                runner_kind: RunnerKind::Deterministic,
                procedure: "全CIチェック完了を監視し、失敗時は失敗ログを抽出して修正ループを回す。"
                    .into(),
                gate_checks: vec!["全必須チェックが緑である".into()],
            },
            SubStageDef {
                id: SubStageId::from("ci_green"),
                label: "CI Green".into(),
                runner: "Gate Ready".into(),
                runner_kind: RunnerKind::Deterministic,
                procedure: "CI緑の証跡Artifactを添付し、承認ゲートへ引き渡す。".into(),
                gate_checks: vec!["CiReport Artifact が紐づいている".into()],
            },
        ],
    };

    let tmpl_formal = SubMachineTemplate {
        id: SubMachineTemplateId::from("tmpl-formal"),
        name: "Formal Verify".into(),
        description: "仕様抽出からZ3・Lean検証、証明出力".into(),
        philosophy_notes: "エッジケースは勘ではなく不変条件と自動検証で潰す。".into(),
        stages: vec![
            SubStageDef {
                id: SubStageId::from("extract_prop"),
                label: "Extract Inv".into(),
                runner: "Claude Duet".into(),
                runner_kind: RunnerKind::Ai,
                procedure: "仕様から不変条件（競合・権限・状態遷移）を抽出しテスト仕様化する。"
                    .into(),
                gate_checks: vec!["最低1つ以上の異常系不変条件が定義されている".into()],
            },
            SubStageDef {
                id: SubStageId::from("z3_prove"),
                label: "Z3 Prove".into(),
                runner: "Z3 Verifier".into(),
                runner_kind: RunnerKind::Deterministic,
                procedure: "抽出した不変条件を検証器／ユニットテストで実行し証跡を残す。".into(),
                gate_checks: vec!["全検証が pass している".into()],
            },
        ],
    };

    let tmpl_research = SubMachineTemplate {
        id: SubMachineTemplateId::from("tmpl-research"),
        name: "Deep Triage".into(),
        description: "一次情報収集と比較要約".into(),
        philosophy_notes: "推測で補完せず、一次ソース・実測値・トレードオフを明記する。".into(),
        stages: vec![
            SubStageDef {
                id: SubStageId::from("crawl"),
                label: "Crawl Docs".into(),
                runner: "Grok Bot".into(),
                runner_kind: RunnerKind::Ai,
                procedure: "公式ドキュメント・一次実装・ベンチマークから根拠付きメモを収集する。"
                    .into(),
                gate_checks: vec!["すべての主張に一次ソースまたは実測根拠がある".into()],
            },
            SubStageDef {
                id: SubStageId::from("synthesize"),
                label: "Synthesize".into(),
                runner: "Claude Duet".into(),
                runner_kind: RunnerKind::Ai,
                procedure:
                    "意思決定に必要な比較軸（速度・コスト・堅牢性）で構造化ブリーフを作成する。"
                        .into(),
                gate_checks: vec!["推奨案と棄却案の理由が明記されている".into()],
            },
        ],
    };

    let template_order = vec![
        tmpl_pr_ci.id.clone(),
        tmpl_formal.id.clone(),
        tmpl_research.id.clone(),
    ];
    templates.insert(tmpl_pr_ci.id.clone(), tmpl_pr_ci);
    templates.insert(tmpl_formal.id.clone(), tmpl_formal);
    templates.insert(tmpl_research.id.clone(), tmpl_research);

    let code_philosophy = Philosophy {
        version: PhilosophyVersion::new(1),
        summary: "計測駆動・型安全・ロールバック可能性を最優先にする本番実装ディストリクト。"
            .into(),
        principles: vec![
            Principle {
                id: "P-CODE-1".into(),
                title: "Measure Before Optimize".into(),
                statement: "性能変更は必ず Before/After の定量実測値を伴わせる。".into(),
                rationale: "勘による最適化は複雑さだけ増やし逆行を見逃すため。".into(),
            },
            Principle {
                id: "P-CODE-2".into(),
                title: "Fail Closed on Boundaries".into(),
                statement: "権限・データ境界・競合ロックはアプリ層とDB層の両方で安全側に倒す。"
                    .into(),
                rationale:
                    "エージェントの暴走やネットワーク遅延によるデータ破損を構造的に防ぐため。"
                        .into(),
            },
        ],
        anti_patterns: vec![
            "テストやビルドを通さずに次ステージへ進める".into(),
            "承認ゲートをエージェント自身の判断でバイパスする".into(),
        ],
        decision_heuristics: vec![
            "迷ったら状態変更を小さく刻み、各ステップを独立コミット可能にする".into(),
            "抽象化はローカル⇔クラウドの切替点（Port）だけに限定する".into(),
        ],
        source_feedback_ids: vec![],
        updated_by: seed_actor.clone(),
        updated_at: SEED_TS,
    };

    let code_spec = ProcedureSpec {
        version: SpecVersion::new(1),
        changelog: "Initial code implementation district procedure spec".into(),
        source_feedback_ids: vec![],
        updated_by: seed_actor.clone(),
        updated_at: SEED_TS,
        stages: vec![
            StageDef {
                id: StageId::from("design"),
                label: "Design".into(),
                runner: "Claude Duet".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure: "要件・制約・境界条件を整理し、型定義と変更対象ファイルを明示した設計メモを作成する。".into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "変更対象と非対象が明記されている".into(),
                        "異常系・ロールバック方針がある".into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: None,
                next: vec![StageId::from("implement")],
                rework_to: None,
            },
            StageDef {
                id: StageId::from("implement"),
                label: "Implement".into(),
                runner: "Dots".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure: "設計メモに従い最小差分で実装し、ユニットテストと警告ゼロのチェックを通す。".into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "cargo check / test が全 pass している".into(),
                        "District Philosophy のアンチパターンに抵触していない".into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: Some(SubMachineTemplateId::from("tmpl-formal")),
                next: vec![StageId::from("pr_pipeline")],
                rework_to: Some(StageId::from("design")),
            },
            StageDef {
                id: StageId::from("pr_pipeline"),
                label: "PR & CI".into(),
                runner: "Dots + CI".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure: "PRを作成し全CIが緑になるまで監視・修正を回す。".into(),
                gate_rubric: Some(GateRubric {
                    checks: vec!["CI がすべて green である".into()],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: Some(SubMachineTemplateId::from("tmpl-pr-ci")),
                next: vec![StageId::from("approval_gate")],
                rework_to: Some(StageId::from("implement")),
            },
            StageDef {
                id: StageId::from("approval_gate"),
                label: "Approval".into(),
                runner: "You".into(),
                runner_kind: RunnerKind::Human,
                kind: StageKind::ApprovalGate,
                procedure: "LLM事前検証サマリとArtifactを確認し、人間オーナーがApproveまたは理由付きReworkを行う。".into(),
                gate_rubric: Some(GateRubric {
                    checks: vec![
                        "District Philosophy に合致している".into(),
                        "本番デプロイ時のリスクが許容範囲内である".into(),
                    ],
                    auto_verify_by_llm: true,
                    requires_human_approval: true,
                }),
                sub_template_id: None,
                next: vec![StageId::from("merged")],
                rework_to: Some(StageId::from("implement")),
            },
            StageDef {
                id: StageId::from("merged"),
                label: "Merged".into(),
                runner: "Done".into(),
                runner_kind: RunnerKind::Deterministic,
                kind: StageKind::Terminal,
                procedure: "マージ完了・テレメトリ記録済み。".into(),
                gate_rubric: None,
                sub_template_id: None,
                next: vec![],
                rework_to: None,
            },
        ],
    };

    let research_philosophy = Philosophy {
        version: PhilosophyVersion::new(1),
        summary: "一次情報と実測を武器に、意思決定コストを最小化するリサーチディストリクト。"
            .into(),
        principles: vec![Principle {
            id: "P-RES-1".into(),
            title: "Primary Evidence Only".into(),
            statement: "推測と一次事実を明確に分離し、取得不能な情報はその場で申告する。".into(),
            rationale: "誤った前提に基づく設計ミスを根絶するため。".into(),
        }],
        anti_patterns: vec!["出典や実測値のない一般論で比較表を埋める".into()],
        decision_heuristics: vec!["2〜3案に絞り、推奨案と撤退条件を必ず添える".into()],
        source_feedback_ids: vec![],
        updated_by: seed_actor.clone(),
        updated_at: SEED_TS,
    };

    let research_spec = ProcedureSpec {
        version: SpecVersion::new(1),
        changelog: "Initial research & spec district procedure spec".into(),
        source_feedback_ids: vec![],
        updated_by: seed_actor,
        updated_at: SEED_TS,
        stages: vec![
            StageDef {
                id: StageId::from("investigate"),
                label: "Investigate".into(),
                runner: "Grok Bot".into(),
                runner_kind: RunnerKind::Ai,
                kind: StageKind::Work,
                procedure:
                    "一次ソース収集・比較表・コスト/リスク試算を作成してArtifactとして添付する。"
                        .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec!["一次ソースと具体的な数値根拠が含まれる".into()],
                    auto_verify_by_llm: true,
                    requires_human_approval: false,
                }),
                sub_template_id: Some(SubMachineTemplateId::from("tmpl-research")),
                next: vec![StageId::from("brief_gate")],
                rework_to: None,
            },
            StageDef {
                id: StageId::from("brief_gate"),
                label: "Approval".into(),
                runner: "You".into(),
                runner_kind: RunnerKind::Human,
                kind: StageKind::ApprovalGate,
                procedure: "調査ブリーフの十分性と意思決定の妥当性を人間オーナーが最終確認する。"
                    .into(),
                gate_rubric: Some(GateRubric {
                    checks: vec!["次の実装フェーズに着手できる明確さがある".into()],
                    auto_verify_by_llm: true,
                    requires_human_approval: true,
                }),
                sub_template_id: None,
                next: vec![StageId::from("shipped")],
                rework_to: Some(StageId::from("investigate")),
            },
            StageDef {
                id: StageId::from("shipped"),
                label: "Shipped".into(),
                runner: "Done".into(),
                runner_kind: RunnerKind::Deterministic,
                kind: StageKind::Terminal,
                procedure: "調査ブリーフ確定済み。".into(),
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
            name: "DISTRICT 01 - Code Implementation".into(),
            setup_by: "AI Setup: Claude Duet".into(),
            active_philosophy_version: PhilosophyVersion::new(1),
            philosophy_history: vec![code_philosophy],
            active_spec_version: SpecVersion::new(1),
            spec_history: vec![code_spec],
        },
        District {
            id: DistrictId::from("DIST-002"),
            owner_id: owner_id.clone(),
            name: "DISTRICT 02 - Research & Spec".into(),
            setup_by: "AI Setup: Grok Bot".into(),
            active_philosophy_version: PhilosophyVersion::new(1),
            philosophy_history: vec![research_philosophy],
            active_spec_version: SpecVersion::new(1),
            spec_history: vec![research_spec],
        },
    ];

    let jobs = vec![
        Job {
            id: JobId::from("JOB-041"),
            owner_id: owner_id.clone(),
            revision: 4,
            title: "UIのpxトークンと縦中央揃えの決定論的ガードを適用".into(),
            raw_input: "uiのpxとボーダー幅をルール化してズレないようにしたい".into(),
            spec_summary: "Guard SpacePx/FontPx/RowHeightPx; enforce cap-height centering & Backdrop-Synced Glass.".into(),
            location: JobLocation::InDistrict {
                district_id: DistrictId::from("DIST-001"),
                stage_id: StageId::from("approval_gate"),
                sub_stage_id: None,
            },
            target_district_hint: Some(DistrictId::from("DIST-001")),
            pinned_philosophy_version: Some(PhilosophyVersion::new(1)),
            pinned_spec_version: Some(SpecVersion::new(1)),
            lease: None,
            artifacts: vec![
                Artifact::simple("art-041-1", "spec/ui-guard.md", ArtifactKind::Spec),
                Artifact::simple("art-041-2", "pr/#142", ArtifactKind::PullRequest),
                Artifact::simple("art-041-3", "ci-green.ok", ArtifactKind::CiReport),
            ],
            gate_verifications: vec![],
            feedback_ids: vec![],
            created_at: SEED_TS,
            updated_at: SEED_TS,
        },
        Job {
            id: JobId::from("JOB-042"),
            owner_id: owner_id.clone(),
            revision: 3,
            title: "SQLite WAL 同時claim時のロック待ちベンチマーク".into(),
            raw_input: "複数Runnerが同時にclaimした時のロック待ちを測りたい".into(),
            spec_summary: "BEGIN IMMEDIATE と楽観的 CAS を16並列Runnerで比較計測する。".into(),
            location: JobLocation::InDistrict {
                district_id: DistrictId::from("DIST-001"),
                stage_id: StageId::from("pr_pipeline"),
                sub_stage_id: Some(SubStageId::from("watch_ci")),
            },
            target_district_hint: Some(DistrictId::from("DIST-001")),
            pinned_philosophy_version: Some(PhilosophyVersion::new(1)),
            pinned_spec_version: Some(SpecVersion::new(1)),
            lease: Some(seed_lease(2, "ci-poller", "CI Poller")),
            artifacts: vec![
                Artifact::simple("art-042-1", "src/claim.rs", ArtifactKind::SourceCode),
                Artifact::simple("art-042-2", "pr/#145", ArtifactKind::PullRequest),
            ],
            gate_verifications: vec![],
            feedback_ids: vec![],
            created_at: SEED_TS,
            updated_at: SEED_TS,
        },
        Job {
            id: JobId::from("JOB-043"),
            owner_id: owner_id.clone(),
            revision: 2,
            title: "Grok Bot webhook adapter for PR review lane".into(),
            raw_input: "PRが開いたらGrok Botに差分投げてコメント返させる".into(),
            spec_summary: "Subscribe to pr_open transition; dispatch diff payload and await verdict.".into(),
            location: JobLocation::InDistrict {
                district_id: DistrictId::from("DIST-001"),
                stage_id: StageId::from("implement"),
                sub_stage_id: Some(SubStageId::from("z3_prove")),
            },
            target_district_hint: Some(DistrictId::from("DIST-001")),
            pinned_philosophy_version: Some(PhilosophyVersion::new(1)),
            pinned_spec_version: Some(SpecVersion::new(1)),
            lease: Some(seed_lease(3, "z3-verifier", "Z3 Verifier")),
            artifacts: vec![Artifact::simple(
                "art-043-1",
                "design/grok-adapter.md",
                ArtifactKind::Spec,
            )],
            gate_verifications: vec![],
            feedback_ids: vec![],
            created_at: SEED_TS,
            updated_at: SEED_TS,
        },
        Job {
            id: JobId::from("JOB-044"),
            owner_id: owner_id.clone(),
            revision: 2,
            title: "ローカルRunner間IPC（UDS vs Mach Port）の比較調査".into(),
            raw_input: "Macローカルで最速のIPC方式を調査したい".into(),
            spec_summary: "Unix Domain Socket と XPC/Mach Port のレイテンシおよび再接続挙動を比較。".into(),
            location: JobLocation::InDistrict {
                district_id: DistrictId::from("DIST-002"),
                stage_id: StageId::from("brief_gate"),
                sub_stage_id: None,
            },
            target_district_hint: Some(DistrictId::from("DIST-002")),
            pinned_philosophy_version: Some(PhilosophyVersion::new(1)),
            pinned_spec_version: Some(SpecVersion::new(1)),
            lease: Some(seed_lease(4, "human-operator", "You")),
            artifacts: vec![Artifact::simple(
                "art-044-1",
                "briefs/ipc-bench.md",
                ArtifactKind::Spec,
            )],
            gate_verifications: vec![],
            feedback_ids: vec![],
            created_at: SEED_TS,
            updated_at: SEED_TS,
        },
        Job {
            id: JobId::from("JOB-045"),
            owner_id: owner_id.clone(),
            revision: 1,
            title: "差し戻し時の成果物スナップショット自動保存に対応".into(),
            raw_input: "差し戻しした時に前の成果物スナップショット残るようにして".into(),
            spec_summary: "Jobsで待機中。RunnerによるClaim待ち。".into(),
            location: JobLocation::Queue,
            target_district_hint: Some(DistrictId::from("DIST-001")),
            pinned_philosophy_version: None,
            pinned_spec_version: None,
            lease: None,
            artifacts: vec![Artifact::simple(
                "art-045-1",
                "raw-note.txt",
                ArtifactKind::RawNote,
            )],
            gate_verifications: vec![],
            feedback_ids: vec![],
            created_at: SEED_TS,
            updated_at: SEED_TS,
        },
        Job {
            id: JobId::from("JOB-046"),
            owner_id: owner_id.clone(),
            revision: 1,
            title: "Auto-route CI failure logs to Dots".into(),
            raw_input: "CI落ちたらログ抜粋してDotsに直させる".into(),
            spec_summary: "Waiting in Jobs for Runner claim.".into(),
            location: JobLocation::Queue,
            target_district_hint: Some(DistrictId::from("DIST-001")),
            pinned_philosophy_version: None,
            pinned_spec_version: None,
            lease: None,
            artifacts: vec![Artifact::simple(
                "art-046-1",
                "raw-note.txt",
                ArtifactKind::RawNote,
            )],
            gate_verifications: vec![],
            feedback_ids: vec![],
            created_at: SEED_TS,
            updated_at: SEED_TS,
        },
        Job {
            id: JobId::from("JOB-047"),
            owner_id: owner_id.clone(),
            revision: 1,
            title: "Hiragino Sans と SF Mono 混植時のベースライン検証".into(),
            raw_input: "Hiragino SansとSF Monoの混植時のベースラインズレ検証".into(),
            spec_summary: "日本語・英数字混在ラベルのCap-Height中央揃えと折り返し視認性を確認。".into(),
            location: JobLocation::Queue,
            target_district_hint: Some(DistrictId::from("DIST-002")),
            pinned_philosophy_version: None,
            pinned_spec_version: None,
            lease: None,
            artifacts: vec![Artifact::simple(
                "art-047-1",
                "raw-note.txt",
                ArtifactKind::RawNote,
            )],
            gate_verifications: vec![],
            feedback_ids: vec![],
            created_at: SEED_TS,
            updated_at: SEED_TS,
        },
    ];

    (templates, template_order, districts, jobs, 48, 5)
}
