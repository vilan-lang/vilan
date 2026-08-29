//! `std::style` and the styling arc (A8/A22/A23/W11, the relation axis, the
//! declaration block, typed `raw`), plus the Kolt-migration std packages
//! (`crypto`, `db`, storage, `router`).
//!
//! One subject module of the `inference` test binary; the harness it is
//! written against lives in `support.rs`.

use crate::support::*;

// --- A8: std::style — typed atomic styles, compiled ---------------------------
// The styling system riding const evaluation and the asset channel
// (proposal/ui-styling.md): builder-chain construction inside `const`, atomic
// rules with content-hashed class names, per-property last-wins merge,
// var-carried theme tokens, condition combinators.

#[test]
fn a_style_emits_atomic_rules_and_theme_vars() {
    let assets = collected_assets(
        r#"
        import std::style::{ style, space, Style };
        fun card(): Style {
            style().padding(space(4))
        }
        let _card = const card();
        fun main() {}
        main();
        "#,
    );
    assert!(
        // `*.` rather than `.`: `padding` is a family shorthand, and the
        // marker is what sorts it ahead of its edges (§0bis.4). The CLASS is
        // unchanged — the hash is over `key|declaration`, which the marker
        // never enters.
        assets.contains(&(
            "css".to_string(),
            "*.s1ufvr2{padding:var(--space-4)}".to_string()
        )),
        "{assets:?}"
    );
    assert!(
        assets.contains(&("css".to_string(), ":root{--space-4:1rem}".to_string())),
        "{assets:?}"
    );
}

#[test]
fn last_wins_within_a_chain() {
    // Two paddings, one slot: the class list carries exactly one class — the
    // later one's.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::style::{ style, space, Style };
        fun padded(): Style {
            style().padding(space(4)).padding(space(6))
        }
        fun main() {
            let card = const padded();
            let classes = card.class_list();
            print(classes.contains(" "));
            let six = const style().padding(space(6));
            print(classes == six.class_list());
        }
        main();
        "#,
        "false\ntrue\n",
    );
}

#[test]
fn add_merges_per_property_right_wins() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::style::{ style, space, Style, Color };
        fun base(): Style {
            style().padding(space(4)).background(Color::gray(50))
        }
        fun accent(): Style {
            style().padding(space(6))
        }
        fun main() {
            let merged = const base() + accent();
            let expected = const style().padding(space(6)).background(Color::gray(50));
            print(merged.class_list().len() > 0);
            print(merged.class_list() == expected.class_list());
        }
        main();
        "#,
        "true\ntrue\n",
    );
}

#[test]
fn extend_with_override_is_a_property_method_on_a_style() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::style::{ style, space, Style };
        fun main() {
            let base = const style().padding(space(4));
            let bigger = const base.padding(space(6));
            let six = const style().padding(space(6));
            print(bigger.class_list() == six.class_list());
        }
        main();
        "#,
        "true\n",
    );
}

#[test]
fn hover_emits_a_pseudo_rule() {
    let assets = collected_assets(
        r#"
        import std::style::{ style, Style, Color };
        fun s(): Style {
            style().hover(style().background(Color::gray(100)))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    assert!(
        assets
            .iter()
            .any(|(_, line)| line.contains(":hover{background-color:var(--gray-100)}")),
        "{assets:?}"
    );
}

#[test]
fn breakpoints_wrap_media_and_stack_with_pseudo() {
    let assets = collected_assets(
        r#"
        import std::style::{ style, space, Style };
        fun s(): Style {
            style().md(style().hover(style().padding(space(6))))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    assert!(
        assets
            .iter()
            // `{*.` — `padding` is a family shorthand, so its rule carries the
            // sort marker inside the media block (§0bis.4). The class and the
            // declaration are unchanged.
            .any(|(_, line)| line.starts_with("@media (min-width: 768px){*.")
                && line.contains(":hover{padding:var(--space-6)}")),
        "{assets:?}"
    );
}

/// The ancestor guard (ui-styling.md §0bis.6): `within("data-theme", "dark",
/// ..)` is the theme condition's spelling since `Style::dark`'s deletion
/// (kolt.local 014, ruled 2026-08-27). The rule is UNLAYERED — its (0,2,0)
/// beats the element's own base rule exactly when the guard matches, which is
/// the semantics `dark()` had and a layered rule could never express.
#[test]
fn within_prefixes_the_ancestor_guard() {
    let assets = collected_assets(
        r#"
        import std::style::{ style, Style, Color };
        fun s(): Style {
            style().within("data-theme", "dark", style().background(Color::gray(900)))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    assert!(
        assets
            .iter()
            .any(|(_, line)| line.starts_with("[data-theme=\"dark\"] .")
                && line.ends_with("{background-color:var(--gray-900)}")),
        "{assets:?}"
    );
}

/// within × pseudo composes on ONE slot: the ancestor guard and the
/// pseudo-class suffix land in the same rule, nested the way CSS nests them.
#[test]
fn within_stacks_over_a_pseudo_class() {
    let assets = collected_assets(
        r#"
        import std::style::{ style, Style, Color };
        fun s(): Style {
            style().within("data-theme", "dark", style().hover(style().background(Color::gray(700))))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    assert!(
        assets.iter().any(|(_, line)| {
            line.starts_with("[data-theme=\"dark\"] .")
                && line.ends_with(":hover{background-color:var(--gray-700)}")
        }),
        "{assets:?}"
    );
}

/// All three axes at once, media outermost. The composed line still starts
/// with '@', so B35's numeric media ordering keeps seeing it.
#[test]
fn a_breakpoint_wraps_within_over_a_pseudo_class() {
    let assets = collected_assets(
        r#"
        import std::style::{ style, space, Style };
        fun s(): Style {
            style().md(style().within("data-theme", "dark", style().hover(style().padding(space(6)))))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    assert!(
        assets.iter().any(|(_, line)| {
            line.starts_with("@media (min-width: 768px){[data-theme=\"dark\"] *.")
                && line.ends_with(":hover{padding:var(--space-6)}}")
        }),
        "{assets:?}"
    );
}

/// Composition has ONE spelling. The relation goes outside the pseudo — the
/// same outside-in rule that makes `md(hover(..))` legal — and the refusal
/// names the fix rather than just saying no.
#[test]
fn a_pseudo_class_cannot_wrap_within() {
    let diagnostics = failure_diagnostics(
        r#"
        import std::style::{ style, Style, Color };
        fun s(): Style {
            style().hover(style().within("data-theme", "dark", style().background(Color::gray(700))))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    assert!(
        diagnostics
            .iter()
            .any(|(message, _)| message.contains("within(.., hover(..)), not hover(within(..))")),
        "{diagnostics:#?}"
    );
}

#[test]
fn within_cannot_wrap_within() {
    let diagnostics = failure_diagnostics(
        r#"
        import std::style::{ style, Style, Color };
        fun s(): Style {
            style().within("data-theme", "dark", style().within("data-theme", "dim", style().background(Color::gray(700))))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    assert!(
        diagnostics
            .iter()
            .any(|(message, _)| message.contains("already relation-conditioned")),
        "{diagnostics:#?}"
    );
}

#[test]
fn within_cannot_wrap_a_breakpoint() {
    let diagnostics = failure_diagnostics(
        r#"
        import std::style::{ style, space, Style };
        fun s(): Style {
            style().within("data-theme", "dark", style().md(style().padding(space(6))))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    assert!(
        diagnostics
            .iter()
            .any(|(message, _)| message.contains("nest conditions as md(within(..))")),
        "{diagnostics:#?}"
    );
}

/// within's name and value ride the slot key and the selector, so they are
/// fenced at const time by the same checks `attribute` uses.
#[test]
fn within_validates_its_name_and_value() {
    let diagnostics = failure_diagnostics(
        r#"
        import std::style::{ style, Style, Color };
        fun s(): Style {
            style().within("data theme", "dark", style().background(Color::gray(700)))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    assert!(
        diagnostics
            .iter()
            .any(|(message, _)| message.contains("an attribute name cannot contain")),
        "{diagnostics:#?}"
    );
}

/// The two nesting guards that shipped with the core in 2026-07-10 and were
/// never pinned (found by the A8 tail's verification sweep).
#[test]
fn a_pseudo_class_cannot_wrap_a_pseudo_class() {
    let diagnostics = failure_diagnostics(
        r#"
        import std::style::{ style, Style, Color };
        fun s(): Style {
            style().hover(style().focus(style().background(Color::gray(700))))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    assert!(
        diagnostics
            .iter()
            .any(|(message, _)| message.contains("already pseudo-conditioned")),
        "{diagnostics:#?}"
    );
}

#[test]
fn a_breakpoint_cannot_wrap_a_breakpoint() {
    let diagnostics = failure_diagnostics(
        r#"
        import std::style::{ style, space, Style };
        fun s(): Style {
            style().md(style().lg(style().padding(space(6))))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    assert!(
        diagnostics
            .iter()
            .any(|(message, _)| message.contains("cannot wrap another media-conditioned style")),
        "{diagnostics:#?}"
    );
}

/// The slot key gained a condition GRAMMAR, not a fourth field — so every
/// class name minted before dark×pseudo (and then the relation axis) existed
/// is byte-identical after it. (The `style.vl` corpus golden is the broad
/// version of this; these two are the ones the composition code could
/// plausibly have disturbed.)
#[test]
fn composing_conditions_leaves_the_uncomposed_class_names_untouched() {
    let assets = collected_assets(
        r#"
        import std::style::{ style, Style, Color };
        fun s(): Style {
            style().background(Color::gray(50)).hover(style().background(Color::gray(100)))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    let lines: Vec<&str> = assets.iter().map(|(_, line)| line.as_str()).collect();
    assert!(
        lines.contains(&".siolu0w{background-color:var(--gray-50)}")
            && lines.contains(&".s1c7l5ao:hover{background-color:var(--gray-100)}"),
        "{assets:?}"
    );
}

#[test]
fn an_unknown_scale_step_fails_the_build() {
    let diagnostics = failure_diagnostics(
        r#"
        import std::style::{ style, space, Style };
        fun s(): Style {
            style().padding(space(37))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    assert!(
        diagnostics
            .iter()
            .any(|(message, _)| message.contains("unknown spacing step 37")),
        "{diagnostics:#?}"
    );
}

#[test]
fn an_unknown_ramp_step_fails_the_build() {
    let diagnostics = failure_diagnostics(
        r#"
        import std::style::{ style, Style, Color };
        fun s(): Style {
            style().background(Color::gray(55))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    assert!(
        diagnostics
            .iter()
            .any(|(message, _)| message.contains("unknown gray step 55")),
        "{diagnostics:#?}"
    );
}

#[test]
fn runtime_style_construction_is_rejected() {
    let diagnostics = failure_diagnostics(
        r#"
        import std::style::{ style, space, Style };
        fun main() {
            let card = style().padding(space(4));
        }
        main();
        "#,
    );
    assert!(
        diagnostics
            .iter()
            .any(|(message, _)| message.contains("compile-time-only")),
        "{diagnostics:#?}"
    );
}

#[test]
fn length_units_render_their_css() {
    let assets = collected_assets(
        r#"
        import std::style::{ style, Style, Length };
        fun s(): Style {
            style()
                .width(Length::px(37))
                .height(Length::pct(50))
                .margin(Length::auto())
                .max_width(Length::var("--w"))
                .font_size(Length::rem(1.5))
                .letter_spacing(Length::em(0.02))
                .min_height(Length::vh(100))
                .min_width(Length::vw(50))
                .max_height(Length::calc("100% - 2rem"))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    let lines: Vec<&str> = assets.iter().map(|(_, line)| line.as_str()).collect();
    for expected in [
        "{width:37px}",
        "{height:50%}",
        "{margin:auto}",
        "{max-width:var(--w)}",
        "{font-size:1.5rem}",
        "{letter-spacing:0.02em}",
        "{min-height:100vh}",
        "{min-width:50vw}",
        // `calc` wraps: the author writes the arithmetic, not the call.
        "{max-height:calc(100% - 2rem)}",
    ] {
        assert!(
            lines.iter().any(|line| line.contains(expected)),
            "missing {expected}: {lines:?}"
        );
    }
}

/// The property long tail's first slice (A8): the ≥5-site head of the demand
/// sweep, one atomic rule each. Asserted as a table — every method's exact
/// emitted declaration, so a wrong CSS property name or a mistyped keyword is
/// a named failure rather than a silent one.
#[test]
fn the_demanded_properties_emit_their_declarations() {
    let assets = collected_assets(
        r#"
        import std::style::{ style, space, Style, Color, Length, Position, UserSelect, WhiteSpace };
        fun s(): Style {
            style()
                .position(Position::Absolute)
                .inset(space(0))
                .top(Length::px(4))
                .right(Length::pct(50))
                .bottom(Length::px(8))
                .left(Length::auto())
                .flex("1 1 auto")
                .flex_shrink(0.0)
                .grid_template_columns("repeat(3, 1fr)")
                .font_family("system-ui, sans-serif")
                .text_decoration("line-through")
                .white_space(WhiteSpace::Nowrap)
                .user_select(UserSelect::Off)
                .border_color(Color::red(600))
                .box_shadow("0 1px 2px rgba(0,0,0,0.08)")
                .transform("translateY(-2px)")
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    let lines: Vec<&str> = assets.iter().map(|(_, line)| line.as_str()).collect();
    for expected in [
        "{inset:var(--space-0)}",
        "{top:4px}",
        "{right:50%}",
        "{bottom:8px}",
        "{left:auto}",
        "{flex:1 1 auto}",
        "{flex-shrink:0}",
        "{grid-template-columns:repeat(3, 1fr)}",
        "{font-family:system-ui, sans-serif}",
        "{text-decoration:line-through}",
        "{white-space:nowrap}",
        // `Off`, not `None`: the `Display::Hidden` naming rule.
        "{user-select:none}",
        "{border-color:var(--red-600)}",
        "{box-shadow:0 1px 2px rgba(0,0,0,0.08)}",
        "{transform:translateY(-2px)}",
    ] {
        assert!(
            lines.iter().any(|line| line.contains(expected)),
            "missing {expected}: {lines:?}"
        );
    }
}

/// Every variant of the two new keyword enums maps to its CSS keyword — the
/// ordering-sensitive, exhaustive half a happy-path pin misses.
#[test]
fn the_new_keyword_enums_cover_every_variant() {
    let assets = collected_assets(
        r#"
        import std::style::{ style, Style, UserSelect, WhiteSpace };
        fun space_variants(): Style {
            style()
                .white_space(WhiteSpace::Normal)
                .hover(style().white_space(WhiteSpace::Nowrap))
                .focus(style().white_space(WhiteSpace::Pre))
                .active(style().white_space(WhiteSpace::PreWrap))
                .disabled(style().white_space(WhiteSpace::PreLine))
        }
        fun select_variants(): Style {
            style()
                .user_select(UserSelect::Auto)
                .hover(style().user_select(UserSelect::Text))
                .focus(style().user_select(UserSelect::All))
                .active(style().user_select(UserSelect::Off))
        }
        let _a = const space_variants();
        let _b = const select_variants();
        fun main() {}
        main();
        "#,
    );
    let lines: Vec<&str> = assets.iter().map(|(_, line)| line.as_str()).collect();
    for expected in [
        "{white-space:normal}",
        "{white-space:nowrap}",
        "{white-space:pre}",
        "{white-space:pre-wrap}",
        "{white-space:pre-line}",
        "{user-select:auto}",
        "{user-select:text}",
        "{user-select:all}",
        "{user-select:none}",
    ] {
        assert!(
            lines.iter().any(|line| line.contains(expected)),
            "missing {expected}: {lines:?}"
        );
    }
}

/// The reason `border_color` exists as its own method: `border(width, color)`
/// fills ONE slot, so recolouring under `:hover` used to mean restating the
/// width. Two slots, two classes, and the pseudo-class rule wins by
/// specificity — which is what four of the five real `border-color` uses in
/// the demand sweep were hand-rolling through `raw`.
#[test]
fn border_color_is_its_own_slot_so_a_hover_can_recolour_a_border() {
    let assets = collected_assets(
        r#"
        import std::style::{ style, Style, Color, Length };
        fun s(): Style {
            style()
                .border(Length::px(1), Color::gray(300))
                .hover(style().border_color(Color::blue(600)))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    let lines: Vec<&str> = assets.iter().map(|(_, line)| line.as_str()).collect();
    assert!(
        lines
            .iter()
            .any(|line| line.contains("{border:1px solid var(--gray-300)}")),
        "{lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains(":hover{border-color:var(--blue-600)}")),
        "{lines:?}"
    );
}

/// The property tail's SECOND slice (A8 §3b): the value types. Alpha is the
/// single biggest driver of the escape hatch (58 sites), and it arrives two
/// ways — a literal `rgba(..)` and `.alpha(..)` derived from an existing
/// colour. The derived form is the one with a claim to prove: it renders the
/// RELATIVE colour form so a ramp token stays a `var()`, which is what keeps
/// a translucent themed colour themeable.
#[test]
fn alpha_colours_render_their_css() {
    let assets = collected_assets(
        r##"
        import std::style::{ style, Style, Color, Length };
        fun s(): Style {
            style()
                .background(Color::rgba(27, 6, 13, 0.9))
                .color(Color::gray(900).alpha(0.08))
                .border(Length::px(1), Color::hex("#EB682E").alpha(0.6))
                .border_color(Color::rgba(235, 104, 46, 1.0))
        }
        let _s = const s();
        fun main() {}
        main();
        "##,
    );
    let lines: Vec<&str> = assets.iter().map(|(_, line)| line.as_str()).collect();
    for expected in [
        "{background-color:rgba(27, 6, 13, 0.9)}",
        // The token survives: `var(--gray-900)`, not `#111827`.
        "{color:rgb(from var(--gray-900) r g b / 0.08)}",
        "{border:1px solid rgb(from #EB682E r g b / 0.6)}",
        "{border-color:rgba(235, 104, 46, 1)}",
        // …and the token's `:root` line still rides out with it, which is
        // what makes the translucent colour re-theme like the opaque one.
        ":root{--gray-900:#111827}",
    ] {
        assert!(
            lines.iter().any(|line| line.contains(expected)),
            "missing {expected}: {lines:?}"
        );
    }
}

/// A gradient is `background-image`, not a `Color` — a separate slot from
/// `background`, so a style may set both. Both constructors are pinned:
/// radial is 12 of the 16 real gradient sites, linear 2, and they ship as a
/// family (§0bis.3).
#[test]
fn gradients_paint_the_background_image_slot() {
    let assets = collected_assets(
        r##"
        import std::style::{ style, Style, Color, Gradient, RadialExtent };
        fun linear(): Style {
            style()
                .background(Color::gray(50))
                .background_gradient(
                    Gradient::linear(90.0)
                        .stop(Color::hex("#B23056"), 0.0)
                        .stop(Color::blue(600), 100.0),
                )
        }
        fun radial(): Style {
            style().background_gradient(
                Gradient::radial(RadialExtent::ClosestSide)
                    .stop(Color::rgba(178, 48, 86, 0.5), 0.0)
                    .stop(Color::transparent(), 100.0),
            )
        }
        fun corners(): Style {
            style()
                .background_gradient(
                    Gradient::radial(RadialExtent::ClosestCorner)
                        .stop(Color::black(), 0.0)
                        .stop(Color::white(), 100.0),
                )
                .hover(style().background_gradient(
                    Gradient::radial(RadialExtent::FarthestSide)
                        .stop(Color::black(), 0.0)
                        .stop(Color::white(), 100.0),
                ))
                .focus(style().background_gradient(
                    Gradient::radial(RadialExtent::FarthestCorner)
                        .stop(Color::black(), 0.0)
                        .stop(Color::white(), 100.0),
                ))
        }
        let _a = const linear();
        let _b = const radial();
        let _c = const corners();
        fun main() {}
        main();
        "##,
    );
    let lines: Vec<&str> = assets.iter().map(|(_, line)| line.as_str()).collect();
    for expected in [
        "{background-image:linear-gradient(90deg, #B23056 0%, var(--blue-600) 100%)}",
        "{background-image:radial-gradient(closest-side, rgba(178, 48, 86, 0.5) 0%, transparent 100%)}",
        "{background-image:radial-gradient(closest-corner, #000000 0%, #ffffff 100%)}",
        "{background-image:radial-gradient(farthest-side, #000000 0%, #ffffff 100%)}",
        "{background-image:radial-gradient(farthest-corner, #000000 0%, #ffffff 100%)}",
        // The image slot is not the colour slot: both survive on one style.
        "{background-color:var(--gray-50)}",
        // A stop's token carries its `:root` line out to the emitter.
        ":root{--blue-600:#2563eb}",
    ] {
        assert!(
            lines.iter().any(|line| line.contains(expected)),
            "missing {expected}: {lines:?}"
        );
    }
}

/// The border family: four edges plus `none`, `solid` baked in because the
/// demand sweep found zero non-`solid` borders. Table-shaped, so a wrong CSS
/// property name is a named failure.
#[test]
fn the_border_family_emits_one_declaration_per_edge() {
    let assets = collected_assets(
        r#"
        import std::style::{ style, Style, Color, Length };
        fun s(): Style {
            style()
                .border_top(Length::px(1), Color::gray(300))
                .border_right(Length::px(2), Color::blue(600))
                .border_bottom(Length::rem(0.5), Color::red(500))
                .border_left(Length::px(3), Color::green(700))
        }
        fun cleared(): Style {
            style().border(Length::px(1), Color::gray(300)).border_none()
        }
        let _s = const s();
        let _c = const cleared();
        fun main() {}
        main();
        "#,
    );
    let lines: Vec<&str> = assets.iter().map(|(_, line)| line.as_str()).collect();
    for expected in [
        "{border-top:1px solid var(--gray-300)}",
        "{border-right:2px solid var(--blue-600)}",
        "{border-bottom:0.5rem solid var(--red-500)}",
        "{border-left:3px solid var(--green-700)}",
        "{border:none}",
    ] {
        assert!(
            lines.iter().any(|line| line.contains(expected)),
            "missing {expected}: {lines:?}"
        );
    }
}

/// Why `border_none()` is a method and not a `BorderStyle::None`: it fills
/// the SAME slot the shorthand does, so clearing a border is the ordinary
/// last-wins override and the cleared style carries ONE border class — not a
/// second rule racing the first through the cascade.
#[test]
fn border_none_replaces_the_border_slot_rather_than_racing_it() {
    let program = r#"
        import std::print;
        import std::style::{ style, Style, Color, Length };
        fun cleared(): Style {
            style().border(Length::px(1), Color::gray(300)).border_none()
        }
        fun main() {
            let c = const cleared();
            print(c.class_list());
        }
        main();
        "#;
    let output = compile_and_run(program).expect("a clean run");
    assert_eq!(
        output.trim().split(' ').count(),
        1,
        "the cleared style should carry one border class, got {output:?}"
    );
}

/// The escape-hatch conversion this slice is evidence for: the typed method
/// and the `raw` call it replaces are the SAME rule, so unwinding a real
/// `raw` site changes zero bytes of stylesheet. Asserted on the CLASS LISTS
/// — equal names means equal slot keys AND equal declarations, which a
/// substring count over the emitted lines cannot tell apart (a diverging
/// method still leaves the `raw` site's own line in the sheet).
#[test]
fn the_typed_methods_mint_the_rules_their_raw_sites_did() {
    let source = r#"
        import std::print;
        import std::style::{ style, Style, Length };
        fun escaped(): Style {
            style()
                .raw("border", "none")
                .raw("margin-left", "auto")
                .raw("padding-right", "16px")
        }
        fun typed(): Style {
            style()
                .border_none()
                .margin_left(Length::auto())
                .padding_right(Length::px(16))
        }
        fun main() {
            let e = const escaped();
            let t = const typed();
            print(e.class_list());
            print(t.class_list());
        }
        main();
        "#;
    let output = compile_and_run(source).expect("a clean run");
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines.len(), 2, "{output:?}");
    assert_eq!(lines[0].split(' ').count(), 3, "{output:?}");
    assert_eq!(lines[0], lines[1], "the typed chain changed the classes");

    // …and the shared rule is in the sheet once, not twice.
    let assembled = assembled_assets(source);
    let css = assembled.get("css").expect("css");
    for expected in [
        "{border:none}",
        "{margin-left:auto}",
        "{padding-right:16px}",
    ] {
        assert_eq!(
            css.matches(expected).count(),
            1,
            "{expected} should be minted once by both spellings: {css}"
        );
    }
}

/// The eight box edges, the hole the surface had: `padding`, `padding_x` and
/// `padding_y` shipped, and nothing could set one edge.
#[test]
fn the_box_edges_emit_their_longhands() {
    let assets = collected_assets(
        r#"
        import std::style::{ style, space, Style, Length };
        fun s(): Style {
            style()
                .padding_top(space(2))
                .padding_right(Length::px(16))
                .padding_bottom(space(3))
                .padding_left(Length::px(8))
                .margin_top(Length::px(96))
                .margin_right(Length::px(1))
                .margin_bottom(space(4))
                .margin_left(Length::auto())
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    let lines: Vec<&str> = assets.iter().map(|(_, line)| line.as_str()).collect();
    for expected in [
        "{padding-top:var(--space-2)}",
        "{padding-right:16px}",
        "{padding-bottom:var(--space-3)}",
        "{padding-left:8px}",
        "{margin-top:96px}",
        "{margin-right:1px}",
        "{margin-bottom:var(--space-4)}",
        // The flex-push idiom, five of the nine single-edge sites in the sweep.
        "{margin-left:auto}",
    ] {
        assert!(
            lines.iter().any(|line| line.contains(expected)),
            "missing {expected}: {lines:?}"
        );
    }
}

/// `Display` could not name two legal values of its own property. Every
/// variant, so the ordering-sensitive exhaustive half is covered rather than
/// the two new arms alone.
#[test]
fn the_display_enum_covers_every_variant() {
    let assets = collected_assets(
        r#"
        import std::style::{ style, Style, Display };
        fun s(): Style {
            style()
                .display(Display::Flex)
                .hover(style().display(Display::Grid))
                .focus(style().display(Display::Block))
                .active(style().display(Display::Inline))
                .disabled(style().display(Display::InlineBlock))
                .first(style().display(Display::InlineFlex))
                .last(style().display(Display::InlineGrid))
                .within("data-theme", "dark", style().display(Display::Hidden))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    let lines: Vec<&str> = assets.iter().map(|(_, line)| line.as_str()).collect();
    for expected in [
        "{display:flex}",
        "{display:grid}",
        "{display:block}",
        "{display:inline}",
        "{display:inline-block}",
        "{display:inline-flex}",
        "{display:inline-grid}",
        "{display:none}",
    ] {
        assert!(
            lines.iter().any(|line| line.contains(expected)),
            "missing {expected}: {lines:?}"
        );
    }
}

/// The value types validate at const time, the way `space` and the ramps do:
/// a bad channel, a bad alpha or a one-stop gradient is a build error naming
/// the value, not a silently invalid declaration in the stylesheet.
#[test]
fn out_of_range_colour_values_fail_the_build() {
    for (source, expected) in [
        (
            r#"
            import std::style::{ style, Style, Color };
            fun s(): Style { style().background(Color::rgba(300, 0, 0, 0.5)) }
            let _s = const s();
            fun main() {}
            main();
            "#,
            "red channel 300 is outside 0-255",
        ),
        (
            r#"
            import std::style::{ style, Style, Color };
            fun s(): Style { style().background(Color::rgba(0, 0, 0, 1.5)) }
            let _s = const s();
            fun main() {}
            main();
            "#,
            "alpha 1.5 is outside 0.0-1.0",
        ),
        (
            r#"
            import std::style::{ style, Style, Color };
            fun s(): Style { style().color(Color::gray(500).alpha(-0.2)) }
            let _s = const s();
            fun main() {}
            main();
            "#,
            "alpha -0.2 is outside 0.0-1.0",
        ),
        (
            r#"
            import std::style::{ style, Style, Color, Gradient };
            fun s(): Style {
                style().background_gradient(Gradient::linear(90.0).stop(Color::black(), 0.0))
            }
            let _s = const s();
            fun main() {}
            main();
            "#,
            "a gradient needs at least two stops",
        ),
    ] {
        let diagnostics = failure_diagnostics(source);
        assert!(
            diagnostics
                .iter()
                .any(|(message, _)| message.contains(expected)),
            "missing {expected}: {diagnostics:#?}"
        );
    }
}

#[test]
fn identical_rules_deduplicate_across_styles() {
    let assembled = assembled_assets(
        r#"
        import std::style::{ style, space, Style };
        fun a(): Style {
            style().padding(space(4))
        }
        fun b(): Style {
            style().padding(space(4))
        }
        let _a = const a();
        let _b = const b();
        fun main() {}
        main();
        "#,
    );
    let css = assembled.get("css").expect("css");
    assert_eq!(
        css.matches(".s1ufvr2{padding:var(--space-4)}").count(),
        1,
        "{css}"
    );
}

// --- A22: same-family override order (ui-styling.md §0bis.4) ------------------
// An atomic shorthand rule and an atomic longhand rule of one family carry
// EQUAL specificity, so the cascade fell through to stylesheet order — the
// lexical sort over content-hashed class names, i.e. arbitrary. Two rules fix
// it, and they meet exactly: a shorthand set later DROPS every slot it covers
// under the same condition, and a shorthand's rule renders `*.sX{..}` so the
// existing lexical sort puts it ahead of its family's longhands. So two slots
// of one family survive together only when the longhand came last — which is
// the case where the longhand should win — and a family resolves by AUTHORING
// order, never by the hash.
//
// The pins read the rendered CSS and the class LIST; none of them reads the
// order of classes within a list, which is not what decides the cascade.

/// The assembled stylesheet's rule for one declaration: the whole line, so a
/// pin can read the selector, and its byte offset, so a pin can read the
/// cascade order.
fn rule_for<'a>(css: &'a str, declaration: &str) -> (&'a str, usize) {
    let needle = format!("{{{declaration}}}");
    let offset = css
        .find(&needle)
        .unwrap_or_else(|| panic!("no rule for {declaration} in:\n{css}"));
    let line_start = css[..offset].rfind('\n').map_or(0, |index| index + 1);
    let line_end = css[offset..]
        .find('\n')
        .map_or(css.len(), |index| offset + index);
    (&css[line_start..line_end], offset)
}

fn style_css(source: &str) -> String {
    assembled_assets(source)
        .get("css")
        .expect("a css asset")
        .clone()
}

#[test]
fn a_longhand_after_a_shorthand_wins_by_emission_order() {
    // THE RECORD'S REPRO (§0bis.3's hazard note): `padding(4).padding_top(0)`
    // must compute `padding-top: 0`. Both rules are (0,1,0) and both classes
    // are on the element, so the guarantee is that the shorthand's rule sits
    // EARLIER in the stylesheet — which the `*` marker makes true by ASCII,
    // '*' (0x2A) sorting before '.' (0x2E).
    let css = style_css(
        r#"
        import std::style::{ style, space, Style };
        fun s(): Style {
            style().padding(space(4)).padding_top(space(0))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    let (shorthand, shorthand_at) = rule_for(&css, "padding:var(--space-4)");
    let (longhand, longhand_at) = rule_for(&css, "padding-top:var(--space-0)");
    assert!(
        shorthand.starts_with("*."),
        "the shorthand rule must carry the sort marker: {shorthand}"
    );
    assert!(
        longhand.starts_with('.'),
        "a longhand rule must NOT carry the marker: {longhand}"
    );
    assert!(
        shorthand_at < longhand_at,
        "padding must precede padding-top so the edge wins:\n{css}"
    );
}

#[test]
fn a_longhand_after_a_shorthand_keeps_both_classes() {
    // The other half of the repro: the shorthand is still live for the three
    // edges it still owns, so its class stays on the element.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::style::{ style, space, Style };
        fun main() {
            let boxed = const style().padding(space(4)).padding_top(space(0));
            print(boxed.class_list().split(" ").len());
        }
        main();
        "#,
        "2\n",
    );
}

#[test]
fn a_shorthand_after_a_longhand_clears_the_whole_family() {
    // The reverse order: the later shorthand resets the whole box, so every
    // edge it covers leaves the style. One class, and the same class a bare
    // `padding(4)` mints — the family is gone, not merely outranked.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::style::{ style, space, Style };
        fun main() {
            let boxed = const style().padding_top(space(0)).padding(space(4));
            let plain = const style().padding(space(4));
            print(boxed.class_list() == plain.class_list());
        }
        main();
        "#,
        "true\n",
    );
}

#[test]
fn the_axis_methods_resolve_against_the_shorthand_too() {
    // `padding` + `padding_x` is the instance of the hazard that predates the
    // per-edge methods entirely (§0bis.3). The axis writes the same two edge
    // slots the edge methods do, so it resolves identically.
    let css = style_css(
        r#"
        import std::style::{ style, space, Style };
        fun s(): Style {
            style().padding(space(4)).padding_x(space(6))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    let (_, shorthand_at) = rule_for(&css, "padding:var(--space-4)");
    let (_, left_at) = rule_for(&css, "padding-left:var(--space-6)");
    let (_, right_at) = rule_for(&css, "padding-right:var(--space-6)");
    assert!(
        shorthand_at < left_at && shorthand_at < right_at,
        "the axis must outrank the box it narrows:\n{css}"
    );
}

#[test]
fn border_and_border_colour_resolve_by_authoring_order() {
    // The second live family. `border` covers `border-color`, so the colour
    // set after it wins on order, and a `border` set after a colour clears it.
    let css = style_css(
        r#"
        import std::style::{ style, Style, Color, Length };
        fun s(): Style {
            style().border(Length::px(1), Color::gray(300)).border_color(Color::blue(600))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    let (shorthand, shorthand_at) = rule_for(&css, "border:1px solid var(--gray-300)");
    let (_, colour_at) = rule_for(&css, "border-color:var(--blue-600)");
    assert!(
        shorthand.starts_with("*."),
        "the border shorthand must carry the marker: {shorthand}"
    );
    assert!(
        shorthand_at < colour_at,
        "border must precede border-color:\n{css}"
    );

    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::style::{ style, Style, Color, Length };
        fun main() {
            let framed = const style()
                .border_color(Color::blue(600))
                .border(Length::px(1), Color::gray(300));
            let plain = const style().border(Length::px(1), Color::gray(300));
            print(framed.class_list() == plain.class_list());
        }
        main();
        "#,
        "true\n",
    );
}

#[test]
fn a_merge_resolves_a_family_the_way_a_chain_does() {
    // The live `+` instance from the website (`df_node + border_color`). `+`
    // is runtime-legal and so cannot emit, which is why the fix is a drop plus
    // an order and not a shorthand SPLIT: the drop is a map removal.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::style::{ style, Style, Color, Length };
        fun main() {
            let base = const style().border(Length::px(1), Color::gray(300));
            // Right side narrows the family: both survive, the colour outranks.
            let lit = const base + style().border_color(Color::blue(600));
            print(lit.class_list().split(" ").len());
            // Right side resets the family: the colour goes.
            let reset = const style().border_color(Color::blue(600)) + base;
            print(reset.class_list() == base.class_list());
        }
        main();
        "#,
        "2\ntrue\n",
    );
}

#[test]
fn a_condition_never_clears_the_base_family() {
    // Cross-condition: the drop is keyed on media AND condition, so a themed
    // or hover variant of one family leaves the base slots alone — and the
    // marker changes no specificity, so the base/condition cascade is exactly
    // what it was (a within rule is (0,2,0) over a base rule's (0,1,0),
    // either way round).
    let css = style_css(
        r#"
        import std::style::{ style, space, Style };
        fun shorthand_under_within(): Style {
            style().padding_top(space(0)).within("data-theme", "dark", style().padding(space(4)))
        }
        fun longhand_under_hover(): Style {
            style().padding(space(6)).hover(style().padding_top(space(2)))
        }
        let _a = const shorthand_under_within();
        let _b = const longhand_under_hover();
        fun main() {}
        main();
        "#,
    );
    // The themed shorthand did not clear the base edge — the base edge rule
    // is still there, unmarked and outside the ancestor-guard band.
    let (base_edge, _) = rule_for(&css, "padding-top:var(--space-0)");
    assert!(
        base_edge.starts_with('.') && !base_edge.contains("data-theme"),
        "the base edge must survive a themed shorthand: {base_edge}"
    );
    // The themed shorthand keeps the '[' band (B35 / §0bis.6) and gains the
    // marker inside it. (Its inner chain's own base rule is also in the
    // stylesheet — the recorded over-approximation — so the pin names the
    // selector rather than taking the first match.)
    assert!(
        css.lines()
            .any(|line| line.starts_with(r#"[data-theme="dark"] *."#)
                && line.ends_with("{padding:var(--space-4)}")),
        "no marked themed shorthand:\n{css}"
    );
    // The base shorthand is neither cleared by nor clears the hover edge.
    let (base_shorthand, _) = rule_for(&css, "padding:var(--space-6)");
    assert!(
        base_shorthand.starts_with("*."),
        "the base shorthand survives a hover edge: {base_shorthand}"
    );
    assert!(
        css.lines()
            .any(|line| line.ends_with(":hover{padding-top:var(--space-2)}")),
        "no hover edge rule:\n{css}"
    );

    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::style::{ style, space, Style };
        fun main() {
            let themed = const style().padding_top(space(0)).within("data-theme", "dark", style().padding(space(4)));
            print(themed.class_list().split(" ").len());
        }
        main();
        "#,
        "2\n",
    );
}

#[test]
fn the_marker_keeps_the_media_bands_intact() {
    // B35, unchanged: the media sort reads a `@media (min-width: ` prefix the
    // marker never appears in, and the marker orders rules INSIDE a block.
    let css = style_css(
        r#"
        import std::style::{ style, space, Style };
        fun s(): Style {
            style().sm(style().padding(space(2))).lg(style().padding(space(3)))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    // Each breakpoint's inner chain also emits its own base rule, so the pin
    // names the media selector rather than taking the first match.
    let small_at = css
        .find("@media (min-width: 640px){*.")
        .expect("a marked sm block");
    let large_at = css
        .find("@media (min-width: 1024px){*.")
        .expect("a marked lg block");
    assert!(
        small_at < large_at,
        "the ascending min-width sort must survive the marker:\n{css}"
    );
    assert!(
        css.contains("{padding:var(--space-2)}") && css.contains("{padding:var(--space-3)}"),
        "both breakpoint declarations must be present:\n{css}"
    );
}

#[test]
fn raw_belongs_to_its_property_s_family() {
    // `raw` writes slots like any other method, so its property places it —
    // the family relation is a fact about CSS, not about which method wrote
    // the slot. (`border_none()` IS `raw("border", "none")`, and both live
    // instances of this hazard had `raw` on one side.) This is the website's
    // `status_line`: a zeroed box with one edge pushed to `auto`.
    let css = style_css(
        r#"
        import std::style::{ style, space, Style };
        fun s(): Style {
            style().margin(space(0)).raw("margin-left", "auto")
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    let (shorthand, shorthand_at) = rule_for(&css, "margin:var(--space-0)");
    let (_, edge_at) = rule_for(&css, "margin-left:auto");
    assert!(
        shorthand.starts_with("*."),
        "a raw-written shorthand is marked too: {shorthand}"
    );
    assert!(
        shorthand_at < edge_at,
        "margin must precede margin-left:\n{css}"
    );

    // And a raw-written shorthand clears the family, exactly as the typed one
    // does — which is what `border_none()` has always relied on.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::style::{ style, space, Style };
        fun main() {
            let pushed = const style().raw("margin-left", "auto").raw("margin", "0");
            print(pushed.class_list().contains(" "));
        }
        main();
        "#,
        "false\n",
    );
}

#[test]
fn every_family_in_the_table_is_marked_and_its_longhands_are_not() {
    // The six rows, each earned by a real site in the demand sweep. `inset`
    // over the placement methods, `background` over the typed colour and
    // gradient slots and `flex` over `flex-shrink` are the three the record's
    // note had missed.
    let css = style_css(
        r##"
        import std::style::{ style, space, Style, Color, Length };
        fun boxes(): Style {
            style().padding(space(4)).margin(space(6)).inset(space(0))
        }
        fun edges(): Style {
            style().padding_top(space(1)).margin_left(Length::auto()).top(Length::px(3))
        }
        fun rest(): Style {
            style().raw("flex", "1 1 auto").raw("background", "#ffffff")
        }
        fun rest_longhands(): Style {
            style().raw("flex-shrink", "0").background(Color::gray(50))
        }
        let _a = const boxes();
        let _b = const edges();
        let _c = const rest();
        let _d = const rest_longhands();
        fun main() {}
        main();
        "##,
    );
    for declaration in [
        "padding:var(--space-4)",
        "margin:var(--space-6)",
        "inset:var(--space-0)",
        "flex:1 1 auto",
        "background:#ffffff",
    ] {
        let (rule, _) = rule_for(&css, declaration);
        assert!(
            rule.starts_with("*."),
            "{declaration} must be marked: {rule}"
        );
    }
    for declaration in [
        "padding-top:var(--space-1)",
        "margin-left:auto",
        "top:3px",
        "flex-shrink:0",
        "background-color:var(--gray-50)",
    ] {
        let (rule, _) = rule_for(&css, declaration);
        assert!(
            rule.starts_with('.'),
            "{declaration} must NOT be marked: {rule}"
        );
    }
}

#[test]
fn a_border_edge_survives_the_family_it_narrows() {
    // `border` covers `border-top`, so the edge set after it wins on order and
    // a `border` set after it clears it — the same pair of rules one level in.
    let css = style_css(
        r#"
        import std::style::{ style, Style, Color, Length };
        fun s(): Style {
            style().border(Length::px(1), Color::gray(300)).border_top(Length::px(2), Color::blue(600))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    let (_, box_at) = rule_for(&css, "border:1px solid var(--gray-300)");
    let (edge, edge_at) = rule_for(&css, "border-top:2px solid var(--blue-600)");
    assert!(
        edge.starts_with('.'),
        "an edge is a longhand of the box, so it is not marked: {edge}"
    );
    assert!(box_at < edge_at, "border must precede border-top:\n{css}");

    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::style::{ style, Style, Color, Length };
        fun main() {
            let framed = const style()
                .border_top(Length::px(2), Color::blue(600))
                .border(Length::px(1), Color::gray(300));
            print(framed.class_list().contains(" "));
        }
        main();
        "#,
        "false\n",
    );
}

// --- A23: the website's measured remainder (ui-styling.md §0bis.5) -----------
// The third value-type slice. Its charter's headline — 36 `raw("background")`
// sites — turned out to be a CONVERSION backlog rather than a supply hole: the
// value types §0bis.3 shipped already hold 33 of them. So the pins here split
// in two. The first group asserts the five surfaces this slice DID add; the
// second is the evidence for the reversal, pinning that each shape the website
// actually writes is expressible with what already exists — which is what the
// next cycle's conversion will lean on.

/// The five additions, table-shaped like every property pin here: the exact
/// declaration each emits, so a wrong CSS property name is a named failure.
#[test]
fn the_a23_value_surfaces_emit_their_declarations() {
    let assets = collected_assets(
        r#"
        import std::style::{ style, Style, Length };
        fun s(): Style {
            style()
                .inset(Length::zero())
                .min_width(Length::zero())
                .left(Length::raw("clamp(120px, 30%, 185px)"))
                .width(Length::raw("min(400px, 70%)"))
                .line_height_length(Length::px(24))
                .background_image("url(data:image/svg+xml;base64,PHN2Zz48L3N2Zz4=)")
                .background_size("120px 120px")
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    let lines: Vec<&str> = assets.iter().map(|(_, line)| line.as_str()).collect();
    for expected in [
        // Bare `0`, not `0px` and not `var(--space-0)`: the spelling the
        // `inset:0` and `min-width:0` idioms are written in.
        "{inset:0}",
        "{min-width:0}",
        // `raw` is verbatim — no `calc(..)` wrapper, unlike `calc`.
        "{left:clamp(120px, 30%, 185px)}",
        "{width:min(400px, 70%)}",
        "{line-height:24px}",
        "{background-image:url(data:image/svg+xml;base64,PHN2Zz48L3N2Zz4=)}",
        "{background-size:120px 120px}",
    ] {
        assert!(
            lines.iter().any(|line| line.contains(expected)),
            "missing {expected}: {lines:?}"
        );
    }
}

/// `calc` is now sugar over `raw` and must stay byte-identical: it wraps, `raw`
/// does not, and the same text through both differs by exactly the wrapper.
#[test]
fn calc_still_wraps_and_raw_does_not() {
    let assets = collected_assets(
        r#"
        import std::style::{ style, Style, Length };
        fun s(): Style {
            style()
                .width(Length::calc("100% - 2rem"))
                .height(Length::raw("calc(100% - 2rem)"))
                .max_width(Length::raw("100% - 2rem"))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    let lines: Vec<&str> = assets.iter().map(|(_, line)| line.as_str()).collect();
    for expected in [
        "{width:calc(100% - 2rem)}",
        // Spelling the wrapper by hand through `raw` reaches the same value.
        "{height:calc(100% - 2rem)}",
        // And `raw` adds nothing of its own.
        "{max-width:100% - 2rem}",
    ] {
        assert!(
            lines.iter().any(|line| line.contains(expected)),
            "missing {expected}: {lines:?}"
        );
    }
}

/// `line_height_length` is a SIBLING on the same slot, not a second property:
/// the two forms override each other last-wins and the style carries one class,
/// where two slots would have raced in the cascade at equal specificity.
#[test]
fn line_height_length_shares_the_line_height_slot() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::style::{ style, Style, Length };
        fun main() {
            let unitless_last = const style().line_height_length(Length::px(24)).line_height(1.5);
            let length_last = const style().line_height(1.5).line_height_length(Length::px(24));
            print(unitless_last.class_list().contains(" "));
            print(length_last.class_list().contains(" "));
        }
        main();
        "#,
        "false\nfalse\n",
    );
}

/// `background_image` writes the slot `background_gradient` writes — the
/// `border`/`border_none` shape. One slot, so the later call REPLACES the
/// earlier one instead of emitting a second rule to race it.
#[test]
fn background_image_and_background_gradient_share_one_slot() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::style::{ style, Style, Color, Gradient, RadialExtent };
        fun main() {
            let painted = const style()
                .background_gradient(
                    Gradient::radial(RadialExtent::ClosestSide)
                        .stop(Color::rgba(235, 104, 46, 0.4), 0.0)
                        .stop(Color::transparent(), 100.0),
                )
                .background_image("url(tile.png)");
            print(painted.class_list().contains(" "));
        }
        main();
        "#,
        "false\n",
    );
}

/// The new slots take their place in the `background` family: both are
/// LONGHANDS under the `background` shorthand, so neither is marked and a
/// `background` set after either clears it. (A22's own table pin is untouched;
/// this asserts the two methods A23 adds land on the rows already written for
/// them.)
#[test]
fn the_new_background_slots_are_longhands_of_their_family() {
    let css = style_css(
        r##"
        import std::style::{ style, Style };
        fun s(): Style {
            style().raw("background", "#180509").background_image("url(a.png)").background_size("cover")
        }
        let _s = const s();
        fun main() {}
        main();
        "##,
    );
    let (shorthand, shorthand_at) = rule_for(&css, "background:#180509");
    let (image, image_at) = rule_for(&css, "background-image:url(a.png)");
    let (size, size_at) = rule_for(&css, "background-size:cover");
    assert!(
        shorthand.starts_with("*."),
        "the background shorthand is marked: {shorthand}"
    );
    assert!(
        image.starts_with('.') && size.starts_with('.'),
        "the new slots are longhands and must NOT be marked: {image} / {size}"
    );
    assert!(
        shorthand_at < image_at && shorthand_at < size_at,
        "background must precede its longhands:\n{css}"
    );

    // And the shorthand written LAST clears them, the ordinary family drop.
    assert_compiles_and_runs(
        r##"
        import std::print;
        import std::style::{ style };
        fun main() {
            let reset = const style()
                .background_image("url(a.png)")
                .background_size("cover")
                .raw("background", "#180509");
            print(reset.class_list().contains(" "));
        }
        main();
        "##,
        "false\n",
    );
}

/// §0bis.3's precedent, extended to the CSS-text escapes: the one malformation
/// they can detect is an EMPTY value, whose realistic source is an
/// interpolation whose variable was never set. A build error naming the value,
/// not a `property:` declaration the browser drops in silence.
#[test]
fn a_blank_css_escape_fails_the_build() {
    for (source, expected) in [
        (
            r#"
            import std::style::{ style, Style, Length };
            fun s(): Style { style().width(Length::raw("")) }
            let _s = const s();
            fun main() {}
            main();
            "#,
            "Length::raw was given an empty value",
        ),
        (
            r#"
            import std::style::{ style, Style, Length };
            fun s(): Style { style().width(Length::calc("   ")) }
            let _s = const s();
            fun main() {}
            main();
            "#,
            "Length::calc was given an empty value",
        ),
        (
            r#"
            import std::style::{ style, Style };
            fun s(): Style { style().background_image("") }
            let _s = const s();
            fun main() {}
            main();
            "#,
            "background_image was given an empty value",
        ),
        (
            r#"
            import std::style::{ style, Style };
            fun s(): Style { style().background_size("") }
            let _s = const s();
            fun main() {}
            main();
            "#,
            "background_size was given an empty value",
        ),
    ] {
        let diagnostics = failure_diagnostics(source);
        assert!(
            diagnostics
                .iter()
                .any(|(message, _)| message.contains(expected)),
            "missing {expected}: {diagnostics:#?}"
        );
    }
}

/// The reversal, pinned as evidence rather than asserted in prose: every shape
/// the website's 36 `raw("background", ..)` sites write is already expressible
/// with the value types §0bis.3 shipped. Each arm is a real site, quoted.
#[test]
fn the_website_background_sites_convert_with_the_shipped_value_types() {
    let assets = collected_assets(
        r##"
        import std::style::{ style, Style, Color, Gradient, RadialExtent };
        // art.vl:92 — `background: #EB682E`, 8 of the 20 solid sites.
        fun literal_hex(): Style { style().background(Color::hex("#EB682E")) }
        // art.vl:55 — `background: rgba(27, 6, 13, 0.88)`, the other 12.
        fun literal_rgba(): Style { style().background(Color::rgba(27, 6, 13, 0.88)) }
        // art.vl:117 and ten more — the closest-side glow, the single most
        // repeated shape in the sweep. Positions default to 0/100 in CSS, so
        // stating them is computed-identical.
        fun glow(): Style {
            style().background_gradient(
                Gradient::radial(RadialExtent::ClosestSide)
                    .stop(Color::rgba(178, 48, 86, 0.5), 0.0)
                    .stop(Color::transparent(), 100.0),
            )
        }
        // art.vl:151 — `linear-gradient(to left, #B23056, #672283)`. The side
        // keywords ARE angles: `to left` is 270deg, `to right` is 90deg.
        fun to_left(): Style {
            style().background_gradient(
                Gradient::linear(270.0)
                    .stop(Color::hex("#B23056"), 0.0)
                    .stop(Color::hex("#672283"), 100.0),
            )
        }
        // art.vl:578 — an 8-digit hex with a mid stop; `hex` is unvalidated
        // text, so the alpha rides along.
        fun eight_digit(): Style {
            style().background_gradient(
                Gradient::radial(RadialExtent::ClosestSide)
                    .stop(Color::hex("#120004d6"), 35.0)
                    .stop(Color::transparent(), 100.0),
            )
        }
        let _a = const literal_hex();
        let _b = const literal_rgba();
        let _c = const glow();
        let _d = const to_left();
        let _e = const eight_digit();
        fun main() {}
        main();
        "##,
    );
    let lines: Vec<&str> = assets.iter().map(|(_, line)| line.as_str()).collect();
    for expected in [
        "{background-color:#EB682E}",
        "{background-color:rgba(27, 6, 13, 0.88)}",
        "{background-image:radial-gradient(closest-side, rgba(178, 48, 86, 0.5) 0%, transparent 100%)}",
        "{background-image:linear-gradient(270deg, #B23056 0%, #672283 100%)}",
        "{background-image:radial-gradient(closest-side, #120004d6 35%, transparent 100%)}",
    ] {
        assert!(
            lines.iter().any(|line| line.contains(expected)),
            "missing {expected}: {lines:?}"
        );
    }
}

/// The conversion hazard §0bis.5 records for the next cycle, pinned in both
/// directions: converting `raw("background", v)` to a typed method moves the
/// slot from the family SHORTHAND to a longhand, so the shorthand's reset of
/// the rest of the family stops happening. It is safe at every website site
/// (none pairs a colour with an image), and a half-converted style still
/// resolves by AUTHORING order through A22's marker — which is what makes the
/// conversion safe to do incrementally, in any order.
#[test]
fn converting_a_background_shorthand_to_a_longhand_keeps_authoring_order() {
    let css = style_css(
        r##"
        import std::style::{ style, Style, Color, Gradient, RadialExtent };
        // An UNconverted base under a converted override: the raw shorthand is
        // marked and sorts first, so the gradient still wins.
        fun half_converted(): Style {
            style().raw("background", "#180509").background_gradient(
                Gradient::radial(RadialExtent::ClosestSide)
                    .stop(Color::rgba(235, 104, 46, 0.4), 0.0)
                    .stop(Color::transparent(), 100.0),
            )
        }
        let _s = const half_converted();
        fun main() {}
        main();
        "##,
    );
    let (_, shorthand_at) = rule_for(&css, "background:#180509");
    let (_, image_at) = rule_for(
        &css,
        "background-image:radial-gradient(closest-side, rgba(235, 104, 46, 0.4) 0%, transparent 100%)",
    );
    assert!(
        shorthand_at < image_at,
        "the unconverted shorthand must still sort ahead of the converted longhand:\n{css}"
    );

    // Fully converted, the colour and the image occupy DIFFERENT slots and both
    // survive — CSS paints the image over the colour, which is the whole reason
    // `background_gradient` was given its own slot in §0bis.3.
    assert_compiles_and_runs(
        r##"
        import std::print;
        import std::style::{ style, Color, Gradient, RadialExtent };
        fun main() {
            let converted = const style()
                .background(Color::hex("#180509"))
                .background_gradient(
                    Gradient::radial(RadialExtent::ClosestSide)
                        .stop(Color::rgba(235, 104, 46, 0.4), 0.0)
                        .stop(Color::transparent(), 100.0),
                );
            print(converted.class_list().contains(" "));
        }
        main();
        "##,
        "true\n",
    );
}

/// Decision 2, pinned: `padding_xy` is not minted because the axis methods
/// already write EXACTLY the shorthand's four slots. All four two-value sites
/// in the sweep are the `y x` form, and this is what they compose to — plus the
/// A22 resolution in both directions, which is what confirmed §0bis.3's cut
/// rather than reopening it.
#[test]
fn the_two_value_padding_sites_compose_from_the_axis_methods() {
    let assets = collected_assets(
        r#"
        import std::style::{ style, Style, Length };
        // playground_page.vl:147 — `padding: 8px 20px`.
        fun s(): Style {
            style().padding_y(Length::px(8)).padding_x(Length::px(20))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    let lines: Vec<&str> = assets.iter().map(|(_, line)| line.as_str()).collect();
    for expected in [
        "{padding-top:8px}",
        "{padding-bottom:8px}",
        "{padding-left:20px}",
        "{padding-right:20px}",
    ] {
        assert!(
            lines.iter().any(|line| line.contains(expected)),
            "missing {expected}: {lines:?}"
        );
    }

    // The axes cover the whole box, so a `padding` set AFTER them drops all
    // four — one class, exactly as it would have replaced a raw shorthand.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::style::{ style, space, Length };
        fun main() {
            let boxed = const style()
                .padding_y(Length::px(8))
                .padding_x(Length::px(20))
                .padding(space(4));
            print(boxed.class_list().contains(" "));
        }
        main();
        "#,
        "false\n",
    );
}

// --- W11 style dogfood: size / Color::var / Color::oklch / attribute ----------
// Cycle 29's kolt-dogfood additions (kolt.local tracker items 010-013). Same
// pin idioms as A8/A22/A23 above: table-shaped declarations through
// `collected_assets`, refusals through `failure_diagnostics`, rendered class
// lists through `assert_compiles_and_runs`.

/// `size` is the sizing pair's axis shorthand (`padding_x`'s pattern): one
/// value into the two slots `width` and `height` already own — two classes,
/// not a new property (item 011).
#[test]
fn size_writes_the_width_and_height_slots() {
    let assets = collected_assets(
        r#"
        import std::style::{ style, Style, Length };
        fun s(): Style {
            style().size(Length::rem(1.0))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    let lines: Vec<&str> = assets.iter().map(|(_, line)| line.as_str()).collect();
    for expected in ["{width:1rem}", "{height:1rem}"] {
        assert!(
            lines.iter().any(|line| line.contains(expected)),
            "missing {expected}: {lines:?}"
        );
    }
}

/// Because they are the SAME slots, a later `height` narrows the square by
/// the ordinary last-wins rule: three calls, two classes.
#[test]
fn a_height_after_size_narrows_by_last_wins() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::style::{ style, Style, Length };
        fun main() {
            let squared = const style().size(Length::rem(1.0)).height(Length::rem(2.0));
            print(squared.class_list().split(" ").len());
        }
        main();
        "#,
        "2\n",
    );
}

/// `Color::var` is `Length::var`'s counterpart (item 012): the typed spelling
/// of a CSS-variable-backed colour. It declares NOTHING — no `:root` line
/// emits, the app owns the custom property's declaration — and `.alpha()`
/// composes over it through the relative-colour form, like over a ramp token.
#[test]
fn color_var_references_without_declaring() {
    let assets = collected_assets(
        r#"
        import std::style::{ style, Style, Color };
        fun s(): Style {
            style()
                .background(Color::var("--accent"))
                .border_color(Color::var("--accent").alpha(0.5))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    let lines: Vec<&str> = assets.iter().map(|(_, line)| line.as_str()).collect();
    for expected in [
        "{background-color:var(--accent)}",
        "{border-color:rgb(from var(--accent) r g b / 0.5)}",
    ] {
        assert!(
            lines.iter().any(|line| line.contains(expected)),
            "missing {expected}: {lines:?}"
        );
    }
    assert!(
        !lines.iter().any(|line| line.starts_with(":root{--accent")),
        "Color::var must not declare the property: {lines:?}"
    );
}

/// `Color::oklch` (item 013): the perceptual literal, emitted in the CSS
/// number form — space-joined components, the hue a bare degree count — with
/// `.alpha()` composing through the relative form like over any colour.
#[test]
fn oklch_emits_the_number_form() {
    let assets = collected_assets(
        r#"
        import std::style::{ style, Style, Color };
        fun s(): Style {
            style()
                .background(Color::oklch(0.62, 0.19, 313.0))
                .color(Color::oklch(0.97, 0.02, 340.0).alpha(0.8))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    let lines: Vec<&str> = assets.iter().map(|(_, line)| line.as_str()).collect();
    for expected in [
        "{background-color:oklch(0.62 0.19 313)}",
        "{color:rgb(from oklch(0.97 0.02 340) r g b / 0.8)}",
    ] {
        assert!(
            lines.iter().any(|line| line.contains(expected)),
            "missing {expected}: {lines:?}"
        );
    }
}

/// The three range refusals, per case like `rgba`'s channels: lightness is
/// the CSS NUMBER form (0.0-1.0), not the percentage.
#[test]
fn an_oklch_lightness_outside_the_unit_range_fails_the_build() {
    let diagnostics = failure_diagnostics(
        r#"
        import std::style::{ style, Style, Color };
        fun s(): Style {
            style().background(Color::oklch(62.0, 0.19, 313.0))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    assert!(
        diagnostics
            .iter()
            .any(|(message, _)| message.contains("oklch lightness 62 is outside 0.0-1.0")),
        "{diagnostics:#?}"
    );
}

#[test]
fn an_oklch_chroma_outside_its_range_fails_the_build() {
    let diagnostics = failure_diagnostics(
        r#"
        import std::style::{ style, Style, Color };
        fun s(): Style {
            style().background(Color::oklch(0.62, 0.7, 313.0))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    assert!(
        diagnostics
            .iter()
            .any(|(message, _)| message.contains("oklch chroma 0.7 is outside 0.0-0.5")),
        "{diagnostics:#?}"
    );
}

/// Angles wrap in CSS, so admitting 700 beside 340 would mint two classes
/// for one colour — the canonical turn is required.
#[test]
fn an_oklch_hue_outside_the_canonical_turn_fails_the_build() {
    let diagnostics = failure_diagnostics(
        r#"
        import std::style::{ style, Style, Color };
        fun s(): Style {
            style().background(Color::oklch(0.62, 0.19, 700.0))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    assert!(
        diagnostics
            .iter()
            .any(|(message, _)| message.contains("oklch hue 700 is outside 0.0-360.0")),
        "{diagnostics:#?}"
    );
}

// `Style::attribute(name, value, inner)` (item 010): a condition on the
// element ITSELF — `.sX[data-open="true"]` — where `within` is the ancestor
// form. Its own slot in the condition axis, between the relation and the
// pseudo-class: `md(within(.., attribute(.., hover(..))))`. The pins mirror
// the within×pseudo set: composition on each axis pair, the full stack, the
// ordering refusals naming the fix, the const validation, per-(condition,
// property) merge, and the ssr leg.

#[test]
fn an_attribute_condition_selects_on_the_element_itself() {
    let assets = collected_assets(
        r#"
        import std::style::{ style, Style };
        fun s(): Style {
            style().attribute("data-open", "true", style().opacity(0.5))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    assert!(
        assets.iter().any(|(_, line)| {
            // Starts with '.' — no ancestor, and the base cascade band (B35).
            line.starts_with('.') && line.contains("[data-open=\"true\"]{opacity:0.5}")
        }),
        "{assets:?}"
    );
}

/// The attribute suffix sits between the class and the pseudo-class, the
/// outside-in order of the call: `attribute(.., hover(..))`.
#[test]
fn an_attribute_condition_wraps_a_pseudo_class() {
    let assets = collected_assets(
        r#"
        import std::style::{ style, Style };
        fun s(): Style {
            style().attribute("data-open", "true", style().hover(style().opacity(0.8)))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    assert!(
        assets.iter().any(|(_, line)| {
            line.starts_with('.') && line.contains("[data-open=\"true\"]:hover{opacity:0.8}")
        }),
        "{assets:?}"
    );
}

/// The ancestor guard composes over an attribute-conditioned style through
/// the same generic path it composes over a pseudo-class.
#[test]
fn within_wraps_an_attribute_condition() {
    let assets = collected_assets(
        r#"
        import std::style::{ style, Style };
        fun s(): Style {
            style().within("data-theme", "dark", style().attribute("data-open", "true", style().opacity(0.8)))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    assert!(
        assets.iter().any(|(_, line)| {
            line.starts_with("[data-theme=\"dark\"] .")
                && line.contains("[data-open=\"true\"]{opacity:0.8}")
        }),
        "{assets:?}"
    );
}

/// All four axes at once, outside-in: media, the relation, attribute, pseudo.
/// The composed line still starts with '@', so B35's media ordering sees it.
#[test]
fn all_four_condition_axes_compose_outside_in() {
    let assets = collected_assets(
        r#"
        import std::style::{ style, Style };
        fun s(): Style {
            style().md(style().within("data-theme", "dark", style().attribute(
                "data-open",
                "true",
                style().hover(style().opacity(0.8)),
            )))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    assert!(
        assets.iter().any(|(_, line)| {
            line.starts_with("@media (min-width: 768px){[data-theme=\"dark\"] .")
                && line.ends_with("[data-open=\"true\"]:hover{opacity:0.8}}")
        }),
        "{assets:?}"
    );
}

/// The four ordering refusals, each naming the fix — the dark×pseudo
/// refusal set extended to the new axis.
#[test]
fn an_attribute_cannot_wrap_a_media_conditioned_style() {
    let diagnostics = failure_diagnostics(
        r#"
        import std::style::{ style, space, Style };
        fun s(): Style {
            style().attribute("data-open", "true", style().md(style().padding(space(6))))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    assert!(
        diagnostics
            .iter()
            .any(|(message, _)| message.contains("nest conditions as md(attribute(..))")),
        "{diagnostics:#?}"
    );
}

#[test]
fn an_attribute_cannot_wrap_within() {
    let diagnostics = failure_diagnostics(
        r#"
        import std::style::{ style, Style, Color };
        fun s(): Style {
            style().attribute("data-open", "true", style().within("data-theme", "dark", style().background(Color::gray(700))))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    assert!(
        diagnostics
            .iter()
            .any(|(message, _)| message
                .contains("within(.., attribute(..)), not attribute(within(..))")),
        "{diagnostics:#?}"
    );
}

#[test]
fn a_pseudo_class_cannot_wrap_an_attribute_condition() {
    let diagnostics = failure_diagnostics(
        r#"
        import std::style::{ style, Style, Color };
        fun s(): Style {
            style().hover(style().attribute("data-open", "true", style().background(Color::gray(700))))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    assert!(
        diagnostics
            .iter()
            .any(|(message, _)| message
                .contains("attribute(.., hover(..)), not hover(attribute(..))")),
        "{diagnostics:#?}"
    );
}

#[test]
fn an_attribute_cannot_wrap_an_attribute_condition() {
    let diagnostics = failure_diagnostics(
        r#"
        import std::style::{ style, Style, Color };
        fun s(): Style {
            style().attribute(
                "data-open",
                "true",
                style().attribute("data-side", "left", style().background(Color::gray(700))),
            )
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    assert!(
        diagnostics
            .iter()
            .any(|(message, _)| message.contains("already attribute-conditioned")),
        "{diagnostics:#?}"
    );
}

/// The name and value fences: the characters that delimit the slot key, the
/// condition grammar, and the selector's own quoting are refused at const
/// time, like every other validation in the module.
#[test]
fn an_attribute_name_with_a_delimiter_fails_the_build() {
    let diagnostics = failure_diagnostics(
        r#"
        import std::style::{ style, Style };
        fun s(): Style {
            style().attribute("data open", "true", style().opacity(0.5))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    assert!(
        diagnostics
            .iter()
            .any(|(message, _)| message.contains("an attribute name cannot contain ' '")),
        "{diagnostics:#?}"
    );
}

#[test]
fn an_attribute_value_with_a_quote_fails_the_build() {
    let diagnostics = failure_diagnostics(
        r#"
        import std::style::{ style, Style };
        fun s(): Style {
            style().attribute("data-open", "tr\"ue", style().opacity(0.5))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    assert!(
        diagnostics
            .iter()
            .any(|(message, _)| message.contains("an attribute value cannot contain '\"'")),
        "{diagnostics:#?}"
    );
}

/// The slot key carries the attribute condition, so last-wins merge stays
/// per-(condition, property): the same attribute and property override to
/// one class; two values of one attribute are two conditions and coexist.
#[test]
fn attribute_slots_merge_per_condition_and_property() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::style::{ style, Style };
        fun main() {
            let togged = const style()
                .attribute("data-open", "true", style().opacity(0.5))
                .attribute("data-open", "true", style().opacity(1.0));
            print(togged.class_list().split(" ").len());
            let sided = const style()
                .attribute("data-side", "left", style().opacity(0.5))
                .attribute("data-side", "right", style().opacity(1.0));
            print(sided.class_list().split(" ").len());
        }
        main();
        "#,
        "1\n2\n",
    );
}

/// The ssr leg: an attribute-conditioned style reaches `styled` like any
/// other — the class attribute carries the attribute class beside the base
/// class, and the rules were already in the build-time stylesheet.
#[test]
fn ssr_renders_attribute_conditioned_classes() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::style::{ style, Style, Color };
        import std::ui::{ view, render };
        fun main() {
            let disclosure = const style()
                .color(Color::gray(700))
                .attribute("data-open", "true", style().color(Color::gray(900)));
            print(render(view("div").styled(disclosure)));
        }
        main();
        "#,
        "<div class=\"s1hbtfg8 sjt5x3g\"></div>\n",
    );
}

// --- kolt.local 009+014: the child-side relations (ui-styling.md §0bis.6) ------
// `children`/`divide` rules REACH IN — they style elements other than the one
// carrying the class — so they emit inside `@layer vilan`, and 032's cascade
// invariant reads as: a child's own `Style` always wins against a rule
// reaching in from an ancestor. `within` stays UNLAYERED (its rules dress the
// element itself; see the within pins above). `divide` renders
// `> :not(:first-child)` — the same element set as the owl `> * + *` but
// carrying (0,2,0) — so on a property both relations touch, divide outranks
// children by SPECIFICITY instead of tying and falling to the class-hash line
// order (the §0bis.6 probe caught the owl's winner flipping with hash bytes).
//
// Order 12's lesson applies here: the CSS bytes alone can stay right while
// the slot map goes wrong, so each behavior pins BOTH the emitted rule and
// the RESOLVED state (`class_list` counts and identities at runtime).

#[test]
fn children_emits_a_layered_child_combinator() {
    let assets = collected_assets(
        r#"
        import std::style::{ style, space, Style };
        fun s(): Style {
            style().children(style().margin_top(space(2)))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    assert!(
        assets.iter().any(|(_, line)| {
            line.starts_with("@layer vilan{.") && line.ends_with(" > *{margin-top:var(--space-2)}}")
        }),
        "{assets:?}"
    );
}

#[test]
fn divide_emits_the_layered_not_first_child_refinement() {
    let assets = collected_assets(
        r#"
        import std::style::{ style, space, Style };
        fun s(): Style {
            style().divide(style().margin_top(space(4)))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    assert!(
        assets.iter().any(|(_, line)| {
            line.starts_with("@layer vilan{.")
                && line.ends_with(" > :not(:first-child){margin-top:var(--space-4)}}")
        }),
        "{assets:?}"
    );
}

/// The RESOLVED state of the relation slots, not just the emitted CSS: the
/// same (relation, property) slot merges last-wins — in a chain and across
/// `+` — while children and divide are two slots and coexist, and a relation
/// slot never collides with the base slot of the same property.
#[test]
fn relation_slots_merge_per_relation_and_property() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::style::{ style, space, Style };
        fun main() {
            // same relation, same property, twice: ONE slot, last wins —
            // byte-identical to writing only the last call
            let merged = const style()
                .children(style().margin_top(space(2)))
                .children(style().margin_top(space(4)));
            let last_only = const style().children(style().margin_top(space(4)));
            print(merged.class_list() == last_only.class_list());
            // across `+` too: the right side wins the shared slot
            let a = const style().children(style().margin_top(space(2)));
            let b = const style().children(style().margin_top(space(4)));
            print((a + b).class_list() == last_only.class_list());
            // children and divide on ONE property: TWO slots, both survive
            let overlap = const style()
                .children(style().margin_top(space(2)))
                .divide(style().margin_top(space(4)));
            print(overlap.class_list().split(" ").len());
            // a relation slot and the base slot of one property are distinct
            // (the base dresses the element, the relation its children)
            let split = const style()
                .margin_top(space(2))
                .children(style().margin_top(space(4)));
            print(split.class_list().split(" ").len());
        }
        main();
        "#,
        "true\ntrue\n2\n2\n",
    );
}

/// A breakpoint wraps a child relation through the existing media pass-through
/// — media outermost in the emitted line, so B35's numeric `min-width` sort
/// still sees its prefix, with the layer nested inside.
#[test]
fn a_breakpoint_wraps_a_child_relation() {
    let assets = collected_assets(
        r#"
        import std::style::{ style, space, Style };
        fun s(): Style {
            style().md(style().children(style().gap(space(2))))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    assert!(
        assets.iter().any(|(_, line)| {
            line.starts_with("@media (min-width: 768px){@layer vilan{.")
                && line.ends_with(" > *{gap:var(--space-2)}}}")
        }),
        "{assets:?}"
    );
}

/// The §0bis.4 family marker holds inside the layer: a shorthand's `*.` still
/// sorts ahead of its family's longhands because the `@layer vilan{` prefix is
/// byte-identical across the pair, so the `*`/`.` byte still decides.
#[test]
fn the_family_marker_orders_a_shorthand_inside_the_layer() {
    let css = style_css(
        r#"
        import std::style::{ style, space, Style };
        fun s(): Style {
            style().children(style().padding(space(4)).padding_top(space(0)))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    let shorthand_at = css
        .find("@layer vilan{*.")
        .expect("a marked layered shorthand");
    let longhand = css
        .lines()
        .find(|line| line.starts_with("@layer vilan{.") && line.contains("padding-top"))
        .expect("a layered longhand");
    let longhand_at = css.find(longhand).expect("the longhand's offset");
    assert!(
        shorthand_at < longhand_at,
        "the shorthand must sort ahead of the longhand inside the layer:\n{css}"
    );
}

/// The `[` band (§0bis.6's ledger): a within rule sorts AFTER a pseudo rule,
/// so their (0,2,0) tie on one property resolves to the theme — the
/// dark-beats-hover outcome, now carried by source order instead of `:root`'s
/// extra specificity point.
#[test]
fn a_within_rule_sorts_after_the_pseudo_band() {
    let css = style_css(
        r#"
        import std::style::{ style, Style, Color };
        fun s(): Style {
            style()
                .hover(style().background(Color::gray(100)))
                .within("data-theme", "dark", style().background(Color::gray(900)))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    // Each inner chain's own base rule also emits (the recorded
    // over-approximation), so both pins name the conditioned selector shape
    // rather than taking the first declaration match.
    let hover_line = css
        .lines()
        .find(|line| {
            line.starts_with('.') && line.contains(":hover{background-color:var(--gray-100)}")
        })
        .expect("the hover rule");
    let within_line = css
        .lines()
        .find(|line| {
            line.starts_with("[data-theme=\"dark\"] .")
                && line.ends_with("{background-color:var(--gray-900)}")
        })
        .expect("the within rule");
    let hover_at = css.find(hover_line).expect("the hover rule's offset");
    let within_at = css.find(within_line).expect("the within rule's offset");
    assert!(
        hover_at < within_at,
        "the within rule must take the later cascade position:\n{css}"
    );
}

/// The v1 scope refusals, each naming the fix: a child relation takes an
/// UNCONDITIONED inner (a pseudo or attribute under it would bind to the
/// child's compound — unruled semantics), cannot wrap a breakpoint, and no
/// relation wraps a relation.
#[test]
fn a_child_relation_takes_an_unconditioned_style() {
    let diagnostics = failure_diagnostics(
        r#"
        import std::style::{ style, Style, Color };
        fun s(): Style {
            style().children(style().hover(style().background(Color::gray(700))))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    assert!(
        diagnostics
            .iter()
            .any(|(message, _)| message.contains("children takes an unconditioned style")),
        "{diagnostics:#?}"
    );
}

#[test]
fn divide_takes_an_unconditioned_style() {
    let diagnostics = failure_diagnostics(
        r#"
        import std::style::{ style, Style };
        fun s(): Style {
            style().divide(style().attribute("data-open", "true", style().opacity(0.5)))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    assert!(
        diagnostics
            .iter()
            .any(|(message, _)| message.contains("divide takes an unconditioned style")),
        "{diagnostics:#?}"
    );
}

#[test]
fn a_child_relation_cannot_wrap_a_breakpoint() {
    let diagnostics = failure_diagnostics(
        r#"
        import std::style::{ style, space, Style };
        fun s(): Style {
            style().children(style().md(style().padding(space(6))))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    assert!(
        diagnostics
            .iter()
            .any(|(message, _)| message.contains("nest conditions as md(children(..))")),
        "{diagnostics:#?}"
    );
}

#[test]
fn within_cannot_wrap_a_child_relation() {
    let diagnostics = failure_diagnostics(
        r#"
        import std::style::{ style, space, Style };
        fun s(): Style {
            style().within("data-theme", "dark", style().children(style().margin_top(space(2))))
        }
        let _s = const s();
        fun main() {}
        main();
        "#,
    );
    assert!(
        diagnostics
            .iter()
            .any(|(message, _)| message.contains("already relation-conditioned")),
        "{diagnostics:#?}"
    );
}

// --- kolt.local 032: the declaration block (std::style::declare) ---------------
// The generic form of the escape hatch apps were hand-rolling: a set of
// declarations under an author-chosen selector, straight into the const-only
// CSS channel. It mints NO class, produces no `Style`, touches no slot key and
// rehashes nothing — the atomic system is untouched, which is what keeps this
// beside `Style` rather than inside it.
//
// ORDERING RULING. Every block emits inside one cascade layer, `@layer vilan`.
// Unlayered styles beat layered ones whatever their specificity, so a `Style`
// always outranks a declaration block and the block's position in the sheet's
// lexical sort decides nothing — which is why B35's `@media` comparator is
// untouched here rather than extended (the layer line carries no `min-width`,
// so it sorts as an ordinary non-media line).
//
// VALIDATION. The channel is line-granular, so a newline in a selector or a
// declaration SPLITS the rule into two independently-deduped, independently
// sorted lines; braces are `declare`'s own; and an at-rule is refused because a
// group at-rule holds rules, not declarations.

#[test]
fn a_declaration_block_emits_its_selector_and_declarations() {
    // THE EXHIBIT, in std: kolt's theme.vl emitted
    // `[data-theme="{id}"]{{variables_css}}` by hand and then flattened its own
    // multi-line declarations with `.replace(";\n--", ";--")`. `declare` builds
    // the one line, so the surgery has nothing left to do.
    let assets = collected_assets(
        r##"
        import std::style::{ declare, declarations, Color };
        fun theme(id: str) {
            declare(
                i"[data-theme=\"{id}\"]",
                declarations()
                    .color("--color-ink", Color::hex("#fafafa"))
                    .color("--color-ground", Color::hex("#161616")),
            );
        }
        let _theme = const theme("iron-dark");
        fun main() {}
        main();
        "##,
    );
    assert!(
        assets.contains(&(
            "css".to_string(),
            "@layer vilan{[data-theme=\"iron-dark\"]{--color-ink:#fafafa;--color-ground:#161616}}"
                .to_string()
        )),
        "{assets:?}"
    );
}

#[test]
fn a_declaration_block_mints_no_class() {
    // The whole sheet, for a program whose only styling IS a declaration
    // block: one line, no class band at all. Nothing was hashed, so there is
    // nothing for a slot key or a class name to be.
    let css = style_css(
        r##"
        import std::style::{ declare, declarations };
        fun reset() {
            declare("*", declarations().raw("box-sizing", "border-box"));
        }
        let _reset = const reset();
        fun main() {}
        main();
        "##,
    );
    assert_eq!(css, "@layer vilan{*{box-sizing:border-box}}\n", "{css}");
}

#[test]
fn a_declaration_block_leaves_the_atomic_sheet_byte_identical() {
    // The invariant this item lives or dies by. The same styled program, with
    // and without a declaration block: strip the block's own lines and the two
    // stylesheets are equal BYTE FOR BYTE — same class names, same rules, same
    // order. A declaration block adds; it never moves anything.
    const STYLED: &str = r##"
        import std::style::{ style, space, Color, Style };
        fun card(): Style {
            style().padding(space(4)).color(Color::gray(700)).hover(style().color(Color::gray(900)))
        }
        let _card = const card();
        "##;
    let without = style_css(&format!("{STYLED}\nfun main() {{}}\nmain();\n"));
    let with = style_css(&format!(
        r##"{STYLED}
        import std::style::{{ declare, declarations }};
        fun theme() {{
            declare(":root", declarations().raw("--color-ink", "#fafafa"));
        }}
        let _theme = const theme();
        fun main() {{}}
        main();
        "##
    ));
    let stripped = with
        .lines()
        .filter(|line| !line.starts_with("@layer vilan{"))
        .map(|line| format!("{line}\n"))
        .collect::<String>();
    assert_eq!(without, stripped, "with the block:\n{with}");
    assert!(
        with.contains("@layer vilan{:root{--color-ink:#fafafa}}"),
        "the block must actually be there:\n{with}"
    );
}

#[test]
fn preflight_emits_its_reset_only_when_the_program_asks_for_it() {
    // kolt.local 008, the opt-in half. The reset is a std style asset the app
    // EMITS, so the request is a call and the opt-out is its absence — there is
    // no build flag, no `Document` option, and nothing to switch off.
    const STYLED: &str = r##"
        import std::style::{ style, space, Color, Style };
        fun card(): Style {
            style().padding(space(4)).color(Color::gray(700))
        }
        let _card = const card();
        "##;
    let without = style_css(&format!("{STYLED}\nfun main() {{}}\nmain();\n"));
    assert!(
        !without.contains("preflight"),
        "a program that does not ask for the reset gets none of it:\n{without}"
    );

    let with = style_css(&format!(
        r##"{STYLED}
        import std::style::preflight;
        let _reset = const preflight();
        fun main() {{}}
        main();
        "##
    ));
    // Every reset rule is in the reset's own sub-layer, and NOTHING else is.
    let reset_lines: Vec<&str> = with
        .lines()
        .filter(|line| line.starts_with("@layer vilan.preflight{"))
        .collect();
    assert!(
        reset_lines.len() > 20,
        "the reset is Tailwind-preflight scope, not a box-sizing line: {}",
        reset_lines.len()
    );
    for rule in [
        "@layer vilan.preflight{*,::before,::after{box-sizing:border-box}}",
        "@layer vilan.preflight{body{margin:0;line-height:inherit}}",
        "@layer vilan.preflight{img,video{max-width:100%;height:auto}}",
        "@layer vilan.preflight{h1,h2,h3,h4,h5,h6{font-size:inherit;font-weight:inherit}}",
        // The ruling's own addition, verbatim: buttons, anchors and selects.
        "@layer vilan.preflight{a,button,select{display:block}}",
        // …and the UA chrome the form controls carry, stripped.
        "@layer vilan.preflight{button,input,optgroup,select,textarea{font-family:inherit;\
font-size:100%;font-weight:inherit;line-height:inherit;color:inherit;margin:0;padding:0;\
border:0;background-color:transparent}}",
    ] {
        assert!(with.contains(rule), "the reset must carry {rule}:\n{with}");
    }

    // ADDS ONLY: strip the reset's lines and the styled sheet is byte-identical
    // to the one the same program emits without it — same class names, same
    // rules, same order. The reset moves nothing, exactly as `declare` moves
    // nothing.
    let stripped = with
        .lines()
        .filter(|line| !line.starts_with("@layer vilan.preflight{"))
        .map(|line| format!("{line}\n"))
        .collect::<String>();
    assert_eq!(without, stripped, "with the reset:\n{with}");
}

#[test]
fn a_reset_rule_loses_to_a_style_and_to_a_declaration_block() {
    // The ordering ruling, read off the RESOLVED cascade rather than off the
    // sheet's byte order — which is the point, because that order is a lexical
    // sort with no notion of a reset (const-eval.md §3).
    //
    // Three tiers on ONE property, each strictly beating the next by a rule
    // that does not consult specificity or source position:
    //
    //   unlayered `.sX{display:…}`               — a Style, always wins
    //   `@layer vilan{…}`                        — a declare block
    //   `@layer vilan.preflight{…}`              — the reset, always loses
    //
    // css-cascade-5 §6.4.3: unlayered rules sort after layered ones "within the
    // same parent layer (if any)" — applied at the top level that is
    // Style-beats-layer, and applied inside `vilan` it is
    // declare-beats-its-sublayer. One rule, twice, in the same direction.
    let css = style_css(
        r##"
        import std::style::{ style, declare, declarations, preflight, Display, Style };
        fun s(): Style {
            style().display(Display::Flex)
        }
        fun blocks() {
            declare("a", declarations().raw("display", "inline-block"));
        }
        let _reset = const preflight();
        let _blocks = const blocks();
        let _style = const s();
        fun main() {}
        main();
        "##,
    );
    // All three claim `display`, on purpose: this is the collision.
    let (styled, _) = rule_for(&css, "display:flex");
    let (declared, _) = rule_for(&css, "display:inline-block");
    assert!(
        styled.starts_with(".s") && !styled.contains("@layer"),
        "a Style rule is UNLAYERED, so it beats every layer whatever the \
         specificity: {styled}"
    );
    assert!(
        declared.starts_with("@layer vilan{") && !declared.starts_with("@layer vilan.preflight{"),
        "a declaration block is in `vilan` itself, so it beats the sub-layer \
         below it: {declared}"
    );
    assert!(
        css.contains("@layer vilan.preflight{a,button,select{display:block}}"),
        "and the reset's own `display` rule is in the SUB-layer, so it loses to \
         both of them — an app resetting `a` further needs no !important and no \
         longer selector:\n{css}"
    );
    // The claim this test is really making: none of the above is a statement
    // about where the lines landed. Nothing here reads an offset.
    assert!(
        css.lines().filter(|line| line.contains("display:")).count() >= 3,
        "the three tiers are all present in one sheet:\n{css}"
    );
}

#[test]
fn a_declaration_block_is_layered_and_a_style_rule_is_not() {
    // The ordering ruling, read off the sheet: the block is inside
    // `@layer vilan`, the atomic rules are unlayered, and unlayered wins the
    // cascade against any layer regardless of specificity — so the author's
    // chosen selector can never out-specify a view's own style.
    let css = style_css(
        r##"
        import std::style::{ style, declare, declarations, Color, Style };
        fun s(): Style {
            style().color(Color::hex("#111111"))
        }
        fun over() {
            declare("#app div.card", declarations().raw("color", "#eeeeee"));
        }
        let _s = const s();
        let _over = const over();
        fun main() {}
        main();
        "##,
    );
    for line in css.lines() {
        if line.contains("--") && line.starts_with(':') {
            continue;
        }
        let layered = line.starts_with("@layer vilan{");
        assert_eq!(
            layered,
            line.contains("#app div.card"),
            "exactly the declaration block is layered:\n{css}"
        );
    }
}

#[test]
fn the_declaration_layer_does_not_disturb_the_media_sort() {
    // B35, unchanged and unextended. `media_min_width` reads an
    // `@media (min-width: ` prefix that `@layer vilan{` does not carry, so the
    // layer line sorts as an ordinary non-media line and every media block
    // still lands after it in ascending min-width order.
    let css = style_css(
        r##"
        import std::style::{ style, space, declare, declarations, Style };
        fun s(): Style {
            style().sm(style().padding(space(2))).lg(style().padding(space(3)))
        }
        fun theme() {
            declare(":root", declarations().raw("--color-ink", "#fafafa"));
        }
        let _s = const s();
        let _theme = const theme();
        fun main() {}
        main();
        "##,
    );
    let layer_at = css.find("@layer vilan{").expect("the layer line");
    let small_at = css
        .find("@media (min-width: 640px){")
        .expect("the sm block");
    let large_at = css
        .find("@media (min-width: 1024px){")
        .expect("the lg block");
    assert!(
        layer_at < small_at && small_at < large_at,
        "the layer sorts as a non-media line and the min-width order survives:\n{css}"
    );
}

#[test]
fn a_token_spent_in_a_declaration_block_declares_itself() {
    // `.color`/`.length` carry the value's own `:root` line onto the sheet
    // exactly as a `Style` property does, so a ramp or spacing token spent in a
    // block is never a dangling `var()`. That line stays UNLAYERED, like every
    // other token line — the two channels agree rather than one shadowing the
    // other.
    let assets = collected_assets(
        r##"
        import std::style::{ declare, declarations, space, Color };
        fun tokens() {
            declare(
                ":root",
                declarations().color("--brand", Color::gray(50)).length("--pad", space(4)),
            );
        }
        let _tokens = const tokens();
        fun main() {}
        main();
        "##,
    );
    assert!(
        assets.contains(&("css".to_string(), ":root{--gray-50:#f9fafb}".to_string()))
            && assets.contains(&("css".to_string(), ":root{--space-4:1rem}".to_string())),
        "{assets:?}"
    );
    assert!(
        assets.contains(&(
            "css".to_string(),
            "@layer vilan{:root{--brand:var(--gray-50);--pad:var(--space-4)}}".to_string()
        )),
        "{assets:?}"
    );
}

#[test]
fn declarations_keep_their_authoring_order() {
    // A declaration block is CASCADE text: reordering its links would change
    // what it means, where reordering a `style()` chain's links cannot (each
    // link owns a slot). The links join in authoring order here, and the
    // formatter's canonical style-chain sort never reaches a `declarations()`
    // chain — it fires only on a run rooted at the literal `style ( )` tokens
    // (`starts_style_builder`), pinned in `vilan-cli/tests/style_chain_order.rs`.
    let css = style_css(
        r##"
        import std::style::{ declare, declarations };
        fun block() {
            declare(
                "body",
                declarations()
                    .raw("z-index", "1")
                    .raw("box-sizing", "border-box")
                    .raw("display", "block"),
            );
        }
        let _block = const block();
        fun main() {}
        main();
        "##,
    );
    assert!(
        css.contains("{z-index:1;box-sizing:border-box;display:block}"),
        "{css}"
    );
}

/// A data URI carries a `;` (`url("data:image/svg+xml;base64,…")`), so a value
/// keeps its semicolons on purpose — the fences are exactly the characters that
/// break the CHANNEL (a newline) or the BLOCK (a brace), and nothing else.
#[test]
fn a_data_uri_value_keeps_its_semicolon() {
    let css = style_css(
        r##"
        import std::style::{ declare, declarations };
        fun block() {
            declare(
                "body",
                declarations().raw("background-image", "url(\"data:image/svg+xml;base64,AAA\")"),
            );
        }
        let _block = const block();
        fun main() {}
        main();
        "##,
    );
    assert!(
        css.contains("@layer vilan{body{background-image:url(\"data:image/svg+xml;base64,AAA\")}}"),
        "{css}"
    );
}

#[test]
fn a_declaration_block_selector_cannot_contain_a_newline() {
    let diagnostics = failure_diagnostics(
        r##"
        import std::style::{ declare, declarations };
        fun block() {
            declare(":root\n:host", declarations().raw("--color-ink", "#fafafa"));
        }
        let _block = const block();
        fun main() {}
        main();
        "##,
    );
    assert!(
        diagnostics.iter().any(|(message, _)| message
            .contains("selector cannot contain a newline")
            && message.contains("line-granular")),
        "{diagnostics:#?}"
    );
}

#[test]
fn a_declaration_block_selector_cannot_contain_a_brace() {
    let diagnostics = failure_diagnostics(
        r##"
        import std::style::{ declare, declarations };
        fun block() {
            declare(":root{color:red}", declarations().raw("--color-ink", "#fafafa"));
        }
        let _block = const block();
        fun main() {}
        main();
        "##,
    );
    assert!(
        diagnostics.iter().any(
            |(message, _)| message.contains("selector cannot contain '{'")
                && message.contains("declare writes the block's braces")
        ),
        "{diagnostics:#?}"
    );
}

/// A group at-rule holds RULES, not declarations, so `@media (…){color:red}`
/// would be invalid CSS the moment the surface admitted it — and the surface's
/// meaning would start depending on the first byte of its argument.
#[test]
fn a_declaration_block_selector_cannot_be_an_at_rule() {
    let diagnostics = failure_diagnostics(
        r##"
        import std::style::{ declare, declarations };
        fun block() {
            declare("@media (prefers-color-scheme: light)", declarations().raw("--color-ink", "#111111"));
        }
        let _block = const block();
        fun main() {}
        main();
        "##,
    );
    assert!(
        diagnostics
            .iter()
            .any(|(message, _)| message.contains("selector cannot be an at-rule")),
        "{diagnostics:#?}"
    );
}

#[test]
fn a_declaration_block_with_no_declarations_is_rejected() {
    let diagnostics = failure_diagnostics(
        r##"
        import std::style::{ declare, declarations };
        fun block() {
            declare(":root", declarations());
        }
        let _block = const block();
        fun main() {}
        main();
        "##,
    );
    assert!(
        diagnostics
            .iter()
            .any(|(message, _)| message.contains("declares nothing")),
        "{diagnostics:#?}"
    );
}

/// The property owns the `:` that separates it from its value and the `;` that
/// separates it from the next declaration, so it may carry neither.
#[test]
fn a_declaration_property_cannot_carry_its_own_separator() {
    let diagnostics = failure_diagnostics(
        r##"
        import std::style::{ declare, declarations };
        fun block() {
            declare(":root", declarations().raw("color:red", "1"));
        }
        let _block = const block();
        fun main() {}
        main();
        "##,
    );
    assert!(
        diagnostics.iter().any(
            |(message, _)| message.contains("property cannot contain ':'")
                && message.contains("pass the value as the second argument")
        ),
        "{diagnostics:#?}"
    );
}

#[test]
fn a_blank_declaration_value_is_rejected() {
    let diagnostics = failure_diagnostics(
        r##"
        import std::style::{ declare, declarations };
        fun block() {
            declare(":root", declarations().raw("--color-ink", ""));
        }
        let _block = const block();
        fun main() {}
        main();
        "##,
    );
    assert!(
        diagnostics
            .iter()
            .any(|(message, _)| message.contains("the value for \"--color-ink\"")),
        "{diagnostics:#?}"
    );
}

/// `declare` reaches `asset::emit`, so it inherits the const-only bit with no
/// new machinery — the diagnostic names `declare` and the channel it reaches.
#[test]
fn a_runtime_declare_is_rejected() {
    assert_fails_spanning(
        r##"
        import std::style::{ declare, declarations };
        fun main() {
            declare(":root", declarations().raw("--color-ink", "#fafafa"));
        }
        main();
        "##,
        r##"declare(":root", declarations().raw("--color-ink", "#fafafa"))"##,
        "compile-time-only",
    );
}

// --- K2b: typed values in `raw` (proposal/css-block.md §6, slice S1) ---------
// `Style::raw` and `Declarations::raw` are generic over `CssValue` — a `str`
// is a verbatim CSS value, a `Length`/`Color` is a value that may be a THEME
// TOKEN, and a token owes the sheet the `:root` line that defines it. Before
// the widening the only way to get a token into `raw` was to reach for its
// `.text` field, which hands over the text and throws the line away: the
// emitted stylesheet then referenced a custom property nothing declared.

#[test]
fn raw_carries_a_length_tokens_root_line() {
    // The fix, on the `Style` side. `space(4)` renders `var(--space-4)`, and
    // the declaration of `--space-4` lands beside it.
    let assets = collected_assets(
        r##"
        import std::style::{ style, space };
        let _padded = const style().raw("padding", space(4));
        fun main() {}
        main();
        "##,
    );
    assert!(
        assets.contains(&("css".to_string(), ":root{--space-4:1rem}".to_string())),
        "the spacing token's :root line is missing: {assets:?}"
    );
    assert!(
        assets
            .iter()
            .any(|(_, line)| line.contains("padding:var(--space-4)")),
        "{assets:?}"
    );
}

#[test]
fn raw_carries_a_color_tokens_root_line() {
    // The same, for a ramp step — the other half of `CssValue`'s token arm.
    let assets = collected_assets(
        r##"
        import std::style::{ style, Color };
        let _outlined = const style().raw("outline-color", Color::gray(50));
        fun main() {}
        main();
        "##,
    );
    assert!(
        assets.contains(&("css".to_string(), ":root{--gray-50:#f9fafb}".to_string())),
        "the ramp token's :root line is missing: {assets:?}"
    );
    assert!(
        assets
            .iter()
            .any(|(_, line)| line.contains("outline-color:var(--gray-50)")),
        "{assets:?}"
    );
}

#[test]
fn the_text_field_route_declares_no_token() {
    // The hazard the widening removes, pinned as the contrast it is. A `.text`
    // field access hands over a `str`; a `str` carries no token, so the sheet
    // gets `var(--space-4)` with nothing declaring it. This is not a bug in
    // `raw` — it is what asking for the text alone MEANS — and it is exactly
    // why `raw` had to grow the typed spelling above.
    let css = style_css(
        r##"
        import std::style::{ style, space };
        let _padded = const style().raw("padding", space(4).text);
        fun main() {}
        main();
        "##,
    );
    assert!(css.contains("padding:var(--space-4)"), "{css}");
    assert!(
        !css.contains("--space-4:"),
        "the text-only route cannot declare the token it references:\n{css}"
    );
}

#[test]
fn raw_and_with_length_mint_the_same_rule() {
    // The widening is not a second channel: `raw` at the `Length`
    // instantiation IS `with_length`, so the two spellings produce one rule,
    // one class name and one token line — byte for byte.
    let through_raw = style_css(
        r##"
        import std::style::{ style, space };
        let _padded = const style().raw("scroll-margin-top", space(4));
        fun main() {}
        main();
        "##,
    );
    let through_with_length = style_css(
        r##"
        import std::style::{ style, space };
        let _padded = const style().with_length("scroll-margin-top", space(4));
        fun main() {}
        main();
        "##,
    );
    assert_eq!(through_raw, through_with_length, "{through_raw}");
}

#[test]
fn raw_with_a_str_declares_nothing() {
    // The instantiation every existing call site uses. Widening must be
    // INVISIBLE here: the rule alone, no token line, no empty line from an
    // unguarded emit.
    let css = style_css(
        r##"
        import std::style::{ style };
        let _flex = const style().raw("display", "flex");
        fun main() {}
        main();
        "##,
    );
    assert_eq!(css, ".sbiovxm{display:flex}\n", "{css}");
}

#[test]
fn raw_with_an_untokened_length_declares_nothing() {
    // `Length::px` is a literal, not a token: its `root` is "", and an empty
    // root must not reach the channel as a blank line.
    let css = style_css(
        r##"
        import std::style::{ style, Length };
        let _wide = const style().raw("scroll-padding", Length::px(37.0));
        fun main() {}
        main();
        "##,
    );
    assert_eq!(css, ".susg4iv{scroll-padding:37px}\n", "{css}");
}

#[test]
fn a_typed_raw_keeps_its_conditions_and_its_family() {
    // `raw` reaches the sheet through `rule` like every property method, so
    // the widened value composes with the condition combinators and with
    // last-wins across a FAMILY: the `padding` shorthand written here clears
    // the `padding-left` longhand it covers, and the hover variant is its own
    // slot.
    let css = style_css(
        r##"
        import std::style::{ style, space };
        let _card = const style()
            .padding_left(space(2))
            .raw("padding", space(4))
            .hover(style().raw("padding", space(6)));
        fun main() {}
        main();
        "##,
    );
    assert!(css.contains("*.s1ufvr2{padding:var(--space-4)}"), "{css}");
    assert!(css.contains(":hover{padding:var(--space-6)}"), "{css}");
    assert!(
        css.contains(":root{--space-4:1rem}")
            && css.contains(":root{--space-6:1.5rem}")
            && css.contains(":root{--space-2:0.5rem}"),
        "every token spent in the chain declares itself:\n{css}"
    );
}

#[test]
fn a_non_css_value_in_raw_names_the_trait() {
    // The bound's failure shape, the `Slot`/`AttrValue` precedent: the type,
    // the trait, and a secondary span at the declaration.
    assert_fails_with(
        r##"
        import std::style::{ style };
        let _bad = const style().raw("z-index", 3);
        fun main() {}
        main();
        "##,
        "'i32' does not implement trait 'CssValue'",
    );
}

#[test]
fn a_declaration_blocks_raw_carries_a_length_token() {
    // The fix on the `Declarations` side — the surface whose whole job is
    // custom properties, where a dangling `var()` is likeliest.
    let css = style_css(
        r##"
        import std::style::{ declare, declarations, space };
        fun tokens() {
            declare(":root", declarations().raw("--pad", space(6)));
        }
        let _tokens = const tokens();
        fun main() {}
        main();
        "##,
    );
    assert!(css.contains(":root{--space-6:1.5rem}"), "{css}");
    assert!(
        css.contains("@layer vilan{:root{--pad:var(--space-6)}}"),
        "{css}"
    );
}

#[test]
fn a_declaration_blocks_raw_carries_a_color_token() {
    let css = style_css(
        r##"
        import std::style::{ declare, declarations, Color };
        fun tokens() {
            declare(":root", declarations().raw("--brand", Color::gray(50)));
        }
        let _tokens = const tokens();
        fun main() {}
        main();
        "##,
    );
    assert!(css.contains(":root{--gray-50:#f9fafb}"), "{css}");
    assert!(
        css.contains("@layer vilan{:root{--brand:var(--gray-50)}}"),
        "{css}"
    );
}

#[test]
fn a_declaration_blocks_str_raw_declares_nothing() {
    // The instantiation the existing call sites use: the block's line alone.
    let css = style_css(
        r##"
        import std::style::{ declare, declarations };
        fun reset() {
            declare("*", declarations().raw("box-sizing", "border-box"));
        }
        let _reset = const reset();
        fun main() {}
        main();
        "##,
    );
    assert_eq!(css, "@layer vilan{*{box-sizing:border-box}}\n", "{css}");
}

#[test]
fn a_declaration_chain_carries_its_tokens_rather_than_emitting_them() {
    // The `Gradient` shape, and the reason `Declarations` stayed usable
    // outside a `const` expression: the chain ACCUMULATES the `:root` lines it
    // owes and `declare` puts them on the sheet. A chain that is never
    // declared reaches the sheet with nothing — including its tokens.
    let assets = collected_assets(
        r##"
        import std::print;
        import std::style::{ declarations, space };
        fun main() {
            let dropped = declarations().raw("--pad", space(6));
            print(dropped.text);
        }
        main();
        "##,
    );
    assert!(
        assets.is_empty(),
        "an undeclared chain emits nothing: {assets:?}"
    );
}

#[test]
fn a_declaration_chain_builds_outside_a_const_expression() {
    // The corollary, pinned in its own right: building the chain is ordinary
    // runtime code — only `declare` is const-only. Routing the token lines
    // through the VALUE is what keeps that true; an `emit` inside `raw` would
    // have made every declaration chain, `str` ones included, compile-time-only.
    assert_compiles_and_runs(
        r##"
        import std::print;
        import std::style::{ declarations, space, Color };
        fun main() {
            let block = declarations()
                .raw("box-sizing", "border-box")
                .raw("--pad", space(6))
                .raw("--brand", Color::gray(50));
            print(block.text);
        }
        main();
        "##,
        "box-sizing:border-box;--pad:var(--space-6);--brand:var(--gray-50)\n",
    );
}

#[test]
fn a_non_css_value_in_a_declaration_names_the_trait() {
    assert_fails_with(
        r##"
        import std::style::{ declare, declarations };
        fun tokens() {
            declare(":root", declarations().raw("--z", 3));
        }
        let _tokens = const tokens();
        fun main() {}
        main();
        "##,
        "'i32' does not implement trait 'CssValue'",
    );
}

#[test]
fn a_typed_declaration_value_is_checked_like_a_str_one() {
    // `check_declaration` guards the RESOLVED text, so the line-granular
    // channel's rules reach a typed value too — a `Length::raw` carrying a
    // newline is refused exactly as the `str` spelling is.
    assert_run_panics(
        r##"
        import std::style::{ declare, declarations, Length };
        fun tokens() {
            declare(":root", declarations().raw("--pad", Length::raw("1rem\n2rem")));
        }
        let _tokens = const tokens();
        fun main() {}
        main();
        "##,
        "cannot contain a newline",
    );
}

// --- B143: the const-only check follows refined trait dispatch --------------
// `check_const_only` propagates over call edges, and a bounded generic's
// trait dispatch is not one — an `emit` inside a trait impl was invisible to
// the check, compiled clean, and reached the emitted JS as a live
// `__emit_asset` call with no runtime binding (a `ReferenceError` at run
// time). The check now follows `dispatch_refine`'s edges: an `OnConstraint`
// site is charged at the ENTRY whose recorded substitution selects an
// R-member, so refusal is per call site and a clean instantiation of the same
// generic stays admitted; every unresolvable shape (a receiver-less `OnType`,
// an opaque binding) widens to the whole candidate list — over-refusal is the
// deliberate fallback direction, an escape is not.

#[test]
fn emit_reached_through_a_bounded_generic_is_const_only() {
    assert_fails_with(
        r##"
        import std::print;
        import std::asset::emit;
        struct Token {
            value: str,
        }
        trait Emitter {
            fun text(self): str;
        }
        impl Token with Emitter {
            fun text(self): str {
                emit("css", ":root{--p:1rem}");
                self.value
            }
        }
        fun render<V: Emitter>(value: V): str {
            value.text()
        }
        fun main() {
            print(render(Token { value = "var(--p)" }));
        }
        main();
        "##,
        "compile-time-only",
    );
}

#[test]
fn a_clean_impl_through_the_same_bounded_generic_stays_admitted() {
    // The refinement, not a blanket union: a SIBLING impl of the same trait
    // member reaches `emit`, and the clean instantiation still compiles and
    // runs — the entry's recorded substitution selects `Plain::text`, which
    // reaches nothing.
    assert_compiles_and_runs(
        r##"
        import std::print;
        import std::asset::emit;
        struct Token {
            value: str,
        }
        struct Plain {
            text_value: str,
        }
        trait Emitter {
            fun text(self): str;
        }
        impl Token with Emitter {
            fun text(self): str {
                emit("css", ":root{--p:1rem}");
                self.value
            }
        }
        impl Plain with Emitter {
            fun text(self): str {
                self.text_value
            }
        }
        fun render<V: Emitter>(value: V): str {
            value.text()
        }
        fun main() {
            print(render(Plain { text_value = "clean" }));
        }
        main();
        "##,
        "clean\n",
    );
}

#[test]
fn a_bounded_generic_is_charged_per_call_site_not_per_function() {
    // One generic, two entries: the `Token` entry is refused at ITS call, the
    // `Plain` entry draws no diagnostic at all. This is the property that
    // separates the refinement from putting `render` itself in R.
    let source = r##"
        import std::print;
        import std::asset::emit;
        struct Token {
            value: str,
        }
        struct Plain {
            text_value: str,
        }
        trait Emitter {
            fun text(self): str;
        }
        impl Token with Emitter {
            fun text(self): str {
                emit("css", ":root{--p:1rem}");
                self.value
            }
        }
        impl Plain with Emitter {
            fun text(self): str {
                self.text_value
            }
        }
        fun render<V: Emitter>(value: V): str {
            value.text()
        }
        fun main() {
            print(render(Plain { text_value = "clean" }));
            print(render(Token { value = "var(--p)" }));
        }
        main();
        "##;
    let diagnostics = failure_diagnostics(source);
    let token_call = source.find("render(Token").unwrap();
    let plain_call = source.find("render(Plain").unwrap();
    assert!(
        diagnostics
            .iter()
            .any(|(message, range)| message.contains("compile-time-only")
                && range.start == token_call),
        "no refusal at the Token entry: {diagnostics:#?}"
    );
    assert!(
        diagnostics
            .iter()
            .all(|(_, range)| range.start != plain_call),
        "the clean Plain entry must draw no diagnostic: {diagnostics:#?}"
    );
}

#[test]
fn a_forwarding_wrapper_charges_the_entry_that_resolves_it() {
    // The dispatch sits two generics deep; the constraint chases through the
    // wrapper's own parameter to the entry that grounds it, and the refusal
    // anchors at main's call — the outermost runtime crossing, exactly where
    // the concrete spelling anchors.
    let source = r##"
        import std::print;
        import std::asset::emit;
        struct Token {
            value: str,
        }
        trait Emitter {
            fun text(self): str;
        }
        impl Token with Emitter {
            fun text(self): str {
                emit("css", ":root{--p:1rem}");
                self.value
            }
        }
        fun render<V: Emitter>(value: V): str {
            value.text()
        }
        fun wrapper<W: Emitter>(value: W): str {
            render(value)
        }
        fun main() {
            print(wrapper(Token { value = "var(--p)" }));
        }
        main();
        "##;
    let diagnostics = failure_diagnostics(source);
    let entry = source.find("wrapper(Token").unwrap();
    assert!(
        diagnostics
            .iter()
            .any(|(message, range)| message.contains("compile-time-only") && range.start == entry),
        "no refusal anchored at the resolving entry: {diagnostics:#?}"
    );
}

#[test]
fn a_second_generic_parameter_still_charges_its_entry() {
    // Multi-parameter shape: the emitting type arrives through the SECOND
    // constraint while the first stays clean — each parameter's binding
    // resolves independently at the entry.
    assert_fails_with(
        r##"
        import std::print;
        import std::asset::emit;
        struct Token {
            value: str,
        }
        struct Plain {
            text_value: str,
        }
        trait Emitter {
            fun text(self): str;
        }
        impl Token with Emitter {
            fun text(self): str {
                emit("css", ":root{--p:1rem}");
                self.value
            }
        }
        impl Plain with Emitter {
            fun text(self): str {
                self.text_value
            }
        }
        fun render_pair<A: Emitter, B: Emitter>(first: A, second: B): str {
            first.text() + second.text()
        }
        fun main() {
            print(render_pair(Plain { text_value = "clean" }, Token { value = "tok" }));
        }
        main();
        "##,
        "compile-time-only",
    );
}

#[test]
fn a_clean_two_parameter_generic_dispatch_stays_admitted() {
    assert_compiles_and_runs(
        r##"
        import std::print;
        import std::asset::emit;
        struct Token {
            value: str,
        }
        struct Plain {
            text_value: str,
        }
        trait Emitter {
            fun text(self): str;
        }
        impl Token with Emitter {
            fun text(self): str {
                emit("css", ":root{--p:1rem}");
                self.value
            }
        }
        impl Plain with Emitter {
            fun text(self): str {
                self.text_value
            }
        }
        fun render_pair<A: Emitter, B: Emitter>(first: A, second: B): str {
            first.text() + second.text()
        }
        fun main() {
            print(render_pair(Plain { text_value = "a" }, Plain { text_value = "b" }));
        }
        main();
        "##,
        "ab\n",
    );
}

#[test]
fn a_generic_dispatch_reaching_emit_inside_const_stays_legal() {
    // The whole styling shape: inside a `const` the interpreter makes the
    // call, so the restriction lifts entirely — the refined edges anchor at
    // runtime crossings and a `const` entry is not one.
    assert_compiles_and_runs(
        r##"
        import std::print;
        import std::asset::emit;
        struct Token {
            value: str,
        }
        trait Emitter {
            fun text(self): str;
        }
        impl Token with Emitter {
            fun text(self): str {
                emit("css", ":root{--p:1rem}");
                self.value
            }
        }
        fun render<V: Emitter>(value: V): str {
            value.text()
        }
        fun main() {
            let styled = const render(Token { value = "var(--p)" });
            print(styled);
        }
        main();
        "##,
        "var(--p)\n",
    );
}

#[test]
fn an_inherited_trait_default_reaching_emit_is_refused_on_a_concrete_receiver() {
    // The `OnType` half of the same hole: the emitting body is the TRAIT's
    // default, the impl inherits it, and the call re-dispatches per receiver
    // — no call edge either. The receiver's head narrows to the inherited
    // default, which is in R.
    assert_fails_with(
        r##"
        import std::print;
        import std::asset::emit;
        struct Token {
            value: str,
        }
        trait Report {
            fun report(self): str {
                emit("css", ".report{}");
                "reported"
            }
        }
        impl Token with Report {}
        fun main() {
            print(Token { value = "x" }.report());
        }
        main();
        "##,
        "compile-time-only",
    );
}

#[test]
fn a_module_level_initializer_entry_is_a_boundary_for_generic_dispatch() {
    // Initializers own no graph node; the refined edge charges the entry as
    // TopLevel and the refusal lands on the initializer's call, exactly where
    // a direct call to an R-function lands.
    assert_fails_with(
        r##"
        import std::print;
        import std::asset::emit;
        struct Token {
            value: str,
        }
        trait Emitter {
            fun text(self): str;
        }
        impl Token with Emitter {
            fun text(self): str {
                emit("css", ":root{--p:1rem}");
                self.value
            }
        }
        fun render<V: Emitter>(value: V): str {
            value.text()
        }
        let STYLED = render(Token { value = "top" });
        fun main() {
            print(STYLED);
        }
        main();
        "##,
        "compile-time-only",
    );
}

#[test]
fn a_shared_default_self_call_refuses_conservatively_even_for_a_clean_receiver() {
    // DELIBERATE over-refusal, pinned as the chosen posture: `loud`'s body
    // dispatches `self.text()` with no recorded receiver (`OnType(None)` — a
    // shared default body), so the site cannot know which impl a given
    // receiver selects and widens to every candidate. `Token::text` reaches
    // `emit`, so `loud` joins R, and even `Plain`'s clean receiver is refused
    // — the fallback direction is refusal, never escape, because the failure
    // mode being fenced is a clean compile that throws `ReferenceError` at
    // run time. If per-receiver refinement of shared default bodies ever
    // ships, this pin is the one to revisit.
    let source = r##"
        import std::print;
        import std::asset::emit;
        struct Token {
            value: str,
        }
        struct Plain {
            text_value: str,
        }
        trait Emitter {
            fun text(self): str;
            fun loud(self): str {
                self.text()
            }
        }
        impl Token with Emitter {
            fun text(self): str {
                emit("css", ":root{--p:1rem}");
                self.value
            }
        }
        impl Plain with Emitter {
            fun text(self): str {
                self.text_value
            }
        }
        fun main() {
            print(Plain { text_value = "clean" }.loud());
        }
        main();
        "##;
    let diagnostics = failure_diagnostics(source);
    assert!(
        diagnostics
            .iter()
            .any(|(message, _)| message.contains("compile-time-only")),
        "the conservative posture: a shared default whose self-dispatch could select an \
         emitting impl is refused for EVERY receiver, clean ones included — over-refusal \
         is the deliberate fallback, an emitted `ReferenceError` is not: {diagnostics:#?}"
    );
}

// --- K3: std::crypto / std::jwt / std::base64 (Kolt migration) ---------------
// WebCrypto-backed auth primitives. HMAC/PBKDF2 run against the host
// crypto.subtle (present in node), so these are assert_compiles_and_runs; the
// vectors are RFC-checked (HMAC-SHA-512 = RFC 4231 #2). base64url and
// constant-time compare are pure vilan.

#[test]
fn base64url_round_trips_every_tail_length() {
    // 0, 1, 2 leftover bytes each exercise a distinct decode tail.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::base64::{ encode_url, decode_url };
        import std::bytes::{ encode_utf8, decode_utf8 };
        import std::option::Option::{ self, Some, None };
        fun show(text: str) {
            let encoded = encode_url(encode_utf8(text));
            match decode_url(encoded) {
                Some(let bytes) => print(decode_utf8(bytes)),
                None => print("decode failed"),
            }
        }
        fun main() {
            show("abc");
            show("ab");
            show("a");
            show("hello, world");
        }
        main();
        "#,
        "abc\nab\na\nhello, world\n",
    );
}

#[test]
fn hmac_sha512_matches_the_rfc_vector() {
    // RFC 4231 test case 2: key "Jefe", data "what do ya want for nothing?".
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::crypto::hmac_sha512;
        import std::bytes::encode_utf8;
        async fun main() {
            let mac = hmac_sha512(encode_utf8("Jefe"), encode_utf8("what do ya want for nothing?"));
            print(mac.to_hex());
        }
        main();
        "#,
        "164b7a7bfcf819e2e395fbe73b56e0a387bd64222e831fd610270cd7ea2505549758bf75c05a994a6d034f65f8f0e6fdcaeab1a34d4a6b4b636e070a38bce737\n",
    );
}

#[test]
fn the_unkeyed_digests_match_their_published_vectors() {
    // kolt.local 024. The vectors are FIPS 180-4's one-block message sample
    // ("abc") for each of the three widths — a digest pin is only worth
    // anything against a value published independently of this implementation.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::crypto::{ sha256, sha384, sha512 };
        import std::bytes::encode_utf8;
        async fun main() {
            print(sha256(encode_utf8("abc")).to_hex());
            print(sha384(encode_utf8("abc")).to_hex());
            print(sha512(encode_utf8("abc")).to_hex());
        }
        main();
        "#,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\n\
         cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7\n\
         ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f\n",
    );
}

#[test]
fn sha256_digests_the_empty_input_and_mints_a_fingerprint_prefix() {
    // The empty input is the digest's other published edge (FIPS 180-4), and
    // it is the one a naive "hash the bytes I read" path most easily gets
    // wrong by returning "" instead. The second line is kolt's actual demand
    // case (024's exhibit): an asset fingerprint is the first eight hex digits
    // of the file's sha256 — which nothing in the language could produce, so
    // kolt's names were minted out of band and `is_fingerprinted` could only
    // RECOGNIZE the shape.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::crypto::sha256;
        import std::bytes::encode_utf8;
        async fun main() {
            let empty = sha256(encode_utf8("")).to_hex();
            print(empty);
            let hex = sha256(encode_utf8("body { color: red }")).to_hex();
            print(hex.substring(0, 8));
        }
        main();
        "#,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\n\
         925e8741\n",
    );
}

#[test]
fn etag_of_mints_a_quoted_truncated_sha256_validator() {
    // kolt.local 025c. The value is pinned against the FIPS 180-4 "abc"
    // vector (the same one the digest pins hold): the first 32 hex digits of
    // sha256("abc"), with the RFC 9110 quotes as part of the string — 34
    // characters in all. The format is documented surface, so a change to the
    // width or the quoting is a breaking change, not a tweak.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::http::etag_of;
        import std::bytes::encode_utf8;
        async fun main() {
            let tag = etag_of(encode_utf8("abc"));
            print(tag);
            print(i"{tag.len()}");
        }
        main();
        "#,
        "\"ba7816bf8f01cfea414140de5dae2223\"\n34\n",
    );
}

#[test]
fn if_none_match_handles_the_exact_list_star_and_weak_forms() {
    // kolt.local 025c: the RFC 9110 §13.1.2 forms, one leg per case. Weak
    // comparison is pinned in BOTH directions (a `W/` candidate against a
    // strong tag, and a weak tag against a strong candidate), because the RFC
    // mandates weak comparison for If-None-Match and a strong-only drift
    // would silently stop revalidating through a tag-weakening proxy. The
    // last two legs pin the documented limits: quotes are part of the tag
    // (a bare `abc123` names nothing), and a comma-bearing tag's split
    // fragments must not false-positive against a well-formed target.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::http::if_none_match_matches;
        fun main() {
            let tag = "\"abc123\"";
            print(i"exact={if_none_match_matches("\"abc123\"", tag)}");
            print(i"mismatch={if_none_match_matches("\"zzz\"", tag)}");
            print(i"list={if_none_match_matches("\"a\", \"abc123\", \"b\"", tag)}");
            print(i"list_absent={if_none_match_matches("\"a\", \"b\"", tag)}");
            print(i"star={if_none_match_matches("*", tag)}");
            print(i"weak_candidate={if_none_match_matches("W/\"abc123\"", tag)}");
            print(i"weak_target={if_none_match_matches("\"abc123\"", "W/\"abc123\"")}");
            print(i"padded={if_none_match_matches("  \"abc123\"  ", tag)}");
            print(i"empty={if_none_match_matches("", tag)}");
            print(i"unquoted={if_none_match_matches("abc123", tag)}");
            print(i"fragment={if_none_match_matches("\"abc,123\"", "\"abc\"")}");
        }
        main();
        "#,
        "exact=true\nmismatch=false\nlist=true\nlist_absent=false\nstar=true\n\
         weak_candidate=true\nweak_target=true\npadded=true\nempty=false\n\
         unquoted=false\nfragment=false\n",
    );
}

#[test]
fn a_jwt_round_trips_signs_and_verifies() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::jwt::{ sign_hs512, verify_hs512 };
        import std::bytes::encode_utf8;
        import std::option::Option::{ self, Some, None };
        import std::wire::Wire;

        [derive(Wire)]
        struct Claims {
            sub: str,
            admin: bool,
        }

        async fun main() {
            let secret = encode_utf8("top-secret");
            let token = sign_hs512(secret, Claims { sub = "user-42", admin = true });
            print(token.split(".").len());
            let ok: Option<Claims> = verify_hs512(secret, token);
            match ok {
                Some(let claims) => print(i"{claims.sub} {claims.admin}"),
                None => print("verify failed"),
            }
        }
        main();
        "#,
        "3\nuser-42 true\n",
    );
}

#[test]
fn a_tampered_or_wrong_key_jwt_is_rejected() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::jwt::{ sign_hs512, verify_hs512 };
        import std::bytes::encode_utf8;
        import std::option::Option::{ self, Some, None };
        import std::wire::Wire;

        [derive(Wire)]
        struct Claims {
            sub: str,
        }

        fun outcome(label: str, result: Option<Claims>) {
            match result {
                Some(let _c) => print(i"{label}: ACCEPTED"),
                None => print(i"{label}: rejected"),
            }
        }

        async fun main() {
            let secret = encode_utf8("top-secret");
            let token = sign_hs512(secret, Claims { sub = "user-42" });
            let tampered: Option<Claims> = verify_hs512(secret, token + "x");
            outcome("tampered", tampered);
            let wrong: Option<Claims> = verify_hs512(encode_utf8("other-key"), token);
            outcome("wrong-key", wrong);
        }
        main();
        "#,
        "tampered: rejected\nwrong-key: rejected\n",
    );
}

#[test]
fn constant_time_equality_is_correct() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::crypto::equals_constant_time;
        import std::bytes::encode_utf8;
        fun main() {
            print(equals_constant_time(encode_utf8("abcd"), encode_utf8("abcd")));
            print(equals_constant_time(encode_utf8("abcd"), encode_utf8("abce")));
            print(equals_constant_time(encode_utf8("abcd"), encode_utf8("abc")));
        }
        main();
        "#,
        "true\nfalse\nfalse\n",
    );
}

#[test]
fn a_generic_call_in_an_else_branch_binds_its_type_argument() {
    // B17 (FIXED): the root cause was structural, not async — the `if`
    // inference arm propagated the expected-type constraint only into the
    // `then` branch, so a generic call reached only through an `else`
    // (here `dec<C>` in a nested-then inside the outer `else`) never received
    // `Option<C>` and left `C` unbound, miscompiling the `Wire` deserialize
    // to its abstract body. The await in the discovering case was incidental.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::json::{ encode_json, decode_json };
        import std::option::Option::{ self, Some, None };
        import std::result::Result::{ self, Ok, Err };
        import std::wire::Wire;

        [derive(Wire)]
        struct P { v: str }

        fun dec<C: Wire>(json: str): Option<C> {
            let decoded: Result<C, str> = decode_json(json);
            match decoded {
                Ok(let c) => Some(c),
                Err(let _e) => None,
            }
        }

        fun f<C: Wire>(json: str): Option<C> {
            if json.len() == 0 {
                None
            } else {
                if json.len() > 0 { dec(json) } else { None }
            }
        }

        fun main() {
            let json = encode_json(P { v = "hi" });
            let back: Option<P> = f(json);
            match back {
                Some(let c) => print(c.v),
                None => print("none"),
            }
        }
        main();
        "#,
        "hi\n",
    );
}

#[test]
fn a_generic_call_in_a_match_arm_binds_its_type_argument() {
    // The second half of B17: a `match` reads its expectation from the
    // `expected_types` channel, which the constraint parameter alone doesn't
    // feed — so a generic call in a match arm reached through a branch needs
    // the expectation seeded there too. This is the exact std::jwt shape:
    // if -> else -> match Some-arm -> if then -> generic decode.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::json::{ encode_json, decode_json };
        import std::option::Option::{ self, Some, None };
        import std::result::Result::{ self, Ok, Err };
        import std::wire::Wire;

        [derive(Wire)]
        struct P { v: str }

        fun dec<C: Wire>(json: str): Option<C> {
            let decoded: Result<C, str> = decode_json(json);
            match decoded {
                Ok(let c) => Some(c),
                Err(let _e) => None,
            }
        }

        fun f<C: Wire>(json: str): Option<C> {
            if json.len() == 0 {
                None
            } else {
                match Some(json) {
                    Some(let inner) => {
                        if inner.len() > 0 { dec(inner) } else { None }
                    },
                    None => None,
                }
            }
        }

        fun main() {
            let json = encode_json(P { v = "hi" });
            let back: Option<P> = f(json);
            match back {
                Some(let c) => print(c.v),
                None => print("none"),
            }
        }
        main();
        "#,
        "hi\n",
    );
}

#[test]
fn a_generic_call_after_a_branch_nested_await_monomorphizes() {
    // The exact shape jwt.vl had to be restructured around (the async form of
    // the same B17 else-branch bug).
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::json::{ encode_json, decode_json };
        import std::crypto::hmac_sha512;
        import std::bytes::{ Bytes, encode_utf8 };
        import std::option::Option::{ self, Some, None };
        import std::result::Result::{ self, Ok, Err };
        import std::wire::Wire;

        [derive(Wire)]
        struct P { v: str }

        fun dec<C: Wire>(json: str): Option<C> {
            let decoded: Result<C, str> = decode_json(json);
            match decoded {
                Ok(let c) => Some(c),
                Err(let _e) => None,
            }
        }

        async fun f<C: Wire>(secret: Bytes, json: str): Option<C> {
            if json.len() == 0 {
                None
            } else {
                let _mac = hmac_sha512(secret, encode_utf8(json));
                if json.len() > 0 { dec(json) } else { None }
            }
        }

        async fun main() {
            let json = encode_json(P { v = "hi" });
            let back: Option<P> = f(encode_utf8("k"), json);
            match back {
                Some(let c) => print(c.v),
                None => print("none"),
            }
        }
        main();
        "#,
        "hi\n",
    );
}

// --- K4: std::db — SQLite over node:sqlite (Kolt migration) ------------------
// The server-only storage seam: `node:sqlite`'s DatabaseSync through the new
// module-qualified `[extern(new, "module", "Class")]` binding form, with
// `__db_*` helpers for parameter spreads and column reads. Runs against the
// real host database (node ships it built in).

#[test]
fn a_database_round_trips_inserts_and_queries() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::db::{ Database, Statement, Row };
        import std::option::Option::{ self, Some, None };
        fun main() {
            let db = Database::open(":memory:");
            db.exec("CREATE TABLE account (id INTEGER PRIMARY KEY, username TEXT, age INTEGER)");
            let insert = db.prepare("INSERT INTO account (username, age) VALUES (?, ?)");
            print(insert.run(["reed", 30]));
            print(insert.run(["ada", 36]));
            let by_name = db.prepare("SELECT id, username, age FROM account WHERE username = ?");
            match by_name.first(["ada"]) {
                Some(let row) => print(i"{row.text("username")} is {row.integer("age")}"),
                None => print("not found"),
            }
            match by_name.first(["nobody"]) {
                Some(let _row) => print("ghost"),
                None => print("none"),
            }
            let names = db.prepare("SELECT username FROM account ORDER BY id").all([]);
            for row in names {
                print(row.text("username"));
            }
        }
        main();
        "#,
        "1\n2\nada is 36\nnone\nreed\nada\n",
    );
}

#[test]
fn null_columns_are_detectable() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::db::{ Database, Row };
        import std::option::Option::{ self, Some, None };
        fun main() {
            let db = Database::open(":memory:");
            db.exec("CREATE TABLE t (name TEXT, note TEXT)");
            db.prepare("INSERT INTO t (name, note) VALUES (?, NULL)").run(["only-name"]);
            match db.prepare("SELECT name, note FROM t").first([]) {
                Some(let row) => {
                    print(row.is_null("note"));
                    print(row.is_null("name"));
                },
                None => print("empty"),
            }
        }
        main();
        "#,
        "true\nfalse\n",
    );
}

// --- A11 / pilot: web storage + the method-call-result-call parse gap --------

#[test]
fn calling_a_method_call_result_binds_first() {
    // The pilot's KoltStore stored server hooks as `Shared<|..| R>` and called
    // them; `self.hook.read()(args)` — calling a METHOD-call result directly —
    // does not parse (B-note), but binding the result first does. This pins the
    // working shape; the direct form is the ignored pin below.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::shared::Shared;
        struct Holder { hook: Shared<|str| i32> }
        impl Holder {
            fun call_it(self, a: str): i32 {
                let hook = self.hook.read();
                hook(a)
            }
        }
        fun main() {
            let h = Holder { hook = Shared::new(|a: str| a.len()) };
            print(h.call_it("abcd"));
        }
        main();
        "#,
        "4\n",
    );
}

#[test]
fn calling_a_method_call_result_directly_parses() {
    // Fixed with the direct-call postfix (backlog §H.18): a member fuses at
    // most one call, so a second `(args)` calls the RESULT.
    assert_compiles(
        r#"
        import std::shared::Shared;
        struct Holder { hook: Shared<|str| i32> }
        impl Holder {
            fun call_it(self, a: str): i32 {
                self.hook.read()(a)
            }
        }
        fun main() {
            let holder = Holder { hook = Shared::new(|text: str| text.len()) };
            let _n = holder.call_it("hi");
        }
        "#,
    );
}

// --- A10: `std::router` + `View.swap` (proposal/router.md) -------------------
//
// The runtime semantics (interception, pushState/popstate, dedupe, disposal)
// are pinned end-to-end in `crates/vilan-cli/tests/router.rs` under a DOM
// stub; these pin the compile-level surface.

#[test]
fn swap_renders_a_dynamic_subtree_per_route_value() {
    // The canonical router shape: nested route enums, a hand-written
    // parse/href pair, `link` through the app's `Routable` impl, and a `swap`
    // whose render closure matches the (unannotated) route value.
    assert_compiles_browser(
        r#"
        import std::ui::{ View, view, mount_root };
        import std::reactive::Signal;
        import std::router::{ current_path, navigate, segments, link, Routable };

        [derive(PartialEq)]
        enum Route {
            Home,
            Workspace(str, WorkspaceRoute),
        }

        [derive(PartialEq)]
        enum WorkspaceRoute {
            Overview,
            Task(i32),
        }

        fun parse(path: str): Route {
            let parts = segments(path);
            if parts.len() == 0 {
                Route::Home
            } else {
                Route::Workspace(parts[0], WorkspaceRoute::Overview)
            }
        }

        fun href(route: Route): str {
            match route {
                Route::Home => "/",
                Route::Workspace(let org, let _inner) => i"/w/{org}",
            }
        }

        impl Route with Routable {
            fun to_path(self): str {
                href(self)
            }
        }

        fun workspace_layout(org: str, inner: WorkspaceRoute): View {
            view("section").child(view("aside").text(org)).child(match inner {
                WorkspaceRoute::Overview => view("div").text("overview"),
                WorkspaceRoute::Task(let id) => view("div").text(i"task {id}"),
            })
        }

        fun main() {
            let route = current_path().map(parse);
            let _root = mount_root("app", || view("main")
                .child(link("Home", Route::Home))
                .child(view("button").on("click", || navigate(href(Route::Home))))
                .swap(route, |current| match current {
                    Route::Home => view("section").text("home"),
                    Route::Workspace(let org, let inner) => workspace_layout(org, inner),
                }));
        }
        "#,
    );
}

// --- B6: closure-return element inference (CLOSED) ---------------------------
//
// `xs.map(|p| p.x)` once typed as `List<unknown>`: `map` bound its result
// generic `U` from the closure's return while the body's field accessor was
// still in-flight. A first general fix deadlocked the slot case and was
// reverted; the B19 defer machinery (plus this window's binder work) closed
// the family for real. These pins hold every recorded shape — this area has
// regressed before, so each case stands on its own.

#[test]
fn a_field_mapped_element_types_without_annotation() {
    // The headline case: `U` comes only from the closure's `p.name`, and the
    // element must be concrete enough to dispatch `len()`.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Point { x: i32, name: str }
        fun main() {
            let points = [Point { x = 1, name = "ab" }];
            let names = points.map(|p| p.name);
            print(names[0].len());
        }
        "#,
        "2\n",
    );
}

#[test]
fn a_field_mapped_element_meets_an_annotated_expectation() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Point { x: i32, name: str }
        fun main() {
            let points = [Point { x = 1, name = "abc" }];
            let names: List<str> = points.map(|p| p.name);
            print(names[0].len());
        }
        "#,
        "3\n",
    );
}

#[test]
fn a_field_mapped_result_chains_immediately() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Point { x: i32, name: str }
        fun main() {
            let points = [Point { x = 1, name = "ab" }];
            print(points.map(|p| p.name)[0].len());
        }
        "#,
        "2\n",
    );
}

#[test]
fn mapped_maps_thread_the_element_type() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Point { x: i32, name: str }
        fun main() {
            let points = [Point { x = 1, name = "abc" }];
            let lens = points.map(|p| p.name).map(|s| s.len());
            print(lens[0]);
        }
        "#,
        "3\n",
    );
}

#[test]
fn a_nested_accessor_closure_return_grounds() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Inner { v: i32 }
        struct Point { inner: Inner }
        fun main() {
            let points = [Point { inner = Inner { v = 41 } }];
            let vs = points.map(|p| p.inner.v);
            print(vs[0] + 1);
        }
        "#,
        "42\n",
    );
}

#[test]
fn a_struct_element_map_dispatches_members_downstream() {
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Point { x: i32, name: str }
        fun main() {
            let points = [Point { x = 1, name = "ab" }];
            let same = points.map(|p| p);
            print(same.map(|q| q.name)[0].len());
        }
        "#,
        "2\n",
    );
}

#[test]
fn a_slot_grounded_list_maps_a_field_closure() {
    // The combination the reverted general fix deadlocked on: the element
    // type comes from a `push`-grounded slot AND the map's `U` comes from a
    // field-access closure return. Both resolutions must be observable to
    // the constraint wake.
    assert_compiles_and_runs(
        r#"
        import std::print;
        struct Point { x: i32, name: str }
        fun main() {
            mut ps = List::new();
            ps.push(Point { x = 1, name = "abcd" });
            let names = ps.map(|p| p.name);
            print(names[0].len());
        }
        "#,
        "4\n",
    );
}

#[test]
fn a_slot_grounded_list_maps_and_sums() {
    // The exact deadlock reproducer from the reverted attempt.
    assert_compiles_and_runs(
        r#"
        import std::print;
        fun main() {
            mut xs = List::new();
            xs.push(1);
            let s = xs.map(|n| n + 1).sum();
            print(s);
        }
        "#,
        "2\n",
    );
}

#[test]
fn a_mapped_signal_meets_a_bound_without_annotation() {
    // B19 (FIXED): `current_path().map(..)` yields `Signal<U = Route>`;
    // passing it to `swap<T: PartialEq>` without annotating the intermediate
    // binding must check the bound against the RESOLVED `Route`, not demand
    // `U: PartialEq`. The method resolution now DEFERS while a closure
    // argument's body is untyped, so `U` binds from the closure's return on
    // the retry instead of freezing abstract.
    assert_compiles_browser(
        r#"
        import std::ui::{ View, view, mount_root };
        import std::reactive::Signal;
        import std::router::{ current_path, segments };

        [derive(PartialEq)]
        enum Route {
            Home,
            Other,
        }

        fun parse(path: str): Route {
            if segments(path).len() == 0 { Route::Home } else { Route::Other }
        }

        fun main() {
            let route = current_path().map(|path| parse(path));
            let _root = mount_root("app", || view("main")
                .swap(route, |current| match current {
                    Route::Home => view("section").text("home"),
                    Route::Other => view("section").text("other"),
                }));
        }
        "#,
    );
}

#[test]
fn swap_requires_a_comparable_value() {
    // `swap<T: PartialEq>` — the dedupe needs `==`, so a source over a struct
    // without the impl is rejected at the call.
    assert_fails_browser_with(
        r#"
        import std::ui::{ View, view, mount_root };
        import std::reactive::Signal;

        struct Opaque {
            tag: str,
        }

        fun main() {
            let source: Signal<Opaque> = Signal::new(Opaque { tag = "a" });
            let _root = mount_root("app", || view("main")
                .swap(source, |current| view("p").text(current.tag)));
        }
        "#,
        "does not implement trait 'PartialEq'",
    );
}

#[test]
fn swap_boundaries_nest() {
    // A swap inside another swap's render closure — each level is its own
    // disposal boundary, and the inner render's owner registration must
    // resolve under the outer's injected extent.
    assert_compiles_browser(
        r#"
        import std::ui::{ View, view, mount_root };
        import std::reactive::Signal;

        fun main() {
            let outer: Signal<i32> = Signal::new(0);
            let inner: Signal<str> = Signal::new("a");
            let _root = mount_root("app", || view("main")
                .swap(outer, |level| view("section")
                    .child(view("h1").text(i"level {level}"))
                    .swap(inner, |name| view("p").text(name))));
        }
        "#,
    );
}

#[test]
fn swap_composes_with_sibling_bindings() {
    // `swap` alongside `bind_each` and `show` on one element tree — the mixed
    // form: three boundary kinds registering into the same enclosing owner.
    assert_compiles_browser(
        r#"
        import std::ui::{ View, view, mount_root };
        import std::reactive::Signal;

        fun main() {
            let page: Signal<i32> = Signal::new(0);
            let items: Signal<List<str>> = Signal::new(["a", "b"]);
            let visible: Signal<bool> = Signal::new(true);
            let _root = mount_root("app", || view("main")
                .child(view("ul").bind_each(items, |item| item, |item| view("li").text(item)))
                .child(view("aside").show(visible))
                .swap(page, |current| view("section").text(i"page {current}")));
        }
        "#,
    );
}

#[test]
fn on_event_hands_the_handler_the_dom_event() {
    // `View.on_event` — the handler receives a typed `Event` and can consult
    // modifier/key state and cancel the default action.
    assert_compiles_browser(
        r#"
        import std::ui::{ View, view, mount_root };
        import std::dom::Event;

        fun main() {
            let _root = mount_root("app", || view("input")
                .on_event("keydown", |event| {
                    if event.key() == "Enter" && !event.shift_key() && event.button() == 0 {
                        event.prevent_default();
                    }
                }));
        }
        "#,
    );
}

#[test]
fn link_accepts_any_routable_and_chains() {
    // `link<R: Routable>` dispatches `to_path` through the bound, and the
    // returned `View` chains like any other.
    assert_compiles_browser(
        r#"
        import std::ui::{ View, view, mount_root };
        import std::router::{ link, Routable };

        [derive(PartialEq)]
        enum Route {
            Home,
            Item(i32),
        }

        impl Route with Routable {
            fun to_path(self): str {
                match self {
                    Route::Home => "/",
                    Route::Item(let id) => i"/item/{id}",
                }
            }
        }

        fun main() {
            let _root = mount_root("app", || view("nav")
                .child(link("Home", Route::Home).class("nav-item"))
                .child(link("First", Route::Item(1))));
        }
        "#,
    );
}

#[test]
fn platform_requirement_flows_through_trait_dispatch() {
    // A bounded method call can't name one callee pre-monomorphization, so the
    // walk descends into every CANDIDATE (async_infer's rule): a browser build
    // reaching `save_it` is charged for the @process impl.
    assert_fails_browser_with(
        r#"
        import std::fs::write_file;

        trait Save {
            fun save(self): bool;
        }

        struct DiskStore { path: str }

        impl DiskStore with Save {
            fun save(self): bool {
                write_file(self.path, "state");
                true
            }
        }

        fun save_it<T: Save>(store: T): bool {
            store.save()
        }

        fun main() {
            save_it(DiskStore { path = "s.txt" });
        }
        "#,
        "requires the `process` layer of `std`",
    );
}

#[test]
fn a_closures_platform_charges_its_creator() {
    // The v1 creator rule: making the closure is the colored act — the body
    // is charged where the literal is created, whether or not it is called.
    assert_fails_browser_with(
        r#"
        import std::fs::write_file;

        fun make_saver(path: str): |str| void {
            |content: str| {
                write_file(path, content);
            }
        }

        fun main() {
            let _saver = make_saver("s.txt");
        }
        "#,
        "requires the `process` layer of `std`",
    );
}

#[test]
fn a_neutral_instantiation_is_admitted_despite_a_colored_impl() {
    // §3.2's refinement, landed: the walk threads each call's recorded
    // bindings, so `save_it(MemStore { .. })` descends only into
    // `MemStore`'s impl — `DiskStore`'s `@process` body no longer charges
    // an instantiation that never selects it.
    assert_compiles_browser(
        r#"
        import std::fs::write_file;

        trait Save {
            fun save(self): bool;
        }

        struct MemStore { last: str }
        struct DiskStore { path: str }

        impl MemStore with Save {
            fun save(self): bool { true }
        }

        impl DiskStore with Save {
            fun save(self): bool {
                write_file(self.path, "state");
                true
            }
        }

        fun save_it<T: Save>(store: T): bool {
            store.save()
        }

        fun main() {
            // Only the neutral impl is instantiated; the disk impl exists but
            // is never reached on this build.
            save_it(MemStore { last = "" });
        }
        "#,
    );
}

// --- K2c: `css` is a hard keyword (proposal/css-block.md §5.4, Q3) -------------
// The word was promoted so the block — and, later, the headed form, which needs
// a token two-token lookahead cannot give — has a grammar seat. The promotion
// took three names out of `std::style`: `Length::css(…)` became `Length::raw(…)`
// and the `css` field of a `Length` and a `Color` became `text`. Every position
// that used to spell the word now refuses, and the refusal NAMES both renames —
// a bare "found `css`, expected an identifier" would leave the reader to guess
// what their `.css` became. One pin per position, because each reaches the rule
// through a different seam in the parser: a member access, a `::` path, a
// binding, a struct field declaration and a struct-initializer field.

/// The rename the refusal has to name, in the wording every position shares.
const CSS_RENAME_NOTE: &str = "`Length::css(…)` is now `Length::raw(…)`";

#[test]
fn a_css_member_access_refuses_naming_the_rename() {
    assert_fails_with(
        r#"
        import std::print;
        import std::style::space;
        fun main() {
            print(space(4).css);
        }
        main();
        "#,
        CSS_RENAME_NOTE,
    );
}

#[test]
fn a_css_path_segment_refuses_naming_the_rename() {
    // The `::` seam recovers OVER the word rather than rolling the `::` back:
    // rolled back, the failure surfaces at the operator as a missing `;` and
    // the word the reader has to change is never named.
    assert_fails_with(
        r#"
        import std::style::{ Length, style };
        let _x = const style().left(Length::css("1px"));
        fun main() {}
        main();
        "#,
        CSS_RENAME_NOTE,
    );
}

#[test]
fn a_binding_named_css_refuses_naming_the_rename() {
    assert_fails_with(
        r#"
        import std::print;
        fun main() {
            let css = 1;
            print(css);
        }
        main();
        "#,
        CSS_RENAME_NOTE,
    );
}

#[test]
fn a_struct_field_named_css_refuses_naming_the_rename() {
    // A struct body whose first token is the keyword commits to nothing, so
    // nothing inside is noted and the delimiter recovery would otherwise name
    // the struct body instead of the word.
    assert_fails_with(
        r#"
        struct Token {
            css: str,
        }
        fun main() {}
        main();
        "#,
        CSS_RENAME_NOTE,
    );
}

#[test]
fn a_struct_initializer_field_named_css_refuses_naming_the_rename() {
    assert_fails_with(
        r#"
        struct Token {
            value: str,
        }
        fun main() {
            let _token = Token { css = "x" };
        }
        main();
        "#,
        CSS_RENAME_NOTE,
    );
}

#[test]
fn the_renamed_length_surface_is_what_compiles_instead() {
    // The other half of the promotion: the two names it minted both work, and
    // `raw` is the same verbatim value `css` was — no wrapper, unlike `calc`.
    let css = style_css(
        r#"
        import std::style::{ style, space, Length };
        let _s = const style()
            .left(Length::raw("clamp(120px, 30%, 185px)"))
            .width(Length::raw(space(4).text));
        fun main() {}
        main();
        "#,
    );
    assert!(css.contains("{left:clamp(120px, 30%, 185px)}"), "{css}");
    assert!(css.contains("{width:var(--space-4)}"), "{css}");
}

// --- K2d: the `css` block, S2 (proposal/css-block.md §5, §11) -----------------
// CSS-shaped sugar over the `style()` chain, lowered before analysis. There is
// no third emission channel and no new emitter code — `Style::rule` is still
// the one chokepoint — and the arc's HEADLINE GATE is what that buys: the same
// program written as a block and as the chain it desugars to emits byte-
// identical CSS and byte-identical JS. The tree-level half of the same claim
// (the desugar builds the very node shapes a written chain parses to) is pinned
// beside the pass, in `vilan_core::css`'s own tests.

/// The exhibit, in both spellings. `{TWIN}` is substituted with the body so the
/// two programs differ in NOTHING but the style's spelling — a stray difference
/// elsewhere would make the byte comparison pass for the wrong reason.
const TWIN_PROGRAM: &str = r#"
    import std::print;
    import std::style::{ Color, Length, Style, space, style };
    fun card(): Style {
        {TWIN}
    }
    fun main() {
        print(const card().class_list());
    }
    main();
"#;

const TWIN_BLOCK: &str = r#"css {
            display: flex;
            gap: {space(4)};
            padding: {space(4)};
            background-color: {Color::gray(50)};
            border-radius: {Length::px(8)};
            grid-template-columns: repeat(3, 1fr);
            .md {
                padding: {space(6)};
            }
            .hover {
                background-color: {Color::gray(100)};
            }
            .within("data-theme", "dark") {
                color: {Color::gray(50)};
            }
            .children {
                margin-top: {space(2)};
            }
        }"#;

const TWIN_CHAIN: &str = r#"style()
            .raw("display", "flex")
            .raw("gap", space(4))
            .raw("padding", space(4))
            .raw("background-color", Color::gray(50))
            .raw("border-radius", Length::px(8))
            .raw("grid-template-columns", "repeat(3, 1fr)")
            .md(style().raw("padding", space(6)))
            .hover(style().raw("background-color", Color::gray(100)))
            .within("data-theme", "dark", style().raw("color", Color::gray(50)))
            .children(style().raw("margin-top", space(2)))"#;

fn twin(spelling: &str) -> String {
    TWIN_PROGRAM.replace("{TWIN}", spelling)
}

#[test]
fn a_css_block_emits_byte_identical_css_against_the_chain() {
    // Class names are content hashes of the slot key and the declaration, so
    // this is not a weak check: a block that changed one declaration, one
    // selector or one condition moves a hash and the sheets diverge.
    let block = style_css(&twin(TWIN_BLOCK));
    let chain = style_css(&twin(TWIN_CHAIN));
    assert_eq!(block, chain);
    // Non-vacuity: the sheet is real, not two empty strings.
    assert!(block.contains("{display:flex}"), "{block}");
    assert!(block.contains("@layer vilan{"), "{block}");
    assert!(block.contains("[data-theme=\"dark\"] "), "{block}");
    assert!(block.contains("@media (min-width: 768px)"), "{block}");
}

#[test]
fn a_css_block_emits_byte_identical_js_against_the_chain() {
    // The stylesheet is an over-approximation — every rule a chain builds is
    // emitted, including one a later link overrides — so the CSS gate alone
    // would not catch a lowering that resolved a different SLOT. The whole
    // emitted module must match to the byte, and the const-folded class list
    // in it is exactly which slots survived, in which order.
    let block = compile(&twin(TWIN_BLOCK)).expect("the block spelling compiles");
    let chain = compile(&twin(TWIN_CHAIN)).expect("the chain spelling compiles");
    assert_eq!(block, chain);
    // Non-vacuity: the fold really happened and really resolved ten slots.
    let folded = block
        .lines()
        .find(|line| line.starts_with("console.log(\""))
        .unwrap_or_else(|| panic!("no folded class list in:\n{block}"));
    assert_eq!(
        folded.split(' ').count(),
        10,
        "ten slots survive the merge: {folded}"
    );
}

#[test]
fn a_css_block_is_an_ordinary_expression() {
    // It evaluates to a `Style`, so `+` still combines and last wins — the
    // §1.2 requirement, that the form compose natively, met by lowering to the
    // chain that composes natively.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::style::{ style, space };
        fun main() {
            let base = const css { padding: {space(4)}; };
            let wider = const css { padding: {space(6)}; };
            print((base + wider).class_list());
        }
        main();
        "#,
        "s1ufvsw\n",
    );
}

#[test]
fn a_one_hole_value_carries_its_tokens_root_line() {
    // The row that keeps a `Length` a `Length`. Reaching for `.text` instead
    // would put a `var()` on the sheet that nothing declares — the hazard S1
    // closed, restated for the block.
    let css = style_css(
        r#"
        import std::style::{ style, space };
        let _s = const css { gap: {space(4)}; };
        fun main() {}
        main();
        "#,
    );
    assert!(css.contains("{gap:var(--space-4)}"), "{css}");
    assert!(css.contains(":root{--space-4:1rem}"), "{css}");
}

#[test]
fn a_hole_free_value_is_its_own_source_slice() {
    // A value is a TOKEN RUN, not a typed grammar: commas, parens and a quoted
    // string ride through verbatim, and the quotes survive the round trip into
    // the emitted stylesheet.
    let css = style_css(
        r#"
        import std::style::style;
        let _s = const css {
            grid-template-columns: repeat(3, 1fr);
            background-image: url("tile.png");
            width: 50%;
            line-height: 1.5;
        };
        fun main() {}
        main();
        "#,
    );
    assert!(
        css.contains("{grid-template-columns:repeat(3, 1fr)}"),
        "{css}"
    );
    assert!(
        css.contains("{background-image:url(\"tile.png\")}"),
        "{css}"
    );
    assert!(css.contains("{width:50%}"), "{css}");
    assert!(css.contains("{line-height:1.5}"), "{css}");
}

#[test]
fn a_custom_property_is_span_adjacency_and_nothing_new() {
    let css = style_css(
        r#"
        import std::style::{ style, Color };
        let _s = const css { --brand-ink: {Color::gray(900)}; };
        fun main() {}
        main();
        "#,
    );
    assert!(css.contains("{--brand-ink:var(--gray-900)}"), "{css}");
    assert!(css.contains(":root{--gray-900:#111827}"), "{css}");
}

#[test]
fn nested_rules_lower_to_the_shipped_relation_combinators() {
    // The desugar is NAME-BLIND: a dotted head is always a method call with
    // the block's own chain last, so every combinator that exists works on the
    // day it ships and the grammar never consults `Style`'s method list.
    let css = style_css(
        r#"
        import std::style::{ style, space, Color };
        let _s = const css {
            .within("data-theme", "dark") {
                color: {Color::gray(50)};
            }
            .children {
                margin-top: {space(2)};
            }
            .divide {
                margin-top: {space(4)};
            }
        };
        fun main() {}
        main();
        "#,
    );
    assert!(css.contains("[data-theme=\"dark\"] "), "{css}");
    assert!(
        css.contains("@layer vilan{") && css.contains(" > *{"),
        "{css}"
    );
    assert!(css.contains(" > :not(:first-child){"), "{css}");
}

#[test]
fn nesting_order_is_combinator_order() {
    // Textual nesting IS the outside-in call order the model requires —
    // media, then the relation, then the attribute, then the pseudo-class —
    // so the shape that is legal is the shape that reads correctly.
    let css = style_css(
        r#"
        import std::style::{ style, Color };
        let _s = const css {
            .md {
                .within("data-theme", "dark") {
                    .attribute("data-open", "true") {
                        .hover {
                            color: {Color::gray(50)};
                        }
                    }
                }
            }
        };
        fun main() {}
        main();
        "#,
    );
    assert!(
        css.contains("@media (min-width: 768px){[data-theme=\"dark\"] ")
            && css.contains("[data-open=\"true\"]:hover{color:var(--gray-50)}"),
        "{css}"
    );
}

#[test]
fn a_misnested_condition_still_refuses_by_name() {
    // The block does not add validation — the const-time fences are the
    // chain's own, reached through the same calls.
    assert_run_panics(
        r#"
        import std::style::{ style, Color };
        let _s = const css {
            .hover {
                .md {
                    color: {Color::gray(50)};
                }
            }
        };
        fun main() {}
        main();
        "#,
        "cannot wrap a media-conditioned style",
    );
}

#[test]
fn a_macro_generated_css_block_desugars() {
    // The pass runs at every parse entry, `parse_generated` included, so a
    // block emitted by a macro lowers like a hand-written one — the coverage
    // elements had to have.
    assert_compiles_and_runs(
        r#"
        import std::print;
        import std::style::{ style, space };
        fun main() {
            let made = macro {
                import macro_std::source;
                source("const css { padding: {space(4)}; .hover { color: red; } }")
            };
            print(made.class_list());
        }
        main();
        "#,
        "s1ufvr2 sh41dyk\n",
    );
}

#[test]
fn a_css_block_inside_markup_desugars() {
    // A block can sit in an element's head, so the css pass descends into
    // markup itself — it runs BEFORE the element desugar, and a block left
    // there would reach the analyzer as a `Node::Css`.
    assert_compiles(
        r#"
        import std::ui::{ View, view };
        import std::style::{ style, space };
        fun main() {
            let _card = <div .styled(const css { padding: {space(4)}; }) />;
        }
        "#,
    );
}

// The refusals, each with its fix named (§4.1, §7.3, §10).

#[test]
fn a_bare_hex_colour_refuses_naming_the_hole() {
    // `#` is in no charset, so this is a LEX error and cannot be anything
    // else: lexing is context-free by spec and finishes before the parser
    // exists. The mitigation is the `UNESCAPED_BRACE` precedent — a rule code
    // on the `LexError` naming the vilan spelling.
    assert_fails_with(
        r#"
        import std::style::style;
        let _s = const css { color: #333; };
        fun main() {}
        main();
        "#,
        "Color::hex",
    );
}

#[test]
fn an_at_rule_refuses_naming_the_breakpoint_combinator() {
    assert_fails_with(
        r#"
        import std::style::style;
        let _s = const css { @media (min-width: 768px) { color: red; } };
        fun main() {}
        main();
        "#,
        "a media query is a breakpoint combinator",
    );
}

#[test]
fn important_refuses_permanently_and_says_why() {
    assert_fails_with(
        r#"
        import std::style::style;
        let _s = const css { color: red !important; };
        fun main() {}
        main();
        "#,
        "`!important` has no place in a `css` block",
    );
}

#[test]
fn a_missing_terminator_asks_for_the_semicolon() {
    // The `;` is required after every declaration, including the last: the
    // formatter may never invent a token, and a required terminator makes
    // value scanning decidable in one pass.
    assert_fails_with(
        r#"
        import std::style::style;
        let _s = const css { color: red };
        fun main() {}
        main();
        "#,
        "expected `;` to end this statement",
    );
}

#[test]
fn a_block_in_condition_position_asks_for_parentheses() {
    // Unlike an element — which begins with a byte no expression could start
    // — a `css` block is BRACE-INITIAL, so it is suppressed where a struct
    // literal is, and takes the same escape hatch.
    assert_fails_with(
        r#"
        import std::print;
        fun main() {
            if css { color: red; } { print("x"); }
        }
        main();
        "#,
        "parenthesize it",
    );
}

#[test]
fn a_parenthesized_block_is_admitted_in_a_condition() {
    // The other half of the rule: the escape hatch works.
    assert_compiles(
        r#"
        import std::print;
        import std::style::style;
        fun main() {
            if (const css { color: red; }).class_list() != "" {
                print("styled");
            }
        }
        "#,
    );
}

#[test]
fn a_block_without_style_in_scope_fails_at_the_css_keyword() {
    // The one generated accessor S2 gave a REAL span, and this is what the
    // span was kept for (§7.3): the block lowers to `style()`, so a missing
    // `import std::style::style` fails on the generated accessor — and the
    // squiggle lands on the word that asked for a `Style`, not on a
    // zero-width anchor somewhere inside the block.
    assert_fails_spanning(
        r#"
        fun main() {
            let _s = const css { display: flex; };
        }
        "#,
        "css",
        "cannot find 'style' in this scope",
    );
    // S4's tailored note. The generic report is honest but disjointed — `css`
    // underlined, `style` in the message, nothing drawing the line — so the
    // note says which is which, on the element-syntax precedent.
    assert_fails_noting(
        r#"
        fun main() {
            let _s = const css { display: flex; };
        }
        "#,
        "cannot find 'style' in this scope",
        "css",
        "a `css { … }` block lowers to a std::style::style chain",
    );
}

#[test]
fn a_hand_written_style_accessor_gets_no_css_note() {
    // The note's gate is the SPAN reading `css`, so it cannot fire on an
    // ordinary unresolved `style` — which would be a note about a construct
    // the author never wrote.
    assert_fails_without(
        r#"
        fun main() {
            let _s = const style().raw("display", "flex");
        }
        "#,
        "a `css { … }` block lowers to",
    );
}
