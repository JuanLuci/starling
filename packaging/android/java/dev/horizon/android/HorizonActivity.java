package dev.horizon.android;

import android.app.NativeActivity;
import android.os.Build;
import android.os.Bundle;
import android.view.View;
import android.view.WindowInsets;
import android.view.WindowInsetsController;
import android.view.WindowManager;

/**
 * The game runs in libhorizon_android (NativeActivity); this only does what the NDK cannot:
 * hides the system bars, tells the game how tall the on-screen keyboard is (it no longer
 * shrinks the window since edge-to-edge), and ends the process with the activity, since the
 * game's event loop cannot start twice in one process.
 */
public final class HorizonActivity extends NativeActivity {
    static {
        // Registers the library with this class loader, so the native method below resolves.
        System.loadLibrary("horizon_android");
    }

    /** The keyboard's height in window pixels, 0 when it is hidden (android_lib.rs). */
    private static native void onImeHeight(int pixels);

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
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
    protected void onDestroy() {
        // The world was saved when the window went away (the game's `suspended`).
        android.os.Process.killProcess(android.os.Process.myPid());
        super.onDestroy();
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
