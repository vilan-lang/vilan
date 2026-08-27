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
//!   - **The call is compile-time-only**, like its two `std::asset` siblings.

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
