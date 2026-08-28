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

/// Every top-level section a `vilan.toml` may contain — the whitelist behind
/// the unknown-key warning, and the set any surface that *describes* the
/// manifest (the language server's completion, the editor's JSON schema) is
/// pinned against. `server` / `client` are here only so [`Manifest::validate`]
/// can point their users at the replacement; they are not valid content.
pub const KNOWN_SECTIONS: &[&str] = &[
    "package", "library", "project", "build", "macro", "entry", "server", "client",
];

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
    /// Route-chunk emission for the single-entry shape — the same key
    /// `[entry.<name>] split` carries, since `[package] target = "browser"`
    /// declares a browser entry just as an entry section does
    /// (`bundle-splitting.md` §4).
    pub split: Option<bool>,
    /// Which `[entry.<name>]` `vilan run` executes when the package declares
    /// several node entries (A15's follow-up). `--entry` still overrides it.
    #[serde(rename = "default-entry")]
    pub default_entry: Option<String>,
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
    /// Which member `vilan run` executes when the workspace has several node
    /// packages (A15's follow-up) — the member's `[package] name`. `--entry`
    /// still overrides it.
    #[serde(rename = "default-entry")]
    pub default_entry: Option<String>,
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
    /// Emit this entry as an eager bundle plus one lazily-imported file per
    /// route chunk (`bundle-splitting.md` §4). Browser entries only; default
    /// `false`, and single-file emission stays the default mode forever.
    pub split: Option<bool>,
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

    /// Whether this entry asked for route-chunk emission (default `false`).
    pub fn splits(&self) -> bool {
        self.split.unwrap_or(false)
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
/// (proposal `distribution.md` §5); `project = true` *inherits* the workspace
/// root's declaration (§5's rider); with none of them it is a *registry
/// dependency*, which nothing resolves yet.
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
        /// `project = true`: take this dependency's whole declaration from the
        /// enclosing workspace root's `[project.dependencies]`. Opt-in, per
        /// member, per dependency — never automatic (see [`enclosing_project`]).
        project: Option<bool>,
        /// `build-hooks = true` — **tier 2's opt-in** (`build-trust.md` §3,
        /// specified by `build-hooks.md` §8's S2 under the owner's Q6 ruling
        /// of 2026-08-28: tier 2's threshold is the git dependency, i.e. now).
        ///
        /// It grants this one dependency permission to run *its* `[build]`
        /// hooks with your privileges. It satisfies §3's three properties by
        /// construction: it is **per dependency** (there is no project-wide
        /// spelling), it is **recorded in the manifest** (a word on the line a
        /// reviewer already reads when a dependency changes, which a revert
        /// undoes), and **absent means no**.
        ///
        /// It lives on the dependency's own declaration rather than in a
        /// grant table of its own for two reasons that are properties, not
        /// taste: you cannot grant what you do not declare, so no grant can
        /// name a package that is not there; and removing the dependency
        /// removes the grant, so a grant can never outlive its subject.
        ///
        /// **Nothing honors it yet, deliberately.** Every dependency hook is
        /// refused in this slice, opted in or not; the point of shipping the
        /// key now is that the syntax is fixed and reviewable *before*
        /// anything can cross it, and a registry inherits a spelling that
        /// already exists rather than minting one under pressure
        /// (`build-hooks.md` §4.2). `false` is accepted and means exactly what
        /// absence means — a considered "no" is worth being able to write down.
        #[serde(rename = "build-hooks")]
        build_hooks: Option<bool>,
    },
}

/// What a dependency resolves against — the four kinds [`Dependency::source`]
/// distinguishes once, for both validation and resolution.
pub enum DependencySource<'declaration> {
    /// A local directory, relative to the declaring manifest.
    ///
    /// **Deliberately not containment-checked, and this is the decision, not an
    /// oversight (E95).** `[entry.<name>] path` is refused when it is absolute
    /// or carries `..`; a dependency `path` is not, and the asymmetry is on
    /// purpose. An entry path names a file *inside the package being built* —
    /// it is resolved against `root` and its name becomes `dist/<name>.js`, so
    /// climbing out of the package is meaningless there and the guard is a
    /// well-formedness rule, not a security boundary. A dependency path names
    /// *another package*, and pointing outside the declaring directory is the
    /// whole point of the form: `common = { path = "../common" }` is what
    /// every workspace, every monorepo, and this repo's own
    /// `examples/fullstack` legs are spelled as. A `..` guard here would refuse
    /// the correct spelling of the common case.
    ///
    /// The security argument for adding one anyway does not survive the ruled
    /// trust model (`proposal/build-trust.md` §2, RULED 2026-08-26): the
    /// manifest is the developer's own file, and building a project already
    /// runs its `[build] run` hooks with the developer's full privileges, no
    /// sandbox and no prompt. Against a manifest that can execute arbitrary
    /// shell, a rule about which *directory* it may name reads a package it was
    /// free to run code from anyway — it buys nothing and, worse, advertises a
    /// containment property the trust model explicitly declines to offer.
    /// Tier 1 is trusted; the tier that is not is dependency-authored build
    /// code (§3), whose enforcement point is ~~the registry (D5)~~ **the
    /// `build-hooks` key on this declaration's own table** — the threshold
    /// moved to the git dependency, i.e. now, on the owner's Q6 ruling of
    /// 2026-08-28 (`build-hooks.md` §4.2). Still not this path: a grant is a
    /// word the declaring manifest writes, not a shape the directory has.
    Path(&'declaration Path),
    /// One immutable point of one repository, materialized into the git cache
    /// and then treated exactly like a path dependency.
    Git(GitSource),
    /// `project = true` — whatever the enclosing `[project.dependencies]` says
    /// this name is, resolved against the *project root* (the manifest that
    /// declares it), not the member's directory.
    Inherited,
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

    /// Whether this declaration carries tier 2's opt-in (`build-hooks = true`).
    /// Absent — and a bare version string, which has nowhere to write it — is
    /// no (`build-trust.md` §3).
    pub fn grants_build_hooks(&self) -> bool {
        matches!(
            self,
            Dependency::Detailed {
                build_hooks: Some(true),
                ..
            }
        )
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
            project,
            build_hooks,
        } = self
        else {
            // A bare version string.
            return Ok(DependencySource::Registry);
        };
        // Inheritance is decided FIRST: `{ project = true, path = "…" }` is a
        // contradiction about where the dependency comes from, and saying so
        // beats diagnosing the leftover key as if it stood alone.
        if let Some(project) = project {
            if !project {
                return Err(
                    "sets `project = false`: a dependency either inherits the workspace \
                     root's declaration (`project = true`) or declares its own source; \
                     drop the key to declare your own"
                        .to_string(),
                );
            }
            for (key, present) in [
                ("version", version.is_some()),
                ("registry", registry.is_some()),
                ("path", path.is_some()),
                ("git", git.is_some()),
                ("tag", tag.is_some()),
                ("rev", rev.is_some()),
                ("branch", branch.is_some()),
                // Trust travels with the declaration, like everything else an
                // inherited dependency takes wholesale: the grant belongs in
                // the `[project.dependencies]` line that says where the
                // dependency comes from, so one review sees both facts about
                // it at once (`build-trust.md` §3's "recorded in the
                // manifest").
                ("build-hooks", build_hooks.is_some()),
            ] {
                if present {
                    return Err(format!(
                        "sets `project = true` alongside `{key}`: an inherited dependency \
                         takes its WHOLE declaration from `[project.dependencies]`, so drop \
                         `{key}` (or drop `project = true` and declare it here)"
                    ));
                }
            }
            return Ok(DependencySource::Inherited);
        }
        let Some(url) = git else {
            // A git key without `git` is a declaration that does nothing — the
            // one thing a manifest error is for.
            for (key, value) in [("tag", tag), ("rev", rev), ("branch", branch)] {
                if value.is_some() {
                    return Err(format!(
                        "declares `{key}` without `git`: `{key}` only means something on a \
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
                "sets both `git` and `path`: a dependency is either a local directory \
                 or a repository checkout, not both"
                    .to_string(),
            );
        }
        if version.is_some() || registry.is_some() {
            return Err(
                "sets `version`/`registry` alongside `git`: a git dependency is pinned by \
                 its `tag`/`rev`; there is no version resolution to take part in"
                    .to_string(),
            );
        }
        if let Some(branch) = branch {
            return Err(format!(
                "pins the branch `{branch}`: a branch moves, so it cannot pin anything; \
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
                         hexadecimal digits); a branch or tag name goes in `tag`"
                    ));
                }
                GitRef::Rev(rev.clone())
            }
            (Some(_), Some(_)) => {
                return Err(
                    "sets both `tag` and `rev`: a git dependency pins exactly one point, \
                     so pick the release (`tag`) or the commit (`rev`)"
                        .to_string(),
                );
            }
            (None, None) => {
                return Err(
                    "has a `git` URL but no point to pin; add `tag = \"v1.2.0\"` or \
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

/// One value or a list of them — the manifest's "write one bare, or several in
/// a list" shape. `[build] run` (A9) spells its commands this way, and
/// `[[build.hook]]`'s `run` / `inputs` / `outputs` reuse it so the three keys
/// share one vocabulary rather than each inventing its own
/// (`build-hooks.md` §2.3).
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum StringList {
    One(String),
    Many(Vec<String>),
}

impl StringList {
    /// The declared values, in order.
    pub fn values(&self) -> &[String] {
        match self {
            StringList::One(value) => std::slice::from_ref(value),
            StringList::Many(values) => values,
        }
    }
}

/// A named `[[build.hook]]` — the table form of a build hook
/// (`build-hooks.md` §2.3), which exists so a hook has somewhere to hang the
/// facts the staleness predicate needs. `run = [...]` keeps working exactly as
/// it did and stays the "every build" default; a hook here that declares
/// `inputs` and/or `outputs` is *skipped* while none of them has moved (§3.1).
///
/// `name` is required, because it is what every message and every stamp entry
/// keys on: a hook the build reports on has to be nameable by the reader.
#[derive(Debug, Clone, Deserialize)]
pub struct HookDeclaration {
    /// The hook's name — required, unique within the manifest.
    pub name: Option<String>,
    /// The command(s) this hook runs, in order.
    pub run: Option<StringList>,
    /// The files (or directories) the hook READS. A declared input that is
    /// missing is recorded as missing, so its *appearance* invalidates (§3.1).
    pub inputs: Option<StringList>,
    /// The files (or directories) the hook WRITES. A missing or hand-edited
    /// output re-runs the hook.
    pub outputs: Option<StringList>,
}

impl HookDeclaration {
    /// The hook's command(s), in declaration order. Empty when `run` is absent
    /// (which [`Manifest::validate`] refuses).
    pub fn commands(&self) -> &[String] {
        self.run
            .as_ref()
            .map(StringList::values)
            .unwrap_or_default()
    }

    /// The declared inputs, in declaration order.
    pub fn inputs(&self) -> &[String] {
        self.inputs
            .as_ref()
            .map(StringList::values)
            .unwrap_or_default()
    }

    /// The declared outputs, in declaration order.
    pub fn outputs(&self) -> &[String] {
        self.outputs
            .as_ref()
            .map(StringList::values)
            .unwrap_or_default()
    }
}

/// The `[build]` section: the code-generation knobs, deserialized before
/// resolving against a preset (see [`Manifest::build_options`]), plus the `run`
/// hooks and the named `[[build.hook]]` tables beside them.
#[derive(Debug, Default, Deserialize)]
pub struct Build {
    /// Commands to run before each build (A9). See [`Manifest::build_hooks`].
    pub run: Option<StringList>,
    /// `[[build.hook]]` — named hooks with declared inputs and outputs, which
    /// is what makes them skippable (`build-hooks.md` §3). They run *after*
    /// every `run = [...]` command, in declaration order.
    #[serde(rename = "hook", default)]
    pub hooks: Vec<HookDeclaration>,
    pub preset: Option<String>,
    pub indent: Option<bool>,
    pub spaces: Option<bool>,
    #[serde(rename = "readable-names")]
    pub readable_names: Option<bool>,
    #[serde(rename = "debug-names")]
    pub debug_names: Option<bool>,
    /// The `const` inference sweep (const-eval.md §9.4) — on under `release`,
    /// off under `debug`, overridable here like every other knob.
    #[serde(rename = "infer-const")]
    pub infer_const: Option<bool>,
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

    /// Whether this package's single entry asked for route-chunk emission
    /// (default `false`).
    pub fn splits(&self) -> bool {
        self.split.unwrap_or(false)
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
        let warnings = table
            .keys()
            .filter(|key| !KNOWN_SECTIONS.contains(&key.as_str()))
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
                "the `[server]`/`[client]` form was removed; declare a `[package]` \
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
                "set exactly one of `[package]`, `[library]`, or `[project]`: an app, a \
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
        // A workspace's default entry names a MEMBER package, whose name is
        // validated in that member's own manifest — all this one can check is
        // the shape (a name that isn't an identifier could never match).
        if let Some(project) = &self.project {
            if let Some(name) = &project.default_entry {
                if !is_identifier(name) {
                    errors.push(format!(
                        "`[project] default-entry` must name a member package \
                         (got `{name}`)"
                    ));
                }
            }
        }
        // The `[project.dependencies]` table is the one members inherit FROM,
        // so it is the one table that cannot itself inherit.
        for (table, inheritable, dependencies) in [
            (
                "[package.dependencies]",
                true,
                self.package.as_ref().map(|p| &p.dependencies),
            ),
            (
                "[library.dependencies]",
                true,
                self.library.as_ref().map(|l| &l.dependencies),
            ),
            (
                "[project.dependencies]",
                false,
                self.project.as_ref().map(|p| &p.dependencies),
            ),
        ] {
            let Some(dependencies) = dependencies else {
                continue;
            };
            validate_dependencies(table, inheritable, dependencies, &mut errors);
        }
        if let Some(build) = &self.build {
            if let Some(preset) = &build.preset {
                if Preset::parse(preset).is_none() {
                    errors.push(format!(
                        "unknown build preset `{preset}` (expected `debug` or `release`)"
                    ));
                }
            }
            // A blank hook would spawn a shell to do nothing, and fail
            // confusingly; a declaration that does nothing is what a manifest
            // error is for.
            if self
                .build_hooks()
                .iter()
                .any(|command| command.trim().is_empty())
            {
                errors.push(
                    "`[build] run` has an empty command: each entry is a command line \
                     for the platform shell (`npx tailwindcss -i src/app.css -o dist/app.css`)"
                        .to_string(),
                );
            }
            validate_declared_hooks(&build.hooks, &mut errors);
        }
        errors
    }

    fn validate_package(&self, package: &Package, errors: &mut Vec<String>) {
        match &package.name {
            None => errors.push("`[package]` is missing a `name`".to_string()),
            Some(name) if !is_identifier(name) => errors.push(format!(
                "`[package] name` must be a valid identifier (got `{name}`)"
            )),
            Some(name) => {
                if let Some(reason) = reserved_package_name(name) {
                    errors.push(reserved_name_refusal(name, reason, "package"));
                }
            }
        }
        if let Some(target) = &package.target {
            if let Err(error) = Platform::parse(target) {
                errors.push(format!("invalid `[package] target`: {error}"));
            }
        }
        if package.splits() && self.entries.is_empty() {
            let declared = package.target.as_deref().unwrap_or("node");
            if !matches!(Platform::parse(declared), Ok(Platform::Browser)) {
                errors.push(split_needs_a_browser_leg("[package] split", declared));
            }
        }
        // The designated default entry names one of this package's
        // `[entry.<name>]` sections — checkable right here, so a typo is a
        // manifest error rather than a `run` that picks the wrong leg.
        if let Some(name) = &package.default_entry {
            if self.entries.is_empty() {
                errors.push(format!(
                    "`[package] default-entry = \"{name}\"` has nothing to choose \
                     between: it names one of several `[entry.<name>]` sections, \
                     and this package declares none (its single `entry` is already \
                     what `vilan run` runs)"
                ));
            } else if !self.entries.contains_key(name) {
                errors.push(format!(
                    "`[package] default-entry = \"{name}\"` names no `[entry.{name}]` \
                     section; declared entries: {}",
                    self.entries.keys().cloned().collect::<Vec<_>>().join(", ")
                ));
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
            if package.entry.is_some() || package.target.is_some() || package.split.is_some() {
                errors.push(
                    "`[package] entry`/`target`/`split` can't be combined with \
                     `[entry.<name>]` sections: with multiple entries, each \
                     declares its own `path`, `target` and `split`"
                        .to_string(),
                );
            }
        }
        for (name, entry) in &self.entries {
            if !is_identifier(name) {
                errors.push(format!(
                    "`[entry.{name}]`: an entry name must be a valid identifier \
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
            if entry.splits() {
                let declared = entry.target.as_deref().unwrap_or("node");
                if !matches!(Platform::parse(declared), Ok(Platform::Browser)) {
                    errors.push(split_needs_a_browser_leg(
                        &format!("[entry.{name}] split"),
                        declared,
                    ));
                }
            }
            if let Some(path) = &entry.path {
                // Well-formedness, NOT containment: an entry is a file inside
                // the package being built, and its name becomes `dist/<name>.js`,
                // so a path that climbs out names nothing this build can emit.
                // A dependency `path` gets no such rule ON PURPOSE — see
                // `DependencySource::Path` for why the asymmetry is the
                // decision (E95).
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

    /// The build unit `vilan run` executes when this manifest offers a choice
    /// (A15's follow-up): the `[entry.<name>]` a `[package]` designates, or the
    /// member package a `[project]` does. One accessor, because the two shapes
    /// lower onto the same workspace orchestration — both name a build unit.
    /// `--entry` overrides it; `None` means no designation.
    pub fn default_entry(&self) -> Option<&str> {
        self.package
            .as_ref()
            .and_then(|package| package.default_entry.as_deref())
            .or_else(|| {
                self.project
                    .as_ref()
                    .and_then(|project| project.default_entry.as_deref())
            })
    }

    /// The `[build] run` hooks (A9), in declaration order — the commands to run
    /// before *every* build. Empty when none are declared.
    ///
    /// These are the undeclared hooks: they name no inputs and no outputs, so
    /// they can never be fresh and the staleness predicate never skips them
    /// (`build-hooks.md` §3.1). That is what keeps the change backward
    /// compatible to the byte — a manifest that only writes `run = [...]`
    /// behaves exactly as it did before `[[build.hook]]` existed.
    pub fn build_hooks(&self) -> &[String] {
        self.build
            .as_ref()
            .and_then(|build| build.run.as_ref())
            .map(StringList::values)
            .unwrap_or_default()
    }

    /// The named `[[build.hook]]` declarations, in declaration order — the
    /// hooks the staleness predicate may skip (`build-hooks.md` §3). They run
    /// after every [`Manifest::build_hooks`] command, as §2.3 spells it.
    pub fn declared_hooks(&self) -> &[HookDeclaration] {
        self.build
            .as_ref()
            .map(|build| build.hooks.as_slice())
            .unwrap_or_default()
    }

    /// Whether this manifest asks for **any** build-time command execution —
    /// the question tier 2 asks of a *dependency's* manifest
    /// (`build-trust.md` §3, `build-hooks.md` §4.3). Nothing here decides
    /// whether it runs; it only decides whether there is something to say.
    pub fn declares_build_hooks(&self) -> bool {
        !self.build_hooks().is_empty() || !self.declared_hooks().is_empty()
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
        options.infer_const = build.infer_const.unwrap_or(options.infer_const);
        Ok(options)
    }
}

/// Checks every dependency declaration in the `table` named (`[package.
/// dependencies]`, …): a malformed git dependency names its mistake, a registry
/// dependency is still unsupported, and only a *member's* table may inherit —
/// the workspace root's own declarations are what inheritance reads. Reported as
/// errors so a declared dependency is never silently ignored.
fn validate_dependencies(
    table: &str,
    inheritable: bool,
    dependencies: &BTreeMap<String, Dependency>,
    errors: &mut Vec<String>,
) {
    for (name, dependency) in dependencies {
        // A dependency's key IS the import root it binds, so a reserved name
        // here is a claim on the namespace itself — refused whatever the
        // declaration's source looks like (a second, independent problem with
        // the source still reports beside this: different root causes).
        if let Some(reason) = reserved_package_name(name) {
            errors.push(reserved_name_refusal(name, reason, "dependency"));
        }
        match dependency.source() {
            Ok(DependencySource::Path(_)) | Ok(DependencySource::Git(_)) => {}
            Ok(DependencySource::Inherited) if !inheritable => errors.push(format!(
                "`{table} {name}` sets `project = true`, but this IS the project's table: \
                 `project = true` is how a member package opts in to a dependency declared \
                 here, so give `{name}` a `path` or a `git` source"
            )),
            Ok(DependencySource::Inherited) => {}
            Ok(DependencySource::Registry) => errors.push(format!(
                "registry dependency `{name}` is not yet supported (a dependency is a \
                 local `path` or a `git` repository today)"
            )),
            Err(problem) => errors.push(format!("dependency `{name}` {problem}")),
        }
    }
}

/// The characters a declared `inputs` / `outputs` entry may not contain, and
/// the reason: they are the shell-glob metacharacters, and this design
/// deliberately does **not** match globs (`build-hooks.md` §3.1 digests
/// *declared* paths). Left unchecked, `inputs = ["src/**/*.css"]` would be
/// digested as one path that is simply never there — recorded as missing,
/// matching "missing" forever, and so silently freezing the hook after its
/// first run. Refusing it is the difference between a loud manifest error and
/// a hook that quietly stops running.
const GLOB_METACHARACTERS: [char; 3] = ['*', '?', '['];

/// Checks the `[[build.hook]]` tables (`build-hooks.md` §2.3): every hook is
/// named, uniquely, and carries at least one command, and every declared path
/// is a plain path. Reported as errors rather than ignored, because each of
/// these mistakes produces a hook that looks declared and does nothing.
fn validate_declared_hooks(hooks: &[HookDeclaration], errors: &mut Vec<String>) {
    let mut seen: HashSet<&str> = HashSet::new();
    for (position, hook) in hooks.iter().enumerate() {
        // Positional, because a hook with no usable name has nothing else to
        // be addressed by — and the ordinal is exactly how the reader finds
        // the table in the file.
        let ordinal = position + 1;
        let name = match hook.name.as_deref() {
            Some(name) if !name.trim().is_empty() => name,
            Some(_) => {
                errors.push(format!(
                    "`[[build.hook]]` #{ordinal} has a blank `name`: the name is what the \
                     build reports on and what its freshness stamp keys on, so give it one \
                     (`name = \"lucide\"`)"
                ));
                continue;
            }
            None => {
                errors.push(format!(
                    "`[[build.hook]]` #{ordinal} is missing a `name`: the name is what the \
                     build reports on and what its freshness stamp keys on, so give it one \
                     (`name = \"lucide\"`)"
                ));
                continue;
            }
        };
        if !seen.insert(name) {
            errors.push(format!(
                "two `[[build.hook]]` tables are both named `{name}`: the name keys the \
                 freshness stamp, so it has to pick out one hook"
            ));
        }
        let commands = hook.commands();
        if commands.is_empty() {
            errors.push(format!(
                "`[[build.hook]]` `{name}` declares no `run`: a hook is a command line for \
                 the platform shell (`run = \"sh scripts/lucide.sh\"`)"
            ));
        }
        if commands.iter().any(|command| command.trim().is_empty()) {
            errors.push(format!(
                "`[[build.hook]]` `{name}` has an empty command: each entry is a command \
                 line for the platform shell (`run = \"sh scripts/lucide.sh\"`)"
            ));
        }
        for (key, paths) in [("inputs", hook.inputs()), ("outputs", hook.outputs())] {
            for path in paths {
                if path.trim().is_empty() {
                    errors.push(format!(
                        "`[[build.hook]]` `{name}` declares an empty `{key}` entry: each one \
                         is a file or directory path relative to the manifest"
                    ));
                    continue;
                }
                if let Some(metacharacter) = path.chars().find(|c| GLOB_METACHARACTERS.contains(c))
                {
                    errors.push(format!(
                        "`[[build.hook]]` `{name}` declares `{key}` entry `{path}`, which \
                         contains `{metacharacter}`: freshness digests the paths you declare, \
                         and there is no glob matching — name the directory (`src/static`) or \
                         the files themselves"
                    ));
                }
            }
        }
    }
}

/// Why a [`resolve_workspace`] failure is the user's problem — or isn't. The
/// two kinds are carried structurally rather than sniffed out of a message
/// because their **severity differs where they are reported**: something broken
/// is an error wherever it appears, while an unfetched git dependency means
/// every manifest is correct and a single `vilan build` fixes it — a warning in
/// the editor, which never fetches (proposal `distribution.md` §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkspaceErrorKind {
    /// A manifest is invalid, a dependency does not resolve, the graph has a
    /// cycle, a fetch failed — something the user must fix.
    Broken,
    /// A declared git dependency is not in the cache and this caller's policy
    /// is cache-only, so nothing fetched it. Nothing is *wrong*.
    Unfetched,
}

/// Why [`resolve_workspace`] failed: the kind (above), the message, and — when
/// it is not simply the manifest resolution started from — **the manifest that
/// wrote the offending declaration**.
///
/// That last part exists for inheritance (`dep = { project = true }`): the
/// member opts in, but the declaration lives in the project root's
/// `[project.dependencies]`, so a broken one is only fixable there. Without the
/// path the editor squiggles the member's `vilan.toml`, which is correct, while
/// the file that needs the edit says nothing (proposal `distribution.md` §7's
/// S5 residual).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceError {
    kind: WorkspaceErrorKind,
    message: String,
    declared_in: Option<PathBuf>,
}

impl WorkspaceError {
    /// Something the user must fix.
    pub fn broken(message: impl Into<String>) -> WorkspaceError {
        WorkspaceError {
            kind: WorkspaceErrorKind::Broken,
            message: message.into(),
            declared_in: None,
        }
    }

    /// A git dependency this caller's policy would not fetch.
    pub fn unfetched(message: impl Into<String>) -> WorkspaceError {
        WorkspaceError {
            kind: WorkspaceErrorKind::Unfetched,
            message: message.into(),
            declared_in: None,
        }
    }

    /// The message, without the kind.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Whether the only thing standing in the way is an unfetched git
    /// dependency — the kind [`WorkspaceError::unfetched`] builds.
    pub fn is_unfetched(&self) -> bool {
        self.kind == WorkspaceErrorKind::Unfetched
    }

    /// The `vilan.toml` holding the declaration at fault, when that is not the
    /// manifest resolution started from — the address a diagnostic belongs on.
    pub fn declared_in(&self) -> Option<&Path> {
        self.declared_in.as_deref()
    }

    /// The same failure, attributed to the manifest that declared it. The
    /// **first** attribution wins: the innermost frame is the one that knows
    /// whose declaration it was resolving, so an outer frame must not restamp
    /// an error that already found its home.
    pub fn declared_in_manifest(mut self, manifest: &Path) -> WorkspaceError {
        if self.declared_in.is_none() {
            self.declared_in = Some(manifest.to_path_buf());
        }
        self
    }

    /// The same kind and address with the message rewritten — a caller adding
    /// context (`git dependency `x`: …`) must not flatten either away, which is
    /// the whole reason they are carried.
    pub fn map_message(self, rewrite: impl FnOnce(String) -> String) -> WorkspaceError {
        WorkspaceError {
            kind: self.kind,
            message: rewrite(self.message),
            declared_in: self.declared_in,
        }
    }
}

impl std::fmt::Display for WorkspaceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.message())
    }
}

impl From<String> for WorkspaceError {
    fn from(message: String) -> Self {
        WorkspaceError::broken(message)
    }
}

/// Resolves the dependency [`Workspace`] for the package rooted at `package_dir`
/// (P2): every reachable `path` and `git` dependency, transitively, with cycle
/// detection. Each `PackageSpec` records its declared `target`, but the graph
/// itself is target-independent — the target-compatibility *diagnostic* is the
/// analyzer's, reported at the import (P3). Shared by the CLI and the language
/// server so both resolve imports identically.
///
/// The rooted manifest may be a `[package]` (an app) **or a `[library]`**: a
/// library's own `[library.dependencies]` are the edges of anything compiled
/// from inside it — its `*_test.vl` files, and the file an editor has open.
/// Everything else — a `[project]` root, which has no sources of its own, and a
/// bare file, which has no manifest — yields an empty workspace.
///
/// `git` decides what a git dependency may do here — fetch on a cache miss (a
/// build) or read a warm cache only (the editor; see [`GitDeps`]). It is an
/// explicit parameter precisely so every caller states its policy.
pub fn resolve_workspace(package_dir: &Path, git: &GitDeps) -> Result<Workspace, WorkspaceError> {
    resolve_workspace_with_hook_report(package_dir, git).map(|(workspace, _)| workspace)
}

/// One dependency in the resolved graph whose **own** manifest asks for
/// build-time command execution — tier 2's subject (`build-trust.md` §3,
/// `build-hooks.md` §4.3).
///
/// It exists because "absent means no" and "the toolchain never looked" are
/// indistinguishable from the terminal, and today they are literally the same
/// thing: `Project::hooks` reads the addressed manifest and nothing else, so a
/// dependency's `[build] run` produces no output, no warning and no note (the
/// paper's probe P5). Reporting it is the whole of the difference.
///
/// Recorded during resolution because resolution is the one pass that already
/// reads every dependency manifest; deciding it anywhere else would mean a
/// second walk of the graph and a second answer to "what is a dependency".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyHooks {
    /// The import name the declaration bound it to — what a message calls it.
    pub name: String,
    /// The dependency's own package directory.
    pub directory: PathBuf,
    /// Whether the manifest that declared this dependency wrote tier 2's
    /// opt-in (`build-hooks = true`). Nothing honors it yet: every dependency
    /// hook is refused in either case (`build-hooks.md` §8's S2).
    pub opted_in: bool,
}

/// [`resolve_workspace`], plus the tier-2 report: every dependency in the
/// resolved graph whose manifest declares build hooks, sorted by directory so
/// the report is a function of the graph and not of the walk.
///
/// Two entry points rather than one so the CLI can ask the question and the
/// language server — which resolves the same graph on every keystroke and has
/// no terminal to say anything on — is not made to carry the answer.
pub fn resolve_workspace_with_hook_report(
    package_dir: &Path,
    git: &GitDeps,
) -> Result<(Workspace, Vec<DependencyHooks>), WorkspaceError> {
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
    let declared = match (&manifest.package, &manifest.library) {
        (Some(package), _) => &package.dependencies,
        (None, Some(library)) => &library.dependencies,
        (None, None) => {
            return Ok((
                Workspace {
                    macro_limits,
                    ..Workspace::default()
                },
                Vec::new(),
            ));
        }
    };
    let mut packages = Vec::new();
    let mut index_by_path = HashMap::new();
    let mut visiting = HashSet::new();
    // Keyed on the same canonical directory the dedup keys on, so one package
    // reached through two edges is reported once however many times it is
    // depended on — §9's "once per build, not once per member".
    let mut hook_report: BTreeMap<PathBuf, DependencyHooks> = BTreeMap::new();
    // E90 (diagnostics-standard.md C3a): the enclosing workspace root's
    // declared members, resolved once against the ENTRY's directory — the
    // packages the demotion carve-out treats as the user's own code, wherever
    // in the graph their edge appears. Read only when there are dependencies
    // to classify, so a dependency-free package still reads no file outside
    // its own directory.
    let members = if declared.is_empty() {
        HashSet::new()
    } else {
        workspace_member_dirs(package_dir)?
    };
    let entry_dependencies = resolve_dependency_edges(
        declared,
        package_dir,
        &mut packages,
        &mut index_by_path,
        &mut visiting,
        &members,
        git,
        &mut hook_report,
    )?;
    Ok((
        Workspace {
            packages,
            entry_dependencies,
            macro_limits,
        },
        hook_report.into_values().collect(),
    ))
}

/// Resolves a `[library]`'s layered [`PackageSpec`] from its package directory `dir`
/// (with its `vilan.toml`). A directory with no manifest is a base-only library (its
/// own `dir` is the base layer). Dependency edges are left empty — this resolves the
/// library's *own* layer structure (for `std`, and for the platform contract check),
/// not a full dependency build.
pub fn resolve_library(dir: &Path) -> PackageSpec {
    // Through the one reader, so a manifest that exists only in the overlay
    // resolves — which is the whole toolchain's situation under wasm. This does
    // not change the editor: the language server deliberately never REGISTERS a
    // `vilan.toml` overlay (a manifest edit is meant to take effect on save),
    // so the lookup misses and this falls through to disk exactly as before.
    if let Ok(contents) = crate::util::read_source(dir.join("vilan.toml")) {
        if let Ok((manifest, _)) = Manifest::parse(&contents) {
            if let Some(library) = manifest.library {
                // A library's OWN spec (std's, a contract check's) is never a
                // workspace member — membership is a property of the entry's
                // workspace, decided during `resolve_workspace` (E90).
                return library_spec(dir, &library, Vec::new(), false);
            }
        }
    }
    PackageSpec {
        base_root: dir.to_path_buf(),
        layers: Vec::new(),
        dependencies: Vec::new(),
        surface: true,
        member: false,
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
/// platform patterns it serves), the already-resolved dependency edges, and
/// whether the package is a declared workspace member (E90).
fn library_spec(
    dir: &Path,
    library: &Library,
    dependencies: Vec<(String, usize)>,
    member: bool,
) -> PackageSpec {
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
        member,
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

/// The nearest enclosing workspace root of `package_dir` — the closest ancestor
/// (starting at `package_dir` itself) whose `vilan.toml` declares a `[project]`
/// — together with that whole `[project]` declaration: its `packages` (the
/// declared members, E90's carve-out reads them) and its
/// `[project.dependencies]` (the table a member inherits from).
///
/// Two properties are deliberate. **The walk is lazy**: it runs only when a
/// manifest actually writes `project = true` or the resolution has dependency
/// packages to classify as members (E90), so a dependency-free package that
/// inherits nothing reads no file outside its own directory and behaves
/// exactly as it did before inheritance existed. And **membership is not
/// required for inheritance**: as in P2's Q5, `[project] packages` is the
/// *build set*, not a visibility list, so what makes a package a member for
/// inheritance is simply living under the project root. (Demotion is the
/// opposite, by the owner's E90 ruling: there, membership IS the `packages`
/// declaration — see [`workspace_member_dirs`].)
///
/// A `vilan.toml` on the way up that is not a `[project]` (the member's own,
/// a nested package) is skipped; one that cannot be read or parsed stops the
/// walk with that error, since staying quiet would report "no workspace" for a
/// workspace that is merely broken.
fn enclosing_project(package_dir: &Path) -> Result<Option<(PathBuf, Project)>, WorkspaceError> {
    let start = crate::util::canonical_path(package_dir);
    for directory in start.ancestors() {
        let manifest_path = directory.join("vilan.toml");
        if !manifest_path.is_file() {
            continue;
        }
        // Every failure here is a failure of the manifest being read, which is
        // never the one resolution started from — so each is addressed to it.
        let contents = std::fs::read_to_string(&manifest_path).map_err(|error| {
            WorkspaceError::broken(format!("cannot read {}: {error}", manifest_path.display()))
                .declared_in_manifest(&manifest_path)
        })?;
        let (manifest, _warnings) = Manifest::parse(&contents).map_err(|error| {
            WorkspaceError::broken(format!("invalid {}: {error}", manifest_path.display()))
                .declared_in_manifest(&manifest_path)
        })?;
        if manifest.project.is_none() {
            continue;
        }
        // The project root is about to be read for its declarations, so it is
        // held to the same standard as any manifest resolution reads.
        let errors = manifest.validate();
        if !errors.is_empty() {
            return Err(WorkspaceError::broken(format!(
                "invalid {}:\n  - {}",
                manifest_path.display(),
                errors.join("\n  - ")
            ))
            .declared_in_manifest(&manifest_path));
        }
        let project = manifest.project.expect("checked just above");
        return Ok(Some((directory.to_path_buf(), project)));
    }
    Ok(None)
}

/// The canonical directories the workspace enclosing `package_dir` DECLARES as
/// members (`[project] packages`) — the membership test behind E90's demotion
/// carve-out (diagnostics-standard.md C3a): a dependency package resolving to
/// one of these is the user's own code, so the analyzer never demotes it.
/// Empty when no ancestor declares a `[project]`.
///
/// Membership is the declaration and nothing else (the owner's rule): a path
/// dependency living under the project directory but absent from `packages` is
/// external, and a declared member is a member however its edge was written —
/// never a path-containment heuristic. A git dependency can never match: its
/// checkout lives in the cache, which no root would declare.
fn workspace_member_dirs(package_dir: &Path) -> Result<HashSet<PathBuf>, WorkspaceError> {
    Ok(match enclosing_project(package_dir)? {
        Some((project_dir, project)) => project
            .packages
            .iter()
            .map(|member| crate::util::canonical_path(&project_dir.join(member)))
            .collect(),
        None => HashSet::new(),
    })
}

/// What a dependency table declares, for an error that has to say what IS
/// available: ``it declares `a`, `b` `` — or that it declares nothing.
fn declared_names(dependencies: &BTreeMap<String, Dependency>) -> String {
    if dependencies.is_empty() {
        return "its `[project.dependencies]` is empty".to_string();
    }
    let names: Vec<String> = dependencies
        .keys()
        .map(|name| format!("`{name}`"))
        .collect();
    format!("its `[project.dependencies]` declares {}", names.join(", "))
}

/// Resolves one package's dependency edges to `(import name, index)` pairs,
/// loading each referenced package (transitively) into `packages`. `index_by_path`
/// dedups a shared dependency; `visiting` is the in-progress stack for cycle
/// detection. Paths are relative to `base_dir` (the depending package's directory)
/// — or, for an inherited declaration, to the workspace root that declared it.
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
    members: &HashSet<PathBuf>,
    git: &GitDeps,
    // Tier 2's report, accumulated as the walk reads each dependency manifest
    // ([`DependencyHooks`]). Keyed on the canonical directory so a package
    // reached through several edges is one row.
    hook_report: &mut BTreeMap<PathBuf, DependencyHooks>,
) -> Result<Vec<(String, usize)>, WorkspaceError> {
    let mut edges = Vec::new();
    // The enclosing workspace root, read at most once per manifest and only if
    // some declaration actually asks to inherit.
    let mut project: Option<(PathBuf, Project)> = None;
    let mut project_searched = false;
    for (import_name, dependency) in dependencies {
        // `project = true` substitutes the workspace root's declaration — and
        // with it the directory the declaration's `path` is relative to. A path
        // written in `[project.dependencies]` is written from the project root,
        // which is the whole point of declaring it in one place.
        let (declaration, declaration_dir, inherited_from) = match dependency.source() {
            Ok(DependencySource::Inherited) => {
                if !project_searched {
                    project = enclosing_project(base_dir)?;
                    project_searched = true;
                }
                // The two failures below are the MEMBER's: it wrote `project =
                // true` where there is no project, or for a name the project
                // does not declare. They stay addressed to the member manifest,
                // which is where the opt-in that has to change lives.
                let Some((project_dir, root_project)) = &project else {
                    return Err(WorkspaceError::broken(format!(
                        "dependency `{import_name}` sets `project = true`, but `{}` is not \
                         inside a workspace: inheritance reads the `[project.dependencies]` \
                         of the nearest ancestor `vilan.toml` that declares a `[project]`",
                        base_dir.display()
                    )));
                };
                let Some(inherited) = root_project.dependencies.get(import_name) else {
                    return Err(WorkspaceError::broken(format!(
                        "dependency `{import_name}` sets `project = true`, but {} declares \
                         no `{import_name}`; {}",
                        project_dir.join("vilan.toml").display(),
                        declared_names(&root_project.dependencies)
                    )));
                };
                (
                    inherited.clone(),
                    project_dir.clone(),
                    Some(project_dir.join("vilan.toml")),
                )
            }
            _ => (dependency.clone(), base_dir.to_path_buf(), None),
        };
        // Everything from here on resolves *the declaration*, so a failure
        // belongs to the manifest that wrote it — the project root for an
        // inherited dependency, and (already, by omission) this package's own
        // manifest otherwise. Both halves in one place so the diagnostic's
        // address and its wording can never drift apart.
        let attribute = |error: WorkspaceError| match &inherited_from {
            None => error,
            Some(manifest) => error
                .map_message(|message| {
                    format!(
                        "{message}; `{import_name}` is inherited from {}",
                        manifest.display()
                    )
                })
                .declared_in_manifest(manifest),
        };
        // `validate` has already rejected registry dependencies, malformed
        // git ones, and an inheriting `[project.dependencies]` entry, so only
        // resolvable declarations reach here.
        let dependency_dir = match declaration.source() {
            Ok(DependencySource::Path(relative)) => declaration_dir.join(relative),
            Ok(DependencySource::Git(source)) => {
                crate::git_dep::materialize(&source, git, import_name).map_err(|error| {
                    attribute(error.map_message(|message| {
                        format!("git dependency `{import_name}`: {message}")
                    }))
                })?
            }
            Ok(DependencySource::Inherited) | Ok(DependencySource::Registry) | Err(_) => continue,
        };
        // The dedup map's key AND the cycle set's member. Both must be one
        // canonical form (`windows-support.md` §5): a raw-string fallback lets
        // `../lib` and `../lib/.` key two entries for one package, and Windows'
        // `\\?\` prefix would do the same for a mix of on-disk and not-yet-on-disk
        // directories — duplicating a package, or hiding a cycle.
        let canonical = crate::util::canonical_path(&dependency_dir);
        // A grant is a property of the DECLARATION, and one package can be
        // depended on from several manifests — so a second edge that grants
        // has to reach the row the first edge wrote, or a real opt-in would be
        // reported as absent.
        let grants = declaration.grants_build_hooks();
        if let Some(&index) = index_by_path.get(&canonical) {
            if let Some(recorded) = hook_report.get_mut(&canonical) {
                recorded.opted_in |= grants;
            }
            edges.push((import_name.clone(), index));
            continue;
        }
        if !visiting.insert(canonical.clone()) {
            return Err(attribute(WorkspaceError::broken(format!(
                "dependency cycle through `{}`",
                dependency_dir.display()
            ))));
        }
        let manifest = load_manifest(&dependency_dir).map_err(|error| {
            attribute(WorkspaceError::broken(format!(
                "dependency `{import_name}`: {error}"
            )))
        })?;
        // Read here, where the dependency's manifest is already in hand and
        // before it is taken apart below. Whether it is a `[library]` or a
        // `[package]` makes no difference: the question is only whether this
        // dependency asks to run commands on the machine building it.
        if manifest.declares_build_hooks() {
            hook_report.insert(
                canonical.clone(),
                DependencyHooks {
                    name: import_name.clone(),
                    directory: dependency_dir.clone(),
                    opted_in: grants,
                },
            );
        }
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
                return Err(attribute(WorkspaceError::broken(format!(
                    "dependency `{import_name}` at `{}` is not a `[library]` or `[package]`",
                    dependency_dir.display()
                ))));
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
            members,
            git,
            hook_report,
        )?;
        visiting.remove(&canonical);
        let index = packages.len();
        // E90 (diagnostics-standard.md C3a): a dependency that IS a declared
        // workspace member is the user's own code — the analyzer reads this
        // flag to keep the package out of its demotion set. Decided on the
        // same canonical key the dedup uses, by the root manifest's
        // `packages` declaration alone.
        let member = members.contains(&canonical);
        let spec = match &library {
            Some(library) => library_spec(&dependency_dir, library, dependency_edges, member),
            // A `[package]` dependency: base-only over its `src/`, no layers,
            // no `lib.vl` surface.
            None => PackageSpec {
                base_root: dependency_dir.join("src"),
                layers: Vec::new(),
                dependencies: dependency_edges,
                surface: false,
                member,
            },
        };
        packages.push(spec);
        index_by_path.insert(canonical, index);
        edges.push((import_name.clone(), index));
    }
    Ok(edges)
}

/// The refusal for `split = true` outside a browser leg. Route chunks are
/// fetched with `import()` when a navigation advances the route signal
/// (`bundle-splitting.md` §2), so the key means nothing on a process leg —
/// silently ignoring it would leave the author believing a bundle was split.
fn split_needs_a_browser_leg(key: &str, declared: &str) -> String {
    format!(
        "`{key}` applies to a `browser` leg only: route chunks are fetched \
         with `import()` when a navigation advances the route signal, and \
         this leg targets `{declared}`"
    )
}

/// Why `name` is reserved as a package name, if it is. Three of the reserved
/// names are the import roots the toolchain itself owns (spec §4.2's three
/// namespaces). A dependency key under one of them would silently shadow the
/// root (`std`, `macro_std` — the analyzer binds dependency edges before the
/// global roots) or be silently unreachable behind it (`pkg` — the hardcoded
/// self-package root wins); a `[package]` claiming one as its own name stakes
/// the same claim for the day it is depended on or published (std-shape.md
/// §4, L12). `vilan` is the fourth: not an import root today, but the
/// language's own name, held ahead of the official-package namespace decision
/// (kolt.local 026's recorded first step, 2026-08-26) — reserved before any
/// user package can claim it, whichever way that re-ruling lands. `[library]
/// name` deliberately carries NO check: the reserved names are reserved *for*
/// the toolchain's own `[library]` manifests (std is `[library] name =
/// "std"`, likewise macro_std), a context-free validation cannot tell the
/// owner from an impostor, and a library's own name — unlike a dependency key
/// — never binds an import root.
pub(crate) fn reserved_package_name(name: &str) -> Option<&'static str> {
    match name {
        "std" => Some("the standard library owns it"),
        "pkg" => Some("it always means the importing package's own modules"),
        "macro_std" => Some("the macro standard library owns it"),
        "vilan" => Some("the language owns its own name"),
        _ => None,
    }
}

/// The refusal for a reserved package name: the rule, the whole reserved set,
/// and the one fix (`renamed` names the thing to rename — the package or the
/// dependency).
fn reserved_name_refusal(name: &str, reason: &str, renamed: &str) -> String {
    format!(
        "`{name}` is a reserved package name: {reason} (`std`, `pkg`, \
         `macro_std`, and `vilan` are all reserved); rename the {renamed}"
    )
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
            project: None,
            build_hooks: None,
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
            &HashSet::new(),
            &GitDeps::cache_only(root.join("unused-git-cache")),
            &mut BTreeMap::new(),
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

    // The reserved package names (L12, std-shape.md §4): `std`, `pkg`, and
    // `macro_std` are the import roots the toolchain owns. Before the refusal,
    // a dependency named `std` silently shadowed the whole standard library
    // (`import std::print` stopped resolving), one named `pkg` was silently
    // unreachable behind the self-package root, and one named `macro_std`
    // shadowed the macro world's std — measured 2026-08-24, the plant these
    // pins were proven red against. `vilan` joined the set 2026-08-26
    // (kolt.local 026's recorded first step): not an import root, but the
    // language's own name, held ahead of the official-package namespace
    // re-ruling so no user package claims it first.

    /// The refusal's exact head for `name`, as [`reserved_name_refusal`]
    /// builds it — asserted verbatim so the ledger row's key stays honest.
    fn reserved_head(name: &str, reason: &str, renamed: &str) -> String {
        format!(
            "`{name}` is a reserved package name: {reason} (`std`, `pkg`, \
             `macro_std`, and `vilan` are all reserved); rename the {renamed}"
        )
    }

    #[test]
    fn a_package_named_std_is_refused() {
        let manifest = parse("[package]\nname = \"std\"\n");
        let errors = manifest.validate();
        assert_eq!(
            errors,
            vec![reserved_head(
                "std",
                "the standard library owns it",
                "package"
            )]
        );
    }

    #[test]
    fn a_package_named_pkg_is_refused() {
        let manifest = parse("[package]\nname = \"pkg\"\n");
        let errors = manifest.validate();
        assert_eq!(
            errors,
            vec![reserved_head(
                "pkg",
                "it always means the importing package's own modules",
                "package"
            )]
        );
    }

    #[test]
    fn a_package_named_macro_std_is_refused() {
        let manifest = parse("[package]\nname = \"macro_std\"\n");
        let errors = manifest.validate();
        assert_eq!(
            errors,
            vec![reserved_head(
                "macro_std",
                "the macro standard library owns it",
                "package"
            )]
        );
    }

    #[test]
    fn a_package_named_vilan_is_refused() {
        let manifest = parse("[package]\nname = \"vilan\"\n");
        let errors = manifest.validate();
        assert_eq!(
            errors,
            vec![reserved_head(
                "vilan",
                "the language owns its own name",
                "package"
            )]
        );
    }

    #[test]
    fn a_dependency_named_std_is_refused() {
        let manifest = parse(
            "[package]\nname = \"app\"\n[package.dependencies]\n\
             std = { path = \"../std\" }\n",
        );
        let errors = manifest.validate();
        assert_eq!(
            errors,
            vec![reserved_head(
                "std",
                "the standard library owns it",
                "dependency"
            )]
        );
    }

    #[test]
    fn a_dependency_named_pkg_is_refused() {
        let manifest = parse(
            "[package]\nname = \"app\"\n[package.dependencies]\n\
             pkg = { path = \"../helpers\" }\n",
        );
        let errors = manifest.validate();
        assert_eq!(
            errors,
            vec![reserved_head(
                "pkg",
                "it always means the importing package's own modules",
                "dependency"
            )]
        );
    }

    #[test]
    fn a_dependency_named_macro_std_is_refused() {
        let manifest = parse(
            "[package]\nname = \"app\"\n[package.dependencies]\n\
             macro_std = { path = \"../macro_std\" }\n",
        );
        let errors = manifest.validate();
        assert_eq!(
            errors,
            vec![reserved_head(
                "macro_std",
                "the macro standard library owns it",
                "dependency"
            )]
        );
    }

    #[test]
    fn a_dependency_named_vilan_is_refused() {
        let manifest = parse(
            "[package]\nname = \"app\"\n[package.dependencies]\n\
             vilan = { path = \"../vilan\" }\n",
        );
        let errors = manifest.validate();
        assert_eq!(
            errors,
            vec![reserved_head(
                "vilan",
                "the language owns its own name",
                "dependency"
            )]
        );
    }

    #[test]
    fn a_library_dependency_named_std_is_refused() {
        let manifest = parse(
            "[library]\nname = \"helpers\"\n[library.dependencies]\n\
             std = { path = \"../std\" }\n",
        );
        let errors = manifest.validate();
        assert_eq!(
            errors,
            vec![reserved_head(
                "std",
                "the standard library owns it",
                "dependency"
            )]
        );
    }

    #[test]
    fn a_project_dependency_named_std_is_refused() {
        // The workspace root's table is the one members inherit FROM, so a
        // reserved key there would hand every opted-in member the shadowing.
        let manifest = parse(
            "[project]\npackages = [\"app\"]\n[project.dependencies]\n\
             std = { path = \"std\" }\n",
        );
        let errors = manifest.validate();
        assert_eq!(
            errors,
            vec![reserved_head(
                "std",
                "the standard library owns it",
                "dependency"
            )]
        );
    }

    #[test]
    fn a_reserved_dependency_with_a_broken_source_reports_both_problems() {
        // Different root causes, both actionable: the name is reserved AND the
        // declaration pins nothing.
        let manifest = parse(
            "[package]\nname = \"app\"\n[package.dependencies]\n\
             std = { git = \"https://example.com/repo\" }\n",
        );
        let errors = manifest.validate();
        assert_eq!(errors.len(), 2, "both problems report: {errors:?}");
        assert!(errors[0].contains("reserved package name"));
        assert!(errors[1].contains("no point to pin"));
    }

    #[test]
    fn a_library_named_std_is_the_standard_librarys_own_shape() {
        // `[library] name` deliberately carries no reserved check: std itself
        // is `[library] name = "std"` (likewise macro_std), and a library's
        // own name — unlike a dependency key — never binds an import root.
        let std_manifest = parse("[library]\nname = \"std\"\n");
        assert!(std_manifest.validate().is_empty());
        let macro_std_manifest = parse("[library]\nname = \"macro_std\"\n");
        assert!(macro_std_manifest.validate().is_empty());
    }

    #[test]
    fn a_near_miss_of_a_reserved_name_is_not_refused() {
        // Exact match only: `stdx`, `std_helpers`, `pkg_tools`,
        // `macro_stdlib`, `vilan_ui` are ordinary names.
        let manifest = parse(
            "[package]\nname = \"stdx\"\n[package.dependencies]\n\
             std_helpers = { path = \"../std_helpers\" }\n\
             pkg_tools = { path = \"../pkg_tools\" }\n\
             macro_stdlib = { path = \"../macro_stdlib\" }\n\
             vilan_ui = { path = \"../vilan_ui\" }\n",
        );
        assert!(manifest.validate().is_empty());
    }

    #[test]
    fn a_legal_dependency_name_still_resolves_end_to_end() {
        // The refusal sits on `Manifest::validate`, which every
        // `resolve_workspace` rides — so the complement is pinned through the
        // real resolution path, not just `validate()`.
        let root = workspace_tree(
            "legal-name-loads",
            &[
                (
                    "app/vilan.toml",
                    "[package]\nname = \"app\"\n[package.dependencies]\n\
                     shapes = { path = \"../shapes\" }\n",
                ),
                ("app/src/main.vl", ""),
                ("shapes/vilan.toml", "[library]\nname = \"shapes\"\n"),
                ("shapes/src/lib.vl", ""),
            ],
        );
        let workspace = resolve_workspace(
            &root.join("app"),
            &GitDeps::cache_only(root.join("unused-git-cache")),
        )
        .expect("a legal name resolves");
        assert_eq!(workspace.packages.len(), 1);
        assert_eq!(workspace.entry_dependencies.len(), 1);
        assert_eq!(workspace.entry_dependencies[0].0, "shapes");
        assert!(
            !workspace.packages[0].member,
            "no enclosing `[project]`, so no member (E90)"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_reserved_dependency_name_is_refused_through_resolution_too() {
        // The same channel, refusing: `resolve_workspace` never resolves a
        // manifest `validate` rejects, so the CLI and the editor both see the
        // refusal before any dependency work happens.
        let root = workspace_tree(
            "reserved-name-refuses",
            &[
                (
                    "app/vilan.toml",
                    "[package]\nname = \"app\"\n[package.dependencies]\n\
                     std = { path = \"../shapes\" }\n",
                ),
                ("app/src/main.vl", ""),
                ("shapes/vilan.toml", "[library]\nname = \"shapes\"\n"),
                ("shapes/src/lib.vl", ""),
            ],
        );
        let error = resolve_workspace(
            &root.join("app"),
            &GitDeps::cache_only(root.join("unused-git-cache")),
        )
        .expect_err("a reserved name refuses");
        assert!(!error.is_unfetched());
        assert!(
            error.message().contains("`std` is a reserved package name"),
            "unexpected message: {}",
            error.message()
        );
        let _ = std::fs::remove_dir_all(&root);
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
        assert!(error.is_unfetched(), "a cache miss is not a fault: {error}");
        assert!(
            error.message().contains("git dependency `shapes`"),
            "{error}"
        );
        assert!(
            error.message().contains("not in the local cache"),
            "{error}"
        );
        assert!(error.message().contains("vilan build"), "{error}");
        assert!(!cache.exists(), "cache-only must not create the cache root");
        let _ = std::fs::remove_dir_all(&root);
    }

    // ── `[project.dependencies]` inheritance (proposal/distribution.md §5's
    // riders; the P2 Q4 deferral) ──
    //
    // The shape: a workspace root declares a dependency once, and a member opts
    // IN per dependency with `project = true`. Nothing is inherited implicitly,
    // so there is no shadowing question to answer — a member either inherits or
    // declares, never both, and `validate` says so.

    /// Writes `files` (relative path → contents) under a fresh temp directory
    /// named after `label`, returning the root. Directories are created as
    /// needed, so a whole workspace is one literal.
    fn workspace_tree(label: &str, files: &[(&str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("vilan-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (relative, contents) in files {
            let path = root.join(relative);
            std::fs::create_dir_all(path.parent().expect("a parent directory"))
                .expect("create the directory");
            std::fs::write(&path, contents).expect("write the file");
        }
        root
    }

    /// A `[library]` package's two files at `directory`, for a tree literal.
    /// The library's name is the directory's last segment.
    fn library_files(directory: &str) -> [(String, String); 2] {
        let name = directory.rsplit('/').next().expect("a last segment");
        [
            (
                format!("{directory}/vilan.toml"),
                format!("[library]\nname = \"{name}\"\n"),
            ),
            (format!("{directory}/src/lib.vl"), String::new()),
        ]
    }

    #[test]
    fn a_member_inherits_the_projects_declaration_by_opting_in() {
        let root = workspace_tree(
            "inherit-optin",
            &[
                (
                    "vilan.toml",
                    "[project]\npackages = [\"app\"]\n[project.dependencies]\n\
                     shapes = { path = \"shapes\" }\n",
                ),
                ("shapes/vilan.toml", "[library]\nname = \"shapes\"\n"),
                ("shapes/src/lib.vl", ""),
                (
                    "app/vilan.toml",
                    "[package]\nname = \"app\"\n[package.dependencies]\n\
                     shapes = { project = true }\n",
                ),
                ("app/src/main.vl", ""),
            ],
        );
        let workspace = resolve_workspace(
            &root.join("app"),
            &GitDeps::cache_only(root.join("unused-git-cache")),
        )
        .expect("the member inherits `shapes`");
        assert_eq!(workspace.entry_dependencies.len(), 1);
        assert_eq!(workspace.entry_dependencies[0].0, "shapes");
        assert_eq!(workspace.packages.len(), 1);
        assert!(
            workspace.packages[0]
                .base_root
                .ends_with(Path::new("shapes").join("src")),
            "{:?}",
            workspace.packages[0].base_root
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_inherited_path_is_relative_to_the_project_root_not_the_member() {
        // THE base-directory rule, pinned against a decoy: `path = "shapes"` in
        // `[project.dependencies]` is written from the project root, so a
        // same-named library sitting next to the member must NOT be the one
        // that resolves. (Without the rule the decoy wins, silently.)
        let mut files: Vec<(String, String)> = vec![
            (
                "vilan.toml".to_string(),
                "[project]\npackages = [\"app\"]\n[project.dependencies]\n\
                 shapes = { path = \"shapes\" }\n"
                    .to_string(),
            ),
            (
                "app/vilan.toml".to_string(),
                "[package]\nname = \"app\"\n[package.dependencies]\n\
                 shapes = { project = true }\n"
                    .to_string(),
            ),
            ("app/src/main.vl".to_string(), String::new()),
        ];
        files.extend(library_files("shapes"));
        files.extend(library_files("app/shapes"));
        let borrowed: Vec<(&str, &str)> = files
            .iter()
            .map(|(path, contents)| (path.as_str(), contents.as_str()))
            .collect();
        let root = workspace_tree("inherit-basedir", &borrowed);

        let workspace = resolve_workspace(
            &root.join("app"),
            &GitDeps::cache_only(root.join("unused-git-cache")),
        )
        .expect("the inherited path resolves");
        let resolved = &workspace.packages[0].base_root;
        assert!(
            resolved.starts_with(crate::util::canonical_path(&root).join("shapes"))
                || resolved.starts_with(root.join("shapes")),
            "expected the project root's `shapes`, got {resolved:?}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_git_dependency_inherits_identically() {
        // Inheritance substitutes the DECLARATION, whatever kind it is — a git
        // dependency needs nothing of its own (and the warm cache keeps this
        // offline).
        let root = workspace_tree(
            "inherit-git",
            &[
                (
                    "vilan.toml",
                    "[project]\npackages = [\"app\"]\n[project.dependencies]\n\
                     shapes = { git = \"https://example.invalid/org/shapes\", tag = \"v1.2.0\" }\n",
                ),
                (
                    "app/vilan.toml",
                    "[package]\nname = \"app\"\n[package.dependencies]\n\
                     shapes = { project = true }\n",
                ),
                ("app/src/main.vl", ""),
            ],
        );
        let cache = root.join("cache");
        let source = crate::git_dep::GitSource {
            url: "https://example.invalid/org/shapes".to_string(),
            reference: GitRef::Tag("v1.2.0".to_string()),
        };
        seed_cache_entry(&cache, &source, "shapes");

        let workspace = resolve_workspace(&root.join("app"), &GitDeps::cache_only(&cache))
            .expect("the inherited git dependency resolves from the warm cache");
        assert_eq!(workspace.entry_dependencies.len(), 1);
        assert!(
            workspace.packages[0]
                .base_root
                .starts_with(crate::git_dep::entry_path(&cache, &source)),
            "{:?}",
            workspace.packages[0].base_root
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_dependency_of_a_member_inherits_from_the_same_project() {
        // Inheritance lives in the general edge loop, not in a special case for
        // the entry package: a library inside the workspace opts in the same
        // way, and its own inherited path is relative to the project root too.
        let root = workspace_tree(
            "inherit-transitive",
            &[
                (
                    "vilan.toml",
                    "[project]\npackages = [\"app\"]\n[project.dependencies]\n\
                     shapes = { path = \"shapes\" }\n",
                ),
                ("shapes/vilan.toml", "[library]\nname = \"shapes\"\n"),
                ("shapes/src/lib.vl", ""),
                (
                    "middle/vilan.toml",
                    "[library]\nname = \"middle\"\n[library.dependencies]\n\
                     shapes = { project = true }\n",
                ),
                ("middle/src/lib.vl", ""),
                (
                    "app/vilan.toml",
                    "[package]\nname = \"app\"\n[package.dependencies]\n\
                     middle = { path = \"../middle\" }\n",
                ),
                ("app/src/main.vl", ""),
            ],
        );
        let workspace = resolve_workspace(
            &root.join("app"),
            &GitDeps::cache_only(root.join("unused-git-cache")),
        )
        .expect("the library's own inherited dependency resolves");
        assert_eq!(workspace.packages.len(), 2, "shapes and middle");
        let middle = workspace
            .packages
            .iter()
            .find(|spec| spec.base_root.ends_with(Path::new("middle").join("src")))
            .expect("middle is in the workspace");
        assert_eq!(
            middle.dependencies.len(),
            1,
            "middle depends on the inherited shapes"
        );
        assert_eq!(middle.dependencies[0].0, "shapes");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_member_that_does_not_opt_in_inherits_nothing() {
        // Inheritance is ADDITIVE and opt-in: declaring dependencies at the
        // workspace root changes nothing for a member that never asks — which
        // is also why there is no shadowing rule to learn.
        let root = workspace_tree(
            "inherit-optout",
            &[
                (
                    "vilan.toml",
                    "[project]\npackages = [\"app\"]\n[project.dependencies]\n\
                     shapes = { path = \"shapes\" }\n",
                ),
                ("shapes/vilan.toml", "[library]\nname = \"shapes\"\n"),
                ("shapes/src/lib.vl", ""),
                ("app/vilan.toml", "[package]\nname = \"app\"\n"),
                ("app/src/main.vl", ""),
            ],
        );
        let workspace = resolve_workspace(
            &root.join("app"),
            &GitDeps::cache_only(root.join("unused-git-cache")),
        )
        .expect("resolves");
        assert!(workspace.entry_dependencies.is_empty());
        assert!(workspace.packages.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_members_own_declaration_of_the_same_name_is_the_one_that_resolves() {
        // The "override" question, answered by construction: a member that
        // declares its own `shapes` never consults the project's, because
        // inheritance only happens where `project = true` is written.
        let mut files: Vec<(String, String)> = vec![
            (
                "vilan.toml".to_string(),
                "[project]\npackages = [\"app\"]\n[project.dependencies]\n\
                 shapes = { path = \"shapes\" }\n"
                    .to_string(),
            ),
            (
                "app/vilan.toml".to_string(),
                "[package]\nname = \"app\"\n[package.dependencies]\n\
                 shapes = { path = \"own_shapes\" }\n"
                    .to_string(),
            ),
            ("app/src/main.vl".to_string(), String::new()),
        ];
        files.extend(library_files("shapes"));
        files.extend(library_files("app/own_shapes"));
        let borrowed: Vec<(&str, &str)> = files
            .iter()
            .map(|(path, contents)| (path.as_str(), contents.as_str()))
            .collect();
        let root = workspace_tree("inherit-own", &borrowed);

        let workspace = resolve_workspace(
            &root.join("app"),
            &GitDeps::cache_only(root.join("unused-git-cache")),
        )
        .expect("resolves");
        assert!(
            workspace.packages[0]
                .base_root
                .ends_with(Path::new("own_shapes").join("src")),
            "{:?}",
            workspace.packages[0].base_root
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    // --- E90 (diagnostics-standard.md C3a): workspace membership ---
    // A dependency package that IS a declared member of the entry's enclosing
    // `[project]` resolves with `member: true` — the flag the analyzer's
    // demotion carve-out reads. Membership is the root manifest's `packages`
    // declaration, never a path test.

    #[test]
    fn a_declared_project_member_resolves_flagged_as_a_member() {
        let root = workspace_tree(
            "member-declared",
            &[
                (
                    "vilan.toml",
                    "[project]\npackages = [\"app\", \"common\"]\n",
                ),
                (
                    "app/vilan.toml",
                    "[package]\nname = \"app\"\n[package.dependencies]\n\
                     common = { path = \"../common\" }\n",
                ),
                ("app/src/main.vl", ""),
                ("common/vilan.toml", "[library]\nname = \"common\"\n"),
                ("common/src/lib.vl", ""),
            ],
        );
        let workspace = resolve_workspace(
            &root.join("app"),
            &GitDeps::cache_only(root.join("unused-git-cache")),
        )
        .expect("a member workspace resolves");
        assert_eq!(workspace.packages.len(), 1);
        assert!(
            workspace.packages[0].member,
            "`common` is in `[project] packages`, so its spec is a member"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_unlisted_path_dependency_under_the_project_root_is_not_a_member() {
        // The "never by path" half of the owner's rule: `common` LIVES inside
        // the project directory, but the root's `packages` does not declare
        // it — path containment must not make it a member.
        let root = workspace_tree(
            "member-unlisted",
            &[
                ("vilan.toml", "[project]\npackages = [\"app\"]\n"),
                (
                    "app/vilan.toml",
                    "[package]\nname = \"app\"\n[package.dependencies]\n\
                     common = { path = \"../common\" }\n",
                ),
                ("app/src/main.vl", ""),
                ("common/vilan.toml", "[library]\nname = \"common\"\n"),
                ("common/src/lib.vl", ""),
            ],
        );
        let workspace = resolve_workspace(
            &root.join("app"),
            &GitDeps::cache_only(root.join("unused-git-cache")),
        )
        .expect("resolves");
        assert_eq!(workspace.packages.len(), 1);
        assert!(
            !workspace.packages[0].member,
            "an undeclared directory is external, wherever it lives"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_git_dependency_is_never_a_member() {
        // A git dependency materializes into the cache, which no `[project]`
        // declares — so even inside a workspace it stays external and keeps
        // the E84 demotion.
        let root = std::env::temp_dir().join(format!("vilan-member-git-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let cache = root.join("cache");
        let source = crate::git_dep::GitSource {
            url: "https://example.invalid/org/shapes".to_string(),
            reference: GitRef::Tag("v1.0.0".to_string()),
        };
        seed_cache_entry(&cache, &source, "shapes");
        std::fs::create_dir_all(root.join("app").join("src")).expect("create the app");
        std::fs::write(root.join("vilan.toml"), "[project]\npackages = [\"app\"]\n")
            .expect("the project manifest");
        std::fs::write(
            root.join("app").join("vilan.toml"),
            "[package]\nname = \"app\"\n[package.dependencies]\n\
             shapes = { git = \"https://example.invalid/org/shapes\", tag = \"v1.0.0\" }\n",
        )
        .expect("the app manifest");
        let workspace = resolve_workspace(&root.join("app"), &GitDeps::cache_only(&cache))
            .expect("a warm cache resolves");
        assert_eq!(workspace.packages.len(), 1);
        assert!(
            !workspace.packages[0].member,
            "a cached checkout is nobody's member"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_member_inherited_from_the_project_root_is_still_a_member() {
        // Membership rides the resolved DIRECTORY, not the edge's spelling: a
        // member reached through `project = true` inheritance canonicalizes to
        // the same declared directory a direct path edge would.
        let root = workspace_tree(
            "member-inherited",
            &[
                (
                    "vilan.toml",
                    "[project]\npackages = [\"app\", \"common\"]\n[project.dependencies]\n\
                     common = { path = \"common\" }\n",
                ),
                (
                    "app/vilan.toml",
                    "[package]\nname = \"app\"\n[package.dependencies]\n\
                     common = { project = true }\n",
                ),
                ("app/src/main.vl", ""),
                ("common/vilan.toml", "[library]\nname = \"common\"\n"),
                ("common/src/lib.vl", ""),
            ],
        );
        let workspace = resolve_workspace(
            &root.join("app"),
            &GitDeps::cache_only(root.join("unused-git-cache")),
        )
        .expect("resolves");
        assert_eq!(workspace.packages.len(), 1);
        assert!(
            workspace.packages[0].member,
            "the inherited edge resolves to the declared member directory"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_nearest_enclosing_project_is_the_one_inherited_from() {
        // Nested workspaces: the walk stops at the FIRST ancestor that declares
        // a `[project]`, so an inner workspace's declarations win over an outer
        // one's — the ordering-sensitive case of the ancestor walk.
        let mut files: Vec<(String, String)> = vec![
            (
                "vilan.toml".to_string(),
                "[project]\npackages = []\n[project.dependencies]\n\
                 shapes = { path = \"outer_shapes\" }\n"
                    .to_string(),
            ),
            (
                "inner/vilan.toml".to_string(),
                "[project]\npackages = [\"app\"]\n[project.dependencies]\n\
                 shapes = { path = \"inner_shapes\" }\n"
                    .to_string(),
            ),
            (
                "inner/app/vilan.toml".to_string(),
                "[package]\nname = \"app\"\n[package.dependencies]\n\
                 shapes = { project = true }\n"
                    .to_string(),
            ),
            ("inner/app/src/main.vl".to_string(), String::new()),
        ];
        files.extend(library_files("outer_shapes"));
        files.extend(library_files("inner/inner_shapes"));
        let borrowed: Vec<(&str, &str)> = files
            .iter()
            .map(|(path, contents)| (path.as_str(), contents.as_str()))
            .collect();
        let root = workspace_tree("inherit-nested", &borrowed);

        let workspace = resolve_workspace(
            &root.join("inner").join("app"),
            &GitDeps::cache_only(root.join("unused-git-cache")),
        )
        .expect("resolves");
        assert!(
            workspace.packages[0]
                .base_root
                .ends_with(Path::new("inner_shapes").join("src")),
            "{:?}",
            workspace.packages[0].base_root
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn opting_in_to_an_undeclared_name_names_the_projects_set() {
        let root = workspace_tree(
            "inherit-undeclared",
            &[
                (
                    "vilan.toml",
                    "[project]\npackages = [\"app\"]\n[project.dependencies]\n\
                     geometry = { path = \"geometry\" }\n",
                ),
                ("geometry/vilan.toml", "[library]\nname = \"geometry\"\n"),
                ("geometry/src/lib.vl", ""),
                (
                    "app/vilan.toml",
                    "[package]\nname = \"app\"\n[package.dependencies]\n\
                     shapes = { project = true }\n",
                ),
                ("app/src/main.vl", ""),
            ],
        );
        let error = resolve_workspace(
            &root.join("app"),
            &GitDeps::cache_only(root.join("unused-git-cache")),
        )
        .expect_err("nothing to inherit");
        assert!(
            !error.is_unfetched(),
            "a manifest mistake, not a cold cache"
        );
        let message = error.message();
        assert!(message.contains("dependency `shapes`"), "{message}");
        assert!(message.contains("declares no `shapes`"), "{message}");
        assert!(
            message.contains("`[project.dependencies]` declares `geometry`"),
            "{message}"
        );
        // The member wrote the opt-in, so the member is where it is fixed.
        assert_eq!(error.declared_in(), None, "{message}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_broken_inherited_declaration_is_addressed_to_the_project() {
        // distribution.md §7's S5 residual: the member's manifest is correct —
        // it opted in — so the failure has to carry the manifest that WROTE the
        // declaration, or every surface reports a file with nothing to fix.
        let root = workspace_tree(
            "inherit-broken",
            &[
                (
                    "vilan.toml",
                    "[project]\npackages = [\"app\"]\n[project.dependencies]\n\
                     shapes = { path = \"nowhere\" }\n",
                ),
                (
                    "app/vilan.toml",
                    "[package]\nname = \"app\"\n[package.dependencies]\n\
                     shapes = { project = true }\n",
                ),
                ("app/src/main.vl", ""),
            ],
        );
        let error = resolve_workspace(
            &root.join("app"),
            &GitDeps::cache_only(root.join("unused-git-cache")),
        )
        .expect_err("the inherited path does not resolve");
        assert_eq!(
            error.declared_in(),
            Some(
                crate::util::canonical_path(&root)
                    .join("vilan.toml")
                    .as_path()
            ),
            "{}",
            error.message()
        );
        assert!(
            error.message().contains("is inherited from"),
            "{}",
            error.message()
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_broken_own_declaration_is_addressed_nowhere_but_here() {
        // The control for the rule above: a package that declares its own
        // dependency owns the mistake, so nothing is re-addressed and the
        // message gains no inheritance clause.
        let root = workspace_tree(
            "inherit-broken-own",
            &[
                (
                    "vilan.toml",
                    "[project]\npackages = [\"app\"]\n[project.dependencies]\n\
                     shapes = { path = \"shapes\" }\n",
                ),
                ("shapes/vilan.toml", "[library]\nname = \"shapes\"\n"),
                ("shapes/src/lib.vl", ""),
                (
                    "app/vilan.toml",
                    "[package]\nname = \"app\"\n[package.dependencies]\n\
                     shapes = { path = \"../nowhere\" }\n",
                ),
                ("app/src/main.vl", ""),
            ],
        );
        let error = resolve_workspace(
            &root.join("app"),
            &GitDeps::cache_only(root.join("unused-git-cache")),
        )
        .expect_err("the member's own path does not resolve");
        assert_eq!(error.declared_in(), None, "{}", error.message());
        assert!(
            !error.message().contains("is inherited from"),
            "{}",
            error.message()
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_deeper_declaration_keeps_its_own_address_under_an_inherited_one() {
        // The ordering-sensitive case: `app` INHERITS `middle`, and `middle`
        // declares a broken dependency of its own. The inner frame is the one
        // that knows whose declaration failed, so the outer inherited frame
        // must not restamp it onto the project root.
        let root = workspace_tree(
            "inherit-deeper",
            &[
                (
                    "vilan.toml",
                    "[project]\npackages = [\"app\"]\n[project.dependencies]\n\
                     middle = { path = \"middle\" }\n",
                ),
                (
                    "middle/vilan.toml",
                    "[library]\nname = \"middle\"\n[library.dependencies]\n\
                     shapes = { path = \"../nowhere\" }\n",
                ),
                ("middle/src/lib.vl", ""),
                (
                    "app/vilan.toml",
                    "[package]\nname = \"app\"\n[package.dependencies]\n\
                     middle = { project = true }\n",
                ),
                ("app/src/main.vl", ""),
            ],
        );
        let error = resolve_workspace(
            &root.join("app"),
            &GitDeps::cache_only(root.join("unused-git-cache")),
        )
        .expect_err("middle's own dependency does not resolve");
        assert_eq!(error.declared_in(), None, "{}", error.message());
        assert!(
            !error.message().contains("is inherited from"),
            "{}",
            error.message()
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn opting_in_outside_a_workspace_is_an_error_that_says_so() {
        let root = workspace_tree(
            "inherit-no-project",
            &[
                (
                    "app/vilan.toml",
                    "[package]\nname = \"app\"\n[package.dependencies]\n\
                     shapes = { project = true }\n",
                ),
                ("app/src/main.vl", ""),
            ],
        );
        let error = resolve_workspace(
            &root.join("app"),
            &GitDeps::cache_only(root.join("unused-git-cache")),
        )
        .expect_err("there is no project to inherit from");
        let message = error.message();
        assert!(message.contains("is not inside a workspace"), "{message}");
        assert!(message.contains("[project.dependencies]"), "{message}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn project_true_alongside_any_other_key_is_an_error() {
        // An inherited dependency takes its WHOLE declaration from the project,
        // so a leftover key is a contradiction about where it comes from — one
        // message per key, naming the key.
        for (key, declaration) in [
            ("path", "{ project = true, path = \"../shapes\" }"),
            (
                "git",
                "{ project = true, git = \"https://example.invalid/s\" }",
            ),
            ("tag", "{ project = true, tag = \"v1\" }"),
            ("rev", "{ project = true, rev = \"0123456\" }"),
            ("branch", "{ project = true, branch = \"main\" }"),
            ("version", "{ project = true, version = \"1.2\" }"),
            ("registry", "{ project = true, registry = \"r\" }"),
        ] {
            let (_, errors) = dependency_declaration(declaration);
            assert!(
                errors.iter().any(|error| error
                    .contains(&format!("sets `project = true` alongside `{key}`"))
                    && error.contains("WHOLE declaration")),
                "{declaration}: {errors:?}"
            );
        }
    }

    #[test]
    fn project_false_is_an_error_rather_than_a_no_op() {
        let (_, errors) = dependency_declaration("{ project = false }");
        assert!(
            errors
                .iter()
                .any(|error| error.contains("sets `project = false`")
                    && error.contains("declares its own source")),
            "{errors:?}"
        );
    }

    #[test]
    fn the_projects_own_table_cannot_inherit_from_itself() {
        let manifest = parse(
            "[project]\npackages = []\n[project.dependencies]\nshapes = { project = true }\n",
        );
        assert!(
            manifest
                .validate()
                .iter()
                .any(|error| error.contains("`[project.dependencies] shapes`")
                    && error.contains("this IS the project's table")),
            "{:?}",
            manifest.validate()
        );
    }

    #[test]
    fn an_inheriting_declaration_is_not_a_registry_dependency() {
        // `project = true` must not fall through to the registry stub's "not
        // yet supported" — it is a supported declaration with a source.
        let (manifest, errors) = dependency_declaration("{ project = true }");
        assert!(errors.is_empty(), "{errors:?}");
        assert!(
            matches!(source_of(&manifest), DependencySource::Inherited),
            "expected an inherited source"
        );
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
        assert!(!server.splits(), "single-file emission is the default");
        assert!(
            !client.splits(),
            "a browser entry does not split by default"
        );
    }

    #[test]
    fn a_browser_entry_may_ask_to_split() {
        let manifest = parse(
            "[package]\nname = \"app\"\n\n[entry.server]\n\n\
             [entry.client]\ntarget = \"browser\"\nsplit = true\n",
        );
        assert_eq!(manifest.validate(), Vec::<String>::new());
        assert!(manifest.entries["client"].splits());
        assert!(!manifest.entries["server"].splits());
    }

    #[test]
    fn split_is_refused_outside_a_browser_leg() {
        // A process leg has no navigation to gate, so the key is meaningless
        // there — and an ignored key would read as a split that never happened.
        let node_entry = parse("[package]\nname = \"app\"\n\n[entry.server]\nsplit = true\n");
        assert!(
            node_entry
                .validate()
                .iter()
                .any(|e| e.contains("`[entry.server] split`")
                    && e.contains("`browser` leg only")
                    && e.contains("targets `node`")),
            "{:?}",
            node_entry.validate()
        );
        // Explicitly declared, not merely defaulted.
        let bun_entry =
            parse("[package]\nname = \"app\"\n\n[entry.probe]\ntarget = \"bun\"\nsplit = true\n");
        assert!(
            bun_entry
                .validate()
                .iter()
                .any(|e| e.contains("`[entry.probe] split`") && e.contains("targets `bun`"))
        );
        // The single-entry shape carries the same key and the same refusal.
        let package = parse("[package]\nname = \"app\"\nsplit = true\n");
        assert!(
            package
                .validate()
                .iter()
                .any(|e| e.contains("`[package] split`") && e.contains("targets `node`"))
        );
        let browser_package =
            parse("[package]\nname = \"app\"\ntarget = \"browser\"\nsplit = true\n");
        assert_eq!(browser_package.validate(), Vec::<String>::new());
        assert!(
            browser_package
                .package
                .as_ref()
                .expect("a package")
                .splits()
        );
    }

    #[test]
    fn package_split_cannot_be_combined_with_entry_sections() {
        // `[package] split` is the single-entry shape's key; with entry
        // sections each entry declares its own.
        let manifest = parse("[package]\nname = \"app\"\nsplit = true\n\n[entry.client]\n");
        assert!(
            manifest
                .validate()
                .iter()
                .any(|e| e.contains("can't be combined with") && e.contains("split"))
        );
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
    fn a_dependency_path_may_leave_the_declaring_directory() {
        // E95's decision, pinned so it cannot be "fixed" into the entry rule by
        // an auditor who sees only the asymmetry. A dependency `path` names
        // ANOTHER package, and `../` is how every workspace spells that — this
        // repo's own `examples/fullstack` legs are `common = { path =
        // "../common" }`. The trust model behind the choice is
        // `proposal/build-trust.md` §2: the manifest is the developer's own
        // file, and it may already run arbitrary shell through `[build] run`,
        // so a containment rule on the directory it names would buy nothing
        // and advertise a guarantee the ruling declines to make. The entry
        // pin above is the sibling that DOES refuse `..`, for a different
        // reason: an entry is a file inside the package being built.
        let manifest = parse(
            "[package]\nname = \"app\"\n\n[dependencies]\ncommon = { path = \"../common\" }\n",
        );
        assert!(
            manifest.validate().is_empty(),
            "a `..` dependency path is the normal spelling, not an error: {:?}",
            manifest.validate()
        );
    }

    // --- A15's follow-up: a manifest-designated default `run` entry ---------

    #[test]
    fn a_package_designates_a_default_entry() {
        // The single-package multi-entry shape: `default-entry` names one of the
        // `[entry.<name>]` sections, and the accessor hands it to the CLI.
        let manifest = parse(
            "[package]\nname = \"app\"\ndefault-entry = \"server\"\n\n\
             [entry.client]\ntarget = \"browser\"\n\n[entry.server]\n",
        );
        assert!(manifest.validate().is_empty());
        assert_eq!(manifest.default_entry(), Some("server"));
    }

    #[test]
    fn a_project_designates_a_default_member() {
        // The workspace shape: the same key on `[project]`, naming a member
        // package. One accessor covers both shapes.
        let manifest =
            parse("[project]\npackages = [\"api\", \"jobs\"]\ndefault-entry = \"jobs\"\n");
        assert!(manifest.validate().is_empty());
        assert_eq!(manifest.default_entry(), Some("jobs"));
    }

    #[test]
    fn no_designation_is_no_designation() {
        // Absent in both shapes — the CLI falls back to a lone Node leg or the
        // ambiguity error.
        assert_eq!(
            parse("[package]\nname = \"app\"\n\n[entry.a]\n\n[entry.b]\n").default_entry(),
            None
        );
        assert_eq!(
            parse("[project]\npackages = [\"a\"]\n").default_entry(),
            None
        );
    }

    #[test]
    fn a_package_default_entry_must_name_a_declared_entry() {
        let manifest = parse(
            "[package]\nname = \"app\"\ndefault-entry = \"workr\"\n\n\
             [entry.client]\ntarget = \"browser\"\n\n[entry.worker]\n",
        );
        assert!(manifest.validate().iter().any(
            |e| e.contains("names no `[entry.workr]` section") && e.contains("client, worker")
        ));
    }

    #[test]
    fn a_package_default_entry_needs_entries_to_choose_between() {
        // A single-entry package already has exactly one thing to run, so the
        // key would designate nothing — say so rather than ignore it.
        let manifest = parse("[package]\nname = \"app\"\ndefault-entry = \"app\"\n");
        assert!(
            manifest
                .validate()
                .iter()
                .any(|e| e.contains("has nothing to choose between"))
        );
    }

    #[test]
    fn a_project_default_entry_must_look_like_a_package_name() {
        // A member's name is validated in the member's own manifest, so all this
        // one can catch is a name no package could ever have.
        let manifest = parse("[project]\npackages = [\"api\"]\ndefault-entry = \"not an id\"\n");
        assert!(
            manifest
                .validate()
                .iter()
                .any(|e| e.contains("`[project] default-entry` must name a member package"))
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

    // ── `[[build.hook]]`: the table form of a build hook
    // (`build-hooks.md` §2.3, §9's "Manifest parsing" list) ──

    /// A `[package]` whose `[build]` section is `body`, with its validation
    /// errors — the shape every hook-parsing pin below reads.
    fn build_section(body: &str) -> (Manifest, Vec<String>) {
        let manifest = parse(&format!("[package]\nname = \"app\"\n[build]\n{body}"));
        let errors = manifest.validate();
        (manifest, errors)
    }

    #[test]
    fn the_hook_table_stands_beside_the_string_and_list_forms_of_run() {
        // All three spellings in one manifest, because they compose: `run`
        // keeps meaning "every build" and the table adds the skippable kind.
        let (manifest, errors) = build_section(
            "run = [\"echo one\", \"echo two\"]\n\
             [[build.hook]]\nname = \"lucide\"\nrun = \"sh scripts/lucide.sh\"\n\
             inputs = [\"scripts/lucide.sh\", \"lucide.lock\"]\noutputs = \"src/icons.vl\"\n",
        );
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(manifest.build_hooks(), ["echo one", "echo two"]);
        let hooks = manifest.declared_hooks();
        assert_eq!(hooks.len(), 1);
        assert_eq!(hooks[0].name.as_deref(), Some("lucide"));
        assert_eq!(hooks[0].commands(), ["sh scripts/lucide.sh"]);
        assert_eq!(hooks[0].inputs(), ["scripts/lucide.sh", "lucide.lock"]);
        // A bare string is one entry, exactly as `run` has always read one.
        assert_eq!(hooks[0].outputs(), ["src/icons.vl"]);

        let (manifest, errors) = build_section("run = \"echo one\"\n");
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(manifest.build_hooks(), ["echo one"]);
        assert!(manifest.declared_hooks().is_empty());
    }

    #[test]
    fn a_hook_run_takes_a_bare_string_or_a_list() {
        let (manifest, errors) =
            build_section("[[build.hook]]\nname = \"two\"\nrun = [\"echo one\", \"echo two\"]\n");
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(
            manifest.declared_hooks()[0].commands(),
            ["echo one", "echo two"]
        );
    }

    #[test]
    fn a_hook_with_no_name_is_refused_naming_the_key() {
        let (_, errors) = build_section("[[build.hook]]\nrun = \"echo hi\"\n");
        assert!(
            errors.iter().any(|error| error.contains("missing a `name`")
                && error.contains("[[build.hook]]")
                && error.contains("#1")),
            "{errors:?}"
        );
        // A name that is only whitespace is no name either.
        let (_, errors) = build_section("[[build.hook]]\nname = \"  \"\nrun = \"echo hi\"\n");
        assert!(
            errors
                .iter()
                .any(|error| error.contains("blank `name`") && error.contains("#1")),
            "{errors:?}"
        );
    }

    #[test]
    fn two_hooks_with_one_name_are_refused() {
        // The name keys the freshness stamp, so two hooks under one name would
        // share — and overwrite — one entry.
        let (_, errors) = build_section(
            "[[build.hook]]\nname = \"gen\"\nrun = \"echo one\"\n\
             [[build.hook]]\nname = \"gen\"\nrun = \"echo two\"\n",
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("both named `gen`")),
            "{errors:?}"
        );
    }

    #[test]
    fn a_hook_with_no_command_or_an_empty_one_is_refused() {
        let (_, errors) = build_section("[[build.hook]]\nname = \"gen\"\n");
        assert!(
            errors
                .iter()
                .any(|error| error.contains("`gen` declares no `run`")),
            "{errors:?}"
        );
        let (_, errors) =
            build_section("[[build.hook]]\nname = \"gen\"\nrun = [\"echo\", \"  \"]\n");
        assert!(
            errors
                .iter()
                .any(|error| error.contains("`gen` has an empty command")),
            "{errors:?}"
        );
        // The pre-existing `[build] run` rule is untouched.
        let (_, errors) = build_section("run = [\"\"]\n");
        assert!(
            errors
                .iter()
                .any(|error| error.contains("`[build] run` has an empty command")),
            "{errors:?}"
        );
    }

    #[test]
    fn a_declared_path_carrying_a_glob_is_refused_on_both_keys() {
        // The loud failure exists because the quiet one is so bad: an
        // unmatched pattern would digest as a path that is never there, match
        // "missing" forever, and freeze the hook after its first run.
        for (key, body) in [
            ("inputs", "inputs = [\"src/**/*.css\"]"),
            ("outputs", "outputs = \"out/?.js\""),
            ("inputs", "inputs = \"src/[abc].vl\""),
        ] {
            let (_, errors) = build_section(&format!(
                "[[build.hook]]\nname = \"gen\"\nrun = \"x\"\n{body}\n"
            ));
            assert!(
                errors.iter().any(|error| error.contains(key)
                    && error.contains("no glob matching")
                    && error.contains("`gen`")),
                "{key}: {errors:?}"
            );
        }
        // A plain path — including a directory — is fine.
        let (_, errors) =
            build_section("[[build.hook]]\nname = \"gen\"\nrun = \"x\"\ninputs = \"src/static\"\n");
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn an_empty_declared_path_is_refused() {
        let (_, errors) =
            build_section("[[build.hook]]\nname = \"gen\"\nrun = \"x\"\noutputs = [\"\"]\n");
        assert!(
            errors
                .iter()
                .any(|error| error.contains("empty `outputs` entry")),
            "{errors:?}"
        );
    }

    #[test]
    fn declares_build_hooks_sees_either_form_and_nothing_else() {
        // The question tier 2 asks of a DEPENDENCY's manifest: does it ask to
        // run commands at all?
        assert!(!parse("[package]\nname = \"app\"\n").declares_build_hooks());
        assert!(
            !parse("[package]\nname = \"app\"\n[build]\npreset = \"release\"\n")
                .declares_build_hooks()
        );
        assert!(
            build_section("run = \"echo hi\"\n")
                .0
                .declares_build_hooks()
        );
        assert!(
            build_section("[[build.hook]]\nname = \"g\"\nrun = \"echo hi\"\n")
                .0
                .declares_build_hooks()
        );
        // A `[library]` can write one too, and that is exactly the case the
        // note exists for — a dependency is usually a library.
        assert!(
            parse("[library]\nname = \"dep\"\n[build]\nrun = \"echo hi\"\n").declares_build_hooks()
        );
    }

    // ── tier 2's opt-in: `build-hooks = true` on a dependency
    // (`build-trust.md` §3, `build-hooks.md` §8's S2) ──

    #[test]
    fn a_dependency_records_the_build_hooks_grant() {
        let (manifest, errors) =
            dependency_declaration("{ path = \"../dep\", build-hooks = true }");
        assert!(errors.is_empty(), "{errors:?}");
        let declaration = &manifest.package.as_ref().unwrap().dependencies["shapes"];
        assert!(declaration.grants_build_hooks());
        // The grant does not disturb the source: it is a policy key, not a
        // second way of saying where the package comes from.
        assert!(matches!(source_of(&manifest), DependencySource::Path(_)));
    }

    #[test]
    fn absent_and_false_both_mean_no() {
        // "Absent means no" is the ruled property; `false` is the same answer
        // written down on purpose, and refusing it would punish the reader who
        // documented their decision.
        for declaration in [
            "{ path = \"../dep\" }",
            "{ path = \"../dep\", build-hooks = false }",
            "\"1.2\"",
        ] {
            let (manifest, _) = dependency_declaration(declaration);
            assert!(
                !manifest.package.as_ref().unwrap().dependencies["shapes"].grants_build_hooks(),
                "{declaration} must not grant"
            );
        }
    }

    #[test]
    fn an_inherited_dependency_cannot_grant_build_hooks_of_its_own() {
        // `project = true` takes the WHOLE declaration from the workspace
        // root, and trust is part of the declaration — so the grant belongs in
        // `[project.dependencies]`, where one review sees the source and the
        // permission together.
        let (_, errors) = dependency_declaration("{ project = true, build-hooks = true }");
        assert!(
            errors
                .iter()
                .any(|error| error.contains("`build-hooks`") && error.contains("project = true")),
            "{errors:?}"
        );
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
