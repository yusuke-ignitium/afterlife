use std::collections::{HashMap, HashSet};

use crate::{
    error::ValidationError,
    ids::{DistrictId, StageId, SubMachineTemplateId},
    model::{District, StageDef, StageKind, StepFieldDef, SubMachineTemplate},
};

fn validate_step_fields(scope: String, fields: &[StepFieldDef], errors: &mut Vec<ValidationError>) {
    let mut seen = HashSet::new();
    for field in fields {
        let key = field.key.trim();
        if key.is_empty() {
            errors.push(ValidationError::EmptyStepFieldKey {
                scope: scope.clone(),
            });
            continue;
        }
        if !seen.insert(key.to_string()) {
            errors.push(ValidationError::DuplicateStepFieldKey {
                scope: scope.clone(),
                key: key.to_string(),
            });
        }
    }
}

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

        validate_step_fields(
            format!("{district_id}/{}", stage.id),
            &stage.record_fields,
            &mut errors,
        );
    }

    errors
}

pub fn validate_template(tmpl: &SubMachineTemplate) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    let tid = &tmpl.id;
    if tmpl.stages.is_empty() {
        errors.push(ValidationError::EmptySubTemplate(tid.clone()));
        return errors;
    }
    let mut seen_sub_ids: HashSet<String> = HashSet::new();
    for sub in &tmpl.stages {
        let sid_trimmed = sub.id.as_str().trim();
        if sid_trimmed.is_empty() {
            errors.push(ValidationError::EmptySubStageField {
                template_id: tid.clone(),
                sub_stage_id: sub.id.clone(),
                field: "id".into(),
            });
        } else if !seen_sub_ids.insert(sid_trimmed.to_string()) {
            errors.push(ValidationError::DuplicateSubStageId {
                template_id: tid.clone(),
                sub_stage_id: sub.id.clone(),
            });
        }
        if sub.label.trim().is_empty() {
            errors.push(ValidationError::EmptySubStageField {
                template_id: tid.clone(),
                sub_stage_id: sub.id.clone(),
                field: "label".into(),
            });
        }
        if sub.runner.trim().is_empty() {
            errors.push(ValidationError::EmptySubStageField {
                template_id: tid.clone(),
                sub_stage_id: sub.id.clone(),
                field: "runner".into(),
            });
        }
        if sub.procedure.trim().is_empty() {
            errors.push(ValidationError::EmptySubStageField {
                template_id: tid.clone(),
                sub_stage_id: sub.id.clone(),
                field: "procedure".into(),
            });
        }
        validate_step_fields(
            format!("{tid}/{}", sub.id),
            &sub.record_fields,
            &mut errors,
        );
    }
    errors
}

pub fn validate_workspace(
    districts: &[District],
    templates: &HashMap<SubMachineTemplateId, SubMachineTemplate>,
) -> Result<(), Vec<ValidationError>> {
    let mut errors = Vec::new();

    for tmpl in templates.values() {
        errors.extend(validate_template(tmpl));
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
