use std::collections::{HashMap, HashSet};

use crate::{
    error::ValidationError,
    ids::{DistrictId, StageId, SubMachineTemplateId},
    model::{District, StageDef, StageKind, SubMachineTemplate},
};

pub fn validate_stages(
    district_id: &DistrictId,
    stages: &[StageDef],
    templates: &HashMap<SubMachineTemplateId, SubMachineTemplate>,
) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    if stages.is_empty() {
        errors.push(ValidationError::EmptyDistrict(district_id.clone()));
        return errors;
    }

    let mut seen_stages: HashSet<&StageId> = HashSet::new();
    for stage in stages {
        if !seen_stages.insert(&stage.id) {
            errors.push(ValidationError::DuplicateStageId {
                district_id: district_id.clone(),
                stage_id: stage.id.clone(),
            });
        }
    }

    for stage in stages {
        if stage.kind == StageKind::Terminal && !stage.next.is_empty() {
            errors.push(ValidationError::TerminalStageHasOutgoing {
                district_id: district_id.clone(),
                stage_id: stage.id.clone(),
            });
        }

        for next_id in &stage.next {
            if !seen_stages.contains(next_id) {
                errors.push(ValidationError::DanglingNextStage {
                    district_id: district_id.clone(),
                    from: stage.id.clone(),
                    to: next_id.clone(),
                });
            }
        }

        if let Some(rework_id) = &stage.rework_to {
            if !seen_stages.contains(rework_id) {
                errors.push(ValidationError::DanglingReworkStage {
                    district_id: district_id.clone(),
                    from: stage.id.clone(),
                    to: rework_id.clone(),
                });
            }
        }

        if let Some(tid) = &stage.sub_template_id {
            if !stage.supports_sub_machine() {
                errors.push(ValidationError::SubTemplateOnNonWorkStage {
                    district_id: district_id.clone(),
                    stage_id: stage.id.clone(),
                });
            }
            if !templates.contains_key(tid) {
                errors.push(ValidationError::DanglingSubTemplate {
                    district_id: district_id.clone(),
                    stage_id: stage.id.clone(),
                    template_id: tid.clone(),
                });
            }
        }
    }

    errors
}

pub fn validate_workspace(
    districts: &[District],
    templates: &HashMap<SubMachineTemplateId, SubMachineTemplate>,
) -> Result<(), Vec<ValidationError>> {
    let mut errors = Vec::new();

    for (tid, tmpl) in templates {
        if tmpl.stages.is_empty() {
            errors.push(ValidationError::EmptySubTemplate(tid.clone()));
        }
    }

    for district in districts {
        if district.philosophy_history.is_empty() {
            errors.push(ValidationError::MissingPhilosophy(district.id.clone()));
        }
        if district.spec_history.is_empty() {
            errors.push(ValidationError::MissingProcedureSpec(district.id.clone()));
            continue;
        }
        errors.extend(validate_stages(&district.id, district.stages(), templates));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
