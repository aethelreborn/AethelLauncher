use anyhow::Context;
use std::fmt::Write as _;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GraphicsPreset {
    Performance,
    #[default]
    Balanced,
    Quality,
}

impl GraphicsPreset {
    pub fn label(&self) -> &'static str {
        match self {
            GraphicsPreset::Performance => "Performance",
            GraphicsPreset::Balanced => "Balanced",
            GraphicsPreset::Quality => "Quality",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            GraphicsPreset::Performance => {
                "Lowest settings, maximum FPS. Best for 4–8 GB machines."
            }
            GraphicsPreset::Balanced => "Good looks with solid frame rates.",
            GraphicsPreset::Quality => "Highest visual fidelity — needs a real GPU.",
        }
    }

    pub fn all() -> [GraphicsPreset; 3] {
        [
            GraphicsPreset::Performance,
            GraphicsPreset::Balanced,
            GraphicsPreset::Quality,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TunedOptions {
    pub render_distance: u32,
    pub simulation_distance: u32,
    pub max_fps: u32,
    pub graphics_mode: u32,
    pub particles: u32,
    pub entity_distance_scaling: f32,
    pub biome_blend_radius: u32,
    pub mipmap_levels: u32,
    pub clouds: bool,
    pub ao: bool,
    pub vsync: bool,
    pub menu_background_blurriness: u32,
}

impl TunedOptions {
    pub fn for_preset(preset: GraphicsPreset, system_ram_mb: u64) -> Self {
        let low_memory = system_ram_mb > 0 && system_ram_mb <= 8192;

        match preset {
            GraphicsPreset::Performance => Self {
                render_distance: if low_memory { 4 } else { 6 },
                simulation_distance: 4,
                max_fps: 90,
                graphics_mode: 0,
                particles: 2,
                entity_distance_scaling: 0.5,
                biome_blend_radius: 0,
                mipmap_levels: 0,
                clouds: false,
                ao: false,
                vsync: false,
                menu_background_blurriness: 0,
            },
            GraphicsPreset::Balanced => Self {
                render_distance: if low_memory { 6 } else { 10 },
                simulation_distance: if low_memory { 5 } else { 8 },
                max_fps: 144,
                graphics_mode: 1,
                particles: 1,
                entity_distance_scaling: 0.75,
                biome_blend_radius: 2,
                mipmap_levels: 2,
                clouds: true,
                ao: true,
                vsync: false,
                menu_background_blurriness: 2,
            },
            GraphicsPreset::Quality => Self {
                render_distance: if low_memory { 8 } else { 16 },
                simulation_distance: if low_memory { 6 } else { 12 },
                max_fps: 240,
                graphics_mode: 1,
                particles: 1,
                entity_distance_scaling: 1.0,
                biome_blend_radius: 5,
                mipmap_levels: 4,
                clouds: true,
                ao: true,
                vsync: false,
                menu_background_blurriness: 5,
            },
        }
    }

    pub fn entries(&self) -> Vec<(&'static str, String)> {
        vec![
            ("renderDistance", self.render_distance.to_string()),
            ("simulationDistance", self.simulation_distance.to_string()),
            ("maxFps", self.max_fps.to_string()),
            ("graphicsMode", self.graphics_mode.to_string()),
            ("particles", self.particles.to_string()),
            (
                "entityDistanceScaling",
                format!("{:.1}", self.entity_distance_scaling),
            ),
            ("biomeBlendRadius", self.biome_blend_radius.to_string()),
            ("mipmapLevels", self.mipmap_levels.to_string()),
            (
                "cloudStatus",
                if self.clouds { "1" } else { "0" }.to_string(),
            ),
            ("clouds", self.clouds.to_string()),
            ("ao", self.ao.to_string()),
            ("enableVsync", self.vsync.to_string()),
            (
                "menuBackgroundBlurriness",
                self.menu_background_blurriness.to_string(),
            ),
            ("pauseOnLostFocus", "true".to_string()),
            ("prioritizeChunkUpdates", "1".to_string()),
        ]
    }
}

pub fn apply(
    game_dir: &Path,
    tuned: &TunedOptions,
    extra: &[(&str, String)],
) -> anyhow::Result<bool> {
    let path = game_dir.join("options.txt");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();

    let updates: Vec<(&str, String)> = tuned
        .entries()
        .into_iter()
        .chain(extra.iter().cloned())
        .collect();

    let mut seen: std::collections::HashSet<&str> =
        std::collections::HashSet::with_capacity(updates.len());
    let mut output = String::with_capacity(existing.len() + 256);
    let mut changed = false;

    for line in existing.lines() {
        let key = line.split(':').next().unwrap_or("");
        match updates.iter().find(|(k, _)| *k == key) {
            Some((k, value)) => {
                seen.insert(k);
                let replacement = format!("{k}:{value}");
                if replacement != line {
                    changed = true;
                }
                output.push_str(&replacement);
                output.push('\n');
            }
            None => {
                output.push_str(line);
                output.push('\n');
            }
        }
    }

    for (key, value) in &updates {
        if seen.contains(key) {
            continue;
        }
        let _ = writeln!(output, "{key}:{value}");
        changed = true;
    }

    if !changed && path.exists() {
        return Ok(false);
    }

    std::fs::create_dir_all(game_dir)
        .with_context(|| format!("failed to create {}", game_dir.display()))?;
    std::fs::write(&path, output).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_memory_gets_a_smaller_render_distance() {
        let weak = TunedOptions::for_preset(GraphicsPreset::Performance, 4096);
        let strong = TunedOptions::for_preset(GraphicsPreset::Performance, 32768);
        assert_eq!(weak.render_distance, 4);
        assert_eq!(strong.render_distance, 6);
        assert!(weak.render_distance < strong.render_distance);
    }

    #[test]
    fn performance_preset_trims_the_expensive_settings() {
        let tuned = TunedOptions::for_preset(GraphicsPreset::Performance, 4096);
        assert_eq!(tuned.graphics_mode, 0, "should use fast graphics");
        assert_eq!(tuned.particles, 2, "should use minimal particles");
        assert!(!tuned.clouds);
        assert!(!tuned.ao);
        assert!((tuned.entity_distance_scaling - 0.5).abs() < f32::EPSILON);
        assert_eq!(tuned.simulation_distance, 4);
    }

    #[test]
    fn quality_preset_only_scales_up_on_capable_hardware() {
        let weak = TunedOptions::for_preset(GraphicsPreset::Quality, 4096);
        assert_eq!(weak.render_distance, 8);
        let strong = TunedOptions::for_preset(GraphicsPreset::Quality, 32768);
        assert_eq!(strong.render_distance, 16);
    }

    #[test]
    fn apply_preserves_unrelated_keys() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("options.txt"),
            "fov:0.5\nguiScale:3\nrenderDistance:16\nsoundCategory_master:0.4\n",
        )
        .unwrap();

        let tuned = TunedOptions::for_preset(GraphicsPreset::Performance, 4096);
        let changed = apply(dir.path(), &tuned, &[]).unwrap();
        assert!(changed);

        let result = std::fs::read_to_string(dir.path().join("options.txt")).unwrap();

        assert!(result.contains("fov:0.5"));
        assert!(result.contains("guiScale:3"));
        assert!(result.contains("soundCategory_master:0.4"));
        assert!(result.contains("renderDistance:4"));
        assert!(!result.contains("renderDistance:16"));
        assert!(result.contains("simulationDistance:4"));
        assert!(result.contains("particles:2"));
    }

    #[test]
    fn second_apply_is_a_no_op() {
        let dir = tempfile::tempdir().unwrap();
        let tuned = TunedOptions::for_preset(GraphicsPreset::Balanced, 8192);
        assert!(apply(dir.path(), &tuned, &[]).unwrap());
        assert!(
            !apply(dir.path(), &tuned, &[]).unwrap(),
            "re-applying the same preset should not rewrite the file"
        );
    }

    #[test]
    fn extra_keys_are_merged_too() {
        let dir = tempfile::tempdir().unwrap();
        let tuned = TunedOptions::for_preset(GraphicsPreset::Balanced, 8192);
        apply(
            dir.path(),
            &tuned,
            &[("aethelRenderer", "vulkan".to_string())],
        )
        .unwrap();
        let result = std::fs::read_to_string(dir.path().join("options.txt")).unwrap();
        assert!(result.contains("aethelRenderer:vulkan"));
    }
}
