package dev.aethel.hud;

public final class SessionState {
    private SessionState() {}

    public static volatile String sessionId;
    public static volatile String theme;
    public static volatile String toggles;
    public static volatile String hudLayout;
}
