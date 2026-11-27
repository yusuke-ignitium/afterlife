use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use afterlife_core::{JobId, ProposalId};

use crate::app::{
    motion::ScalarTransition,
    state::{AppState, TabId},
};

pub const CONFIRM_ARM_MS: u64 = 220;
pub const CONFIRM_DISARM_MS: u64 = 200;
pub const CONFIRM_AUTO_DISARM_MS: u64 = 3_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfirmTone {
    Approve,
    Rework,
    Archive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfirmPressOutcome {
    Armed { epoch: u64 },
    Confirmed,
}

#[derive(Clone, PartialEq, Eq)]
struct ConfirmContextKey {
    active_tab: TabId,
    detail_open: bool,
    selected_job_id: JobId,
    selected_proposal_id: Option<ProposalId>,
}

impl ConfirmContextKey {
    fn from_state(st: &AppState) -> Self {
        Self {
            active_tab: st.active_tab,
            detail_open: st.detail_open,
            selected_job_id: st.selected_job_id.clone(),
            selected_proposal_id: st.selected_proposal_id.clone(),
        }
    }
}

#[derive(Clone, Debug)]
struct ArmedButton {
    key: String,
    epoch: u64,
}

#[derive(Clone, Debug, Default)]
pub struct ConfirmSnapshot {
    weights: HashMap<String, f32>,
    pub animating: bool,
}

impl ConfirmSnapshot {
    pub fn weight(&self, key: &str) -> f32 {
        self.weights.get(key).copied().unwrap_or(0.0)
    }
}

pub struct ConfirmState {
    armed: Option<ArmedButton>,
    epoch: u64,
    transitions: HashMap<String, ScalarTransition>,
    last_context: Option<ConfirmContextKey>,
}

impl Default for ConfirmState {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfirmState {
    pub fn new() -> Self {
        Self {
            armed: None,
            epoch: 0,
            transitions: HashMap::new(),
            last_context: None,
        }
    }

    pub fn is_armed(&self, key: &str) -> bool {
        self.armed.as_ref().is_some_and(|a| a.key == key)
    }

    pub fn disarm(&mut self, now: Instant) -> bool {
        let Some(prev) = self.armed.take() else {
            return false;
        };
        self.transitions
            .entry(prev.key)
            .or_insert_with(|| ScalarTransition::settled(1.0, now))
            .set_target(0.0, Duration::from_millis(CONFIRM_DISARM_MS), now);
        true
    }

    pub fn disarm_if_epoch(&mut self, epoch: u64, now: Instant) -> bool {
        if self.armed.as_ref().is_some_and(|a| a.epoch == epoch) {
            return self.disarm(now);
        }
        false
    }

    pub fn press(&mut self, key: impl Into<String>, now: Instant) -> ConfirmPressOutcome {
        let key = key.into();
        if self.is_armed(&key) {
            self.disarm(now);
            return ConfirmPressOutcome::Confirmed;
        }
        self.disarm(now);
        self.epoch += 1;
        let epoch = self.epoch;
        self.transitions
            .entry(key.clone())
            .or_insert_with(|| ScalarTransition::settled(0.0, now))
            .set_target(1.0, Duration::from_millis(CONFIRM_ARM_MS), now);
        self.armed = Some(ArmedButton { key, epoch });
        ConfirmPressOutcome::Armed { epoch }
    }

    pub fn snapshot(&mut self, st: &AppState, now: Instant) -> ConfirmSnapshot {
        let ctx = ConfirmContextKey::from_state(st);
        if self.last_context.as_ref() != Some(&ctx) {
            if self.last_context.is_some() {
                self.disarm(now);
            }
            self.last_context = Some(ctx);
        }

        let armed_key = self.armed.as_ref().map(|a| a.key.as_str());
        self.transitions
            .retain(|k, tr| armed_key == Some(k.as_str()) || tr.is_active(now) || tr.value(now) > 0.002);

        let mut weights = HashMap::with_capacity(self.transitions.len());
        let mut animating = false;
        for (key, tr) in &self.transitions {
            weights.insert(key.clone(), tr.value(now));
            if tr.is_active(now) {
                animating = true;
            }
        }
        ConfirmSnapshot { weights, animating }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_press_arms_second_press_confirms_and_other_button_switches_arm() {
        let mut confirm = ConfirmState::new();
        let t0 = Instant::now();

        let first = confirm.press("detail-approve-JOB-001", t0);
        let ConfirmPressOutcome::Armed { epoch: e1 } = first else {
            panic!("expected first press to arm");
        };
        assert!(confirm.is_armed("detail-approve-JOB-001"));

        let second_other = confirm.press("detail-rework-JOB-001", t0 + Duration::from_millis(50));
        let ConfirmPressOutcome::Armed { epoch: e2 } = second_other else {
            panic!("expected different button to arm instead of confirming");
        };
        assert_ne!(e1, e2);
        assert!(!confirm.is_armed("detail-approve-JOB-001"));
        assert!(confirm.is_armed("detail-rework-JOB-001"));

        assert!(!confirm.disarm_if_epoch(e1, t0 + Duration::from_millis(60)));
        assert!(confirm.is_armed("detail-rework-JOB-001"));

        let confirmed = confirm.press("detail-rework-JOB-001", t0 + Duration::from_millis(120));
        assert_eq!(confirmed, ConfirmPressOutcome::Confirmed);
        assert!(!confirm.is_armed("detail-rework-JOB-001"));
    }

    #[test]
    fn transition_animates_smoothly_and_settles_with_zero_idle_redraws() {
        let st = AppState::new_ephemeral();
        let mut confirm = ConfirmState::new();
        let t0 = Instant::now();

        let _ = confirm.snapshot(&st, t0);
        let ConfirmPressOutcome::Armed { epoch } = confirm.press("detail-archive-JOB-001", t0)
        else {
            panic!("expected armed");
        };

        let mid = confirm.snapshot(&st, t0 + Duration::from_millis(CONFIRM_ARM_MS / 2));
        assert!(mid.animating);
        let mid_w = mid.weight("detail-archive-JOB-001");
        assert!(mid_w > 0.1 && mid_w < 0.99, "expected smooth mid weight, got {mid_w}");

        let settled = confirm.snapshot(&st, t0 + Duration::from_millis(CONFIRM_ARM_MS + 10));
        assert!(!settled.animating);
        assert!((settled.weight("detail-archive-JOB-001") - 1.0).abs() < 1e-4);

        assert!(confirm.disarm_if_epoch(epoch, t0 + Duration::from_millis(CONFIRM_AUTO_DISARM_MS)));
        let disarm_mid = confirm.snapshot(
            &st,
            t0 + Duration::from_millis(CONFIRM_AUTO_DISARM_MS + CONFIRM_DISARM_MS / 2),
        );
        assert!(disarm_mid.animating);
        let disarm_w = disarm_mid.weight("detail-archive-JOB-001");
        assert!(disarm_w > 0.01 && disarm_w < 0.9, "expected smooth disarm weight, got {disarm_w}");

        let done = confirm.snapshot(
            &st,
            t0 + Duration::from_millis(CONFIRM_AUTO_DISARM_MS + CONFIRM_DISARM_MS + 10),
        );
        assert!(!done.animating);
        assert_eq!(done.weight("detail-archive-JOB-001"), 0.0);
    }

    #[test]
    fn context_change_disarms_active_button() {
        let mut st = AppState::new_ephemeral();
        let mut confirm = ConfirmState::new();
        let t0 = Instant::now();

        let _ = confirm.snapshot(&st, t0);
        confirm.press("detail-approve-JOB-001", t0);
        assert!(confirm.is_armed("detail-approve-JOB-001"));

        st.active_tab = TabId::Guard;
        let after = confirm.snapshot(&st, t0 + Duration::from_millis(50));
        assert!(!confirm.is_armed("detail-approve-JOB-001"));
        assert!(after.animating);
    }
}
