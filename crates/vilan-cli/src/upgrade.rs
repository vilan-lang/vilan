//! `vilan upgrade` — update this binary (and `vilan-lsp` beside it) to the
//! newest release (proposal/releases.md §6).
//!
//! The CLI never touches the network except here, on explicit request. The
//! work is delegated to the same tools the install script already requires
//! (`curl`, `tar`), so upgrading works exactly where installing worked and the
//! binary carries no HTTP/TLS machinery. The checksum is the one deliberate
//! exception: it is computed in-process, because Windows ships neither
//! `sha256sum` nor `shasum` — one code path on every platform, and the "no
//! sha256 tool found" failure class is gone (windows-support.md §8, §10e).
//!
//! Release assets are versionless (`vilan-<target>.tar.gz`, or `.zip` on the
//! Windows targets), so the newest version is discovered without an API
//! round-trip: `releases/latest` redirects to `releases/tag/v<version>`, and
//! the assets are then fetched from that tag's own download path (pinned — a
//! release published mid-run can't mix versions).
//!
//! The swap is atomic per binary — the replacement is staged *inside* the
//! install directory and renamed into place, same filesystem — but the two
//! platforms reach that rename differently:
//!
//! - **unix** renames straight over the old file; a running executable keeps
//!   its inode, so the upgrading process is unharmed.
//! - **Windows** forbids renaming over (or deleting) a running executable, but
//!   it does permit renaming it *aside* — so the old file moves to
//!   `vilan.exe.old` first, and the leftovers are swept at the start of the
//!   next upgrade run. Exactly the dance releases.md §6 recorded.
//!
//! Test seams (undocumented, for the integration tests): `$VILAN_UPGRADE_BASE`
//! replaces the repository base URL (a `file://` tree works — `curl` speaks
//! it), and `$VILAN_UPGRADE_LATEST` skips the redirect discovery.

use std::path::Path;
use std::process::{Command, ExitCode};

use sha2::{Digest, Sha256};

use crate::paint::{self, Style};

/// The platform's discard sink for `curl -o` (`/dev/null` has no Windows
/// equivalent path, but `NUL` is the same idea).
const NULL_DEVICE: &str = if cfg!(windows) { "NUL" } else { "/dev/null" };

const DEFAULT_BASE: &str = "https://github.com/ReedSyllas/vilan";

pub fn upgrade(check_only: bool) -> ExitCode {
    match run(check_only) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{} {message}", paint::error_prefix());
            ExitCode::FAILURE
        }
    }
}

fn run(check_only: bool) -> Result<(), String> {
    let current = parse_version(env!("CARGO_PKG_VERSION"))
        .ok_or_else(|| "this binary's own version is unparseable".to_string())?;
    let base = std::env::var("VILAN_UPGRADE_BASE").unwrap_or_else(|_| DEFAULT_BASE.to_string());

    let latest_label = discover_latest(&base)?;
    let latest = parse_version(&latest_label)
        .ok_or_else(|| format!("cannot parse the latest release version from `{latest_label}`"))?;

    if latest <= current {
        println!(
            "{}",
            paint::out(
                Style::GREEN,
                &format!("vilan {} is the newest release.", env!("CARGO_PKG_VERSION"))
            )
        );
        return Ok(());
    }
    if check_only {
        println!(
            "{}",
            paint::out(
                Style::CYAN,
                &format!(
                    "vilan {} → {latest_label} available — run `vilan upgrade`.",
                    env!("CARGO_PKG_VERSION")
                )
            )
        );
        return Ok(());
    }

    let executable = std::env::current_exe()
        .map_err(|error| format!("cannot locate the running binary: {error}"))?;
    let install_dir = executable
        .parent()
        .ok_or_else(|| "the running binary has no parent directory".to_string())?
        .to_path_buf();

    // A previous Windows upgrade renamed the then-running executable aside;
    // that process is gone now, so its leftover is finally deletable.
    sweep_aside_leftovers(&install_dir);

    let asset = asset_name(env!("VILAN_TARGET"));
    let download_base = format!("{base}/releases/download/v{latest_label}");
    let workdir = std::env::temp_dir().join(format!("vilan-upgrade-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&workdir);
    std::fs::create_dir_all(&workdir)
        .map_err(|error| format!("cannot create {}: {error}", workdir.display()))?;
    let result = download_verify_swap(
        &download_base,
        &asset,
        &workdir,
        &install_dir,
        &latest_label,
    );
    let _ = std::fs::remove_dir_all(&workdir);
    result
}

fn download_verify_swap(
    download_base: &str,
    asset: &str,
    workdir: &Path,
    install_dir: &Path,
    latest_label: &str,
) -> Result<(), String> {
    println!(
        "{}",
        paint::out(
            Style::DIM,
            &format!(
                "vilan {} → v{latest_label} — downloading {asset} ...",
                env!("CARGO_PKG_VERSION")
            )
        )
    );
    fetch(&format!("{download_base}/{asset}"), &workdir.join(asset))?;
    fetch(
        &format!("{download_base}/sha256sums.txt"),
        &workdir.join("sha256sums.txt"),
    )?;
    verify_checksum(workdir, asset)?;

    // `-xf`, not `-xzf`: both tars in play detect the compression themselves
    // (GNU tar since 1.15, bsdtar always), and bsdtar — the `tar` Windows
    // ships — additionally reads the `.zip` asset, which `-z` would reject.
    // One command for every platform and both archive kinds.
    let status = Command::new("tar")
        .args(["-xf", asset])
        .current_dir(workdir)
        .status()
        .map_err(|error| format!("cannot run tar: {error}"))?;
    if !status.success() {
        return Err(format!("unpacking {asset} failed"));
    }

    // Sanity before touching anything: the downloaded binary must execute.
    let unpacked = workdir.join(format!("vilan{}", std::env::consts::EXE_SUFFIX));
    let version_probe = Command::new(&unpacked)
        .arg("--version")
        .output()
        .map_err(|error| format!("the downloaded vilan does not execute: {error}"))?;
    if !version_probe.status.success() {
        return Err("the downloaded vilan does not report a version".to_string());
    }

    install_binaries(
        workdir,
        install_dir,
        std::env::consts::EXE_SUFFIX,
        cfg!(windows),
    )?;

    let installed = String::from_utf8_lossy(&version_probe.stdout)
        .trim()
        .to_string();
    println!(
        "{}",
        paint::out(
            Style::GREEN,
            &format!("installed {installed} to {}", install_dir.display())
        )
    );

    // Housekeeping while we own ~/.vilan: drop std-cache entries no current
    // binary can use (each build materializes under its own content hash and
    // nothing deletes the old ones). The week-long age guard keeps any entry
    // a running binary might still be reading.
    let pruned = vilan_embedded_std::prune_stale(
        &vilan_embedded_std::default_cache_root(),
        std::time::Duration::from_secs(7 * 24 * 60 * 60),
    );
    if pruned > 0 {
        println!(
            "{}",
            paint::out(
                Style::DIM,
                &format!(
                    "pruned {pruned} stale std cache entr{}",
                    if pruned == 1 { "y" } else { "ies" }
                )
            )
        );
    }
    Ok(())
}

/// The release asset for `target`. The Windows targets ship a `.zip` (the
/// platform's convention, and what `install.ps1` consumes); everyone else a
/// gzipped tarball.
fn asset_name(target: &str) -> String {
    let extension = if target.contains("windows-msvc") {
        "zip"
    } else {
        "tar.gz"
    };
    format!("vilan-{target}.{extension}")
}

/// Move the freshly unpacked binaries from `workdir` into `install_dir`.
///
/// `rename_aside` selects the Windows strategy — move the old file to
/// `<name>.old` before renaming the replacement in, because it may be the
/// running executable — over unix's rename-over. It is a parameter rather than
/// a `cfg!` so that both strategies, and the difference between them, are
/// exercisable from either platform's tests.
fn install_binaries(
    workdir: &Path,
    install_dir: &Path,
    executable_suffix: &str,
    rename_aside: bool,
) -> Result<(), String> {
    // `vilan-lsp` first so the pair is never newer-cli/older-lsp.
    for binary in ["vilan-lsp", "vilan"] {
        let name = format!("{binary}{executable_suffix}");
        let destination = install_dir.join(&name);
        let staged = install_dir.join(format!(".{name}.upgrade-{}", std::process::id()));
        std::fs::copy(workdir.join(&name), &staged)
            .map_err(|error| format!("cannot stage into {}: {error}", install_dir.display()))?;
        if rename_aside && destination.exists() {
            let aside = install_dir.join(format!("{name}.old"));
            let _ = std::fs::remove_file(&aside);
            std::fs::rename(&destination, &aside).map_err(|error| {
                let _ = std::fs::remove_file(&staged);
                format!("cannot move {} aside: {error}", destination.display())
            })?;
        }
        std::fs::rename(&staged, &destination).map_err(|error| {
            let _ = std::fs::remove_file(&staged);
            format!("cannot replace {}: {error}", destination.display())
        })?;
    }
    Ok(())
}

/// Does `file_name` name an executable a previous upgrade renamed aside?
/// Selection only, no filesystem, so both platforms' spellings are pinnable
/// from either.
fn is_aside_leftover(file_name: &str, executable_suffix: &str) -> bool {
    ["vilan", "vilan-lsp"]
        .iter()
        .any(|binary| file_name == format!("{binary}{executable_suffix}.old"))
}

/// Best-effort removal of the executables a previous upgrade renamed aside.
/// One still locked by a running process simply waits for the run after this
/// one; nothing here is allowed to fail an upgrade.
fn sweep_aside_leftovers(install_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(install_dir) else {
        return;
    };
    for entry in entries.flatten() {
        if is_aside_leftover(
            &entry.file_name().to_string_lossy(),
            std::env::consts::EXE_SUFFIX,
        ) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// The newest release's version label (no `v`), from `$VILAN_UPGRADE_LATEST`
/// or the `releases/latest` redirect.
fn discover_latest(base: &str) -> Result<String, String> {
    if let Ok(forced) = std::env::var("VILAN_UPGRADE_LATEST") {
        return Ok(forced);
    }
    let output = Command::new("curl")
        .args([
            "-fsSLI",
            "-o",
            NULL_DEVICE,
            "-w",
            "%{url_effective}",
            &format!("{base}/releases/latest"),
        ])
        .output()
        .map_err(|error| format!("cannot run curl: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "cannot reach {base}/releases/latest — check your connection (or see {base}/releases)"
        ));
    }
    let final_url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    version_from_tag_url(&final_url)
        .ok_or_else(|| format!("`{final_url}` does not name a release tag"))
}

fn fetch(url: &str, to: &Path) -> Result<(), String> {
    let status = Command::new("curl")
        .args(["-fsSL", "-o"])
        .arg(to)
        .arg(url)
        .status()
        .map_err(|error| format!("cannot run curl: {error}"))?;
    if !status.success() {
        return Err(format!("download failed: {url}"));
    }
    Ok(())
}

/// Verify `asset` against the release's `sha256sums.txt` (both already in
/// `workdir`). The hash is computed in-process on every platform — see the
/// module doc.
fn verify_checksum(workdir: &Path, asset: &str) -> Result<(), String> {
    let sums = std::fs::read_to_string(workdir.join("sha256sums.txt"))
        .map_err(|error| format!("cannot read sha256sums.txt: {error}"))?;
    let expected = recorded_checksum(&sums, asset)
        .ok_or_else(|| format!("sha256sums.txt has no entry for {asset}"))?;
    let actual = sha256_file(&workdir.join(asset))?;
    if !actual.eq_ignore_ascii_case(expected) {
        return Err(format!("checksum mismatch for {asset} — aborting"));
    }
    Ok(())
}

/// The hash `sha256sums.txt` records for `asset`. The format is `sha256sum`'s
/// own: the hash, two spaces (or ` *` in binary mode), the file name.
fn recorded_checksum<'a>(sums: &'a str, asset: &str) -> Option<&'a str> {
    sums.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let hash = fields.next()?;
        let name = fields.next()?.trim_start_matches('*');
        (name == asset && fields.next().is_none()).then_some(hash)
    })
}

/// The SHA-256 of a file, lowercase hex — streamed, so a release tarball is
/// never held in memory whole.
fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

/// `".../releases/tag/v0.3.0"` → `"0.3.0"`.
fn version_from_tag_url(url: &str) -> Option<String> {
    let (_, tag) = url.rsplit_once("/tag/")?;
    let tag = tag.strip_prefix('v').unwrap_or(tag);
    if parse_version(tag).is_some() {
        Some(tag.to_string())
    } else {
        None
    }
}

/// `"0.2.0"` → `(0, 2, 0)`; a missing patch or minor reads as zero.
fn parse_version(label: &str) -> Option<(u64, u64, u64)> {
    let mut parts = label.trim().split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().map_or(Some(0), |part| part.parse().ok())?;
    let patch = parts.next().map_or(Some(0), |part| part.parse().ok())?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

#[cfg(test)]
mod tests {
    use super::{
        asset_name, install_binaries, is_aside_leftover, parse_version, recorded_checksum,
        sha256_file, sweep_aside_leftovers, verify_checksum, version_from_tag_url,
    };
    use std::fs;
    use std::path::PathBuf;

    /// A scratch directory of this test's own, removed on the next run.
    fn scratch(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("vilan-upgrade-unit-{name}"));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create the scratch directory");
        path
    }

    #[test]
    fn versions_parse_and_order() {
        assert_eq!(parse_version("0.2.0"), Some((0, 2, 0)));
        assert_eq!(parse_version("1.10"), Some((1, 10, 0)));
        assert_eq!(parse_version("2"), Some((2, 0, 0)));
        assert_eq!(
            parse_version("v0.2.0"),
            None,
            "the v prefix is the tag's, not the version's"
        );
        assert_eq!(parse_version("0.2.0.1"), None);
        assert_eq!(parse_version("not-a-version"), None);
        assert!(parse_version("0.3.0") > parse_version("0.2.9"));
        assert!(
            parse_version("0.10.0") > parse_version("0.9.9"),
            "numeric, not lexicographic"
        );
        assert!(parse_version("1.0.0") > parse_version("0.99.99"));
    }

    #[test]
    fn the_release_tag_comes_from_the_redirect_url() {
        assert_eq!(
            version_from_tag_url("https://github.com/ReedSyllas/vilan/releases/tag/v0.3.0"),
            Some("0.3.0".to_string())
        );
        // No tag in the URL (e.g. no releases yet → /releases) or garbage: None.
        assert_eq!(
            version_from_tag_url("https://github.com/ReedSyllas/vilan/releases"),
            None
        );
        assert_eq!(
            version_from_tag_url("https://github.com/x/y/releases/tag/nightly"),
            None
        );
    }

    #[test]
    fn the_asset_extension_follows_the_target() {
        assert_eq!(
            asset_name("x86_64-pc-windows-msvc"),
            "vilan-x86_64-pc-windows-msvc.zip"
        );
        assert_eq!(
            asset_name("aarch64-pc-windows-msvc"),
            "vilan-aarch64-pc-windows-msvc.zip"
        );
        for target in [
            "x86_64-unknown-linux-musl",
            "aarch64-unknown-linux-musl",
            "x86_64-apple-darwin",
            "aarch64-apple-darwin",
        ] {
            assert_eq!(asset_name(target), format!("vilan-{target}.tar.gz"));
        }
        // This binary's own target must name an asset the release workflow
        // actually publishes.
        let own = asset_name(env!("VILAN_TARGET"));
        assert!(
            own.ends_with(".zip") == cfg!(windows),
            "the running platform asks for the wrong archive kind: {own}"
        );
    }

    #[test]
    fn the_recorded_checksum_is_found_by_exact_file_name() {
        let sums = "\
1111111111111111111111111111111111111111111111111111111111111111  install.sh
2222222222222222222222222222222222222222222222222222222222222222 *vilan-x86_64-pc-windows-msvc.zip
3333333333333333333333333333333333333333333333333333333333333333  vilan-x86_64-unknown-linux-musl.tar.gz
";
        assert_eq!(
            recorded_checksum(sums, "install.sh"),
            Some("1111111111111111111111111111111111111111111111111111111111111111")
        );
        // Binary-mode (`*`) entries are the same entries.
        assert_eq!(
            recorded_checksum(sums, "vilan-x86_64-pc-windows-msvc.zip"),
            Some("2222222222222222222222222222222222222222222222222222222222222222")
        );
        assert_eq!(
            recorded_checksum(sums, "vilan-x86_64-unknown-linux-musl.tar.gz"),
            Some("3333333333333333333333333333333333333333333333333333333333333333")
        );
        assert_eq!(
            recorded_checksum(sums, "vilan-aarch64-apple-darwin.tar.gz"),
            None
        );
        // A suffix of a recorded name is not that name (the old shell-out
        // matched with a trailing-space grep, which this must not regress to).
        assert_eq!(recorded_checksum(sums, "musl.tar.gz"), None);
        assert_eq!(recorded_checksum(sums, "sh"), None);
        assert_eq!(recorded_checksum("", "anything"), None);
    }

    #[test]
    fn the_hash_matches_the_published_sha256_vectors() {
        let directory = scratch("hash");
        // FIPS 180-2 vectors: the empty input and "abc".
        for (content, expected) in [
            (
                "",
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ),
            (
                "abc",
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            ),
        ] {
            let file = directory.join("payload");
            fs::write(&file, content).expect("write the payload");
            assert_eq!(sha256_file(&file).expect("hash the payload"), expected);
        }
        assert!(
            sha256_file(&directory.join("absent")).is_err(),
            "a missing file is an error, not a hash"
        );
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn verification_accepts_the_real_hash_and_rejects_a_flipped_one() {
        let directory = scratch("verify");
        fs::write(directory.join("asset.tar.gz"), "the release payload").expect("write the asset");
        let real = sha256_file(&directory.join("asset.tar.gz")).expect("hash the asset");
        fs::write(
            directory.join("sha256sums.txt"),
            format!("{real}  asset.tar.gz\ndeadbeef  other.zip\n"),
        )
        .expect("write the sums");
        assert!(verify_checksum(&directory, "asset.tar.gz").is_ok());
        // Uppercase hex is the same hash (`Get-FileHash` writes it that way).
        fs::write(
            directory.join("sha256sums.txt"),
            format!("{}  asset.tar.gz\n", real.to_uppercase()),
        )
        .expect("write the sums");
        assert!(verify_checksum(&directory, "asset.tar.gz").is_ok());

        let flipped: String = real
            .chars()
            .enumerate()
            .map(|(index, digit)| {
                if index > 0 {
                    digit
                } else if digit == '0' {
                    'f'
                } else {
                    '0'
                }
            })
            .collect();
        fs::write(
            directory.join("sha256sums.txt"),
            format!("{flipped}  asset.tar.gz\n"),
        )
        .expect("write the sums");
        let error = verify_checksum(&directory, "asset.tar.gz").expect_err("a flipped hash fails");
        assert!(error.contains("checksum mismatch"), "{error}");
        // An asset with no entry at all is its own, distinct failure.
        let error = verify_checksum(&directory, "absent.zip").expect_err("no entry fails");
        assert!(error.contains("no entry for absent.zip"), "{error}");
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn only_the_renamed_aside_executables_are_leftovers() {
        for (suffix, aside) in [("", "vilan.old"), (".exe", "vilan.exe.old")] {
            assert!(is_aside_leftover(aside, suffix));
            assert!(is_aside_leftover(&format!("vilan-lsp{suffix}.old"), suffix));
            // Not the live binaries, not a staged copy, not a user's file.
            assert!(!is_aside_leftover(&format!("vilan{suffix}"), suffix));
            assert!(!is_aside_leftover(".vilan.upgrade-1234", suffix));
            assert!(!is_aside_leftover("notes.old", suffix));
            assert!(!is_aside_leftover("vilan.old.old", suffix));
        }
        // The suffixes do not cross: an `.exe.old` is not a leftover on unix.
        assert!(!is_aside_leftover("vilan.exe.old", ""));
        assert!(!is_aside_leftover("vilan.old", ".exe"));
    }

    #[test]
    fn the_sweep_removes_the_leftovers_and_nothing_else() {
        let directory = scratch("sweep");
        let suffix = std::env::consts::EXE_SUFFIX;
        let leftovers = [
            format!("vilan{suffix}.old"),
            format!("vilan-lsp{suffix}.old"),
        ];
        let keepers = [
            format!("vilan{suffix}"),
            format!("vilan-lsp{suffix}"),
            "notes.old".to_string(),
        ];
        for name in leftovers.iter().chain(keepers.iter()) {
            fs::write(directory.join(name), name).expect("seed the install directory");
        }
        sweep_aside_leftovers(&directory);
        for name in &leftovers {
            assert!(!directory.join(name).exists(), "{name} survived the sweep");
        }
        for name in &keepers {
            assert!(directory.join(name).exists(), "{name} was swept away");
        }
        // A directory that does not exist is not an error.
        sweep_aside_leftovers(&directory.join("absent"));
        let _ = fs::remove_dir_all(&directory);
    }

    /// The swap dance, both strategies, from this platform — and the assertion
    /// that they differ, so neither arm can quietly become the other.
    #[test]
    fn the_swap_renames_aside_only_when_asked() {
        for (rename_aside, name) in [(true, "aside"), (false, "over")] {
            let root = scratch(&format!("swap-{name}"));
            let workdir = root.join("work");
            let install = root.join("bin");
            fs::create_dir_all(&workdir).expect("create the work directory");
            fs::create_dir_all(&install).expect("create the install directory");
            for binary in ["vilan", "vilan-lsp"] {
                fs::write(
                    workdir.join(format!("{binary}.exe")),
                    format!("new {binary}"),
                )
                .expect("stage the replacement");
                fs::write(
                    install.join(format!("{binary}.exe")),
                    format!("old {binary}"),
                )
                .expect("seed the installed binary");
            }

            install_binaries(&workdir, &install, ".exe", rename_aside).expect("swap");

            for binary in ["vilan", "vilan-lsp"] {
                assert_eq!(
                    fs::read_to_string(install.join(format!("{binary}.exe"))).expect("read"),
                    format!("new {binary}"),
                    "the replacement is in place either way"
                );
                let aside = install.join(format!("{binary}.exe.old"));
                if rename_aside {
                    assert_eq!(
                        fs::read_to_string(&aside).expect("the old binary was moved aside"),
                        format!("old {binary}"),
                        "the old file survives under .old, deletable next run"
                    );
                } else {
                    assert!(!aside.exists(), "unix renames straight over — no .old");
                }
            }
            // Nothing staged is left behind on either path.
            let strays: Vec<_> = fs::read_dir(&install)
                .expect("list")
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().to_string())
                .filter(|entry| entry.contains(".upgrade-"))
                .collect();
            assert!(strays.is_empty(), "staging leftovers: {strays:?}");
            let _ = fs::remove_dir_all(&root);
        }
    }

    /// A first install (nothing to move aside) works under the Windows
    /// strategy too — the `.old` step is conditional on there being an old.
    #[test]
    fn the_aside_swap_handles_an_empty_install_directory() {
        let root = scratch("swap-first");
        let workdir = root.join("work");
        let install = root.join("bin");
        fs::create_dir_all(&workdir).expect("create the work directory");
        fs::create_dir_all(&install).expect("create the install directory");
        for binary in ["vilan", "vilan-lsp"] {
            fs::write(workdir.join(binary), format!("new {binary}")).expect("stage");
        }
        install_binaries(&workdir, &install, "", true).expect("swap");
        for binary in ["vilan", "vilan-lsp"] {
            assert_eq!(
                fs::read_to_string(install.join(binary)).expect("read"),
                format!("new {binary}")
            );
            assert!(!install.join(format!("{binary}.old")).exists());
        }
        let _ = fs::remove_dir_all(&root);
    }
}
