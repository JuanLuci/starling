package io.github.juanluci.starling;

import android.app.Activity;
import android.app.AlertDialog;
import android.content.Context;
import android.content.Intent;
import android.content.pm.ActivityInfo;
import android.database.Cursor;
import android.graphics.Color;
import android.graphics.Typeface;
import android.graphics.drawable.GradientDrawable;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.provider.OpenableColumns;
import android.util.TypedValue;
import android.view.Gravity;
import android.view.View;
import android.widget.LinearLayout;
import android.widget.EditText;
import android.widget.ScrollView;
import android.view.WindowInsets;
import android.view.WindowInsetsController;
import android.widget.TextView;
import java.io.File;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.OutputStream;

/**
 * What runs before the game (StarlingActivity) when it is handed something, in a process and a
 * task of its own: a file opened with Starling or shared to it (a resource pack, a world, a mod,
 * the prepared resources) is copied into the game's import inbox, which the game installs from
 * (crates/horizon-client/src/import.rs); and a restart the game asked for starts it again once
 * its old process has ended.
 */
public final class LauncherActivity extends Activity {
    /** The game's process id: start the game again once it has ended. */
    static final String AFTER = "io.github.juanluci.starling.AFTER";

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        Intent intent = getIntent();
        final int after = intent == null ? 0 : intent.getIntExtra(AFTER, 0);
        Uri file = handedFile(intent);
        if (after != 0) {
            showRestarting();
            new Thread(new Runnable() {
                @Override
                public void run() {
                    // A few seconds at most: the game saves and ends as its activity finishes.
                    for (int i = 0; i < 100 && new File("/proc/" + after).exists(); i++) {
                        try {
                            Thread.sleep(50);
                        } catch (InterruptedException e) {
                            break;
                        }
                    }
                    runOnUiThread(new Runnable() {
                        @Override
                        public void run() {
                            play();
                        }
                    });
                }
            }, "starling-restart").start();
        } else if (file != null) {
            final AlertDialog copying =
                    new AlertDialog.Builder(this, android.R.style.Theme_DeviceDefault_Dialog_Alert)
                            .setMessage("Handing the file to Starling...")
                            .setCancelable(false)
                            .show();
            receive(this, file, new Runnable() {
                @Override
                public void run() {
                    copying.dismiss();
                    play();
                }
            });
        } else {
            play();
        }
    }

    private void play() {
        // The game's own task (singleTask): it stays in the recents, this one does not.
        Intent game = new Intent(this, StarlingActivity.class);
        game.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
        startActivity(game);
        finish();
        // Straight into the game's black first frame, no window animation between.
        overridePendingTransition(0, 0);
    }

    /**
     * While the game starts again (a world whose mods are not the running ones): a screen of its
     * own, black, landscape and without bars as the game is, so the phone does not show what is
     * under this see-through activity between the two processes, as if the game had closed.
     */
    @SuppressWarnings("deprecation")
    private void showRestarting() {
        // Android 8.0 refuses an orientation to a see-through activity (it throws).
        if (Build.VERSION.SDK_INT != Build.VERSION_CODES.O) {
            try {
                setRequestedOrientation(ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE);
            } catch (IllegalStateException e) {
                // As it is, then.
            }
        }
        TextView text = new TextView(this);
        text.setText("Opening the world with its mods…");
        text.setTextColor(0xFFFFFFFF);
        text.setTextSize(TypedValue.COMPLEX_UNIT_SP, 18);
        text.setGravity(Gravity.CENTER);
        text.setBackgroundColor(0xFF000000);
        setContentView(text);
        View decor = getWindow().getDecorView();
        if (Build.VERSION.SDK_INT >= 30) {
            getWindow().setDecorFitsSystemWindows(false);
            WindowInsetsController bars = decor.getWindowInsetsController();
            if (bars != null) {
                bars.hide(WindowInsets.Type.systemBars());
                bars.setSystemBarsBehavior(
                        WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
            }
        } else {
            decor.setSystemUiVisibility(View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
                    | View.SYSTEM_UI_FLAG_FULLSCREEN
                    | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
                    | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN
                    | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION);
        }
    }

    /** The file an "Open with" or a share hands to Horizon, if any. */
    @SuppressWarnings("deprecation")
    private static Uri handedFile(Intent intent) {
        if (intent == null) {
            return null;
        }
        if (Intent.ACTION_VIEW.equals(intent.getAction())) {
            return intent.getData();
        }
        if (Intent.ACTION_SEND.equals(intent.getAction())) {
            return intent.getParcelableExtra(Intent.EXTRA_STREAM);
        }
        return null;
    }

    /**
     * Copy a file into the game's import inbox as `name.part`, renamed to `name` once complete,
     * on a thread of its own; then `done` on the UI thread (if any).
     */
    static void receive(final Activity activity, final Uri file, final Runnable done) {
        final File inbox = new File(activity.getFilesDir(), "import-inbox");
        final String name = displayName(activity, file);
        new Thread(new Runnable() {
            @Override
            public void run() {
                inbox.mkdirs();
                File part = new File(inbox, name + ".part");
                try (InputStream in = activity.getContentResolver().openInputStream(file);
                        OutputStream out = new FileOutputStream(part)) {
                    if (in == null) {
                        throw new java.io.IOException("nothing to read");
                    }
                    byte[] buffer = new byte[1 << 16];
                    for (int n; (n = in.read(buffer)) > 0; ) {
                        out.write(buffer, 0, n);
                    }
                } catch (Exception e) {
                    part.delete();
                    android.util.Log.w("starling", "could not receive " + name, e);
                }
                if (part.exists() && !part.renameTo(new File(inbox, name))) {
                    part.delete();
                }
                if (done != null) {
                    activity.runOnUiThread(done);
                }
            }
        }, "starling-receive").start();
    }

    /** The file's name as the app it came from shows it, without folders. */
    private static String displayName(Context context, Uri file) {
        String name = null;
        try (Cursor cursor = context.getContentResolver().query(
                file, new String[] {OpenableColumns.DISPLAY_NAME}, null, null, null)) {
            if (cursor != null && cursor.moveToFirst()) {
                name = cursor.getString(0);
            }
        } catch (Exception e) {
            // A file:// or an odd provider: the path below.
        }
        if (name == null) {
            name = file.getLastPathSegment();
        }
        if (name == null || name.isEmpty()) {
            name = "imported";
        }
        name = name.replaceAll("[/\\\\:*?\"<>|]", "_");
        // ".part" is the inbox's mark for a file still arriving.
        return name.endsWith(".part") ? name + "_" : name;
    }
    /** Native launcher shell. Game rendering and settings remain in the Rust client. */
    public static final class HomeActivity extends Activity {
        private static final int BG = Color.rgb(12, 15, 23);
        private static final int SIDEBAR = Color.rgb(17, 22, 33);
        private static final int PANEL = Color.rgb(25, 32, 46);
        private static final int PANEL_ALT = Color.rgb(32, 42, 60);
        private static final int TEXT = Color.rgb(241, 245, 252);
        private static final int MUTED = Color.rgb(151, 164, 184);
        private static final int ACCENT = Color.rgb(126, 160, 255);
        private static final int GREEN = Color.rgb(104, 211, 165);
        private LinearLayout content;
        private String page = "home";

        @Override protected void onCreate(Bundle state) {
            super.onCreate(state);
            getWindow().setStatusBarColor(BG);
            getWindow().setNavigationBarColor(BG);
            getWindow().getDecorView().setSystemUiVisibility(0);
            showShell();
        }

        private void showShell() {
            LinearLayout root = new LinearLayout(this);
            root.setOrientation(LinearLayout.HORIZONTAL);
            root.setBackgroundColor(BG);

            LinearLayout nav = new LinearLayout(this);
            nav.setOrientation(LinearLayout.VERTICAL);
            nav.setPadding(dp(12), dp(22), dp(12), dp(14));
            nav.setBackgroundColor(SIDEBAR);
            root.addView(nav, new LinearLayout.LayoutParams(dp(190), -1));

            TextView brand = text("STARLING", 18, TEXT, true);
            nav.addView(brand);
            TextView sub = text("HORIZON • ANDROID", 10, ACCENT, true);
            LinearLayout.LayoutParams subP = params(-1, -2);
            subP.topMargin = dp(5);
            nav.addView(sub, subP);

            TextView navLabel = text("WORKSPACE", 10, MUTED, true);
            LinearLayout.LayoutParams labelP = params(-1, -2);
            labelP.topMargin = dp(32);
            labelP.bottomMargin = dp(10);
            nav.addView(navLabel, labelP);
            addNav(nav, "⌂", "Home", "home");
            addNav(nav, "◉", "Accounts", "accounts");
            addNav(nav, "▣", "Instances", "instances");
            addNav(nav, "◇", "Mods & packs", "mods");
            addNav(nav, "⚙", "Settings", "settings");
            addNav(nav, "ⓘ", "About", "about");
            View spacer = new View(this);
            nav.addView(spacer, new LinearLayout.LayoutParams(1, 0, 1));
            nav.addView(text("STARLING PORT", 9, MUTED, true));

            LinearLayout main = new LinearLayout(this);
            main.setOrientation(LinearLayout.VERTICAL);
            main.setPadding(dp(22), dp(18), dp(22), dp(18));
            root.addView(main, new LinearLayout.LayoutParams(0, -1, 1));

            LinearLayout top = new LinearLayout(this);
            top.setGravity(Gravity.CENTER_VERTICAL);
            TextView section = text(pageTitle(page), 20, TEXT, true);
            top.addView(section, new LinearLayout.LayoutParams(0, -2, 1));
            TextView status = text("●  READY", 10, GREEN, true);
            top.addView(status);
            main.addView(top);

            View line = new View(this);
            line.setBackgroundColor(0xFF283246);
            LinearLayout.LayoutParams lineP = params(-1, dp(1));
            lineP.topMargin = dp(15);
            lineP.bottomMargin = dp(14);
            main.addView(line, lineP);

            ScrollView scroll = new ScrollView(this);
            scroll.setFillViewport(true);
            content = new LinearLayout(this);
            content.setOrientation(LinearLayout.VERTICAL);
            content.setPadding(0, 0, 0, dp(20));
            scroll.addView(content);
            main.addView(scroll, new LinearLayout.LayoutParams(-1, 0, 1));
            setContentView(root);
            renderPage();
        }

        private void addNav(LinearLayout nav, String glyph, String label, String target) {
            LinearLayout item = new LinearLayout(this);
            item.setGravity(Gravity.CENTER_VERTICAL);
            item.setPadding(dp(12), 0, dp(8), 0);
            GradientDrawable bg = new GradientDrawable();
            bg.setColor(page.equals(target) ? 0xFF293956 : Color.TRANSPARENT);
            bg.setCornerRadius(dp(13));
            item.setBackground(bg);
            TextView icon = text(glyph, 17, page.equals(target) ? ACCENT : MUTED, true);
            item.addView(icon, params(dp(28), dp(48)));
            TextView name = text(label, 13, page.equals(target) ? TEXT : MUTED, page.equals(target));
            item.addView(name);
            LinearLayout.LayoutParams p = params(-1, dp(48));
            p.bottomMargin = dp(5);
            nav.addView(item, p);
            item.setClickable(true);
            item.setFocusable(true);
            item.setOnClickListener(v -> {
                page = target;
                showShell();
            });
        }

        private String pageTitle(String p) {
            if ("accounts".equals(p)) return "Accounts";
            if ("instances".equals(p)) return "Instances";
            if ("mods".equals(p)) return "Mods & content";
            if ("settings".equals(p)) return "Settings";
            if ("about".equals(p)) return "About Starling";
            return "Dashboard";
        }

        private void renderPage() {
            content.removeAllViews();
            if ("accounts".equals(page)) renderAccounts();
            else if ("instances".equals(page)) {
                addHeading("Game instances");
                addCard("Starling • Default", "The Rust game runtime and current game folder.", "LAUNCH GAME", ACCENT, this::launchGame);
                addNote("Instance management is not yet wired to a separate Rust profile store. The current Starling game folder remains in use.");
            } else if ("mods".equals(page)) {
                addHeading("Mods, shaders & resource packs");
                addCard("Import content", "Open .hmod, .starling, resource packs, worlds and shader packs with Starling.", "OPEN GAME", ACCENT, this::launchGame);
                addNote("Use Android's file manager → Open with → Starling to import supported files.");
            } else if ("settings".equals(page)) {
                addHeading("Launcher preferences");
                addCard("Game settings", "Graphics, controls, video, audio and gameplay settings are owned by the Rust client.", "OPEN IN-GAME SETTINGS", ACCENT, this::launchGame);
                addCard("Storage", "Starling uses its private app folder until shared storage access is granted.", "OPEN GAME", PANEL_ALT, this::launchGame);
            } else if ("about".equals(page)) {
                addHeading("Starling for Android");
                addCard("Powered by Horizon Rust", "Native Rust game runtime with an Android launcher shell. Microsoft authentication remains available inside the game.", "PLAY STARLING", ACCENT, this::launchGame);
            } else renderHome();
        }

        private void renderHome() {
            addHeading("Welcome back");
            LinearLayout hero = card(PANEL_ALT);
            addToContent(hero, 18);
            hero.addView(text("STARLING • ANDROID EDITION", 10, ACCENT, true));
            TextView headline = text("Your Minecraft Java,\nyour way.", 28, TEXT, true);
            LinearLayout.LayoutParams hp = params(-1, -2);
            hp.topMargin = dp(12);
            hero.addView(headline, hp);
            TextView body = text("A Rust-powered game runtime with accounts, content and game tools in one place.", 13, MUTED, false);
            LinearLayout.LayoutParams bp = params(-1, -2);
            bp.topMargin = dp(9);
            hero.addView(body, bp);
            TextView play = action("▶   PLAY STARLING", ACCENT, 0xFF111A2B);
            LinearLayout.LayoutParams pp = params(-1, dp(52));
            pp.topMargin = dp(20);
            hero.addView(play, pp);
            play.setOnClickListener(v -> launchGame());

            addSection("QUICK ACCESS");
            addCard("Accounts", "Create and switch local offline profiles, or sign in with Microsoft in-game.", "MANAGE ACCOUNTS", PANEL, () -> { page = "accounts"; showShell(); });
            addCard("Mods & packs", "Import mods, shaders, worlds and resource packs.", "VIEW CONTENT", PANEL, () -> { page = "mods"; showShell(); });
            addCard("Game settings", "Open the Rust client's graphics, controls and gameplay settings.", "OPEN SETTINGS", PANEL, () -> { page = "settings"; showShell(); });
            TextView foot = text("Rust game engine • Android native launcher", 10, MUTED, false);
            foot.setGravity(Gravity.CENTER);
            LinearLayout.LayoutParams fp = params(-1, -2);
            fp.topMargin = dp(20);
            content.addView(foot, fp);
        }

        private void renderAccounts() {
            addHeading("Offline profiles");
            addNote("These are local player identities for single-player, LAN and offline-mode servers. They do not authenticate with Microsoft or unlock online-mode servers.");
            TextView add = action("+   ADD OFFLINE ACCOUNT", ACCENT, 0xFF111A2B);
            LinearLayout.LayoutParams ap = params(-1, dp(50));
            ap.topMargin = dp(14);
            content.addView(add, ap);
            add.setOnClickListener(v -> addOfflineProfile());

            java.util.List<String> names = readProfiles();
            String active = readActiveName();
            if (names.isEmpty()) {
                addCard("No offline accounts yet", "Create an offline profile and choose the username you want the game to use.", "CREATE PROFILE", PANEL, this::addOfflineProfile);
            } else {
                addSection("SAVED PROFILES");
                for (String name : names) {
                    final String profile = name;
                    boolean selected = profile.equals(active);
                    addCard((selected ? "✓  " : "◉  ") + profile,
                            selected ? "Currently selected offline identity" : "Saved local profile",
                            selected ? "ACTIVE" : "USE THIS ACCOUNT",
                            selected ? PANEL_ALT : PANEL,
                            () -> selectOfflineProfile(profile));
                }
            }
            addCard("Microsoft account", "For online-mode servers and official game ownership features, use the Microsoft sign-in inside Starling.", "OPEN GAME", PANEL, this::launchGame);
        }

        private java.io.File profileFile() {
            return new java.io.File(new java.io.File(getFilesDir(), "accounts"), "offline-profiles.txt");
        }

        private java.util.List<String> readProfiles() {
            java.util.List<String> out = new java.util.ArrayList<>();
            java.io.File file = profileFile();
            if (!file.isFile()) return out;
            try (java.io.BufferedReader reader = new java.io.BufferedReader(new java.io.FileReader(file))) {
                for (String line; (line = reader.readLine()) != null;) {
                    String name = line.trim();
                    if (validName(name) && !out.contains(name)) out.add(name);
                }
            } catch (Exception ignored) { }
            return out;
        }

        private String readActiveName() {
            java.io.File folder = StarlingActivity.sharedFolder(this) == null
                    ? getFilesDir() : new java.io.File(StarlingActivity.sharedFolder(this));
            java.io.File file = new java.io.File(folder, "horizon-player.txt");
            if (!file.isFile()) return "";
            try (java.io.BufferedReader reader = new java.io.BufferedReader(new java.io.FileReader(file))) {
                for (String line; (line = reader.readLine()) != null;) {
                    if (line.startsWith("name:")) return line.substring(5).trim();
                }
            } catch (Exception ignored) { }
            return "";
        }

        private boolean validName(String name) {
            return name != null && name.matches("[A-Za-z0-9_]{3,16}");
        }

        private void addOfflineProfile() {
            EditText input = new EditText(this);
            input.setSingleLine(true);
            input.setTextColor(TEXT);
            input.setHintTextColor(MUTED);
            input.setHint("Username (3–16 letters, numbers or _)");
            input.setPadding(dp(14), dp(12), dp(14), dp(12));
            input.setBackgroundTintList(android.content.res.ColorStateList.valueOf(ACCENT));
            AlertDialog dialog = new AlertDialog.Builder(this)
                    .setTitle("Create offline account")
                    .setMessage("Choose a local player name. This is not a Microsoft account.")
                    .setView(input)
                    .setNegativeButton("CANCEL", null)
                    .setPositiveButton("CREATE", null)
                    .create();
            dialog.setOnShowListener(d -> dialog.getButton(AlertDialog.BUTTON_POSITIVE).setOnClickListener(v -> {
                String name = input.getText().toString().trim();
                if (!validName(name)) {
                    input.setError("Use 3–16 letters, numbers or underscore.");
                    return;
                }
                java.util.List<String> names = readProfiles();
                if (!names.contains(name)) names.add(name);
                if (!writeProfiles(names)) {
                    input.setError("Could not save this profile.");
                    return;
                }
                if (!selectOfflineProfile(name)) return;
                dialog.dismiss();
                page = "accounts";
                showShell();
            }));
            dialog.show();
        }

        private boolean writeProfiles(java.util.List<String> names) {
            try {
                java.io.File file = profileFile();
                java.io.File parent = file.getParentFile();
                if (parent != null) parent.mkdirs();
                try (java.io.FileWriter writer = new java.io.FileWriter(file, false)) {
                    for (String name : names) writer.write(name + "\n");
                }
                return true;
            } catch (Exception e) {
                android.util.Log.e("starling", "could not save offline profiles", e);
                return false;
            }
        }

        private boolean selectOfflineProfile(String name) {
            if (!validName(name)) return false;
            java.io.File privateAccounts = new java.io.File(getFilesDir(), "accounts");
            java.io.File marker = new java.io.File(privateAccounts, "offline-mode");
            java.io.File home;
            String shared = StarlingActivity.sharedFolder(this);
            home = shared == null ? getFilesDir() : new java.io.File(shared);
            try {
                privateAccounts.mkdirs();
                marker.createNewFile();
                java.io.File player = new java.io.File(home, "horizon-player.txt");
                try (java.io.FileWriter writer = new java.io.FileWriter(player, false)) {
                    writer.write("name:" + name + "\nchosen:true\n");
                }
                return true;
            } catch (Exception e) {
                android.util.Log.e("starling", "could not activate offline profile", e);
                new AlertDialog.Builder(this).setMessage("Could not activate offline account: " + e.getMessage()).setPositiveButton("OK", null).show();
                return false;
            }
        }

        private void addHeading(String value) {
            TextView heading = text(value, 25, TEXT, true);
            LinearLayout.LayoutParams p = params(-1, -2);
            p.bottomMargin = dp(14);
            content.addView(heading, p);
        }

        private void addSection(String value) {
            TextView label = text(value, 10, MUTED, true);
            LinearLayout.LayoutParams p = params(-1, -2);
            p.topMargin = dp(22);
            p.bottomMargin = dp(8);
            content.addView(label, p);
        }

        private void addNote(String value) {
            TextView note = text(value, 12, MUTED, false);
            note.setLineSpacing(dp(3), 1f);
            LinearLayout.LayoutParams p = params(-1, -2);
            p.bottomMargin = dp(10);
            content.addView(note, p);
        }

        private void addCard(String heading, String description, String actionLabel, int color, Runnable click) {
            LinearLayout box = card(color);
            addToContent(box, 10);
            box.addView(text(heading, 16, TEXT, true));
            TextView body = text(description, 12, MUTED, false);
            body.setLineSpacing(dp(2), 1f);
            LinearLayout.LayoutParams bp = params(-1, -2);
            bp.topMargin = dp(7);
            box.addView(body, bp);
            TextView action = text(actionLabel + "  →", 10, ACCENT, true);
            LinearLayout.LayoutParams ap = params(-1, -2);
            ap.topMargin = dp(13);
            box.addView(action, ap);
            box.setClickable(true);
            box.setFocusable(true);
            box.setOnClickListener(v -> click.run());
        }

        private void addToContent(View view, int top) {
            LinearLayout.LayoutParams p = params(-1, -2);
            p.topMargin = dp(top);
            content.addView(view, p);
        }

        private LinearLayout card(int color) {
            LinearLayout box = new LinearLayout(this);
            box.setOrientation(LinearLayout.VERTICAL);
            box.setPadding(dp(17), dp(16), dp(17), dp(16));
            GradientDrawable shape = new GradientDrawable();
            shape.setColor(color);
            shape.setCornerRadius(dp(18));
            box.setBackground(shape);
            return box;
        }

        private TextView action(String label, int background, int foreground) {
            TextView view = new TextView(this);
            view.setText(label);
            view.setTextColor(foreground);
            view.setTextSize(12);
            view.setTypeface(Typeface.DEFAULT, Typeface.BOLD);
            view.setGravity(Gravity.CENTER);
            GradientDrawable shape = new GradientDrawable();
            shape.setColor(background);
            shape.setCornerRadius(dp(14));
            view.setBackground(shape);
            view.setClickable(true);
            view.setFocusable(true);
            return view;
        }

        private TextView text(String value, int size, int color, boolean bold) {
            TextView view = new TextView(this);
            view.setText(value);
            view.setTextSize(size);
            view.setTextColor(color);
            view.setTypeface(Typeface.DEFAULT, bold ? Typeface.BOLD : Typeface.NORMAL);
            return view;
        }

        private void launchGame() {
            startActivity(new Intent(this, StarlingActivity.class));
        }

        private LinearLayout.LayoutParams params(int width, int height) {
            return new LinearLayout.LayoutParams(width, height);
        }

        private int dp(int value) {
            return (int) (value * getResources().getDisplayMetrics().density + 0.5f);
        }
    }

}
