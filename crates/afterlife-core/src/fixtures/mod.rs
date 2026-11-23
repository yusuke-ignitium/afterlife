mod districts_core;
mod districts_hardening;
mod districts_ops;
mod templates_delivery;
mod templates_discovery;

use std::collections::HashMap;

use crate::{
    ids::{OwnerId, SubMachineTemplateId},
    model::{Actor, District, Job, SubMachineTemplate},
};

pub const LOCAL_OWNER_ID: &str = "local-owner";
pub(super) const SEED_TS: u64 = 1_763_600_000_000;

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
    let mut template_order = Vec::new();
    for tmpl in templates_discovery::templates()
        .into_iter()
        .chain(templates_delivery::templates())
    {
        template_order.push(tmpl.id.clone());
        templates.insert(tmpl.id.clone(), tmpl);
    }

    let mut districts = Vec::new();
    districts.extend(districts_core::districts(owner_id, &seed_actor));
    districts.extend(districts_ops::districts(owner_id, &seed_actor));
    districts.extend(districts_hardening::districts(owner_id, &seed_actor));

    WorkspaceSeed {
        templates,
        template_order,
        districts,
        jobs: Vec::new(),
        next_job_seq: 1,
        next_fencing_token: 1,
    }
}
