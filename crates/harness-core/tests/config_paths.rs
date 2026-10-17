#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

use harness_core::{APP_SLUG, user_config_path, workspace_config_path};

#[test]
fn a_workspace_config_lives_under_dot_banana_harness() {
    assert_eq!(
        workspace_config_path(Path::new("/work/repo")),
        Path::new("/work/repo/.banana-harness/config.toml")
    );
}

#[test]
fn the_user_config_lives_under_the_app_slug_in_the_config_directory() {
    let path = user_config_path().expect("a home directory");
    assert!(
        path.ends_with(Path::new(APP_SLUG).join("config.toml")),
        "{path:?}"
    );
}
