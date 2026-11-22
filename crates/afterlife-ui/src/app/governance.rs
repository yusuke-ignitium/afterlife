use afterlife_core::{EngineCommand, JobId, ProposalId, SubMachineTemplateId};

use super::state::{AppState, now_ms};

impl AppState {
    pub fn open_job_detail(&mut self, job_id: JobId) {
        self.selected_job_id = job_id;
        self.selected_proposal_id = None;
        self.proposal_error = None;
        self.detail_open = true;
    }

    pub fn open_proposal_detail(&mut self, proposal_id: ProposalId) {
        self.selected_proposal_id = Some(proposal_id);
        self.proposal_error = None;
        self.preview_artifact = None;
        self.detail_open = true;
    }

    pub fn close_detail(&mut self) {
        self.detail_open = false;
        self.selected_proposal_id = None;
        self.proposal_error = None;
        self.preview_artifact = None;
    }

    pub fn approve_proposal(&mut self, proposal_id: ProposalId) -> Result<(), String> {
        let cmd = if self
            .engine
            .philosophy_proposals
            .iter()
            .any(|p| p.id == proposal_id)
        {
            EngineCommand::ApprovePhilosophyProposal {
                proposal_id: proposal_id.clone(),
                at: now_ms(),
            }
        } else if self
            .engine
            .spec_proposals
            .iter()
            .any(|p| p.id == proposal_id)
        {
            EngineCommand::ApproveSpecProposal {
                proposal_id: proposal_id.clone(),
                at: now_ms(),
            }
        } else if self
            .engine
            .template_proposals
            .iter()
            .any(|p| p.id == proposal_id)
        {
            EngineCommand::ApproveTemplateProposal {
                proposal_id: proposal_id.clone(),
                at: now_ms(),
            }
        } else {
            let msg = format!("Proposal {proposal_id} was not found.");
            self.proposal_error = Some(msg.clone());
            return Err(msg);
        };
        match self.dispatch(cmd) {
            Ok(_) => {
                self.proposal_error = None;
                self.selected_proposal_id = None;
                self.detail_open = false;
                Ok(())
            }
            Err(err) => {
                self.proposal_error = Some(err.clone());
                Err(err)
            }
        }
    }

    pub fn sync_builtin_templates(
        &mut self,
        template_ids: Option<Vec<SubMachineTemplateId>>,
    ) -> Result<(), String> {
        self.dispatch(EngineCommand::SyncBuiltinTemplates {
            template_ids,
            at: now_ms(),
        })
        .map(|_| ())
    }

    pub fn stop_job(&mut self, job_id: JobId) -> Result<(), String> {
        self.dispatch(EngineCommand::StopJob {
            job_id,
            reason: None,
            expected_revision: None,
            at: now_ms(),
        })
        .map(|_| ())
    }

    pub fn resume_job(&mut self, job_id: JobId) -> Result<(), String> {
        self.dispatch(EngineCommand::ResumeJob {
            job_id,
            expected_revision: None,
            at: now_ms(),
        })
        .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use afterlife_core::DistrictId;

    use super::*;

    #[test]
    fn app_state_stop_and_resume_sync_job_state_without_losing_stage() {
        let mut st = AppState::new_ephemeral();
        let dist_id = DistrictId::from("DIST-001");
        let jid = st.engine_mut().queue_preset_job("UI Stop Target", Some(&dist_id));
        st.engine_mut()
            .assign_job_to_district(&jid, &dist_id)
            .unwrap();
        let loc_before = st.engine.find_job(&jid).unwrap().location.clone();

        st.stop_job(jid.clone()).unwrap();
        let stopped = st.engine.find_job(&jid).unwrap();
        assert!(stopped.stopped);
        assert_eq!(stopped.location, loc_before);

        st.resume_job(jid.clone()).unwrap();
        let resumed = st.engine.find_job(&jid).unwrap();
        assert!(!resumed.stopped);
        assert_eq!(resumed.location, loc_before);
    }
}
