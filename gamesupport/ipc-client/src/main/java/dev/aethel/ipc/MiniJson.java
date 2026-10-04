package dev.aethel.ipc;

public final class MiniJson {
    private MiniJson() {}

    public static String string(String json, String key) {
        int i = valueStart(json, key);
        if (i < 0 || i >= json.length() || json.charAt(i) != '"') {
            return null;
        }
        int end = json.indexOf('"', i + 1);
        if (end < 0) {
            return null;
        }
        return json.substring(i + 1, end);
    }

    public static Long number(String json, String key) {
        int i = valueStart(json, key);
        if (i < 0) {
            return null;
        }
        int end = i;
        while (end < json.length() && "-+.eE0123456789".indexOf(json.charAt(end)) >= 0) {
            end++;
        }
        if (end == i) {
            return null;
        }
        try {
            return (long) Double.parseDouble(json.substring(i, end));
        } catch (NumberFormatException e) {
            return null;
        }
    }

    private static int valueStart(String json, String key) {
        String needle = "\"" + key + "\"";
        int at = json.indexOf(needle);
        if (at < 0) {
            return -1;
        }
        int colon = json.indexOf(':', at + needle.length());
        if (colon < 0) {
            return -1;
        }
        int i = colon + 1;
        while (i < json.length() && Character.isWhitespace(json.charAt(i))) {
            i++;
        }
        return i;
    }
}
