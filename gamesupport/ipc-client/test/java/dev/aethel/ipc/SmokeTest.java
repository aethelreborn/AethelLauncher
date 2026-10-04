package dev.aethel.ipc;

import java.time.Duration;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;

public final class SmokeTest {

    private SmokeTest() {}

    public static void main(String[] args) throws Exception {
        if (args.length != 2) {
            System.err.println("usage: SmokeTest <ws-url> <token>");
            System.exit(1);
        }
        String url = args[0];
        String token = args[1];

        CountDownLatch themeSeen = new CountDownLatch(1);
        CountDownLatch pongSeen = new CountDownLatch(1);
        AtomicReference<Long> pongSeq = new AtomicReference<>();
        AtomicReference<String> sessionId = new AtomicReference<>();

        IpcClient client;
        try {
            client = IpcClient.connect(url, token, new IpcClient.Listener() {
                @Override
                public void onWelcome(String sid) {
                    sessionId.set(sid);
                    System.out.println("welcome sessionId=" + sid);
                }

                @Override
                public void onMessage(String type, String raw) {
                    System.out.println("frame type=" + type);
                    if ("setTheme".equals(type)) {
                        themeSeen.countDown();
                    } else if ("pong".equals(type)) {
                        pongSeq.set(MiniJson.number(raw, "seq"));
                        pongSeen.countDown();
                    }
                }

                @Override
                public void onClosed() {
                    System.out.println("closed");
                }
            });
        } catch (Exception e) {
            System.err.println("connect failed: " + e);
            System.exit(1);
            return;
        }

        try {
            String sid = client.awaitWelcome(Duration.ofSeconds(10));
            if (sid == null || sid.isEmpty()) {
                System.err.println("welcome missing sessionId");
                System.exit(2);
                return;
            }

            if (!themeSeen.await(10, TimeUnit.SECONDS)) {
                System.err.println("queued setTheme never arrived");
                System.exit(3);
                return;
            }

            client.sendLaunched("vulkan", 1920, 1080);
            client.sendPing(7);
            if (!pongSeen.await(10, TimeUnit.SECONDS)) {
                System.err.println("pong never arrived");
                System.exit(4);
                return;
            }
            if (!Long.valueOf(7L).equals(pongSeq.get())) {
                System.err.println("pong seq mismatch: " + pongSeq.get());
                System.exit(4);
                return;
            }

            client.sendBye(0, "game_shutdown");
            try {
                client.awaitClosed(Duration.ofSeconds(5));
            } catch (Exception e) {
                System.err.println("no close after bye: " + e);
                System.exit(5);
                return;
            }

            System.out.println("smoke ok");
            client.close();
            System.exit(0);
        } catch (Exception e) {
            System.err.println("smoke failed: " + e);
            System.exit(1);
        }
    }
}
