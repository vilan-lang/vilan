//! **A built app needs nothing but `dist/`** — the gate `const asset::bundle`
//! exists to pass (kolt.local 029).
//!
//! Before it, a program that depended on a non-code resource read that resource
//! out of its own source tree at runtime: kolt's HTTP server read `src/static/*`
//! and `src/head.html`, and this project's own website hand-copies
//! `playground/editor.js` out of the source tree in CI because `vilan build`
//! would not carry it. Both are the same defect wearing two costumes — a
//! deployed `dist/` is not a deployable app.
//!
//! The headline test builds a two-leg project whose client bundles a resource,
//! **removes the source tree**, and then runs `dist/server.mjs` and fetches the
//! resource over HTTP. Nothing but `dist/` and the manifest is on disk when the
//! server starts; if any part of the pipeline still reached for `src/`, the
//! server would 404 its own icon or refuse to boot.
//!
//! The rest pin the properties that make the feature safe rather than merely
//! present:
//!
//!   - **Reachability is the compiler's.** A resource sitting beside a bundled
//!     one, named by nothing, does not ship. That is the whole difference
//!     between this and a copy step.
//!   - **A resource keeps its package-relative path**, subdirectory and all —
//!     so two files can never claim one output name, and nothing is renamed
//!     behind the author's back.
//!   - **A name a leg's build owns is refused**, because the sweeps in
//!     `write_chunks` would otherwise DELETE a resource parked on one.
//!   - **A source that is already its destination is not copied** — `fs::copy`
//!     over itself truncates, so carrying the file would destroy it.
//!   - **A copy the build stops naming is swept**, because a stale file in
//!     `dist/` SHIPS — on the per-kind prune's law and not a second one: only
//!     what the build recorded, never a file it merely found.
//!   - **The call is compile-time-only**, like its `std::asset` siblings.
//!
//! The last block pins kolt.local 035's three additions on the same machinery:
//! `bundle_as`'s target spelled at the call, `read_dir_all`'s sorted and
//! tracked listing, and `digest`'s fingerprint.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

mod support;

/// The resource the client bundles. An `.svg` deliberately: it is not a `.js`,
/// a `.css` or a `.json`, so nothing about it can be confused with an artifact
/// the build would have emitted anyway, and its content type comes from the
/// generated mime table (kolt.local 022) rather than from a special case.
const ICON: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\"><circle r=\"4\"/></svg>\n";

/// A resource that is on disk, beside the bundled one, and named by no `const`.
const ORPHAN: &str = "no `const` names this file\n";

fn temp_project(tag: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "vilan_asset_bundle_{tag}_{}_{unique}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn write(dir: &Path, relative: &str, contents: &str) {
    let path = dir.join(relative);
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("create the directory");
    std::fs::write(path, contents).expect("write the file");
}

fn vilan(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(args)
        .env("NO_COLOR", "1")
        .output()
        .expect("run vilan")
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind an ephemeral port")
        .local_addr()
        .expect("read the bound address")
        .port()
}

/// A two-entry project: a browser client that bundles `static/icon.svg`, and a
/// server that serves the client's build and nothing else. The server names no
/// resource at all — every route it answers for the icon came from the build.
fn stage(tag: &str, port: u16) -> PathBuf {
    let dir = temp_project(tag);
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"bundled\"\n\n[entry.client]\ntarget = \"browser\"\n\n[entry.server]\n",
    );
    write(
        &dir,
        "src/client.vl",
        "import std::asset::bundle;\n\
         import std::ui::{ mount_root, view };\n\
         \n\
         let icon = const bundle(\"static/icon.svg\");\n\
         \n\
         fun main() {\n\
         \tlet _root = mount_root(\"app\", || view(\"img\").attr(\"src\", icon));\n\
         }\n",
    );
    write(
        &dir,
        "src/server.vl",
        &format!(
            "import std::build::require_build;\n\
             import std::http::{{ Request, Response, Server }};\n\
             import std::io::print;\n\
             \n\
             async fun main() {{\n\
             \tlet build = require_build(\"client\");\n\
             \tServer::builder()\n\
             \t\t.port({port})\n\
             \t\t.serve_build(build)\n\
             \t\t.on_request(|request| Response::builder().body(\"app\").build())\n\
             \t\t.on_start(|server| print(\"listening\"))\n\
             \t\t.build()\n\
             \t\t.start();\n\
             }}\n"
        ),
    );
    write(&dir, "src/static/icon.svg", ICON);
    write(&dir, "src/static/orphan.svg", ORPHAN);
    dir
}

fn build(dir: &Path) -> Output {
    vilan(&["build", dir.to_str().expect("utf-8 temp path")])
}

fn build_ok(dir: &Path) -> Output {
    let output = build(dir);
    assert!(
        output.status.success(),
        "vilan build failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn serve(dir: &Path) -> Child {
    Command::new("node")
        .arg("dist/server.mjs")
        .current_dir(dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn the server")
}

fn wait_for_port(port: u16) -> bool {
    let deadline = Instant::now() + support::run_liveness();
    while Instant::now() < deadline {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

/// A plain HTTP GET, returning `(status line + headers, body)`.
fn http_get(port: u16, path: &str) -> (String, String) {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect for GET");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set a read timeout");
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    )
    .expect("send GET");
    let mut response = Vec::new();
    let _ = stream.read_to_end(&mut response);
    let response = String::from_utf8_lossy(&response).into_owned();
    match response.split_once("\r\n\r\n") {
        Some((head, body)) => (head.to_string(), body.to_string()),
        None => (response, String::new()),
    }
}

fn stop(server: &mut Child) {
    let _ = server.kill();
    let _ = server.wait();
}

/// **The gate.** Build, delete the source tree, run `dist/` — and the resource
/// is still there, served with the content type its extension implies.
#[test]
fn a_built_app_needs_nothing_but_dist() {
    let port = free_port();
    let dir = stage("gate", port);
    build_ok(&dir);

    // Everything the app was written from is now gone. `vilan.toml` stays: it
    // is the project's identity, not its source, and nothing in the running
    // program reads it.
    std::fs::remove_dir_all(dir.join("src")).expect("remove the source tree");
    assert!(!dir.join("src").exists(), "the source tree is gone");

    let mut server = serve(&dir);
    assert!(
        wait_for_port(port),
        "the server should bind {port} with no source tree on disk"
    );

    let (head, body) = http_get(port, "/static/icon.svg");
    assert!(
        head.contains("200"),
        "the bundled resource is served from `dist/` alone:\n{head}"
    );
    assert!(
        head.contains("Content-Type: image/svg+xml"),
        "typed by its extension through the generated mime table:\n{head}"
    );
    assert_eq!(body, ICON, "and byte for byte what the source tree held");

    stop(&mut server);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Reachability stays the compiler's: a resource no `const` names is not
/// copied, so it does not ship. This is the property a build script cannot
/// offer and the reason 029 chose an import-file function over one.
#[test]
fn a_resource_no_const_names_does_not_ship() {
    let dir = stage("reachability", free_port());
    build_ok(&dir);
    assert!(
        dir.join("dist/static/icon.svg").is_file(),
        "the bundled resource rides the build"
    );
    assert!(
        !dir.join("dist/static/orphan.svg").exists(),
        "its unreferenced neighbour does not — `dist/` is what the program \
         named, not what the directory held"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The path IS the name: a subdirectory survives into `dist/` and into the url,
/// and the call folds to that url with no runtime call left behind.
#[test]
fn a_resource_keeps_its_package_relative_path() {
    let dir = stage("naming", free_port());
    build_ok(&dir);
    let javascript = std::fs::read_to_string(dir.join("dist/client.js")).expect("the bundle");
    assert!(
        javascript.contains("\"/static/icon.svg\""),
        "the call folds to the url its bundled copy answers on:\n{javascript}"
    );
    assert!(
        !javascript.contains("__bundle_asset"),
        "and no runtime call survives:\n{javascript}"
    );
    let manifest =
        std::fs::read_to_string(dir.join("dist/client.chunks.json")).expect("the manifest");
    assert!(
        manifest.contains("\"assets\": [\n\t\t\"static/icon.svg\"\n\t]"),
        "the leg's build manifest names what it bundled, which is how \
         `serve_build` serves it with no route of the app's own:\n{manifest}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A leg's build owns `<leg>.js`, `<leg>.css`, `<leg>.chunks.json` and the whole
/// `<leg>.<arm>.js` chunk namespace — in `dist/`, for EVERY leg, because
/// `dist/` is one directory. A resource landing on one of those is refused, and
/// the refusal is not fussiness: `sweep_stale_sidecar` deletes `<leg>.css` when
/// a build emits no styles, so a resource parked there would vanish on the next
/// build rather than merely lose a race with this one.
#[test]
fn a_name_a_legs_build_owns_is_refused() {
    for (bundled, expected) in [
        ("client.js", "`client` leg's compiled bundle"),
        ("client.css", "`client` leg's style sidecar"),
        ("client.chunks.json", "`client` leg's build manifest"),
        ("client.route.js", "`client` leg's route-chunk namespace"),
        // The OTHER leg's bundle: one `dist/`, so a client resource can
        // clobber the server exactly as it can clobber itself.
        ("server.mjs", "`server` leg's compiled bundle"),
    ] {
        let dir = temp_project("collision");
        write(
            &dir,
            "vilan.toml",
            "[package]\nname = \"bundled\"\n\n[entry.client]\ntarget = \"browser\"\n\n[entry.server]\n",
        );
        write(
            &dir,
            "src/client.vl",
            &format!(
                "import std::asset::bundle;\n\
                 import std::io::print;\n\
                 \n\
                 let taken = const bundle(\"{bundled}\");\n\
                 \n\
                 fun main() {{\n\
                 \tprint(taken);\n\
                 }}\n"
            ),
        );
        write(
            &dir,
            "src/server.vl",
            "import std::io::print;\n\nfun main() {\n\tprint(\"server\");\n}\n",
        );
        write(&dir, &format!("src/{bundled}"), "a resource in the way\n");
        let output = build(&dir);
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        assert!(
            !output.status.success(),
            "bundling `{bundled}` must fail the build:\n{stderr}"
        );
        assert!(
            stderr.contains(expected),
            "the refusal must name whose artifact `{bundled}` is; got:\n{stderr}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// A bare file bundling a sibling resolves source and destination to one path.
/// `fs::copy` over itself truncates, so the naive copy would DESTROY the file
/// the build was asked to carry. It is left alone instead, and it is still
/// there — with its bytes — after the build.
#[test]
fn a_resource_that_is_already_in_place_is_not_copied_over_itself() {
    let dir = temp_project("selfcopy");
    write(&dir, "note.txt", ICON);
    write(
        &dir,
        "app.vl",
        "import std::asset::bundle;\n\
         import std::io::print;\n\
         \n\
         let note = const bundle(\"note.txt\");\n\
         \n\
         fun main() {\n\
         \tprint(note);\n\
         }\nmain();\n",
    );
    let entry = dir.join("app.vl");
    let output = vilan(&["build", entry.to_str().expect("utf-8 temp path")]);
    assert!(
        output.status.success(),
        "build failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("note.txt")).expect("the resource"),
        ICON,
        "the resource must survive being carried"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// `run --watch` recopies a resource that changed — which is what makes
/// `asset_body`'s watch-mode re-read see new bytes rather than the copy round 1
/// left in `dist/`.
///
/// The round is driven by the RESOURCE alone: no `.vl` file is touched between
/// the two observations, so the only thing that could have disqualified the
/// per-leg skip is the const channel's build-input record of the bundled file.
/// A workspace, so the source (`src/static/note.txt`) and the copy
/// (`dist/static/note.txt`) are two different paths and the second can be
/// watched without the first being the same file — and a BROWSER leg beside
/// the server, so the round runs under HMR (`activate_hmr` needs one), which
/// is the watch path that carries a leg's resources across a per-leg skip.
#[test]
fn a_watch_round_recopies_a_changed_resource() {
    let dir = temp_project("watch");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"bundled\"\n\n[entry.client]\ntarget = \"browser\"\n\n[entry.server]\n",
    );
    write(&dir, "src/static/note.txt", "round one\n");
    write(
        &dir,
        "src/client.vl",
        "import std::asset::bundle;\n\
         import std::ui::{ mount_root, view };\n\
         \n\
         let note = const bundle(\"static/note.txt\");\n\
         \n\
         fun main() {\n\
         \tlet _root = mount_root(\"app\", || view(\"a\").attr(\"href\", note));\n\
         }\n",
    );
    write(
        &dir,
        "src/server.vl",
        "import std::io::print;\n\nfun main() {\n\tprint(\"server\");\n}\n",
    );
    let mut watcher = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["run", "--watch", dir.to_str().expect("utf-8 temp path")])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn run --watch");

    let copy = dir.join("dist/static/note.txt");
    let round_one = wait_for(&copy, "round one\n", support::WATCH_LIVENESS);
    // Only the resource is edited. Nothing else on disk changes.
    std::fs::write(dir.join("src/static/note.txt"), "round two\n").expect("edit the resource");
    let round_two = wait_for(&copy, "round two\n", support::WATCH_LIVENESS);

    support::kill_watcher(&mut watcher);
    round_one
        .map_err(|last| format!("round 1 never copied the resource; dist/ held: {last:?}"))
        .expect("round 1");
    round_two
        .map_err(|last| {
            format!(
                "editing only the RESOURCE did not drive a round that recopied it; \
                 dist/ still held: {last:?}"
            )
        })
        .expect("round 2");
    let _ = std::fs::remove_dir_all(&dir);
}

// --- kolt.local 035: the estate verbs, end to end ------------------------------
//
// `bundle` alone said "the path is the url", so a path-pinned name forced the
// file to the package root and a static estate was a hand-written list of
// calls. These pin what closing that costs and what it must keep: the target is
// spelled at the call, the listing is deterministic and tracked, and the
// fingerprint is mintable in the language that ships the file.
//
// The green negative for the whole slice is the corpus gate — `asset_bundle.vl`
// and every other golden are byte-identical, so a project using plain `bundle`
// emits exactly what it emitted before.

/// The estate a `read_dir_all` project ships: two files and a nested one, so a
/// listing pin measures the recursion and the sort rather than one name.
fn stage_estate(dir: &Path) {
    write(
        dir,
        "vilan.toml",
        "[package]\nname = \"estate\"\n\n[entry.client]\ntarget = \"browser\"\n\n[entry.server]\n",
    );
    // Written in an order the sort has to undo.
    write(dir, "src/static/robots.txt", "User-agent: *\n");
    write(dir, "src/static/icons/open.svg", ICON);
    write(dir, "src/static/icons/close.svg", ICON);
    write(dir, "src/static/logo.svg", ICON);
    write(
        dir,
        "src/server.vl",
        "import std::io::print;\n\nfun main() {\n\tprint(\"server\");\n}\n",
    );
}

/// The client of that project: 035's recipe verbatim — enumerate, strip the
/// prefix, bundle each file at the url the strip produced.
const ESTATE_CLIENT: &str = "import std::asset;\n\
     import std::ui::{ mount_root, view };\n\
     \n\
     fun estate(): List<str> {\n\
     \tmut urls: List<str> = [];\n\
     \tfor file in asset::read_dir_all(\"static\") {\n\
     \t\turls.push(asset::bundle_as(i\"static/{file}\", i\"/{file}\"));\n\
     \t}\n\
     \turls\n\
     }\n\
     \n\
     let ESTATE = const estate();\n\
     \n\
     fun main() {\n\
     \tlet _root = mount_root(\"app\", || view(\"img\").attr(\"src\", ESTATE[0]));\n\
     }\n";

/// **The gate for `bundle_as`.** A two-leg project whose client bundles its
/// estate at urls the paths do not spell: the copies land on the TARGETS, the
/// manifest names the targets, and the running server — reading only that
/// manifest — answers on them, with the source tree gone.
#[test]
fn a_targeted_resource_is_served_at_the_url_it_was_given() {
    let port = free_port();
    let dir = temp_project("estate-serve");
    stage_estate(&dir);
    write(&dir, "src/client.vl", ESTATE_CLIENT);
    write(
        &dir,
        "src/server.vl",
        &format!(
            "import std::build::require_build;\n\
             import std::http::{{ Request, Response, Server }};\n\
             import std::io::print;\n\
             \n\
             async fun main() {{\n\
             \tlet build = require_build(\"client\");\n\
             \tServer::builder()\n\
             \t\t.port({port})\n\
             \t\t.serve_build(build)\n\
             \t\t.on_request(|request| Response::builder().body(\"app\").build())\n\
             \t\t.on_start(|server| print(\"listening\"))\n\
             \t\t.build()\n\
             \t\t.start();\n\
             }}\n"
        ),
    );
    build_ok(&dir);

    let manifest =
        std::fs::read_to_string(dir.join("dist/client.chunks.json")).expect("the manifest");
    assert!(
        manifest.contains("\"icons/close.svg\"") && manifest.contains("\"robots.txt\""),
        "the manifest names the TARGETS, prefix stripped — that row is what \
         `serve_build` turns into a route:\n{manifest}"
    );
    assert!(
        !manifest.contains("static/"),
        "and no target keeps the source prefix:\n{manifest}"
    );
    assert!(
        dir.join("dist/icons/close.svg").is_file(),
        "the copy lands on the target, subdirectory and all"
    );

    // Nothing but `dist/` from here, exactly as the `bundle` gate proves.
    std::fs::remove_dir_all(dir.join("src")).expect("delete the source tree");
    let mut server = serve(&dir);
    assert!(wait_for_port(port), "the server never came up");
    let (head, body) = http_get(port, "/icons/close.svg");
    // The SOURCE path is not a route: the target replaced it rather than
    // aliasing it, so this falls through to the app's own handler.
    let (_, fallthrough) = http_get(port, "/static/icons/close.svg");
    stop(&mut server);
    assert!(
        head.contains("200"),
        "the targeted url must answer:\n{head}"
    );
    assert_eq!(body, ICON, "and with the resource's own bytes");
    assert_eq!(
        fallthrough, "app",
        "the source path must not be a route of its own — the build serves \
         the target it was given, and nothing else"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A recursive listing is byte-sorted, whatever order the host walks in — the
/// determinism a const result compiled INTO the build has to have. The emitted
/// array is the observable: two builds of one tree must fold to one list.
#[test]
fn a_recursive_listing_folds_in_sorted_order() {
    let dir = temp_project("estate-order");
    stage_estate(&dir);
    write(&dir, "src/client.vl", ESTATE_CLIENT);
    build_ok(&dir);
    let javascript = std::fs::read_to_string(dir.join("dist/client.js")).expect("the bundle");
    let expected = "[ \"/icons/close.svg\", \"/icons/open.svg\", \"/logo.svg\", \"/robots.txt\" ]";
    assert!(
        javascript.contains(expected),
        "the listing must fold byte-sorted, files only (`icons` is not an \
         entry), and with no runtime call left; expected {expected}:\n{javascript}"
    );
    assert!(
        !javascript.contains("__read_asset_dir"),
        "no runtime listing survives:\n{javascript}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The listed DIRECTORY is a tracked build input: a file dropped into it joins
/// the next build with no source edit at all. Without the record the estate
/// recipe would be a one-shot — the leg's inputs would re-hash equal and the
/// new resource would never ship.
#[test]
fn a_file_added_to_a_listed_directory_joins_the_next_build() {
    let dir = temp_project("estate-membership");
    stage_estate(&dir);
    write(&dir, "src/client.vl", ESTATE_CLIENT);
    build_ok(&dir);
    assert!(
        !dir.join("dist/late.svg").exists(),
        "the fixture must start without the late arrival"
    );
    // The ONLY change: a new file in the listed tree. No `.vl` is touched.
    write(&dir, "src/static/late.svg", ICON);
    build_ok(&dir);
    assert!(
        dir.join("dist/late.svg").is_file(),
        "a file appearing in a listed directory must reach the build"
    );
    let javascript = std::fs::read_to_string(dir.join("dist/client.js")).expect("the bundle");
    assert!(
        javascript.contains("\"/late.svg\""),
        "and the folded listing must name it:\n{javascript}"
    );
    // And the other direction: removing it takes it back out of the listing.
    std::fs::remove_file(dir.join("src/static/late.svg")).expect("remove the file");
    build_ok(&dir);
    let javascript = std::fs::read_to_string(dir.join("dist/client.js")).expect("the bundle");
    assert!(
        !javascript.contains("\"/late.svg\""),
        "disappearance is the same event:\n{javascript}"
    );
    // Its `dist/` half (backlog G13). The listing no longer names the file, so
    // nothing routes to the copy — but `dist/` is the DEPLOY artifact, and a
    // static host in front of it would go on serving a resource the source tree
    // no longer has. This is the case that needs no source edit at all to
    // reach: only the tree moved.
    assert!(
        !dir.join("dist/late.svg").exists(),
        "the copy of a file the build stopped naming must go with it — a stale \
         file in dist/ SHIPS"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// `digest` against the canonical vector, through a whole build: the url the
/// program computed carries sha-256("abc")'s first eight hex digits, the copy
/// lands on it, and editing the file re-mints it. A fingerprinted url that did
/// not move with the bytes would serve stale content under a name promising it
/// is immutable — the worst failure the cache tier this exists for can have.
#[test]
fn a_fingerprinted_url_is_the_files_digest_and_moves_with_it() {
    let dir = temp_project("estate-digest");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"fingerprint\"\n\n[entry.app]\n",
    );
    write(&dir, "src/logo.svg", "abc");
    write(
        &dir,
        "src/app.vl",
        "import std::asset;\n\
         import std::io::print;\n\
         \n\
         let LOGO = const asset::bundle_as(\n\
         \t\"logo.svg\",\n\
         \ti\"/logo.{asset::digest(\"logo.svg\").substring(0, 8)}.svg\",\n\
         );\n\
         \n\
         fun main() {\n\
         \tprint(LOGO);\n\
         }\n",
    );
    build_ok(&dir);
    let javascript = std::fs::read_to_string(dir.join("dist/app.mjs")).expect("the bundle");
    assert!(
        javascript.contains("\"/logo.ba7816bf.svg\""),
        "sha-256(\"abc\") begins `ba7816bf` — the url is the file's own \
         digest:\n{javascript}"
    );
    assert!(
        dir.join("dist/logo.ba7816bf.svg").is_file(),
        "and the copy lands on the minted url"
    );
    write(&dir, "src/logo.svg", "abcd");
    build_ok(&dir);
    let javascript = std::fs::read_to_string(dir.join("dist/app.mjs")).expect("the bundle");
    assert!(
        !javascript.contains("ba7816bf"),
        "an edited file must re-mint its url — the digested file is a tracked \
         build input:\n{javascript}"
    );
    // And the copy the old url named goes with the url (backlog G13). This is
    // the unbounded case: the recipe mints a NEW name on every save, so without
    // the sweep a `--watch` session accumulates one orphaned copy per edit for
    // its whole life, each of them served.
    assert!(
        !dir.join("dist/logo.ba7816bf.svg").exists(),
        "the copy on the old fingerprint is orphaned the moment the url moves, \
         and must not survive the build that moved it"
    );
    assert!(
        dir.join("dist/logo.88d4266f.svg").is_file(),
        "sha-256(\"abcd\") begins `88d4266f` — the new url has its copy"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The url's shape, refused at the `const` expression with the fix named. Each
/// row is a distinct rule: one message for all of them would make the rest of
/// these vacuous.
#[test]
fn a_target_that_is_not_a_url_is_refused() {
    for (url, expected) in [
        ("robots.txt", "urls start at the site root"),
        ("/a\\\\b.txt", "urls are `/`-separated on every host"),
        ("/a//b.txt", "has an empty segment"),
        ("/a/./b.txt", "has a `.` segment"),
        ("/a/../b.txt", "has a `..` segment"),
        ("/", "has an empty segment"),
    ] {
        let dir = temp_project("estate-urlshape");
        write(&dir, "vilan.toml", "[package]\nname = \"shapes\"\n");
        write(&dir, "src/note.txt", "a resource\n");
        write(
            &dir,
            "src/main.vl",
            &format!(
                "import std::asset;\n\
                 import std::io::print;\n\
                 \n\
                 let TAKEN = const asset::bundle_as(\"note.txt\", \"{url}\");\n\
                 \n\
                 fun main() {{\n\
                 \tprint(TAKEN);\n\
                 }}\n"
            ),
        );
        let output = build(&dir);
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        assert!(
            !output.status.success(),
            "`{url}` must fail the build:\n{stderr}"
        );
        assert!(
            stderr.contains(expected),
            "the refusal for `{url}` must say {expected:?}; got:\n{stderr}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// The refusal the identity rule used to give for free. Two sources, one
/// target: refused at const evaluation, naming BOTH, because a collision is a
/// statement about a pair.
#[test]
fn two_files_bundling_to_one_url_are_refused_naming_both() {
    let dir = temp_project("estate-collision");
    write(&dir, "vilan.toml", "[package]\nname = \"collide\"\n");
    write(&dir, "src/first.txt", "one\n");
    write(&dir, "src/second.txt", "two\n");
    write(
        &dir,
        "src/main.vl",
        "import std::asset;\n\
         import std::io::print;\n\
         \n\
         let ONE = const asset::bundle_as(\"first.txt\", \"/pinned.txt\");\n\
         let TWO = const asset::bundle_as(\"second.txt\", \"/pinned.txt\");\n\
         \n\
         fun main() {\n\
         \tprint(ONE);\n\
         \tprint(TWO);\n\
         }\n",
    );
    let output = build(&dir);
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        !output.status.success(),
        "the collision must fail the build:\n{stderr}"
    );
    assert!(
        stderr.contains("`first.txt` and `second.txt` both bundle to `/pinned.txt`"),
        "the refusal must name both sources; got:\n{stderr}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The same collision ACROSS legs, which the const pass cannot see: a workspace's
/// legs are separate compiles into one `dist/`, so the copy is where two legs
/// claiming one name meet. Without this the second copy would silently win, and
/// `dist/` would serve one leg's file under the other leg's url.
#[test]
fn two_legs_bundling_to_one_url_are_refused_at_the_copy() {
    let dir = temp_project("estate-crossleg");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"collide\"\n\n[entry.client]\ntarget = \"browser\"\n\n[entry.server]\n",
    );
    write(&dir, "src/first.txt", "one\n");
    write(&dir, "src/second.txt", "two\n");
    write(
        &dir,
        "src/client.vl",
        "import std::asset;\n\
         import std::ui::{ mount_root, view };\n\
         \n\
         let ONE = const asset::bundle_as(\"first.txt\", \"/pinned.txt\");\n\
         \n\
         fun main() {\n\
         \tlet _root = mount_root(\"app\", || view(\"a\").attr(\"href\", ONE));\n\
         }\n",
    );
    write(
        &dir,
        "src/server.vl",
        "import std::asset;\n\
         import std::io::print;\n\
         \n\
         let TWO = const asset::bundle_as(\"second.txt\", \"/pinned.txt\");\n\
         \n\
         fun main() {\n\
         \tprint(TWO);\n\
         }\n",
    );
    let output = build(&dir);
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        !output.status.success(),
        "one `dist/` cannot serve two files on one url:\n{stderr}"
    );
    assert!(
        stderr.contains("both bundle to `pinned.txt`"),
        "the refusal must name the collision; got:\n{stderr}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The build-owned-name fence applies to the TARGET, and by reaching the same
/// check `bundle` reaches — one list, never two. A resource parked on
/// `client.css` does not merely collide: `sweep_stale_sidecar` deletes that name
/// when a leg emits no styles, so it would vanish on the next build.
#[test]
fn a_target_a_legs_build_owns_is_refused() {
    for (url, expected) in [
        ("/client.js", "`client` leg's compiled bundle"),
        ("/client.css", "`client` leg's style sidecar"),
        ("/server.mjs", "`server` leg's compiled bundle"),
    ] {
        let dir = temp_project("estate-owned");
        write(
            &dir,
            "vilan.toml",
            "[package]\nname = \"owned\"\n\n[entry.client]\ntarget = \"browser\"\n\n[entry.server]\n",
        );
        write(&dir, "src/note.txt", "a resource in the way\n");
        write(
            &dir,
            "src/client.vl",
            &format!(
                "import std::asset;\n\
                 import std::ui::{{ mount_root, view }};\n\
                 \n\
                 let TAKEN = const asset::bundle_as(\"note.txt\", \"{url}\");\n\
                 \n\
                 fun main() {{\n\
                 \tlet _root = mount_root(\"app\", || view(\"a\").attr(\"href\", TAKEN));\n\
                 }}\n"
            ),
        );
        write(
            &dir,
            "src/server.vl",
            "import std::io::print;\n\nfun main() {\n\tprint(\"server\");\n}\n",
        );
        let output = build(&dir);
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        assert!(
            !output.status.success(),
            "targeting `{url}` must fail the build:\n{stderr}"
        );
        assert!(
            stderr.contains(expected),
            "the refusal must name whose artifact `{url}` is; got:\n{stderr}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

// --- backlog G13: `dist/` sweeps what the build stopped naming ----------------
//
// `write_bundled` copied and recorded nothing, alone among the four writers
// that share one `dist/`. The two above pin the disappearances; these pin the
// LAW that makes the sweep safe, which is G6's and not a second one: the
// pruner acts only on its own record, so a user's file, another leg's copies
// and anything unrecorded are untouchable however bundle-shaped their names.

/// A leg that stops bundling loses its copies — and the build touches nothing
/// else on the way. The record (`.vilan-bundled`, beside the outputs) is the
/// whole authority: it names what to remove, it is removed with its last row,
/// and a hand-placed file it never named survives the sweep that empties it.
#[test]
fn a_bundle_the_build_stops_naming_is_swept_from_dist() {
    let dir = temp_project("estate-sweep");
    stage_estate(&dir);
    write(&dir, "src/client.vl", ESTATE_CLIENT);
    build_ok(&dir);
    assert_eq!(
        std::fs::read_to_string(dir.join("dist/.vilan-bundled"))
            .ok()
            .as_deref(),
        Some(
            "client/icons/close.svg\n\
             client/icons/open.svg\n\
             client/logo.svg\n\
             client/robots.txt\n"
        ),
        "the build records what it carried, keyed by leg and sorted — the only \
         thing the next build's prune may act on"
    );

    // Placed by hand, in the same directory, named by no `const`. The sweep
    // about to run must not so much as look at it.
    write(&dir, "dist/hand-written.txt", ORPHAN);
    // The estate recipe, gone. No file moved in `static/`: the CALL that named
    // them is what left.
    write(
        &dir,
        "src/client.vl",
        "import std::ui::{ mount_root, view };\n\
         \n\
         fun main() {\n\
         \tlet _root = mount_root(\"app\", || view(\"p\"));\n\
         }\n",
    );
    build_ok(&dir);

    for gone in [
        "dist/robots.txt",
        "dist/logo.svg",
        "dist/icons/open.svg",
        "dist/icons/close.svg",
    ] {
        assert!(
            !dir.join(gone).exists(),
            "{gone} outlived the build that stopped naming it"
        );
    }
    assert_eq!(
        std::fs::read_to_string(dir.join("dist/hand-written.txt"))
            .ok()
            .as_deref(),
        Some(ORPHAN),
        "an unrecorded file is not the build's to remove — one law with the \
         per-kind prune's"
    );
    assert!(
        !dir.join("dist/.vilan-bundled").exists(),
        "an empty record is removed, not left behind as its own stale artifact"
    );
    assert!(
        dir.join("dist/client.js").is_file() && dir.join("dist/server.mjs").is_file(),
        "and the sweep touches nothing but the copies it recorded"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// `dist/` is ONE directory and every leg copies into it, so the record is
/// keyed by leg — and a name is still not one leg's to delete just because that
/// leg stopped naming it. Two legs bundling the same file to the same target is
/// legal and expected (the copy is idempotent), so the sweep removes a file
/// only when the record it is about to write no longer names it AT ALL.
#[test]
fn a_sweep_leaves_the_other_legs_bundles_alone() {
    let dir = temp_project("estate-crossleg-sweep");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"twolegs\"\n\n[entry.client]\ntarget = \"browser\"\n\n[entry.server]\n",
    );
    write(&dir, "src/first.txt", "the client's own\n");
    write(&dir, "src/second.txt", "the server's own\n");
    write(&dir, "src/both.txt", "carried by both legs\n");
    write(
        &dir,
        "src/client.vl",
        "import std::asset;\n\
         import std::ui::{ mount_root, view };\n\
         \n\
         let MINE = const asset::bundle_as(\"first.txt\", \"/client-only.txt\");\n\
         let OURS = const asset::bundle_as(\"both.txt\", \"/shared.txt\");\n\
         \n\
         fun main() {\n\
         \tlet _root = mount_root(\"app\", || view(\"a\").attr(\"href\", MINE + OURS));\n\
         }\n",
    );
    write(
        &dir,
        "src/server.vl",
        "import std::asset;\n\
         import std::io::print;\n\
         \n\
         let MINE = const asset::bundle_as(\"second.txt\", \"/server-only.txt\");\n\
         let OURS = const asset::bundle_as(\"both.txt\", \"/shared.txt\");\n\
         \n\
         fun main() {\n\
         \tprint(MINE);\n\
         \tprint(OURS);\n\
         }\n",
    );
    build_ok(&dir);
    for present in [
        "dist/client-only.txt",
        "dist/server-only.txt",
        "dist/shared.txt",
    ] {
        assert!(dir.join(present).is_file(), "{present} must be carried");
    }

    // Only the SERVER stops bundling — the leg that builds SECOND, so its prune
    // is the last word on `dist/`. Dropping the client's instead would prove
    // nothing: the server's copy runs after it and would put the shared file
    // back, hiding a prune that had no business removing it.
    write(
        &dir,
        "src/server.vl",
        "import std::io::print;\n\nfun main() {\n\tprint(\"server\");\n}\n",
    );
    build_ok(&dir);
    assert!(
        !dir.join("dist/server-only.txt").exists(),
        "the server's own copy goes with the call that named it"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("dist/client-only.txt"))
            .ok()
            .as_deref(),
        Some("the client's own\n"),
        "one leg's prune may never reach another leg's copies"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("dist/shared.txt"))
            .ok()
            .as_deref(),
        Some("carried by both legs\n"),
        "a name the OTHER leg still bundles is not stale — the record this \
         build is about to write still carries it, so it is not the pruner's"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("dist/.vilan-bundled"))
            .ok()
            .as_deref(),
        Some("client/client-only.txt\nclient/shared.txt\n"),
        "and the record keeps exactly the rows that survived"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Polls for `path` to hold `expected`, up to a bounded deadline. Returns the
/// last content seen (for a helpful assert message) if it never matches.
fn wait_for(path: &Path, expected: &str, deadline: Duration) -> Result<(), String> {
    let start = Instant::now();
    let mut last = String::from("<never written>");
    while start.elapsed() < deadline {
        if let Ok(contents) = std::fs::read_to_string(path) {
            if contents == expected {
                return Ok(());
            }
            last = contents;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(last)
}
