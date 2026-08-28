//! `std::build`'s content-type table, generated from the `mime-db` dataset and
//! held to it.
//!
//! `serve_build` types every artifact it serves from the file's extension. For
//! most of this tree's life that table was four hand-written rows, and the
//! reason recorded for its size was a capability claim that had stopped being
//! true — no vilan program could read a binary file, so no row could name
//! `image/png`. `read_bytes`, `Bytes` and `body_bytes` all exist now and
//! `BuildAsset` carries bytes end to end, so the table grew to cover what a
//! build emits (kolt.local 022, 030).
//!
//! A table that size is not something to hand-type. Vite does not: it ships
//! `mrmime`, generated from `mime-db`. Neither does this one. The lineage is
//!
//! ```text
//!   mime-db  --scripts/regen-mime-table.py-->  mime-table.tsv  --this file-->  build.vl
//! ```
//!
//! and each arrow has exactly one implementation. The left arrow runs rarely
//! (when mime-db publishes) and needs the network; the right arrow is this
//! file, runs on every `cargo test`, and needs nothing.
//!
//! Five gates, the `grammar_sync` (Order 11) and `style_table_sync` (Order 12)
//! shapes together:
//!
//!   1. **The generated region is current** — the `match` arms between the
//!      `GENERATED(mime-table)` markers in `build.vl` are regenerated from the
//!      TSV and compared BYTE FOR BYTE. `VILAN_REGENERATE_MIME_TABLE=1` rewrites
//!      them instead of failing. This is the only gate that needs to pass for
//!      the table to be honest; the rest exist to catch a bad TSV.
//!   2. **The charset rule** — a `text/*` row is served `; charset=utf-8` and
//!      NOTHING else carries a charset, in both directions. The body goes out as
//!      raw bytes, so an unspelled `text/*` is decoded by the browser's default;
//!      `application/json` and its `+json` relatives are utf8 by spec, where a
//!      charset is the error rather than the fix.
//!   3. **The curation is pinned** — the extension list is restated here and
//!      held equal to the TSV's, so growing the table is a deliberate edit in
//!      two places and never a silent consequence of an upstream refresh.
//!   4. **The fence still stands** — `_ => None` survives (an unknown extension
//!      is not served rather than guessed at), no row types a body that needs
//!      `Range` to be usable, and the four rows that predate this table still
//!      carry the media types they always did, so no deployed client's wire
//!      changes underneath it.
//!   5. **Provenance** — the TSV names the pinned mime-db release, and the
//!      script that produced it pins the same one.
//!
//! The behaviour is pinned on a running server in
//! `crates/vilan-cli/tests/serve_build.rs`.

use std::collections::BTreeMap;
use std::path::PathBuf;

/// The std surface whose table this gates.
const BUILD_SOURCE: &str = "vilan/std/src/process/build.vl";

/// The dataset the table is generated from.
const DATASET: &str = "crates/vilan-core/tests/mime-table.tsv";

/// The upstream the dataset is derived from. The TSV header and
/// `scripts/regen-mime-table.py` must both name exactly this.
const PINNED_MIME_DB: &str = "mime-db 1.54.0";

/// Set (to anything) to make [`the_generated_table_is_the_dataset`] REWRITE the
/// stale arms in place instead of failing — the regeneration entry point.
const REGENERATE_ENV: &str = "VILAN_REGENERATE_MIME_TABLE";

/// The regeneration command, verbatim — named by every red this file raises and
/// carried in the generated seam's own marker.
const REGENERATE_COMMAND: &str =
    "VILAN_REGENERATE_MIME_TABLE=1 cargo test -p vilan-core --test mime_table_sync";

const BEGIN_MARKER: &str = "// GENERATED(mime-table)";
const END_MARKER: &str = "// END GENERATED(mime-table)";

/// The curated extension list, restated away from the TSV so that neither can
/// move alone (gate 3). Growing `serve_build`'s reach is a decision, not a
/// consequence of `mime-db` publishing.
const CURATED: &[&str] = &[
    "apng",
    "avif",
    "bmp",
    "csv",
    "css",
    "gif",
    "htm",
    "html",
    "ico",
    "jpeg",
    "jpg",
    "js",
    "json",
    "map",
    "mjs",
    "otf",
    "pdf",
    "png",
    "svg",
    "ttf",
    "txt",
    "wasm",
    "webmanifest",
    "webp",
    "woff",
    "woff2",
    "xml",
];

/// The rows that predate the generated table, with the media types they were
/// served with before it (gate 4). These are already on the wire in released
/// versions: the table may add a charset, but it may not retype them.
const PRE_EXISTING: &[(&str, &str)] = &[
    ("js", "text/javascript"),
    ("mjs", "text/javascript"),
    ("css", "text/css"),
    ("json", "application/json"),
    ("html", "text/html"),
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repo root is two levels above the crate")
}

fn read(relative: &str) -> String {
    let path = repo_root().join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{} is unreadable: {error}", path.display()))
}

/// One dataset row: the extension, the media type `mime-db` gave it, and where
/// that came from.
struct Row {
    group: String,
    extension: String,
    media_type: String,
    provenance: String,
}

fn dataset() -> Vec<Row> {
    let text = read(DATASET);
    let rows: Vec<Row> = text
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let columns: Vec<&str> = line.split('\t').collect();
            assert_eq!(
                columns.len(),
                4,
                "{DATASET}: `{line}` is not four tab-separated columns — refresh it with \
                 `python3 scripts/regen-mime-table.py`"
            );
            Row {
                group: columns[0].to_string(),
                extension: columns[1].to_string(),
                media_type: columns[2].to_string(),
                provenance: columns[3].to_string(),
            }
        })
        .collect();
    // Anti-vacuity: a parser that silently stops matching must go red, not green.
    assert!(
        rows.len() >= 25,
        "{DATASET} parsed as only {} row(s) — the format moved under this gate",
        rows.len()
    );
    rows
}

/// The content type a row is SERVED with — the charset rule, in the one place
/// that owns it.
fn served_type(media_type: &str) -> String {
    if media_type.starts_with("text/") {
        format!("{media_type}; charset=utf-8")
    } else {
        media_type.to_string()
    }
}

/// The `match` arms the dataset implies, in dataset order, indented as the two
/// levels of `content_type_of`'s body require.
fn generated_arms(rows: &[Row]) -> Vec<String> {
    let mut lines = vec![format!(
        "\t\t{BEGIN_MARKER}: from {DATASET}, itself derived from"
    )];
    lines.push(format!(
        "\t\t// {PINNED_MIME_DB} — regenerate: {REGENERATE_COMMAND}"
    ));
    let mut group = String::new();
    for row in rows {
        if row.group != group {
            group = row.group.clone();
            lines.push(format!("\t\t// {group}"));
        }
        lines.push(format!(
            "\t\t\"{}\" => Some(\"{}\"),",
            row.extension,
            served_type(&row.media_type)
        ));
    }
    lines.push(format!("\t\t{END_MARKER}"));
    lines
}

/// `build.vl` with its generated region replaced by `arms`.
fn spliced(current: &[String], arms: &[String]) -> Vec<String> {
    let begin = current
        .iter()
        .position(|line| line.trim_start().starts_with(BEGIN_MARKER))
        .unwrap_or_else(|| panic!("{BUILD_SOURCE}: no `{BEGIN_MARKER}` seam"));
    let end = current
        .iter()
        .position(|line| line.trim_start().starts_with(END_MARKER))
        .unwrap_or_else(|| panic!("{BUILD_SOURCE}: no `{END_MARKER}` seam"));
    assert!(
        begin < end,
        "{BUILD_SOURCE}: the generated seam's markers are out of order"
    );
    let mut spliced = current[..begin].to_vec();
    spliced.extend_from_slice(arms);
    spliced.extend_from_slice(&current[end + 1..]);
    spliced
}

fn first_difference(current: &[String], desired: &[String]) -> String {
    for (index, (a, b)) in current.iter().zip(desired.iter()).enumerate() {
        if a != b {
            return format!(
                "line {}:\n  in the file: {a}\n  generated:   {b}",
                index + 1
            );
        }
    }
    format!(
        "the file has {} line(s), the generated table implies {}",
        current.len(),
        desired.len()
    )
}

// --- 1. The generated region is current -------------------------------------

#[test]
fn the_generated_table_is_the_dataset() {
    let rows = dataset();
    let path = repo_root().join(BUILD_SOURCE);
    let current: Vec<String> = read(BUILD_SOURCE).split('\n').map(String::from).collect();
    let arms = generated_arms(&rows);
    let desired = spliced(&current, &arms);

    assert_eq!(
        spliced(&desired, &arms),
        desired,
        "{BUILD_SOURCE}: regeneration is not idempotent — a generated line matches a seam marker"
    );

    if current == desired {
        return;
    }
    if std::env::var_os(REGENERATE_ENV).is_some() {
        std::fs::write(&path, desired.join("\n")).expect("the table is writable");
        eprintln!("regenerated {BUILD_SOURCE}");
        return;
    }
    panic!(
        "{BUILD_SOURCE}'s content-type table is not what {DATASET} implies — the dataset moved \
         without regeneration, or the arms were hand-edited. Never hand-edit a generated \
         fragment; regenerate: `{REGENERATE_COMMAND}`\n{}",
        first_difference(&current, &desired)
    );
}

// --- 2. The charset rule ----------------------------------------------------

#[test]
fn exactly_the_text_rows_spell_a_charset() {
    for row in dataset() {
        let served = served_type(&row.media_type);
        let is_text = row.media_type.starts_with("text/");
        let spells_charset = served.contains("charset=");
        assert_eq!(
            is_text, spells_charset,
            "`.{}` is served `{served}`: a `text/*` type MUST spell `; charset=utf-8` (the body \
             goes out as raw bytes, so an unspelled one is decoded by the browser's default) and \
             anything else MUST NOT (`application/json` and its `+json` relatives are utf8 by \
             spec, where a charset parameter is an error)",
            row.extension
        );
        if spells_charset {
            assert!(
                served.ends_with("; charset=utf-8"),
                "`.{}` spells a charset that is not `; charset=utf-8`: {served}",
                row.extension
            );
        }
    }
}

#[test]
fn the_json_family_carries_no_charset() {
    // The rule above is stated over `text/`; this is the case it exists to get
    // right, asserted by name so a future rewrite cannot lose it quietly.
    let rows = dataset();
    for extension in ["json", "map", "webmanifest"] {
        let row = rows
            .iter()
            .find(|row| row.extension == extension)
            .unwrap_or_else(|| panic!("the dataset lost `.{extension}`"));
        assert!(
            row.media_type.ends_with("json"),
            "`.{extension}` is typed {}, which is no longer a json type",
            row.media_type
        );
        assert!(
            !served_type(&row.media_type).contains("charset"),
            "`.{extension}` is served with a charset — json is utf8 by spec"
        );
    }
    let webmanifest = rows
        .iter()
        .find(|row| row.extension == "webmanifest")
        .expect("the dataset lost `.webmanifest`");
    assert_eq!(
        webmanifest.media_type, "application/manifest+json",
        "a `.webmanifest` served as anything else is rejected by Chrome"
    );
}

// --- 3. The curation is pinned ----------------------------------------------

#[test]
fn the_dataset_covers_exactly_the_curated_extensions() {
    let rows = dataset();
    let mut in_dataset: Vec<&str> = rows.iter().map(|row| row.extension.as_str()).collect();
    in_dataset.sort_unstable();
    let mut curated = CURATED.to_vec();
    curated.sort_unstable();
    assert_eq!(
        in_dataset, curated,
        "{DATASET} and this file's CURATED list disagree about which extensions `serve_build` \
         serves. Growing the table is a decision taken in both places — in \
         `scripts/regen-mime-table.py`'s CURATED, and here — never a silent consequence of an \
         upstream refresh"
    );

    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    for row in &rows {
        *seen.entry(row.extension.as_str()).or_default() += 1;
    }
    for (extension, count) in seen {
        assert_eq!(
            count, 1,
            "`.{extension}` has {count} rows — the first would win and the rest would be dead"
        );
    }
}

#[test]
fn every_media_type_is_well_formed_and_lowercase() {
    for row in dataset() {
        assert_eq!(
            row.extension,
            row.extension.to_ascii_lowercase(),
            "`{}` is not lowercase, and the table matches a lowercased extension",
            row.extension
        );
        assert_eq!(
            row.media_type.matches('/').count(),
            1,
            "`.{}` is typed `{}`, which is not one `type/subtype`",
            row.extension,
            row.media_type
        );
        assert!(
            !row.media_type.contains(' ') && !row.media_type.contains(';'),
            "`.{}` is typed `{}` — parameters are the charset rule's business, not the dataset's",
            row.extension,
            row.media_type
        );
    }
}

// --- 4. The fence still stands ----------------------------------------------

#[test]
fn an_unnamed_extension_is_still_not_served() {
    let source = read(BUILD_SOURCE);
    let table = source
        .split_once("fun content_type_of")
        .expect("content_type_of is gone")
        .1;
    assert!(
        table.contains("_ => None,"),
        "content_type_of no longer falls through to `None`. The fence is the point: an extension \
         the table does not name is NOT SERVED rather than guessed at, because `serve_build` \
         serves a build and not a directory (fullstack-dx.md §5.10)"
    );
}

#[test]
fn no_row_types_a_body_that_needs_range_requests() {
    // `serve_build` writes a whole body and honours no `Range` header, so a
    // browser could not seek in anything it served. A row here would type a
    // response that does not work.
    for row in dataset() {
        assert!(
            !row.media_type.starts_with("audio/") && !row.media_type.starts_with("video/"),
            "`.{}` is typed `{}`, but `serve_build` sends whole bodies and honours no `Range` — a \
             browser could not seek in it. Media belongs where §5.10 already puts it",
            row.extension,
            row.media_type
        );
    }
}

#[test]
fn the_rows_that_predate_the_table_still_carry_their_media_types() {
    let rows = dataset();
    for (extension, expected) in PRE_EXISTING {
        let row = rows
            .iter()
            .find(|row| row.extension == *extension)
            .unwrap_or_else(|| panic!("the dataset dropped `.{extension}`, which was served"));
        assert_eq!(
            row.media_type, *expected,
            "`.{extension}` was served as `{expected}` before this table existed. The table may \
             add a charset to it; it may not retype it under a client that already works"
        );
    }
}

// --- 5. Provenance ----------------------------------------------------------

#[test]
fn the_dataset_and_its_script_name_the_same_upstream() {
    let tsv = read(DATASET);
    assert!(
        tsv.contains(PINNED_MIME_DB),
        "{DATASET} does not name `{PINNED_MIME_DB}` — the dataset and this gate disagree about \
         which upstream release the rows came from"
    );
    assert!(
        tsv.contains("mime-db"),
        "{DATASET} has lost its provenance header"
    );
    let script = read("scripts/regen-mime-table.py");
    assert!(
        script.contains(PINNED_MIME_DB),
        "scripts/regen-mime-table.py pins a different mime-db release than {DATASET} carries — \
         bump PINNED_MIME_DB and the dataset in one commit"
    );
    assert!(
        script.contains("MIT"),
        "scripts/regen-mime-table.py no longer records mime-db's licence"
    );
}

#[test]
fn every_row_says_where_its_media_type_came_from() {
    // The `provenance` column is the audit trail: a registry name means the row
    // is `mime-db`'s answer verbatim, and anything else must be a NAMED override
    // — a human overruling the dataset, one grep away rather than hidden in a
    // per-extension preference tweak.
    let mut overrides = Vec::new();
    for row in dataset() {
        let registry = matches!(
            row.provenance.as_str(),
            "iana" | "apache" | "nginx" | "none"
        );
        if !registry {
            assert!(
                row.provenance.starts_with("override: ")
                    && row.provenance.len() > "override: ".len(),
                "`.{}` records its media type's provenance as `{}`, which is neither a mime-db \
                 registry nor a named `override: <why>`",
                row.extension,
                row.provenance
            );
            overrides.push(row.extension.clone());
        }
    }
    assert_eq!(
        overrides,
        vec!["ico".to_string()],
        "the overrides over mime-db changed. Exactly one is expected: `.ico`, where the IANA row \
         is `image/vnd.microsoft.icon` and every browser, Apache and nginx send `image/x-icon`. \
         Each new one is a deviation from the dataset and needs its own reason"
    );
}

#[test]
fn the_generated_seam_names_the_regeneration_command() {
    let source = read(BUILD_SOURCE);
    assert!(
        source.contains(REGENERATE_COMMAND),
        "{BUILD_SOURCE}'s generated seam no longer names the regeneration command \
         ({REGENERATE_COMMAND}) — the next reader will hand-edit it"
    );
}
