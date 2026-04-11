//! Per-project context store (TASK-48).

use openfang_kernel::OpenFangKernel;
use openfang_types::config::{DefaultModelConfig, KernelConfig};
use openfang_types::project::{Project, SpokeDescriptor};
use std::path::PathBuf;

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
fn project_context_file_roundtrip_and_workflow_input() {
    let tmp = tempfile::tempdir().unwrap();
    let kernel = OpenFangKernel::boot_with_config(test_kernel_config(&tmp)).expect("boot");
    let root = tmp.path().join("proj");
    std::fs::create_dir_all(&root).unwrap();
    let id = kernel
        .project_store
        .register(Project {
            name: "ctxproj".into(),
            path: root,
            ..Default::default()
        })
        .unwrap();

    let ctx_path = tmp
        .path()
        .join("projects")
        .join(id.to_string())
        .join("context.json");
    assert!(!ctx_path.exists());

    kernel
        .update_project_context_tool(
            &id.to_string(),
            &serde_json::json!({
                "set_repo_structure_summary": "crates/foo, crates/bar",
                "append_decision": { "summary": "prefer anyhow" },
                "append_past_failure": { "task_id": "TASK-1", "stderr_snippet": "clippy -D warnings" }
            }),
        )
        .expect("update");

    assert!(ctx_path.exists());
    let raw = std::fs::read_to_string(&ctx_path).unwrap();
    assert!(raw.contains("anyhow"));
    assert!(raw.contains("clippy"));

    let inp = kernel.conduit_input_with_project_context(id, "TASK-99", None);
    assert!(inp.contains("TASK-99"));
    assert!(inp.contains("crates/foo"));
    assert!(inp.contains("[OpenFang conduit binding]"));
    assert!(inp.contains("[OpenFang resolved paths"));
    assert!(inp.contains(&format!("project_id={id}")));
    assert!(inp.contains("trigger_task_id=TASK-99"));

    kernel.shutdown();
}

#[test]
fn conduit_input_embeds_trigger_task_plain_when_provided() {
    let tmp = tempfile::tempdir().unwrap();
    let kernel = OpenFangKernel::boot_with_config(test_kernel_config(&tmp)).expect("boot");
    let root = tmp.path().join("proj2");
    std::fs::create_dir_all(&root).unwrap();
    let id = kernel
        .project_store
        .register(Project {
            name: "snap".into(),
            path: root,
            ..Default::default()
        })
        .unwrap();
    let body = "Status: Ready for Dev\n\nDoc: backlog/docs/overview/doc-1.md";
    let inp = kernel.conduit_input_with_project_context(id, "TASK-7", Some(body));
    assert!(inp.contains("[OpenFang trigger task snapshot"));
    assert!(inp.contains("TASK-7"));
    assert!(inp.contains("doc-1.md"));
    kernel.shutdown();
}

#[test]
fn read_project_context_rejects_unknown_project() {
    let tmp = tempfile::tempdir().unwrap();
    let kernel = OpenFangKernel::boot_with_config(test_kernel_config(&tmp)).expect("boot");
    let err = kernel
        .read_project_context_tool("00000000-0000-0000-0000-000000000001")
        .unwrap_err();
    assert!(err.contains("not found"));
    kernel.shutdown();
}

#[test]
fn read_project_context_includes_orchestrator_hints() {
    let tmp = tempfile::tempdir().unwrap();
    let kernel = OpenFangKernel::boot_with_config(test_kernel_config(&tmp)).expect("boot");
    let root = tmp.path().join("proj");
    let spoke_a = root.join("spoke-a");
    std::fs::create_dir_all(&spoke_a).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    let id = kernel
        .project_store
        .register(Project {
            name: "hintsproj".into(),
            path: root.clone(),
            spokes: vec![SpokeDescriptor {
                name: "custom".into(),
                path: PathBuf::from("spoke-a"),
                labels: vec!["repo:custom".into()],
            }],
            admin_spoke: Some("custom".into()),
            ..Default::default()
        })
        .unwrap();

    let json = kernel.read_project_context_tool(&id.to_string()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let hints = v.get("orchestrator_hints").expect("orchestrator_hints");
    assert_eq!(hints["project_name"], "hintsproj");
    assert_eq!(hints["admin_spoke"], "custom");
    let spokes = hints["configured_spokes"].as_array().unwrap();
    assert_eq!(spokes.len(), 1);
    assert_eq!(spokes[0]["name"], "custom");
    let resolved = spokes[0]["path_resolved"].as_str().unwrap();
    let expected = spoke_a.canonicalize().unwrap();
    assert_eq!(resolved, expected.display().to_string());
    let abr = hints["admin_backlog_root"].as_str().unwrap();
    assert!(abr.ends_with("backlog"), "{abr}");
    kernel.shutdown();
}
