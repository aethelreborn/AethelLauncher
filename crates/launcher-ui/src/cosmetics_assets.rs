use eframe::egui;
use egui::{ColorImage, TextureHandle, TextureOptions};

struct Art {
    path: &'static str,
    bytes: &'static [u8],
}

const ART: &[Art] = &[
    Art {
        path: "capes/aethel.png",
        bytes: include_bytes!("../assets/cosmetics/capes/aethel.png"),
    },
    Art {
        path: "capes/aurora.png",
        bytes: include_bytes!("../assets/cosmetics/capes/aurora.png"),
    },
    Art {
        path: "capes/ember.png",
        bytes: include_bytes!("../assets/cosmetics/capes/ember.png"),
    },
    Art {
        path: "wings/void.png",
        bytes: include_bytes!("../assets/cosmetics/wings/void.png"),
    },
    Art {
        path: "wings/prism.png",
        bytes: include_bytes!("../assets/cosmetics/wings/prism.png"),
    },
    Art {
        path: "badges/founder.png",
        bytes: include_bytes!("../assets/cosmetics/badges/founder.png"),
    },
    Art {
        path: "badges/early.png",
        bytes: include_bytes!("../assets/cosmetics/badges/early.png"),
    },
    Art {
        path: "badges/bugfinder.png",
        bytes: include_bytes!("../assets/cosmetics/badges/bugfinder.png"),
    },
    Art {
        path: "huds/minimal.png",
        bytes: include_bytes!("../assets/cosmetics/huds/minimal.png"),
    },
    Art {
        path: "huds/neon.png",
        bytes: include_bytes!("../assets/cosmetics/huds/neon.png"),
    },
    Art {
        path: "elytras/dragon.png",
        bytes: include_bytes!("../assets/cosmetics/elytras/dragon.png"),
    },
    Art {
        path: "skins/shadow.png",
        bytes: include_bytes!("../assets/cosmetics/skins/shadow.png"),
    },
];

pub fn texture(ctx: &egui::Context, asset_url: &str) -> Option<TextureHandle> {
    let art = ART.iter().find(|a| a.path == asset_url).or_else(|| {
        let (dir, file) = asset_url.split_once('/')?;
        if dir.ends_with('s') {
            return None;
        }
        let plural = format!("{dir}s/{file}");
        ART.iter().find(|a| a.path == plural)
    })?;
    let id = egui::Id::new(("cosmetics-art", asset_url));
    if let Some(existing) = ctx.data(|d| d.get_temp::<TextureHandle>(id)) {
        return Some(existing);
    }
    let rgba = image::load_from_memory(art.bytes).ok()?.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    let image = ColorImage::from_rgba_unmultiplied(size, &rgba);
    let handle = ctx.load_texture(asset_url, image, TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, handle.clone()));
    Some(handle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_embedded_asset_decodes() {
        for art in ART {
            let decoded = image::load_from_memory(art.bytes)
                .unwrap_or_else(|e| panic!("{} does not decode: {e}", art.path));
            assert!(decoded.width() >= 256, "{} is too small", art.path);
        }
    }

    #[test]
    fn paths_are_unique_and_match_the_asset_url_scheme() {
        for (i, art) in ART.iter().enumerate() {
            assert!(art.path.ends_with(".png"));
            assert!(
                ART[..i].iter().all(|o| o.path != art.path),
                "duplicate {}",
                art.path
            );
            let dir = art.path.split('/').next().unwrap();
            assert!(dir.ends_with('s'), "{dir} should be pluralised");
        }
    }

    #[test]
    fn texture_lookup_misses_for_unknown_assets() {
        let ctx = egui::Context::default();
        assert!(texture(&ctx, "capes/nope.png").is_none());
        assert!(texture(&ctx, "capes/aethel.png").is_some());
        assert!(texture(&ctx, "capes/aethel.png").is_some());
        assert!(texture(&ctx, "hud/minimal.png").is_some());
        assert!(texture(&ctx, "elytra/dragon.png").is_some());
    }
}
