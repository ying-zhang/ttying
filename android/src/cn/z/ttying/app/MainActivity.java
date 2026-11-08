
package cn.z.ttying;

import android.Manifest;
import android.app.Activity;
import android.content.ComponentName;
import android.content.Context;
import android.content.Intent;
import android.content.ServiceConnection;
import android.content.pm.PackageManager;
import android.graphics.Color;
import android.graphics.Typeface;
import android.graphics.Insets;
import android.graphics.drawable.GradientDrawable;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.os.Handler;
import android.os.IBinder;
import android.os.Looper;
import android.view.Gravity;
import android.view.View;
import android.view.WindowInsets;
import android.widget.Button;
import android.widget.FrameLayout;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;
import org.json.JSONObject;

public class MainActivity extends Activity {
    private TextView statusTitle;
    private TextView statusDetail;
    private Button primaryButton;
    private Button openButton;
    private TtyingService.LocalBinder binder;
    private boolean starting = false;
    private boolean stopping = false;
    private boolean serviceRunning = false;
    private final Handler handler = new Handler(Looper.getMainLooper());
    private final Runnable poll = new Runnable() {
        @Override public void run() {
            refresh();
            handler.postDelayed(this, 1000);
        }
    };
    private final ServiceConnection connection = new ServiceConnection() {
        @Override public void onServiceConnected(ComponentName name, IBinder service) {
            binder = (TtyingService.LocalBinder) service;
            refresh();
        }
        @Override public void onServiceDisconnected(ComponentName name) {
            binder = null;
            refresh();
        }
    };

    @Override protected void onCreate(Bundle state) {
        super.onCreate(state);
        if (Build.VERSION.SDK_INT >= 33 &&
            checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
            requestPermissions(new String[]{Manifest.permission.POST_NOTIFICATIONS}, 10);
        }
        setContentView(buildContent());
        bindService(new Intent(this, TtyingService.class), connection, Context.BIND_AUTO_CREATE);
        handler.post(poll);
    }

    private int dp(int value) {
        return Math.round(value * getResources().getDisplayMetrics().density);
    }

    private GradientDrawable rounded(int fill, int stroke) {
        GradientDrawable shape = new GradientDrawable();
        shape.setShape(GradientDrawable.RECTANGLE);
        shape.setCornerRadius(dp(18));
        shape.setColor(fill);
        shape.setStroke(dp(1), stroke);
        return shape;
    }

    private GradientDrawable circle(int startColor, int endColor) {
        GradientDrawable shape = new GradientDrawable(GradientDrawable.Orientation.TL_BR, new int[]{startColor, endColor});
        shape.setShape(GradientDrawable.OVAL);
        shape.setStroke(dp(2), 0x5522D3EE);
        return shape;
    }

    private TextView text(String value, int sp, int color) {
        TextView view = new TextView(this);
        view.setText(value);
        view.setTextSize(sp);
        view.setTextColor(color);
        view.setLineSpacing(dp(2), 1.0f);
        return view;
    }

    private LinearLayout.LayoutParams margins(int width, int height, int left, int top, int right, int bottom) {
        LinearLayout.LayoutParams params = new LinearLayout.LayoutParams(width, height);
        params.setMargins(dp(left), dp(top), dp(right), dp(bottom));
        return params;
    }

    private View buildContent() {
        ScrollView scroll = new ScrollView(this);
        scroll.setFillViewport(true);
        scroll.setBackgroundColor(0xFF0F172A);

        LinearLayout root = new LinearLayout(this);
        root.setOrientation(LinearLayout.VERTICAL);
        root.setPadding(dp(24), dp(28), dp(24), dp(22));
        root.setLayoutParams(new ScrollView.LayoutParams(
            ScrollView.LayoutParams.MATCH_PARENT, ScrollView.LayoutParams.MATCH_PARENT));
        scroll.addView(root);
        if (Build.VERSION.SDK_INT >= 30) {
            scroll.setOnApplyWindowInsetsListener((view, insets) -> {
                Insets bars = insets.getInsets(WindowInsets.Type.systemBars());
                root.setPadding(dp(24), dp(28) + bars.top, dp(24), dp(22) + bars.bottom);
                return insets;
            });
            scroll.requestApplyInsets();
        } else {
            scroll.setFitsSystemWindows(true);
        }

        TextView title = text("ttying", 32, 0xFFF8FAFC);
        title.setTypeface(Typeface.DEFAULT_BOLD);
        root.addView(title);

        TextView subtitle = text("本机 Web 终端 · 仅本机访问", 14, 0xFF94A3B8);
        root.addView(subtitle, margins(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT, 0, 4, 0, 22));

        LinearLayout card = new LinearLayout(this);
        card.setOrientation(LinearLayout.VERTICAL);
        card.setPadding(dp(18), dp(16), dp(18), dp(16));
        card.setBackground(rounded(0xFF172033, 0xFF334155));
        card.setElevation(dp(4));
        root.addView(card, margins(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT, 0, 0, 0, 8));

        TextView label = text("服务状态", 12, 0xFF64748B);
        label.setLetterSpacing(0.12f);
        card.addView(label);

        statusTitle = text("正在检查…", 24, 0xFFCBD5E1);
        statusTitle.setTypeface(Typeface.DEFAULT_BOLD);
        card.addView(statusTitle, margins(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT, 0, 4, 0, 6));

        statusDetail = text("正在读取服务状态…", 14, 0xFF94A3B8);
        card.addView(statusDetail);

        FrameLayout center = new FrameLayout(this);
        root.addView(center, new LinearLayout.LayoutParams(
            LinearLayout.LayoutParams.MATCH_PARENT, 0, 1.0f));

        primaryButton = new Button(this);
        primaryButton.setAllCaps(false);
        primaryButton.setTextSize(25);
        primaryButton.setTypeface(Typeface.DEFAULT_BOLD);
        primaryButton.setStateListAnimator(null);
        primaryButton.setGravity(Gravity.CENTER);
        primaryButton.setOnClickListener(v -> {
            if (starting || stopping) return;
            if (serviceRunning) stopServer(); else startServer();
        });
        FrameLayout.LayoutParams circleParams = new FrameLayout.LayoutParams(dp(190), dp(190), Gravity.CENTER);
        center.addView(primaryButton, circleParams);

        openButton = new Button(this);
        openButton.setAllCaps(false);
        openButton.setText("在浏览器中打开终端");
        openButton.setTextSize(16);
        openButton.setTextColor(Color.WHITE);
        openButton.setOnClickListener(v -> openBrowser());
        root.addView(openButton, margins(LinearLayout.LayoutParams.MATCH_PARENT, dp(54), 0, 12, 0, 0));

        setPrimaryState("启动", 0xFF0891B2, 0xFF2563EB, true);
        setOpenEnabled(false);
        return scroll;
    }

    private void setPrimaryState(String label, int startColor, int endColor, boolean enabled) {
        primaryButton.setText(label);
        primaryButton.setTextColor(enabled ? Color.WHITE : 0xFF94A3B8);
        primaryButton.setBackground(circle(enabled ? startColor : 0xFF334155, enabled ? endColor : 0xFF334155));
        primaryButton.setEnabled(enabled);
        primaryButton.setAlpha(enabled ? 1.0f : 0.82f);
    }

    private void setOpenEnabled(boolean enabled) {
        openButton.setEnabled(enabled);
        openButton.setTextColor(enabled ? Color.WHITE : 0xFF94A3B8);
        GradientDrawable shape = rounded(enabled ? 0xFF1D4ED8 : 0xFF1E293B, enabled ? 0xFF3B82F6 : 0xFF334155);
        openButton.setBackground(shape);
    }

    private void startServer() {
        starting = true;
        stopping = false;
        statusTitle.setText("正在启动…");
        statusTitle.setTextColor(0xFFFBBF24);
        statusDetail.setText("正在初始化本机终端服务，请稍候。");
        setPrimaryState("启动中", 0xFF475569, 0xFF475569, false);
        setOpenEnabled(false);
        try {
            Intent intent = new Intent(this, TtyingService.class).setAction(TtyingService.ACTION_START);
            if (Build.VERSION.SDK_INT >= 26) startForegroundService(intent); else startService(intent);
        } catch (Exception e) {
            starting = false;
            statusTitle.setText("启动失败");
            statusTitle.setTextColor(0xFFF87171);
            statusDetail.setText(e.getMessage() == null ? "无法启动服务。" : e.getMessage());
            setPrimaryState("重试", 0xFF0891B2, 0xFF2563EB, true);
        }
        handler.postDelayed(() -> {
            if (starting) {
                starting = false;
                refresh();
            }
        }, 8000);
    }

    private void stopServer() {
        stopping = true;
        starting = false;
        statusTitle.setText("正在停止…");
        statusTitle.setTextColor(0xFFFBBF24);
        statusDetail.setText("正在关闭终端服务和相关会话。");
        setPrimaryState("停止中", 0xFF475569, 0xFF475569, false);
        setOpenEnabled(false);
        if (binder != null) {
            binder.stopServer();
        } else {
            stopService(new Intent(this, TtyingService.class));
        }
        handler.postDelayed(() -> {
            if (stopping) {
                stopping = false;
                refresh();
            }
        }, 5000);
    }

    private void refresh() {
        if (binder == null) {
                if (!starting && !stopping) {
                statusTitle.setText(serviceRunning ? "正在连接服务…" : "服务未运行");
                statusTitle.setTextColor(serviceRunning ? 0xFFFBBF24 : 0xFFCBD5E1);
                statusDetail.setText(serviceRunning ? "正在重新连接前台服务。" : "点击中间的圆形按钮启动服务。");
            }
            setPrimaryState(starting ? "启动中" : stopping ? "停止中" : "启动", 0xFF0891B2, 0xFF2563EB, !starting && !stopping);
            setOpenEnabled(false);
            return;
        }
        try {
            JSONObject obj = new JSONObject(binder.getStatus());
            boolean running = obj.optBoolean("running", false);
            int port = obj.optInt("port", 0);
            String error = obj.optString("error", "");
            serviceRunning = running;
            if (!error.isEmpty()) {
                starting = false;
                stopping = false;
                statusTitle.setText("启动失败");
                statusTitle.setTextColor(0xFFF87171);
                statusDetail.setText(error);
                setPrimaryState("重试", 0xFF0891B2, 0xFF2563EB, true);
                setOpenEnabled(false);
            } else if (running) {
                starting = false;
                stopping = false;
                statusTitle.setText("服务运行中");
                statusTitle.setTextColor(0xFF4ADE80);
                statusDetail.setText("端口 " + port + " · 点击下方按钮打开终端。");
                setPrimaryState("停止", 0xFFBE123C, 0xFFE11D48, true);
                setOpenEnabled(true);
            } else if (starting) {
                setPrimaryState("启动中", 0xFF475569, 0xFF475569, false);
                setOpenEnabled(false);
            } else {
                stopping = false;
                statusTitle.setText("服务未运行");
                statusTitle.setTextColor(0xFFCBD5E1);
                statusDetail.setText("点击中间的圆形按钮启动服务。");
                setPrimaryState("启动", 0xFF0891B2, 0xFF2563EB, true);
                setOpenEnabled(false);
            }
        } catch (Exception e) {
            statusTitle.setText("状态读取失败");
            statusTitle.setTextColor(0xFFF87171);
            statusDetail.setText(e.getMessage() == null ? "无法读取服务状态。" : e.getMessage());
            setPrimaryState("重试", 0xFF0891B2, 0xFF2563EB, true);
            setOpenEnabled(false);
        }
    }

    private void openBrowser() {
        if (binder == null) return;
        try {
            JSONObject obj = new JSONObject(binder.getStatus());
            String url = obj.optString("url", "");
            if (!url.isEmpty()) {
                startActivity(new Intent(Intent.ACTION_VIEW, Uri.parse(url)));
            }
        } catch (Exception ignored) { }
    }

    @Override protected void onDestroy() {
        handler.removeCallbacks(poll);
        try { unbindService(connection); } catch (Exception ignored) { }
        super.onDestroy();
    }
}
