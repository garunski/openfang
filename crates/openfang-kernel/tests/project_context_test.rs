//! Per-project context store (TASK-48).

use openfang_kernel::OpenFangKernel;
use openfang_types::config::{DefaultModelConfig, KernelConfig};
use openfang_types::project::Project;

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

    let inp = kernel.workflow_input_with_project_context(id, "TASK-99");
    assert!(inp.contains("TASK-99"));
    assert!(inp.contains("crates/foo"));

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
