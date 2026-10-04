package dev.aethel.cosmetics;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.LinkedHashMap;
import java.util.Map;
import net.minecraft.client.renderer.entity.state.AvatarRenderState;
import net.minecraft.core.ClientAsset;
import net.minecraft.resources.Identifier;
import net.minecraft.world.entity.player.PlayerSkin;

/**
 * The equipped cosmetic set, shared by the cape and wings renderers.
 *
 * <p>The launcher owns the truth: it writes {@code platform/equipped.json} and pushes the same
 * documents over IPC when a game session is live. Either path ends up here, so cosmetics render
 * with or without a live socket.
 */
public final class CosmeticsState {

    private static final String NAMESPACE = "aethel_cosmetics";

    private static final Map<String, Identifier> CAPES = new LinkedHashMap<>();
    private static final Map<String, Identifier> WINGS = new LinkedHashMap<>();

    private static volatile ClientAsset.Texture capeTexture;
    private static volatile ClientAsset.Texture wingsTexture;

    private CosmeticsState() {
    }

    /** The vanilla cape slot renders {@code textures/cape/<slug>}. */
    public static void setCape(String slug) {
        CAPES.clear();
        capeTexture = texture(slug, "cape");
    }

    /** Elytra and wings share one slot, rendered from {@code textures/wings/<slug>}. */
    public static void setWings(String slug) {
        WINGS.clear();
        wingsTexture = texture(slug, "wings");
    }

    public static void clear() {
        CAPES.clear();
        WINGS.clear();
        capeTexture = null;
        wingsTexture = null;
    }

    private static ClientAsset.Texture texture(String slug, String folder) {
        if (slug == null || slug.isBlank()) {
            return null;
        }
        String leaf = slug.contains("-") ? slug.substring(slug.lastIndexOf('-') + 1) : slug;
        Identifier id = Identifier.fromNamespaceAndPath(
                NAMESPACE, "textures/" + folder + "/" + leaf);
        (folder.equals("cape") ? CAPES : WINGS).put(slug, id);
        return new ClientAsset.ResourceTexture(id, id);
    }

    public static Identifier capeIdentifier() {
        return capeTexture == null ? null : capeTexture.texturePath();
    }

    public static Identifier wingsIdentifier() {
        return wingsTexture == null ? null : wingsTexture.texturePath();
    }

    /**
     * Swaps the render state's skin for one carrying our cape, leaving vanilla to animate and light
     * it. Runs every frame for every player, so it stays allocation-free once a set is equipped.
     */
    public static void applyCape(AvatarRenderState state) {
        ClientAsset.Texture cape = capeTexture;
        if (cape == null || state == null) {
            return;
        }
        PlayerSkin skin = state.skin;
        if (skin == null || skin.cape() == cape) {
            return;
        }
        state.skin = PlayerSkin.insecure(
                skin.body(), cape, skin.elytra(), skin.model());
    }

    /** Applies a {@code cosmetics} IPC payload or an {@code equipped.json} body. */
    public static void load(String json) {
        Map<String, String> bySlot = new LinkedHashMap<>();
        for (String slug : slugsIn(json)) {
            String slot = slotIn(json, slug);
            bySlot.put(slot, slug);
        }
        setCape(bySlot.get("cape"));
        setWings(bySlot.get("wings"));
    }

    private static String slotIn(String json, String slug) {
        int at = json.indexOf("\"slug\":\"" + slug + "\"");
        if (at < 0) {
            return "";
        }
        int slotAt = json.indexOf("\"slot\":\"", at);
        if (slotAt < 0) {
            return "";
        }
        int from = slotAt + "\"slot\":\"".length();
        int to = json.indexOf('"', from);
        return to < 0 ? "" : json.substring(from, to);
    }

    private static java.util.List<String> slugsIn(String json) {
        java.util.List<String> out = new java.util.ArrayList<>();
        String needle = "\"slug\":\"";
        int at = json.indexOf(needle);
        while (at >= 0) {
            int from = at + needle.length();
            int to = json.indexOf('"', from);
            if (to < 0) {
                break;
            }
            out.add(json.substring(from, to));
            at = json.indexOf(needle, to);
        }
        return out;
    }

    /** Reads the launcher's on-disk equipped set; safe to call when the file is missing. */
    public static void loadFromLauncherHome(String home) {
        if (home == null || home.isBlank()) {
            return;
        }
        Path file = Path.of(home, "platform", "equipped.json");
        try {
            if (Files.isRegularFile(file)) {
                load(Files.readString(file, StandardCharsets.UTF_8));
                AethelCosmeticsMod.LOGGER.info("loaded {} equipped cosmetics from disk", slugsIn("").size() + count());
            }
        } catch (IOException e) {
            AethelCosmeticsMod.LOGGER.warn("could not read {}", file, e);
        }
    }

    private static int count() {
        return (capeTexture != null ? 1 : 0) + (wingsTexture != null ? 1 : 0);
    }
}