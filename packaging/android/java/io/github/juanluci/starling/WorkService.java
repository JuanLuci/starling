package io.github.juanluci.starling;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.Service;
import android.content.Context;
import android.content.Intent;
import android.content.pm.ServiceInfo;
import android.os.Build;
import android.os.IBinder;
import java.util.LinkedHashMap;
import java.util.Map;

/**
 * Keeps the game's process in the foreground, with a notification, while it waits for something
 * that happens outside it: the Microsoft sign-in in the browser, the game files downloading.
 * Many phones cut a background app's internet, or end it (battery and data savers, the maker's
 * own rules: a sign-in stopped with "Could not reach Microsoft"); a foreground service is spared
 * both. It runs while one reason holds it (StarlingActivity.keepWorking, from the game).
 */
public final class WorkService extends Service {
    private static final String CHANNEL = "work";
    private static final int NOTIFICATION = 1;

    /** Why it runs, each with the line its notification shows; the newest shows. */
    private static final Map<String, String> REASONS = new LinkedHashMap<>();

    /** Holds the service for `reason` (with `line` shown), or lets it go. */
    static void hold(Context context, String reason, String line, boolean on) {
        boolean running;
        synchronized (REASONS) {
            if (on) {
                REASONS.remove(reason);
                REASONS.put(reason, line);
            } else {
                REASONS.remove(reason);
            }
            running = !REASONS.isEmpty();
        }
        Intent intent = new Intent(context, WorkService.class);
        try {
            if (running) {
                if (Build.VERSION.SDK_INT >= 26) {
                    context.startForegroundService(intent);
                } else {
                    context.startService(intent);
                }
            } else {
                context.stopService(intent);
            }
        } catch (RuntimeException e) {
            // Android refused (started from the background): the wait goes on without it.
        }
    }

    private static String line() {
        synchronized (REASONS) {
            String last = "Starling is working";
            for (String line : REASONS.values()) {
                last = line;
            }
            return last;
        }
    }

    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        Notification.Builder builder;
        if (Build.VERSION.SDK_INT >= 26) {
            NotificationManager manager =
                    (NotificationManager) getSystemService(Context.NOTIFICATION_SERVICE);
            if (manager != null) {
                manager.createNotificationChannel(new NotificationChannel(
                        CHANNEL, "Starling at work", NotificationManager.IMPORTANCE_LOW));
            }
            builder = new Notification.Builder(this, CHANNEL);
        } else {
            builder = new Notification.Builder(this);
        }
        Notification notification = builder
                .setSmallIcon(getApplicationInfo().icon)
                .setContentTitle("Starling")
                .setContentText(line())
                .setOngoing(true)
                .build();
        try {
            if (Build.VERSION.SDK_INT >= 29) {
                startForeground(NOTIFICATION, notification,
                        ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC);
            } else {
                startForeground(NOTIFICATION, notification);
            }
        } catch (RuntimeException e) {
            stopSelf();
        }
        return START_NOT_STICKY;
    }

    @Override
    public IBinder onBind(Intent intent) {
        return null;
    }
}
