//! M104: entry-world analysis, driven through the real `Backend` — what one
//! keystroke analyses, which world each open document is served from, and
//! that no document ever shows diagnostics from a world older than its
//! buffer.
//!
//! **How these hold on Windows.** Nothing here reads a clock to decide an
//! outcome: every wait polls COUNTS (analyses started against analyses
//! finished) and E117 REVISIONS until the server is at rest, with a liveness
//! deadline that only bounds a hang. Every path is compared canonical on both
//! sides (`util::canonical_path`), which is what spells a temp root's 8.3
//! short name and its long one as one file.

use super::session_leak_tests::{open_params, whole_file_change};
use super::snapshot_consistency_tests::backend;
use super::*;

const MANIFEST: &str = "[package]\nname = \"app\"\ndefault-entry = \"server\"\n\n\
     [entry.client]\ntarget = \"browser\"\n\n[entry.server]\n";
const CLIENT: &str = "import pkg::model::{ Model, total };\nimport pkg::views::page;\nimport pkg::channel::listen;\n\n\
     fun main() {\n\tlet model = Model { count = 1 };\n\tlet _ = total(model);\n\tpage();\n\tlisten();\n}\n";
const VIEWS: &str = "import pkg::model::{ Model, total };\n\n\
     export fun page(): i32 {\n\ttotal(Model { count = 2 })\n}\n";
const CHANNEL: &str = "import pkg::model::{ Model, total };\n\n\
     export fun listen(): i32 {\n\ttotal(Model { count = 3 })\n}\n";
const MODEL: &str = "export struct Model {\n\tcount: i32,\n}\n\n\
     export fun total(model: Model): i32 {\n\tlet doubled = model.count * 2;\n\tdoubled\n}\n";
const SERVER: &str = "fun main() {}\n";
const ORPHAN: &str = "fun nobody(): i32 {\n\t4\n}\n";

struct Package {
    directory: PathBuf,
}

impl Package {
    fn new(tag: &str) -> Package {
        let directory = std::env::temp_dir().join(format!(
            "vilan-m104-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id(),
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(directory.join("src")).expect("a scratch package");
        for (relative, contents) in [
            ("vilan.toml", MANIFEST),
            ("src/client.vl", CLIENT),
            ("src/views.vl", VIEWS),
            ("src/channel.vl", CHANNEL),
            ("src/model.vl", MODEL),
            ("src/server.vl", SERVER),
            ("src/orphan.vl", ORPHAN),
        ] {
            std::fs::write(directory.join(relative), contents).expect("a source file");
        }
        Package { directory }
    }

    fn uri(&self, name: &str) -> Url {
        Url::from_file_path(self.directory.join("src").join(name)).expect("a file url")
    }

    fn canonical(&self, name: &str) -> PathBuf {
        vilan_core::util::canonical_path(self.directory.join("src").join(name))
    }
}

impl Drop for Package {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

/// The server at rest: every analysis it started has finished (landed,
/// dropped or cancelled), none is in flight, and every open document's
/// analyzed text is its live text — held over several consecutive polls,
/// which is what steps over the debounce window and the instant between one
/// analysis landing and the dependency sweep starting the next. Counts and
/// texts, never a fixed sleep deciding an outcome.
async fn at_rest(server: &Backend) {
    let deadline = std::time::Instant::now() + ANALYSIS_LIVENESS;
    let mut quiet = 0;
    while quiet < 6 {
        if std::time::Instant::now() >= deadline {
            panic!(
                "the server never came to rest: {:?} dropped {} running {}",
                server.analyses.counts(),
                server.analyses.dropped(),
                server.schedule.running(),
            );
        }
        let counts = server.analyses.counts();
        let finished = counts.landed + counts.cancelled + server.analyses.dropped();
        let current = server
            .documents
            .iter()
            .all(|document| document.analysis_revision() > 0 && !document.is_stale());
        if counts.started == finished && server.schedule.running() == 0 && current {
            quiet += 1;
        } else {
            quiet = 0;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn open_all(server: &Backend, package: &Package, names: &[(&str, &str)]) {
    for (name, text) in names {
        server.did_open(open_params(&package.uri(name), text)).await;
        at_rest(server).await;
    }
}

async fn edit(server: &Backend, uri: &Url, version: i32, text: &str) {
    server
        .did_change(whole_file_change(uri, version, text))
        .await;
}

/// What the editor shows for `uri` right now: the planner's merged view —
/// every owner's group for the file, which is exactly what was last sent.
fn shown(server: &Backend, uri: &Url) -> Vec<Diagnostic> {
    server
        .publish_state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .republish(uri)
        .1
}

fn errors(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == Some(DiagnosticSeverity::ERROR))
        .map(|diagnostic| diagnostic.message.clone())
        .collect()
}

/// THE CASE (the owner's): `model.vl` edited with the three files that import
/// it open. It used to run four analyses — the module's own world, then one
/// per open importer — and now runs ONE: the client entry's world, which
/// every one of the four documents is served from.
#[tokio::test]
async fn m104_a_keystroke_in_a_module_analyses_its_entrys_world_once() {
    let package = Package::new("once");
    let (service, _socket) = backend();
    let server = service.inner();
    open_all(
        server,
        &package,
        &[
            ("client.vl", CLIENT),
            ("views.vl", VIEWS),
            ("channel.vl", CHANNEL),
            ("model.vl", MODEL),
        ],
    )
    .await;
    let before = server.analyses.counts().started;
    let edited = MODEL.replace("model.count * 2", "model.count * 3");
    edit(server, &package.uri("model.vl"), 2, &edited).await;
    at_rest(server).await;
    assert_eq!(
        server.analyses.counts().started - before,
        1,
        "one analysis of the client world serves all four open documents",
    );
    let client = package.canonical("client.vl");
    for name in ["model.vl", "views.vl", "channel.vl"] {
        let document = server.documents.get(&package.uri(name)).expect("open");
        assert_eq!(
            document.world_root(),
            Some(client.as_path()),
            "{name} is served from the client entry's world",
        );
    }
    assert_eq!(
        server
            .documents
            .get(&package.uri("client.vl"))
            .expect("open")
            .world_root(),
        None,
        "the entry is its own world",
    );
    let revisions: Vec<u64> = ["client.vl", "views.vl", "channel.vl", "model.vl"]
        .iter()
        .map(|name| {
            server
                .documents
                .get(&package.uri(name))
                .expect("open")
                .analysis_revision()
        })
        .collect();
    assert!(
        revisions.iter().all(|landed| *landed == revisions[0]),
        "every document landed the SAME analysis: {revisions:?}",
    );
}

/// The entry need not be open for the count to hold: a keystroke in a module
/// whose entry is CLOSED analyzes the entry's world once — the dependency
/// sweep does not analyze the kept world a second time because it, too, read
/// the edited module.
#[tokio::test]
async fn m104_a_keystroke_in_a_module_whose_entry_is_closed_analyses_once() {
    let package = Package::new("closed-once");
    let (service, _socket) = backend();
    let server = service.inner();
    open_all(server, &package, &[("model.vl", MODEL)]).await;
    let before = server.analyses.counts().started;
    edit(
        server,
        &package.uri("model.vl"),
        2,
        &MODEL.replace("model.count * 2", "model.count * 3"),
    )
    .await;
    at_rest(server).await;
    assert_eq!(server.analyses.counts().started - before, 1);
}

/// Opening a file of a world another open document already holds serves it
/// from that world: no analysis — the open costs the file's own tables. And
/// the guard: a buffer that differs from what the held world read (the disk,
/// here) is a world to redo, and the open analyzes.
#[tokio::test]
async fn m104_opening_a_file_of_a_held_world_analyses_nothing() {
    let package = Package::new("held");
    let (service, _socket) = backend();
    let server = service.inner();
    open_all(
        server,
        &package,
        &[("client.vl", CLIENT), ("model.vl", MODEL)],
    )
    .await;
    let before = server.analyses.counts().started;
    open_all(server, &package, &[("views.vl", VIEWS)]).await;
    assert_eq!(
        server.analyses.counts().started - before,
        0,
        "views.vl is served from the client world model.vl's open already holds",
    );
    let views = server
        .documents
        .get(&package.uri("views.vl"))
        .expect("open");
    assert_eq!(
        views.world_root(),
        Some(package.canonical("client.vl").as_path())
    );
    assert!(views.holds_program());
    drop(views);

    let edited = CHANNEL.replace("count = 3", "count = 4");
    open_all(server, &package, &[("channel.vl", &edited)]).await;
    assert_eq!(
        server.analyses.counts().started - before,
        1,
        "a buffer the held world did not read is analyzed",
    );
}

/// A file shows diagnostics as its entry sees them: the module's error is
/// published by the client world's analysis, and the module's own document
/// owns none of it.
#[tokio::test]
async fn m104_a_module_shows_its_diagnostics_as_its_entry_sees_them() {
    let package = Package::new("attribution");
    let (service, _socket) = backend();
    let server = service.inner();
    open_all(server, &package, &[("model.vl", MODEL)]).await;
    let broken = MODEL.replace(
        "let doubled = model.count * 2;",
        "let doubled: i32 = \"text\";",
    );
    edit(server, &package.uri("model.vl"), 2, &broken).await;
    at_rest(server).await;
    assert!(
        !errors(&shown(server, &package.uri("model.vl"))).is_empty(),
        "the module's error is shown",
    );
    let state = server
        .publish_state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    assert!(
        errors(&state.owned_by(&package.uri("model.vl"), &package.uri("model.vl"))).is_empty(),
        "and it is the WORLD's: the module's own document publishes only its paint",
    );
    assert!(
        !errors(&state.owned_by(
            &Url::from_file_path(package.canonical("client.vl")).expect("a file url"),
            &package.uri("model.vl"),
        ))
        .is_empty(),
        "the client entry's analysis owns it — the entry is not even open",
    );
}

/// The acceptance's second half: no document ever shows diagnostics from a
/// stale world. An analysis that read `model.vl` mid-edit (the error) and
/// finishes after the fix has been typed is a view of a world that no longer
/// exists; it lands on NO document of the world and publishes nothing.
#[tokio::test]
async fn m104_no_document_shows_diagnostics_from_a_stale_world() {
    let package = Package::new("stale");
    let (service, _socket) = backend();
    let server = service.inner();
    open_all(
        server,
        &package,
        &[
            ("client.vl", CLIENT),
            ("views.vl", VIEWS),
            ("model.vl", MODEL),
        ],
    )
    .await;
    let broken = MODEL.replace(
        "let doubled = model.count * 2;",
        "let doubled: i32 = \"text\";",
    );
    edit(server, &package.uri("model.vl"), 2, &broken).await;
    at_rest(server).await;
    assert!(!errors(&shown(server, &package.uri("model.vl"))).is_empty());

    // Break again and fix at once: whatever analysis runs between the two
    // read one text or the other, and only the one that read the fix may
    // reach the editor.
    for round in 0..3 {
        edit(server, &package.uri("model.vl"), 3 + 2 * round, &broken).await;
        edit(server, &package.uri("model.vl"), 4 + 2 * round, MODEL).await;
        at_rest(server).await;
        for name in ["client.vl", "views.vl", "model.vl"] {
            assert_eq!(
                errors(&shown(server, &package.uri(name))),
                Vec::<String>::new(),
                "round {round}: {name} shows only what the CURRENT world says",
            );
        }
    }
}

/// The same guarantee, made DETERMINISTIC: a world analysis that read
/// `model.vl` at the broken text — the overlay is live, so this is what an
/// analysis started mid-edit sees — is handed to the landing after the buffer
/// is back to the fixed text. The entry's own buffer never moved, so its
/// document would accept the analysis on text equality alone, and publish the
/// module's error from a world that no longer exists. The world is refused
/// whole: nothing lands, nothing is published, every revision stands.
#[tokio::test]
async fn m104_a_world_that_read_a_buffer_since_moved_lands_nowhere() {
    let package = Package::new("refused");
    let (service, _socket) = backend();
    let server = service.inner();
    open_all(
        server,
        &package,
        &[
            ("client.vl", CLIENT),
            ("views.vl", VIEWS),
            ("model.vl", MODEL),
        ],
    )
    .await;
    let revisions = |server: &Backend| -> Vec<u64> {
        ["client.vl", "views.vl", "model.vl"]
            .iter()
            .map(|name| {
                server
                    .documents
                    .get(&package.uri(name))
                    .expect("open")
                    .analysis_revision()
            })
            .collect()
    };
    let before = revisions(server);
    let model_path = package.directory.join("src/model.vl");
    let client_path = package.directory.join("src/client.vl");
    let broken = MODEL.replace(
        "let doubled = model.count * 2;",
        "let doubled: i32 = \"text\";",
    );
    // The analysis reads the module mid-edit…
    vilan_core::analyzer::set_document_overlay(&model_path, Some(broken.clone()));
    let open: Vec<(Url, PathBuf)> = ["client.vl", "views.vl", "model.vl"]
        .iter()
        .map(|name| (package.uri(name), package.directory.join("src").join(name)))
        .collect();
    let std_dir = discover_std_dir(&client_path);
    let analysis = tokio::task::spawn_blocking(move || {
        analyze_world(
            &client_path,
            Some(CLIENT.to_string()),
            &std_dir,
            &open,
            &CancelToken::new(),
        )
    })
    .await
    .expect("the analysis thread")
    .expect("an uncancelled analysis");
    assert!(
        analysis.root.published_diagnostics().iter().any(|item| item
            .path
            .as_deref()
            .is_some_and(|path| path.ends_with("model.vl"))),
        "the premise: the analysis saw the broken module",
    );
    // …and the fix is back in the buffer before it lands.
    vilan_core::analyzer::set_document_overlay(&model_path, Some(MODEL.to_string()));
    let started_at = server.revision.load(Ordering::SeqCst);
    let outcome = land_world(
        &server.analysis_context(),
        analysis,
        started_at,
        discover_std_dir(&model_path),
    )
    .await;
    assert_eq!(outcome.outcome, AnalysisOutcome::Dropped);
    assert!(outcome.landed.is_empty());
    assert_eq!(
        revisions(server),
        before,
        "no document adopted the stale world"
    );
    for name in ["client.vl", "views.vl", "model.vl"] {
        assert_eq!(
            errors(&shown(server, &package.uri(name))),
            Vec::<String>::new(),
            "{name} never shows the stale world's error",
        );
    }
}

/// The ruling's third clause: a module no entry reaches keeps its own
/// analysis.
#[tokio::test]
async fn m104_a_module_no_entry_reaches_keeps_its_own_analysis() {
    let package = Package::new("orphan");
    let (service, _socket) = backend();
    let server = service.inner();
    open_all(server, &package, &[("orphan.vl", ORPHAN)]).await;
    let document = server
        .documents
        .get(&package.uri("orphan.vl"))
        .expect("open");
    assert_eq!(document.world_root(), None);
    assert!(document.holds_program());
}

/// The entry need not be open: a module alone is served from its entry's
/// world, and closing the module retires that world — nothing it published
/// stays behind.
#[tokio::test]
async fn m104_a_world_lives_while_a_document_it_serves_is_open() {
    let package = Package::new("lifetime");
    let (service, _socket) = backend();
    let server = service.inner();
    open_all(
        server,
        &package,
        &[("client.vl", CLIENT), ("model.vl", MODEL)],
    )
    .await;
    let broken = MODEL.replace(
        "let doubled = model.count * 2;",
        "let doubled: i32 = \"text\";",
    );
    edit(server, &package.uri("model.vl"), 2, &broken).await;
    at_rest(server).await;

    // The entry closes; the module it serves is still open, so its world
    // goes on — the module's error stays, and a fix still lands.
    server
        .did_close(DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier {
                uri: package.uri("client.vl"),
            },
        })
        .await;
    assert!(
        !errors(&shown(server, &package.uri("model.vl"))).is_empty(),
        "closing the entry does not take its world's diagnostics off the module",
    );
    edit(server, &package.uri("model.vl"), 3, MODEL).await;
    at_rest(server).await;
    assert_eq!(
        errors(&shown(server, &package.uri("model.vl"))),
        Vec::<String>::new()
    );
    assert_eq!(
        server
            .documents
            .get(&package.uri("model.vl"))
            .expect("open")
            .world_root(),
        Some(package.canonical("client.vl").as_path()),
    );

    // The last document the world serves closes: the world is retired.
    server
        .did_close(DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier {
                uri: package.uri("model.vl"),
            },
        })
        .await;
    assert!(server.worlds.is_empty(), "no world outlives its documents");
}

/// E113 survives the move: a module BOTH platforms compile is served from the
/// browser entry's world and reported by the node entry's too — here the
/// browser `View`'s `element` field, clean under the browser leg and "no
/// field" under the node leg. With only the module open, neither entry is,
/// and the node leg's error still reaches it.
#[tokio::test]
async fn m104_a_module_both_entries_reach_reports_both_legs() {
    const WIDGET: &str = "import std::ui::{ View, view };\n\n\
         export fun attach(): View {\n\tlet root = view(\"div\");\n\t\
         root.element.set_attribute(\"id\", \"app\");\n\troot\n}\n";
    const REACHES: &str = "import pkg::widget::attach;\n\nfun main() {\n\tattach();\n}\n";
    let package = Package::new("legs");
    for (name, text) in [
        ("widget.vl", WIDGET),
        ("client.vl", REACHES),
        ("server.vl", REACHES),
    ] {
        std::fs::write(package.directory.join("src").join(name), text).expect("a source");
    }
    let (service, _socket) = backend();
    let server = service.inner();
    open_all(server, &package, &[("widget.vl", WIDGET)]).await;
    let document = server
        .documents
        .get(&package.uri("widget.vl"))
        .expect("open");
    assert_eq!(
        document.world_root(),
        Some(package.canonical("client.vl").as_path()),
        "the browser leg serves it",
    );
    assert_eq!(document.further_worlds(), &[package.canonical("server.vl")]);
    drop(document);
    let shown = errors(&shown(server, &package.uri("widget.vl")));
    assert!(
        shown.iter().any(|message| message.contains("element")),
        "the node leg's error reaches the module: {shown:?}",
    );
    assert!(
        server.worlds.contains_key(&package.canonical("server.vl")),
        "the node entry's world is kept while the module is open",
    );
}

/// E247: the status bar's menu asks `vilan/analysisPlatform`, and the answer
/// names the entry whose world the file is analyzed in — `null` for a file
/// that is its own entry — and the analysis's size in COUNTS. A released
/// document (M63: not one of the focused few) still answers.
#[tokio::test]
async fn e247_the_platform_answer_names_the_world_and_the_work_counts() {
    let package = Package::new("status");
    let (service, _socket) = backend();
    let server = service.inner();
    open_all(
        server,
        &package,
        &[
            ("client.vl", CLIENT),
            ("views.vl", VIEWS),
            ("model.vl", MODEL),
        ],
    )
    .await;
    let model = answer(server, &package.uri("model.vl"))
        .await
        .expect("an answer for the module");
    assert_eq!(
        model["world"].as_str().map(PathBuf::from),
        Some(package.canonical("client.vl")),
        "{model}"
    );
    assert!(
        model["work"]["files"]
            .as_u64()
            .is_some_and(|files| files > 3),
        "{model}"
    );
    assert!(
        model["work"]["entities"]
            .as_u64()
            .is_some_and(|count| count > 0),
        "{model}"
    );
    assert_eq!(model["platform"], "browser");
    let client = answer(server, &package.uri("client.vl"))
        .await
        .expect("an answer for the entry");
    assert!(
        client["world"].is_null(),
        "the entry is its own world: {client}"
    );
    // `client.vl` was opened first and has since fallen out of the focused
    // two, so it answers from what it captured before its program went.
    assert!(
        !server
            .documents
            .get(&package.uri("client.vl"))
            .expect("open")
            .holds_program(),
        "the premise: the entry's document was released"
    );
    assert!(
        client["work"]["files"]
            .as_u64()
            .is_some_and(|files| files > 3)
    );
}

async fn answer(server: &Backend, uri: &Url) -> Option<serde_json::Value> {
    server
        .analysis_platform(TextDocumentIdentifier { uri: uri.clone() })
        .await
        .expect("the request answers")
}
