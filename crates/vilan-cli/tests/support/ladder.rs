//! Where on the full-stack ladder a server source stands
//! (`proposal/fullstack-dx.md` §5.3) — specifically, whether it writes a
//! leg's document ITSELF (rung 2, `Document::of(build)`) rather than reading
//! one from disk.
//!
//! Two gates need the answer and neither can get it from the filesystem: a
//! rung-2 page is written at boot and never lands on disk, so a check that
//! looks for an `.html` — `examples.rs::unlinked_stylesheets`, the
//! blessed-layout gate in `init.rs` — sees a leg with a stylesheet and no page
//! at all (§16.2, backlog E65). This module is the second source of truth for
//! "linked": the SERVER source, read for the call that writes the page.
//!
//! Read through the real lexer, not a substring search. A `Document::of(` in a
//! comment or inside a string literal is trivia or one `String` token to the
//! lexer, so it cannot satisfy the shape; only the call itself lexes to
//! `Document` `::` `of` `(`. The argument is then resolved to the leg it
//! describes — `require_build("client")` or `build_of("client")` written
//! inline, or a `let`/`mut` binding to one of them — because a document links
//! the stylesheet of THE BUILD IT WAS GIVEN, and a gate that credited every
//! leg's stylesheet to one `Document::of` call would be honest only while no
//! example has two browser legs.

use vilan_core::lexing::tokenize;
use vilan_core::token::Token;

/// The legs whose document `source` writes itself: one entry per
/// `Document::of(<build>)` call whose argument resolves to a leg name, sorted
/// and deduplicated. A call whose argument this cannot resolve (a build passed
/// through a parameter, a struct field) is not counted — a gate reading this
/// should say which shapes it recognizes rather than guess.
pub fn documented_legs(source: &str) -> Vec<String> {
    let (tokens, _lexing_errors) = tokenize(source);
    let tokens: Vec<Token> = tokens.into_iter().map(|(token, _span)| token).collect();
    let mut legs: Vec<String> = (0..tokens.len())
        .filter(|&index| is_document_of_call(&tokens, index))
        .filter_map(|index| {
            // The argument starts after `Document` `::` `of` `(`.
            let argument = index + 4;
            build_leg_at(&tokens, argument).or_else(|| match tokens.get(argument..argument + 2) {
                Some([Token::Ident(name), Token::Ctrl(')')]) => bound_leg(&tokens, name),
                _ => None,
            })
        })
        .collect();
    legs.sort();
    legs.dedup();
    legs
}

/// Whether `Document::of(` starts at `index`.
fn is_document_of_call(tokens: &[Token], index: usize) -> bool {
    matches!(
        tokens.get(index..index + 4),
        Some([
            Token::Ident("Document"),
            Token::Op("::"),
            Token::Ident("of"),
            Token::Ctrl('(')
        ])
    )
}

/// The leg named by a `require_build("leg")` or `build_of("leg")` call
/// starting at `index`, if one does. What follows the call (`!`, a method) is
/// the caller's business.
fn build_leg_at(tokens: &[Token], index: usize) -> Option<String> {
    match tokens.get(index..index + 4)? {
        [
            Token::Ident("require_build" | "build_of"),
            Token::Ctrl('('),
            Token::String(leg),
            Token::Ctrl(')'),
        ] => Some((*leg).to_string()),
        _ => None,
    }
}

/// The leg `name` is bound to by the first `let name = …` / `mut name = …`
/// (an optional `: Type` annotation allowed) whose initializer is a build
/// call. One level, one file: the ladder's own idiom is
/// `let build = require_build("client");` in `main`, and a gate is the wrong
/// place to reimplement name resolution.
fn bound_leg(tokens: &[Token], name: &str) -> Option<String> {
    (0..tokens.len()).find_map(|index| {
        if !matches!(tokens[index], Token::Let | Token::Mut) {
            return None;
        }
        if !matches!(tokens.get(index + 1), Some(Token::Ident(bound)) if *bound == name) {
            return None;
        }
        // Past an optional `: Type` to the `=`, never crossing the statement.
        let mut cursor = index + 2;
        loop {
            match tokens.get(cursor) {
                Some(Token::Op("=")) => break,
                Some(Token::Ctrl(';')) | None => return None,
                _ => cursor += 1,
            }
        }
        build_leg_at(tokens, cursor + 1)
    })
}
