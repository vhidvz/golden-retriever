//! Version, update and guided-download checks. Network access happens only when the user
//! asks for it. `upgrade` downloads a user-local artifact and verifies its SHA-256; it
//! never installs or overwrites anything, and it refuses system packages and checkouts.

use anyhow::{Context, Result, bail};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, copy};
use std::path::PathBuf;
use std::time::Duration;

pub const REPO: &str = "vhidvz/golden-retriever";
const GODOT_REPO: &str = "godotengine/godot";
const OLLAMA_REPO: &str = "ollama/ollama";
const USER_AGENT: &str = "buddy-update-check";

#[derive(Clone, Debug, PartialEq)]
pub struct Asset {
    pub name: String,
    pub url: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub tag: String,
    pub name: String,
    pub url: String,
    pub assets: Vec<Asset>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallMode {
    System,
    User,
    Checkout,
    Other,
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(timeout)).build().into()
}

pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// The git tag/commit recorded by the build script, or `unknown` outside a checkout.
pub fn build_description() -> &'static str {
    option_env!("BUDDY_GIT_DESCRIBE").unwrap_or("unknown")
}

pub fn format_version(version: &str, describe: &str) -> String {
    if describe.is_empty() || describe == "unknown" { format!("buddy {version}") } else { format!("buddy {version} ({describe})") }
}

pub fn version_text() -> String {
    format_version(current_version(), build_description())
}

fn tag_number(tag: &str) -> &str {
    tag.trim().trim_start_matches(['v', 'V'])
}

fn numbers(version: &str) -> Vec<u64> {
    version
        .split(['.', '-', '+'])
        .take(4)
        .map(|part| part.parse::<u64>().unwrap_or(0))
        .collect()
}

/// True when `latest` is a higher numeric release than `current`. Unparseable
/// versions are compared as unequal, and the caller can present both.
pub fn is_newer(current: &str, latest: &str) -> bool {
    let (a, b) = (numbers(tag_number(current)), numbers(tag_number(latest)));
    let len = a.len().max(b.len());
    for index in 0..len {
        let left = a.get(index).copied().unwrap_or(0);
        let right = b.get(index).copied().unwrap_or(0);
        if left != right {
            return right > left;
        }
    }
    false
}

pub fn parse_release(value: &Value) -> Result<Release> {
    let tag = value["tag_name"].as_str().context("the release has no tag")?.to_string();
    let assets = value["assets"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| Some(Asset { name: item["name"].as_str()?.to_string(), url: item["browser_download_url"].as_str()?.to_string() }))
                .collect()
        })
        .unwrap_or_default();
    Ok(Release {
        name: value["name"].as_str().unwrap_or(&tag).to_string(),
        url: value["html_url"].as_str().unwrap_or("https://github.com").to_string(),
        tag,
        assets,
    })
}

pub fn release_asset<'a>(release: &'a Release, suffix: &str) -> Option<&'a Asset> {
    release.assets.iter().find(|asset| asset.name.ends_with(suffix))
}

/// Reads one `sha256sum`-style line. A leading `*` (binary mode) is accepted.
pub fn checksum_from_manifest<'a>(manifest: &'a str, name: &str) -> Option<&'a str> {
    manifest.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let checksum = parts.next()?;
        let listed = parts.next()?.trim_start_matches('*');
        (listed == name).then_some(checksum)
    })
}

pub fn install_mode(exe: &std::path::Path) -> InstallMode {
    let path = exe.to_string_lossy();
    if path.starts_with("/usr/bin/") || path.starts_with("/usr/local/bin/") {
        InstallMode::System
    } else if path.contains("/.local/") {
        InstallMode::User
    } else if path.contains("/target/") {
        InstallMode::Checkout
    } else {
        InstallMode::Other
    }
}

pub fn latest(repo: &str) -> Result<Release> {
    let mut response = agent(Duration::from_secs(12))
        .get(&format!("https://api.github.com/repos/{repo}/releases/latest"))
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .call()
        .with_context(|| format!("could not ask GitHub about {repo}"))?;
    let value: Value = response.body_mut().read_json().context("GitHub sent a release this version does not understand")?;
    parse_release(&value)
}

pub fn repository() -> String {
    std::env::var("BUDDY_UPDATE_REPO").unwrap_or_else(|_| REPO.to_string())
}

fn cache_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".cache"));
    base.join("buddy").join("updates")
}

fn fetch_text(asset: &Asset) -> Result<String> {
    let mut response = agent(Duration::from_secs(20))
        .get(&asset.url)
        .header("User-Agent", USER_AGENT)
        .call()
        .with_context(|| format!("could not download {}", asset.name))?;
    response.body_mut().read_to_string().with_context(|| format!("could not read {}", asset.name))
}

fn download_asset(asset: &Asset, path: &std::path::Path) -> Result<()> {
    let mut response = agent(Duration::from_secs(120))
        .get(&asset.url)
        .header("User-Agent", USER_AGENT)
        .call()
        .with_context(|| format!("could not download {}", asset.name))?;
    let mut file = File::create(path).with_context(|| format!("could not create {}", path.display()))?;
    let mut reader = response.body_mut().as_reader();
    copy(&mut reader, &mut file).with_context(|| format!("could not save {}", path.display()))?;
    Ok(())
}

fn sha256_file(path: &std::path::Path) -> Result<String> {
    let mut file = File::open(path).with_context(|| format!("could not open {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).with_context(|| format!("could not read {}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Download and verify the latest user-local artifact. This never installs it and never
/// touches a system-package or checkout copy.
pub fn upgrade() -> Result<String> {
    let exe = std::env::current_exe().context("could not find the running buddy binary")?;
    let mode = install_mode(&exe);
    let repo = repository();
    let release_page = format!("https://github.com/{repo}/releases");
    match mode {
        InstallMode::System => {
            return Ok(format!(
                "Buddy {} is installed at {}, by a system package manager.\nNothing was downloaded or checked. Update it with your package manager; releases are at {}\n",
                version_text(),
                exe.display(),
                release_page
            ));
        }
        InstallMode::Checkout => {
            return Ok(format!("This is a development checkout.\nNothing was downloaded. Update it with git; releases are at {}\n", release_page));
        }
        InstallMode::Other => {
            return Ok(format!(
                "I cannot tell how this copy of Buddy was installed, so I did not download anything.\nReleases are at {}\n",
                release_page
            ));
        }
        InstallMode::User => {}
    }
    let release = latest(&repo)?;
    if !is_newer(current_version(), &release.tag) {
        return Ok(format!("Buddy {} is already the newest release I can see ({}).", version_text(), release.tag));
    }

    let asset = release_asset(&release, "linux-x86_64.tar.zst").with_context(|| format!("release {} has no portable Linux artifact", release.tag))?;
    let manifest = release_asset(&release, "SHA256SUMS").with_context(|| format!("release {} has no SHA256SUMS", release.tag))?;
    let checksums = fetch_text(manifest)?;
    let expected = checksum_from_manifest(&checksums, &asset.name).with_context(|| format!("{} is not listed in SHA256SUMS", asset.name))?;
    let dir = cache_dir();
    fs::create_dir_all(&dir).with_context(|| format!("could not create {}", dir.display()))?;
    let file_name =
        std::path::Path::new(&asset.name).file_name().with_context(|| format!("the release has an invalid asset name: {}", asset.name))?;
    let path = dir.join(file_name);
    download_asset(asset, &path)?;
    let actual = sha256_file(&path)?;
    if actual != expected {
        let _ = fs::remove_file(&path);
        bail!("the downloaded artifact did not match SHA256SUMS; it was deleted");
    }
    Ok(format!(
        "Downloaded {} and verified SHA-256 {}.\nNothing was installed.\n\nTo install this user-local copy, inspect the archive and then replace only Buddy's files:\n  tmp=$(mktemp -d)\n  tar -C \"$tmp\" -xf \"{}\"\n  install -d \"$HOME/.local/bin\" \"$HOME/.local/share/buddy\"\n  install -m 755 \"$tmp/usr/bin/buddy\" \"$tmp/usr/bin/buddy-ui\" \"$HOME/.local/bin/\"\n  for part in renderer perception tools ui; do rm -rf \"$HOME/.local/share/buddy/$part\"; cp -a \"$tmp/usr/share/buddy/$part\" \"$HOME/.local/share/buddy/\"; done\n  systemctl --user restart buddy.service 2>/dev/null || true\n  rm -rf \"$tmp\"\n",
        asset.name,
        actual,
        path.display()
    ))
}

pub fn check_buddy() -> Result<String> {
    let repo = repository();
    let current = current_version();
    let release = match latest(&repo) {
        Ok(release) => release,
        Err(error) if format!("{error:#}").contains("404") => {
            return Ok(format!("Buddy {current}\nNo published releases were found for {repo} yet."));
        }
        Err(error) => return Err(error),
    };
    let newer = is_newer(current, &release.tag);
    Ok(format!(
        "Buddy {current}\nlatest release: {} ({})\n{}\n{}",
        release.tag,
        release.name,
        release.url,
        if newer {
            "A newer release is available. From a user-local install, run `buddy upgrade`; otherwise download it from the release page. Nothing was installed."
        } else {
            "This is the newest release I can see."
        }
    ))
}

fn managed_godot() -> Option<String> {
    if let Some(path) = std::env::var_os("BUDDY_GODOT") {
        let name = std::path::Path::new(&path).file_name()?.to_string_lossy().into_owned();
        return name.split('_').find(|part| part.chars().next().is_some_and(|c| c.is_ascii_digit())).map(str::to_string);
    }
    let root = buddy_core::paths::data_dir().join("godot");
    let mut found = None;
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                found = Some(entry.file_name().to_string_lossy().into_owned());
            }
        }
    }
    found
}

fn managed_ollama() -> Option<String> {
    let manifest = buddy_core::paths::runtime_manifest_path();
    let text = std::fs::read_to_string(manifest).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    value["ollama_version"].as_str().map(str::to_string)
}

pub fn check_tools() -> Result<String> {
    let mut lines = vec![format!("Buddy {}", current_version())];
    let godot = managed_godot();
    let ollama = managed_ollama();
    match latest(GODOT_REPO) {
        Ok(release) => lines.push(format!(
            "Godot: {} | latest release: {} ({})",
            godot.as_deref().unwrap_or("not found"),
            release.tag,
            release.url
        )),
        Err(error) => lines.push(format!("Godot: {} | latest check failed: {error}", godot.as_deref().unwrap_or("not found"))),
    }
    match latest(OLLAMA_REPO) {
        Ok(release) => lines.push(format!(
            "Ollama: {} | latest release: {} ({})",
            ollama.as_deref().unwrap_or("not found"),
            release.tag,
            release.url
        )),
        Err(error) => lines.push(format!("Ollama: {} | latest check failed: {error}", ollama.as_deref().unwrap_or("not found"))),
    }
    lines.push("Models are checked with `buddy ai status`; no model is updated automatically.".to_string());
    Ok(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn release_tags_are_compared_numerically() {
        assert!(is_newer("0.1.0", "v0.2.0"));
        assert!(is_newer("0.1.9", "v0.2.0"));
        assert!(!is_newer("1.0.0", "v1.0.0"));
        assert!(!is_newer("1.2.0", "v1.1.9"));
        assert!(is_newer("1.2", "1.2.1"));
        assert!(!is_newer("1.2.1", "1.2"));
    }

    #[test]
    fn release_metadata_is_read_without_trusting_missing_fields() {
        let release = parse_release(&json!({
            "tag_name": "v0.2.0",
            "name": "Buddy 0.2.0",
            "html_url": "https://example.test/release"
        }))
        .unwrap();
        assert_eq!(release.tag, "v0.2.0");
        assert_eq!(release.name, "Buddy 0.2.0");
        assert!(parse_release(&json!({})).is_err());
    }
}

#[cfg(test)]
mod version_tests {
    use super::*;

    #[test]
    fn version_reports_a_commit_when_one_is_known() {
        assert_eq!(format_version("0.1.0", "v0.1.0-2-gabc123"), "buddy 0.1.0 (v0.1.0-2-gabc123)");
        assert_eq!(format_version("0.1.0", "unknown"), "buddy 0.1.0");
    }
}

#[cfg(test)]
mod upgrade_tests {
    use super::*;
    use serde_json::json;
    use std::path::Path;

    #[test]
    fn checksum_manifest_finds_the_named_artifact() {
        let manifest = "abc123  buddy-0.1.0-linux-x86_64.tar.zst\ndef456 *SHA256SUMS\n";
        assert_eq!(checksum_from_manifest(manifest, "buddy-0.1.0-linux-x86_64.tar.zst"), Some("abc123"));
        assert_eq!(checksum_from_manifest(manifest, "missing.tar.zst"), None);
    }

    #[test]
    fn system_and_checkout_installs_are_not_self_replaced() {
        assert_eq!(install_mode(Path::new("/usr/bin/buddy")), InstallMode::System);
        assert_eq!(install_mode(Path::new("/opt/buddy/bin/buddy")), InstallMode::Other);
        assert_eq!(install_mode(Path::new("/home/u/.local/bin/buddy")), InstallMode::User);
        assert_eq!(install_mode(Path::new("/home/u/golden-retriever/target/release/buddy")), InstallMode::Checkout);
    }

    #[test]
    fn update_asset_is_chosen_by_suffix() {
        let release = parse_release(&json!({
            "tag_name": "v0.2.0",
            "assets": [
                {"name": "buddy-0.2.0-linux-x86_64.tar.zst", "browser_download_url": "https://example.test/buddy.tar.zst"},
                {"name": "SHA256SUMS", "browser_download_url": "https://example.test/SHA256SUMS"}
            ]
        }))
        .unwrap();
        assert_eq!(release_asset(&release, "linux-x86_64.tar.zst").unwrap().name, "buddy-0.2.0-linux-x86_64.tar.zst");
        assert_eq!(release_asset(&release, "SHA256SUMS").unwrap().name, "SHA256SUMS");
        assert!(release_asset(&release, "windows.exe").is_none());
    }
}
