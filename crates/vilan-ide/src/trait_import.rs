//! The trait a method call needs in scope, read off the diagnostic that says
//! so (B515, B535): the language server's "import this trait" quick fix and
//! its file-wide twin read it here, so the editor and any later bulk driver
//! answer one question one way.
//!
//! Two diagnostics name the import a trait method needs, and both spell it as
//! a whole statement in backticks — `` (`import std::display::Display;`) ``:
//!
//! - B515's scope check: the call RESOLVES, because some other loaded module
//!   imports the trait, but this file does not — "`to_string` is `Display`'s,
//!   and this file does not import `Display`: …". A warning in v0.44.0 and
//!   refused from v0.45.0 (R-c), so this is the fix the flip waits on.
//! - std-surface.md §5's no-method steer: nothing loaded the trait's module at
//!   all, so the call does not resolve — "i32 has no method 'to_string';
//!   import std::display::Display to use it (`import …;`)". An error.
//!
//! The statement in the message is where the analyzer FOUND the trait. The
//! fix still resolves the name through the editor's own candidate scan
//! before writing it, and prefers the message's path only when the scan
//! agrees: a path the scan cannot reach is not one to write into a file.

/// B515's warning's stable code. The language server publishes it as the
/// diagnostic's `code`; it does not change when the message is reworded, and
/// it outlives the flip from a warning to a refusal.
pub const TRAIT_NOT_IMPORTED_CODE: &str = "trait-scope/not-imported";

/// The fixed middle of B515's message: "`m` is `T`'s, and this file does not
/// import `T`: …".
const NOT_IMPORTED: &str = "'s, and this file does not import `";

/// The fixed tail of the no-method steer's clause: "; import P to use it
/// (`import P;`)".
const TO_USE_IT: &str = " to use it (`import ";

/// The import a trait-method diagnostic names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraitImport<'message> {
    /// The trait's name.
    pub name: &'message str,
    /// The module the message says to import it from — `["std", "display"]`
    /// — or `None` when the message names no module (the analyzer found no
    /// top-level module declaring the trait, and spells the bare name).
    pub module: Option<Vec<&'message str>>,
    /// Whether the diagnostic is B515's (the call resolves without the
    /// import), rather than the no-method steer's (it does not resolve).
    pub resolves: bool,
}

/// The import `message` names for a trait method, when it is one of the two
/// diagnostics above; `None` for every other message.
pub fn trait_import_of_message(message: &str) -> Option<TraitImport<'_>> {
    let resolves = message.contains(NOT_IMPORTED);
    if !resolves && !message.contains(TO_USE_IT) {
        return None;
    }
    let statement = message.split("(`import ").nth(1)?;
    let path = statement.split_once(";`)")?.0;
    let segments: Vec<&str> = path.split("::").collect();
    let well_formed = segments.iter().all(|segment| {
        !segment.is_empty()
            && segment
                .chars()
                .all(|character| character.is_alphanumeric() || character == '_')
    });
    if !well_formed {
        return None;
    }
    let (name, module) = segments.split_last()?;
    // B515 names the trait twice; the two must agree, or the message is not
    // the one this reads.
    if resolves && !message.contains(&format!("{NOT_IMPORTED}{name}`")) {
        return None;
    }
    Some(TraitImport {
        name,
        module: (!module.is_empty()).then(|| module.to_vec()),
        resolves,
    })
}

/// [`TRAIT_NOT_IMPORTED_CODE`] for B515's warning (and, from v0.45.0, its
/// refusal); `None` for every other message.
pub fn trait_import_code(message: &str) -> Option<&'static str> {
    trait_import_of_message(message)
        .filter(|import| import.resolves)
        .map(|_| TRAIT_NOT_IMPORTED_CODE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_diagnostics_name_their_import() {
        let warning = "`to_string` is `Display`'s, and this file does not import `Display`: the call \
             resolves only because another loaded module does. Import it (`import \
             std::display::Display;`) — this is an error from v0.45.0";
        let import = trait_import_of_message(warning).expect("B515's warning");
        assert_eq!(import.name, "Display");
        assert_eq!(import.module, Some(vec!["std", "display"]));
        assert!(import.resolves);
        assert_eq!(trait_import_code(warning), Some(TRAIT_NOT_IMPORTED_CODE));

        let steer = "i32 has no method 'to_string'; import std::display::Display to use it \
             (`import std::display::Display;`)";
        let import = trait_import_of_message(steer).expect("the no-method steer");
        assert_eq!(import.name, "Display");
        assert_eq!(import.module, Some(vec!["std", "display"]));
        assert!(!import.resolves);
        // The steer is an error of another kind, not B515's: no code.
        assert_eq!(trait_import_code(steer), None);
    }

    /// A trait no top-level module declares is spelled by its bare name: the
    /// name is still read, and the module is left to the editor's scan.
    #[test]
    fn a_bare_name_is_read_without_a_module() {
        let warning = "`area` is `Area`'s, and this file does not import `Area`: the call \
             resolves only because another loaded module does. Import it (`import Area;`) — \
             this is an error from v0.45.0";
        let import = trait_import_of_message(warning).expect("B515's warning");
        assert_eq!(import.name, "Area");
        assert_eq!(import.module, None);
    }

    #[test]
    fn other_messages_name_none() {
        for message in [
            "cannot find 'Json' in this scope",
            "i32 has no method 'frobnicate'",
            // A web-prelude steer also carries a statement, and is not a trait's.
            "cannot find 'Signal'; `Signal` is in the prelude of the web set — switch the \
             playground's prelude to the web set, or import it (`import std::web::ui::Signal;`)",
            // A brace list is not a statement this reads.
            "`m` is `T`'s, and this file does not import `T`: Import it (`import a::{ T };`)",
            // The statement must import the trait the message names.
            "`m` is `T`'s, and this file does not import `T`: Import it (`import a::U;`)",
        ] {
            assert_eq!(trait_import_of_message(message), None, "{message}");
        }
    }
}
