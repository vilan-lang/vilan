//! The book-wide anchor golden for `std::markdown` (proposal/markdown.md §3):
//! the shipped parser must reproduce mdBook v0.5.4's heading ids — bit-exact,
//! in order, on every rendered page of `vilan/docs/` — and must STRICT-parse
//! every page, so a construct outside the census grammar fails the suite
//! loudly (the ruled failure mode, §9 Q1).
//!
//! `markdown_anchors.golden` is generated from a REAL mdBook build, never
//! from the parser under test:
//!
//!     python3 scripts/regen-markdown-golden.py     # needs mdbook v0.5.4
//!
//! Regenerate it when a docs page's headings change or when the mdBook pin
//! moves, and eyeball the diff — every changed line is a changed URL anchor
//! (the LSP keyword hovers and the book's cross-page links ride on them).
//! The suite itself needs no renderer: this test compiles a walker program
//! against the real `std`, runs it with node, and diffs its output against
//! the committed golden.

use std::path::{Path, PathBuf};

use vilan_core::{BuildOptions, PackageSpec, Platform, Workspace, analyze_source, transform};

fn std_spec() -> PackageSpec {
    vilan_core::manifest::resolve_std(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std"),
    )
}

fn docs_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/docs")
}

/// Every rendered page of the book, relative to `vilan/docs`, sorted —
/// `book/` (build output) and `theme/` are not content, and `SUMMARY.md` is
/// nav, not a rendered page (it is still strict-parsed below).
fn rendered_pages() -> Vec<String> {
    fn walk(directory: &Path, root: &Path, pages: &mut Vec<String>) {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(directory)
            .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()))
            .map(|entry| entry.expect("directory entry").path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                if path
                    .file_name()
                    .is_some_and(|name| name == "book" || name == "theme")
                {
                    continue;
                }
                walk(&path, root, pages);
            } else if path.extension().is_some_and(|extension| extension == "md")
                && path.file_name().is_some_and(|name| name != "SUMMARY.md")
            {
                let relative = path
                    .strip_prefix(root)
                    .expect("under the docs root")
                    .to_string_lossy()
                    .replace('\\', "/");
                pages.push(relative);
            }
        }
    }
    let root = docs_root();
    let mut pages = Vec::new();
    walk(&root, &root, &mut pages);
    pages.sort();
    assert!(pages.len() > 50, "the book has {} pages?", pages.len());
    pages
}

/// The walker program: strict-parses every page through `std::markdown` and
/// prints one `page h<level> <id>` line per heading, in document order
/// (descending into quotes and list items exactly as a renderer would), plus
/// a loud `PARSE-ERROR` line on any refusal. SUMMARY.md is parsed too — the
/// docs gate walks it, so it must stay inside the census grammar — but
/// emits no anchors (mdBook renders no page for it).
fn walker_source(root: &Path, pages: &[String]) -> String {
    let page_list = pages
        .iter()
        .map(|page| format!("\t\t\"{page}\","))
        .collect::<Vec<_>>()
        .join("\n");
    let root = root.display();
    format!(
        r#"
import std::io::print;
import std::fs::read_file_to_str;
import std::markdown::{{ parse, Block, Doc, Inline, ParseError }};
import std::result::Result::{{ Err, Ok }};

fun emit(blocks: List<Block>, page: str) {{
	for block in blocks {{
		match block {{
			Block::Heading(let level, let content, let id) => {{
				print(page + " h" + level.to_string() + " " + id);
			}}
			Block::Quote(let inner) => {{
				emit(inner, page);
			}}
			Block::Items(let ordered, let items) => {{
				for item in items {{
					emit(item, page);
				}}
			}}
			Block::Paragraph(let content) => {{}}
			Block::CodeFence(let info, let body) => {{}}
			Block::Table(let header, let rows) => {{}}
		}}
	}}
}}

fun main() {{
	let root = "{root}/";
	let pages = [
{page_list}
	];
	for page in pages {{
		match parse(read_file_to_str(root + page)) {{
			Ok(let doc) => {{
				emit(doc.blocks, page);
			}}
			Err(let error) => {{
				print("PARSE-ERROR " + page + " " + error.to_string());
			}}
		}}
	}}
	match parse(read_file_to_str(root + "SUMMARY.md")) {{
		Ok(let doc) => {{}}
		Err(let error) => {{
			print("PARSE-ERROR SUMMARY.md " + error.to_string());
		}}
	}}
}}
"#
    )
}

/// Compile through the full pipeline on a large-stack worker (the harness
/// shape the `inference` suite and `docs.rs` use; test targets cannot import one
/// another).
fn compile(source: &str) -> Result<String, Vec<String>> {
    let source = source.to_string();
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let leaked: &'static str = Box::leak(source.into_boxed_str());
                let (program, errors) = analyze_source(
                    leaked,
                    &std_spec(),
                    Path::new("."),
                    Path::new("markdown_golden_walker.vl"),
                    Some(Platform::default()),
                    &Workspace::default(),
                );
                match program {
                    Some(program) if errors.is_empty() => {
                        transform(&program, &BuildOptions::default())
                            .map_err(|error| vec![error.msg])
                    }
                    _ => Err(errors.into_iter().map(|error| error.msg).collect()),
                }
            }))
            .unwrap_or_else(|_| Err(vec!["compiler panicked".to_string()]))
        })
        .expect("spawn worker")
        .join()
        .unwrap_or_else(|_| Err(vec!["compiler thread aborted".to_string()]))
}

fn run_node(js: &str) -> String {
    let path =
        std::env::temp_dir().join(format!("vilan_markdown_golden_{}.mjs", std::process::id()));
    std::fs::write(&path, js).expect("write walker script");
    let output = std::process::Command::new("node")
        .arg(&path)
        .output()
        .expect("run node");
    let _ = std::fs::remove_file(&path);
    assert!(
        output.status.success(),
        "the walker program failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn the_book_parses_strictly_and_reproduces_the_mdbook_anchor_golden() {
    let pages = rendered_pages();
    let js = compile(&walker_source(&docs_root(), &pages))
        .unwrap_or_else(|errors| panic!("the walker program failed to compile: {errors:#?}"));
    let stdout = run_node(&js);

    let refusals: Vec<&str> = stdout
        .lines()
        .filter(|line| line.starts_with("PARSE-ERROR"))
        .collect();
    assert!(
        refusals.is_empty(),
        "std::markdown refused book pages — either the page stepped outside \
         the census grammar (fix the page) or the grammar regressed:\n{}",
        refusals.join("\n")
    );

    let golden_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/markdown_anchors.golden");
    let golden = std::fs::read_to_string(&golden_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", golden_path.display()));
    let actual: Vec<&str> = stdout.lines().collect();
    let expected: Vec<&str> = golden.lines().collect();

    let mismatches: Vec<String> = expected
        .iter()
        .zip(actual.iter())
        .enumerate()
        .filter(|(_, (want, got))| want != got)
        .map(|(index, (want, got))| format!("  [{index}] golden: {want}\n        parser: {got}"))
        .collect();
    assert!(
        mismatches.is_empty() && expected.len() == actual.len(),
        "std::markdown diverges from the mdBook anchor golden \
         ({} golden lines, {} parsed; regenerate deliberately with \
         scripts/regen-markdown-golden.py if the DOCS changed):\n{}",
        expected.len(),
        actual.len(),
        mismatches.join("\n")
    );
    assert!(
        expected.len() >= 400,
        "only {} anchors in the golden — did the walk go wrong?",
        expected.len()
    );
}

/// Package-shape rule 2 (proposal/markdown.md §6, RULED): leaf imports only —
/// Tier-1 core, no platform module, no host binding of any kind.
#[test]
fn the_package_keeps_leaf_imports_and_binds_no_host() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std/src/markdown.vl");
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let allowed = [
        "import pkg::display::",
        "import pkg::option::",
        "import pkg::result::",
    ];
    let mut imports = 0;
    for (index, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("import ") {
            imports += 1;
            assert!(
                allowed.iter().any(|prefix| trimmed.starts_with(prefix)),
                "markdown.vl:{}: `{trimmed}` — the package-shaped ruling allows \
                 only Tier-1 core imports ({allowed:?})",
                index + 1
            );
        }
    }
    assert!(
        imports >= 2,
        "the import scan found {imports} imports — did the file change shape?"
    );
    assert!(
        !source.contains("[extern") && !source.contains("external "),
        "markdown.vl declares a host binding — the package is pure computation by ruling"
    );
}
