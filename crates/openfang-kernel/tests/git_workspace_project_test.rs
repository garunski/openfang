//! TASK-49: git tools require spoke paths under registered project spokes.

use openfang_kernel::OpenFangKernel;
use openfang_types::config::{DefaultModelConfig, KernelConfig};
use openfang_types::project::{Project, SpokeDescriptor};
use std::path::PathBuf;

fn test_kernel_config(tmp: &tempfile::TempDir) -> KernelConfig {
    let mut c = KernelConfig {
        home_dir: tmp.path().to_path_buf(),
        data_dir: tmp.path().join("data"),
        default_model: DefaultModelConfig {
            provider: "ollama".to_string(),
            model: "test-model".to_string(),
            api_key_env: "OLLAMA_API_KEY".to_string(),
            base_url: None,
        },
        ..Default::default()
    };
    c.automation.spoke_roots = vec![tmp.path().canonicalize().unwrap()];
    c
}

#[test]
fn resolve_git_workspace_accepts_spoke_rejects_other_under_allowlist() {
    let tmp = tempfile::tempdir().unwrap();
    let kernel = OpenFangKernel::boot_with_config(test_kernel_config(&tmp)).expect("boot");
    let proj_root = tmp.path().join("proj");
    let spoke = proj_root.join("code");
    std::fs::create_dir_all(&spoke).unwrap();
    std::fs::create_dir_all(tmp.path().join("other")).unwrap();
    let id = kernel
        .project_store
        .register(Project {
            name: "p".into(),
            path: proj_root.clone(),
            spokes: vec![SpokeDescriptor {
                name: "code".into(),
                path: PathBuf::from("code"),
                labels: vec![],
            }],
            ..Default::default()
        })
        .unwrap();

    let ok_path = spoke.canonicalize().unwrap();
    kernel
        .resolve_git_workspace_for_project(&id.to_string(), ok_path.to_str().unwrap())
        .expect("spoke path ok");

    let bad = tmp.path().join("other").canonicalize().unwrap();
    let err = kernel
        .resolve_git_workspace_for_project(&id.to_string(), bad.to_str().unwrap())
        .unwrap_err();
    assert!(err.contains("spoke") || err.contains("not under"), "{err}");

    kernel.shutdown();
}
