//! End-to-end runtime test for `std::router` + `View.swap` (backlog A10,
//! proposal/router.md): a browser-target app is built with the real CLI and
//! run under node against a ~60-line DOM/history stub, asserting the routing
//! semantics the corpus can't reach (it only runs process-platform programs):
//! parse-on-load, typed `link` hrefs, plain-click interception (and modifier
//! passthrough), `pushState`/`popstate` driving one signal, nested layouts
//! through `swap`, the `PartialEq` no-op on an unchanged route, and disposal
//! of a swapped-out subtree's subscriptions — `bind_text`'s, and (A21)
//! `style_var`'s, whose signal is written from a button outside the subtree so
//! the write lands after the page is gone.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A fresh temp directory for the test's project tree.
fn temp_project(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("vilan_router_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// Writes `contents` to `dir/relative`, creating parent directories.
fn write(dir: &Path, relative: &str, contents: &str) {
    let path = dir.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

/// The app under test: the Kolt-shaped route space — nested enums mirroring
/// nested layouts, a hand-written `parse`/`href` pair over `segments`, typed
/// `link`s, programmatic `navigate`, and a `swap`-rendered page tree.
const APP: &str = r#"import std::ui::{ View, view, mount_root };
import std::reactive::{ Signal, SignalCell };
import std::router::{ current_path, navigate, segments, link, Routable };
import std::option::Option::{ self, Some, None };

[derive(PartialEq)]
enum Route {
	Home,
	Login,
	Workspace(str, WorkspaceRoute),
	NotFound,
}

[derive(PartialEq)]
enum WorkspaceRoute {
	Overview,
	Tasks,
	Task(i32),
}

fun parse(path: str): Route {
	let parts = segments(path);
	match parts.len() {
		0 => Route::Home,
		1 => if parts[0] == "login" { Route::Login } else { Route::NotFound },
		_ => {
			if parts[0] == "w" {
				Route::Workspace(parts[1], parse_workspace(parts))
			} else {
				Route::NotFound
			}
		},
	}
}

fun parse_workspace(parts: List<str>): WorkspaceRoute {
	if parts.len() == 2 {
		WorkspaceRoute::Overview
	} else if parts[2] == "tasks" {
		WorkspaceRoute::Tasks
	} else if parts[2] == "task" && parts.len() > 3 {
		match parts[3].parse_i32() {
			Some(let id) => WorkspaceRoute::Task(id),
			None => WorkspaceRoute::Overview,
		}
	} else {
		WorkspaceRoute::Overview
	}
}

fun href(route: Route): str {
	match route {
		Route::Home => "/",
		Route::Login => "/login",
		Route::Workspace(let org, let inner) => i"/w/{org}" + workspace_href(inner),
		Route::NotFound => "/404",
	}
}

fun workspace_href(inner: WorkspaceRoute): str {
	match inner {
		WorkspaceRoute::Overview => "",
		WorkspaceRoute::Tasks => "/tasks",
		WorkspaceRoute::Task(let id) => i"/task/{id}",
	}
}

impl Route with Routable {
	fun to_path(self): str {
		href(self)
	}
}

fun home_page(): View {
	view("section").text("home")
}

/// The swapped-AWAY page for the disposal check (A21): a `style_var` whose
/// signal is written from a button OUTSIDE the swapped subtree, so the write
/// can be fired after this page is gone.
fun login_page(width: SignalCell<str>): View {
	view("section")
		.attr("id", "login")
		.style_var("--w", width)
		.bind_text(current_path())
}

fun workspace_layout(org: str, inner: WorkspaceRoute): View {
	view("section")
		.child(view("aside").text(org))
		.child(match inner {
			WorkspaceRoute::Overview => view("div").text("overview"),
			WorkspaceRoute::Tasks => view("div").text("tasks"),
			WorkspaceRoute::Task(let id) => view("div").text(i"task {id}"),
		})
}

fun app(route: SignalCell<Route>): View {
	let width = Signal::new("10px");
	view("main")
		.child(view("nav")
			.child(link("Home", Route::Home))
			.child(link("Tasks", Route::Workspace("acme", WorkspaceRoute::Tasks))))
		.child(view("button").text("go").on("click", || navigate(href(Route::Login))))
		.child(view("button").attr("id", "widen").text("widen").on("click", || width.set("99px")))
		.swap(route, |current| match current {
			Route::Home => home_page(),
			Route::Login => login_page(width),
			Route::Workspace(let org, let inner) => workspace_layout(org, inner),
			Route::NotFound => view("section").text("not found"),
		})
}

fun main() {
	let route = current_path().map(parse);
	let _root = mount_root("app", || app(route));
}
"#;

/// The DOM/history stub plus the behavioral assertions, run under node against
/// the compiled bundle. Prints one `ok - ..` line per assertion; exits 1 on
/// any failure.
const HARNESS: &str = concat!(
    include_str!("support/dom/stub.js"),
    include_str!("support/dom/router.js"),
    r##"require("./app.js");

let failures = 0;
const assert = (cond, msg) => {
    if (!cond) { failures += 1; console.error("FAIL - " + msg); }
    else console.log("ok   - " + msg);
};

const main = root.children[0];
// A71: `swap` now renders at its OWN position in the chain — before the
// region's empty text anchor, which is what the chain appended last. The page
// is therefore the last ELEMENT child, not the last child.
const page = () => main.children.filter(c => c.tagName !== "#text").pop();

assert(page().tagName === "section" && page().textContent === "home", "initial route renders home");

const nav = main.children[0];
const homeLink = nav.children[0];
const tasksLink = nav.children[1];
assert(tasksLink.tagName === "a" && tasksLink.attributes.href === "/w/acme/tasks",
    "link renders <a href> printed from the route value");

let event = tasksLink.click();
assert(event.prevented, "plain left-click is intercepted (preventDefault)");
assert(global.location.pathname === "/w/acme/tasks", "link click navigated (pushState)");
assert(page().render().includes("acme") && page().render().includes("tasks"),
    "swap rendered the nested workspace layout");

const before = page();
tasksLink.click();
assert(page() === before, "navigating to the current route is a no-op (PartialEq dedupe)");

event = homeLink.click({ metaKey: true });
assert(!event.prevented && global.location.pathname === "/w/acme/tasks",
    "modified click keeps native anchor behavior (no interception)");

main.find(e => e.tagName === "button").click();
assert(global.location.pathname === "/login", "navigate() from an event handler");
const loginPage = page();
assert(loginPage.textContent === "/login", "page binding tracks current_path()");
assert(loginPage.styleProperties["--w"] === "10px", "style_var writes the custom property on mount");

homeLink.click();
assert(page().textContent === "home", "navigated back to home");
// Within the unmounting turn itself the page's subscriber may fire once more
// (notification order inside one drain — the recorded turn semantics); the
// disposal guarantee is about every LATER change.
const staleText = loginPage.textContent;
tasksLink.click();
assert(loginPage.textContent === staleText,
    "swapped-out subtree's subscription was disposed (detached element never updates again)");

// A21: `style_var` was the one reactive View method built on `sub` + a parked
// `let _sub` instead of `effect`, so its subscription never reached the owner
// and outlived the boundary. Fire its signal from a button OUTSIDE the swapped
// subtree, well after the unmounting turn: a leaked subscription writes the
// detached element, a disposed one does not.
main.find(e => e.attributes.id === "widen").click();
assert(loginPage.styleProperties["--w"] === "10px",
    "style_var's subscription was disposed with the swapped-out subtree");

global.location.pathname = "/login";
global.window.fire("popstate", {});
assert(page().textContent === "/login", "popstate (back/forward) drives the same route signal");

process.exit(failures === 0 ? 0 : 1);
"##,
);

#[test]
fn router_swap_link_and_history_semantics() {
    let dir = temp_project("swap");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"router_e2e\"\nroot = \".\"\nentry = \"app.vl\"\ntarget = \"browser\"\n",
    );
    write(&dir, "app.vl", APP);
    write(&dir, "harness.js", HARNESS);

    let build = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["build", dir.to_str().unwrap()])
        .output()
        .expect("run vilan build");
    assert!(
        build.status.success(),
        "vilan build failed:\n{}\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    let run = Command::new("node")
        .arg("harness.js")
        .current_dir(&dir)
        .output()
        .expect("run node harness");
    assert!(
        run.status.success(),
        "router harness failed:\n{}\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// --- A62: the READ direction ------------------------------------------------

/// A62's exhibit: kolt's own route space, with the hand-written `parse` over
/// `segments` replaced by a `FromPath` impl over `parse_path(path).segments` —
/// and the query, fragment and percent-decoding that were unreachable before.
const PARSE_APP: &str = r#"import std::io::print;
import std::router::{
	FromPath,
	Routable,
	from_path,
	location_url,
	parse_path,
	parse_query,
	percent_decode,
	segments,
};

[derive(PartialEq)]
enum Route {
	Home,
	Messages,
	Workspace(i32, WorkspaceRoute),
	NotFound,
}

[derive(PartialEq)]
enum WorkspaceRoute {
	Overview,
	Task(i32),
}

impl Route with FromPath {
	fun from_segments(parts: List<str>): Route {
		match parts.len() {
			0 => Route::Home,
			_ => {
				if parts[0] == "m" {
					Route::Messages
				} else if parts[0] == "w" && parts.len() > 1 {
					match parts[1].parse_i32() {
						Some(let id) => Route::Workspace(id, WorkspaceRoute::from_segments(parts)),
						None => Route::NotFound,
					}
				} else {
					Route::NotFound
				}
			},
		}
	}
}

impl WorkspaceRoute with FromPath {
	fun from_segments(parts: List<str>): WorkspaceRoute {
		match parts.len() {
			4 => {
				if parts[2] == "task" {
					match parts[3].parse_i32() {
						Some(let task) => WorkspaceRoute::Task(task),
						None => WorkspaceRoute::Overview,
					}
				} else {
					WorkspaceRoute::Overview
				}
			},
			_ => WorkspaceRoute::Overview,
		}
	}
}

impl Route with Routable {
	fun to_path(self): str {
		match self {
			Route::Home => "/",
			Route::Messages => "/m",
			Route::Workspace(let id, let inner) => {
				let tail = match inner {
					WorkspaceRoute::Overview => "",
					WorkspaceRoute::Task(let task) => i"/task/{task}",
				};
				i"/w/{id}" + tail
			},
			Route::NotFound => "/404",
		}
	}
}

fun name(route: Route): str {
	match route {
		Route::Home => "home",
		Route::Messages => "messages",
		Route::Workspace(let id, let inner) => {
			let tail = match inner {
				WorkspaceRoute::Overview => "overview",
				WorkspaceRoute::Task(let task) => i"task{task}",
			};
			i"ws{id}/" + tail
		},
		Route::NotFound => "notfound",
	}
}

/// One line per path: what it parses to, and what that route prints back.
fun report(path: str) {
	let route: Route = from_path(path);
	let label = name(route);
	let printed = route.to_path();
	print(i"route {path} -> {label} -> {printed}");
}

fun main() {
	// The empty-segment rule: a leading, trailing or doubled slash produces no
	// segment, so every spelling of the same route parses alike and no match
	// arm has to name the trailing-slash case.
	report("/");
	report("");
	report("/m");
	report("/w/3");
	report("/w/3/");
	report("//w//3//");
	report("/w/3/task/7");
	report("/x");
	// A query and a fragment do not disturb the segments.
	report("/w/3?tab=open#notes");

	let full = parse_path("/w/acme/tasks/?tag=due%20soon&open&a+b=c+d#notes");
	let count = full.segments.len();
	print(i"segments {count} {full.segments[0]}/{full.segments[1]}/{full.segments[2]}");
	let tag = full.query.get("tag").unwrap();
	let open = full.query.get("open").unwrap();
	let plus = full.query.get("a b").unwrap();
	let size = full.query.len();
	print(i"query {size} tag={tag} open=[{open}] plus={plus}");
	let fragment = full.fragment.unwrap();
	print(i"fragment {fragment}");

	// The fragment is cut FIRST: a `?` after a `#` is fragment text.
	let hashed = parse_path("/a#b?c");
	let hashed_fragment = hashed.fragment.unwrap();
	let hashed_query = hashed.query.len();
	print(i"hash-first {hashed_fragment} {hashed_query}");

	// `None` (no `#` at all) and `Some("")` (a bare trailing `#`) are different
	// URLs, and the shape says so.
	let absent = parse_path("/a").fragment.is_none();
	let bare = parse_path("/a#").fragment.unwrap();
	print(i"fragment-absent {absent} bare=[{bare}]");

	// Decoding happens AFTER the split, so an escaped separator stays inside
	// its segment instead of becoming one.
	let escaped = parse_path("/hello%20world/a%2Fb");
	let escaped_count = escaped.segments.len();
	print(i"decoded {escaped_count} [{escaped.segments[0]}] [{escaped.segments[1]}]");
	// `segments` stays RAW — the pair is deliberate, and documented.
	let raw = segments("/hello%20world");
	print(i"raw [{raw[0]}]");

	let cafe = percent_decode("caf%C3%A9");
	let malformed = percent_decode("%zz");
	print(i"decode {cafe} {malformed}");

	let sparse = parse_query("&&a=1&").len();
	let repeated = parse_query("a=1&a=2").get("a").unwrap();
	print(i"query-forms {sparse} {repeated}");

	let url = location_url();
	print(i"url {url}");
}

main();
"#;

/// The stub `parse_path` needs: a location for `location_url`, plus the DOM
/// globals a browser bundle touches at load.
const PARSE_HARNESS: &str = r##"global.document = {
    createElement: () => ({ children: [], attributes: {}, style: { setProperty() {} },
        appendChild() {}, replaceChildren() {}, addEventListener() {} }),
    getElementById: () => null,
    querySelector: () => null,
    querySelectorAll: () => [],
};
global.window = { addEventListener: () => {}, removeEventListener: () => {} };
global.location = { pathname: "/w/acme", search: "?tag=x", hash: "#top" };
global.history = { pushState() {} };
require("./app.js");
"##;

#[test]
fn a62_parse_path_reads_a_url_and_from_path_round_trips_through_to_path() {
    let dir = temp_project("parse");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"router_parse\"\nroot = \".\"\nentry = \"app.vl\"\ntarget = \"browser\"\n",
    );
    write(&dir, "app.vl", PARSE_APP);
    write(&dir, "harness.js", PARSE_HARNESS);

    let build = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["build", dir.to_str().unwrap()])
        .output()
        .expect("run vilan build");
    assert!(
        build.status.success(),
        "vilan build failed:\n{}\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    let run = Command::new("node")
        .arg("harness.js")
        .current_dir(&dir)
        .output()
        .expect("run node harness");
    let stdout = String::from_utf8_lossy(&run.stdout).into_owned();
    assert!(
        run.status.success(),
        "parse harness failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&run.stderr)
    );

    let expected = [
        // The empty-segment rule: `""`, `"/"`, a trailing slash and doubled
        // slashes all parse alike, so no match arm names the trailing-slash case.
        "route / -> home -> /",
        "route  -> home -> /",
        "route /m -> messages -> /m",
        "route /w/3 -> ws3/overview -> /w/3",
        "route /w/3/ -> ws3/overview -> /w/3",
        "route //w//3// -> ws3/overview -> /w/3",
        "route /w/3/task/7 -> ws3/task7 -> /w/3/task/7",
        "route /x -> notfound -> /404",
        "route /w/3?tab=open#notes -> ws3/overview -> /w/3",
        "segments 3 w/acme/tasks",
        // `%20` decodes, a bare key is present with an empty value, and `+` is a
        // space in a query (only in a query).
        "query 3 tag=due soon open=[] plus=c d",
        "fragment notes",
        // The fragment is cut FIRST: a `?` after a `#` is fragment text.
        "hash-first b?c 0",
        "fragment-absent true bare=[]",
        // Decoding happens after the split, so `%2F` stays inside its segment.
        "decoded 2 [hello world] [a/b]",
        "raw [hello%20world]",
        "decode café %zz",
        // Empty pairs are skipped; a repeated key keeps the last value.
        "query-forms 1 2",
        "url /w/acme?tag=x#top",
    ];
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines, expected,
        "parse_path/from_path output drifted; got:\n{stdout}"
    );
}

// --- B293: the anchor `link` builds is not drag-armed -----------------------

/// B293's exhibit, in both spellings: `link`, and a hand-built anchor turned
/// into a link by `View::link_to`. Both must carry `href` AND
/// `draggable="false"`, and both must still intercept exactly the plain
/// left-click.
const DRAGGABLE_APP: &str = r#"import std::ui::{ View, view, mount_root };
import std::router::{ link, Routable };

[derive(PartialEq)]
enum Route {
	Home,
	Tasks,
}

impl Route with Routable {
	fun to_path(self): str {
		match self {
			Route::Home => "/",
			Route::Tasks => "/tasks",
		}
	}
}

fun main() {
	let _root = mount_root("app", || {
		view("nav")
			.child(link("Tasks", Route::Tasks))
			.child(view("a").attr("class", "own").link_to(Route::Home).text("Home"))
	});
}
"#;

/// The stub is the minimum this claim needs: attributes, children, and a click
/// that records `preventDefault`.
const DRAGGABLE_HARNESS: &str = concat!(
    include_str!("support/dom/stub.js"),
    include_str!("support/dom/router.js"),
    r##"require("./app.js");

let failures = 0;
const assert = (cond, msg) => {
    if (!cond) { failures += 1; console.error("FAIL - " + msg); }
    else console.log("ok   - " + msg);
};
const nav = root.children[0];
const built = nav.children[0];
const own = nav.children[1];

assert(built.tagName === "a" && built.attributes.href === "/tasks",
    "link still builds a real <a href>");
assert(built.attributes.draggable === "false",
    "the anchor link builds is not drag-armed (B293)");
assert(own.attributes.href === "/" && own.attributes.draggable === "false",
    "link_to arms an app's own anchor the same way");
assert(own.attributes.class === "own", "link_to leaves the app's own attributes alone");

let event = built.click();
assert(event.prevented && global.location.pathname === "/tasks",
    "a plain left-click is still intercepted");
event = own.click({ metaKey: true });
assert(!event.prevented && global.location.pathname === "/tasks",
    "a modified click still keeps native anchor behavior");

process.exit(failures === 0 ? 0 : 1);
"##,
);

/// B293: an `<a href>` is drag-armed by default, and a link drag started while
/// a quick click's in-app navigation is settling wedges the TAB in Chrome
/// (per tab, surviving a hard refresh). std stops arming the gesture; the href
/// — and everything native that rides on it — stays.
#[test]
fn b293_the_anchor_link_builds_is_not_drag_armed() {
    let dir = temp_project("draggable");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"router_draggable\"\nroot = \".\"\nentry = \"app.vl\"\ntarget = \"browser\"\n",
    );
    write(&dir, "app.vl", DRAGGABLE_APP);
    write(&dir, "harness.js", DRAGGABLE_HARNESS);

    let build = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["build", dir.to_str().unwrap()])
        .output()
        .expect("run vilan build");
    assert!(
        build.status.success(),
        "vilan build failed:\n{}\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    let run = Command::new("node")
        .arg("harness.js")
        .current_dir(&dir)
        .output()
        .expect("run node harness");
    assert!(
        run.status.success(),
        "draggable harness failed:\n{}\n{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );

    let _ = std::fs::remove_dir_all(&dir);
}
