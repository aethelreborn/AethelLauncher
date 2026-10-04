package dev.aethel.cosmetics;

import dev.aethel.ipc.IpcBootstrap;
import dev.aethel.ipc.IpcClient;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import net.fabricmc.api.ClientModInitializer;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

public final class AethelCosmeticsMod implements ClientModInitializer {
    public static final Logger LOGGER = LoggerFactory.getLogger("aethel-cosmetics");

    private static final List<String> equipped = Collections.synchronizedList(new ArrayList<>());
    private static volatile String sessionId;
    private static IpcClient ipc;

    public static List<String> equippedItems() {
        synchronized (equipped) {
            return new ArrayList<>(equipped);
        }
    }

    public static String sessionId() {
        return sessionId;
    }

    @Override
    public void onInitializeClient() {
        ipc = IpcBootstrap.connect(new IpcClient.Listener() {
            @Override
            public void onWelcome(String id) {
                sessionId = id;
                LOGGER.info("launcher IPC up (session {})", id);
            }

            @Override
            public void onMessage(String type, String raw) {
                if ("cosmetics".equals(type)) {
                    equipped.clear();
                    equipped.add(raw);
                    LOGGER.info("cosmetics push received");
                }
            }

            @Override
            public void onClosed() {
                sessionId = null;
                equipped.clear();
            }
        });

        if (ipc == null) {
            LOGGER.info("Aethel Cosmetics running standalone (no launcher IPC)");
        }
    }
}
