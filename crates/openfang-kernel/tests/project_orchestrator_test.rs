//! Per-project conduit-coordinator orchestrator (TASK-47).

use openfang_kernel::OpenFangKernel;
use openfang_types::agent::AgentId;
use openfang_types::config::{DefaultModelConfig, KernelConfig};
use openfang_types::project::Project;
use std::str::FromStr;

fn test_kernel_config(tmp: &tempfile::TempDir) -> KernelConfig {
    KernelConfig {
        home_dir: tmp.path().to_path_buf(),
        data_dir: tmp.path().join("data"),
        default_model: DefaultModelConfig {
            provider: "ollama".to_string(),
            model: "test-model".to_string(),
            api_key_env: "OLLAMA_API_KEY".to_string(),
            base_url: None,
        },
        ..Default::default()
    }
}

#[test]
fn two_projects_get_distinct_orchestrator_agents() {
    let tmp = tempfile::tempdir().unwrap();
    let kernel = OpenFangKernel::boot_with_config(test_kernel_config(&tmp)).expect("boot");

    let r1 = tmp.path().join("p1");
    let r2 = tmp.path().join("p2");
    std::fs::create_dir_all(&r1).unwrap();
    std::fs::create_dir_all(&r2).unwrap();

    let p1 = Project {
        name: "alpha".into(),
        path: r1,
        mattermost_channel_id: Some("mm-ch-alpha".into()),
        ..Default::default()
    };
    let id1 = kernel.project_store.register(p1).unwrap();
    let g1 = kernel.project_store.get(id1).unwrap();
    kernel
        .sync_project_mattermost_orchestrator(None, &g1)
        .expect("sync p1");

    let p2 = Project {
        name: "beta".into(),
        path: r2,
        mattermost_channel_id: Some("mm-ch-beta".into()),
        ..Default::default()
    };
    let id2 = kernel.project_store.register(p2).unwrap();
    let g2 = kernel.project_store.get(id2).unwrap();
    kernel
        .sync_project_mattermost_orchestrator(None, &g2)
        .expect("sync p2");

    let u1 = kernel.project_store.get(id1).unwrap();
    let u2 = kernel.project_store.get(id2).unwrap();
    let s1 = u1.orchestrator_agent_id.as_ref().expect("orch1");
    let s2 = u2.orchestrator_agent_id.as_ref().expect("orch2");
    assert_ne!(s1, s2);

    let a1 = AgentId::from_str(s1).unwrap();
    let a2 = AgentId::from_str(s2).unwrap();
    assert!(kernel.registry.get(a1).is_some());
    assert!(kernel.registry.get(a2).is_some());

    assert_eq!(
        kernel.hand_registry.list_instances().len(),
        2,
        "two hand instances"
    );

    kernel.shutdown();
}

#[test]
fn orchestrator_spawn_uses_orchestrator_default_model_when_set() {
    let tmp = tempfile::tempdir().unwrap();
    let mut cfg = test_kernel_config(&tmp);
    cfg.orchestrator_default_model = DefaultModelConfig {
        provider: "ollama".to_string(),
        model: "test-model".to_string(),
        api_key_env: "OLLAMA_API_KEY".to_string(),
        base_url: None,
    };
    let kernel = OpenFangKernel::boot_with_config(cfg).expect("boot");

    let r = tmp.path().join("p-orch-model");
    std::fs::create_dir_all(&r).unwrap();
    let p = Project {
        name: "orch-model".into(),
        path: r,
        mattermost_channel_id: Some("mm-ch-orch-model".into()),
        ..Default::default()
    };
    let id = kernel.project_store.register(p).unwrap();
    let g = kernel.project_store.get(id).unwrap();
    kernel
        .sync_project_mattermost_orchestrator(None, &g)
        .expect("sync");

    let updated = kernel.project_store.get(id).unwrap();
    let oid = updated
        .orchestrator_agent_id
        .as_ref()
        .expect("orchestrator");
    let aid = AgentId::from_str(oid).unwrap();
    let entry = kernel.registry.get(aid).expect("agent entry");
    assert_eq!(entry.manifest.model.provider, "ollama");
    assert_eq!(entry.manifest.model.model, "test-model");

    kernel.shutdown();
}

#[test]
fn clearing_mattermost_channel_deactivates_orchestrator() {
    let tmp = tempfile::tempdir().unwrap();
    let kernel = OpenFangKernel::boot_with_config(test_kernel_config(&tmp)).expect("boot");
    let r = tmp.path().join("p");
    std::fs::create_dir_all(&r).unwrap();

    let p = Project {
        name: "solo".into(),
        path: r,
        mattermost_channel_id: Some("mm-x".into()),
        ..Default::default()
    };
    let id = kernel.project_store.register(p).unwrap();
    let g = kernel.project_store.get(id).unwrap();
    kernel
        .sync_project_mattermost_orchestrator(None, &g)
        .expect("sync");
    let with_orch = kernel.project_store.get(id).unwrap();
    assert!(with_orch.orchestrator_agent_id.is_some());

    let cleared = Project {
        mattermost_channel_id: None,
        ..with_orch.clone()
    };
    kernel
        .sync_project_mattermost_orchestrator(Some(&with_orch), &cleared)
        .expect("sync clear");

    let after = kernel.project_store.get(id).unwrap();
    assert!(after.orchestrator_agent_id.is_none());
    assert!(
        kernel.hand_registry.list_instances().is_empty(),
        "hand instance removed"
    );

    kernel.shutdown();
}
