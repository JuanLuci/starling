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
    /** Main launcher dashboard. Kept nested so the public-tree guard can distinguish app glue
     * from recovered Minecraft Java sources. The game itself remains the Rust NativeActivity. */
    public static final class HomeActivity extends Activity {
        private static final int BG = Color.rgb(12, 15, 22);
        private static final int PANEL = Color.rgb(24, 29, 40);
        private static final int PANEL_ALT = Color.rgb(31, 38, 52);
        private static final int TEXT = Color.rgb(242, 245, 250);
        private static final int MUTED = Color.rgb(157, 168, 186);
        private static final int ACCENT = Color.rgb(119, 155, 255);

        @Override protected void onCreate(Bundle state) {
            super.onCreate(state);
            getWindow().setStatusBarColor(BG);
            getWindow().setNavigationBarColor(BG);
            getWindow().getDecorView().setSystemUiVisibility(0);
            showHome();
        }

        private void showHome() {
            ScrollView scroll = new ScrollView(this);
            scroll.setFillViewport(true);
            scroll.setBackgroundColor(BG);
            LinearLayout page = new LinearLayout(this);
            page.setOrientation(LinearLayout.VERTICAL);
            page.setPadding(dp(22), dp(28), dp(22), dp(28));
            scroll.addView(page);

            page.addView(text("HORIZON • ANDROID", 12, ACCENT, true));
            TextView title = text("Starling", 34, TEXT, true);
            LinearLayout.LayoutParams titleParams = params(-2, -2);
            titleParams.topMargin = dp(7);
            page.addView(title, titleParams);
            TextView subtitle = text("Minecraft Java, rebuilt for Android with Rust.", 14, MUTED, false);
            LinearLayout.LayoutParams subParams = params(-1, -2);
            subParams.topMargin = dp(5);
            page.addView(subtitle, subParams);

            LinearLayout hero = card(PANEL_ALT);
            LinearLayout.LayoutParams heroParams = params(-1, -2);
            heroParams.topMargin = dp(26);
            page.addView(hero, heroParams);
            hero.addView(text("READY WHEN YOU ARE", 11, ACCENT, true));
            TextView heroTitle = text("Your next world is waiting.", 21, TEXT, true);
            LinearLayout.LayoutParams heroTitleParams = params(-1, -2);
            heroTitleParams.topMargin = dp(9);
            hero.addView(heroTitle, heroTitleParams);
            TextView heroBody = text("Start the Rust game engine. Worlds, packs and game options stay managed by Starling.", 13, MUTED, false);
            LinearLayout.LayoutParams bodyParams = params(-1, -2);
            bodyParams.topMargin = dp(7);
            hero.addView(heroBody, bodyParams);
            TextView play = action("PLAY STARLING", ACCENT, Color.rgb(15, 22, 38));
            LinearLayout.LayoutParams playParams = params(-1, dp(54));
            playParams.topMargin = dp(18);
            hero.addView(play, playParams);
            play.setOnClickListener(v -> launchGame());

            TextView section = text("QUICK ACCESS", 12, MUTED, true);
            LinearLayout.LayoutParams sectionParams = params(-1, -2);
            sectionParams.topMargin = dp(28);
            page.addView(section, sectionParams);
            page.addView(infoCard("Game settings", "Graphics, controls, shaders and gameplay options are handled by the Rust game.", "OPEN GAME SETTINGS", true));
            page.addView(infoCard("Mods & content", "Open .hmod, .starling, resource packs and shader packs with Starling to import them.", "IMPORT THROUGH FILE MANAGER", false));
            page.addView(infoCard("Accounts & servers", "Sign-in, offline profiles and multiplayer connection remain in the Rust game flow.", "OPEN STARLING", true));

            TextView footer = text("Unofficial Android port • Powered by Horizon Rust", 11, MUTED, false);
            footer.setGravity(Gravity.CENTER);
            LinearLayout.LayoutParams footerParams = params(-1, -2);
            footerParams.topMargin = dp(26);
            page.addView(footer, footerParams);
            setContentView(scroll);
        }

        private View infoCard(String heading, String description, String action, final boolean openGame) {
            LinearLayout box = card(PANEL);
            LinearLayout.LayoutParams boxParams = params(-1, -2);
            boxParams.topMargin = dp(12);
            box.setLayoutParams(boxParams);
            box.addView(text(heading, 17, TEXT, true));
            TextView body = text(description, 13, MUTED, false);
            LinearLayout.LayoutParams bodyParams = params(-1, -2);
            bodyParams.topMargin = dp(6);
            box.addView(body, bodyParams);
            TextView link = text(action + (openGame ? "  →" : ""), 11, ACCENT, true);
            LinearLayout.LayoutParams linkParams = params(-1, -2);
            linkParams.topMargin = dp(14);
            box.addView(link, linkParams);
            if (openGame) {
                box.setOnClickListener(v -> launchGame());
                box.setFocusable(true);
                box.setClickable(true);
            }
            return box;
        }

        private LinearLayout card(int color) {
            LinearLayout box = new LinearLayout(this);
            box.setOrientation(LinearLayout.VERTICAL);
            box.setPadding(dp(18), dp(18), dp(18), dp(18));
            GradientDrawable shape = new GradientDrawable();
            shape.setColor(color);
            shape.setCornerRadius(dp(20));
            box.setBackground(shape);
            return box;
        }

        private TextView action(String label, int background, int foreground) {
            TextView view = new TextView(this);
            view.setText(label);
            view.setTextColor(foreground);
            view.setTextSize(13);
            view.setTypeface(Typeface.DEFAULT, Typeface.BOLD);
            view.setGravity(Gravity.CENTER);
            GradientDrawable shape = new GradientDrawable();
            shape.setColor(background);
            shape.setCornerRadius(dp(15));
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
