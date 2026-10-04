package dev.aethel.ipc;

public final class IpcBootstrap {
    private IpcBootstrap() {}

    public static String endpoint() {
        String ep = System.getProperty("aethel.ipc");
        if (ep == null || !ep.startsWith("ws://127.0.0.1:")) {
            return null;
        }
        return ep;
    }

    public static String token() {
        String tok = System.getProperty("aethel.ipcToken");
        if (tok == null || tok.length() != 32) {
            return null;
        }
        for (int i = 0; i < tok.length(); i++) {
            char c = tok.charAt(i);
            boolean hex = (c >= '0' && c <= '9') || (c >= 'a' && c <= 'f');
            if (!hex) {
                return null;
            }
        }
        return tok;
    }

    public static IpcClient connect(IpcClient.Listener listener) {
        String ep = endpoint();
        String tok = token();
        if (ep == null || tok == null) {
            return null;
        }
        try {
            return IpcClient.connect(ep, tok, listener);
        } catch (Exception e) {
            return null;
        }
    }
}
