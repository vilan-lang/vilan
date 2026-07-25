//! The typed `vilan.toml` manifest: a declarative description of a *package* (an
//! app), a *library* (an importable, target-layered unit), or a *project* (a
//! workspace grouping members). Both the `vilan` CLI and the language server parse
//! a manifest through here, so the schema — and its validation — has a single
//! source of truth.
//!
//! P1 makes resolution fully declarative (no inference): a package names its
//! source `root` (default `src`) and `entry` (default `main.vl`, resolved against
//! the root) and its default `target`. The workspace (`[project]`) and dependency
//! schema parse here too, but resolving them across packages is later work — see
//! `proposal/project-model-p1.md`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::analyzer::{Layer, PackageSpec, Workspace};
use crate::git_dep::{GitDeps, GitRef, GitSource};
use crate::options::{BuildOptions, Preset};
use crate::target::{Platform, PlatformPattern};

/// A parsed `vilan.toml`. Exactly one of `[package]` (an app) / `[library]` (an
/// importable library) / `[project]` (a workspace) is present for a current-shape
/// manifest. A `[package]` may declare several build entries with
/// `[entry.<name>]` sections (proposal/platform-coloring.md §4.2) — the
/// single-package full-stack form.
#[derive(Debug, Default, Deserialize)]
pub struct Manifest {
    pub package: Option<Package>,
    pub library: Option<Library>,
    pub project: Option<Project>,
    pub build: Option<Build>,
    /// `[macro]` — expansion budgets (macro-engine.md §5): `fuel` (interpreter
    /// steps per macro run, default 1_000_000) and `depth` (expansion fixpoint
    /// rounds, default 16).
    #[serde(rename = "macro", default)]
    pub macro_: Option<MacroSection>,
    /// `[entry.<name>]` — the package's build entries, each with its own
    /// platform. Empty for the classic single-entry form.
    #[serde(rename = "entry", default)]
    pub entries: BTreeMap<String, EntryDecl>,
    /// The retired `[server]` form — parsed only so `validate` can point its
    /// users at `[entry.server]` instead of an unknown-key shrug.
    pub server: Option<EntrySection>,
    /// The retired `[client]` form (see `server`).
    pub client: Option<EntrySection>,
}

/// The `[macro]` section: per-package expansion budgets.
#[derive(Debug, Default, Deserialize)]
pub struct MacroSection {
    pub fuel: Option<u64>,
    pub depth: Option<u32>,
}

/// A package: a buildable, importable unit.
#[derive(Debug, Deserialize)]
pub struct Package {
    /// How other packages import this one (P2). Required; a valid identifier.
    pub name: Option<String>,
    pub description: Option<String>,
    /// The package's source root, relative to the manifest. Default `src`.
    pub root: Option<PathBuf>,
    /// The `build`/`run` entry, resolved against `root`. Default `main.vl`.
    pub entry: Option<PathBuf>,
    /// The default build platform (`node` / `deno` / `bun` / `browser` / `none`).
    /// Default `node`.
    pub target: Option<String>,
    #[serde(default)]
    pub dependencies: BTreeMap<String, Dependency>,
}

/// A library: an importable unit with a public surface (`lib.vl`) and no app
/// baggage — no `entry`, no single host `target`. It serves every platform by
/// **layering** its source: a base `root` (shared) plus `[library.layer.<name>]`
/// overlays that each declare the platforms they serve (a module there shadows the
/// base for those platforms). See `proposal/platform-model.md`.
#[derive(Debug, Deserialize)]
pub struct Library {
    /// How dependents import this library. Required; a valid identifier.
    pub name: Option<String>,
    pub description: Option<String>,
    /// The base (shared) source root, relative to the manifest. Default `src`.
    pub root: Option<PathBuf>,
    /// Overlay layers, keyed by layer name (`process`, `browser`, …).
    #[serde(default)]
    pub layer: BTreeMap<String, LayerDecl>,
    #[serde(default)]
    pub dependencies: BTreeMap<String, Dependency>,
}

/// One `[library.layer.<name>]`: a source root plus the platform patterns it serves.
#[derive(Debug, Deserialize)]
pub struct LayerDecl {
    /// The layer's source root, relative to the manifest. Defaults to `src/<name>`.
    pub root: Option<PathBuf>,
    /// The platforms this layer serves: `node` / `node:24` / `node:*` / `deno` /
    /// `bun` / `browser`, or a family (`@process`). At least one.
    #[serde(default)]
    pub platform: Vec<String>,
}

impl Library {
    /// The base source root (default `src`).
    pub fn base_root(&self) -> &Path {
        self.root.as_deref().unwrap_or(Path::new("src"))
    }
}

/// A workspace root: a set of member packages plus dependencies they inherit.
#[derive(Debug, Default, Deserialize)]
pub struct Project {
    #[serde(default)]
    pub packages: Vec<PathBuf>,
    #[serde(default)]
    pub dependencies: BTreeMap<String, Dependency>,
}

/// One `[entry.<name>]`: a build entry of a multi-entry package. The name
/// labels its `dist/<name>.js` output.
#[derive(Debug, Default, Deserialize)]
pub struct EntryDecl {
    /// The entry file, resolved against the package `root` (like
    /// `[package] entry`). Default `<name>.vl`.
    pub path: Option<PathBuf>,
    /// The entry's build platform (`node` / `deno` / `bun` / `browser`).
    /// Default `node`.
    pub target: Option<String>,
}

impl EntryDecl {
    /// The entry file relative to the package root (default `<name>.vl`).
    pub fn path(&self, name: &str) -> PathBuf {
        self.path
            .clone()
            .unwrap_or_else(|| PathBuf::from(format!("{name}.vl")))
    }

    /// The declared platform, if any (validated by [`Manifest::validate`]).
    pub fn resolved_target(&self) -> Option<Platform> {
        self.target.as_deref().and_then(|t| Platform::parse(t).ok())
    }
}

/// A retired `[server]` / `[client]` section — kept parseable only so the
/// migration error in [`Manifest::validate`] can name the replacement.
#[derive(Debug, Deserialize)]
pub struct EntrySection {
    pub entry: Option<PathBuf>,
}

/// A dependency: either a bare version string (`dep = "1.2"`, a registry
/// dependency) or the table form (`{ version, registry, path, git, tag, rev }`).
/// A `path` makes it a local *path dependency*, a `git` a *git dependency*
/// (proposal `distribution.md` §5); with neither it is a *registry dependency*,
/// which nothing resolves yet.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Dependency {
    Version(String),
    Detailed {
        version: Option<String>,
        registry: Option<String>,
        path: Option<PathBuf>,
        /// The repository URL of a git dependency.
        git: Option<String>,
        /// The tag it pins (exactly one of `tag` / `rev`).
        tag: Option<String>,
        /// The commit SHA it pins (exactly one of `tag` / `rev`).
        rev: Option<String>,
        /// Parsed only to reject it: a branch moves, so it cannot pin anything
        /// (the error steers to `tag`/`rev`).
        branch: Option<String>,
    },
}

/// What a dependency resolves against — the three kinds [`Dependency::source`]
/// distinguishes once, for both validation and resolution.
pub enum DependencySource<'declaration> {
    /// A local directory, relative to the declaring manifest.
    Path(&'declaration Path),
    /// One immutable point of one repository, materialized into the git cache
    /// and then treated exactly like a path dependency.
    Git(GitSource),
    /// A bare version / `registry` dependency: parsed, never resolved.
    Registry,
}

impl Dependency {
    /// The local path, if this is a path dependency.
    pub fn path(&self) -> Option<&Path> {
        match self {
            Dependency::Detailed {
                path: Some(path), ..
            } => Some(path),
            _ => None,
        }
    }

    /// What this dependency resolves against, or the reason its declaration is
    /// not a dependency at all. One place decides, so `validate` and
    /// `resolve_dependency_edges` can never disagree about a manifest.
    ///
    /// The messages name the mistake and the fix; the caller prefixes them with
    /// the dependency's name (the manifest keys are per-dependency).
    pub fn source(&self) -> Result<DependencySource<'_>, String> {
        let Dependency::Detailed {
            version,
            registry,
            path,
            git,
            tag,
            rev,
            branch,
        } = self
        else {
            // A bare version string.
            return Ok(DependencySource::Registry);
        };
        let Some(url) = git else {
            // A git key without `git` is a declaration that does nothing — the
            // one thing a manifest error is for.
            for (key, value) in [("tag", tag), ("rev", rev), ("branch", branch)] {
                if value.is_some() {
                    return Err(format!(
                        "declares `{key}` without `git` — `{key}` only means something on a \
                         git dependency, so add `git = \"<repository url>\"` or drop `{key}`"
                    ));
                }
            }
            return Ok(match path {
                Some(path) => DependencySource::Path(path),
                None => DependencySource::Registry,
            });
        };
        if path.is_some() {
            return Err(
                "sets both `git` and `path` — a dependency is either a local directory \
                 or a repository checkout, not both"
                    .to_string(),
            );
        }
        if version.is_some() || registry.is_some() {
            return Err(
                "sets `version`/`registry` alongside `git` — a git dependency is pinned by \
                 its `tag`/`rev`; there is no version resolution to take part in"
                    .to_string(),
            );
        }
        if let Some(branch) = branch {
            return Err(format!(
                "pins the branch `{branch}` — a branch moves, so it cannot pin anything; \
                 use `tag = \"v1.2.0\"` for a release or `rev = \"<commit sha>\"` for an \
                 exact commit"
            ));
        }
        if !crate::git_dep::is_usable_url(url) {
            return Err(format!(
                "has `git = \"{url}\"`, which is not a repository URL (it cannot be empty \
                 or start with `-`)"
            ));
        }
        let reference = match (tag, rev) {
            (Some(tag), None) => {
                if !crate::git_dep::is_usable_tag(tag) {
                    return Err(format!(
                        "has `tag = \"{tag}\"`, which is not a usable git tag name (no \
                         leading `-`, no whitespace, none of `~^:?*[\\`)"
                    ));
                }
                GitRef::Tag(tag.clone())
            }
            (None, Some(rev)) => {
                if !crate::git_dep::is_commit_sha(rev) {
                    return Err(format!(
                        "has `rev = \"{rev}\"`, which is not a commit SHA (7 to 40 \
                         hexadecimal digits) — a branch or tag name goes in `tag`"
                    ));
                }
                GitRef::Rev(rev.clone())
            }
            (Some(_), Some(_)) => {
                return Err(
                    "sets both `tag` and `rev` — a git dependency pins exactly one point, \
                     so pick the release (`tag`) or the commit (`rev`)"
                        .to_string(),
                );
            }
            (None, None) => {
                return Err(
                    "has a `git` URL but no point to pin — add `tag = \"v1.2.0\"` or \
                     `rev = \"<commit sha>\"` (there are no version ranges to fall back on)"
                        .to_string(),
                );
            }
        };
        Ok(DependencySource::Git(GitSource {
            url: url.clone(),
            reference,
        }))
    }
}

/// The `[build]` code-generation knobs, deserialized before resolving against a
/// preset (see [`Manifest::build_options`]).
#[derive(Debug, Default, Deserialize)]
pub struct Build {
    pub preset: Option<String>,
    pub indent: Option<bool>,
    pub spaces: Option<bool>,
    #[serde(rename = "readable-names")]
    pub readable_names: Option<bool>,
    #[serde(rename = "debug-names")]
    pub debug_names: Option<bool>,
}

impl Package {
    /// The source root (default `src`).
    pub fn root(&self) -> &Path {
        self.root.as_deref().unwrap_or(Path::new("src"))
    }

    /// The entry file name, relative to the root (default `main.vl`).
    pub fn entry(&self) -> &Path {
        self.entry.as_deref().unwrap_or(Path::new("main.vl"))
    }

    /// The declared platform, if any (validated by [`Manifest::validate`]).
    pub fn resolved_target(&self) -> Option<Platform> {
        self.target.as_deref().and_then(|t| Platform::parse(t).ok())
    }
}

impl Manifest {
    /// Parses `vilan.toml` text into the typed schema. Returns the manifest plus
    /// non-fatal warnings (e.g. unknown top-level keys, which a forward-compatible
    /// reader ignores rather than rejects). Structural / type errors are `Err`.
    pub fn parse(text: &str) -> Result<(Manifest, Vec<String>), String> {
        // A leading BOM is an encoding marker, not TOML (`windows-support.md`
        // §2) — a Windows editor's default "UTF-8 with BOM". This is the choke
        // point every manifest read goes through, so stripping here covers all
        // of them, and it matches `util::read_source`: no reader in the
        // toolchain hands a BOM to a parser. MEASURED: `toml` 0.8 already
        // tolerates a leading BOM, so this is the guarantee rather than a fix
        // for an observed failure — the pins below are guards.
        let text = crate::util::strip_bom(text);
        let manifest: Manifest = toml::from_str(text).map_err(|error| error.to_string())?;
        // Unknown top-level keys are ignored (forward-compat), but worth flagging
        // so a typo doesn't silently do nothing. A second, untyped parse keeps the
        // typed deserialize free of a catch-all field.
        let table: toml::Table = toml::from_str(text).map_err(|error| error.to_string())?;
        const KNOWN: &[&str] = &[
            "package", "library", "project", "build", "macro", "entry", "server", "client",
        ];
        let warnings = table
            .keys()
            .filter(|key| !KNOWN.contains(&key.as_str()))
            .map(|key| format!("unknown `vilan.toml` key `{key}` (ignored)"))
            .collect();
        Ok((manifest, warnings))
    }

    /// Validates the schema, returning a (possibly empty) list of error messages.
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();

        // The old `[server]`/`[client]` full-stack pair is retired; its
        // replacement is per-entry targets in one `[package]`
        // (proposal/platform-coloring.md §4.2).
        if self.server.is_some() || self.client.is_some() {
            errors.push(
                "the `[server]`/`[client]` form was removed — declare a `[package]` \
                 with `[entry.server]` / `[entry.client]` sections instead (each \
                 takes an optional `path` and `target`)"
                    .to_string(),
            );
        }

        // Exactly one of `[package]` (app) / `[library]` / `[project]` (workspace).
        let kinds = self.package.is_some() as u8
            + self.library.is_some() as u8
            + self.project.is_some() as u8;
        if kinds > 1 {
            errors.push(
                "set exactly one of `[package]`, `[library]`, or `[project]` — an app, a \
                 library, and a workspace root are different manifests"
                    .to_string(),
            );
        } else if kinds == 0 && self.server.is_none() && self.client.is_none() {
            errors.push(
                "`vilan.toml` must declare a `[package]`, `[library]`, or `[project]`".to_string(),
            );
        }

        if let Some(package) = &self.package {
            self.validate_package(package, &mut errors);
        }
        self.validate_entries(&mut errors);
        if let Some(library) = &self.library {
            self.validate_library(library, &mut errors);
        }
        for dependencies in [
            self.package.as_ref().map(|p| &p.dependencies),
            self.library.as_ref().map(|l| &l.dependencies),
            self.project.as_ref().map(|p| &p.dependencies),
        ]
        .into_iter()
        .flatten()
        {
            validate_dependencies(dependencies, &mut errors);
        }
        if let Some(build) = &self.build {
            if let Some(preset) = &build.preset {
                if Preset::parse(preset).is_none() {
                    errors.push(format!(
                        "unknown build preset `{preset}` (expected `debug` or `release`)"
                    ));
                }
            }
        }
        errors
    }

    fn validate_package(&self, package: &Package, errors: &mut Vec<String>) {
        match &package.name {
            None => errors.push("`[package]` is missing a `name`".to_string()),
            Some(name) if !is_identifier(name) => errors.push(format!(
                "`[package] name` must be a valid identifier (got `{name}`)"
            )),
            Some(_) => {}
        }
        if let Some(target) = &package.target {
            if let Err(error) = Platform::parse(target) {
                errors.push(format!("invalid `[package] target`: {error}"));
            }
        }
    }

    /// Validates `[entry.<name>]` sections: they belong to a `[package]`, they
    /// replace (not combine with) the single-entry `entry`/`target` keys, each
    /// name labels a `dist/<name>.js` output, and each path stays inside the
    /// package root.
    fn validate_entries(&self, errors: &mut Vec<String>) {
        if self.entries.is_empty() {
            return;
        }
        if self.package.is_none() {
            errors.push(
                "`[entry.<name>]` sections require a `[package]` (a library has \
                 no entries; a workspace's entries live in its member packages)"
                    .to_string(),
            );
        }
        if let Some(package) = &self.package {
            if package.entry.is_some() || package.target.is_some() {
                errors.push(
                    "`[package] entry`/`target` can't be combined with \
                     `[entry.<name>]` sections — with multiple entries, each \
                     declares its own `path` and `target`"
                        .to_string(),
                );
            }
        }
        for (name, entry) in &self.entries {
            if !is_identifier(name) {
                errors.push(format!(
                    "`[entry.{name}]` — an entry name must be a valid identifier \
                     (it names the `dist/{name}.js` output)"
                ));
            }
            if let Some(target) = &entry.target {
                match Platform::parse(target) {
                    Err(error) => errors.push(format!("invalid `[entry.{name}] target`: {error}")),
                    // An entry is something to build and run; `none` is the
                    // pure-library platform and would build nothing.
                    Ok(Platform::None) => errors.push(format!(
                        "`[entry.{name}] target` must be a host platform \
                         (`node`/`deno`/`bun`/`browser`), not `none`"
                    )),
                    Ok(_) => {}
                }
            }
            if let Some(path) = &entry.path {
                let escapes = path.is_absolute()
                    || path
                        .components()
                        .any(|component| matches!(component, std::path::Component::ParentDir));
                if escapes {
                    errors.push(format!(
                        "`[entry.{name}] path` must be relative to the package \
                         root and free of `..` (got `{}`)",
                        path.display()
                    ));
                }
            }
        }
    }

    fn validate_library(&self, library: &Library, errors: &mut Vec<String>) {
        match &library.name {
            None => errors.push("`[library]` is missing a `name`".to_string()),
            Some(name) if !is_identifier(name) => errors.push(format!(
                "`[library] name` must be a valid identifier (got `{name}`)"
            )),
            Some(_) => {}
        }
        for (name, layer) in &library.layer {
            if layer.platform.is_empty() {
                errors.push(format!(
                    "`[library.layer.{name}]` must declare the platforms it serves \
                     (e.g. `platform = [\"@process\"]`)"
                ));
            }
            for token in &layer.platform {
                if PlatformPattern::parse(token).is_none() {
                    errors.push(format!(
                        "`[library.layer.{name}]` has an unknown platform `{token}` \
                         (expected `node`/`node:24`/`deno`/`bun`/`browser`, or `@process`)"
                    ));
                }
            }
        }
    }

    /// Resolves the `[build]` options: a `preset` (default `debug`) initializes
    /// every option, then individual keys override it.
    pub fn build_options(&self) -> Result<BuildOptions, String> {
        let Some(build) = &self.build else {
            return Ok(BuildOptions::default());
        };
        let mut options = match &build.preset {
            Some(name) => BuildOptions::from_preset(Preset::parse(name).ok_or_else(|| {
                format!("unknown build preset `{name}` (expected `debug` or `release`)")
            })?),
            None => BuildOptions::default(),
        };
        options.indent = build.indent.unwrap_or(options.indent);
        options.spaces = build.spaces.unwrap_or(options.spaces);
        options.readable_names = build.readable_names.unwrap_or(options.readable_names);
        options.debug_names = build.debug_names.unwrap_or(options.debug_names);
        Ok(options)
    }
}

/// Checks every dependency declaration: a malformed git dependency names its
/// mistake, and a registry dependency is still unsupported. Reported as errors
/// so a declared dependency is never silently ignored.
fn validate_dependencies(dependencies: &BTreeMap<String, Dependency>, errors: &mut Vec<String>) {
    for (name, dependency) in dependencies {
        match dependency.source() {
            Ok(DependencySource::Path(_)) | Ok(DependencySource::Git(_)) => {}
            Ok(DependencySource::Registry) => errors.push(format!(
                "registry dependency `{name}` is not yet supported (a dependency is a \
                 local `path` or a `git` repository today)"
            )),
            Err(problem) => errors.push(format!("dependency `{name}` {problem}")),
        }
    }
}

/// Resolves the dependency [`Workspace`] for the package rooted at `package_dir`
/// (P2): every reachable `path` and `git` dependency, transitively, with cycle
/// detection. Each `PackageSpec` records its declared `target`, but the graph
/// itself is target-independent — the target-compatibility *diagnostic* is the
/// analyzer's, reported at the import (P3). A directory whose manifest declares no
/// `[package]` (and a bare file, which has no manifest) yields an empty workspace.
/// Shared by the CLI and the language server so both resolve imports identically.
///
/// `git` decides what a git dependency may do here — fetch on a cache miss (a
/// build) or read a warm cache only (the editor; see [`GitDeps`]). It is an
/// explicit parameter precisely so every caller states its policy.
pub fn resolve_workspace(package_dir: &Path, git: &GitDeps) -> Result<Workspace, String> {
    let manifest = load_manifest(package_dir)?;
    let defaults = crate::macros::MacroLimits::default();
    let macro_limits = manifest
        .macro_
        .as_ref()
        .map(|section| crate::macros::MacroLimits {
            fuel: section.fuel.unwrap_or(defaults.fuel),
            depth: section.depth.unwrap_or(defaults.depth),
        })
        .unwrap_or(defaults);
    let Some(package) = manifest.package else {
        return Ok(Workspace {
            macro_limits,
            ..Workspace::default()
        });
    };
    let mut packages = Vec::new();
    let mut index_by_path = HashMap::new();
    let mut visiting = HashSet::new();
    let entry_dependencies = resolve_dependency_edges(
        &package.dependencies,
        package_dir,
        &mut packages,
        &mut index_by_path,
        &mut visiting,
        git,
    )?;
    Ok(Workspace {
        packages,
        entry_dependencies,
        macro_limits,
    })
}

/// Resolves a `[library]`'s layered [`PackageSpec`] from its package directory `dir`
/// (with its `vilan.toml`). A directory with no manifest is a base-only library (its
/// own `dir` is the base layer). Dependency edges are left empty — this resolves the
/// library's *own* layer structure (for `std`, and for the platform contract check),
/// not a full dependency build.
pub fn resolve_library(dir: &Path) -> PackageSpec {
    if let Ok(contents) = std::fs::read_to_string(dir.join("vilan.toml")) {
        if let Ok((manifest, _)) = Manifest::parse(&contents) {
            if let Some(library) = manifest.library {
                return library_spec(dir, &library, Vec::new());
            }
        }
    }
    PackageSpec {
        base_root: dir.to_path_buf(),
        layers: Vec::new(),
        dependencies: Vec::new(),
        surface: true,
    }
}

/// Resolves the `std` library's spec — `std` is just a library, so this is
/// [`resolve_library`] at the std package directory.
///
/// Forgives the common mis-configuration of pointing `VILAN_STD` /
/// `vilan.stdPath` at the SOURCE root (`.../std/src`) instead of the package
/// directory: when the given directory has no manifest but its parent is a
/// `[library]`, the parent is resolved. Without this, the bare-source
/// fallback has no platform layers, so every layered module (`std::ui`,
/// `std::rpc_server`, ...) silently fails to resolve — a wall of import
/// errors instead of one fixable mistake.
pub fn resolve_std(std_dir: &Path) -> PackageSpec {
    if !std_dir.join("vilan.toml").exists() {
        if let Some(parent) = std_dir.parent() {
            let is_library = std::fs::read_to_string(parent.join("vilan.toml"))
                .ok()
                .and_then(|contents| Manifest::parse(&contents).ok())
                .is_some_and(|(manifest, _)| manifest.library.is_some());
            if is_library {
                return resolve_library(parent);
            }
        }
    }
    resolve_library(std_dir)
}

/// Builds a [`PackageSpec`] for the `[library]` rooted at `dir`: its base root
/// (default `src`) plus each declared layer (root default `src/<name>`, with the
/// platform patterns it serves), and the already-resolved dependency edges.
fn library_spec(dir: &Path, library: &Library, dependencies: Vec<(String, usize)>) -> PackageSpec {
    let layers = library
        .layer
        .iter()
        .map(|(name, decl)| {
            let root = decl
                .root
                .clone()
                .unwrap_or_else(|| PathBuf::from("src").join(name));
            let patterns = decl
                .platform
                .iter()
                .filter_map(|token| PlatformPattern::parse(token))
                .flatten()
                .collect();
            Layer {
                name: name.clone(),
                patterns,
                root: dir.join(root),
            }
        })
        .collect();
    PackageSpec {
        base_root: dir.join(library.base_root()),
        layers,
        dependencies,
        surface: true,
    }
}

/// Reads, parses, and validates the `vilan.toml` in `directory` (for dependency
/// resolution — warnings are the front-end's concern and are dropped here).
fn load_manifest(directory: &Path) -> Result<Manifest, String> {
    let manifest_path = directory.join("vilan.toml");
    let contents = std::fs::read_to_string(&manifest_path)
        .map_err(|error| format!("cannot read {}: {error}", manifest_path.display()))?;
    let (manifest, _warnings) = Manifest::parse(&contents)
        .map_err(|error| format!("invalid {}: {error}", manifest_path.display()))?;
    let errors = manifest.validate();
    if !errors.is_empty() {
        return Err(format!(
            "invalid {}:\n  - {}",
            manifest_path.display(),
            errors.join("\n  - ")
        ));
    }
    Ok(manifest)
}

/// Resolves one package's dependency edges to `(import name, index)` pairs,
/// loading each referenced package (transitively) into `packages`. `index_by_path`
/// dedups a shared dependency; `visiting` is the in-progress stack for cycle
/// detection. Paths are relative to `base_dir` (the depending package's directory).
///
/// A **git** dependency is materialized into the cache *here* and then continues
/// down the same path as a local directory — which is why a git dependency
/// needs no machinery of its own: it dedups, cycle-detects, layers and recurses
/// (into its own `path` **and** `git` dependencies) exactly like a path
/// dependency, through this one loop.
fn resolve_dependency_edges(
    dependencies: &BTreeMap<String, Dependency>,
    base_dir: &Path,
    packages: &mut Vec<PackageSpec>,
    index_by_path: &mut HashMap<PathBuf, usize>,
    visiting: &mut HashSet<PathBuf>,
    git: &GitDeps,
) -> Result<Vec<(String, usize)>, String> {
    let mut edges = Vec::new();
    for (import_name, dependency) in dependencies {
        // `validate` has already rejected registry dependencies and malformed
        // git ones, so only resolvable declarations reach here.
        let dependency_dir = match dependency.source() {
            Ok(DependencySource::Path(relative)) => base_dir.join(relative),
            Ok(DependencySource::Git(source)) => {
                crate::git_dep::materialize(&source, git, import_name)
                    .map_err(|error| format!("git dependency `{import_name}`: {error}"))?
            }
            Ok(DependencySource::Registry) | Err(_) => continue,
        };
        // The dedup map's key AND the cycle set's member. Both must be one
        // canonical form (`windows-support.md` §5): a raw-string fallback lets
        // `../lib` and `../lib/.` key two entries for one package, and Windows'
        // `\\?\` prefix would do the same for a mix of on-disk and not-yet-on-disk
        // directories — duplicating a package, or hiding a cycle.
        let canonical = crate::util::canonical_path(&dependency_dir);
        if let Some(&index) = index_by_path.get(&canonical) {
            edges.push((import_name.clone(), index));
            continue;
        }
        if !visiting.insert(canonical.clone()) {
            return Err(format!(
                "dependency cycle through `{}`",
                dependency_dir.display()
            ));
        }
        let manifest = load_manifest(&dependency_dir)
            .map_err(|error| format!("dependency `{import_name}`: {error}"))?;
        // A dependency is a `[library]` (layered, contract-checked, with a
        // `lib.vl` surface) — or, since platform coloring, a `[package]` (an
        // app): its `src/` modules import by path, its items color
        // inferentially, and reaching an off-platform function is the
        // analyzer's chain diagnostic. This is the blessed client→server
        // service shape (proposal/platform-coloring.md §7.3).
        let (library, package_dependencies) = match (&manifest.library, &manifest.package) {
            (Some(_), _) => (Some(manifest.library.unwrap()), None),
            (None, Some(package)) => (None, Some(package.dependencies.clone())),
            (None, None) => {
                return Err(format!(
                    "dependency `{import_name}` at `{}` is not a `[library]` or `[package]`",
                    dependency_dir.display()
                ));
            }
        };
        // Resolve the library's own dependencies first, so they take lower indices
        // (a valid load order), then record the library itself. Its layered roots (a
        // base plus each declared per-target overlay) come from `library_spec`; a
        // target-specific module being unavailable for the build target is the
        // analyzer's per-module diagnostic at the import (L1), not a resolution error.
        let own_dependencies = library
            .as_ref()
            .map(|library| library.dependencies.clone())
            .or(package_dependencies)
            .unwrap_or_default();
        let dependency_edges = resolve_dependency_edges(
            &own_dependencies,
            &dependency_dir,
            packages,
            index_by_path,
            visiting,
            git,
        )?;
        visiting.remove(&canonical);
        let index = packages.len();
        let spec = match &library {
            Some(library) => library_spec(&dependency_dir, library, dependency_edges),
            // A `[package]` dependency: base-only over its `src/`, no layers,
            // no `lib.vl` surface.
            None => PackageSpec {
                base_root: dependency_dir.join("src"),
                layers: Vec::new(),
                dependencies: dependency_edges,
                surface: false,
            },
        };
        packages.push(spec);
        index_by_path.insert(canonical, index);
        edges.push((import_name.clone(), index));
    }
    Ok(edges)
}

/// Whether `name` is a valid Vilan identifier: a leading letter or `_`, then
/// letters, digits, or `_`.
fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_alphabetic() || first == '_')
        && chars.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Manifest {
        Manifest::parse(text).expect("parses").0
    }

    /// A `{ path = "…" }` declaration, built directly (no TOML round-trip).
    fn path_dependency(path: &str) -> Dependency {
        Dependency::Detailed {
            version: None,
            registry: None,
            path: Some(PathBuf::from(path)),
            git: None,
            tag: None,
            rev: None,
            branch: None,
        }
    }

    /// The single dependency of a `[package]` manifest whose `[package.dependencies]`
    /// section is `shapes = <declaration>`.
    fn dependency_declaration(declaration: &str) -> (Manifest, Vec<String>) {
        let manifest = parse(&format!(
            "[package]\nname = \"app\"\n[package.dependencies]\nshapes = {declaration}\n"
        ));
        let errors = manifest.validate();
        (manifest, errors)
    }

    /// The declared dependency's resolved source (panicking if it is malformed).
    fn source_of(manifest: &Manifest) -> DependencySource<'_> {
        manifest.package.as_ref().unwrap().dependencies["shapes"]
            .source()
            .expect("a well-formed declaration")
    }

    #[test]
    fn a_byte_order_marked_manifest_parses_like_its_clean_twin() {
        // "UTF-8 with BOM" is a Windows editor default. A GUARD, not a
        // discriminator: `toml` 0.8 happens to tolerate a leading BOM today
        // (measured), so this pins the guarantee against a parser that does
        // not — and against the strip being dropped.
        let clean = "[package]\nname = \"web\"\ntarget = \"browser\"\n";
        let marked = format!("\u{feff}{clean}");
        let (clean_manifest, clean_warnings) = Manifest::parse(clean).expect("the clean twin");
        let (marked_manifest, marked_warnings) =
            Manifest::parse(&marked).expect("a BOM'd manifest parses");
        assert_eq!(
            marked_manifest.package.as_ref().map(|p| p.name.clone()),
            clean_manifest.package.as_ref().map(|p| p.name.clone())
        );
        assert_eq!(
            marked_manifest
                .package
                .as_ref()
                .and_then(|p| p.resolved_target()),
            Some(Platform::Browser)
        );
        assert_eq!(marked_warnings, clean_warnings);
        assert!(marked_manifest.validate().is_empty());
    }

    #[test]
    fn an_interior_feff_is_still_a_syntax_error() {
        // Only offset 0 is an encoding marker; the strip must not swallow one
        // that appears as content.
        assert!(Manifest::parse("[package]\n\u{feff}name = \"web\"\n").is_err());
    }

    #[test]
    fn one_dependency_reached_by_two_spellings_is_one_package() {
        // The dedup map's key and the cycle set's member go through
        // `util::canonical_path` (windows-support.md §5). Before that, `../lib`
        // and `../lib/.` keyed two entries for one package — and on Windows a
        // `\\?\` mix would do the same.
        let root = std::env::temp_dir().join(format!("vilan-dep-dedup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let library = root.join("lib");
        std::fs::create_dir_all(library.join("src")).expect("create the library");
        std::fs::write(library.join("vilan.toml"), "[library]\nname = \"lib\"\n")
            .expect("manifest");
        std::fs::write(library.join("src").join("lib.vl"), "").expect("lib.vl");
        let application = root.join("app");
        std::fs::create_dir_all(application.join("src")).expect("create the app");

        let mut packages = Vec::new();
        let mut index_by_path = HashMap::new();
        let mut visiting = HashSet::new();
        let mut dependencies = BTreeMap::new();
        dependencies.insert("plain".to_string(), path_dependency("../lib"));
        dependencies.insert("roundabout".to_string(), path_dependency("../lib/./../lib"));
        let edges = resolve_dependency_edges(
            &dependencies,
            &application,
            &mut packages,
            &mut index_by_path,
            &mut visiting,
            &GitDeps::cache_only(root.join("unused-git-cache")),
        )
        .expect("both spellings resolve");
        assert_eq!(packages.len(), 1, "one package, not one per spelling");
        assert_eq!(edges.len(), 2);
        assert_eq!(edges[0].1, edges[1].1, "both edges point at the same index");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn package_defaults() {
        let manifest = parse("[package]\nname = \"math\"\n");
        let package = manifest.package.as_ref().unwrap();
        assert_eq!(package.root(), Path::new("src"));
        assert_eq!(package.entry(), Path::new("main.vl"));
        assert_eq!(package.resolved_target(), None);
        assert!(manifest.validate().is_empty());
    }

    #[test]
    fn package_explicit_fields() {
        let manifest = parse(
            "[package]\nname = \"web\"\nroot = \"source\"\nentry = \"app.vl\"\ntarget = \"browser\"\n",
        );
        let package = manifest.package.as_ref().unwrap();
        assert_eq!(package.root(), Path::new("source"));
        assert_eq!(package.entry(), Path::new("app.vl"));
        assert_eq!(package.resolved_target(), Some(Platform::Browser));
        assert!(manifest.validate().is_empty());
    }

    #[test]
    fn missing_name_is_an_error() {
        let manifest = parse("[package]\ntarget = \"node\"\n");
        assert!(manifest.validate().iter().any(|e| e.contains("name")));
    }

    #[test]
    fn non_identifier_name_is_an_error() {
        let manifest = parse("[package]\nname = \"my-pkg\"\n");
        assert!(manifest.validate().iter().any(|e| e.contains("identifier")));
    }

    #[test]
    fn unknown_target_is_an_error() {
        let manifest = parse("[package]\nname = \"x\"\ntarget = \"nodejs\"\n");
        assert!(manifest.validate().iter().any(|e| e.contains("target")));
    }

    #[test]
    fn target_none_is_valid() {
        let manifest = parse("[package]\nname = \"common\"\ntarget = \"none\"\n");
        assert_eq!(
            manifest.package.as_ref().unwrap().resolved_target(),
            Some(Platform::None)
        );
        assert!(manifest.validate().is_empty());
    }

    #[test]
    fn deno_target_is_valid() {
        let manifest = parse("[package]\nname = \"svc\"\ntarget = \"deno\"\n");
        assert_eq!(
            manifest.package.as_ref().unwrap().resolved_target(),
            Platform::parse("deno").ok()
        );
        assert!(manifest.validate().is_empty());
    }

    #[test]
    fn library_layer_serving_deno_is_valid() {
        let manifest =
            parse("[library]\nname = \"x\"\n[library.layer.deno]\nplatform = [\"deno\"]\n");
        assert!(manifest.validate().is_empty());
    }

    /// Whether any layer in `spec` serves a platform matching `pattern`.
    fn serves(spec: &PackageSpec, pattern: PlatformPattern) -> bool {
        spec.layers
            .iter()
            .any(|layer| layer.patterns.iter().any(|p| *p == pattern))
    }

    #[test]
    fn resolve_std_reads_manifest_layers() {
        // The real `std` library declares `process`/`browser` layers in its manifest.
        let std = resolve_std(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std"));
        assert!(std.base_root.ends_with("std/src"));
        assert!(serves(&std, PlatformPattern::Node { version: None }));
        assert!(serves(&std, PlatformPattern::Browser));
    }

    #[test]
    fn resolve_std_without_manifest_is_base_only() {
        // Pointing at a bare source root whose parent is NOT a library (a
        // truly orphan directory) yields a base-only library: the directory
        // is the base layer and there are no platform overlays. (A source
        // root INSIDE a real library package is forgiven up to the package —
        // see `resolve_std_forgives_a_source_root_path`.)
        let orphan = std::env::temp_dir().join("vilan_manifest_orphan_std");
        let _ = std::fs::create_dir_all(&orphan);
        let std = resolve_std(&orphan);
        assert_eq!(std.base_root, orphan);
        assert!(std.layers.is_empty());
    }

    #[test]
    fn package_and_project_are_mutually_exclusive() {
        let manifest = parse("[package]\nname = \"x\"\n[project]\npackages = []\n");
        assert!(
            manifest
                .validate()
                .iter()
                .any(|e| e.contains("exactly one"))
        );
    }

    #[test]
    fn package_and_library_are_mutually_exclusive() {
        let manifest = parse("[package]\nname = \"x\"\n[library]\nname = \"y\"\n");
        assert!(
            manifest
                .validate()
                .iter()
                .any(|e| e.contains("exactly one"))
        );
    }

    #[test]
    fn library_with_layer_is_valid() {
        let manifest = parse(
            "[library]\nname = \"geometry\"\n[library.layer.process]\nplatform = [\"@process\"]\n",
        );
        let library = manifest.library.as_ref().unwrap();
        assert_eq!(library.base_root(), Path::new("src"));
        assert!(library.layer.contains_key("process"));
        assert!(manifest.validate().is_empty());
    }

    #[test]
    fn library_missing_name_is_an_error() {
        let manifest = parse("[library]\n");
        assert!(manifest.validate().iter().any(|e| e.contains("name")));
    }

    #[test]
    fn library_layer_without_platform_is_an_error() {
        // A layer must declare the platforms it serves — the layer *name* is free
        // (it doesn't imply a platform), so an empty `platform` is ambiguous.
        let manifest = parse("[library]\nname = \"x\"\n[library.layer.weird]\nroot = \"w\"\n");
        assert!(manifest.validate().iter().any(|e| e.contains("weird")));
    }

    #[test]
    fn unknown_library_layer_platform_is_an_error() {
        let manifest =
            parse("[library]\nname = \"x\"\n[library.layer.l]\nplatform = [\"nodejs\"]\n");
        assert!(manifest.validate().iter().any(|e| e.contains("nodejs")));
    }

    #[test]
    fn neither_section_is_an_error() {
        let manifest = parse("[build]\npreset = \"release\"\n");
        assert!(
            manifest
                .validate()
                .iter()
                .any(|e| e.contains("must declare"))
        );
    }

    #[test]
    fn registry_dependency_is_rejected() {
        let manifest =
            parse("[package]\nname = \"x\"\n[package.dependencies]\ngeometry = \"1.2\"\n");
        assert!(
            manifest
                .validate()
                .iter()
                .any(|e| e.contains("registry dependency"))
        );
    }

    // ── git dependencies (proposal/distribution.md §5) ──
    //
    // The declaration matrix, one case per test: what a git dependency may say,
    // and — for each way of saying it wrong — that the manifest says so cleanly
    // instead of resolving something surprising.

    #[test]
    fn a_git_dependency_pinned_to_a_tag_is_accepted() {
        let (manifest, errors) = dependency_declaration(
            "{ git = \"https://example.com/org/shapes\", tag = \"v1.2.0\" }",
        );
        assert!(errors.is_empty(), "{errors:?}");
        let DependencySource::Git(source) = source_of(&manifest) else {
            panic!("expected a git source");
        };
        assert_eq!(source.url, "https://example.com/org/shapes");
        assert_eq!(source.reference, GitRef::Tag("v1.2.0".to_string()));
    }

    #[test]
    fn a_git_dependency_pinned_to_a_rev_is_accepted() {
        let (manifest, errors) = dependency_declaration(
            "{ git = \"https://example.com/org/shapes\", rev = \"0123456789abcdef0123456789abcdef01234567\" }",
        );
        assert!(errors.is_empty(), "{errors:?}");
        let DependencySource::Git(source) = source_of(&manifest) else {
            panic!("expected a git source");
        };
        assert_eq!(
            source.reference,
            GitRef::Rev("0123456789abcdef0123456789abcdef01234567".to_string())
        );
    }

    #[test]
    fn a_git_dependency_setting_both_tag_and_rev_is_an_error() {
        let (_, errors) = dependency_declaration(
            "{ git = \"https://example.com/org/shapes\", tag = \"v1\", rev = \"0123456\" }",
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("dependency `shapes`")
                    && error.contains("both `tag` and `rev`")
                    && error.contains("exactly one point")),
            "{errors:?}"
        );
    }

    #[test]
    fn a_git_dependency_pinning_nothing_is_an_error() {
        let (_, errors) = dependency_declaration("{ git = \"https://example.com/org/shapes\" }");
        assert!(
            errors.iter().any(|error| error.contains("no point to pin")
                && error.contains("tag")
                && error.contains("rev")),
            "{errors:?}"
        );
    }

    #[test]
    fn a_branch_is_an_error_that_steers_to_tag_or_rev() {
        // The one deliberate omission of v1: a branch moves, so it pins
        // nothing. The error has to say what to write instead.
        let (_, errors) = dependency_declaration(
            "{ git = \"https://example.com/org/shapes\", branch = \"main\" }",
        );
        assert!(
            errors.iter().any(|error| error.contains("branch `main`")
                && error.contains("tag = \"v1.2.0\"")
                && error.contains("rev = \"<commit sha>\"")),
            "{errors:?}"
        );
    }

    #[test]
    fn git_and_path_together_is_an_error() {
        let (_, errors) = dependency_declaration(
            "{ git = \"https://example.com/org/shapes\", tag = \"v1\", path = \"../shapes\" }",
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("both `git` and `path`")),
            "{errors:?}"
        );
    }

    #[test]
    fn a_version_alongside_git_is_an_error() {
        // There is no resolver, so a `version` next to a `git` would be a
        // silently ignored constraint — the exact thing a manifest error is for.
        let (_, errors) = dependency_declaration(
            "{ git = \"https://example.com/org/shapes\", tag = \"v1\", version = \"1.2\" }",
        );
        assert!(
            errors.iter().any(
                |error| error.contains("`version`/`registry`") && error.contains("`tag`/`rev`")
            ),
            "{errors:?}"
        );
    }

    #[test]
    fn a_rev_that_is_not_a_commit_sha_is_an_error() {
        let (_, errors) =
            dependency_declaration("{ git = \"https://example.com/org/shapes\", rev = \"main\" }");
        assert!(
            errors
                .iter()
                .any(|error| error.contains("not a commit SHA") && error.contains("goes in `tag`")),
            "{errors:?}"
        );
    }

    #[test]
    fn a_tag_that_would_read_as_a_git_option_is_an_error() {
        // `--upload-pack=…` as a "tag" must never reach the command line.
        let (_, errors) = dependency_declaration(
            "{ git = \"https://example.com/org/shapes\", tag = \"--upload-pack=touch owned\" }",
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("not a usable git tag name")),
            "{errors:?}"
        );
    }

    #[test]
    fn a_git_url_that_would_read_as_a_git_option_is_an_error() {
        let (_, errors) =
            dependency_declaration("{ git = \"--upload-pack=touch owned\", tag = \"v1\" }");
        assert!(
            errors
                .iter()
                .any(|error| error.contains("not a repository URL")),
            "{errors:?}"
        );
        let (_, empty) = dependency_declaration("{ git = \"\", tag = \"v1\" }");
        assert!(
            empty
                .iter()
                .any(|error| error.contains("not a repository URL")),
            "{empty:?}"
        );
    }

    #[test]
    fn a_git_key_without_git_is_an_error() {
        for (key, declaration) in [
            ("tag", "{ path = \"../shapes\", tag = \"v1\" }"),
            ("rev", "{ path = \"../shapes\", rev = \"0123456\" }"),
            ("branch", "{ path = \"../shapes\", branch = \"main\" }"),
        ] {
            let (_, errors) = dependency_declaration(declaration);
            assert!(
                errors
                    .iter()
                    .any(|error| error.contains(&format!("declares `{key}` without `git`"))),
                "{declaration}: {errors:?}"
            );
        }
    }

    #[test]
    fn a_bare_version_dependency_is_still_not_yet_supported() {
        // Unchanged by git dependencies: the registry stub stays a stub, and
        // now says what a dependency CAN be.
        for declaration in [
            "\"1.2\"",
            "{ version = \"1.2\" }",
            "{ registry = \"crates\" }",
        ] {
            let (_, errors) = dependency_declaration(declaration);
            assert!(
                errors.iter().any(|error| error
                    .contains("registry dependency `shapes` is not yet supported")
                    && error.contains("`git`")),
                "{declaration}: {errors:?}"
            );
        }
    }

    /// Writes a cache entry by hand: a `[library]` checkout at exactly the key
    /// `source` hashes to. Everything the cache does at *read* time flows
    /// through this — no git, no network.
    fn seed_cache_entry(cache_root: &Path, source: &crate::git_dep::GitSource, name: &str) {
        let entry = crate::git_dep::entry_path(cache_root, source);
        std::fs::create_dir_all(entry.join("src")).expect("create the cache entry");
        std::fs::write(
            entry.join("vilan.toml"),
            format!("[library]\nname = \"{name}\"\n"),
        )
        .expect("the checkout's manifest");
        std::fs::write(entry.join("src").join("lib.vl"), "").expect("the checkout's lib.vl");
    }

    #[test]
    fn a_warm_cache_resolves_a_git_dependency_with_no_network_at_all() {
        // The offline guarantee, and the editor's policy in one: `CacheOnly`
        // never fetches, and the URL below is unreachable by construction — so
        // a resolved workspace can only have come from the cache.
        let root = std::env::temp_dir().join(format!("vilan-git-warm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let cache = root.join("cache");
        let source = crate::git_dep::GitSource {
            url: "https://example.invalid/org/shapes".to_string(),
            reference: GitRef::Tag("v1.2.0".to_string()),
        };
        seed_cache_entry(&cache, &source, "shapes");
        let application = root.join("app");
        std::fs::create_dir_all(application.join("src")).expect("create the app");
        std::fs::write(
            application.join("vilan.toml"),
            "[package]\nname = \"app\"\n[package.dependencies]\n\
             shapes = { git = \"https://example.invalid/org/shapes\", tag = \"v1.2.0\" }\n",
        )
        .expect("the app manifest");

        let workspace = resolve_workspace(&application, &GitDeps::cache_only(&cache))
            .expect("a warm cache resolves offline");
        assert_eq!(workspace.packages.len(), 1);
        assert_eq!(workspace.entry_dependencies.len(), 1);
        assert_eq!(workspace.entry_dependencies[0].0, "shapes");
        assert!(
            workspace.packages[0]
                .base_root
                .starts_with(crate::git_dep::entry_path(&cache, &source)),
            "the dependency's source root is the cached checkout: {:?}",
            workspace.packages[0].base_root
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_cold_cache_under_the_editors_policy_is_a_steer_not_a_fetch() {
        // What the language server does with an unfetched dependency: it says
        // so, and it touches nothing (no directory, no `git`, no network).
        let root = std::env::temp_dir().join(format!("vilan-git-cold-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let cache = root.join("cache");
        let application = root.join("app");
        std::fs::create_dir_all(application.join("src")).expect("create the app");
        std::fs::write(
            application.join("vilan.toml"),
            "[package]\nname = \"app\"\n[package.dependencies]\n\
             shapes = { git = \"https://example.invalid/org/shapes\", tag = \"v1.2.0\" }\n",
        )
        .expect("the app manifest");

        let error = resolve_workspace(&application, &GitDeps::cache_only(&cache))
            .expect_err("a cold cache-only resolution fails");
        assert!(error.contains("git dependency `shapes`"), "{error}");
        assert!(error.contains("not in the local cache"), "{error}");
        assert!(error.contains("vilan build"), "{error}");
        assert!(!cache.exists(), "cache-only must not create the cache root");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn path_dependency_is_accepted() {
        let manifest = parse(
            "[package]\nname = \"x\"\n[package.dependencies]\nshapes = { path = \"../shapes\" }\n",
        );
        assert!(manifest.validate().is_empty());
        assert_eq!(
            manifest.package.as_ref().unwrap().dependencies["shapes"].path(),
            Some(Path::new("../shapes"))
        );
    }

    #[test]
    fn the_retired_server_client_form_gets_a_migration_error() {
        // Not an unknown-key shrug: the old form names its replacement.
        for source in [
            "[server]\nentry = \"server.vl\"\n[client]\nentry = \"client.vl\"\n",
            "[client]\nentry = \"app.vl\"\n",
        ] {
            let manifest = parse(source);
            assert!(
                manifest
                    .validate()
                    .iter()
                    .any(|e| e.contains("removed") && e.contains("[entry.server]")),
                "{source}"
            );
        }
    }

    #[test]
    fn entries_parse_with_root_relative_defaults() {
        let manifest = parse(
            "[package]\nname = \"app\"\n\n[entry.server]\n\n\
             [entry.client]\ntarget = \"browser\"\npath = \"web/main.vl\"\n",
        );
        assert!(manifest.validate().is_empty());
        let server = &manifest.entries["server"];
        assert_eq!(server.path("server"), Path::new("server.vl"));
        assert!(server.resolved_target().is_none(), "target defaults later");
        let client = &manifest.entries["client"];
        assert_eq!(client.path("client"), Path::new("web/main.vl"));
        assert_eq!(client.resolved_target(), Some(Platform::Browser));
    }

    #[test]
    fn entries_require_a_package() {
        let manifest = parse("[library]\nname = \"lib\"\n\n[entry.server]\n");
        assert!(
            manifest
                .validate()
                .iter()
                .any(|e| e.contains("require a `[package]`"))
        );
    }

    #[test]
    fn entries_replace_the_single_entry_keys() {
        let manifest = parse("[package]\nname = \"app\"\ntarget = \"browser\"\n\n[entry.server]\n");
        assert!(
            manifest
                .validate()
                .iter()
                .any(|e| e.contains("can't be combined with"))
        );
    }

    #[test]
    fn an_entry_name_must_be_an_identifier() {
        let manifest = parse("[package]\nname = \"app\"\n\n[entry.\"my app\"]\n");
        assert!(
            manifest
                .validate()
                .iter()
                .any(|e| e.contains("valid identifier"))
        );
    }

    #[test]
    fn an_entry_target_must_be_a_host_platform() {
        let none = parse("[package]\nname = \"app\"\n\n[entry.lib]\ntarget = \"none\"\n");
        assert!(
            none.validate()
                .iter()
                .any(|e| e.contains("host platform") && e.contains("`none`"))
        );
        let unknown = parse("[package]\nname = \"app\"\n\n[entry.app]\ntarget = \"wat\"\n");
        assert!(
            unknown
                .validate()
                .iter()
                .any(|e| e.contains("invalid `[entry.app] target`"))
        );
    }

    #[test]
    fn an_entry_path_stays_inside_the_package() {
        let manifest = parse("[package]\nname = \"app\"\n\n[entry.out]\npath = \"../out.vl\"\n");
        assert!(
            manifest
                .validate()
                .iter()
                .any(|e| e.contains("free of `..`"))
        );
    }

    #[test]
    fn build_options_from_preset_and_overrides() {
        let manifest = parse(
            "[package]\nname = \"x\"\n[build]\npreset = \"release\"\nreadable-names = true\n",
        );
        let options = manifest.build_options().unwrap();
        assert!(!options.indent); // release
        assert!(options.readable_names); // overridden on
    }

    #[test]
    fn build_hmr_key_is_ignored_not_a_user_knob() {
        // HMR instrumentation is never a `vilan.toml` setting (A13 S2a): it is set
        // only by an HMR-active `run --watch`. An `hmr` key under `[build]` is
        // ignored exactly like any unknown build key — it never turns on the
        // `BuildOptions.hmr` flag.
        let manifest = parse("[package]\nname = \"x\"\n[build]\nhmr = true\n");
        let options = manifest.build_options().unwrap();
        assert!(!options.hmr, "a `[build] hmr` key must not set the flag");
    }

    #[test]
    fn unknown_top_level_key_warns() {
        let (_, warnings) = Manifest::parse("[package]\nname = \"x\"\n[wat]\nk = 1\n").unwrap();
        assert!(warnings.iter().any(|w| w.contains("wat")));
    }

    #[test]
    fn resolve_std_forgives_a_source_root_path() {
        // The mis-configuration that produced a wall of editor errors:
        // `vilan.stdPath` pointed at `.../std/src` instead of the package
        // directory. Both forms must yield the SAME spec — layers included
        // (a bare-source fallback would drop every platform layer).
        let std_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std");
        let proper = super::resolve_std(&std_dir);
        let forgiven = super::resolve_std(&std_dir.join("src"));
        assert!(!proper.layers.is_empty(), "the real std declares layers");
        assert_eq!(proper.base_root, forgiven.base_root);
        assert_eq!(proper.layers.len(), forgiven.layers.len());
    }
}
