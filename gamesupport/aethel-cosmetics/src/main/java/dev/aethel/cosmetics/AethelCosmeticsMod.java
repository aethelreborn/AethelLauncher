package dev.aethel.cosmetics;

import dev.aethel.ipc.IpcBootstrap;
import dev.aethel.ipc.IpcClient;
import net.fabricmc.api.ClientModInitializer;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * Applies store cosmetics in-game. The launcher writes {@code platform/equipped.json} and pushes the
 * same documents over IPC; the cape and wings renderers read them from {@link CosmeticsState}.
 */
public final class AethelCosmeticsMod implements ClientModInitializer {

    public static final Logger LOGGER = LoggerFactory.getLogger("aethel-cosmetics");

    private static IpcClient ipc;

    @Override
    public void onInitializeClient() {
        CosmeticsState.loadFromLauncherHome(System.getProperty("aethel.home"));
        if (CosmeticsState.capeIdentifier() != null || CosmeticsState.wingsIdentifier() != null) {
            LOGGER.info("equipped cosmetics ready (cape={}, wings={})",
                    CosmeticsState.capeIdentifier(), CosmeticsState.wingsIdentifier());
        }

        ipc = IpcBootstrap.connect(new IpcClient.Listener() {
            @Override
            public void onWelcome(String sessionId) {
                LOGGER.info("launcher IPC up (session {})", sessionId);
            }

            @Override
            public void onMessage(String type, String raw) {
                if ("cosmetics".equals(type)) {
                    CosmeticsState.load(raw);
                    LOGGER.info("cosmetics push applied (cape={}, wings={})",
                            CosmeticsState.capeIdentifier(), CosmeticsState.wingsIdentifier());
                }
            }

            @Override
            public void onClosed() {
                LOGGER.info("launcher IPC closed");
            }
        });

        if (ipc == null) {
            LOGGER.info("Aethel Cosmetics running standalone (no launcher IPC)");
        }
    }
}