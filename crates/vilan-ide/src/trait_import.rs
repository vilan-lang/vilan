//! The trait a method call needs in scope, read off the diagnostic that says
//! so (B515, B535): the language server's "import this trait" quick fix and
//! its file-wide twin read it here, so the editor and any later bulk driver
//! answer one question one way.
//!
//! Two diagnostics name the import a trait method needs, and both spell it as
//! a whole statement in backticks — `` (`import std::display::Display;`) ``:
//!
//! - B535's scope refusal (B515's warning until v0.45.0, R-c): the call would
//!   resolve, because some other loaded module imports the trait, but this
//!   file does not — "`to_string` is `Display`'s, and this file does not
//!   import `Display`: …". Its statement is read by the analyzer's own
//!   [`vilan_core::analyzer::trait_scope_import`], and its stable code is
//!   [`vilan_core::analyzer::TRAIT_SCOPE_CODE`].
//! - std-surface.md §5's no-method steer: nothing loaded the trait's module at
//!   all, so the call does not resolve — "i32 has no method 'to_string';
//!   import std::display::Display to use it (`import …;`)".
//!
//! The statement in the message is where the analyzer FOUND the trait. The
//! fix still resolves the name through the editor's own candidate scan
//! before writing it, and prefers the message's path only when the scan
//! agrees: a path the scan cannot reach is not one to write into a file.

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
    /// Whether the diagnostic is B535's scope refusal (the call would resolve
    /// without the import), rather than the no-method steer's (it does not
    /// resolve).
    pub resolves: bool,
}

/// The import `message` names for a trait method, when it is one of the two
/// diagnostics above; `None` for every other message.
pub fn trait_import_of_message(message: &str) -> Option<TraitImport<'_>> {
    let (statement, resolves) = match vilan_core::analyzer::trait_scope_import(message) {
        Some(statement) => (statement, true),
        None if message.contains(TO_USE_IT) => {
            let rest = message.split("(`").nth(1)?;
            (rest.split_once("`)")?.0, false)
        }
        None => return None,
    };
    let path = statement.strip_prefix("import ")?.strip_suffix(';')?;
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
    Some(TraitImport {
        name,
        module: (!module.is_empty()).then(|| module.to_vec()),
        resolves,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The analyzer's own sentence, as `check_trait_method_scope` writes it.
    const REFUSAL: &str = "`to_string` is `Display`'s, and this file does not import `Display`: \
         a trait's methods resolve only in a file that imports the trait. Import it (`import \
         std::display::Display;`)";

    #[test]
    fn both_diagnostics_name_their_import() {
        let import = trait_import_of_message(REFUSAL).expect("B535's refusal");
        assert_eq!(import.name, "Display");
        assert_eq!(import.module, Some(vec!["std", "display"]));
        assert!(import.resolves);

        let steer = "i32 has no method 'to_string'; import std::display::Display to use it \
             (`import std::display::Display;`)";
        let import = trait_import_of_message(steer).expect("the no-method steer");
        assert_eq!(import.name, "Display");
        assert_eq!(import.module, Some(vec!["std", "display"]));
        assert!(!import.resolves);
    }

    /// A trait no module path names is spelled by its bare name: the name is
    /// still read, and the module is left to the editor's scan.
    #[test]
    fn a_bare_name_is_read_without_a_module() {
        let refusal = "`area` is `Area`'s, and this file does not import `Area`: a trait's \
             methods resolve only in a file that imports the trait. Import it (`import Area;`)";
        let import = trait_import_of_message(refusal).expect("B535's refusal");
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
        ] {
            assert_eq!(trait_import_of_message(message), None, "{message}");
        }
    }
}
