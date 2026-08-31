//! The npm channel (proposal/distribution.md §2): the two `bin` stubs in
//! `npm/meta/` are the only JavaScript vilan ships, they run on every npm
//! user's machine before any vilan code does, and npm is not needed to test
//! them — an installed layout is a directory tree.
//!
//! So each test builds one: `node_modules/@vilan-lang/vilan` copied from the
//! packaging sources, `node_modules/@vilan-lang/<platform>` holding this
//! build's real binary, and node run against the stub. What that pins is the
//! hand-off — same arguments, same bytes on both streams, same exit code as
//! invoking the binary directly — plus the two ways resolution can fail and
//! the message each produces.
//!
//! The second half pins the packaging *sources* instead of the stub: the
//! platform table inside `launch.js` against the packages that exist, the
//! `os`/`cpu` constraints against the directory names, and the
//! `0.0.0-placeholder` version the release workflow stamps. Those manifests
//! are otherwise unexercised until a real `npm publish`, which is exactly the
//! wrong moment to discover they drifted.
//!
//! node is a suite-wide requirement (every emitted-JS test runs it), so
//! nothing here is `#[cfg]`-gated except the one signal test, which pins a
//! unix concept.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The npm scope every package in this channel lives under — the bare name
/// `vilan` is blocked by npm's similarity rule (distribution.md, amendment
/// 2026-07-25).
const SCOPE: &str = "@vilan-lang";

/// What CI stamps over. Never a real version in the tree.
const PLACEHOLDER: &str = "0.0.0-placeholder";

/// The five platform packages, in the order `npm/platform/` lists them.
const PLATFORMS: [&str; 5] = [
    "darwin-arm64",
    "darwin-x64",
    "linux-arm64",
    "linux-x64",
    "win32-x64",
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// This machine's platform package name, in node's own spelling — the same
/// `${process.platform}-${process.arch}` key the stub looks up.
fn platform_key() -> String {
    let output = Command::new("node")
        .args(["-p", "process.platform + '-' + process.arch"])
        .output()
        .expect("run node");
    assert!(
        output.status.success(),
        "node could not report its platform"
    );
    let key = String::from_utf8_lossy(&output.stdout).trim().to_string();
    assert!(
        PLATFORMS.contains(&key.as_str()),
        "this machine is {key}, which vilan publishes no npm package for — the \
         hand-off tests need one of {PLATFORMS:?}"
    );
    key
}

/// Where the platform package sits relative to the meta package. npm's own
/// answer depends on the version and on what else is installed, and node
/// finds both by walking `node_modules` up the ancestors — so both are pinned.
#[derive(Clone, Copy, Debug)]
enum Layout {
    /// `<root>/node_modules/@vilan-lang/{vilan,<platform>}` — hoisted siblings.
    Hoisted,
    /// `…/@vilan-lang/vilan/node_modules/@vilan-lang/<platform>` — nested
    /// under the package that depends on it.
    Nested,
}

/// A machine with `@vilan-lang/vilan` installed: the real packaging sources,
/// and this build's binary standing in for the released one.
struct Install {
    root: PathBuf,
    /// `bin/vilan.js` — what npm's shim would invoke.
    stub: PathBuf,
    /// `bin/vilan-lsp.js`.
    lsp_stub: PathBuf,
    /// The platform package's `bin/`, so a test can take a binary away again.
    platform_bin: PathBuf,
}

impl Install {
    fn new(name: &str, layout: Layout) -> Install {
        let root = std::env::temp_dir().join(format!("vilan-npm-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let modules = root.join("node_modules").join(SCOPE);
        let meta = modules.join("vilan");
        copy_tree(&repo_root().join("npm/meta"), &meta);

        let platform = platform_key();
        let platform_root = match layout {
            Layout::Hoisted => modules.join(&platform),
            Layout::Nested => meta.join("node_modules").join(SCOPE).join(&platform),
        };
        copy_tree(
            &repo_root().join("npm/platform").join(&platform),
            &platform_root,
        );
        let platform_bin = platform_root.join("bin");
        fs::create_dir_all(&platform_bin).expect("create the platform package's bin");
        let suffix = std::env::consts::EXE_SUFFIX;
        for binary in ["vilan", "vilan-lsp"] {
            // Both names carry the CLI binary. `vilan-lsp` is not the language
            // server here: it never speaks without a client, and what its stub
            // has to prove is that it resolves and hands over *its own* name's
            // file — which the test does by taking the other one away.
            fs::copy(
                env!("CARGO_BIN_EXE_vilan"),
                platform_bin.join(format!("{binary}{suffix}")),
            )
            .expect("stage the platform binary");
        }

        Install {
            stub: meta.join("bin/vilan.js"),
            lsp_stub: meta.join("bin/vilan-lsp.js"),
            platform_bin,
            root,
        }
    }

    /// `node bin/vilan.js <arguments>` from `directory`.
    fn through_stub(&self, stub: &Path, arguments: &[&str], directory: &Path) -> Output {
        run_retrying(
            Command::new("node")
                .arg(stub)
                .args(arguments)
                .current_dir(directory),
        )
    }

    /// The same arguments, straight to the binary — the byte-for-byte baseline.
    fn direct(&self, arguments: &[&str], directory: &Path) -> Output {
        run_retrying(
            Command::new(env!("CARGO_BIN_EXE_vilan"))
                .args(arguments)
                .current_dir(directory),
        )
    }

    /// A project whose program echoes the arguments it was handed.
    fn echo_project(&self) -> PathBuf {
        let project = self.root.join("echo");
        fs::create_dir_all(project.join("src")).expect("create the project");
        fs::write(
            project.join("vilan.toml"),
            "[package]\nname = \"echo\"\nversion = \"0.1.0\"\n",
        )
        .expect("write vilan.toml");
        fs::write(
            project.join("src/main.vl"),
            "import std::io::print;\nimport std::process::args;\n\nfun main() {\n\tfor arg in args() {\n\t\tprint(arg);\n\t}\n}\n",
        )
        .expect("write main.vl");
        project
    }
}

impl Drop for Install {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("create the destination");
    for entry in fs::read_dir(from)
        .expect("read the packaging source")
        .flatten()
    {
        let destination = to.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_tree(&entry.path(), &destination);
        } else {
            fs::copy(entry.path(), &destination).expect("copy the packaging source");
        }
    }
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
            other => return other.expect("run the command"),
        }
    }
}

/// Both streams and the status, as a failure message worth reading.
fn describe(output: &Output) -> String {
    format!(
        "status {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

// ---------------------------------------------------------------------------
// The hand-off
// ---------------------------------------------------------------------------

/// The whole promise of the stub: `vilan` through npm is `vilan`. Arguments
/// arrive verbatim (spaces, `=`, leading hyphens, non-ASCII), both streams are
/// the child's own, and the exit code is the child's — under either of the two
/// layouts npm can produce.
#[test]
fn the_stub_hands_over_arguments_streams_and_exit_code_verbatim() {
    for layout in [Layout::Hoisted, Layout::Nested] {
        let install = Install::new(&format!("handoff-{layout:?}"), layout);
        let project = install.echo_project();
        let arguments = [
            "run",
            ".",
            "a b",
            "--flag=x",
            "-n",
            "üñî",
            "",
            "trailing space ",
        ];

        let direct = install.direct(&arguments, &project);
        assert!(direct.status.success(), "{layout:?}: {}", describe(&direct));
        let stubbed = install.through_stub(&install.stub, &arguments, &project);

        assert_eq!(
            stubbed.stdout,
            direct.stdout,
            "{layout:?}: stdout differs — stub {:?} vs direct {:?}",
            String::from_utf8_lossy(&stubbed.stdout),
            String::from_utf8_lossy(&direct.stdout)
        );
        assert_eq!(
            stubbed.stderr,
            direct.stderr,
            "{layout:?}: stderr differs — stub {:?}",
            String::from_utf8_lossy(&stubbed.stderr)
        );
        assert_eq!(stubbed.status.code(), direct.status.code(), "{layout:?}");
        // Not a vacuous comparison of two empty streams: the program really
        // did receive the odd arguments.
        let echoed = String::from_utf8_lossy(&stubbed.stdout);
        for argument in ["a b", "--flag=x", "-n", "üñî", "trailing space "] {
            assert!(
                echoed.lines().any(|line| line == argument),
                "{layout:?}: {argument:?} did not reach the program: {echoed:?}"
            );
        }
    }
}

/// A failing command keeps its exit code and its diagnostics — the stub must
/// not turn a compile error into a stub error, nor swallow the rendering.
#[test]
fn a_failing_build_keeps_its_diagnostics_and_its_nonzero_exit() {
    let install = Install::new("failing", Layout::Hoisted);
    let project = install.echo_project();
    fs::write(
        project.join("src/main.vl"),
        "fun main() {\n\tlet x: i32 = \"not an int\";\n}\n",
    )
    .expect("write a broken program");

    let direct = install.direct(&["build", "."], &project);
    let stubbed = install.through_stub(&install.stub, &["build", "."], &project);
    assert!(!direct.status.success(), "{}", describe(&direct));
    assert_eq!(
        stubbed.status.code(),
        direct.status.code(),
        "the failure's exit code is the child's: {}",
        describe(&stubbed)
    );
    assert_eq!(stubbed.stdout, direct.stdout);
    assert_eq!(stubbed.stderr, direct.stderr);
    assert!(
        String::from_utf8_lossy(&stubbed.stderr).contains("Expected i32"),
        "the diagnostic did not survive the hand-off: {}",
        describe(&stubbed)
    );
}

/// The lsp stub resolves *its own* name. Non-vacuous because the fixture takes
/// `bin/vilan` away first: a stub that resolved the compiler's file instead
/// would fail to find anything at all.
#[test]
fn the_lsp_stub_hands_over_to_the_lsp_binary() {
    let install = Install::new("lsp", Layout::Hoisted);
    let suffix = std::env::consts::EXE_SUFFIX;
    fs::remove_file(install.platform_bin.join(format!("vilan{suffix}")))
        .expect("take the compiler binary away");

    let stubbed = install.through_stub(&install.lsp_stub, &["--version"], &install.root);
    assert!(stubbed.status.success(), "{}", describe(&stubbed));
    // The payload under that name is the CLI binary (see `Install::new`), so
    // what it prints is the CLI's version banner — the point is that
    // `bin/vilan-lsp` was the file spawned.
    let direct = install.direct(&["--version"], &install.root);
    assert_eq!(stubbed.stdout, direct.stdout);
}

/// The platform package is an *optional* dependency, so `--omit=optional`, a
/// half-finished install, or a manual `rm -rf` all land here. The message has
/// to name the package that is missing and a way out.
#[test]
fn a_missing_platform_package_names_the_package_and_the_releases_page() {
    let install = Install::new("absent", Layout::Hoisted);
    fs::remove_dir_all(install.platform_bin.parent().expect("the package"))
        .expect("uninstall the platform package");

    let stubbed = install.through_stub(&install.stub, &["--version"], &install.root);
    assert_eq!(
        stubbed.status.code(),
        Some(1),
        "a stub that cannot find its binary fails: {}",
        describe(&stubbed)
    );
    assert!(stubbed.stdout.is_empty(), "{}", describe(&stubbed));
    let message = String::from_utf8_lossy(&stubbed.stderr);
    let key = platform_key();
    assert!(
        message.contains(&format!("no prebuilt binary for {key}"))
            && message.contains(&format!("{SCOPE}/{key} is not installed"))
            && message.contains("npm install -g @vilan-lang/vilan")
            && message.contains("https://github.com/vilan-lang/vilan/releases"),
        "unhelpful: {message}"
    );
}

/// A platform with no build at all — the other failure class, and the only one
/// a user cannot fix by reinstalling, so it names the five that do exist.
#[test]
fn an_unsupported_platform_names_the_five_that_have_binaries() {
    let install = Install::new("unsupported", Layout::Hoisted);
    // `process.platform` is decided by the machine, so the pin needs a driver
    // that overrides it before loading the stub. Everything after that point
    // is the shipped code path.
    let driver = install.root.join("elsewhere.js");
    fs::write(
        &driver,
        "Object.defineProperty(process, \"platform\", { value: \"sunos\" });\n\
         require(process.env.VILAN_STUB);\n",
    )
    .expect("write the driver");
    let output = run_retrying(
        Command::new("node")
            .arg(&driver)
            .env("VILAN_STUB", &install.stub),
    );

    assert_eq!(output.status.code(), Some(1), "{}", describe(&output));
    let message = String::from_utf8_lossy(&output.stderr);
    assert!(
        message.contains("no prebuilt binary for sunos-"),
        "does not name the platform: {message}"
    );
    for platform in PLATFORMS {
        assert!(
            message.contains(platform),
            "{platform} is missing from the supported list: {message}"
        );
    }
    assert!(
        message.contains("https://github.com/vilan-lang/vilan/releases"),
        "no way out: {message}"
    );
}

/// A child killed by a signal has no exit code, and the stub must not report
/// its own. `128 + n` is what a shell reports, and Ctrl-C on `vilan run` is
/// the everyday case.
///
/// unix-only because signals are: Windows children die by exit code, and the
/// fixture below is a `#!/bin/sh` payload.
#[cfg(unix)]
#[test]
fn a_signalled_child_exits_128_plus_the_signal_number() {
    use std::os::unix::fs::PermissionsExt;

    let install = Install::new("signalled", Layout::Hoisted);
    let payload = install.platform_bin.join("vilan");
    fs::write(&payload, "#!/bin/sh\nkill -TERM $$\n").expect("write the payload");
    fs::set_permissions(&payload, fs::Permissions::from_mode(0o755)).expect("chmod the payload");

    let output = install.through_stub(&install.stub, &[], &install.root);
    assert_eq!(
        output.status.code(),
        Some(143),
        "SIGTERM is 15, so a shell would report 143: {}",
        describe(&output)
    );
}

// ---------------------------------------------------------------------------
// The packaging sources
// ---------------------------------------------------------------------------

/// Runs `program` under node with `$VILAN_JSON` pointing at `file`; answers
/// its trimmed stdout. node is this suite's JSON parser — nothing in the
/// workspace parses JSON in Rust, and node is what npm itself reads these
/// manifests with.
fn node_read(file: &Path, program: &str) -> String {
    let output = Command::new("node")
        .args(["-e", program])
        .env("VILAN_JSON", file)
        .output()
        .expect("run node");
    assert!(
        output.status.success(),
        "reading {}: {}",
        file.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .trim_end()
        .to_string()
}

fn manifest(package: &str) -> PathBuf {
    if package == "vilan" {
        repo_root().join("npm/meta/package.json")
    } else {
        repo_root()
            .join("npm/platform")
            .join(package)
            .join("package.json")
    }
}

/// The stub's table is what decides which package a machine looks for; the
/// directories are what CI publishes. A name in one and not the other is a
/// platform that installs and then cannot run.
#[test]
fn the_stubs_platform_table_and_the_packaged_platforms_are_the_same_five() {
    let table = node_read(
        &repo_root().join("npm/meta/lib/launch.js"),
        "const { PACKAGES } = require(process.env.VILAN_JSON);\
         console.log(Object.entries(PACKAGES).map(([key, name]) => `${key} ${name}`).sort().join('\\n'))",
    );
    let mut packaged: Vec<String> = fs::read_dir(repo_root().join("npm/platform"))
        .expect("read npm/platform")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect();
    packaged.sort();
    assert_eq!(
        packaged, PLATFORMS,
        "npm/platform/ no longer holds exactly the five packages this file knows"
    );

    let expected = packaged
        .iter()
        .map(|name| format!("{name} {SCOPE}/{name}"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        table, expected,
        "launch.js's table and npm/platform/ disagree"
    );

    // …and each directory's manifest claims the name the table resolves.
    for platform in PLATFORMS {
        assert_eq!(
            node_read(
                &manifest(platform),
                "console.log(require(process.env.VILAN_JSON).name)"
            ),
            format!("{SCOPE}/{platform}")
        );
    }
}

/// The third place the five are written down — and the one that actually
/// copies binaries into the packages. A row here without a package (or the
/// reverse) publishes a platform package with nothing in it, or none at all.
#[test]
fn the_assembly_script_packs_exactly_these_five_platforms() {
    let script = fs::read_to_string(repo_root().join("scripts/npm-package.sh"))
        .expect("read the assembly script");
    let table = script
        .split_once("TARGETS=\"")
        .expect("the TARGETS table")
        .1
        .split_once('"')
        .expect("its closing quote")
        .0;

    let mut packed: Vec<&str> = table
        .lines()
        .map(|row| {
            row.split(':')
                .nth(1)
                .expect("<rust target>:<npm package>:<archive kind>")
        })
        .collect();
    packed.sort();
    assert_eq!(
        packed, PLATFORMS,
        "scripts/npm-package.sh and npm/platform/ disagree about the targets"
    );

    // …and each row unpacks the archive kind the release workflow actually
    // publishes for that target: a `.zip` on Windows, a tarball everywhere
    // else.
    for row in table.lines() {
        let fields: Vec<&str> = row.split(':').collect();
        assert_eq!(fields.len(), 3, "malformed row: {row}");
        let expected = if fields[1].starts_with("win32") {
            "zip"
        } else {
            "tar.gz"
        };
        assert_eq!(fields[2], expected, "{row}");
    }
}

/// npm installs a platform package only where it can run — that is the whole
/// reason there are five. The constraint is the directory name, twice over.
#[test]
fn each_platform_manifest_constrains_install_to_its_own_os_and_cpu() {
    for platform in PLATFORMS {
        let (operating_system, cpu) = platform.split_once('-').expect("<platform>-<arch>");
        assert_eq!(
            node_read(
                &manifest(platform),
                "const m = require(process.env.VILAN_JSON);\
                 console.log(JSON.stringify(m.os) + ' ' + JSON.stringify(m.cpu))"
            ),
            format!("[\"{operating_system}\"] [\"{cpu}\"]"),
            "{platform}'s os/cpu do not match its name"
        );
        // Yarn PnP would otherwise keep the package zipped, and an executable
        // inside a zip cannot be spawned.
        assert_eq!(
            node_read(
                &manifest(platform),
                "console.log(require(process.env.VILAN_JSON).preferUnplugged)"
            ),
            "true",
            "{platform} must stay unzipped on disk"
        );
    }
}

/// The meta package is the one that must install anywhere (it is what carries
/// the error message for a platform with no binary), and its dependencies are
/// locked to the exact version so it can never resolve a mismatched binary.
#[test]
fn the_meta_package_installs_everywhere_and_binds_its_platform_packages_exactly() {
    let meta = manifest("vilan");
    assert_eq!(
        node_read(
            &meta,
            "const m = require(process.env.VILAN_JSON);\
             console.log(JSON.stringify([m.os, m.cpu]))"
        ),
        "[null,null]",
        "an os/cpu field on the meta package would block the very installs \
         whose error message it exists to print"
    );
    assert_eq!(
        node_read(
            &meta,
            "const { bin } = require(process.env.VILAN_JSON);\
             console.log(Object.entries(bin).map(([name, file]) => `${name} ${file}`).sort().join('\\n'))"
        ),
        "vilan bin/vilan.js\nvilan-lsp bin/vilan-lsp.js"
    );
    for stub in ["bin/vilan.js", "bin/vilan-lsp.js", "lib/launch.js"] {
        assert!(
            repo_root().join("npm/meta").join(stub).is_file(),
            "{stub} is declared but absent"
        );
    }

    let dependencies = node_read(
        &meta,
        "const { optionalDependencies } = require(process.env.VILAN_JSON);\
         console.log(Object.entries(optionalDependencies).map(([name, range]) => `${name}@${range}`).sort().join('\\n'))",
    );
    let expected = PLATFORMS
        .iter()
        .map(|platform| format!("{SCOPE}/{platform}@{PLACEHOLDER}"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        dependencies, expected,
        "the five platform packages must be optional dependencies at the \
         exact (stamped) version"
    );
}

/// No version is stored in this tree: `scripts/npm-package.sh` stamps the
/// release tag's version over the placeholder, which is why
/// `scripts/bump-version.sh` says nothing about npm. A real version checked in
/// here would be published as-is, and no later step would notice.
#[test]
fn every_manifest_carries_the_placeholder_version_ci_stamps() {
    for package in PLATFORMS.iter().copied().chain(["vilan"]) {
        assert_eq!(
            node_read(
                &manifest(package),
                "console.log(require(process.env.VILAN_JSON).version)"
            ),
            PLACEHOLDER,
            "{package} carries a checked-in version"
        );
        assert_eq!(
            node_read(
                &manifest(package),
                "console.log(require(process.env.VILAN_JSON).license)"
            ),
            "MIT OR Apache-2.0",
            "{package} must carry the project's dual license"
        );
    }
}
