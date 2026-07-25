//! Git dependencies end to end (proposal/distribution.md §5): the real `vilan`
//! binary, real `git`, and real repositories — served over `file://`, the same
//! offline fixture shape `upgrade.rs` uses for releases. Nothing here touches
//! the network, and nothing here touches the developer's own
//! `~/.vilan/git-deps`: every run gets a scratch HOME, so the cache under test
//! is the fixture's.
//!
//! `git` is a hard prerequisite of these tests exactly as `node` is of the
//! runtime suites — the repository's own hygiene test already shells to it, and
//! CI images carry it. A missing `git` fails loudly rather than skipping
//! silently.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

/// A scratch world: a HOME whose `.vilan/git-deps` is the cache under test,
/// a place for fixture repositories, and a place for projects.
struct Fixture {
    root: PathBuf,
    home: PathBuf,
}

impl Fixture {
    fn new(tag: &str) -> Fixture {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "vilan_gitdep_{tag}_{}_{unique}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let home = root.join("home");
        std::fs::create_dir_all(&home).expect("create the scratch home");
        Fixture { root, home }
    }

    /// The git-dependency cache this fixture's `vilan` runs write to.
    fn cache(&self) -> PathBuf {
        self.home.join(".vilan").join("git-deps")
    }

    /// The cache's entry directories (staging leftovers excluded — a name
    /// starting with `.` is never a resolved entry).
    fn cache_entries(&self) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(self.cache()) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| !name.starts_with('.'))
            .collect();
        names.sort();
        names
    }

    /// Runs the built `vilan` against this fixture's HOME. `VILAN_STD` points
    /// at the in-repo std, so the scratch home holds the git cache and nothing
    /// else.
    fn vilan(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_vilan"))
            .args(args)
            .env("HOME", &self.home)
            .env("USERPROFILE", &self.home)
            .env("VILAN_STD", std_dir())
            .output()
            .expect("run vilan")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn std_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std")
}

/// Writes `contents` to `dir/relative`, creating parent directories.
fn write(dir: &Path, relative: &str, contents: &str) {
    let path = dir.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

/// A command's combined stdout + stderr — diagnostics go to stderr, the status
/// line too, and `--stdout` builds put the JS on stdout.
fn combined(output: &Output) -> String {
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    text
}

/// Runs `git` in `directory` with an identity of its own, so the fixture never
/// depends on (or disturbs) the developer's git configuration.
fn git(directory: &Path, args: &[&str]) -> Output {
    let output = Command::new("git")
        .args([
            "-c",
            "user.name=vilan test",
            "-c",
            "user.email=test@vilan.invalid",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(directory)
        .output()
        .expect("git must be installed to run the git-dependency tests");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

/// A `file://` URL for a local directory, in the spelling git wants on this
/// platform (`file:///C:/…` on Windows, `file:///…` on unix).
fn file_url(path: &Path) -> String {
    let text = path.display().to_string().replace('\\', "/");
    if text.starts_with('/') {
        format!("file://{text}")
    } else {
        format!("file:///{text}")
    }
}

/// Creates a git repository at `dir` from `files`, commits it, and tags the
/// commit `tag` (when given). Returns its `file://` URL.
fn repository(dir: &Path, files: &[(&str, &str)], tag: Option<&str>) -> String {
    std::fs::create_dir_all(dir).expect("create the repository directory");
    git(dir, &["init", "--quiet", "."]);
    for (relative, contents) in files {
        write(dir, relative, contents);
    }
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "--quiet", "-m", "fixture"]);
    if let Some(tag) = tag {
        git(dir, &["tag", tag]);
    }
    file_url(dir)
}

/// The commit `HEAD` names in the repository at `dir`.
fn head_sha(dir: &Path) -> String {
    String::from_utf8_lossy(&git(dir, &["rev-parse", "HEAD"]).stdout)
        .trim()
        .to_string()
}

/// The fixture library: one importable function whose body identifies the
/// version, so "which checkout got compiled" is visible in the emitted JS.
fn library_files(greeting: &str) -> Vec<(String, String)> {
    vec![
        (
            "vilan.toml".to_string(),
            "[library]\nname = \"shapes\"\n".to_string(),
        ),
        (
            "src/lib.vl".to_string(),
            format!("fun greeting(): str {{ \"{greeting}\" }}\n"),
        ),
    ]
}

fn as_pairs(files: &[(String, String)]) -> Vec<(&str, &str)> {
    files
        .iter()
        .map(|(name, contents)| (name.as_str(), contents.as_str()))
        .collect()
}

/// Writes an application depending on `shapes` through `declaration`.
fn application(dir: &Path, declaration: &str) {
    write(
        dir,
        "vilan.toml",
        &format!("[package]\nname = \"app\"\n\n[package.dependencies]\nshapes = {declaration}\n"),
    );
    write(
        dir,
        "src/main.vl",
        "import std::print;\nimport shapes::greeting;\n\nfun main() {\n\tprint(greeting())\n}\n",
    );
}

#[test]
fn a_tagged_git_dependency_is_fetched_and_built() {
    let fixture = Fixture::new("tag");
    let files = library_files("hello from v1");
    let url = repository(
        &fixture.root.join("repos/shapes"),
        &as_pairs(&files),
        Some("v1.0.0"),
    );
    let app = fixture.root.join("app");
    application(&app, &format!("{{ git = \"{url}\", tag = \"v1.0.0\" }}"));

    let output = fixture.vilan(&["build", "--stdout", app.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "build failed: {}",
        combined(&output)
    );
    // The dependency was compiled in: its body is in the emitted program.
    let javascript = String::from_utf8_lossy(&output.stdout);
    assert!(
        javascript.contains("hello from v1"),
        "the fetched library should be compiled in: {javascript}"
    );
    // The fetch announced itself — on stderr, so `--stdout` stays pure JS.
    let status = String::from_utf8_lossy(&output.stderr);
    assert!(
        status.contains("fetching shapes") && status.contains("tag `v1.0.0`"),
        "expected a fetch status line on stderr, got: {status}"
    );
    assert!(
        !javascript.contains("fetching shapes"),
        "the status line must never contaminate `--stdout`"
    );
    assert_eq!(
        fixture.cache_entries().len(),
        1,
        "exactly one cache entry: {:?}",
        fixture.cache_entries()
    );
}

#[test]
fn a_rev_pinned_git_dependency_is_fetched_and_built() {
    // The other half of "exactly one of tag|rev": the four-step init/fetch/
    // checkout shape, which is a different code path from `clone --branch`.
    let fixture = Fixture::new("rev");
    let files = library_files("hello from a commit");
    let repository_dir = fixture.root.join("repos/shapes");
    let url = repository(&repository_dir, &as_pairs(&files), None);
    let sha = head_sha(&repository_dir);
    let app = fixture.root.join("app");
    application(&app, &format!("{{ git = \"{url}\", rev = \"{sha}\" }}"));

    let output = fixture.vilan(&["build", "--stdout", app.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "build failed: {}",
        combined(&output)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("hello from a commit"),
        "the commit's library should be compiled in: {}",
        combined(&output)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("rev `"),
        "the status line names the pinned rev: {}",
        combined(&output)
    );
}

#[test]
fn a_warm_cache_serves_the_pinned_content_offline() {
    // The content-addressed claim, tested the only way it can be believed:
    // prime the cache, then MOVE the tag to different content, and finally
    // delete the repository outright. Both later builds must produce the
    // ORIGINAL content — a re-fetch would show the new text, and a build that
    // needed the network at all could not survive the deletion.
    let fixture = Fixture::new("warm");
    let repository_dir = fixture.root.join("repos/shapes");
    let files = library_files("original v1 body");
    let url = repository(&repository_dir, &as_pairs(&files), Some("v1.0.0"));
    let app = fixture.root.join("app");
    application(&app, &format!("{{ git = \"{url}\", tag = \"v1.0.0\" }}"));

    let first = fixture.vilan(&["build", "--stdout", app.to_str().unwrap()]);
    assert!(first.status.success(), "first build: {}", combined(&first));
    assert!(String::from_utf8_lossy(&first.stdout).contains("original v1 body"));
    let entries = fixture.cache_entries();

    // The repository moves its tag onto new content.
    write(
        &repository_dir,
        "src/lib.vl",
        "fun greeting(): str { \"REWRITTEN body\" }\n",
    );
    git(&repository_dir, &["add", "-A"]);
    git(&repository_dir, &["commit", "--quiet", "-m", "rewrite"]);
    git(&repository_dir, &["tag", "-f", "v1.0.0"]);

    let second = fixture.vilan(&["build", "--stdout", app.to_str().unwrap()]);
    assert!(
        second.status.success(),
        "second build: {}",
        combined(&second)
    );
    let javascript = String::from_utf8_lossy(&second.stdout);
    assert!(
        javascript.contains("original v1 body") && !javascript.contains("REWRITTEN body"),
        "the warm cache is authoritative for a pin: {javascript}"
    );
    assert!(
        !String::from_utf8_lossy(&second.stderr).contains("fetching"),
        "a warm cache must not announce (or perform) a fetch: {}",
        combined(&second)
    );
    assert_eq!(entries, fixture.cache_entries(), "no second cache entry");

    // Offline for real: the repository is gone.
    std::fs::remove_dir_all(&repository_dir).expect("delete the source repository");
    let third = fixture.vilan(&["build", "--stdout", app.to_str().unwrap()]);
    assert!(
        third.status.success(),
        "a warm cache must build with no repository at all: {}",
        combined(&third)
    );
    assert!(String::from_utf8_lossy(&third.stdout).contains("original v1 body"));
}

#[test]
fn a_tag_that_does_not_exist_is_a_clean_error() {
    let fixture = Fixture::new("badtag");
    let files = library_files("hello");
    let url = repository(
        &fixture.root.join("repos/shapes"),
        &as_pairs(&files),
        Some("v1.0.0"),
    );
    let app = fixture.root.join("app");
    application(&app, &format!("{{ git = \"{url}\", tag = \"v9.9.9\" }}"));

    let output = fixture.vilan(&["build", app.to_str().unwrap()]);
    assert!(!output.status.success(), "expected a failure");
    let text = combined(&output);
    assert!(
        text.contains("git dependency `shapes`")
            && text.contains("cannot fetch tag `v9.9.9`")
            && text.contains("git said:"),
        "{text}"
    );
    assert!(
        fixture.cache_entries().is_empty(),
        "a failed fetch must leave no cache entry: {:?}",
        fixture.cache_entries()
    );
}

#[test]
fn a_branch_name_smuggled_into_tag_is_caught_after_the_clone() {
    // `git clone --branch` accepts a branch as happily as a tag, so a `tag =
    // "main"` would otherwise pin that branch's TIP — a different commit on
    // another machine, or tomorrow. The manifest cannot tell the two apart
    // (both are just names); the checkout can, and does.
    let fixture = Fixture::new("branchastag");
    let files = library_files("hello");
    let repository_dir = fixture.root.join("repos/shapes");
    let url = repository(&repository_dir, &as_pairs(&files), Some("v1.0.0"));
    let branch =
        String::from_utf8_lossy(&git(&repository_dir, &["symbolic-ref", "--short", "HEAD"]).stdout)
            .trim()
            .to_string();
    let app = fixture.root.join("app");
    application(&app, &format!("{{ git = \"{url}\", tag = \"{branch}\" }}"));

    let output = fixture.vilan(&["build", app.to_str().unwrap()]);
    assert!(!output.status.success(), "expected a failure");
    let text = combined(&output);
    assert!(
        text.contains(&format!("`{branch}` is not a tag"))
            && text.contains("a branch moves")
            && text.contains("rev = \"<commit sha>\""),
        "{text}"
    );
    assert!(
        fixture.cache_entries().is_empty(),
        "a branch tip must never enter the cache: {:?}",
        fixture.cache_entries()
    );
}

#[test]
fn a_rev_that_is_not_in_the_repository_names_the_sha_limitation() {
    // The message covers both ways a rev fetch fails — the commit is absent,
    // or the server refuses to serve a commit by SHA
    // (`uploadpack.allowReachableSHA1InWant`). Only the first is reproducible
    // offline (a `file://` transport always allows it), and both produce this
    // one message, so this is where that wording is pinned.
    let fixture = Fixture::new("badrev");
    let files = library_files("hello");
    let url = repository(&fixture.root.join("repos/shapes"), &as_pairs(&files), None);
    let app = fixture.root.join("app");
    let absent = "0123456789abcdef0123456789abcdef01234567";
    application(&app, &format!("{{ git = \"{url}\", rev = \"{absent}\" }}"));

    let output = fixture.vilan(&["build", app.to_str().unwrap()]);
    assert!(!output.status.success(), "expected a failure");
    let text = combined(&output);
    assert!(
        text.contains(&format!("cannot fetch rev `{absent}`"))
            && text.contains("uploadpack.allowReachableSHA1InWant")
            && text.contains("depend on a `tag` instead"),
        "{text}"
    );
    assert!(fixture.cache_entries().is_empty());
}

#[test]
fn an_unreachable_repository_is_a_clean_error() {
    // Cold cache, nothing to fetch from: the error names the URL and lets git
    // say why, rather than a panic or a stack of resolution noise.
    let fixture = Fixture::new("norepo");
    let url = file_url(&fixture.root.join("repos/absent"));
    let app = fixture.root.join("app");
    application(&app, &format!("{{ git = \"{url}\", tag = \"v1.0.0\" }}"));

    let output = fixture.vilan(&["build", app.to_str().unwrap()]);
    assert!(!output.status.success(), "expected a failure");
    let text = combined(&output);
    assert!(
        text.contains("git dependency `shapes`") && text.contains(&url),
        "the error should name the dependency and its URL: {text}"
    );
    assert!(fixture.cache_entries().is_empty());
}

#[test]
fn a_checkout_without_a_manifest_is_a_clean_error() {
    let fixture = Fixture::new("nomanifest");
    let url = repository(
        &fixture.root.join("repos/shapes"),
        &[("src/lib.vl", "fun greeting(): str { \"hi\" }\n")],
        Some("v1.0.0"),
    );
    let app = fixture.root.join("app");
    application(&app, &format!("{{ git = \"{url}\", tag = \"v1.0.0\" }}"));

    let output = fixture.vilan(&["build", app.to_str().unwrap()]);
    assert!(!output.status.success(), "expected a failure");
    let text = combined(&output);
    assert!(
        text.contains("no `vilan.toml` at its root") && text.contains("`[library]`"),
        "{text}"
    );
    assert!(
        fixture.cache_entries().is_empty(),
        "an unverified checkout must never enter the cache: {:?}",
        fixture.cache_entries()
    );
}

#[test]
fn a_checkout_that_is_a_package_must_be_a_library() {
    // proposal/distribution.md §5: a git dependency is a `[library]`. (A local
    // `path` dependency may still be a `[package]` — that is the blessed
    // client→server service shape, and it is unchanged.)
    let fixture = Fixture::new("packagedep");
    let url = repository(
        &fixture.root.join("repos/shapes"),
        &[
            ("vilan.toml", "[package]\nname = \"shapes\"\n"),
            ("src/main.vl", "fun main() {}\n"),
        ],
        Some("v1.0.0"),
    );
    let app = fixture.root.join("app");
    application(&app, &format!("{{ git = \"{url}\", tag = \"v1.0.0\" }}"));

    let output = fixture.vilan(&["build", app.to_str().unwrap()]);
    assert!(!output.status.success(), "expected a failure");
    let text = combined(&output);
    assert!(
        text.contains("declares a `[package]`") && text.contains("must be a `[library]`"),
        "{text}"
    );
    assert!(fixture.cache_entries().is_empty());
}

#[test]
fn a_git_dependency_of_a_git_dependency_resolves_through_the_same_cache() {
    // v1 ships TRANSITIVE git dependencies: a fetched checkout's own
    // dependencies go down the same resolution loop, so its `git` deps land in
    // the same cache. Nothing special had to be built for this — which is the
    // point of materializing to a path before the graph walk.
    let fixture = Fixture::new("transitive");
    let base_url = repository(
        &fixture.root.join("repos/base"),
        &[
            ("vilan.toml", "[library]\nname = \"base\"\n"),
            ("src/lib.vl", "fun base_value(): str { \"from base\" }\n"),
        ],
        Some("v1.0.0"),
    );
    let shapes_url = repository(
        &fixture.root.join("repos/shapes"),
        &[
            (
                "vilan.toml",
                &format!(
                    "[library]\nname = \"shapes\"\n\n[library.dependencies]\n\
                     base = {{ git = \"{base_url}\", tag = \"v1.0.0\" }}\n"
                ),
            ),
            (
                "src/lib.vl",
                "import base::base_value;\n\nfun greeting(): str { base_value() }\n",
            ),
        ],
        Some("v1.0.0"),
    );
    let app = fixture.root.join("app");
    application(
        &app,
        &format!("{{ git = \"{shapes_url}\", tag = \"v1.0.0\" }}"),
    );

    let output = fixture.vilan(&["build", "--stdout", app.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "build failed: {}",
        combined(&output)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("from base"),
        "the transitive dependency should be compiled in: {}",
        combined(&output)
    );
    assert_eq!(
        fixture.cache_entries().len(),
        2,
        "one entry per repository: {:?}",
        fixture.cache_entries()
    );
}

#[test]
fn a_branch_pin_is_refused_by_the_manifest_before_any_fetch() {
    // The manifest errors render through the ordinary CLI diagnostic path, and
    // they happen BEFORE git is ever run — a branch is refused whether or not
    // the repository exists.
    let fixture = Fixture::new("branch");
    let url = file_url(&fixture.root.join("repos/absent"));
    let app = fixture.root.join("app");
    application(&app, &format!("{{ git = \"{url}\", branch = \"main\" }}"));

    let output = fixture.vilan(&["check", app.to_str().unwrap()]);
    assert!(!output.status.success(), "expected a failure");
    let text = combined(&output);
    assert!(
        text.contains("dependency `shapes` pins the branch `main`")
            && text.contains("tag = \"v1.2.0\"")
            && text.contains("rev = \"<commit sha>\""),
        "{text}"
    );
    assert!(
        !text.contains("git said:"),
        "the manifest error must precede any fetch: {text}"
    );
    assert!(fixture.cache_entries().is_empty());
}
