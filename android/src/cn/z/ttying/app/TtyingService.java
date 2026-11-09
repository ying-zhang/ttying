package cn.z.ttying;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.app.Service;
import android.content.Intent;
import android.content.pm.ServiceInfo;
import android.os.Binder;
import android.os.Build;
import android.os.IBinder;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

public class TtyingService extends Service {
    public static final String ACTION_START = "cn.z.ttying.START";
    public static final String ACTION_STOP = "cn.z.ttying.STOP";
    private static final String CHANNEL_ID = "ttying";
    private static final int NOTIFICATION_ID = 1001;
    private final LocalBinder binder = new LocalBinder();

    private final ExecutorService worker = Executors.newSingleThreadExecutor();
    private volatile String cachedStatus = "{\"running\":false}";
    public class LocalBinder extends Binder {
        public String getStatus() { return cachedStatus; }
        public void stopServer() { requestStop(); }
    }
    private void updateStatus(String json) { cachedStatus = json; }
    private void requestStop() {
        worker.execute(() -> {
            updateStatus(TtyingNative.stop());
            stopForeground(STOP_FOREGROUND_REMOVE);
            stopSelf();
        });
    }

    @Override
    public void onCreate() {
        super.onCreate();
        createChannel();
    }

    private void createChannel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            NotificationChannel channel = new NotificationChannel(
                CHANNEL_ID, getString(R.string.notification_channel_name), NotificationManager.IMPORTANCE_LOW);
            getSystemService(NotificationManager.class).createNotificationChannel(channel);
        }
    }

    private Notification notification() {
        Intent open = new Intent(this, MainActivity.class);
        PendingIntent pi = PendingIntent.getActivity(
            this, 0, open, PendingIntent.FLAG_IMMUTABLE | PendingIntent.FLAG_UPDATE_CURRENT);
        return new Notification.Builder(this, CHANNEL_ID)
            .setContentTitle(getString(R.string.notification_title))
            .setContentText(getString(R.string.notification_text))
            .setSmallIcon(android.R.drawable.stat_sys_upload)
            .setContentIntent(pi)
            .setOngoing(true)
            .build();
    }

    private void foreground() {
        if (Build.VERSION.SDK_INT >= 34) {
            startForeground(NOTIFICATION_ID, notification(), ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE);
        } else {
            startForeground(NOTIFICATION_ID, notification());
        }
    }

    @Override
    public int onStartCommand(Intent intent, int flags, int startId) {
        String action = intent != null ? intent.getAction() : ACTION_START;
        if (ACTION_STOP.equals(action)) {
            requestStop();
            return START_NOT_STICKY;
        }
        foreground();
        worker.execute(() -> updateStatus(TtyingNative.start("127.0.0.1", 0, "", "")));
        return START_NOT_STICKY;
    }

    @Override
    public IBinder onBind(Intent intent) { return binder; }

    @Override
    public void onDestroy() {
        worker.execute(() -> updateStatus(TtyingNative.stop()));
        worker.shutdown();
        super.onDestroy();
    }
}
