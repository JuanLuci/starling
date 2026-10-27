package io.github.juanluci.starling;

import android.Manifest;
import android.app.NativeActivity;
import android.content.ActivityNotFoundException;
import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.Context;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.net.ConnectivityManager;
import android.net.LinkProperties;
import android.net.Network;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.os.Environment;
import android.provider.DocumentsContract;
import android.view.View;
import android.view.WindowInsets;
import android.view.WindowInsetsController;
import android.view.ViewTreeObserver;
import android.view.inputmethod.InputMethodManager;
import android.view.WindowManager;
import android.security.keystore.KeyGenParameterSpec;
import android.security.keystore.KeyProperties;
import java.io.File;
import java.net.InetAddress;
import java.util.List;
import java.security.Key;
import java.security.KeyStore;
import javax.crypto.Cipher;
import javax.crypto.KeyGenerator;
import javax.crypto.SecretKey;
import javax.crypto.spec.GCMParameterSpec;

/**
 * The game runs in libhorizon_android (NativeActivity); this only does what the NDK cannot:
 * tells the game where its folder is (Starling in shared storage once the player allows it), hides the
 * system bars, keeps the screen on while the game is in front, tells the game how tall the
 * on-screen keyboard is (it no longer shrinks the window since edge-to-edge), and ends the
 * process with the activity, since the game's event loop cannot start twice in one process.
 * The game calls the public methods from its own thread (android_lib.rs): the file manager, the
 * file picker, the clipboard, the browser and the keystore.
 */
public final class StarlingActivity extends NativeActivity {
    static {
        // Registers the library with this class loader, so the native methods below resolve.
        System.loadLibrary("horizon_android");
    }

    private static final int PICK_FILE = 1;

    /** The keyboard's height in window pixels, 0 when it is hidden (android_lib.rs). */
    private static native void onImeHeight(int pixels);

    /** The game folder in shared storage, or null to keep it in the app's own files. */
    private static native void onStorage(String folder);

    private ClipboardManager clipboard;

    /** The game has drawn: the window may show (firstFrame). */
    private volatile boolean drawn;

    /** Shared storage's Starling folder when the player allowed the app to use it, else null. */
    static String sharedFolder(Context context) {
        boolean allowed = Build.VERSION.SDK_INT >= 30
                ? Environment.isExternalStorageManager()
                : context.checkSelfPermission(Manifest.permission.WRITE_EXTERNAL_STORAGE)
                        == PackageManager.PERMISSION_GRANTED;
        return allowed
                ? new File(Environment.getExternalStorageDirectory(), "Starling").getPath()
                : null;
    }

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        // Before super.onCreate starts the game's thread, which reads it first thing.
        onStorage(sharedFolder(this));
        super.onCreate(savedInstanceState);
        // Made here, on the UI thread: the game reads and writes it from its own.
        clipboard = (ClipboardManager) getSystemService(Context.CLIPBOARD_SERVICE);
        // As games do: watching, a controller or a benchmark can go minutes without a touch.
        getWindow().addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        // A NativeActivity's window counts as drawn at once, so the system's starting window (the
        // app icon) went and the screen behind showed until the game's first frame: the window
        // is held back until the game says it has drawn, or ten seconds at most.
        final View decor = getWindow().getDecorView();
        decor.getViewTreeObserver().addOnPreDrawListener(new ViewTreeObserver.OnPreDrawListener() {
            @Override
            public boolean onPreDraw() {
                if (!drawn) {
                    return false;
                }
                decor.getViewTreeObserver().removeOnPreDrawListener(this);
                return true;
            }
        });
        decor.postDelayed(new Runnable() {
            @Override
            public void run() {
                firstFrame();
            }
        }, 10000);
        if (Build.VERSION.SDK_INT >= 28) {
            WindowManager.LayoutParams attributes = getWindow().getAttributes();
            attributes.layoutInDisplayCutoutMode =
                    WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;
            getWindow().setAttributes(attributes);
        }
        if (Build.VERSION.SDK_INT >= 30) {
            getWindow().getDecorView().setOnApplyWindowInsetsListener(
                    new View.OnApplyWindowInsetsListener() {
                        @Override
                        public WindowInsets onApplyWindowInsets(View view, WindowInsets insets) {
                            onImeHeight(insets.getInsets(WindowInsets.Type.ime()).bottom);
                            // The keyboard brings the navigation bar back, and it stayed after
                            // the keyboard went: home sat beside the chat's keys.
                            if (insets.isVisible(WindowInsets.Type.navigationBars())
                                    || insets.isVisible(WindowInsets.Type.statusBars())) {
                                view.post(new Runnable() {
                                    @Override
                                    public void run() {
                                        hideSystemBars();
                                    }
                                });
                            }
                            return view.onApplyWindowInsets(insets);
                        }
                    });
        }
        hideSystemBars();
    }

    @Override
    public void onWindowFocusChanged(boolean hasFocus) {
        super.onWindowFocusChanged(hasFocus);
        if (hasFocus) {
            hideSystemBars();
        }
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (requestCode == PICK_FILE && resultCode == RESULT_OK && data != null
                && data.getData() != null) {
            // The game shows the copy arriving in its inbox, then the import.
            LauncherActivity.receive(this, data.getData(), null);
        }
    }

    @Override
    protected void onDestroy() {
        // The world was saved when the window went away (the game's `suspended`).
        android.os.Process.killProcess(android.os.Process.myPid());
        super.onDestroy();
    }

    /** Put the on-screen keyboard away, however it was shown. */
    public void hideKeyboard() {
        runOnUiThread(new Runnable() {
            @Override
            public void run() {
                InputMethodManager keyboard =
                        (InputMethodManager) getSystemService(Context.INPUT_METHOD_SERVICE);
                if (keyboard != null) {
                    keyboard.hideSoftInputFromWindow(getWindow().getDecorView().getWindowToken(), 0);
                }
            }
        });
    }

    /** The game drew its first frame: let the window show. */
    public void firstFrame() {
        runOnUiThread(new Runnable() {
            @Override
            public void run() {
                drawn = true;
                getWindow().getDecorView().invalidate();
            }
        });
    }

    /** Whether the player allowed the Starling folder (they may have just come back from it). */
    public boolean fileAccessAllowed() {
        return sharedFolder(this) != null;
    }

    /**
     * Android's setting that allows the Starling folder, in a task of its own: home and Starling's
     * icon bring the player back to the game, not to the setting.
     */
    public void openFileAccessSettings() {
        if (Build.VERSION.SDK_INT < 30) {
            runOnUiThread(new Runnable() {
                @Override
                public void run() {
                    requestPermissions(
                            new String[] {Manifest.permission.WRITE_EXTERNAL_STORAGE}, 2);
                }
            });
            return;
        }
        Intent[] settings = {
            new Intent(android.provider.Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION,
                    Uri.parse("package:" + getPackageName())),
            new Intent(android.provider.Settings.ACTION_MANAGE_ALL_FILES_ACCESS_PERMISSION),
        };
        for (Intent intent : settings) {
            intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            try {
                startActivity(intent);
                return;
            } catch (ActivityNotFoundException e) {
                // The general list, then.
            }
        }
    }

    /**
     * Start the game again in a new process (the game folder is chosen at start): LauncherActivity,
     * in a process of its own, waits for this one to end.
     */
    public void restart() {
        Intent again = new Intent(this, LauncherActivity.class);
        again.putExtra(LauncherActivity.AFTER, android.os.Process.myPid());
        again.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
        startActivity(again);
        runOnUiThread(new Runnable() {
            @Override
            public void run() {
                finish();
            }
        });
    }

    /** The system's file picker; the file chosen goes to the game's import inbox. */
    public boolean pickFile() {
        runOnUiThread(new Runnable() {
            @Override
            public void run() {
                Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
                intent.addCategory(Intent.CATEGORY_OPENABLE);
                intent.setType("*/*");
                try {
                    startActivityForResult(intent, PICK_FILE);
                } catch (ActivityNotFoundException e) {
                    // No picker: nothing to show.
                }
            }
        });
        return true;
    }

    /** The system's file manager on a folder of shared storage; false where none can show it. */
    public boolean openFolder(String path) {
        String root = Environment.getExternalStorageDirectory().getPath();
        if (!path.startsWith(root)) {
            return false;
        }
        String relative = path.substring(root.length()).replaceFirst("^/+", "");
        Uri folder = DocumentsContract.buildDocumentUri(
                "com.android.externalstorage.documents", "primary:" + relative);
        // Any file manager that takes a folder, else the system's own (Files).
        String[] packages = {null, "com.google.android.documentsui", "com.android.documentsui"};
        for (String app : packages) {
            Intent intent = new Intent(Intent.ACTION_VIEW);
            intent.setDataAndType(folder, DocumentsContract.Document.MIME_TYPE_DIR);
            intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_GRANT_READ_URI_PERMISSION);
            if (app != null) {
                intent.setPackage(app);
            }
            try {
                startActivity(intent);
                return true;
            } catch (ActivityNotFoundException | SecurityException e) {
                // The next one.
            }
        }
        return false;
    }

    /** The system's browser on an https:// page (the Microsoft sign-in); false where none can. */
    public boolean openLink(String url) {
        if (!url.startsWith("https://")) {
            return false;
        }
        Intent intent = new Intent(Intent.ACTION_VIEW, Uri.parse(url));
        intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
        try {
            startActivity(intent);
            return true;
        } catch (ActivityNotFoundException | SecurityException e) {
            return false;
        }
    }

    /** The active network's DNS servers, for the SRV records that name most big servers
     * (Android has no resolv.conf to read them from); empty when there is no network. */
    public String[] dnsServers() {
        ConnectivityManager connectivity =
                (ConnectivityManager) getSystemService(Context.CONNECTIVITY_SERVICE);
        Network network = connectivity == null ? null : connectivity.getActiveNetwork();
        LinkProperties link = network == null ? null : connectivity.getLinkProperties(network);
        if (link == null) {
            return new String[0];
        }
        List<InetAddress> servers = link.getDnsServers();
        String[] out = new String[servers.size()];
        for (int i = 0; i < out.length; i++) {
            out[i] = servers.get(i).getHostAddress();
        }
        return out;
    }

    /** The clipboard's text, or null when it holds none. */
    public String clipboardText() {
        ClipData clip = clipboard == null ? null : clipboard.getPrimaryClip();
        if (clip == null || clip.getItemCount() == 0) {
            return null;
        }
        CharSequence text = clip.getItemAt(0).coerceToText(this);
        return text == null ? null : text.toString();
    }

    public boolean setClipboardText(String text) {
        if (clipboard == null) {
            return false;
        }
        clipboard.setPrimaryClip(ClipData.newPlainText("Starling", text));
        return true;
    }

    /** The Android Keystore key the account's tokens are sealed with; it never leaves the
     * Keystore. */
    private static final String SECRET_KEY = "starling-account";

    private static SecretKey secretKey() throws Exception {
        KeyStore store = KeyStore.getInstance("AndroidKeyStore");
        store.load(null);
        Key key = store.getKey(SECRET_KEY, null);
        if (key instanceof SecretKey) {
            return (SecretKey) key;
        }
        KeyGenerator generator =
                KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore");
        generator.init(new KeyGenParameterSpec.Builder(SECRET_KEY,
                KeyProperties.PURPOSE_ENCRYPT | KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build());
        return generator.generateKey();
    }

    /** `data` sealed with the Keystore key (AES-GCM): the IV's length, the IV, then the
     * ciphertext and its tag. Null when the Keystore refused. */
    public byte[] sealSecret(byte[] data) {
        try {
            Cipher cipher = Cipher.getInstance("AES/GCM/NoPadding");
            cipher.init(Cipher.ENCRYPT_MODE, secretKey());
            byte[] iv = cipher.getIV();
            byte[] sealed = cipher.doFinal(data);
            byte[] out = new byte[1 + iv.length + sealed.length];
            out[0] = (byte) iv.length;
            System.arraycopy(iv, 0, out, 1, iv.length);
            System.arraycopy(sealed, 0, out, 1 + iv.length, sealed.length);
            return out;
        } catch (Exception e) {
            return null;
        }
    }

    /** What sealSecret sealed, or null when it was not sealed here (or was changed). */
    public byte[] openSecret(byte[] sealed) {
        try {
            int ivLength = sealed[0] & 0xFF;
            Cipher cipher = Cipher.getInstance("AES/GCM/NoPadding");
            cipher.init(Cipher.DECRYPT_MODE, secretKey(),
                    new GCMParameterSpec(128, sealed, 1, ivLength));
            return cipher.doFinal(sealed, 1 + ivLength, sealed.length - 1 - ivLength);
        } catch (Exception e) {
            return null;
        }
    }

    @SuppressWarnings("deprecation")
    private void hideSystemBars() {
        if (Build.VERSION.SDK_INT >= 30) {
            WindowInsetsController controller = getWindow().getInsetsController();
            if (controller != null) {
                controller.hide(WindowInsets.Type.systemBars());
                controller.setSystemBarsBehavior(
                        WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
            }
        } else {
            getWindow().getDecorView().setSystemUiVisibility(
                    View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
                            | View.SYSTEM_UI_FLAG_FULLSCREEN
                            | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
                            | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN
                            | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
                            | View.SYSTEM_UI_FLAG_LAYOUT_STABLE);
        }
    }
}
