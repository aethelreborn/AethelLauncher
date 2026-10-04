package dev.aethel.hud;

import dev.aethel.ipc.IpcBootstrap;
import dev.aethel.ipc.IpcClient;
import net.fabricmc.api.ClientModInitializer;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

public final class AethelHudMod implements ClientModInitializer {
    public static final Logger LOGGER = LoggerFactory.getLogger("aethel-hud");

    private static IpcClient ipc;

    @Override
    public void onInitializeClient() {
        ipc = IpcBootstrap.connect(new IpcClient.Listener() {
            @Override
            public void onWelcome(String sessionId) {
                SessionState.sessionId = sessionId;
                LOGGER.info("launcher IPC up (session {})", sessionId);
            }

            @Override
            public void onMessage(String type, String raw) {
                switch (type) {
                    case "setTheme" -> {
                        SessionState.theme = raw;
                        LOGGER.debug("theme push received");
                    }
                    case "setToggles" -> SessionState.toggles = raw;
                    case "hudLayout" -> SessionState.hudLayout = raw;
                    case "exit" -> LOGGER.info("launcher requested exit");
                    default -> {  }
                }
            }

            @Override
            public void onClosed() {
                SessionState.sessionId = null;
                LOGGER.info("launcher IPC closed");
            }
        });

        if (ipc != null) {
            LOGGER.info("Aethel HUD attached to the launcher");
        } else {
            LOGGER.info("Aethel HUD running standalone (no launcher IPC)");
        }
    }

    public static IpcClient ipc() {
        return ipc;
    }
}
