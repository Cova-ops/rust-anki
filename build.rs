use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    add_swift_runtime_rpaths();

    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR should be set"));
    let migrations_dir = manifest_dir.join("migrations");
    println!("cargo:rerun-if-changed={}", migrations_dir.display());

    let mut migrations = Vec::new();

    for entry in fs::read_dir(&migrations_dir).expect("migrations directory should be readable") {
        let path = entry
            .expect("migrations directory entries should be readable")
            .path();

        if path.extension().and_then(|extension| extension.to_str()) != Some("sql") {
            continue;
        }

        let file = path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("migration file names should be valid UTF-8")
            .to_owned();
        let stem = file
            .strip_suffix(".sql")
            .expect("migration files should end with .sql");
        let (version, name) = stem
            .split_once('_')
            .expect("migration file names should follow <version>_<name>.sql");
        let version = version
            .parse::<i64>()
            .expect("migration versions should be integers");

        migrations.push((version, name.to_owned(), file, path));
    }

    migrations.sort_by_key(|migration| migration.0);

    let mut generated = String::from("pub static MIGRATIONS: &[Migration] = &[\n");
    for (version, name, file, path) in migrations {
        generated.push_str(&format!(
            "    Migration {{ version: {version}, name: {name:?}, file: {file:?}, sql: include_str!({path:?}) }},\n",
            path = path.to_str().expect("migration paths should be valid UTF-8"),
        ));
    }
    generated.push_str("];\n");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR should be set"));
    fs::write(out_dir.join("migrations.rs"), generated)
        .expect("generated migrations file should be writable");
}

/// Adds the Swift runtime paths needed by the `speech` crate's bridge.
///
/// The dependency emits these link arguments for its own targets, but they do
/// not propagate to this package's binaries and test harnesses.
fn add_swift_runtime_rpaths() {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }

    println!("cargo:rerun-if-env-changed=DEVELOPER_DIR");
    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");

    let Ok(output) = Command::new("xcode-select").arg("-p").output() else {
        return;
    };
    if !output.status.success() {
        return;
    }

    let developer_dir = String::from_utf8_lossy(&output.stdout);
    let developer_dir = developer_dir.trim();
    if developer_dir.is_empty() {
        return;
    }

    for swift_lib_dir in ["swift/macosx", "swift-5.5/macosx"] {
        let path =
            format!("{developer_dir}/Toolchains/XcodeDefault.xctoolchain/usr/lib/{swift_lib_dir}");
        println!("cargo:rustc-link-arg=-Wl,-rpath,{path}");
    }
}
