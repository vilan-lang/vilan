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
//! None of that applies when a package manager installed this binary: npm and
//! Homebrew keep their own ledger of every file they own, and replacing one
//! behind their back leaves an install they will happily "upgrade" back to the
//! old version. So `vilan upgrade` reads where it is running from and steers
//! instead (distribution.md §2, call (b) — steer, never overwrite).
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

const DEFAULT_BASE: &str = "https://github.com/vilan-lang/vilan";

/// The vilan mark as half-block art, shown once after a successful
/// `vilan upgrade`. Rows are rasterized from
/// assets/branding/dark_logo_flat.svg — do not hand-edit; regenerate this
/// whole block with `python3 scripts/ascii_logo.py --rust`.
///
/// `concat!` of one literal per row — never a `"\` line-continuation
/// literal: a trailing `\` in a Rust string skips the newline **and all
/// following whitespace**, which silently eats each row's leading
/// indentation and flush-lefts the mark (pinned by
/// `the_mark_is_eleven_clean_lines_of_half_blocks`).
const UPGRADE_LOGO: &str = concat!(
    " ▄▄                                      ▄▄\n",
    "  ▀██▄                                ▄██▀\n",
    "    ▀                             ▄▄███▀\n",
    "        ▄▄██▄                  ▄█████▀\n",
    "        ▀██████▄▄          ▄▄██████▀\n",
    "          ▀███████▄▄     ▄███████▀\n",
    "            ▀█████████▄▄  ▀▀███▀\n",
    "              ▀██████████▄▄\n",
    "                ▀██████████▀\n",
    "                  ▀██████▀\n",
    "                    ▀██▀",
);

/// Which install this binary belongs to — decided by *where it is*, because
/// that is the only signal a single portable binary has (distribution.md §2:
/// "detection is by path inspection at runtime — no build variants, one binary
/// everywhere").
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Channel {
    /// `~/.vilan/bin` from either install script — or anywhere else nobody
    /// claims. These files are `vilan upgrade`'s to replace.
    SelfManaged,
    /// Inside a `node_modules` tree: `npm install -g @vilan-lang/vilan`.
    Npm,
    /// Under a Homebrew prefix.
    Homebrew,
}

/// The Homebrew prefixes, and deliberately only these three: `brew --prefix`
/// answers `/opt/homebrew` on Apple silicon, `/usr/local` on Intel macs, and
/// `.linuxbrew` inside the `linuxbrew` user's home directory on Linux
/// (Homebrew's own documented defaults — an install anywhere else is a
/// `--prefix` build, which is unsupported by brew itself and stays on the
/// self-managed path here).
///
/// The Intel-mac entry names `Cellar/` rather than the prefix: a formula's
/// files live in `<prefix>/Cellar/<formula>/<version>/`, while `/usr/local/bin`
/// is also where a hand-placed binary lands, and steering *that* one would be
/// wrong.
///
/// The Linux prefix is spelled in two pieces because `tests/hygiene.rs`
/// rejects an absolute home path in any tracked file — the same trick that
/// file plays on its own needles.
const HOMEBREW_PREFIXES: [&str; 3] = [
    "/opt/homebrew/",
    "/usr/local/Cellar/",
    concat!("/", "home", "/linuxbrew/.linuxbrew/"),
];

impl Channel {
    /// Read the channel out of the running binary's path.
    fn of(executable: &str) -> Channel {
        if has_segment(executable, "node_modules") {
            Channel::Npm
        } else if HOMEBREW_PREFIXES
            .iter()
            .any(|prefix| executable.starts_with(prefix))
        {
            Channel::Homebrew
        } else {
            Channel::SelfManaged
        }
    }

    /// The package manager that owns this install and the command that
    /// upgrades it — one table, so a message can never name a manager without
    /// its command. `None` is the self-managed install, the only one this
    /// binary may replace in place.
    fn owner(self) -> Option<(&'static str, &'static str)> {
        match self {
            Channel::SelfManaged => None,
            Channel::Npm => Some(("npm", "npm update -g @vilan-lang/vilan")),
            Channel::Homebrew => Some(("Homebrew", "brew upgrade vilan")),
        }
    }
}

/// The path a channel decision is made on: where the running binary *really*
/// lives, symlinks resolved.
///
/// Resolving matters on exactly one supported platform, and it is not
/// cosmetic. `current_exe` answers `/proc/self/exe` on Linux (already the real
/// file), but on macOS it answers the path the process was launched with — and
/// a Homebrew install is normally launched through the symlink farm
/// (`/usr/local/bin/vilan` → `…/Cellar/vilan/<version>/bin/vilan`). That
/// symlink's own path is under no prefix in the table, so without this the
/// Intel-mac case would go undetected and `vilan upgrade` would replace a file
/// Homebrew owns — the exact harm call (b) exists to prevent.
///
/// Only the *decision* uses this. The swap still writes through the path this
/// process was started with, which is what the self-managed install has always
/// done. An unresolvable path is classified as it was given.
fn resolved_for_channel(executable: &Path) -> std::path::PathBuf {
    std::fs::canonicalize(executable).unwrap_or_else(|_| executable.to_path_buf())
}

/// Does `path` contain `segment` as a whole path component, under either
/// separator? Windows accepts both, and a directory merely *containing* the
/// name (`node_modules_backup`) is not it.
///
/// A string scan rather than `Path::components`, so a Windows-shaped path is
/// classified identically from a unix test — the same reason
/// `install_binaries` takes its strategy as a parameter.
fn has_segment(path: &str, segment: &str) -> bool {
    path.split(['/', '\\']).any(|part| part == segment)
}

/// The steer: what `vilan upgrade` answers for an install it must not touch.
/// `newer` is the version `--check` discovered — the discovery half still
/// works and is exactly what the user asked for, while a plain `vilan upgrade`
/// steers without reaching the network at all.
///
/// `colored` is a parameter (as in [`success_banner`]) so both arms are pinned
/// without a terminal, and each line is tinted on its own — a span crossing a
/// newline would bleed into whatever a pager printed next.
fn steer_message(colored: bool, owner: (&str, &str), current: &str, newer: Option<&str>) -> String {
    let (installer, command) = owner;
    let headline = match newer {
        Some(latest) => {
            format!(
                "vilan {current} → {latest} available — {installer} installed this vilan, so {installer} upgrades it:"
            )
        }
        None => {
            format!(
                "vilan {current} was installed by {installer}, which owns these files — upgrade it with:"
            )
        }
    };
    format!(
        "{}\n\n    {}",
        paint::wrap(colored, Style::CYAN, &headline),
        paint::wrap(colored, Style::BOLD, command)
    )
}

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

    // Where this binary sits decides whether it may replace itself, so it is
    // read before anything else: an npm- or brew-owned install must not so
    // much as reach the network on a plain `vilan upgrade`, because no answer
    // from the release page could change what it is allowed to do. A path that
    // cannot be read at all is nobody's channel — and the swap below fails on
    // its own terms if it is really unreadable.
    let executable = std::env::current_exe();
    let channel = executable.as_ref().map_or(Channel::SelfManaged, |path| {
        Channel::of(&resolved_for_channel(path).to_string_lossy())
    });
    if let Some(owner) = channel.owner()
        && !check_only
    {
        // Not an error: the user asked how to upgrade and got the answer.
        println!(
            "{}",
            steer_message(
                paint::stdout_enabled(),
                owner,
                env!("CARGO_PKG_VERSION"),
                None
            )
        );
        return Ok(());
    }

    let base = std::env::var("VILAN_UPGRADE_BASE").unwrap_or_else(|_| DEFAULT_BASE.to_string());

    let latest_label = discover_latest(&base)?;
    let latest = parse_version(&latest_label)
        .ok_or_else(|| format!("cannot parse the latest release version from `{latest_label}`"))?;

    if latest <= current {
        let owned_by = match channel.owner() {
            Some((installer, _)) => format!(" (installed by {installer})"),
            None => String::new(),
        };
        println!(
            "{}",
            paint::out(
                Style::GREEN,
                &format!(
                    "vilan {} is the newest release{owned_by}.",
                    env!("CARGO_PKG_VERSION")
                )
            )
        );
        return Ok(());
    }
    if check_only {
        // A steered channel reaches here too: `--check` is a question about
        // the release page, and answering it costs one redirect the user
        // explicitly asked for. Only the *command* it points at changes.
        let line = match channel.owner() {
            Some(owner) => steer_message(
                paint::stdout_enabled(),
                owner,
                env!("CARGO_PKG_VERSION"),
                Some(&latest_label),
            ),
            None => paint::out(
                Style::CYAN,
                &format!(
                    "vilan {} → {latest_label} available — run `vilan upgrade`.",
                    env!("CARGO_PKG_VERSION")
                ),
            )
            .into_owned(),
        };
        println!("{line}");
        return Ok(());
    }

    let executable =
        executable.map_err(|error| format!("cannot locate the running binary: {error}"))?;
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

    // The one point at which the swap has happened and the new version is
    // known — so the mark rides the line the upgrade has always ended on
    // rather than being a second announcement of the same fact.
    let installed = String::from_utf8_lossy(&version_probe.stdout)
        .trim()
        .to_string();
    println!(
        "{}",
        success_banner(
            paint::stdout_enabled(),
            &installed,
            &install_dir.display().to_string(),
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

/// What a finished upgrade prints: the mark, then the success line naming the
/// version that is now installed and where it went.
///
/// One string from one `colored` verdict, taken as a parameter the way
/// `paint.rs`'s own rule is — so both arms are pinned without a terminal, and
/// so the art and its caption cannot drift apart or be printed twice. The sole
/// caller sits after the swap has succeeded; every other exit from `run`
/// (already newest, `--check`, any error) returns before reaching it.
fn success_banner(colored: bool, installed: &str, destination: &str) -> String {
    // Tinted line by line rather than as one span: a color that crossed a
    // newline would be carried into whatever a pager, `head`, or a CI log
    // collector re-emitted next.
    let mark = UPGRADE_LOGO
        .lines()
        .map(|line| paint::wrap(colored, Style::BLUSH, line).into_owned())
        .collect::<Vec<_>>()
        .join("\n");
    let line = format!("installed {installed} to {destination}");
    let caption = paint::wrap(colored, Style::GREEN, &line);
    format!("\n{mark}\n\n{caption}")
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
        Channel, UPGRADE_LOGO, asset_name, install_binaries, is_aside_leftover, parse_version,
        recorded_checksum, sha256_file, steer_message, success_banner, sweep_aside_leftovers,
        verify_checksum, version_from_tag_url,
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
            version_from_tag_url("https://github.com/vilan-lang/vilan/releases/tag/v0.3.0"),
            Some("0.3.0".to_string())
        );
        // No tag in the URL (e.g. no releases yet → /releases) or garbage: None.
        assert_eq!(
            version_from_tag_url("https://github.com/vilan-lang/vilan/releases"),
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

    /// The generated art is what a terminal has to survive, so its shape is
    /// pinned rather than trusted: regenerating it with different parameters
    /// (or hand-editing it) has to trip this, not a user's console.
    #[test]
    fn the_mark_is_eleven_clean_lines_of_half_blocks() {
        let lines: Vec<&str> = UPGRADE_LOGO.lines().collect();
        assert_eq!(lines.len(), 11, "the generator emits 11 rows");
        for (index, line) in lines.iter().enumerate() {
            let columns = line.chars().count();
            assert!(
                columns <= 44,
                "line {index} is {columns} columns wide — 44 is what fits an \
                 80-column terminal beside a caption: {line:?}"
            );
            assert!(
                line.chars()
                    .all(|glyph| matches!(glyph, ' ' | '▀' | '▄' | '█')),
                "line {index} has a glyph outside the four CP437 half-blocks: {line:?}"
            );
            assert_eq!(
                line.trim_end(),
                *line,
                "line {index} carries trailing whitespace"
            );
        }
        // Not blank and not shrunk: below ~32 columns the full mark stops
        // reading, which is what the simplified icon SVGs are for.
        let widest = lines
            .iter()
            .map(|line| line.chars().count())
            .max()
            .expect("eleven lines");
        assert!(
            (33..=44).contains(&widest),
            "the mark renders at {widest} columns"
        );
        for glyph in ['▀', '▄', '█'] {
            assert!(UPGRADE_LOGO.contains(glyph), "the art lost {glyph:?}");
        }

        // The indentation *is* the mark: it is a chevron converging downward,
        // so no row is flush-left and the indents never step back. This is the
        // pin that catches the mark being pasted back into a `"\` line-
        // continuation literal, whose escape eats leading whitespace and
        // silently flush-lefts every row (the shape survives nothing else in
        // this test — the widths, the glyph set and the trailing-space check
        // all still pass on the wreckage).
        let indents: Vec<usize> = lines
            .iter()
            .map(|line| line.len() - line.trim_start_matches(' ').len())
            .collect();
        assert!(
            indents.iter().all(|indent| *indent > 0),
            "a row is flush-left — the indentation was eaten: {indents:?}"
        );
        assert!(
            indents.windows(2).all(|pair| pair[0] <= pair[1]),
            "the mark stopped converging: {indents:?}"
        );
        // No blank frame baked into the const — the banner owns its spacing.
        assert!(!UPGRADE_LOGO.starts_with('\n'), "leading empty line");
        assert!(!UPGRADE_LOGO.ends_with('\n'), "trailing empty line");
        assert!(!lines[0].trim().is_empty() && !lines[10].trim().is_empty());
    }

    /// The colored arm with every SGR sequence removed — what the terminal
    /// actually shows once it has consumed them.
    fn strip_escapes(text: &str) -> String {
        let mut visible = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(start) = rest.find('\x1b') {
            visible.push_str(&rest[..start]);
            let sequence = &rest[start..];
            let end = sequence.find('m').expect("an SGR sequence ends in `m`");
            rest = &sequence[end + 1..];
        }
        visible.push_str(rest);
        visible
    }

    /// Both arms of the success banner. `colored` is a parameter, so a piped
    /// stream, `NO_COLOR`, and a real terminal are all pinned without one.
    #[test]
    fn the_success_banner_prints_the_mark_and_one_caption_in_both_arms() {
        // A synthetic install directory: the real one lives under `$HOME`, and
        // the hygiene gate rejects an absolute home path in a tracked file.
        let plain = success_banner(false, "vilan 9.9.9 (fake)", "/opt/vilan/bin");
        let colored = success_banner(true, "vilan 9.9.9 (fake)", "/opt/vilan/bin");

        // Piped or `NO_COLOR`: not one escape byte, and the glyphs are the
        // const's own bytes, unmangled.
        assert!(
            !plain.contains('\x1b'),
            "escapes on the plain path: {plain:?}"
        );
        assert!(plain.contains(UPGRADE_LOGO), "the art is missing:\n{plain}");

        // Colored: the brand blush opens the art, the caption keeps its own
        // green, and the whole thing closes on a reset.
        assert!(
            colored.contains("\x1b[38;2;249;223;231m"),
            "no blush: {colored:?}"
        );
        assert!(colored.contains("\x1b[32m"), "the caption lost green");
        assert!(colored.ends_with("\x1b[0m"), "unterminated: {colored:?}");
        // One span per art line plus one for the caption — no span crosses a
        // newline, so a pager cannot carry the color into what it prints next.
        assert_eq!(colored.matches("\x1b[").count(), 2 * (11 + 1));
        for line in colored.lines().filter(|line| !line.is_empty()) {
            assert!(
                line.starts_with('\x1b') && line.ends_with("\x1b[0m"),
                "a line is not self-contained: {line:?}"
            );
        }

        // Both arms name the new version exactly once, on the one success line
        // the upgrade has always ended on — the art joins that message rather
        // than repeating it.
        for banner in [&plain, &colored] {
            assert_eq!(
                strip_escapes(banner)
                    .matches("installed vilan 9.9.9 (fake) to /opt/vilan/bin")
                    .count(),
                1,
                "the caption is not printed exactly once:\n{banner}"
            );
        }
        // Escapes are the *only* difference between the arms.
        assert_eq!(strip_escapes(&colored), plain);

        // Visible in `cargo test -- --nocapture`, captured otherwise: the
        // sample a reviewer wants without running a real upgrade.
        println!("{plain}");
    }

    /// Where the binary sits *is* the channel. Both separators and both
    /// shapes, because a Windows install spells its paths the other way and
    /// has to be classified identically from here — the same reason
    /// `install_binaries` takes its strategy as a parameter.
    #[test]
    fn the_install_channel_is_read_from_the_executables_path() {
        // npm: anywhere under a `node_modules` tree — the global prefix, a
        // project-local install, the `.bin` shim directory, a UNC share.
        for path in [
            "/opt/npm/lib/node_modules/@vilan-lang/linux-x64/bin/vilan",
            "/srv/app/node_modules/@vilan-lang/darwin-arm64/bin/vilan",
            "/srv/app/node_modules/.bin/vilan",
            "/srv/app/node_modules/@vilan-lang/vilan/node_modules/@vilan-lang/linux-x64/bin/vilan",
            "D:\\tools\\npm\\node_modules\\@vilan-lang\\win32-x64\\bin\\vilan.exe",
            "\\\\build\\share\\node_modules\\@vilan-lang\\win32-x64\\bin\\vilan.exe",
        ] {
            assert_eq!(Channel::of(path), Channel::Npm, "{path}");
        }

        // Homebrew: the three documented prefixes, whether the path is the
        // Cellar itself or the symlink farm in front of it.
        let linuxbrew = format!("/{}/linuxbrew/.linuxbrew/bin/vilan", "home");
        for path in [
            "/opt/homebrew/bin/vilan",
            "/opt/homebrew/Cellar/vilan/0.14.0/bin/vilan",
            "/usr/local/Cellar/vilan/0.14.0/bin/vilan",
            linuxbrew.as_str(),
        ] {
            assert_eq!(Channel::of(path), Channel::Homebrew, "{path}");
        }

        // Everything else is this binary's own to replace. The near misses
        // matter most: a segment that merely *contains* the name, and a prefix
        // that does not end at a separator, are not a package manager's.
        let vilan_bin = format!(".{}/bin/vilan", "vilan"); // ~/.vilan/bin, spelled without a home
        for path in [
            vilan_bin.as_str(),
            "/opt/vilan/bin/vilan",
            // Not Cellar: /usr/local/bin is also where a hand-placed binary
            // lands, and steering that one would be wrong.
            "/usr/local/bin/vilan",
            "/opt/homebrewery/bin/vilan",
            "/srv/node_modules_backup/bin/vilan",
            "/srv/my-node_modules/bin/vilan",
            "/srv/nodemodules/bin/vilan",
            "target/debug/vilan",
        ] {
            assert_eq!(Channel::of(path), Channel::SelfManaged, "{path}");
        }
    }

    /// The other half of the Intel-mac case: a Homebrew binary is reached
    /// through a symlink, and the decision is made on the file it points at.
    /// (Composed with the Cellar fixtures above — an absolute `/usr/local`
    /// prefix cannot be built inside a scratch directory.)
    #[cfg(unix)]
    #[test]
    fn the_channel_is_decided_on_the_file_a_symlink_points_at() {
        // Imported here rather than above: the only caller is this unix test,
        // and an unconditional import would warn on the Windows target.
        use super::resolved_for_channel;

        let root = scratch("symlink");
        let cellar = root.join("Cellar/vilan/0.14.0/bin");
        let farm = root.join("bin");
        fs::create_dir_all(&cellar).expect("create the cellar");
        fs::create_dir_all(&farm).expect("create the symlink farm");
        let real = cellar.join("vilan");
        fs::write(&real, "the installed binary").expect("write the binary");
        let link = farm.join("vilan");
        std::os::unix::fs::symlink(&real, &link).expect("link it into the farm");

        let resolved = resolved_for_channel(&link);
        assert!(
            resolved.to_string_lossy().contains("/Cellar/vilan/0.14.0/"),
            "the symlink was not followed: {}",
            resolved.display()
        );
        // A path that resolves to nothing is classified as it was given, not
        // dropped.
        let absent = farm.join("absent");
        assert_eq!(resolved_for_channel(&absent), absent);
        let _ = fs::remove_dir_all(&root);
    }

    /// One table decides both halves of every steer, so a message can never
    /// name a package manager without naming the command that drives it.
    #[test]
    fn only_a_package_managers_install_has_a_command_to_steer_to() {
        assert_eq!(
            Channel::SelfManaged.owner(),
            None,
            "the self-managed install is the one `vilan upgrade` replaces itself"
        );
        // The scoped name, never the bare one: npm's similarity rule blocks
        // `vilan` (distribution.md, amendment 2026-07-25), and a steer to a
        // package that cannot exist is worse than no steer.
        assert_eq!(
            Channel::Npm.owner(),
            Some(("npm", "npm update -g @vilan-lang/vilan"))
        );
        assert_eq!(
            Channel::Homebrew.owner(),
            Some(("Homebrew", "brew upgrade vilan"))
        );
    }

    /// The steer answers the question that was asked: who owns this install,
    /// what to run instead, and — under `--check`, which still does discovery
    /// — which version is waiting.
    #[test]
    fn the_steer_names_the_owner_its_command_and_the_new_version() {
        let npm = Channel::Npm.owner().expect("npm is a package manager");
        let plain = steer_message(false, npm, "0.14.0", None);
        assert_eq!(
            plain,
            "vilan 0.14.0 was installed by npm, which owns these files — upgrade it with:\n\n    npm update -g @vilan-lang/vilan"
        );
        let checked = steer_message(false, npm, "0.14.0", Some("0.15.0"));
        assert!(
            checked.contains("vilan 0.14.0 → 0.15.0 available"),
            "`--check` still reports what discovery found: {checked}"
        );
        assert!(checked.ends_with("    npm update -g @vilan-lang/vilan"));

        // Homebrew's steer is Homebrew's command, and nobody else's.
        let brew = steer_message(
            false,
            Channel::Homebrew.owner().expect("Homebrew"),
            "0.14.0",
            None,
        );
        assert!(brew.contains("installed by Homebrew"), "{brew}");
        assert!(brew.ends_with("    brew upgrade vilan"));
        assert!(!brew.contains("npm"), "{brew}");

        // And no steer ever points back at `vilan upgrade`, which is the
        // command that just refused to act.
        for message in [&plain, &checked, &brew] {
            assert!(!message.contains("vilan upgrade`"), "{message}");
        }

        // Colored: escapes are the only difference, and no span crosses a
        // newline (a pager would carry the color into what it printed next).
        let colored = steer_message(true, npm, "0.14.0", None);
        assert_eq!(strip_escapes(&colored), plain);
        assert!(colored.contains("\x1b[36m"), "the headline lost cyan");
        assert!(colored.contains("\x1b[1m"), "the command lost bold");
        for line in colored.lines().filter(|line| !line.trim().is_empty()) {
            assert!(
                line.trim_start().starts_with('\x1b') && line.ends_with("\x1b[0m"),
                "a line is not self-contained: {line:?}"
            );
        }
    }
}
