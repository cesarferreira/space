#![cfg(unix)]

use space::{
    classify::{Clean, Rule, project_rule, slug},
    model::{Confidence, Reclaimability},
    scan::{ProjectKind, find_projects, measure},
};
use std::{fs, io::Cursor, path::PathBuf};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("space-clean-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn rule(name: &'static str, path: PathBuf, clean: Clean) -> Rule {
    Rule {
        path,
        category: "Developer",
        name,
        reclaimability: Reclaimability::Cache,
        confidence: Confidence::High,
        what: "test",
        safety: "test safety",
        clean,
    }
}

fn write(path: PathBuf, bytes: usize) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, vec![1; bytes]).unwrap();
}

fn clean(rules: &[Rule], selection: &str, yes: bool, answer: &str) -> (anyhow::Result<()>, String) {
    let mut out = Vec::new();
    let result = space::clean::run(
        rules,
        selection,
        false,
        None,
        yes,
        true,
        &mut Cursor::new(answer),
        &mut out,
    );
    (result, String::from_utf8(out).unwrap())
}

#[test]
fn slugs_are_lowercase_and_dashed() {
    assert_eq!(slug("Go modules"), "go-modules");
    assert_eq!(slug(" npm "), "npm");
    assert_eq!(slug("node_modules"), "node_modules");
}

#[test]
fn nested_rules_are_not_counted_twice() {
    let root = temp_dir("nested");
    write(root.join("cache/app/blob"), 10_000);
    write(root.join("cache/pip/wheel"), 50_000);
    let rules = vec![
        rule("Caches", root.join("cache"), Clean::Contents),
        rule("pip", root.join("cache/pip"), Clean::Contents),
    ];
    let sizes = measure(&rules, None);
    let caches = sizes.iter().find(|(i, _)| *i == 0).unwrap().1.bytes;
    let pip = sizes.iter().find(|(i, _)| *i == 1).unwrap().1.bytes;
    assert!(pip >= 50_000);
    assert!((10_000..50_000).contains(&caches), "caches={caches}");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cleaning_contents_keeps_the_folder_and_nested_rules() {
    let root = temp_dir("contents");
    write(root.join("cache/app/blob"), 10);
    write(root.join("cache/loose"), 10);
    write(root.join("cache/huggingface/model"), 10);
    let mut models = rule("Hugging Face", root.join("cache/huggingface"), Clean::Never);
    models.category = "AI";
    let rules = vec![rule("Caches", root.join("cache"), Clean::Contents), models];
    let (result, out) = clean(&rules, "caches", false, "y\n");
    result.unwrap();
    assert!(out.contains("Will clean:"), "{out}");
    assert!(out.contains("cleaned Caches"), "{out}");
    assert!(root.join("cache").is_dir());
    assert!(!root.join("cache/app").exists());
    assert!(!root.join("cache/loose").exists());
    assert!(root.join("cache/huggingface/model").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn declining_deletes_nothing() {
    let root = temp_dir("decline");
    write(root.join("gradle/x"), 10);
    let rules = vec![rule("Gradle", root.join("gradle"), Clean::Contents)];
    let (result, out) = clean(&rules, "gradle", false, "n\n");
    result.unwrap();
    assert!(out.contains("Aborted"), "{out}");
    assert!(root.join("gradle/x").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn non_interactive_without_yes_refuses() {
    let root = temp_dir("refuse");
    write(root.join("gradle/x"), 10);
    let rules = vec![rule("Gradle", root.join("gradle"), Clean::Contents)];
    let mut out = Vec::new();
    let error = space::clean::run(
        &rules,
        "gradle",
        false,
        None,
        false,
        false,
        &mut Cursor::new("y\n"),
        &mut out,
    )
    .unwrap_err();
    assert!(error.to_string().contains("--yes"));
    assert!(root.join("gradle/x").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unknown_and_protected_names_are_rejected() {
    let root = temp_dir("reject");
    fs::create_dir_all(root.join("docs")).unwrap();
    let mut docs = rule("Documents", root.join("docs"), Clean::Never);
    docs.category = "Personal";
    let rules = vec![docs, rule("Gradle", root.join("gradle"), Clean::Contents)];
    let (result, _) = clean(&rules, "nonsense", true, "");
    let message = result.unwrap_err().to_string();
    assert!(
        message.contains("Choose from: developer, gradle"),
        "{message}"
    );
    let (result, _) = clean(&rules, "documents", true, "");
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("does not clean Documents")
    );
    assert!(root.join("docs").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_folders_are_found_and_deleted_together() {
    let root = temp_dir("projects");
    write(root.join("web/package.json"), 2);
    write(root.join("web/node_modules/react/index.js"), 100);
    write(root.join("web/node_modules/dep/node_modules/x/i.js"), 100);
    write(root.join("api/package.json"), 2);
    write(root.join("api/node_modules/a.js"), 100);
    write(root.join("tool/Cargo.toml"), 2);
    write(root.join("tool/target/CACHEDIR.TAG"), 2);
    write(root.join("tool/target/debug/bin"), 100);
    // A folder named target without Cargo's marker is not build output.
    write(root.join("site/Cargo.toml"), 2);
    write(root.join("site/target/keep"), 1);
    // No package.json: not a project.
    write(root.join("loose/node_modules/keep"), 1);
    // Hidden folders are skipped.
    write(root.join(".hidden/package.json"), 2);
    write(root.join(".hidden/node_modules/keep"), 1);

    let (found, timed_out) = find_projects(std::slice::from_ref(&root), &[], None);
    assert!(!timed_out);
    let mut found: Vec<_> = found
        .into_iter()
        .map(|(p, k)| (p.strip_prefix(&root).unwrap().to_path_buf(), k))
        .collect();
    found.sort();
    assert_eq!(
        found,
        vec![
            (PathBuf::from("api/node_modules"), ProjectKind::NodeModules),
            (PathBuf::from("tool/target"), ProjectKind::RustTarget),
            (PathBuf::from("web/node_modules"), ProjectKind::NodeModules),
        ]
    );

    let rules: Vec<Rule> = found
        .into_iter()
        .map(|(p, k)| project_rule(root.join(p), k))
        .collect();
    let (result, out) = clean(&rules, "node_modules", true, "");
    result.unwrap();
    assert!(out.contains("delete 2 folders"), "{out}");
    assert!(!root.join("web/node_modules").exists());
    assert!(!root.join("api/node_modules").exists());
    assert!(root.join("web/package.json").exists());
    assert!(root.join("tool/target").exists());
    assert!(root.join("loose/node_modules/keep").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn project_search_respects_the_deadline() {
    let root = temp_dir("deadline");
    write(root.join("a/package.json"), 2);
    write(root.join("a/node_modules/x"), 2);
    let (found, timed_out) = find_projects(
        std::slice::from_ref(&root),
        &[],
        Some(std::time::Instant::now()),
    );
    assert!(timed_out);
    assert!(found.is_empty());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn missing_tools_are_reported_not_hidden() {
    let root = temp_dir("command");
    fs::create_dir_all(root.join("store")).unwrap();
    let rules = vec![rule(
        "Fake",
        root.join("store"),
        Clean::Command(&["space-test-tool-that-does-not-exist"]),
    )];
    let (result, out) = clean(&rules, "fake", true, "");
    assert!(result.is_err());
    assert!(out.contains("is not installed"), "{out}");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn why_explains_a_category_with_its_largest_entries() {
    let root = temp_dir("why");
    write(root.join("gradle/modules-2/big"), 30_000);
    write(root.join("gradle/small"), 10);
    let rules = vec![rule("Gradle", root.join("gradle"), Clean::Contents)];
    let text = space::explain::category(&rules, "Gradle", 1, None).unwrap();
    assert!(text.starts_with("Gradle  "), "{text}");
    assert!(text.contains("Developer · cache"), "{text}");
    assert!(text.contains("Largest entries\n  modules-2"), "{text}");
    assert!(!text.contains("small"), "{text}");
    assert!(text.contains("space --clean gradle"), "{text}");
    assert!(space::explain::category(&rules, "tmpfs", 1, None).is_none());
    fs::remove_dir_all(root).unwrap();
}
