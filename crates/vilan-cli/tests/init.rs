//! The `vilan init` scaffold (backlog E18), gated like corpus programs: every
//! template is materialized by the built binary into a temp directory and then
//! **compiled and run** — so a language, std, or manifest change that would rot
//! a scaffold fails here instead of in a new user's first minute.
//!
//! One test per template shape (node runs and passes its own `*_test.vl`; the
//! browser one builds a DOM-global bundle; the full-stack one builds both
//! entries, serves them, and is compared against the blessed example layout),
//! plus the selection and destination rules — a non-terminal stdin, an occupied
//! directory, an existing project, and a directory name that is not a manifest
//! identifier.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use vilan_core::manifest::Manifest;

/// A fresh temp directory for one test to scaffold into.
fn temp_dir(tag: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir =
        std::env::temp_dir().join(format!("vilan_init_{tag}_{}_{unique}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create the temp directory");
    dir
}

/// Runs the `vilan` binary with `args` from `working_directory`. Its stdin is
/// closed, which is also the non-terminal case the prompt must refuse to hang
/// on — every `init` in this file exercises that path.
fn vilan(working_directory: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(args)
        .current_dir(working_directory)
        .stdin(Stdio::null())
        .output()
        .expect("run vilan")
}

/// A command's combined stdout + stderr — diagnostics go to stderr, results to
/// stdout, and a test asserting on "what the CLI said" wants both.
fn combined(output: &Output) -> String {
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    text
}

fn assert_ok(output: &Output, what: &str) {
    assert!(
        output.status.success(),
        "{what} failed:\n{}",
        combined(output)
    );
}

/// Scaffolds `template` into a fresh temp directory as `app/`, returning the
/// project root.
fn scaffold(tag: &str, template: &str) -> PathBuf {
    let parent = temp_dir(tag);
    let output = vilan(&parent, &["init", "app", "--template", template]);
    assert_ok(&output, &format!("vilan init --template {template}"));
    parent.join("app")
}

/// The in-repo template sources (this crate is `crates/vilan-cli`).
fn template_source_dir(template: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("templates")
        .join(template)
}

/// Every file under `root`, as `/`-joined paths relative to it, sorted.
fn relative_files(root: &Path) -> Vec<String> {
    fn walk(directory: &Path, prefix: &str, into: &mut Vec<String>) {
        for entry in std::fs::read_dir(directory)
            .expect("read a directory")
            .flatten()
        {
            let name = entry.file_name().to_string_lossy().into_owned();
            let relative = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            if entry.path().is_dir() {
                walk(&entry.path(), &relative, into);
            } else {
                into.push(relative);
            }
        }
    }
    let mut files = Vec::new();
    walk(root, "", &mut files);
    files.sort();
    files
}

// --- the templates, compiled and run -------------------------------------

#[test]
fn the_node_template_builds_runs_and_passes_its_own_test() {
    let project = scaffold("node", "node");

    let run = vilan(&project, &["run", "."]);
    assert_ok(&run, "vilan run .");
    assert!(
        String::from_utf8_lossy(&run.stdout).contains("hello, world"),
        "the node scaffold should print its greeting:\n{}",
        combined(&run)
    );

    // The scaffolded `*_test.vl` is discovered and passes — `vilan test` is
    // taught by this template, so it has to work in it.
    let tested = vilan(&project, &["test", "."]);
    assert_ok(&tested, "vilan test .");
    let report = String::from_utf8_lossy(&tested.stdout);
    assert!(
        report.contains("1 passed, 0 failed"),
        "the node scaffold's test should pass:\n{report}"
    );

    assert_ok(&vilan(&project, &["build", "."]), "vilan build .");
    assert!(project.join("main.js").is_file(), "missing main.js");
}

#[test]
fn the_browser_template_builds_a_browser_bundle() {
    let project = scaffold("browser", "browser");
    assert_ok(&vilan(&project, &["build", "."]), "vilan build .");

    // `target = "browser"` in the manifest, so a plain build emits the bundle
    // beside `index.html` — the file the page actually loads.
    let bundle = project.join("app.js");
    assert!(bundle.is_file(), "missing app.js");
    let page = std::fs::read_to_string(project.join("index.html")).expect("index.html");
    assert!(
        page.contains("src=\"app.js\""),
        "index.html should load the emitted bundle:\n{page}"
    );
    assert!(
        page.contains("id=\"app\""),
        "index.html should carry the mount point:\n{page}"
    );

    // The style sidecar must be BOTH emitted and linked. Scaffolding a page
    // that never loads the CSS the build writes is the failure mode this
    // asserts against (A8's `<link>` scaffold): the compiled styles would be
    // produced on every build and silently thrown away.
    let stylesheet = project.join("app.css");
    assert!(
        stylesheet.is_file(),
        "missing app.css — the scaffold's const styles emitted nothing"
    );
    assert!(
        page.contains("href=\"app.css\""),
        "index.html should link the emitted stylesheet:\n{page}"
    );
    let css = std::fs::read_to_string(&stylesheet).expect("app.css");
    assert!(
        css.contains("{display:flex}") && css.contains(":root{--space-4:1rem}"),
        "app.css should carry the scaffold's compiled rules and theme vars:\n{css}"
    );

    // A browser bundle reaches DOM globals and no node host import.
    let javascript = std::fs::read_to_string(&bundle).expect("app.js");
    assert!(
        javascript.contains("document."),
        "the bundle should use DOM globals"
    );
    assert!(
        !javascript.contains("require(\"node:") && !javascript.contains("from \"node:"),
        "a browser bundle must not import a node host module"
    );
}

#[test]
fn the_fullstack_template_builds_both_entries_and_serves_them() {
    let project = scaffold("fullstack", "fullstack");

    // The port is compiled into the server, so each attempt rebuilds with a
    // fresh one: an ephemeral port probed and released can lose the race to a
    // concurrent suite process (backlog E19), and a retry costs a rebuild
    // rather than a flake. A genuinely broken scaffold fails every attempt.
    let mut last_failure = String::new();
    for attempt in 1..=3 {
        let port = free_port();
        let server_source = project.join("src/server.vl");
        let original = std::fs::read_to_string(&server_source).expect("src/server.vl");
        std::fs::write(&server_source, original.replace("8080", &port.to_string()))
            .expect("patch the port");

        assert_ok(&vilan(&project, &["build", "."]), "vilan build .");
        assert!(
            project.join("dist/client.js").is_file(),
            "missing dist/client.js"
        );
        assert!(
            project.join("dist/server.js").is_file(),
            "missing dist/server.js"
        );
        assert!(
            project.join("dist/client.css").is_file(),
            "missing dist/client.css — the scaffold's const styles emitted nothing"
        );

        // The server runs from the project root: it reads `dist/client.js` and
        // `src/app.html` by relative path, exactly as `vilan run .` runs it.
        let log = project.join("server.log");
        let mut server = Command::new("node")
            .arg("dist/server.js")
            .current_dir(&project)
            .stdout(Stdio::null())
            .stderr(Stdio::from(std::fs::File::create(&log).expect("log file")))
            .spawn()
            .expect("spawn node server");

        if !wait_for_port(port, Duration::from_secs(20)) {
            let _ = server.kill();
            let _ = server.wait();
            last_failure = format!(
                "attempt {attempt}: the server never accepted a connection on {port}\n{}",
                std::fs::read_to_string(&log).unwrap_or_default()
            );
            std::fs::write(&server_source, original).expect("restore the port");
            continue;
        }

        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let page = String::from_utf8_lossy(&http_get(port, "/")).into_owned();
            assert!(
                page.contains("id=\"app\"") && page.contains("/client.js"),
                "the served page should be the shell with the client mount point:\n{page}"
            );
            // The full `<link>` loop: the shell links the stylesheet AND the
            // scaffold's server actually has a route that serves it. Either
            // half alone leaves the compiled styles unreachable.
            assert!(
                page.contains("href=\"/client.css\""),
                "the served page should link the client stylesheet:\n{page}"
            );
            let stylesheet = String::from_utf8_lossy(&http_get(port, "/client.css")).into_owned();
            assert!(
                stylesheet.contains("{display:flex}"),
                "the /client.css route should serve the compiled styles:\n{}",
                &stylesheet[..stylesheet.len().min(400)]
            );
            let bundle = String::from_utf8_lossy(&http_get(port, "/client.js")).into_owned();
            assert!(
                bundle.contains("document."),
                "the served bundle should be the browser client:\n{}",
                &bundle[..bundle.len().min(400)]
            );
            // The shared module is what both legs reach; the client carries it.
            assert!(
                bundle.contains("hello from Vilan"),
                "the client bundle should carry the shared greeting"
            );
        }));

        let _ = server.kill();
        let _ = server.wait();
        if let Err(panic) = outcome {
            std::panic::resume_unwind(panic);
        }
        return;
    }
    panic!("{last_failure}");
}

#[test]
fn the_fullstack_template_matches_the_blessed_example_layout() {
    // The scaffold is the delivery vehicle for the default full-stack shape
    // (D7), so it is compared against the examples that TEACH that shape —
    // not against a copy of the layout written down here. A drift in either
    // direction fails.
    let project = scaffold("blessed", "fullstack");
    let scaffolded = read_manifest(&project.join("vilan.toml"));
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vilan/examples");

    for example in ["walkthrough", "todo", "ssr"] {
        let directory = examples.join(example);
        let blessed = read_manifest(&directory.join("vilan.toml"));

        assert!(
            blessed.project.is_none() && blessed.library.is_none(),
            "{example}: the blessed shape is ONE package"
        );
        let (scaffolded_package, blessed_package) = (
            scaffolded.package.as_ref().expect("the scaffold's package"),
            blessed.package.as_ref().expect("the example's package"),
        );
        // Same source root and no `[package] entry` — the entries carry it.
        assert_eq!(
            scaffolded_package.root, blessed_package.root,
            "{example}: source root"
        );
        assert_eq!(
            scaffolded_package.entry, blessed_package.entry,
            "{example}: `[package] entry`"
        );
        assert_eq!(
            scaffolded_package.target, blessed_package.target,
            "{example}: `[package] target`"
        );

        let entry_names =
            |manifest: &Manifest| -> Vec<String> { manifest.entries.keys().cloned().collect() };
        assert_eq!(
            entry_names(&scaffolded),
            entry_names(&blessed),
            "{example}: entry names"
        );
        for (name, entry) in &scaffolded.entries {
            let blessed_entry = &blessed.entries[name];
            assert_eq!(
                entry.target, blessed_entry.target,
                "{example}: `[entry.{name}] target`"
            );
            assert_eq!(
                entry.path, blessed_entry.path,
                "{example}: `[entry.{name}] path`"
            );
            assert_eq!(
                entry.split, blessed_entry.split,
                "{example}: `[entry.{name}] split`"
            );
        }

        // ...and the layout the manifest implies is on disk in both.
        for relative in ["src/client.vl", "src/server.vl", "src/app.html"] {
            assert!(
                project.join(relative).is_file(),
                "the scaffold is missing {relative}"
            );
            assert!(
                directory.join(relative).is_file(),
                "{example} is missing {relative}"
            );
        }
    }
}

fn read_manifest(path: &Path) -> Manifest {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    Manifest::parse(&text)
        .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()))
        .0
}

#[test]
fn every_template_scaffolds_exactly_its_embedded_files_already_formatted() {
    for template in ["node", "browser", "fullstack"] {
        let project = scaffold("files", template);
        // The embedded table's destination keys ARE the template directory's
        // paths: a file added on disk but not embedded (or embedded under a
        // different key) shows up here rather than as a missing scaffold file.
        assert_eq!(
            relative_files(&project),
            relative_files(&template_source_dir(template)),
            "the `{template}` template's files"
        );

        // A scaffold must be canonical the moment it lands, or a user's first
        // `vilan fmt` rewrites files they never edited.
        let formatted = vilan(&project, &["fmt", "--check", "."]);
        assert!(
            formatted.status.success(),
            "the `{template}` scaffold is not canonically formatted:\n{}",
            combined(&formatted)
        );

        // Nothing may carry the substitution token out of the binary.
        for relative in relative_files(&project) {
            let contents = std::fs::read_to_string(project.join(&relative)).expect("a template");
            assert!(
                !contents.contains("{{name}}"),
                "{template}/{relative} still carries the name token"
            );
        }
    }
}

// --- selection and destination rules --------------------------------------

#[test]
fn no_template_and_no_terminal_lists_the_templates_instead_of_hanging() {
    // A script must never hang on a prompt: stdin is closed here, so the
    // command has to fail with the flag and the whole template set named.
    let parent = temp_dir("noninteractive");
    let output = vilan(&parent, &["init", "app"]);
    assert!(!output.status.success(), "init should refuse to guess");
    let message = combined(&output);
    assert!(message.contains("--template"), "{message}");
    for template in ["node", "browser", "fullstack"] {
        assert!(message.contains(template), "{message}");
    }
    assert!(
        !parent.join("app").exists(),
        "nothing should be written when no template was chosen"
    );
}

#[test]
fn an_unknown_template_names_the_ones_that_exist() {
    let parent = temp_dir("unknown");
    let output = vilan(&parent, &["init", "app", "--template", "web"]);
    assert!(!output.status.success(), "`web` is not a template");
    let message = combined(&output);
    assert!(message.contains("unknown template `web`"), "{message}");
    assert!(message.contains("fullstack"), "{message}");
}

#[test]
fn a_named_directory_that_exists_and_is_not_empty_is_refused() {
    let parent = temp_dir("occupied");
    std::fs::create_dir_all(parent.join("app")).expect("create the directory");
    std::fs::write(parent.join("app/notes.txt"), "mine\n").expect("occupy it");

    let output = vilan(&parent, &["init", "app", "--template", "node"]);
    assert!(!output.status.success(), "an occupied directory is refused");
    assert!(
        combined(&output).contains("already exists and is not empty"),
        "{}",
        combined(&output)
    );
    assert!(
        !parent.join("app/vilan.toml").exists(),
        "nothing may be written into an occupied directory"
    );

    // An EMPTY directory that already exists is fine — the refusal is about
    // clobbering, not about the directory existing.
    std::fs::remove_file(parent.join("app/notes.txt")).expect("empty it");
    assert_ok(
        &vilan(&parent, &["init", "app", "--template", "node"]),
        "init into an existing empty directory",
    );
    assert!(parent.join("app/vilan.toml").is_file());
}

#[test]
fn the_current_directory_is_scaffolded_unless_it_is_already_a_project() {
    let project = temp_dir("current");
    // "Empty-ish": unrelated files are fine, since nothing is overwritten.
    std::fs::write(project.join("README.md"), "# notes\n").expect("a stray file");

    let output = vilan(&project, &["init", "--template", "node"]);
    assert_ok(&output, "vilan init (no name)");
    assert!(project.join("vilan.toml").is_file());
    assert!(project.join("README.md").is_file(), "nothing is clobbered");
    // The package name comes from the directory, which here is the temp name.
    let manifest = read_manifest(&project.join("vilan.toml"));
    let name = manifest.package.expect("a package").name.expect("a name");
    assert!(
        name.starts_with("vilan_init_current_"),
        "derived name: {name}"
    );
    assert_ok(&vilan(&project, &["run", "."]), "the scaffold runs");

    // Twice is a refusal: there is already a project here.
    let again = vilan(&project, &["init", "--template", "node"]);
    assert!(!again.status.success(), "a second init is refused");
    assert!(
        combined(&again).contains("already a Vilan project"),
        "{}",
        combined(&again)
    );
}

#[test]
fn naming_the_current_directory_is_the_same_as_naming_nothing() {
    // `vilan init .` is the same destination as `vilan init`, so it takes the
    // current-directory rule — not "already exists and is not empty", which is
    // true of the directory you are standing in and useless to hear.
    let project = temp_dir("dot");
    std::fs::write(project.join("README.md"), "# notes\n").expect("a stray file");

    assert_ok(
        &vilan(&project, &["init", ".", "--template", "node"]),
        "vilan init .",
    );
    assert!(project.join("vilan.toml").is_file());
    assert_ok(&vilan(&project, &["run", "."]), "the scaffold runs");

    let again = vilan(&project, &["init", ".", "--template", "node"]);
    assert!(!again.status.success(), "a second init is refused");
    assert!(
        combined(&again).contains("already a Vilan project"),
        "{}",
        combined(&again)
    );
}

#[test]
fn a_file_a_template_would_overwrite_stops_the_scaffold() {
    let project = temp_dir("collision");
    std::fs::write(project.join("main.vl"), "// mine\n").expect("a stray source");

    let output = vilan(&project, &["init", "--template", "node"]);
    assert!(!output.status.success(), "init never overwrites");
    assert!(
        combined(&output).contains("would overwrite main.vl"),
        "{}",
        combined(&output)
    );
    assert_eq!(
        std::fs::read_to_string(project.join("main.vl")).expect("main.vl"),
        "// mine\n",
        "the existing file is untouched"
    );
    assert!(
        !project.join("vilan.toml").exists(),
        "and nothing was added"
    );
}

#[test]
fn a_directory_name_that_is_not_an_identifier_is_sanitized() {
    // `[package] name` must be an identifier, so the directory name is folded
    // one character at a time — and the result has to actually build.
    let parent = temp_dir("sanitize");
    let output = vilan(&parent, &["init", "my-app.2", "--template", "node"]);
    assert_ok(&output, "init my-app.2");
    assert!(
        combined(&output).contains("package `my_app_2`"),
        "the report should name the derived package:\n{}",
        combined(&output)
    );

    let project = parent.join("my-app.2");
    let manifest = read_manifest(&project.join("vilan.toml"));
    assert_eq!(
        manifest.package.expect("a package").name.as_deref(),
        Some("my_app_2")
    );
    let run = vilan(&project, &["run", "."]);
    assert_ok(&run, "the sanitized package runs");
    assert!(String::from_utf8_lossy(&run.stdout).contains("hello, world"));

    // A leading digit cannot open an identifier either.
    let numeric = vilan(&parent, &["init", "2fast", "--template", "node"]);
    assert_ok(&numeric, "init 2fast");
    let manifest = read_manifest(&parent.join("2fast/vilan.toml"));
    assert_eq!(
        manifest.package.expect("a package").name.as_deref(),
        Some("_2fast")
    );
    assert_ok(&vilan(&parent.join("2fast"), &["check", "."]), "it checks");
}

// --- process helpers (the ssr_fullstack shape) -----------------------------

/// Bind an ephemeral port, then release it (see the retry in the full-stack
/// test for the race this leaves).
fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// Poll until the server accepts a connection (or the deadline passes).
fn wait_for_port(port: u16, deadline: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < deadline {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

/// A plain HTTP GET, returning the response body bytes.
fn http_get(port: u16, path: &str) -> Vec<u8> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect for GET");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    )
    .expect("send GET");
    let mut response = Vec::new();
    let _ = stream.read_to_end(&mut response);
    let separator = b"\r\n\r\n";
    match response
        .windows(separator.len())
        .position(|window| window == separator)
    {
        Some(index) => response[index + separator.len()..].to_vec(),
        None => response,
    }
}
