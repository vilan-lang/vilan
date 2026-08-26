# Spec §11 — The platform model & manifests

A **platform** is a host a build targets: `node` (the default), `deno`,
`bun`, or `browser`. The first three form the **`@process`** family.
One package may build for several platforms at once (§11.4's entries);
the compiler proves, per entry, that no reachable code requires a
capability its platform lacks.

## 11.1 Layers

The standard library is layered:

- the **base** layer: platform-neutral, available everywhere;
- the **browser** layer (`std::dom`, `std::ui`, `std::router`,
  `std::storage`): browser builds only;
- the **process** layer (`std::fs`, `std::http`, `std::db`,
  `std::process`, `std::rpc_server`): `@process` builds only.

A library may declare the same shape for itself (`[library.layer]`,
§11.4): a neutral root plus per-platform overlay roots.

## 11.2 Coloring and the reachability check

Every function is **colored** with the platform requirement it
implies: seeded by the layer its externs and std calls live in, flowing
callee-to-caller through the call graph (the same inference shape as
asyncness, §7.3), including through generic instantiations: a generic
function's requirement is judged **per instantiation**, so `save<T>`
colors process-only only for the `T`s whose code actually reaches a
process capability.

The check is on **reachable code, not imports**: importing a module is
free; each entry is checked along the call paths that start at its
`main` (and its reachable initializers). A path that crosses onto a
platform the entry does not build for is a compile error naming the
chain from the entry to the crossing. Module-level initializers obey
the same rule: a binding's initializer is analyzed, colored, and
bundled only if something reachable references the binding. `const`
initializers evaluate at build time (§9) and never color anything.

## 11.3 Fences

`[platform("browser")]` (one platform, a family like `"@process"`, or
several) on a function declares the platforms it promises to run on.
The promise is checked on **every** compile, whatever the build's
entries: if code the fenced function reaches requires a layer one of
the fenced platforms lacks, the error lands **at the fence** with the
offending chain, not at some distant entry in a dependent build.
Fences add no runtime behavior; they are checked declarations.

## 11.4 Manifests (`vilan.toml`)

The manifest declares what a directory builds. Sections:

- **`[package]`**: an application or plain package: `name`,
  `description`, `root` (source root; default `src/`), `entry` (the
  entry file, when there is exactly one), `target` (a platform;
  default `node`), and `dependencies` (name → `{ path = "…" }` for a
  local directory, or `{ git = "…", tag | rev = "…" }` for a `[library]`
  repository pinned to exactly one of a tag or a commit; registry
  dependencies are future work).
- **`[entry.<name>]`**: one build entry per table: `path` (default
  `<root>/<name>.vl`), `target` (default `node`), and `split` (default
  `false`; §11.5). A package with
  entries builds each for its own platform; reachability (§11.2) is
  what lets one source tree serve several. `[package] default-entry`
  names the entry `vilan run` executes when several are runnable.
- **`[library]`**: a dependency-only package: `name`, `description`,
  `root`, `dependencies`, and **`[library.layer.<name>]`** overlays
  (`root`, `platform = ["…"]`) for per-platform sources.
- **`[project]`**: a workspace: `packages = ["member", …]` (paths);
  building the project builds every member against its own manifest.
  `default-entry` names the member `vilan run` executes when several
  are runnable.
  Its own `dependencies` are declared once for the members to share: a
  member writes `dep = { project = true }` to take that declaration
  (paths in it resolve against the **project root**). Inheritance is
  per dependency and opt-in (nothing is inherited implicitly), and
  `project = true` combines with no other key.
- **`[build]`**: `run`, plus codegen options: `preset` (`"debug"` |
  `"release"`) and the per-feature overrides `indent`, `spaces`,
  `debug-names`. Build options never change program semantics (§7.6),
  only the emitted text. `run` is a command line (or a list of them)
  executed through the host shell **before** each build (each `--watch`
  round included), in the manifest's directory, in order; a non-zero
  exit fails the build. `vilan check` builds nothing and runs none.
- **`[macro]`**: the compile-time interpreter budget: `fuel` (steps
  per macro/const run) and `depth` (nested expansion), §9.3/§10.4.

`std`, `pkg`, `macro_std`, and `vilan` are **reserved package names**.
The first three name the import roots themselves (§4.2), so a
`[package] name` or a key in any `dependencies` table claiming one is a
manifest error — a dependency under such a key could only shadow the
root or be unreachable behind it. `vilan` is the language's own name,
held for its official namespace; the same claim is refused in the same
places. `[library] name` is exempt: the standard library itself is the
`[library]` named `std` (likewise `macro_std`), and a library's own
name, unlike a dependency key, never binds an import root.

## 11.5 Build products

Each entry emits `dist/<name>.<ext>` for its platform (browser entries
first, so a server that ships bundles finds them fresh), plus
`dist/<name>.css` when const evaluation emitted style assets (§9.2).
The extension states the module kind to the host: `.mjs` on the process
platforms, whose runtimes classify a file as ESM or CommonJS before
running it, and `.js` on the browser, where the loading
`<script type="module">` tag classifies it instead.

A `browser` entry declaring **`split = true`** emits **route chunks**
instead of one file: `dist/<name>.js` carries every module-level binding
(so §7.6's initialization order is untouched) and every function two or
more route arms can reach, and `dist/<name>.<arm>.js` carries the
functions exactly one arm of the entry's route `match` can reach. Every
build of a `browser` entry writes `dist/<name>.chunks.json`, the leg's
**build manifest**: its bundle's file name, the style sidecar's file name
or `null`, whether the bundle must be loaded as a classic script, and the
chunks it emitted (empty unless it split). Chunks are fetched when a
navigation first reaches their arm, and the route value does not advance
until one arrives; which functions land where is implementation-defined
beyond that rule. Overlapping navigations resolve by order of DEPARTURE,
not of arrival: a chunk that lands after a later navigation began does not
advance the route value. A fetch that fails leaves the route value where it
was and is not remembered, so a later navigation to that arm fetches again. `split` on a non-`browser` entry is a manifest error.
`vilan run` builds all entries and starts one `@process` entry: the
only one, the designated `default-entry`, or the one `--entry` names;
`vilan check` checks every entry, always. The emitted text beyond
§7.6's guarantees is implementation-defined.
