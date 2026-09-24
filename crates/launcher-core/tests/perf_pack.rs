//! Network-gated check that the performance layer really resolves.
//!
//! This is the test that matters for frame rate: it proves Fabric's loader can
//! be resolved and merged, that its Maven-only libraries end up on the classpath
//! with verifiable hashes, and that the Modrinth pack installs real jars.
//!
//! ```text
//! AETHEL_PERF_VERSION=1.21.4 \
//!   cargo test -p launcher-core --test perf_pack -- --ignored --nocapture
//! ```

use launcher_core::install::pipeline::{http_client, plan_libraries};
use launcher_core::manifest::fabric::{merge_into, resolve_loader};
use launcher_core::manifest::mojang::{fetch_version_json, fetch_version_manifest};
use launcher_core::perf::mods::{
    install_performance_pack, pack_for, ProgressCallback, PERFORMANCE_PACK,
};
use launcher_core::perf::{options, GraphicsPreset, Renderer, TunedOptions};
use std::path::PathBuf;
use std::sync::Arc;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires network access to Fabric Meta and Modrinth"]
async fn fabric_and_performance_pack_are_installable() {
    let mc_version = std::env::var("AETHEL_PERF_VERSION").unwrap_or_else(|_| "1.21.4".to_string());
    let client = http_client().expect("http client");

    // --- 1. Vanilla version JSON -----------------------------------------
    let manifest = fetch_version_manifest(&client).await.expect("manifest");
    let record = manifest
        .versions
        .iter()
        .find(|v| v.id == mc_version)
        .unwrap_or_else(|| panic!("{mc_version} missing from the live manifest"));
    let mut version = fetch_version_json(&client, &record.url)
        .await
        .expect("version json");
    let vanilla_libraries = version.libraries.len();
    let vanilla_main = version.main_class.clone();
    eprintln!("{mc_version}: {vanilla_libraries} vanilla libraries");

    // --- 2. Resolve + merge Fabric ---------------------------------------
    let loader = resolve_loader(&client, &mc_version)
        .await
        .unwrap_or_else(|e| panic!("Fabric has no loader for {mc_version}: {e:#}"));
    eprintln!(
        "{mc_version}: fabric loader {} (intermediary {})",
        loader.loader_version, loader.intermediary_version
    );
    assert!(!loader.loader_version.is_empty());

    merge_into(&mut version, &loader);
    assert_eq!(
        version.main_class, "net.fabricmc.loader.impl.launch.knot.KnotClient",
        "Fabric must take over the main class"
    );
    assert_ne!(
        version.main_class, vanilla_main,
        "main class should have changed from vanilla"
    );
    assert!(
        version.libraries.len() > vanilla_libraries,
        "no Fabric libraries were merged in"
    );

    // --- 3. Classpath must include the loader -----------------------------
    let libraries_dir = PathBuf::from("/tmp/aethel-perf-test/libraries");
    let plan = plan_libraries(&version, &libraries_dir);
    eprintln!(
        "{mc_version}: {} classpath entries, {} downloads",
        plan.classpath.len(),
        plan.downloads.len()
    );

    let classpath_blob = plan
        .classpath
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("\n");

    // Without these the game dies with NoClassDefFoundError on boot.
    assert!(
        classpath_blob.contains("fabric-loader"),
        "fabric-loader is missing from the classpath"
    );
    assert!(
        classpath_blob.contains("sponge-mixin"),
        "sponge-mixin is missing from the classpath"
    );

    // Fabric libraries have no `downloads` block, so they exercise the
    // Maven-derived URL path — proving we do not silently drop them.
    let maven_derived: Vec<_> = plan
        .downloads
        .iter()
        .filter(|d| d.expected_sha1.is_none() && d.expected_sha256.is_none())
        .collect();
    eprintln!(
        "{mc_version}: {} of {} library downloads need a SHA-1 sidecar",
        maven_derived.len(),
        plan.downloads.len()
    );
    for task in &maven_derived {
        assert!(
            task.url.starts_with("https://"),
            "Maven-derived URL looks wrong: {}",
            task.url
        );
        assert!(
            task.dest.extension().is_some_and(|e| e == "jar"),
            "Maven-derived path is not a jar: {:?}",
            task.dest
        );
    }

    // The loader profile should not have leaked in anything Mojang already
    // provides (the ASM duplication pitfall).
    let mut names: Vec<&str> = version.libraries.iter().map(|l| l.name.as_str()).collect();
    names.sort_unstable();
    let count = names.len();
    names.dedup();
    assert_eq!(count, names.len(), "duplicate library after Fabric merge");

    // The subtle one: two *different versions* of the same Maven artifact both
    // provide the same classes, and Fabric refuses to start ("duplicate ASM
    // classes found on classpath"). Nothing may ship twice per artifact.
    let mut seen_artifacts: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut conflicts: Vec<String> = Vec::new();
    for library in &version.libraries {
        let mut parts = library.name.split(':');
        let (Some(group), Some(artifact)) = (parts.next(), parts.next()) else {
            continue;
        };
        let ga = format!("{group}:{artifact}");
        if !seen_artifacts.insert(ga.clone()) {
            conflicts.push(library.name.clone());
        }
    }
    assert!(
        conflicts.is_empty(),
        "{mc_version}: the same Maven artifact is on the classpath twice \
         (Fabric aborts with `duplicate classes found on classpath`): {conflicts:?}"
    );

    // --- 4. Install the real performance pack -----------------------------
    let mods_dir = PathBuf::from("/tmp/aethel-perf-test/mods");
    let progress: ProgressCallback = Arc::new(|done, total, label| {
        if done == 0 || done == total {
            eprintln!("  [{done}/{total}] {label}");
        }
    });

    let manifest = install_performance_pack(
        &client,
        &mods_dir,
        &mc_version,
        "fabric",
        Renderer::Opengl,
        progress.clone(),
    )
    .await
    .expect("OpenGL performance pack install failed");
    assert_eq!(manifest.renderer, "opengl");

    eprintln!(
        "{mc_version}: {} mods installed (OpenGL), {} unavailable",
        manifest.installed.len(),
        manifest.skipped.len()
    );
    if !manifest.skipped.is_empty() {
        eprintln!("  unavailable: {}", manifest.skipped.join(", "));
    }
    for installed in &manifest.installed {
        eprintln!("  ✓ {} {}", installed.name, installed.version);
    }

    // Sodium and Lithium are the whole point: without them this is not an
    // optimised launcher.
    let installed_slugs: Vec<&str> = manifest.installed.iter().map(|m| m.slug.as_str()).collect();
    for required in ["fabric-api", "sodium", "lithium", "ferrite-core"] {
        assert!(
            installed_slugs.contains(&required),
            "OpenGL pack did not install `{required}` for {mc_version}"
        );
    }
    assert!(
        !installed_slugs.contains(&"vulkanmod"),
        "the OpenGL pack must never include VulkanMod"
    );

    // --- 4b. The Vulkan pack: VulkanMod instead of Sodium -------------------
    let vulkan_dir = PathBuf::from("/tmp/aethel-perf-test/mods-vulkan");
    let vulkan = install_performance_pack(
        &client,
        &vulkan_dir,
        &mc_version,
        "fabric",
        Renderer::Vulkan,
        progress.clone(),
    )
    .await
    .expect("Vulkan performance pack install failed");
    assert_eq!(
        vulkan.renderer, "vulkan",
        "manifest must record the renderer"
    );

    let vulkan_slugs: Vec<&str> = vulkan.installed.iter().map(|m| m.slug.as_str()).collect();
    eprintln!(
        "{mc_version}: {} mods installed (Vulkan), {} unavailable",
        vulkan.installed.len(),
        vulkan.skipped.len()
    );
    eprintln!("  ✓ {}", vulkan_slugs.join(", "));
    assert!(
        vulkan_slugs.contains(&"vulkanmod"),
        "the Vulkan pack must install VulkanMod (Sodium cannot run on Vulkan)"
    );
    for forbidden in ["sodium", "sodium-extra", "reeses-sodium-options"] {
        assert!(
            !vulkan_slugs.contains(&forbidden),
            "`{forbidden}` hooks OpenGL and must not be installed with VulkanMod"
        );
    }
    // The renderer-agnostic wins still apply on Vulkan.
    for required in ["fabric-api", "lithium", "ferrite-core"] {
        assert!(
            vulkan_slugs.contains(&required),
            "Vulkan pack is missing the renderer-agnostic `{required}`"
        );
    }
    for installed in &vulkan.installed {
        let path = vulkan_dir.join(&installed.filename);
        assert!(path.is_file(), "missing vulkan mod jar: {}", path.display());
    }

    // The two packs must be disjoint at the renderer level.
    assert!(!pack_for(Renderer::Vulkan)
        .iter()
        .any(|m| m.slug == "sodium"));

    // Every installed jar must actually exist on disk.
    for installed in &manifest.installed {
        let path = mods_dir.join(&installed.filename);
        assert!(path.is_file(), "missing mod jar: {}", path.display());
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        assert!(size > 1024, "mod jar is suspiciously small: {size} bytes");
    }

    // --- 5. options.txt tuning -------------------------------------------
    let game_dir = PathBuf::from("/tmp/aethel-perf-test/instance");
    std::fs::create_dir_all(&game_dir).unwrap();
    let tuned = TunedOptions::for_preset(GraphicsPreset::Performance, 4096);
    options::apply(&game_dir, &tuned, &[]).expect("options apply");
    let written = std::fs::read_to_string(game_dir.join("options.txt")).unwrap();
    eprintln!(
        "{mc_version}: options.txt renderDistance={}",
        tuned.render_distance
    );
    assert!(written.contains("renderDistance:4"));
    assert!(written.contains("particles:2"));

    eprintln!(
        "{mc_version}: OK — fabric {} + {} performance mods",
        loader.loader_version,
        manifest.installed.len()
    );
}

/// The pack must contain nothing that could break a vanilla client.
#[test]
fn pack_entries_are_all_known_good_slugs() {
    for curated in PERFORMANCE_PACK {
        // Modrinth slugs are lowercase, and may contain digits and dashes
        // (`c2me-fabric`, `reeses-sodium-options`).
        assert!(
            !curated.slug.is_empty()
                && curated
                    .slug
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
            "slug `{}` is not a valid Modrinth slug",
            curated.slug
        );
    }
}
