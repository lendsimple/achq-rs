//! Development tasks: `cargo xtask <task>`.

mod conformance;
mod drift;
mod industry;
mod overrides;
mod record;
mod sources;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use overrides::Overrides;
use sources::{Kind, SOURCES};

const USAGE: &str = "usage: cargo xtask <task>

tasks:
    codegen [--check]   generate achq/src/onboarding/industry.rs from
                        schema/industry-categories.md
                        (--check: fail if the committed file is out of date)
    conformance         check the request types against schema/gateway/*.json
                        and schema/overrides.toml
    fetch               download the documentation's pages over schema/,
                        updating schema/README.md
    drift               list what differs between schema/ and the pages the
                        documentation serves, and any new reference pages
    record              run every command against ACHQ's public sandbox and keep
                        the raw responses in achq/tests/data/recorded/";

/// The generated file, relative to the workspace root.
const INDUSTRY_RS: &str = "achq/src/onboarding/industry.rs";

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["codegen"] => codegen(false),
        ["codegen", "--check"] => codegen(true),
        ["conformance"] => conformance(),
        ["fetch"] => fetch(),
        ["drift"] => drift(),
        ["record"] => record::run(&workspace_root()),
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::FAILURE;
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask is inside the workspace")
        .to_path_buf()
}

fn schema_dir() -> PathBuf {
    workspace_root().join("schema")
}

fn read(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(|e| format!("reading {}: {e}", path.display()).into())
}

fn load_overrides() -> Result<Overrides> {
    Ok(Overrides::parse(&read(
        &schema_dir().join("overrides.toml"),
    )?)?)
}

/// The generated industry table.
fn render_industry() -> Result<String> {
    let markdown = read(&schema_dir().join("industry-categories.md"))?;
    let rows = industry::parse(&markdown).map_err(|e| format!("industry-categories.md: {e}"))?;
    Ok(industry::render(&rows, &load_overrides()?.industry.names)?)
}

fn codegen(check: bool) -> Result<()> {
    let rendered = [(INDUSTRY_RS, render_industry()?)];
    let root = workspace_root();
    let stale = stale_files(&rendered, &root);
    if check {
        if !stale.is_empty() {
            return Err(format!(
                "generated code is out of date; run `cargo xtask codegen`:\n  {}",
                stale.join("\n  ")
            )
            .into());
        }
        return Ok(());
    }
    for (name, contents) in &rendered {
        if stale.contains(name) {
            let path = root.join(name);
            std::fs::write(&path, contents)?;
            println!("wrote {}", path.display());
        }
    }
    Ok(())
}

/// The vendored gateway fragments, by operation.
fn load_fragments() -> Result<BTreeMap<String, conformance::Fragment>> {
    let mut fragments = BTreeMap::new();
    for source in SOURCES {
        if let Some(op) = source.gateway_op() {
            let path = schema_dir().join(source.file);
            let fragment = conformance::Fragment::parse(&read(&path)?)
                .map_err(|e| format!("{}: {e}", path.display()))?;
            fragments.insert(op.to_owned(), fragment);
        }
    }
    Ok(fragments)
}

fn conformance() -> Result<()> {
    let fragments = load_fragments()?;
    let overrides = load_overrides()?;
    let registry = conformance::registry();
    let problems = conformance::check(&fragments, &registry, &overrides);
    if !problems.is_empty() {
        return Err(format!(
            "the request types differ from the fragments:\n  {}",
            problems.join("\n  ")
        )
        .into());
    }
    println!(
        "{} gateway operations checked, {} not yet registered",
        registry.len(),
        overrides.unregistered.len()
    );
    Ok(())
}

/// Downloads a source and cuts out its vendored content.
fn fetch_source(source: &sources::Source) -> Result<String> {
    let url = source.url();
    let page = sources::download(&url)?;
    let content = sources::extract(source.kind, &page).map_err(|e| format!("{url}: {e}"))?;
    if source.kind == Kind::Markdown {
        industry::parse(&content).map_err(|e| format!("{url}: {e}"))?;
    }
    Ok(content)
}

fn fetch() -> Result<()> {
    let dir = schema_dir();
    let readme_path = dir.join("README.md");
    let mut readme = read(&readme_path)?;
    for source in SOURCES {
        let live = fetch_source(source)?;
        let path = dir.join(source.file);
        if std::fs::read_to_string(&path).ok().as_deref() == Some(live.as_str()) {
            println!("{} is up to date", source.file);
            continue;
        }
        std::fs::create_dir_all(path.parent().expect("under schema/"))?;
        std::fs::write(&path, &live)?;
        readme = drift::update_readme(
            &readme,
            source.file,
            &drift::today(),
            &drift::sha256_hex(live.as_bytes()),
        )?;
        println!("wrote {}", path.display());
    }
    std::fs::write(&readme_path, readme)?;
    Ok(())
}

fn drift() -> Result<()> {
    let dir = schema_dir();
    let mut clean = true;
    for source in SOURCES {
        let live = fetch_source(source)?;
        let vendored = read(&dir.join(source.file))?;
        if let Some(report) = drift::report(source.kind, &vendored, &live)? {
            clean = false;
            print!("{}:\n{report}", source.file);
        }
    }
    let index = sources::download(sources::INDEX_URL)?;
    for page in sources::reference_pages(&index) {
        let vendored = SOURCES.iter().any(|s| s.page == page);
        if !vendored && !sources::NOT_API.contains(&page) {
            clean = false;
            println!("new reference page: {page}");
        }
    }
    if clean {
        println!("schema/ matches the documentation");
    }
    Ok(())
}

/// The names of the rendered files whose committed copy under `root` differs.
fn stale_files<'a>(rendered: &[(&'a str, String)], root: &Path) -> Vec<&'a str> {
    rendered
        .iter()
        .filter(|(name, contents)| {
            std::fs::read_to_string(root.join(name)).ok().as_ref() != Some(contents)
        })
        .map(|(name, _)| *name)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_files_finds_missing_and_edited_files() {
        let dir = std::env::temp_dir().join(format!("achq-xtask-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let rendered = [("a.rs", "a\n".to_owned()), ("b.rs", "b\n".to_owned())];
        assert_eq!(stale_files(&rendered, &dir), ["a.rs", "b.rs"]);
        std::fs::write(dir.join("a.rs"), "a\n").unwrap();
        std::fs::write(dir.join("b.rs"), "b\n").unwrap();
        assert!(stale_files(&rendered, &dir).is_empty());
        std::fs::write(dir.join("b.rs"), "b, edited by hand\n").unwrap();
        assert_eq!(stale_files(&rendered, &dir), ["b.rs"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_industry_table_parses() {
        let markdown = read(&schema_dir().join("industry-categories.md")).unwrap();
        let rows = industry::parse(&markdown).unwrap();
        assert_eq!(rows.len(), 150);
        let categories: std::collections::BTreeSet<&str> =
            rows.iter().map(|r| r.category.as_str()).collect();
        assert_eq!(categories.len(), 13);
        render_industry().unwrap();
    }

    #[test]
    fn every_gateway_fragment_loads() {
        let fragments = load_fragments().unwrap();
        assert_eq!(fragments.len(), 12);
        for (op, fragment) in &fragments {
            assert!(!fragment.properties.is_empty(), "{op} has no properties");
        }
        assert_eq!(
            fragments["cancelpayment"].properties,
            BTreeMap::from([("Transact_ReferenceID".to_owned(), true)])
        );
    }

    #[test]
    fn the_onboarding_fragments_share_their_error_response() {
        let load = |file: &str| -> serde_json::Value {
            serde_json::from_str(&read(&schema_dir().join(file)).unwrap()).unwrap()
        };
        let merchant = load("onboarding/merchant-onboarding.json");
        let fees = load("onboarding/post_onboarding-fees.json");
        let error =
            |spec: &serde_json::Value| spec["components"]["schemas"]["ErrorResponse"].clone();
        assert!(error(&merchant).is_object());
        assert_eq!(error(&merchant), error(&fees));
    }

    #[test]
    fn the_request_types_conform() {
        let problems = conformance::check(
            &load_fragments().unwrap(),
            &conformance::registry(),
            &load_overrides().unwrap(),
        );
        assert!(problems.is_empty(), "{}", problems.join("\n"));
    }

    #[test]
    fn every_gateway_fragment_is_registered() {
        assert!(load_overrides().unwrap().unregistered.is_empty());
        let registry = conformance::registry();
        assert_eq!(registry.len(), 12);
        for (op, fields) in &registry {
            assert!(!fields.is_empty(), "{op} writes no fields");
        }
    }

    #[test]
    fn the_readme_lists_every_source() {
        let readme = read(&schema_dir().join("README.md")).unwrap();
        for source in SOURCES {
            assert!(
                readme.contains(&format!("| `{}` |", source.file)),
                "schema/README.md has no row for {}",
                source.file
            );
        }
    }
}
