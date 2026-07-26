//! `vilan init` — the project scaffold (backlog E18).
//!
//! The first minute with the toolchain should be `install → init → vilan run`,
//! not hand-writing a manifest from the docs. `init` writes one of three
//! ready-to-run projects — the shapes that actually exist:
//!
//! - [`Template::Node`] — the smallest real package (an entry, a sibling
//!   module, a `*_test.vl`), the `examples/math` shape.
//! - [`Template::Browser`] — a reactive browser app whose `index.html` sits
//!   beside the emitted bundle, the `examples/reactive-ui` shape.
//! - [`Template::Fullstack`] — ONE package with two entries, the blessed
//!   full-stack layout the examples and the book teach (D7). This scaffold is
//!   that default's delivery vehicle, so the two must not drift:
//!   `tests/init.rs::the_fullstack_template_matches_the_blessed_example_layout`
//!   compares what `init` emits against the three example manifests directly.
//!
//! **Storage.** Template files are `include_str!`-embedded from
//! `crates/vilan-cli/templates/<template>/`, so an installed binary carries its
//! scaffolds and never looks up a templates directory at runtime (the embedded
//! `std` precedent, which materializes only because the *compiler* is
//! filesystem-shaped; nothing here is). Each file's destination path is its
//! source path under that directory, and
//! `tests/init.rs::every_template_scaffolds_exactly_its_embedded_files_already_formatted`
//! compares a scaffold against the directory on disk, so a file added to one
//! and not the other fails.
//!
//! **Substitution** is one token, `{{name}}`, replaced by the derived package
//! name ([`package_name_from`]) everywhere it appears. Nothing else in a
//! template is templated; a scaffold is a file you can read.

use std::io::{IsTerminal, Write};
use std::path::Path;
use std::process::ExitCode;

use crate::paint;
use crate::report_error;

/// The scaffolds `init` can write, in prompt order: the smallest shape first,
/// the blessed full-stack shape last — and default, because that is where the
/// book tells a new project to start ("Start here", tour/hello-vilan.md).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Template {
    Node,
    Browser,
    Fullstack,
}

const TEMPLATES: [Template; 3] = [Template::Node, Template::Browser, Template::Fullstack];

/// What a bare Enter at the prompt picks.
const DEFAULT_TEMPLATE: Template = Template::Fullstack;

/// The one substituted token, replaced by the derived `[package] name`.
const NAME_TOKEN: &str = "{{name}}";

impl Template {
    /// The `--template` spelling, which is also the template directory's name.
    pub fn flag(self) -> &'static str {
        match self {
            Template::Node => "node",
            Template::Browser => "browser",
            Template::Fullstack => "fullstack",
        }
    }

    /// The one-line description the prompt and the error list carry.
    fn summary(self) -> &'static str {
        match self {
            Template::Node => "a package that runs on node — an entry, a module, and a test",
            Template::Browser => "a reactive browser app — index.html beside the bundle",
            Template::Fullstack => "one package, two entries: a browser client and a node server",
        }
    }

    /// Parse a `--template` value. Names only — the numeric answers are the
    /// prompt's affordance (see [`parse_answer`]), not part of the flag.
    fn parse(text: &str) -> Result<Template, String> {
        TEMPLATES
            .into_iter()
            .find(|template| template.flag() == text)
            .ok_or_else(|| format!("unknown template `{text}` — {}", the_templates_are()))
    }

    /// The template's files as (destination path relative to the project root,
    /// contents). The order is the order they are written and listed.
    fn files(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Template::Node => &[
                ("vilan.toml", include_str!("../templates/node/vilan.toml")),
                ("main.vl", include_str!("../templates/node/main.vl")),
                ("greeting.vl", include_str!("../templates/node/greeting.vl")),
                (
                    "greeting_test.vl",
                    include_str!("../templates/node/greeting_test.vl"),
                ),
                (".gitignore", include_str!("../templates/node/.gitignore")),
            ],
            Template::Browser => &[
                (
                    "vilan.toml",
                    include_str!("../templates/browser/vilan.toml"),
                ),
                ("app.vl", include_str!("../templates/browser/app.vl")),
                (
                    "counter.vl",
                    include_str!("../templates/browser/counter.vl"),
                ),
                (
                    "index.html",
                    include_str!("../templates/browser/index.html"),
                ),
                (
                    ".gitignore",
                    include_str!("../templates/browser/.gitignore"),
                ),
            ],
            Template::Fullstack => &[
                (
                    "vilan.toml",
                    include_str!("../templates/fullstack/vilan.toml"),
                ),
                (
                    "src/client.vl",
                    include_str!("../templates/fullstack/src/client.vl"),
                ),
                (
                    "src/server.vl",
                    include_str!("../templates/fullstack/src/server.vl"),
                ),
                (
                    "src/shared.vl",
                    include_str!("../templates/fullstack/src/shared.vl"),
                ),
                (
                    "src/app.html",
                    include_str!("../templates/fullstack/src/app.html"),
                ),
                (
                    ".gitignore",
                    include_str!("../templates/fullstack/.gitignore"),
                ),
            ],
        }
    }

    /// The commands that take the fresh project somewhere, printed after the
    /// file list. The browser scaffold has no server to run, so it builds and
    /// says where to look.
    fn next_steps(self) -> &'static [&'static str] {
        match self {
            Template::Node => &["vilan run .", "vilan test ."],
            Template::Browser => &["vilan build .", "# then open index.html"],
            Template::Fullstack => &["vilan run .", "# or `vilan run --watch .` for the dev loop"],
        }
    }
}

/// `node`, `browser`, or `fullstack` — the sanctioned spellings, in one place
/// so every message that lists them lists the same set.
fn the_templates_are() -> String {
    let names: Vec<String> = TEMPLATES
        .into_iter()
        .map(|template| format!("`{}`", template.flag()))
        .collect();
    format!(
        "the templates are {}, and {}",
        names[..names.len() - 1].join(", "),
        names[names.len() - 1]
    )
}

/// `vilan init [name] [--template <name>]`.
///
/// `name` is the directory to create; omitted, the scaffold lands in the
/// current directory. Either way nothing is ever overwritten (see [`refusal`]),
/// and no repository is created — the scaffold ships a `.gitignore`, but
/// `git init` is the user's call, not a surprise from a scaffolding command.
pub fn init(name: Option<String>, template: Option<String>) -> ExitCode {
    let template = match choose_template(template.as_deref(), std::io::stdin().is_terminal(), ask) {
        Ok(template) => template,
        Err(message) => return report_error(&message),
    };

    let working_directory = match std::env::current_dir() {
        Ok(directory) => directory,
        Err(error) => return report_error(&format!("cannot read the working directory: {error}")),
    };
    let directory = match &name {
        Some(name) => working_directory.join(name),
        None => working_directory.clone(),
    };
    // Whether this scaffolds the directory we are standing in — which
    // `vilan init .` does as much as a bare `vilan init` does, so the rule is
    // about the destination, not about whether a name was typed. (Otherwise
    // `vilan init .` would be refused for "already exists and is not empty",
    // which is true of the current directory and useless to hear.)
    let into_current_directory = vilan_core::util::canonical_path(&directory)
        == vilan_core::util::canonical_path(&working_directory);
    // The package name comes from the DIRECTORY, so `vilan init .` and
    // `vilan init x/y` agree with the plain cases. `canonical_path` resolves
    // `.`/`..` (and the on-disk spelling, when the directory already exists)
    // without needing the path to exist; the path written to is the
    // un-canonicalized one above.
    let package_name = match package_name_from_path(&directory) {
        Ok(package_name) => package_name,
        Err(message) => return report_error(&message),
    };

    if let Some(message) = refusal(&directory, !into_current_directory, template) {
        return report_error(&message);
    }

    let mut written = Vec::new();
    for (relative, contents) in template.files() {
        let path = directory.join(relative);
        if let Some(parent) = path.parent()
            && let Err(error) = std::fs::create_dir_all(parent)
        {
            return report_error(&format!("cannot create {}: {error}", parent.display()));
        }
        if let Err(error) = std::fs::write(&path, render(contents, &package_name)) {
            return report_error(&format!("cannot write {}: {error}", path.display()));
        }
        written.push(*relative);
    }

    let step_in = name.filter(|_| !into_current_directory);
    report_created(step_in.as_deref(), &package_name, template, &written);
    ExitCode::SUCCESS
}

/// Renders one template file: the single `{{name}}` token, everywhere.
fn render(contents: &str, package_name: &str) -> String {
    contents.replace(NAME_TOKEN, package_name)
}

/// Which template to write: the flag if given; otherwise a short prompt, but
/// only when stdin is a terminal — a script that pipes or redirects stdin gets
/// a clean error naming the flag instead of a process that hangs on a read.
///
/// The pieces are parameters (the terminal verdict, the reader) so the whole
/// decision is pinnable without a TTY, exactly as `paint::gate` is.
fn choose_template(
    flag: Option<&str>,
    interactive: bool,
    ask: impl FnOnce() -> Option<String>,
) -> Result<Template, String> {
    if let Some(flag) = flag {
        return Template::parse(flag);
    }
    if !interactive {
        return Err(format!(
            "no template chosen, and stdin is not a terminal — there is nothing to prompt. \
             Pass `--template <name>`: {}",
            the_templates_are()
        ));
    }
    match ask() {
        // End of input (Ctrl-D) or an unreadable stdin: nothing was chosen.
        None => Err(format!(
            "no template chosen — pass `--template <name>`: {}",
            the_templates_are()
        )),
        Some(answer) => parse_answer(answer.trim()),
    }
}

/// One prompt answer: a name, the number beside it, or empty for the default.
/// A single shot — a wrong answer is a clean error rather than a re-prompt, so
/// the command never becomes a loop a mistyped keystroke traps you in.
fn parse_answer(answer: &str) -> Result<Template, String> {
    if answer.is_empty() {
        return Ok(DEFAULT_TEMPLATE);
    }
    if let Ok(index) = answer.parse::<usize>()
        && (1..=TEMPLATES.len()).contains(&index)
    {
        return Ok(TEMPLATES[index - 1]);
    }
    Template::parse(answer)
}

/// Prints the menu and reads one line. The prompt goes to **stderr** (it is
/// interaction, and stdout stays the place results are printed); `None` means
/// end of input.
fn ask() -> Option<String> {
    let mut menu = String::from("Which template?\n");
    for (index, template) in TEMPLATES.iter().enumerate() {
        menu.push_str(&format!(
            "  {}) {:<10} {}\n",
            index + 1,
            template.flag(),
            template.summary()
        ));
    }
    eprint!(
        "{menu}Template [{}]: ",
        paint::err(paint::Style::BOLD, DEFAULT_TEMPLATE.flag())
    );
    let _ = std::io::stderr().flush();

    let mut answer = String::new();
    match std::io::stdin().read_line(&mut answer) {
        Ok(0) => {
            eprintln!(); // the user pressed Ctrl-D; end their line for them
            None
        }
        Ok(_) => Some(answer),
        Err(_) => None,
    }
}

/// Why this scaffold must not be written, if it must not be.
///
/// The rule, in full:
///
/// - `vilan init <name>` **creates** `<name>`. If it already exists and has any
///   entry at all, that is a refusal — a named directory is meant to be new.
/// - `vilan init` (and `vilan init .`, which names the same place) writes into
///   the **current** directory, which may hold unrelated files (a `README`, a
///   `.git`) but must not already be a vilan project: a `vilan.toml` here means
///   `init` has nothing to add.
/// - Either way, no file a template would write may already exist. That is the
///   invariant behind both cases — `init` creates, it never overwrites.
///
/// `elsewhere` is the first case: the destination is somewhere other than the
/// directory the command was run in.
fn refusal(directory: &Path, elsewhere: bool, template: Template) -> Option<String> {
    if elsewhere
        && std::fs::read_dir(directory)
            .ok()
            .is_some_and(|mut entries| entries.next().is_some())
    {
        return Some(format!(
            "{} already exists and is not empty — pick a name that is free, or \
             run `vilan init` inside a directory you have prepared",
            directory.display()
        ));
    }
    if !elsewhere && directory.join("vilan.toml").is_file() {
        return Some(format!(
            "{} is already a Vilan project (it has a `vilan.toml`) — \
             run `vilan init <name>` to scaffold a new one beside it",
            directory.display()
        ));
    }
    let existing: Vec<&str> = template
        .files()
        .iter()
        .map(|(relative, _)| *relative)
        .filter(|relative| directory.join(relative).exists())
        .collect();
    if existing.is_empty() {
        return None;
    }
    Some(format!(
        "the `{}` template would overwrite {} in {} — `vilan init` never \
         overwrites; move it aside or scaffold into a fresh directory",
        template.flag(),
        existing.join(", "),
        directory.display()
    ))
}

/// The `[package] name` a directory implies: its final component, with every
/// character a manifest identifier cannot carry folded to `_` and a leading
/// digit given an `_` in front (`my-app` → `my_app`, `2fast` → `_2fast`). The
/// fold is one-for-one, so the name still reads as the directory it names.
///
/// Mirrors `manifest::is_identifier`, which is what would reject the result;
/// `tests/init.rs::a_directory_name_that_is_not_an_identifier_is_sanitized`
/// builds the scaffolded project, so a drift between the two shows up as a
/// failed build rather than as a silent bad manifest.
fn package_name_from(directory_name: &str) -> Option<String> {
    if directory_name.is_empty() {
        return None;
    }
    let mut name: String = directory_name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '_'
            }
        })
        .collect();
    if name.starts_with(|character: char| character.is_ascii_digit()) {
        name.insert(0, '_');
    }
    Some(name)
}

/// [`package_name_from`] over a path's final component.
fn package_name_from_path(directory: &Path) -> Result<String, String> {
    vilan_core::util::canonical_path(directory)
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(package_name_from)
        .ok_or_else(|| {
            format!(
                "cannot derive a package name from {} — run `vilan init <name>` \
                 to name the project's directory",
                directory.display()
            )
        })
}

/// The success report: what was written, then what to run. Mirrors the shape of
/// `build`'s `Compiled … -> …` line (green verb, bold subject) on stdout.
fn report_created(name: Option<&str>, package_name: &str, template: Template, written: &[&str]) {
    let location = name.unwrap_or(".");
    println!(
        "{} {} ({} template, package `{package_name}`)",
        paint::out(paint::Style::GREEN, "Created"),
        paint::out(paint::Style::BOLD, location),
        template.flag()
    );
    for relative in written {
        println!("  {relative}");
    }
    println!();
    if let Some(name) = name {
        println!("  cd {}", shell_argument(name));
    }
    for step in template.next_steps() {
        println!("  {step}");
    }
}

/// A directory name as the `cd` line should carry it: quoted when it holds
/// whitespace, so the printed command survives being pasted back into a shell
/// (double quotes, which POSIX shells, `cmd`, and PowerShell all read the same
/// way here).
fn shell_argument(name: &str) -> String {
    if name.chars().any(char::is_whitespace) {
        format!("\"{name}\"")
    } else {
        name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_template_flag_selects_without_prompting() {
        // The flag is authoritative: no prompt, terminal or not.
        for template in TEMPLATES {
            assert_eq!(
                choose_template(Some(template.flag()), false, || panic!("must not prompt")),
                Ok(template)
            );
            assert_eq!(
                choose_template(Some(template.flag()), true, || panic!("must not prompt")),
                Ok(template)
            );
        }
    }

    #[test]
    fn an_unknown_template_flag_lists_the_templates() {
        let error = choose_template(Some("web"), true, || panic!("must not prompt"))
            .expect_err("`web` is not a template");
        assert!(error.contains("unknown template `web`"), "{error}");
        for template in TEMPLATES {
            assert!(error.contains(template.flag()), "{error}");
        }
    }

    #[test]
    fn a_non_terminal_stdin_never_prompts_and_names_the_flag() {
        // The scripts-never-hang rule: no flag + no terminal = a clean error
        // that names `--template` and every template, with the reader untouched.
        let error = choose_template(None, false, || panic!("must not prompt"))
            .expect_err("a non-terminal stdin cannot be prompted");
        assert!(error.contains("--template"), "{error}");
        assert!(error.contains("not a terminal"), "{error}");
        for template in TEMPLATES {
            assert!(error.contains(template.flag()), "{error}");
        }
    }

    #[test]
    fn the_prompt_takes_a_name_a_number_or_a_bare_enter() {
        assert_eq!(
            choose_template(None, true, || Some("browser\n".to_string())),
            Ok(Template::Browser)
        );
        assert_eq!(
            choose_template(None, true, || Some("1\n".to_string())),
            Ok(Template::Node)
        );
        assert_eq!(
            choose_template(None, true, || Some("  3  \n".to_string())),
            Ok(Template::Fullstack)
        );
        // A bare Enter is the default — the blessed full-stack shape.
        assert_eq!(
            choose_template(None, true, || Some("\n".to_string())),
            Ok(DEFAULT_TEMPLATE)
        );
        assert_eq!(DEFAULT_TEMPLATE, Template::Fullstack);
    }

    #[test]
    fn an_out_of_range_or_unknown_prompt_answer_is_an_error_not_a_reprompt() {
        for answer in ["0", "4", "-1", "fullstck"] {
            let error = choose_template(None, true, || Some(answer.to_string()))
                .expect_err("not a template");
            assert!(error.contains("unknown template"), "{answer}: {error}");
        }
        // End of input (Ctrl-D) chooses nothing.
        let error = choose_template(None, true, || None).expect_err("end of input");
        assert!(error.contains("no template chosen"), "{error}");
    }

    #[test]
    fn the_cd_line_quotes_a_name_that_needs_it() {
        // The next-steps block is meant to be pasted; a directory with a space
        // must still be one argument.
        assert_eq!(shell_argument("my-app"), "my-app");
        assert_eq!(shell_argument("My App-2"), "\"My App-2\"");
    }

    #[test]
    fn a_directory_name_becomes_a_manifest_identifier() {
        assert_eq!(package_name_from("app"), Some("app".to_string()));
        assert_eq!(package_name_from("my-app"), Some("my_app".to_string()));
        assert_eq!(package_name_from("my app.2"), Some("my_app_2".to_string()));
        // A leading digit cannot open an identifier.
        assert_eq!(package_name_from("2fast"), Some("_2fast".to_string()));
        // Non-ASCII folds rather than failing — one `_` per character.
        assert_eq!(package_name_from("caf\u{e9}"), Some("caf_".to_string()));
        assert_eq!(package_name_from(""), None);
        // The fold is one-for-one: runs are not collapsed, so the name still
        // lines up character-for-character with the directory it came from.
        assert_eq!(package_name_from("a--b"), Some("a__b".to_string()));
    }

    #[test]
    fn every_template_substitutes_the_name_token_and_leaves_nothing_behind() {
        for template in TEMPLATES {
            let mut substituted = 0;
            for (relative, contents) in template.files() {
                let rendered = render(contents, "scaffold_probe");
                assert!(
                    !rendered.contains(NAME_TOKEN),
                    "{}/{relative} still carries {NAME_TOKEN}",
                    template.flag()
                );
                substituted += usize::from(contents.contains(NAME_TOKEN));
            }
            // Every template names its package, so at least the manifest
            // carries the token — a template that stopped substituting would
            // otherwise pass the check above vacuously.
            assert!(
                substituted > 0,
                "the `{}` template substitutes nothing",
                template.flag()
            );
            assert!(
                template.files()[0].0 == "vilan.toml",
                "the manifest is written first"
            );
        }
    }

    #[test]
    fn the_prompt_menu_and_the_error_list_agree_on_the_template_set() {
        let listed = the_templates_are();
        for template in TEMPLATES {
            assert!(listed.contains(template.flag()), "{listed}");
            assert!(!template.summary().is_empty());
        }
    }
}
