//! A `Text` becomes a sentence: arguments, plurals, attributes, a miss.

use bisa_core::text;
use bisa_i18n::{Catalog, Locale};

const FTL: &str = r#"
-bisa = Bisa
greeting = Welcome to { -bisa }, { $name }.
needs = { $n ->
    [0] Nothing needs you
    [one] 1 needs you
   *[other] { $n } need you
}
setting-appearance-theme = Theme
    .help = The family the window wears.
    .choice-glass = Glass
"#;

fn catalog() -> Catalog {
    Catalog::from_sources("en", &[FTL]).expect("the test catalog parses")
}

#[test]
fn arguments_ride_through_and_a_plural_is_the_message_s_choice() {
    let c = catalog();
    let en = Locale::english();
    assert_eq!(
        c.render(&en, &text!("greeting", name = "Ada")),
        "Welcome to Bisa, Ada."
    );
    assert_eq!(c.render(&en, &text!("needs", n = 0)), "Nothing needs you");
    assert_eq!(c.render(&en, &text!("needs", n = 1)), "1 needs you");
    assert_eq!(c.render(&en, &text!("needs", n = 3)), "3 need you");
}

#[test]
fn no_isolation_marks_wrap_a_placeable() {
    let c = catalog();
    let out = c.render(&Locale::english(), &text!("greeting", name = "Ada"));
    assert!(
        !out.contains('\u{2068}') && !out.contains('\u{2069}'),
        "{out:?}"
    );
}

#[test]
fn an_attribute_is_read_and_a_missing_message_renders_as_its_id() {
    let c = catalog();
    let en = Locale::english();
    let t = text!("setting-appearance-theme");
    assert_eq!(c.render(&en, &t), "Theme");
    assert_eq!(
        c.attribute(&en, &t.id, "help", &t).as_deref(),
        Some("The family the window wears.")
    );
    assert_eq!(
        c.attribute(&en, &t.id, "choice-glass", &t).as_deref(),
        Some("Glass")
    );
    assert_eq!(c.attribute(&en, &t.id, "choice-dune", &t), None);
    assert_eq!(
        c.render(&en, &text!("nothing-by-this-id", n = 1)),
        "nothing-by-this-id"
    );
    assert!(c.has(&en, "needs") && !c.has(&en, "nothing-by-this-id"));
}

#[test]
fn a_language_not_shipped_falls_back_to_english_and_the_shipped_catalog_builds() {
    let c = catalog();
    let fr = Locale::negotiate(&["fr"]);
    assert_eq!(fr.tag(), "en");
    assert_eq!(c.render(&fr, &text!("needs", n = 2)), "2 need you");
    let shipped = Catalog::shipped().expect("locales/en/*.ftl parse");
    assert_eq!(bisa_i18n::english(&text!("not-a-message")), "not-a-message");
    assert!(!shipped.has(&Locale::english(), "not-a-message"));
}
