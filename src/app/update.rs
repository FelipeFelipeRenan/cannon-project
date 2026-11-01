//! Self-update from the latest GitHub release.

use super::AppResult;

/// Replaces the running binary with the latest release, if there is one.
///
/// # Errors
///
/// Returns an error if the platform has no pre-built release or the download
/// or replacement fails.
pub fn update() -> AppResult<()> {
    let target = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("linux", "x86_64") => "linux-x64",
        ("linux", "aarch64") => "linux-arm64",
        ("macos", "x86_64") => "macos-x64",
        ("macos", "aarch64") => "macos-arm64",
        ("windows", "x86_64") => "windows-x64.exe",
        (os, arch) => {
            return Err(format!("no pre-built Cannon release for {os}/{arch}").into());
        }
    };

    let status = self_update::backends::github::Update::configure()
        .repo_owner("FelipeFelipeRenan")
        .repo_name("cannon-project")
        .bin_name("cannon")
        .target(target)
        .show_download_progress(true)
        .current_version(env!("CARGO_PKG_VERSION"))
        .build()?
        .update()?;

    if status.is_updated() {
        println!("✅ Successfully updated to version {}", status.version());
    } else {
        println!(
            "✨ You are already on the latest version: {}",
            status.version()
        );
    }

    Ok(())
}
