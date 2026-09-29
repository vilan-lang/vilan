//! `vilan upgrade` pins (proposal/releases.md §6), fully offline: a fake
//! release tree served over `file://` (curl speaks it) drives the real
//! discovery/download/verify/swap path against a copied binary. The fake
//! "release" binaries are shell scripts that identify themselves, so a swap
//! is observable by running the result.
//!
//! `vilan cache prune` (L21) is pinned here too, and for the fixture rather
//! than for the command: the scratch `HOME` below already seeds a std cache
//! with one entry inside the seven-day guard and one past it, which is exactly
//! what a prune has to tell apart. The cache is the one piece of `~/.vilan`
//! both commands work on.
//!
//! **The whole file is unix** (windows-support.md §4, §8), not just a test or
//! two: the fake release binaries are `#!/bin/sh` scripts, they are made
//! executable through `PermissionsExt`'s `0o755` (the import below does not
//! even exist on Windows — unconditionally it makes `cargo test` fail to
//! COMPILE there), the fixture tree is built with `tar`/`sha256sum`, the
//! stale-cache entry is backdated with `touch -d`, and the `file://` base URL
//! is assembled by string concatenation from a unix path.
//!
//! S6 taught `vilan upgrade` the `.zip` asset, the in-process checksum and the
//! rename-the-running-exe dance, and **a Windows twin of this file did not
//! land with it**: every fixture ingredient above is unix-shaped, and the one
//! portable substitute for the shell-script payload (a copy of the real
//! `vilan.exe`) makes the swap unobservable — the before and after banners
//! would be identical, which is precisely what these tests assert on. The
//! Windows-specific behavior is instead pinned where it can actually be
//! *executed* from either platform: `upgrade.rs`'s `install_binaries` takes
//! the rename-aside strategy as a parameter, and its unit tests run both arms
//! and assert they differ. What remains Windows-only — that the aside rename
//! succeeds against a *running* executable — is S6's live-host item.
//! Listed, never silently skipped.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

mod support;

/// The release's extension asset (E229).
const VSIX: &str = "vilan-vscode.vsix";

/// The fixture's VS Code CLI: records each call in `$HOME/editor.log` (and the
/// bytes of any `.vsix` it is handed), answers `--list-extensions` with
/// `$VILAN_FIXTURE_LISTED`, and exits `$VILAN_FIXTURE_GALLERY_EXIT` for an
/// install by gallery id (an unreachable gallery) and
/// `$VILAN_FIXTURE_VSIX_EXIT` for an install from a file.
const EDITOR_SHIM: &str = r#"#!/bin/sh
printf '%s\n' "${0##*/} $*" >> "$HOME/editor.log"
for argument in "$@"; do
    case "$argument" in
        --list-extensions) printf '%s\n' "${VILAN_FIXTURE_LISTED:-}"; exit 0 ;;
        *.vsix)
            cat "$argument" >> "$HOME/editor.log"
            if [ "${VILAN_FIXTURE_VSIX_EXIT:-0}" != 0 ]; then
                echo "the editor refused the file" >&2
            fi
            exit "${VILAN_FIXTURE_VSIX_EXIT:-0}"
            ;;
    esac
done
if [ "${VILAN_FIXTURE_GALLERY_EXIT:-0}" != 0 ]; then
    echo "Failed Installing Extensions: the gallery is unreachable" >&2
fi
exit "${VILAN_FIXTURE_GALLERY_EXIT:-0}"
"#;

/// A scratch install: a copied `vilan` in its own bin dir, plus a fake
/// release tree for `$VILAN_UPGRADE_BASE`.
struct Fixture {
    root: PathBuf,
    bin: PathBuf,
    home: PathBuf,
    base_url: String,
}

impl Fixture {
    fn new(name: &str) -> Fixture {
        let root =
            support::scratch_root().join(format!("vilan-upgrade-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let bin = root.join("bin");
        fs::create_dir_all(&bin).expect("create bin dir");
        fs::copy(env!("CARGO_BIN_EXE_vilan"), bin.join("vilan")).expect("copy the binary");

        // The fake v9.9.9 release: self-identifying shell scripts, tarred and
        // checksummed exactly as the release workflow does.
        let stage = root.join("stage");
        fs::create_dir_all(&stage).expect("create stage");
        for (binary, banner) in [
            ("vilan", "vilan 9.9.9 (fake)"),
            ("vilan-lsp", "vilan-lsp 9.9.9 (fake)"),
        ] {
            let path = stage.join(binary);
            fs::write(&path, format!("#!/bin/sh\necho \"{banner}\"\n")).expect("write dummy");
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod dummy");
        }
        let assets = root.join("releases/download/v9.9.9");
        fs::create_dir_all(&assets).expect("create release dir");
        let asset_name = format!("vilan-{}.tar.gz", env!("VILAN_TARGET"));
        run_ok(
            Command::new("tar")
                .args(["-czf"])
                .arg(assets.join(&asset_name))
                .args(["-C"])
                .arg(&stage)
                .args(["vilan", "vilan-lsp"]),
        );
        // E230: the release's extension beside the toolchain, checksummed in
        // the same `sha256sums.txt` — the offline fallback `vilan upgrade`
        // installs when the gallery cannot be reached. The CURRENT version's
        // release holds one too: an up-to-date toolchain still brings a stale
        // extension to its own version.
        fs::write(assets.join(VSIX), "the 9.9.9 extension\n").expect("write the vsix");
        let sums = run_ok(
            Command::new("sha256sum")
                .arg(&asset_name)
                .arg(VSIX)
                .current_dir(&assets),
        );
        fs::write(assets.join("sha256sums.txt"), sums.stdout).expect("write sums");
        let current = root.join(format!("releases/download/v{}", env!("CARGO_PKG_VERSION")));
        fs::create_dir_all(&current).expect("create the current release dir");
        fs::write(current.join(VSIX), "the current extension\n").expect("write the vsix");
        let sums = run_ok(Command::new("sha256sum").arg(VSIX).current_dir(&current));
        fs::write(current.join("sha256sums.txt"), sums.stdout).expect("write sums");

        // A scratch HOME with a pre-seeded std cache: one entry backdated past
        // the prune guard, one fresh — a successful upgrade prunes exactly the
        // stale one.
        let home = root.join("home");
        let cache = home.join(".vilan/std-cache");
        fs::create_dir_all(cache.join("fresh-entry/std")).expect("seed fresh cache entry");
        fs::create_dir_all(cache.join("stale-entry/std")).expect("seed stale cache entry");
        run_ok(
            Command::new("touch")
                .args(["-d", "2020-01-01"])
                .arg(cache.join("stale-entry")),
        );

        let base_url = format!("file://{}", root.display());
        Fixture {
            root,
            bin,
            home,
            base_url,
        }
    }

    fn release_vsix(&self, version: &str) -> PathBuf {
        self.root
            .join(format!("releases/download/v{version}"))
            .join(VSIX)
    }

    fn cache_entry(&self, name: &str) -> PathBuf {
        self.home.join(".vilan/std-cache").join(name)
    }

    /// A second copy of the binary, where npm would have put it. The path is
    /// the whole point: `vilan upgrade` reads its own location to decide
    /// whether these files are its to replace (distribution.md §2, call (b)).
    fn npm_install(&self) -> PathBuf {
        let bin = self.root.join("node_modules/@vilan-lang/linux-x64/bin");
        fs::create_dir_all(&bin).expect("create the npm install directory");
        let binary = bin.join("vilan");
        fs::copy(env!("CARGO_BIN_EXE_vilan"), &binary).expect("copy the binary");
        binary
    }

    /// `vilan upgrade` from an arbitrary copy, with both seams spelled out:
    /// `latest: None` leaves `$VILAN_UPGRADE_LATEST` unset, so discovery would
    /// have to go through `base` — which a steered channel must never do.
    fn upgrade_from(
        &self,
        binary: &Path,
        arguments: &[&str],
        base: &str,
        latest: Option<&str>,
    ) -> Output {
        let mut command = Command::new(binary);
        command
            .arg("upgrade")
            .args(arguments)
            .env("HOME", &self.home)
            .env("VILAN_UPGRADE_BASE", base)
            .env("VILAN_NO_VSCODE", "1")
            .env_remove("VILAN_UPGRADE_LATEST");
        if let Some(latest) = latest {
            command.env("VILAN_UPGRADE_LATEST", latest);
        }
        run_retrying(&mut command)
    }

    /// `vilan upgrade` against the fixture's release tree, with the editor
    /// step OFF: this machine's own `code` (a WSL launcher, say) must never be
    /// handed a fixture's extension. The E230 pins use [`Fixture::upgrade_with_editor`].
    fn upgrade(&self, arguments: &[&str], latest: &str) -> Output {
        run_retrying(
            Command::new(self.bin.join("vilan"))
                .arg("upgrade")
                .args(arguments)
                .env("HOME", &self.home)
                .env("VILAN_UPGRADE_BASE", &self.base_url)
                .env("VILAN_UPGRADE_LATEST", latest)
                .env("VILAN_NO_VSCODE", "1"),
        )
    }

    /// E230: `vilan upgrade` with the fixture's own VS Code CLI — a `code`
    /// shim first on PATH ([`EDITOR_SHIM`]) — and the environment `extra` sets.
    fn upgrade_with_editor(
        &self,
        arguments: &[&str],
        latest: &str,
        extra: &[(&str, &str)],
    ) -> Output {
        let shims = self.root.join("editor-bin");
        fs::create_dir_all(&shims).expect("create the shim dir");
        let code = shims.join("code");
        fs::write(&code, EDITOR_SHIM).expect("write the editor shim");
        fs::set_permissions(&code, fs::Permissions::from_mode(0o755)).expect("chmod the shim");
        let path = std::env::join_paths(std::iter::once(shims).chain(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        )))
        .expect("join PATH");
        let mut command = Command::new(self.bin.join("vilan"));
        command
            .arg("upgrade")
            .args(arguments)
            .env("HOME", &self.home)
            .env("PATH", path)
            .env("VILAN_UPGRADE_BASE", &self.base_url)
            .env("VILAN_UPGRADE_LATEST", latest)
            .env_remove("VILAN_NO_VSCODE");
        for (name, value) in extra {
            command.env(name, value);
        }
        run_retrying(&mut command)
    }

    /// What the editor shim was asked, one line per call, and the bytes of any
    /// extension file it was handed.
    fn editor_log(&self) -> String {
        fs::read_to_string(self.home.join("editor.log")).unwrap_or_default()
    }

    /// `vilan cache prune` against the fixture's scratch `HOME`, so the
    /// command resolves the seeded cache and never the machine's own.
    fn cache(&self, arguments: &[&str]) -> Output {
        run_retrying(
            Command::new(self.bin.join("vilan"))
                .arg("cache")
                .args(arguments)
                .env("HOME", &self.home),
        )
    }

    fn installed_banner(&self) -> String {
        // `--version` on the real binary; the fake one echoes its banner
        // regardless of arguments.
        let output = run_retrying(Command::new(self.bin.join("vilan")).arg("--version"));
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn upgrade_swaps_both_binaries_and_reports_the_new_version() {
    let fixture = Fixture::new("swap");
    let output = fixture.upgrade(&[], "9.9.9");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "upgrade failed:\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("→ v9.9.9"),
        "announces the target version: {stdout}"
    );
    assert!(
        stdout.contains("installed vilan 9.9.9 (fake)"),
        "reports the swapped version: {stdout}"
    );
    // The brand mark rides the success banner — and only here; the other
    // three paths pin its absence. A piped stream stays escape-free, so this
    // also exercises the TTY gate end to end.
    assert!(
        stdout.contains("▀██████████▄▄"),
        "the mark is missing from the success banner: {stdout}"
    );
    assert!(
        !stdout.contains('\x1b'),
        "escape bytes on a piped stream: {stdout:?}"
    );

    // The running binary's path now holds the new release, lsp beside it.
    assert_eq!(fixture.installed_banner(), "vilan 9.9.9 (fake)");
    // And ~/.vilan housekeeping ran: the stale cache entry is gone, the
    // fresh one (a running binary could be reading it) stays.
    assert!(
        stdout.contains("pruned 1 stale std cache entry"),
        "{stdout}"
    );
    assert!(!fixture.cache_entry("stale-entry").exists());
    assert!(fixture.cache_entry("fresh-entry").is_dir());
    let lsp = run_retrying(&mut Command::new(fixture.bin.join("vilan-lsp")));
    assert_eq!(
        String::from_utf8_lossy(&lsp.stdout).trim(),
        "vilan-lsp 9.9.9 (fake)"
    );
}

#[test]
fn upgrade_check_reports_and_changes_nothing() {
    let fixture = Fixture::new("check");
    let output = fixture.upgrade(&["--check"], "9.9.9");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(
        stdout.contains("9.9.9 available") && stdout.contains("vilan upgrade"),
        "points at the command: {stdout}"
    );
    // Still the real binary, no lsp appeared, and the cache is untouched —
    // `--check` changes nothing.
    assert!(
        fixture
            .installed_banner()
            .starts_with(&format!("vilan {}", env!("CARGO_PKG_VERSION")))
    );
    assert!(!fixture.bin.join("vilan-lsp").exists());
    assert!(fixture.cache_entry("stale-entry").is_dir());
    assert!(
        !stdout.chars().any(|glyph| matches!(glyph, '▀' | '▄' | '█')),
        "`--check` must not print the mark: {stdout}"
    );
}

#[test]
fn upgrade_declines_when_already_the_newest() {
    let fixture = Fixture::new("newest");
    let output = fixture.upgrade(&[], "0.1.0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(stdout.contains("is the newest release"), "{stdout}");
    assert!(
        !stdout.chars().any(|glyph| matches!(glyph, '▀' | '▄' | '█')),
        "an up-to-date install must not print the mark: {stdout}"
    );
    assert!(
        fixture
            .installed_banner()
            .starts_with(&format!("vilan {}", env!("CARGO_PKG_VERSION")))
    );
}

#[test]
fn upgrade_aborts_on_a_checksum_mismatch_without_touching_the_install() {
    let fixture = Fixture::new("badsum");
    // Corrupt the recorded hash: flip its first hex digit.
    let sums_path = fixture.root.join("releases/download/v9.9.9/sha256sums.txt");
    let sums = fs::read_to_string(&sums_path).expect("read sums");
    let flipped = if sums.starts_with('0') { "f" } else { "0" };
    fs::write(&sums_path, format!("{flipped}{}", &sums[1..])).expect("corrupt sums");

    let output = fixture.upgrade(&[], "9.9.9");
    assert!(
        !output.status.success(),
        "a bad checksum must fail the upgrade"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("checksum mismatch"),
        "names the problem: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.chars().any(|glyph| matches!(glyph, '▀' | '▄' | '█')),
        "a failed upgrade must not print the mark: {stdout}"
    );
    assert!(
        fixture
            .installed_banner()
            .starts_with(&format!("vilan {}", env!("CARGO_PKG_VERSION")))
    );
    assert!(!fixture.bin.join("vilan-lsp").exists());
}

/// An npm-owned install is steered instead of replaced — and steered *without
/// touching the network*, which is what the unreachable base URL below proves:
/// discovery through it would fail the run, so a passing test is a run that
/// never tried. (`vilan upgrade`'s answer cannot depend on the release page
/// when it is not allowed to act on it either way.)
#[test]
fn an_npm_install_is_steered_without_reaching_the_network() {
    let fixture = Fixture::new("npm-steer");
    let binary = fixture.npm_install();
    let output = fixture.upgrade_from(&binary, &[], "file:///vilan-no-such-release-tree", None);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "the steer answered the question, so it exits 0:\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "a steer is not an error: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("was installed by npm")
            && stdout.contains("npm update -g @vilan-lang/vilan"),
        "does not steer: {stdout}"
    );

    // Nothing was downloaded, nothing was swapped, and the mark — which only a
    // completed upgrade prints — stayed away.
    assert!(!binary.with_file_name("vilan-lsp").exists());
    assert!(
        !stdout.chars().any(|glyph| matches!(glyph, '▀' | '▄' | '█')),
        "a steer must not print the mark: {stdout}"
    );
    assert!(
        banner(&binary).starts_with(&format!("vilan {}", env!("CARGO_PKG_VERSION"))),
        "the binary was replaced"
    );
    assert!(
        fixture.cache_entry("stale-entry").is_dir(),
        "the cache was pruned"
    );
}

/// `--check` under a steered channel still answers what it was asked — the
/// newest version — and only then points at the owner's command. Still no
/// download and no swap.
#[test]
fn a_steered_check_reports_the_new_version_and_swaps_nothing() {
    let fixture = Fixture::new("npm-check");
    let binary = fixture.npm_install();
    let output = fixture.upgrade_from(&binary, &["--check"], &fixture.base_url, Some("9.9.9"));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{stdout}");
    assert!(
        stdout.contains("→ 9.9.9 available") && stdout.contains("npm update -g @vilan-lang/vilan"),
        "the discovery half still runs, the steer replaces only the command: {stdout}"
    );
    assert!(
        !stdout.contains("run `vilan upgrade`"),
        "steering back to the command that just declined: {stdout}"
    );
    assert!(!binary.with_file_name("vilan-lsp").exists());
    assert!(banner(&binary).starts_with(&format!("vilan {}", env!("CARGO_PKG_VERSION"))));

    // And when there is nothing newer, the same channel is reported rather
    // than steered — there is nothing to run.
    let current = fixture.upgrade_from(&binary, &["--check"], &fixture.base_url, Some("0.1.0"));
    let stdout = String::from_utf8_lossy(&current.stdout);
    assert!(current.status.success());
    assert!(
        stdout.contains("is the newest release (installed by npm)"),
        "names the channel without a pointless command: {stdout}"
    );
}

/// `<binary> --version`, trimmed — a swapped-in fake release identifies itself
/// differently, so this is how a test sees whether the swap happened.
fn banner(binary: &Path) -> String {
    let output = run_retrying(Command::new(binary).arg("--version"));
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn run_ok(command: &mut Command) -> Output {
    let output = command.output().expect("spawn");
    assert!(
        output.status.success(),
        "fixture command failed: {command:?}"
    );
    output
}

/// Spawn with an ETXTBSY retry — the fork/CLOEXEC exec race between parallel
/// tests copying binaries (same guard as tests/install.rs).
fn run_retrying(command: &mut Command) -> Output {
    const ETXTBSY: i32 = 26;
    let mut attempts = 0;
    loop {
        match command.output() {
            Err(error) if error.raw_os_error() == Some(ETXTBSY) && attempts < 100 => {
                attempts += 1;
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            other => return other.expect("run the binary"),
        }
    }
}

/// L21: `vilan cache prune --dry-run` names the stale entry, with its size, and
/// deletes nothing.
///
/// The fixture's scratch `HOME` is what makes this safe to run at all: the
/// command resolves `~/.vilan/std-cache` through the same `home_dir` the
/// compiler does, so the pin drives the real path over a seeded cache instead
/// of the machine's own.
#[test]
fn cache_prune_dry_run_names_the_stale_entry_and_deletes_nothing() {
    let fixture = Fixture::new("cache-dry-run");
    let output = fixture.cache(&["prune", "--dry-run"]);
    let text = String::from_utf8_lossy(&output.stdout).to_string();

    assert!(
        output.status.success(),
        "`cache prune --dry-run` failed:\n{text}"
    );
    assert!(
        text.contains("would prune 1 entry"),
        "a dry run says what it WOULD do:\n{text}"
    );
    assert!(
        text.contains("stale-entry"),
        "the entry that would go is named:\n{text}"
    );
    assert!(
        !text.contains("fresh-entry"),
        "an entry inside the guard is not offered for removal:\n{text}"
    );
    assert!(
        fixture.cache_entry("stale-entry").is_dir(),
        "a dry run must delete nothing"
    );
    assert!(fixture.cache_entry("fresh-entry").is_dir());
}

/// L21: `vilan cache prune` deletes what the dry run named, and only that.
///
/// The explicit gesture, for the machine the two automatic prunes did not
/// catch up with — materialization prunes when a new hash lands and `vilan
/// upgrade` prunes while it owns `~/.vilan`, and a developer who neither
/// upgrades nor materializes a new std is exactly who this command is for.
#[test]
fn cache_prune_deletes_the_stale_entry_and_keeps_the_fresh_one() {
    let fixture = Fixture::new("cache-prune");
    let output = fixture.cache(&["prune"]);
    let text = String::from_utf8_lossy(&output.stdout).to_string();

    assert!(output.status.success(), "`cache prune` failed:\n{text}");
    assert!(
        text.contains("pruned 1 entry") && text.contains("stale-entry"),
        "the run reports what went:\n{text}"
    );
    assert!(!fixture.cache_entry("stale-entry").exists());
    assert!(
        fixture.cache_entry("fresh-entry").is_dir(),
        "an entry inside the seven-day guard may belong to a running compile"
    );

    // A second run has nothing left to do and says so rather than failing.
    let again = fixture.cache(&["prune"]);
    let text = String::from_utf8_lossy(&again.stdout).to_string();
    assert!(again.status.success());
    assert!(text.contains("nothing to prune"), "{text}");
}

// --- E230: `vilan upgrade` brings the editor along ---------------------------
//
// The installers install the release's extension beside the toolchain (E229);
// the in-place upgrade did not, so a vsix install — which VS Code pins and
// never auto-updates — sat a release behind its server. `vilan upgrade` runs
// the same step now: the GALLERY id first, which VS Code keeps updated from
// then on, and the release's checksummed vsix when the gallery cannot be
// reached. The editor refusing either is reported, never a failed upgrade.

/// After the swap, the gallery id — unversioned, so the install is not pinned
/// and follows the gallery from then on.
#[test]
fn e230_upgrade_installs_the_extension_from_the_gallery_after_the_swap() {
    let fixture = Fixture::new("e230-gallery");
    let output = fixture.upgrade_with_editor(&[], "9.9.9", &[]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fixture.installed_banner(), "vilan 9.9.9 (fake)");
    assert_eq!(
        fixture.editor_log(),
        "code --install-extension vilan-lang.vilan --force\n",
        "one call, by gallery id"
    );
    assert!(
        stdout.contains("VS Code extension: installed vilan-lang.vilan from the gallery")
            && stdout.contains("reload VS Code"),
        "{stdout}"
    );
}

/// The gallery unreachable: the release's own vsix, verified against the
/// release's `sha256sums.txt` before the editor sees it.
#[test]
fn e230_an_unreachable_gallery_falls_back_to_the_checked_vsix() {
    let fixture = Fixture::new("e230-offline");
    let output = fixture.upgrade_with_editor(&[], "9.9.9", &[("VILAN_FIXTURE_GALLERY_EXIT", "1")]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{stdout}");
    let log = fixture.editor_log();
    let calls: Vec<&str> = log.lines().collect();
    assert_eq!(
        calls[0], "code --install-extension vilan-lang.vilan --force",
        "{log}"
    );
    assert!(
        calls[1].starts_with("code --install-extension ")
            && calls[1].ends_with("vilan-vscode.vsix --force"),
        "{log}"
    );
    assert_eq!(
        calls[2], "the 9.9.9 extension",
        "the release's file, the new version's: {log}"
    );
    assert!(
        stdout.contains(
            "VS Code extension: installed vilan-vscode.vsix (the gallery was unreachable)"
        ),
        "{stdout}"
    );
}

/// A vsix that does not verify is never handed to the editor, and the
/// toolchain — already swapped — stays swapped: the summary says why the
/// extension is missing.
#[test]
fn e230_a_fallback_vsix_that_does_not_verify_is_never_installed() {
    let fixture = Fixture::new("e230-badsum");
    fs::write(fixture.release_vsix("9.9.9"), "tampered\n").expect("tamper");
    let output = fixture.upgrade_with_editor(&[], "9.9.9", &[("VILAN_FIXTURE_GALLERY_EXIT", "1")]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "the toolchain upgrade stands: {stdout}"
    );
    assert_eq!(fixture.installed_banner(), "vilan 9.9.9 (fake)");
    assert_eq!(
        fixture.editor_log(),
        "code --install-extension vilan-lang.vilan --force\n",
        "only the gallery was asked"
    );
    assert!(
        stdout.contains("VS Code extension: NOT installed")
            && stdout.contains("checksum mismatch for vilan-vscode.vsix"),
        "{stdout}"
    );
}

/// The owner's case: the toolchain is already the newest release, and the
/// extension is not its version. `vilan upgrade` brings the extension to it —
/// and leaves an extension that already matches alone.
#[test]
fn e230_an_up_to_date_toolchain_still_brings_a_stale_extension_to_its_version() {
    let fixture = Fixture::new("e230-newest");
    let output = fixture.upgrade_with_editor(
        &[],
        "0.1.0",
        &[("VILAN_FIXTURE_LISTED", "vilan-lang.vilan@0.40.0")],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{stdout}");
    assert!(stdout.contains("is the newest release"), "{stdout}");
    assert_eq!(
        fixture.editor_log(),
        "code --list-extensions --show-versions\ncode --install-extension vilan-lang.vilan --force\n",
    );
    assert!(
        stdout.contains("VS Code extension: installed vilan-lang.vilan from the gallery"),
        "{stdout}"
    );

    let current = Fixture::new("e230-current");
    let listed = format!("vilan-lang.vilan@{}", env!("CARGO_PKG_VERSION"));
    let output = current.upgrade_with_editor(&[], "0.1.0", &[("VILAN_FIXTURE_LISTED", &listed)]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{stdout}");
    assert_eq!(
        current.editor_log(),
        "code --list-extensions --show-versions\n",
        "a matching extension is not reinstalled"
    );
    assert!(
        stdout.contains(&format!(
            "VS Code extension: {listed} matches the toolchain"
        )),
        "{stdout}"
    );

    // Offline, the fallback is the CURRENT version's release vsix.
    let offline = Fixture::new("e230-newest-offline");
    let output = offline.upgrade_with_editor(&[], "0.1.0", &[("VILAN_FIXTURE_GALLERY_EXIT", "1")]);
    assert!(output.status.success());
    assert!(
        offline.editor_log().contains("the current extension"),
        "{}",
        offline.editor_log()
    );
}

/// The installers' opt-out, both spellings: nothing asks the editor anything.
#[test]
fn e230_the_editor_step_is_opted_out_by_flag_or_environment() {
    for (name, arguments, environment) in [
        ("e230-flag", &["--no-vscode"][..], &[][..]),
        ("e230-env", &[][..], &[("VILAN_NO_VSCODE", "1")][..]),
    ] {
        let fixture = Fixture::new(name);
        let output = fixture.upgrade_with_editor(arguments, "9.9.9", environment);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(output.status.success(), "{name}: {stdout}");
        assert_eq!(fixture.installed_banner(), "vilan 9.9.9 (fake)", "{name}");
        assert_eq!(fixture.editor_log(), "", "{name}: the editor was touched");
        assert!(
            stdout.contains("VS Code extension: not touched (--no-vscode / VILAN_NO_VSCODE)"),
            "{name}: {stdout}"
        );
    }
}

/// `--check` asks the release page one question and changes nothing — the
/// editor included.
#[test]
fn e230_check_never_touches_the_editor() {
    let fixture = Fixture::new("e230-check");
    let output = fixture.upgrade_with_editor(&["--check"], "9.9.9", &[]);
    assert!(output.status.success());
    assert_eq!(fixture.editor_log(), "");
}
