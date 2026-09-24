//! Checks against the **live** Mojang manifests.
//!
//! These are `#[ignore]`d because they need network access; run them with:
//!
//! ```text
//! cargo test -p launcher-core --test real_manifest -- --ignored --nocapture
//! ```
//!
//! They are the guard against the classic failure mode where our serde model
//! silently diverges from the real version JSON and every launch dies with a
//! `ClassNotFoundException`.

use launcher_core::auth::offline_uuid;
use launcher_core::install::pipeline::{http_client, plan_libraries};
use launcher_core::launch::args_builder::build_for;
use launcher_core::manifest::mojang::{fetch_version_json, fetch_version_manifest, VersionJson};
use launcher_core::Account;
use std::path::{Path, PathBuf};

async fn fetch_version(id: &str) -> VersionJson {
    let client = http_client().expect("http client");
    let manifest = fetch_version_manifest(&client)
        .await
        .expect("fetch version manifest");
    let record = manifest
        .versions
        .iter()
        .find(|v| v.id == id)
        .unwrap_or_else(|| panic!("version {id} is not in the live manifest"));
    fetch_version_json(&client, &record.url)
        .await
        .unwrap_or_else(|e| panic!("failed to parse {id} version JSON: {e:#}"))
}

/// Parse + plan + expand for a spread of version generations.
#[tokio::test]
#[ignore = "requires network access to Mojang"]
async fn live_versions_plan_a_complete_launch() {
    let libraries_dir = PathBuf::from("/tmp/aethel-test/libs");
    let auth = Account::Offline {
        name: "Steve".to_string(),
        uuid: offline_uuid("Steve"),
    };

    for id in ["1.21.4", "1.20.1", "1.16.5"] {
        let version = fetch_version(id).await;
        eprintln!("--- {id} ---");

        assert_eq!(version.id, id);
        assert!(!version.main_class.is_empty(), "{id}: empty mainClass");
        assert!(
            version.downloads.client.is_some(),
            "{id}: no client jar in downloads"
        );
        assert!(
            version.asset_index.is_some(),
            "{id}: no assetIndex (expected on every version >= 1.7)"
        );
        assert!(
            version.java_version.is_some(),
            "{id}: no javaVersion component"
        );

        let plan = plan_libraries(&version, &libraries_dir);
        eprintln!(
            "{id}: {} classpath entries, {} downloads, {} native jars",
            plan.classpath.len(),
            plan.downloads.len(),
            plan.natives.len()
        );

        assert!(
            plan.classpath.len() > 5,
            "{id}: suspiciously small classpath"
        );
        assert!(
            plan.classpath
                .iter()
                .all(|p| p.extension().is_some_and(|e| e == "jar")),
            "{id}: classpath contains a non-jar"
        );
        // Every classpath entry must be downloaded, or the launch will fail.
        for entry in &plan.classpath {
            assert!(
                plan.downloads.iter().any(|d| &d.dest == entry),
                "{id}: {entry:?} is on the classpath but never downloaded"
            );
        }
        // No library should be downloaded twice.
        let mut dests: Vec<_> = plan.downloads.iter().map(|d| d.dest.clone()).collect();
        dests.sort();
        let unique = dests.len();
        dests.dedup();
        assert_eq!(unique, dests.len(), "{id}: duplicate downloads planned");

        // Two different worlds of native handling:
        //   * <= 1.16.5 declares a `natives` map -> classifier jars to extract
        //   * >= 1.19 drops the map and puts `...:natives-linux` entries on the
        //     classpath, letting LWJGL unpack them at runtime
        let declares_natives = version.libraries.iter().any(|l| !l.natives.is_empty());
        if declares_natives {
            assert!(
                !plan.natives.is_empty(),
                "{id}: declares a natives map but planned no native jars"
            );
        } else {
            assert!(
                plan.natives.is_empty(),
                "{id}: planned native jars but declares no natives map"
            );
            assert!(
                plan.classpath
                    .iter()
                    .any(|p| p.to_string_lossy().contains("natives-")),
                "{id}: modern version must carry platform natives on the classpath"
            );
        }
        // Bundled libraries (no artifact, no natives) must not leak in.
        assert!(
            plan.downloads
                .iter()
                .all(|d| d.dest.extension().is_some_and(|e| e == "jar")),
            "{id}: non-jar download planned"
        );

        let assets_index_name = version
            .asset_index
            .as_ref()
            .map(|a| a.id.as_str())
            .unwrap_or("legacy");
        let args = build_for(
            &version,
            &auth,
            Path::new("/tmp/aethel-test/game"),
            Path::new("/tmp/aethel-test/assets"),
            Path::new("/tmp/aethel-test/natives"),
            &libraries_dir,
            plan.classpath.clone(),
            2048,
            "auto",
            assets_index_name,
        );

        let rendered = format!("{:?} {:?}", args.jvm_args, args.game_args);
        assert!(
            !rendered.contains("${"),
            "{id}: unexpanded token left in the arguments: {rendered}"
        );
        assert!(
            args.jvm_args.iter().any(|f| f == "-Xmx2048M"),
            "{id}: heap flag missing"
        );
        assert!(
            args.jvm_args.iter().any(|f| f == "-cp"),
            "{id}: no -cp in the JVM args"
        );
        assert!(
            args.game_args.iter().any(|a| a == "Steve"),
            "{id}: username not passed to the game"
        );
        assert!(
            args.game_args.iter().any(|a| a == assets_index_name),
            "{id}: asset index not passed to the game"
        );

        eprintln!("{id}: ok ({} jvm args)", args.jvm_args.len());
    }
}
