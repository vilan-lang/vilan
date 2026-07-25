//! Git dependencies — `dep = { git = "…", tag = "v1.2.0" }` (proposal
//! `distribution.md` §5, F5 v1): fetching one immutable point of one repository
//! into a content-addressed cache, after which the checkout is an ordinary
//! **path dependency** as far as the rest of resolution is concerned
//! (`manifest::resolve_dependency_edges`).
//!
//! **Exactly one point, always.** A dependency pins a `tag` or a `rev`, never a
//! branch and never a range: with nothing to resolve there is no resolver, no
//! lockfile, and no "it built yesterday" class of bug. The manifest rejects the
//! other spellings (`manifest::Dependency::source`).
//!
//! **The cache** (`~/.vilan/git-deps/<key>/`) mirrors the std cache next door
//! (`vilan-embedded-std`), which is the same problem — a tree that several
//! processes may want at once:
//!
//! - **Content-addressed.** The key is a fingerprint of the *declaration*
//!   (normalized URL + tag/rev), so an entry can never be stale: a different
//!   pin is a different directory. [`cache_key`] states the normalization.
//! - **Complete by construction.** The fetch lands in a `.staging-…` sibling,
//!   is verified to be a `[library]`, and is then *renamed* into place. Only
//!   whole, verified checkouts are ever visible under a key, so a warm entry
//!   needs no re-validation and concurrent first runs race benignly.
//! - **Never pruned by age.** The std cache prunes week-old entries because a
//!   new binary makes the old std dead, and re-materializing one is free and
//!   offline. A git entry is neither: its key is its content, so it is never
//!   *stale*, and re-fetching one needs the network — an age sweep would delete
//!   exactly the entry that makes the promise below true. Cleanup is therefore
//!   limited to this process's own staging directory. (A staging directory left
//!   behind by a *crashed* run is not swept yet; its natural home is the
//!   upgrade-time sweep that already prunes the std cache.)
//!
//! **Offline with a warm cache works, and nothing ever fetches passively.** A
//! key that is present is used with no network call at all; a key that is
//! missing is fetched only by a build/check of a project that declares the
//! dependency ([`GitPolicy::Fetch`]). The language server passes
//! [`GitPolicy::CacheOnly`] and therefore never touches the network — the
//! editor cannot hang on a repository, and an unfetched dependency simply
//! stays unresolved until a build fetches it.
//!
//! **Shelling to `git`**, as the toolchain already shells to `curl` and `tar`
//! for `vilan upgrade`: no libgit2 in the dependency tree, and the user's own
//! git configuration (credential helpers, `insteadOf` rewrites, SSH keys) is
//! the one that applies. `GIT_TERMINAL_PROMPT=0` keeps a private repository
//! from turning a build into a hanging password prompt.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::manifest::{Manifest, WorkspaceError};

/// The immutable point of a repository a dependency pins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitRef {
    /// `tag = "v1.2.0"` — fetched with `clone --depth 1 --branch <tag>`.
    Tag(String),
    /// `rev = "<sha>"` — fetched with `fetch --depth 1 origin <sha>`.
    Rev(String),
}

impl GitRef {
    /// How the reference reads inside a message: ``tag `v1.2.0` ``.
    pub fn describe(&self) -> String {
        let (kind, value) = self.parts();
        format!("{kind} `{value}`")
    }

    /// `(key, value)` — the manifest key this reference came from and its text.
    fn parts(&self) -> (&'static str, &str) {
        match self {
            GitRef::Tag(tag) => ("tag", tag.as_str()),
            GitRef::Rev(rev) => ("rev", rev.as_str()),
        }
    }
}

/// A resolved git dependency declaration: where from, and which point of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitSource {
    pub url: String,
    pub reference: GitRef,
}

impl GitSource {
    /// How the source reads inside a message: ``file:///repo` (tag `v1`)``.
    pub fn describe(&self) -> String {
        format!("`{}` ({})", self.url, self.reference.describe())
    }
}

/// Whether a caller may fetch a missing dependency, or only read the cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitPolicy {
    /// Fetch on a cache miss — the CLI, which was asked to build.
    Fetch,
    /// Use a warm cache, never touch the network — the language server, which
    /// must not hang the editor on a repository (and must not fetch behind the
    /// user's back while they type).
    CacheOnly,
}

/// How one resolution run handles git dependencies: where the cache lives, what
/// it may do on a miss, and where to announce a fetch (the CLI prints a status
/// line; `vilan-core` never prints).
#[derive(Debug, Clone)]
pub struct GitDeps {
    pub cache_root: PathBuf,
    pub policy: GitPolicy,
    /// Called once per *actual* fetch, just before it starts. A plain `fn`
    /// pointer: the sink is a stream, never captured state.
    pub report: Option<fn(&str)>,
}

impl GitDeps {
    /// Fetch on a miss (a build / check / run / test).
    pub fn fetching(cache_root: impl Into<PathBuf>) -> GitDeps {
        GitDeps {
            cache_root: cache_root.into(),
            policy: GitPolicy::Fetch,
            report: None,
        }
    }

    /// Read the cache, never fetch (the language server, and any resolution
    /// that must stay offline).
    pub fn cache_only(cache_root: impl Into<PathBuf>) -> GitDeps {
        GitDeps {
            cache_root: cache_root.into(),
            policy: GitPolicy::CacheOnly,
            report: None,
        }
    }

    /// Announce each fetch through `report` before it starts.
    pub fn reporting(mut self, report: fn(&str)) -> GitDeps {
        self.report = Some(report);
        self
    }
}

/// A repository URL in the one spelling the cache keys on. The rules, and why
/// each is safe:
///
/// - **Surrounding whitespace** is dropped (a manifest typo, never meaningful).
/// - **Trailing `/`** is dropped, then a **trailing `.git`**, then any `/` that
///   exposed — `…/shapes`, `…/shapes/`, `…/shapes.git` and `…/shapes.git/` are
///   one repository in every git host's addressing.
/// - **Scheme and host are lowercased**; the **path keeps its case**. DNS is
///   case-insensitive, so `HTTPS://GitHub.com` is the same host — but a path is
///   the server's business, and lowercasing it would merge `/Foo` and `/foo`,
///   which on a case-sensitive host are *different repositories*. The cost of
///   the conservative choice is at worst one extra fetch, never wrong content.
/// - Everything else is left alone. In particular an scp-style
///   `git@host:org/repo` is normalized (host lowercased) but is **not** unified
///   with the `ssh://` URL for the same repository: different spellings of a
///   transport are different keys, which costs a duplicate fetch and risks
///   nothing.
pub fn normalize_url(url: &str) -> String {
    let trimmed = url.trim();
    let trimmed = trimmed.trim_end_matches('/');
    let trimmed = trimmed
        .strip_suffix(".git")
        .unwrap_or(trimmed)
        .trim_end_matches('/');
    // The authority is what may be lowercased: `scheme://host` for a URL, the
    // `user@host` prefix of an scp-style address.
    let authority_end = match trimmed.find("://") {
        Some(scheme_end) => trimmed[scheme_end + 3..]
            .find('/')
            .map(|offset| scheme_end + 3 + offset)
            .unwrap_or(trimmed.len()),
        None => trimmed.find(':').unwrap_or(0),
    };
    let (authority, rest) = trimmed.split_at(authority_end);
    format!("{}{rest}", authority.to_ascii_lowercase())
}

/// The cache directory name for `source`: a readable prefix (repository name
/// and reference) plus a fingerprint of the full normalized declaration. The
/// fingerprint is what makes two entries distinct; the prefix is what makes the
/// cache legible when a human looks at `~/.vilan/git-deps/`.
///
/// FNV-1a rather than a cryptographic digest: `vilan-core` carries no hashing
/// dependency, the value must be identical on every platform and every compiler
/// build (`DefaultHasher` guarantees neither), and a collision would have to
/// beat the readable prefix as well — this is a cache key, not a signature.
pub fn cache_key(source: &GitSource) -> String {
    let url = normalize_url(&source.url);
    let (kind, value) = source.reference.parts();
    let identity = format!("{url}\n{kind}={value}");
    format!(
        "{}-{}-{:016x}",
        slug(repository_name(&url), 32),
        slug(value, 24),
        fingerprint(identity.as_bytes())
    )
}

/// Where `source` lives (or would live) in the cache rooted at `cache_root`.
pub fn entry_path(cache_root: &Path, source: &GitSource) -> PathBuf {
    cache_root.join(cache_key(source))
}

/// The checkout directory for `source`, fetching it first when the cache is
/// cold and the policy allows. `label` names the dependency in the status line
/// (messages don't repeat it — the caller prefixes them with the dependency).
///
/// A warm entry short-circuits before anything else: no network, no `git`, not
/// even a directory listing beyond the one `is_dir`.
///
/// The failure is typed ([`WorkspaceError`]) because a cache miss under a
/// cache-only policy is not a *fault*: every manifest is right and one
/// `vilan build` fixes it. Only the caller that knows where it is reporting can
/// pick the severity, and it can only pick it from a kind it is told.
pub fn materialize(
    source: &GitSource,
    config: &GitDeps,
    label: &str,
) -> Result<PathBuf, WorkspaceError> {
    let entry = entry_path(&config.cache_root, source);
    if entry.is_dir() {
        return Ok(entry);
    }
    if config.policy == GitPolicy::CacheOnly {
        return Err(WorkspaceError::Unfetched(format!(
            "{} is not in the local cache, and this command does not fetch — \
             run `vilan build` (or `vilan check`) once to fetch it",
            source.describe()
        )));
    }
    if let Some(report) = config.report {
        report(&format!(
            "fetching {label} — {} {}",
            source.url,
            source.reference.describe()
        ));
    }
    std::fs::create_dir_all(&config.cache_root).map_err(|error| {
        format!(
            "cannot create the git dependency cache at {}: {error}",
            config.cache_root.display()
        )
    })?;
    let staging = config.cache_root.join(format!(
        ".staging-{}-{}",
        cache_key(source),
        std::process::id()
    ));
    // Our own leftovers from a crashed run, under a name only this process
    // uses: `git clone` insists on an empty target.
    let _ = std::fs::remove_dir_all(&staging);
    if let Err(error) = fetch(source, &staging).and_then(|()| verify_library(&staging, source)) {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(WorkspaceError::Broken(error));
    }
    match std::fs::rename(&staging, &entry) {
        Ok(()) => Ok(entry),
        Err(error) => {
            let _ = std::fs::remove_dir_all(&staging);
            if entry.is_dir() {
                // Lost the race with a concurrent fetch of the same key; the
                // winner's checkout is the same content by construction.
                return Ok(entry);
            }
            Err(WorkspaceError::Broken(format!(
                "cannot move the fetched checkout into place at {}: {error}",
                entry.display()
            )))
        }
    }
}

/// Fetches `source` into `staging` (which must not exist), shallowly.
///
/// A **tag** is one `clone --depth 1 --branch <tag>` — `--branch` takes a tag
/// name, and `--depth 1` fetches exactly that commit.
///
/// A **rev** cannot be cloned by name, so it is the four-step shape: `init`,
/// `remote add`, `fetch --depth 1 origin <sha>`, `checkout --detach
/// FETCH_HEAD`. Fetching a bare SHA is a server *option*
/// (`uploadpack.allowReachableSHA1InWant`) that many hosts leave off, so its
/// failure gets a message naming that limitation rather than a raw git error —
/// and no fallback to a full clone, which would silently download a repository
/// of unbounded size.
fn fetch(source: &GitSource, staging: &Path) -> Result<(), String> {
    match &source.reference {
        GitRef::Tag(tag) => {
            let mut command = git();
            command
                .arg("clone")
                .arg("--quiet")
                .arg("--depth")
                .arg("1")
                .arg("--branch")
                .arg(tag)
                .arg("--")
                .arg(&source.url)
                .arg(staging);
            run(command).map_err(|detail| {
                format!(
                    "cannot fetch {} from `{}` — check that the tag exists and the \
                     repository is reachable\n  {detail}",
                    source.reference.describe(),
                    source.url
                )
            })?;
            // `--branch` accepts a BRANCH name too, and would have quietly
            // pinned that branch's tip — a different commit on another machine,
            // or tomorrow. A tag clone leaves `refs/tags/<tag>`; a branch clone
            // leaves `refs/heads/<branch>` and no such tag, which is exactly
            // the question this asks.
            let mut is_a_tag = git();
            is_a_tag
                .arg("-C")
                .arg(staging)
                .arg("rev-parse")
                .arg("--verify")
                .arg("--quiet")
                .arg(format!("refs/tags/{tag}"));
            if succeeds(is_a_tag) {
                return Ok(());
            }
            Err(format!(
                "`{tag}` is not a tag in `{}` — it named a branch, and a branch moves, so \
                 it cannot pin a dependency; use a released tag, or pin the commit with \
                 `rev = \"<commit sha>\"`",
                source.url
            ))
        }
        GitRef::Rev(rev) => {
            let mut initialize = git();
            initialize.arg("init").arg("--quiet").arg(staging);
            run(initialize).map_err(|detail| {
                format!(
                    "cannot prepare a checkout for {}\n  {detail}",
                    source.describe()
                )
            })?;
            let mut remote = git();
            remote
                .arg("-C")
                .arg(staging)
                .arg("remote")
                .arg("add")
                .arg("origin")
                .arg(&source.url);
            run(remote).map_err(|detail| format!("cannot address `{}`\n  {detail}", source.url))?;
            let mut fetch = git();
            fetch
                .arg("-C")
                .arg(staging)
                .arg("fetch")
                .arg("--quiet")
                .arg("--depth")
                .arg("1")
                .arg("origin")
                .arg(rev);
            run(fetch).map_err(|detail| {
                format!(
                    "cannot fetch {} from `{}` — the commit may not exist, or the server \
                     may refuse to serve a commit by SHA (git's \
                     `uploadpack.allowReachableSHA1InWant`, off by default on many hosts); \
                     depend on a `tag` instead\n  {detail}",
                    source.reference.describe(),
                    source.url
                )
            })?;
            let mut checkout = git();
            checkout
                .arg("-C")
                .arg(staging)
                .arg("checkout")
                .arg("--quiet")
                .arg("--detach")
                .arg("FETCH_HEAD");
            run(checkout).map_err(|detail| {
                format!(
                    "cannot check out {} of `{}`\n  {detail}",
                    source.reference.describe(),
                    source.url
                )
            })
        }
    }
}

/// A `git` invocation that can never block on a human: a private repository
/// fails with "authentication required" instead of waiting for a password on a
/// terminal the build may not even have.
fn git() -> Command {
    let mut command = Command::new("git");
    command.env("GIT_TERMINAL_PROMPT", "0");
    command
}

/// Whether `command` exits successfully — a *probe*, whose answer is yes or no
/// rather than a step whose failure is a message.
fn succeeds(mut command: Command) -> bool {
    command
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

/// Runs `command`, capturing its output. `Err` carries a one-line `git said:`
/// digest of stderr — enough to see *why* without pasting a screen of progress.
fn run(mut command: Command) -> Result<(), String> {
    let output = command.output().map_err(|error| {
        format!("cannot run `git`: {error} — a git dependency needs `git` on the PATH")
    })?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!("git said: {}", digest(&output.stderr)))
}

/// The one line of git's stderr worth repeating: its **first** `fatal:` (or
/// `error:`) line, which is the actual failure. Not the last line — git follows
/// a fatal with wrapped prose ("Please make sure you have the correct access
/// rights / and the repository exists."), and quoting its tail hands the user a
/// sentence fragment. A message with no such line falls back to its last
/// non-empty line.
fn digest(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let lines = || text.lines().map(str::trim).filter(|line| !line.is_empty());
    lines()
        .find(|line| line.starts_with("fatal:"))
        .or_else(|| lines().find(|line| line.starts_with("error:")))
        .or_else(|| lines().next_back())
        .unwrap_or("(no output)")
        .to_string()
}

/// The gate on entering the cache: a git dependency is a `[library]` package,
/// with its manifest at the repository root (proposal `distribution.md` §5).
/// Checked on the staging tree, so a wrong repository never becomes a cache
/// entry — and a warm entry is a verified library by construction.
fn verify_library(staging: &Path, source: &GitSource) -> Result<(), String> {
    let manifest_path = staging.join("vilan.toml");
    let Ok(contents) = crate::util::read_source(&manifest_path) else {
        return Err(format!(
            "the checkout of {} has no `vilan.toml` at its root — a git dependency is a \
             vilan `[library]` package, and its manifest is what says so",
            source.describe()
        ));
    };
    let (manifest, _warnings) = Manifest::parse(&contents).map_err(|error| {
        format!(
            "the checkout of {} has an invalid `vilan.toml`: {error}",
            source.describe()
        )
    })?;
    if manifest.library.is_some() {
        return Ok(());
    }
    let declared = if manifest.package.is_some() {
        "a `[package]`"
    } else if manifest.project.is_some() {
        "a `[project]`"
    } else {
        "neither"
    };
    Err(format!(
        "the checkout of {} declares {declared}, but a git dependency must be a `[library]` \
         — an app is not importable, and a workspace root has no source of its own",
        source.describe()
    ))
}

/// The repository's own name — the last path segment of a normalized URL — for
/// the readable half of a cache key.
fn repository_name(normalized_url: &str) -> &str {
    normalized_url
        .rsplit(['/', ':'])
        .next()
        .filter(|segment| !segment.is_empty())
        .unwrap_or("repo")
}

/// `text` reduced to a filename-safe, lowercase slug of at most `limit` bytes.
/// Cosmetic — the fingerprint carries the identity — so a lossy mapping is fine.
fn slug(text: &str, limit: usize) -> String {
    let mut slug = String::new();
    for character in text.chars() {
        if slug.len() >= limit {
            break;
        }
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let trimmed = slug.trim_matches('-');
    if trimmed.is_empty() {
        "repo".to_string()
    } else {
        trimmed.to_string()
    }
}

/// FNV-1a, 64-bit: a fixed, platform-independent, compiler-version-independent
/// fingerprint (see [`cache_key`] for why not `DefaultHasher` and not SHA-256).
fn fingerprint(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Whether `url` is usable as a repository address. Deliberately permissive —
/// git speaks https, ssh, git, file and scp-style, and the user's `insteadOf`
/// rules can rewrite anything — so this rejects only the two spellings that are
/// never an address: empty, and a leading `-` (which `git` would read as an
/// option).
pub fn is_usable_url(url: &str) -> bool {
    let url = url.trim();
    !url.is_empty() && !url.starts_with('-')
}

/// Whether `tag` is usable as a git tag name: non-empty, no leading `-`, and
/// none of the characters git itself forbids in a reference (`git
/// check-ref-format`'s core rules — space, `~^:?*[\`, and control characters).
pub fn is_usable_tag(tag: &str) -> bool {
    !tag.is_empty()
        && !tag.starts_with('-')
        && !tag.contains("..")
        && tag
            .chars()
            .all(|character| !" ~^:?*[\\".contains(character) && !character.is_control())
}

/// Whether `rev` is a commit SHA: 7 to 40 hexadecimal digits (git's own
/// abbreviation range). A `rev` that is really a branch name is thus a clean
/// manifest error rather than a checkout that silently drifts.
pub fn is_commit_sha(rev: &str) -> bool {
    (7..=40).contains(&rev.len()) && rev.chars().all(|character| character.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tagged(url: &str, tag: &str) -> GitSource {
        GitSource {
            url: url.to_string(),
            reference: GitRef::Tag(tag.to_string()),
        }
    }

    #[test]
    fn url_spellings_of_one_repository_collide() {
        // The four spellings every git host treats as one address must be one
        // cache entry — otherwise a `.git` suffix silently doubles the fetches
        // (and the disk) for the same dependency.
        let canonical = cache_key(&tagged("https://example.com/org/shapes", "v1.0.0"));
        for spelling in [
            "https://example.com/org/shapes/",
            "https://example.com/org/shapes.git",
            "https://example.com/org/shapes.git/",
            "  https://example.com/org/shapes  ",
            "HTTPS://Example.COM/org/shapes",
        ] {
            assert_eq!(
                cache_key(&tagged(spelling, "v1.0.0")),
                canonical,
                "`{spelling}` should key the same entry"
            );
        }
    }

    #[test]
    fn distinct_declarations_never_collide() {
        // Each coordinate of the declaration is part of the key: host, path
        // (case included — `/Foo` and `/foo` are different repositories on a
        // case-sensitive host), the reference, and its KIND.
        let base = cache_key(&tagged("https://example.com/org/shapes", "v1.0.0"));
        let others = [
            cache_key(&tagged("https://example.com/org/shapes", "v1.0.1")),
            cache_key(&tagged("https://example.com/other/shapes", "v1.0.0")),
            cache_key(&tagged("https://elsewhere.com/org/shapes", "v1.0.0")),
            cache_key(&tagged("https://example.com/org/Shapes", "v1.0.0")),
            cache_key(&GitSource {
                url: "https://example.com/org/shapes".to_string(),
                reference: GitRef::Rev("1234567".to_string()),
            }),
        ];
        for other in &others {
            assert_ne!(&base, other);
        }
        // ...and they are all distinct from each other, too.
        let mut all = others.to_vec();
        all.push(base);
        all.sort();
        let count = all.len();
        all.dedup();
        assert_eq!(all.len(), count, "every declaration keys its own entry");
    }

    #[test]
    fn an_scp_style_address_lowercases_only_its_host() {
        assert_eq!(
            normalize_url("git@GitHub.com:Org/Shapes.git"),
            "git@github.com:Org/Shapes"
        );
        // ...and it is deliberately NOT unified with the ssh:// spelling.
        assert_ne!(
            cache_key(&tagged("git@github.com:org/shapes", "v1")),
            cache_key(&tagged("ssh://git@github.com/org/shapes", "v1"))
        );
    }

    #[test]
    fn a_cache_key_is_readable_and_filename_safe() {
        let key = cache_key(&tagged("https://example.com/org/shapes", "v1.2.0"));
        assert!(key.starts_with("shapes-v1-2-0-"), "{key}");
        assert!(
            key.chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '-'),
            "{key}"
        );
    }

    #[test]
    fn the_fingerprint_is_a_fixed_constant_not_a_seeded_hash() {
        // A cache key must survive a compiler upgrade: `DefaultHasher` is
        // explicitly allowed to change between Rust releases, which would
        // orphan every warm entry (and quietly re-fetch on every machine).
        // The published FNV-1a-64 vectors (offset basis, and "a"), so a
        // mistyped constant or a changed mixing step is caught here rather
        // than in a mysteriously cold cache.
        assert_eq!(fingerprint(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fingerprint(b"a"), 0xaf63_dc4c_8601_ec8c);
        // ...and the whole key of a fixed declaration, computed independently
        // (FNV-1a over `https://example.com/org/shapes\ntag=v1.2.0`).
        assert_eq!(
            cache_key(&tagged("https://example.com/org/shapes", "v1.2.0")),
            "shapes-v1-2-0-f62914591dc36b41"
        );
    }

    #[test]
    fn a_cache_only_miss_never_touches_the_disk() {
        // The editor's policy: no network, and no cache directory created
        // behind the user's back either.
        let root =
            std::env::temp_dir().join(format!("vilan-git-dep-cacheonly-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let source = tagged("https://example.invalid/org/shapes", "v1.0.0");
        let error = materialize(&source, &GitDeps::cache_only(&root), "shapes")
            .expect_err("a cold cache-only miss is an error");
        assert!(error.is_unfetched(), "a cache miss is not a fault: {error}");
        assert!(
            error.message().contains("not in the local cache"),
            "{error}"
        );
        assert!(error.message().contains("vilan build"), "{error}");
        assert!(!root.exists(), "cache-only must not create the cache root");
    }

    #[test]
    fn reference_shapes_are_checked_before_git_ever_runs() {
        assert!(is_commit_sha("0123456789abcdef"));
        assert!(is_commit_sha("abc1234"));
        assert!(!is_commit_sha("abc123"), "shorter than git's abbreviation");
        assert!(!is_commit_sha("main"));
        assert!(!is_commit_sha(&"a".repeat(41)));
        assert!(is_usable_tag("v1.2.0"));
        assert!(!is_usable_tag("-rf"), "a tag must not read as an option");
        assert!(!is_usable_tag("v1 2"));
        assert!(!is_usable_tag("refs^{}"));
        assert!(is_usable_url("file:///tmp/repo"));
        assert!(!is_usable_url(""));
        assert!(!is_usable_url("--upload-pack=touch"));
    }

    #[test]
    fn a_git_failure_reports_gits_actual_fatal_line() {
        // Progress before it, and — the case that made this a `find` rather
        // than a `last` — git's wrapped advice AFTER it, whose final line
        // ("and the repository exists.") is a sentence fragment on its own.
        assert_eq!(
            digest(
                b"Cloning into 'x'...\nfatal: 'file:///nope' does not appear to be a git \
                  repository\nfatal: Could not read from remote repository.\n\nPlease make \
                  sure you have the correct access rights\nand the repository exists.\n"
            ),
            "fatal: 'file:///nope' does not appear to be a git repository"
        );
        assert_eq!(
            digest(b"warning: Could not find remote branch v9 to clone.\nfatal: Remote branch v9 not found in upstream origin\n"),
            "fatal: Remote branch v9 not found in upstream origin"
        );
        // No `fatal:`/`error:` line at all: the last thing it said.
        assert_eq!(digest(b"something odd\nlast word\n"), "last word");
        assert_eq!(digest(b""), "(no output)");
    }
}
