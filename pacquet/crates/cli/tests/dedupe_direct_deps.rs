//! `dedupeDirectDeps` workspace coverage for `pacquet install`.
//!
//! Ports pnpm's
//! [`installing/deps-installer/test/install/dedupeDirectDeps.ts`](https://github.com/pnpm/pnpm/blob/39101f5e37/installing/deps-installer/test/install/dedupeDirectDeps.ts):
//! when the workspace root provides the same `(alias → resolution)`
//! a non-root project depends on, the dep must not appear under
//! that project's `node_modules/`, and a project whose direct deps
//! are entirely deduped must not have a `node_modules/` created at
//! all. Setting `dedupeDirectDeps: false` must restore the
//! per-project symlinks.

pub mod _utils;

use assert_cmd::prelude::*;
use command_extra::CommandExtra;
use pacquet_testing_utils::{
    bin::{AddMockedRegistry, CommandTempCwd},
    fs::is_symlink_or_junction,
};
use std::{fs, path::Path, process::Command};

/// With `dedupeDirectDeps` left at its default (`true`), a sibling
/// project whose only direct dep is also a direct dep of the
/// workspace root must not get a `node_modules/` of its own.
#[test]
fn dedupes_direct_deps_against_workspace_root_by_default() {
    let CommandTempCwd { pacquet, root, workspace, npmrc_info, .. } =
        CommandTempCwd::init().add_mocked_registry();
    let AddMockedRegistry { mock_instance, .. } = npmrc_info;

    fs::write(
        workspace.join("package.json"),
        serde_json::json!({
            "name": "ws-root",
            "version": "0.0.0",
            "private": true,
            "dependencies": { "@pnpm.e2e/hello-world-js-bin": "1.0.0" },
        })
        .to_string(),
    )
    .expect("write root package.json");

    let workspace_yaml_path = workspace.join("pnpm-workspace.yaml");
    let mut workspace_yaml =
        fs::read_to_string(&workspace_yaml_path).expect("read pnpm-workspace.yaml");
    if !workspace_yaml.ends_with('\n') {
        workspace_yaml.push('\n');
    }
    workspace_yaml.push_str("packages:\n  - 'packages/*'\n");
    fs::write(&workspace_yaml_path, workspace_yaml).expect("write pnpm-workspace.yaml");

    fs::create_dir_all(workspace.join("packages/dup")).expect("mkdir packages/dup");
    fs::write(
        workspace.join("packages/dup/package.json"),
        serde_json::json!({
            "name": "@scope/dup",
            "version": "1.0.0",
            "dependencies": { "@pnpm.e2e/hello-world-js-bin": "1.0.0" },
        })
        .to_string(),
    )
    .expect("write packages/dup/package.json");

    pacquet.with_arg("install").assert().success();

    // Root still has the dep linked.
    let root_dep = workspace.join("node_modules/@pnpm.e2e/hello-world-js-bin");
    assert!(
        is_symlink_or_junction(&root_dep).expect("query root symlink"),
        "root node_modules direct-dep symlink missing",
    );

    // The deduped sibling has no node_modules at all — pnpm's
    // `linkDirectDepsAndDedupe` ends with `rimraf(project.modulesDir)`
    // when every dep was deduped. Pacquet achieves the same effect
    // by never creating the directory in the first place.
    assert!(
        !workspace.join("packages/dup/node_modules").exists(),
        "packages/dup/node_modules should not exist when every direct dep is deduped against root",
    );

    drop((root, mock_instance));
}

/// `dedupeDirectDeps: false` opts out — every sibling gets its
/// own per-project symlink even when the workspace root already
/// resolves the same alias to the same target.
#[test]
fn dedupe_direct_deps_disabled_keeps_per_project_symlinks() {
    let CommandTempCwd { pacquet, root, workspace, npmrc_info, .. } =
        CommandTempCwd::init().add_mocked_registry();
    let AddMockedRegistry { mock_instance, .. } = npmrc_info;

    fs::write(
        workspace.join("package.json"),
        serde_json::json!({
            "name": "ws-root",
            "version": "0.0.0",
            "private": true,
            "dependencies": { "@pnpm.e2e/hello-world-js-bin": "1.0.0" },
        })
        .to_string(),
    )
    .expect("write root package.json");

    let workspace_yaml_path = workspace.join("pnpm-workspace.yaml");
    let mut workspace_yaml =
        fs::read_to_string(&workspace_yaml_path).expect("read pnpm-workspace.yaml");
    if !workspace_yaml.ends_with('\n') {
        workspace_yaml.push('\n');
    }
    workspace_yaml.push_str("packages:\n  - 'packages/*'\ndedupeDirectDeps: false\n");
    fs::write(&workspace_yaml_path, workspace_yaml).expect("write pnpm-workspace.yaml");

    fs::create_dir_all(workspace.join("packages/dup")).expect("mkdir packages/dup");
    fs::write(
        workspace.join("packages/dup/package.json"),
        serde_json::json!({
            "name": "@scope/dup",
            "version": "1.0.0",
            "dependencies": { "@pnpm.e2e/hello-world-js-bin": "1.0.0" },
        })
        .to_string(),
    )
    .expect("write packages/dup/package.json");

    pacquet.with_arg("install").assert().success();

    let root_dep = workspace.join("node_modules/@pnpm.e2e/hello-world-js-bin");
    assert!(
        is_symlink_or_junction(&root_dep).expect("query root symlink"),
        "root node_modules direct-dep symlink missing",
    );
    let sibling_dep = workspace.join("packages/dup/node_modules/@pnpm.e2e/hello-world-js-bin");
    assert!(
        is_symlink_or_junction(&sibling_dep).expect("query sibling symlink"),
        "sibling direct-dep symlink should be kept when dedupeDirectDeps: false",
    );

    drop((root, mock_instance));
}

/// A frozen-lockfile install (the headless path) dedupes too.
/// Mirrors pnpm's second `mutateModules(... frozenLockfile: true)`
/// call in
/// [`dedupeDirectDeps.ts:107`](https://github.com/pnpm/pnpm/blob/39101f5e37/installing/deps-installer/test/install/dedupeDirectDeps.ts#L107)
/// which asserts the same on-disk shape after running through the
/// `install_frozen_lockfile` codepath.
#[test]
fn dedupes_direct_deps_with_frozen_lockfile() {
    let CommandTempCwd { pacquet, root, workspace, npmrc_info, .. } =
        CommandTempCwd::init().add_mocked_registry();
    let AddMockedRegistry { mock_instance, .. } = npmrc_info;

    fs::write(
        workspace.join("package.json"),
        serde_json::json!({
            "name": "ws-root",
            "version": "0.0.0",
            "private": true,
            "dependencies": { "@pnpm.e2e/hello-world-js-bin": "1.0.0" },
        })
        .to_string(),
    )
    .expect("write root package.json");

    let workspace_yaml_path = workspace.join("pnpm-workspace.yaml");
    let mut workspace_yaml =
        fs::read_to_string(&workspace_yaml_path).expect("read pnpm-workspace.yaml");
    if !workspace_yaml.ends_with('\n') {
        workspace_yaml.push('\n');
    }
    workspace_yaml.push_str("packages:\n  - 'packages/*'\n");
    fs::write(&workspace_yaml_path, workspace_yaml).expect("write pnpm-workspace.yaml");

    fs::create_dir_all(workspace.join("packages/dup")).expect("mkdir packages/dup");
    fs::write(
        workspace.join("packages/dup/package.json"),
        serde_json::json!({
            "name": "@scope/dup",
            "version": "1.0.0",
            "dependencies": { "@pnpm.e2e/hello-world-js-bin": "1.0.0" },
        })
        .to_string(),
    )
    .expect("write packages/dup/package.json");

    // First install seeds the lockfile and node_modules.
    pacquet.with_arg("install").assert().success();
    assert!(
        !workspace.join("packages/dup/node_modules").exists(),
        "first install should already have skipped packages/dup/node_modules creation",
    );

    // Tear down node_modules so the frozen-lockfile install is a
    // pure replay (pnpm's test does the same via `rimrafSync`).
    fs_remove_dir_all(&workspace.join("node_modules"));
    fs_remove_dir_all(&workspace.join("packages/dup/node_modules"));

    pacquet_at(&workspace).with_arg("install").with_arg("--frozen-lockfile").assert().success();

    assert!(
        is_symlink_or_junction(&workspace.join("node_modules/@pnpm.e2e/hello-world-js-bin"))
            .expect("query root symlink after frozen install"),
        "frozen-lockfile install should re-link the root's direct dep",
    );
    assert!(
        !workspace.join("packages/dup/node_modules").exists(),
        "frozen-lockfile install should keep packages/dup/node_modules absent",
    );

    drop((root, mock_instance));
}

fn fs_remove_dir_all(path: &Path) {
    match fs::remove_dir_all(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => panic!("remove {path:?}: {error}"),
    }
}

/// Build a fresh `pacquet` `Command` rooted at `workspace`. Used to
/// drive a second invocation in the same workspace because
/// [`assert_cmd::Command::assert`] consumes the wrapped command.
fn pacquet_at(workspace: &Path) -> Command {
    Command::cargo_bin("pacquet").expect("find the pacquet binary").with_current_dir(workspace)
}

/// Partial dedupe: a sibling with one shared dep and one unique
/// dep keeps the unique dep symlinked under its `node_modules/`
/// while the shared dep is omitted.
#[test]
fn dedupes_only_overlapping_direct_deps() {
    let CommandTempCwd { pacquet, root, workspace, npmrc_info, .. } =
        CommandTempCwd::init().add_mocked_registry();
    let AddMockedRegistry { mock_instance, .. } = npmrc_info;

    fs::write(
        workspace.join("package.json"),
        serde_json::json!({
            "name": "ws-root",
            "version": "0.0.0",
            "private": true,
            "dependencies": { "@pnpm.e2e/hello-world-js-bin": "1.0.0" },
        })
        .to_string(),
    )
    .expect("write root package.json");

    let workspace_yaml_path = workspace.join("pnpm-workspace.yaml");
    let mut workspace_yaml =
        fs::read_to_string(&workspace_yaml_path).expect("read pnpm-workspace.yaml");
    if !workspace_yaml.ends_with('\n') {
        workspace_yaml.push('\n');
    }
    workspace_yaml.push_str("packages:\n  - 'packages/*'\n");
    fs::write(&workspace_yaml_path, workspace_yaml).expect("write pnpm-workspace.yaml");

    fs::create_dir_all(workspace.join("packages/mixed")).expect("mkdir packages/mixed");
    fs::write(
        workspace.join("packages/mixed/package.json"),
        serde_json::json!({
            "name": "@scope/mixed",
            "version": "1.0.0",
            "dependencies": {
                "@pnpm.e2e/hello-world-js-bin": "1.0.0",
                "@pnpm.e2e/hello-world-js-bin-parent": "1.0.0",
            },
        })
        .to_string(),
    )
    .expect("write packages/mixed/package.json");

    pacquet.with_arg("install").assert().success();

    let mixed_modules = workspace.join("packages/mixed/node_modules");
    let shared = mixed_modules.join("@pnpm.e2e/hello-world-js-bin");
    assert!(
        !shared.exists(),
        "shared direct-dep should be deduped against root, but found {shared:?}",
    );
    let unique = mixed_modules.join("@pnpm.e2e/hello-world-js-bin-parent");
    assert!(
        is_symlink_or_junction(&unique).expect("query unique symlink"),
        "unique direct-dep symlink missing under packages/mixed/node_modules",
    );

    drop((root, mock_instance));
}

mod known_failures {
    //! Upstream `dedupeDirectDeps` cases blocked on pacquet's install
    //! pipeline ordering. Pacquet runs `SymlinkDirectDependencies`
    //! *before* the hoist pass, so the dedupe map only contains the
    //! root importer's *direct* deps — not the *publicly-hoisted*
    //! transitive deps that pnpm sees because hoist runs first there.
    //! Closing this gap is a separate refactor (running hoist before
    //! the symlink phase, or threading the hoist result into the
    //! dedupe pass) tracked here as a known failure.
    //!
    //! See the call-site note at
    //! `pacquet/crates/package-manager/src/install_frozen_lockfile.rs:953`
    //! ("Pacquet's pipeline order has `SymlinkDirectDependencies` running
    //! *before* hoist").
    use pacquet_testing_utils::{
        allow_known_failure,
        known_failure::{KnownFailure, KnownResult},
    };

    fn dedupe_against_hoisted_root_unsupported() -> KnownResult<()> {
        Err(KnownFailure::new(
            "Pacquet runs SymlinkDirectDependencies before the hoist pass, \
             so the dedupe map only sees root's direct deps — not its \
             publicly-hoisted transitives. Closing this requires running \
             hoist before the per-importer symlink step (or threading \
             the hoist result into the dedupe pass).",
        ))
    }

    /// Upstream: [`installing/deps-installer/test/install/dedupeDirectDeps.ts:113`](https://github.com/pnpm/pnpm/blob/39101f5e37/installing/deps-installer/test/install/dedupeDirectDeps.ts#L113)
    /// `'dedupe direct dependencies after public hoisting'`. A
    /// transitive of the root that gets publicly hoisted into root's
    /// `node_modules/` should dedupe a non-root importer's *direct*
    /// dep with the same alias.
    #[test]
    fn dedupes_direct_dep_against_publicly_hoisted_root_dep() {
        allow_known_failure!(dedupe_against_hoisted_root_unsupported());
    }

    /// Upstream: [`pnpm/test/install/hoist.ts:77`](https://github.com/pnpm/pnpm/blob/39101f5e37/pnpm/test/install/hoist.ts#L77)
    /// `'shamefully-hoist: applied to all the workspace projects when
    /// set to true in the root pnpm-workspace.yaml file (with
    /// dedupe-direct-deps=true)'`. Same pipeline-order gap: with
    /// `shamefullyHoist: true`, a non-root importer's direct dep that
    /// also lands in root via shameful-hoist should be deduped from
    /// the importer's `node_modules/`.
    #[test]
    fn dedupe_under_shamefully_hoist() {
        allow_known_failure!(dedupe_against_hoisted_root_unsupported());
    }
}
