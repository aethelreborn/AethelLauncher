package dev.aethel.ipc;

import java.net.URI;
import java.net.http.HttpClient;
import java.net.http.WebSocket;
import java.time.Duration;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.CompletionStage;
import java.util.concurrent.TimeUnit;

public final class IpcClient implements WebSocket.Listener, AutoCloseable {

    public interface Listener {
        void onWelcome(String sessionId);

        void onMessage(String type, String rawJson);

        void onClosed();
    }

    private static final Duration CONNECT_TIMEOUT = Duration.ofSeconds(10);

    private final String hello;
    private final Listener listener;
    private final StringBuilder partial = new StringBuilder();
    private final CompletableFuture<String> welcome = new CompletableFuture<>();
    private final CompletableFuture<Long> closed = new CompletableFuture<>();
    private boolean closedFired;
    private WebSocket ws;

    private IpcClient(Listener listener, String hello) {
        this.listener = listener;
        this.hello = hello;
    }

    public static IpcClient connect(String endpoint, String token, Listener listener)
            throws Exception {
        String hello = "{\"v\":1,\"type\":\"aethel_hello\",\"token\":\"" + token
                + "\",\"launcher\":\"aethel\",\"version\":\"1.0.0\","
                + "\"gamePid\":" + ProcessHandle.current().pid()
                + ",\"mcVersion\":\"unknown\"}";
        IpcClient client = new IpcClient(listener, hello);
        client.ws = HttpClient.newHttpClient()
                .newWebSocketBuilder()
                .buildAsync(URI.create(endpoint), client)
                .get(CONNECT_TIMEOUT.toMillis(), TimeUnit.MILLISECONDS);
        return client;
    }

    public String awaitWelcome(Duration timeout) throws Exception {
        return welcome.get(timeout.toMillis(), TimeUnit.MILLISECONDS);
    }

    public void awaitClosed(Duration timeout) throws Exception {
        closed.get(timeout.toMillis(), TimeUnit.MILLISECONDS);
    }

    public boolean hasWelcomed() {
        return welcome.isDone() && !welcome.isCompletedExceptionally();
    }


    public void sendLaunched(String renderer, int width, int height) {
        send("{\"v\":1,\"type\":\"launched\",\"renderer\":\"" + renderer
                + "\",\"width\":" + width + ",\"height\":" + height + "}");
    }

    public void sendFps(double fps, double frameTimeMs) {
        send("{\"v\":1,\"type\":\"fps\",\"fps\":" + fps + ",\"frameTimeMs\":" + frameTimeMs + "}");
    }

    public void sendPing(long seq) {
        send("{\"v\":1,\"type\":\"ping\",\"seq\":" + seq + "}");
    }

    public void sendPlaytime(long secs) {
        send("{\"v\":1,\"type\":\"playtime\",\"secs\":" + secs + "}");
    }

    public void sendBye(int code, String reason) {
        send("{\"v\":1,\"type\":\"bye\",\"code\":" + code
                + ",\"reason\":\"" + reason + "\"}");
    }

    public void send(String frame) {
        ws.sendText(frame, true).join();
    }

    @Override
    public void close() {
        try {
            ws.sendClose(WebSocket.NORMAL_CLOSURE, "client_close").join();
        } catch (Exception ignored) {
        }
    }


    @Override
    public void onOpen(WebSocket webSocket) {
        webSocket.sendText(hello, true);
        webSocket.request(1);
    }

    @Override
    public CompletionStage<?> onText(WebSocket webSocket, CharSequence data, boolean last) {
        partial.append(data);
        if (last) {
            String frame = partial.toString();
            partial.setLength(0);
            handleFrame(frame);
        }
        webSocket.request(1);
        return null;
    }

    @Override
    public CompletionStage<?> onClose(WebSocket webSocket, int statusCode, String reason) {
        fireClosed();
        return null;
    }

    @Override
    public void onError(WebSocket webSocket, Throwable error) {
        fireClosed();
    }

    private void handleFrame(String frame) {
        String type = MiniJson.string(frame, "type");
        if (type == null) {
            return;
        }
        if ("welcome".equals(type)) {
            String sid = MiniJson.string(frame, "sessionId");
            welcome.complete(sid == null ? "" : sid);
            if (sid != null) {
                listener.onWelcome(sid);
            }
            return;
        }
        listener.onMessage(type, frame);
    }

    private void fireClosed() {
        if (!closedFired) {
            closedFired = true;
            closed.complete(0L);
            listener.onClosed();
        }
    }
}
