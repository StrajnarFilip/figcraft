//! A well-known UI family that isn't installed draws in a close stand-in at the style asked for
//! (#1237), not in the generic fallback's regular face. Synthetic pending entries (no files) for
//! the choice of stand-in; the embedded faces, when the build has them, for the face drawn.

use std::sync::Arc;

use super::{CRAFT_FONTS, Face, FontBytes, FontDb, Pending, stand_ins};

/// A pending (indexed, never read) face of `style`.
fn entry(style: &str) -> Pending {
    Pending { path: format!("no-such-dir/{style}.ttf").into(), index: 0, style: style.into(), postscript: None, local: None }
}

/// A database that has indexed `families`, each with its styles.
fn installed(families: &[(&str, &[&str])]) -> FontDb {
    let mut db = FontDb::default();
    for (family, styles) in families {
        db.pending.insert(family.to_lowercase(), ((*family).into(), styles.iter().map(|s| entry(s)).collect()));
    }
    db
}

/// The embedded faces of `family` added to a fresh database, or `None` when the build has none.
fn embedded(families: &[&str]) -> Option<FontDb> {
    let mut db = FontDb::default();
    let mut any = false;
    for f in CRAFT_FONTS.iter().filter(|f| families.contains(&f.family)) {
        if let Some(face) = Face::from_bytes(FontBytes::Static(f.bytes), 0, Some(f.family), Some(f.style)) {
            db.add(face);
            any = true;
        }
    }
    any.then_some(db)
}

/// The table maps the families the issue names (and their other spellings) to their stand-ins, and nothing else.
#[test]
fn the_table_maps_apple_google_mono_and_euclid_families_whatever_the_spelling() {
    assert_eq!(stand_ins("SF Pro").first(), Some(&"Inter"));
    assert_eq!(stand_ins("  sf pro display ").first(), Some(&"Inter"), "case and outer spaces don't matter");
    assert_eq!(stand_ins(".SF NS Text").first(), Some(&"Inter"), "Apple's hidden names with their dot");
    assert_eq!(stand_ins("-apple-system").first(), Some(&"Inter"), "CSS names for the system font");
    assert_eq!(stand_ins("Google Sans").first(), Some(&"Roboto"));
    assert_eq!(stand_ins("Product Sans").first(), Some(&"Roboto"));
    assert_eq!(stand_ins("Euclid Circular A").first(), Some(&"DM Sans"));
    for mono in ["Menlo", "SF Mono", "Monaco", "Consolas"] {
        assert!(stand_ins(mono).contains(&"Liberation Mono"), "{mono} has the monospaced stand-ins");
        assert!(!stand_ins(mono).contains(&"Inter"), "{mono} never stands in with a proportional face");
    }
    assert!(stand_ins("Synthetic Grotesk").is_empty(), "other families keep the generic fallback");
    assert!(stand_ins("").is_empty());
}

/// The stand-in named is the first of the list this machine has, and only while the family itself is missing.
#[test]
fn the_first_installed_stand_in_is_named_only_while_the_family_is_missing() {
    let db = installed(&[("Inter", &["Regular", "Bold"]), ("Liberation Mono", &["Regular"])]);
    assert_eq!(db.stand_in("SF Pro").as_deref(), Some("Inter"));
    assert_eq!(db.stand_in("Menlo").as_deref(), Some("Liberation Mono"), "the first of the list this machine has");
    assert_eq!(db.stand_in("Google Sans").as_deref(), Some("Inter"), "Roboto isn't there: the next one");
    assert_eq!(db.stand_in("Synthetic Grotesk"), None, "no stand-in for other families");
    // The family itself installed: nothing stands in for it.
    let db = installed(&[("SF Pro", &["Regular"]), ("Inter", &["Regular"])]);
    assert_eq!(db.stand_in("SF Pro"), None);
    // No stand-in installed: the generic fallback draws it.
    let db = installed(&[("Synthetic Grotesk", &["Regular"])]);
    assert_eq!(db.stand_in("Menlo"), None);
}

/// A missing family draws in its stand-in's face nearest the style asked for (Inter Bold for SF Pro Bold).
#[test]
fn a_missing_family_draws_in_its_stand_in_at_the_style_asked_for() {
    let Some(mut db) = embedded(&["Inter"]) else { return };
    let Some(bold) = db.find("Inter", "Bold").filter(|f| f.style.eq_ignore_ascii_case("Bold")) else { return };
    assert!(db.find("SF Pro", "Bold").is_none(), "SF Pro isn't installed here");
    let drawn = db.find_or_stand_in("SF Pro", "Bold").expect("Inter stands in");
    assert!(Arc::ptr_eq(&drawn, &bold), "Inter Bold, not the fallback's regular face: {} {}", drawn.family, drawn.style);
    // An installed family is drawn in itself.
    let regular = db.find("Inter", "Regular").expect("Inter Regular");
    assert!(Arc::ptr_eq(&db.find_or_stand_in("Inter", "Regular").expect("Inter"), &regular));
    // A family with no stand-in has none here: the caller's generic fallback draws it.
    assert!(db.find_or_stand_in("Synthetic Grotesk", "Bold").is_none());
}

/// Menlo's text draws in a monospaced stand-in, never in proportional Inter.
#[test]
fn menlo_draws_monospaced() {
    let Some(mut db) = embedded(&["Inter", "Liberation Mono"]) else { return };
    let Some(mono) = db.find_or_stand_in("Menlo", "Regular") else { return };
    assert_eq!(mono.family, "Liberation Mono", "Menlo's text keeps a monospaced face, not Inter");
}
