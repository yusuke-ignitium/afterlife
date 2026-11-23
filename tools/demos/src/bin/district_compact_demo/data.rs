#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DemoScenario {
    Realistic,
    Busy,
    Quiet,
}

impl DemoScenario {
    pub const ALL: [Self; 3] = [Self::Realistic, Self::Busy, Self::Quiet];

    pub fn id(self) -> &'static str {
        match self {
            Self::Realistic => "scen-realistic",
            Self::Busy => "scen-busy",
            Self::Quiet => "scen-quiet",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Realistic => "Realistic (8)",
            Self::Busy => "Busy (14)",
            Self::Quiet => "Quiet (0)",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum StageKind {
    Work,
    ApprovalGate,
    Terminal,
}

#[derive(Clone)]
pub struct DemoJob {
    pub id: &'static str,
    pub title: &'static str,
    pub sub_stage: Option<&'static str>,
}

#[derive(Clone)]
pub struct DemoStage {
    pub label: &'static str,
    pub short_label: &'static str,
    pub runner: &'static str,
    pub kind: StageKind,
    pub sub_sm_name: Option<&'static str>,
    pub sub_stages: &'static [(&'static str, &'static str)],
    pub jobs: Vec<DemoJob>,
}

impl DemoStage {
    fn work(
        label: &'static str,
        short_label: &'static str,
        sub_sm_name: Option<&'static str>,
        sub_stages: &'static [(&'static str, &'static str)],
        jobs: Vec<DemoJob>,
    ) -> Self {
        Self {
            label,
            short_label,
            runner: "Agent",
            kind: StageKind::Work,
            sub_sm_name,
            sub_stages,
            jobs,
        }
    }

    fn gate(jobs: Vec<DemoJob>) -> Self {
        Self {
            label: "Human Gate",
            short_label: "Approve",
            runner: "Human",
            kind: StageKind::ApprovalGate,
            sub_sm_name: None,
            sub_stages: &[],
            jobs,
        }
    }

    fn done(label: &'static str, short_label: &'static str) -> Self {
        Self {
            label,
            short_label,
            runner: "Done",
            kind: StageKind::Terminal,
            sub_sm_name: None,
            sub_stages: &[],
            jobs: Vec::new(),
        }
    }
}

#[derive(Clone)]
pub struct DemoDistrict {
    pub id: &'static str,
    pub name: &'static str,
    pub version: &'static str,
    pub stages: Vec<DemoStage>,
}

impl DemoDistrict {
    pub fn active_count(&self) -> usize {
        self.stages.iter().map(|s| s.jobs.len()).sum()
    }
}

fn job(id: &'static str, title: &'static str, sub_stage: Option<&'static str>) -> DemoJob {
    DemoJob {
        id,
        title,
        sub_stage,
    }
}

pub fn build_districts(scenario: DemoScenario) -> Vec<DemoDistrict> {
    const SM_GUARD: &[(&str, &str)] = &[
        ("capture_base", "Capture Baseline"),
        ("lock_guard", "Lock Guard"),
    ];
    const SM_BUILD: &[(&str, &str)] = &[
        ("apply_change", "Apply Change"),
        ("edge_tests", "Harden Tests"),
        ("verify_loop", "Zero-Warn Verify"),
    ];
    const SM_SHIP: &[(&str, &str)] = &[
        ("env_check", "Real-Env Check"),
        ("compare", "Before/After"),
    ];
    const SM_EVIDENCE: &[(&str, &str)] = &[
        ("trace", "Trace Sources"),
        ("options", "Evaluate Options"),
        ("criteria", "Lock Criteria"),
    ];
    const SM_REPRO: &[(&str, &str)] = &[
        ("red_repro", "Lock Repro Test"),
        ("blast_radius", "Map Blast Radius"),
    ];
    const SM_PERF: &[(&str, &str)] = &[
        ("profile", "Profile Bottleneck"),
        ("optimize", "Principled Fix"),
        ("remeasure", "Remeasure"),
    ];

    let (mut d1p, mut d1b, mut d1v, mut d1a) = (vec![], vec![], vec![], vec![]);
    let (mut d2s, mut d2a, mut d3r, mut d4p, mut d5p) = (vec![], vec![], vec![], vec![], vec![]);

    match scenario {
        DemoScenario::Realistic => {
            d1b.push(job("JOB-014", "District Create / Archive / Restore", Some("apply_change")));
            d1b.push(job("JOB-017", "Compact Mode Toggle Persistence", Some("edge_tests")));
            d1b.push(job("JOB-018", "Sub-SM Active Job Indicator", Some("verify_loop")));
            d1b.push(job("JOB-019", "Keyboard Shortcuts for Quick Nav", Some("apply_change")));
            d1b.push(job("JOB-020", "Artifact Diff Preview Highlight", Some("edge_tests")));
            d1a.push(job("JOB-012", "Detail Sticky Footer (C1 Pill)", None));
            d2s.push(job("JOB-016", "External CI Blocker Phase 2 Shaping", Some("options")));
            d4p.push(job("JOB-015", "Test Suite Wall-Clock Optimization", Some("profile")));
        }
        DemoScenario::Busy => {
            d1p.push(job("JOB-021", "Command Palette Interaction Spec", Some("lock_guard")));
            d1b.push(job("JOB-014", "District Create / Archive / Restore", Some("apply_change")));
            d1b.push(job("JOB-017", "Compact Mode Toggle Persistence", Some("edge_tests")));
            d1b.push(job("JOB-018", "Sub-SM Active Job Indicator", Some("verify_loop")));
            d1b.push(job("JOB-019", "Keyboard Shortcuts for Quick Nav", Some("apply_change")));
            d1b.push(job("JOB-020", "Artifact Diff Preview Highlight", Some("edge_tests")));
            d1v.push(job("JOB-013", "Fade UI Transparency Slider (0-90%)", Some("compare")));
            d1a.push(job("JOB-012", "Detail Sticky Footer (C1 Pill)", None));
            d1a.push(job("JOB-022", "Approval Batch Quick Confirm", None));
            d1a.push(job("JOB-023", "Workspace Filter Persistence", None));
            d1a.push(job("JOB-024", "Stage Overflow Tooltip", None));
            d2a.push(job("JOB-016", "External CI Blocker Phase 2", None));
            d3r.push(job("JOB-025", "Fix Multi-Display Window Restore", Some("red_repro")));
            d4p.push(job("JOB-015", "Test Suite Wall-Clock Optimization", Some("profile")));
            d5p.push(job("JOB-026", "Split engine/mod.rs Below 650 Lines", Some("apply_change")));
        }
        DemoScenario::Quiet => {}
    }

    vec![
        DemoDistrict {
            id: "DIST-001",
            name: "DISTRICT 01 - Product Delivery",
            version: "Philosophy v1 · Spec v1",
            stages: vec![
                DemoStage::work("Plan & Guard", "Plan", Some("Baseline & Guard"), SM_GUARD, d1p),
                DemoStage::work("Build & Hygiene", "Build", Some("Build & Guard Loop"), SM_BUILD, d1b),
                DemoStage::work("Verify & Evidence", "Verify", Some("Verify & Ship"), SM_SHIP, d1v),
                DemoStage::gate(d1a),
                DemoStage::done("Shipped", "Shipped"),
            ],
        },
        DemoDistrict {
            id: "DIST-002",
            name: "DISTRICT 02 - Discovery & Shaping",
            version: "Philosophy v2 · Spec v2",
            stages: vec![
                DemoStage::work("Recon", "Recon", Some("Evidence Gathering"), SM_EVIDENCE, vec![]),
                DemoStage::work("Options", "Options", None, &[], vec![]),
                DemoStage::work("Shape Spec", "Shape", Some("Evidence Gathering"), SM_EVIDENCE, d2s),
                DemoStage::gate(d2a),
                DemoStage::done("Shaped", "Shaped"),
            ],
        },
        DemoDistrict {
            id: "DIST-003",
            name: "DISTRICT 03 - Root-Cause Fix",
            version: "Philosophy v1 · Spec v1",
            stages: vec![
                DemoStage::work("Red Repro", "Repro", Some("Red Repro Lock"), SM_REPRO, d3r),
                DemoStage::work("Root Fix", "Fix", Some("Build & Guard Loop"), SM_BUILD, vec![]),
                DemoStage::gate(vec![]),
                DemoStage::done("Resolved", "Done"),
            ],
        },
        DemoDistrict {
            id: "DIST-004",
            name: "DISTRICT 04 - Perf & Efficiency",
            version: "Philosophy v1 · Spec v1",
            stages: vec![
                DemoStage::work("Observe & Metric", "Observe", Some("Perf Loop"), SM_PERF, d4p),
                DemoStage::work("Optimize", "Optimize", Some("Verify & Ship"), SM_SHIP, vec![]),
                DemoStage::gate(vec![]),
                DemoStage::done("Verified", "Done"),
            ],
        },
        DemoDistrict {
            id: "DIST-005",
            name: "DISTRICT 05 - Prune & Hardening",
            version: "Philosophy v1 · Spec v1",
            stages: vec![
                DemoStage::work("Prune & Split", "Prune", Some("Build & Guard Loop"), SM_BUILD, d5p),
                DemoStage::gate(vec![]),
                DemoStage::done("Hardened", "Done"),
            ],
        },
    ]
}
