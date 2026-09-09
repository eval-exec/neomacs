package org.neomacs;

import android.app.Activity;
import android.app.Instrumentation;
import android.os.Bundle;
import android.view.SurfaceView;
import android.view.View;
import android.view.ViewGroup;
import androidx.core.graphics.Insets;
import androidx.core.view.ViewCompat;
import androidx.core.view.WindowInsetsCompat;

/** Device regression at the actual drawable/input surface, without launching a VM test host. */
public final class ViewportProbe extends Instrumentation {
    private boolean withKeyboard;
    private boolean rotate;
    @Override public void onCreate(Bundle arguments) {
        super.onCreate(arguments);
        withKeyboard = arguments != null && "true".equals(arguments.getString("keyboard"));
        rotate = arguments != null && "true".equals(arguments.getString("rotate"));
        start();
    }

    @Override public void onStart() {
        Bundle result = new Bundle();
        try {
            Activity activity = startActivitySync(getTargetContext().getPackageManager()
                    .getLaunchIntentForPackage(getTargetContext().getPackageName()));
            waitForIdleSync();
            waitForSurface(activity);
            // With keyboard=true the device driver taps the editor through
            // adb input. Require actual IME visibility, not merely a request.
            final int originalOrientation = activity.getRequestedOrientation();
            if (rotate) runOnMainSync(() -> activity.setRequestedOrientation(
                    android.content.pm.ActivityInfo.SCREEN_ORIENTATION_PORTRAIT));
            try {
                verifyViewport(activity, false);
                if (rotate) {
                    runOnMainSync(() -> activity.setRequestedOrientation(
                            android.content.pm.ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE));
                    verifyViewport(activity, true);
                }
            } finally {
                if (rotate) runOnMainSync(() -> activity.setRequestedOrientation(originalOrientation));
            }
            result.putString("stream", "PASS: drawable surface respects system and IME insets\n");
            finish(Activity.RESULT_OK, result);
        } catch (Throwable error) {
            result.putString("stream", "FAIL: " + error + "\n");
            finish(Activity.RESULT_CANCELED, result);
        }
    }

    private void verifyViewport(Activity activity, boolean landscape) {
        final String[] failure = {"surface did not become ready"};
        for (int attempt = 0; attempt < 100; attempt++) {
            runOnMainSync(() -> {
                View root = activity.getWindow().getDecorView();
                SurfaceView surface = findSurface(root);
                WindowInsetsCompat insets = ViewCompat.getRootWindowInsets(root);
                if (surface == null || insets == null || surface.getWidth() == 0
                        || surface.getHeight() == 0) return;
                if (rotate && (root.getWidth() > root.getHeight()) != landscape) {
                    failure[0] = "requested rotation did not finish";
                    return;
                }
                if (withKeyboard && !insets.isVisible(WindowInsetsCompat.Type.ime())) {
                    failure[0] = "software keyboard did not become visible";
                    return;
                }
                Insets safe = insets.getInsets(WindowInsetsCompat.Type.systemBars()
                        | WindowInsetsCompat.Type.displayCutout() | WindowInsetsCompat.Type.ime());
                int[] position = new int[2];
                surface.getLocationInWindow(position);
                failure[0] = position[0] >= safe.left && position[1] >= safe.top
                        && position[0] + surface.getWidth() <= root.getWidth() - safe.right
                        && position[1] + surface.getHeight() <= root.getHeight() - safe.bottom
                        ? null : "surface overlaps system/IME insets: position="
                        + position[0] + "," + position[1] + " extent="
                        + surface.getWidth() + "x" + surface.getHeight() + " insets=" + safe;
            });
            if (failure[0] == null) return;
            android.os.SystemClock.sleep(50);
        }
        throw new AssertionError((landscape ? "landscape: " : "portrait: ") + failure[0]);
    }

    private void waitForSurface(Activity activity) {
        final boolean[] ready = {false};
        for (int attempt = 0; attempt < 100; attempt++) {
            runOnMainSync(() -> {
                SurfaceView surface = findSurface(activity.getWindow().getDecorView());
                ready[0] = surface != null && surface.getWidth() > 0 && surface.getHeight() > 0
                        && surface.getHolder().getSurface().isValid();
            });
            if (ready[0]) return;
            android.os.SystemClock.sleep(50);
        }
        throw new AssertionError("initial native surface did not become ready");
    }

    private static SurfaceView findSurface(View view) {
        if (view instanceof SurfaceView) return (SurfaceView) view;
        if (view instanceof ViewGroup) {
            ViewGroup group = (ViewGroup) view;
            for (int index = 0; index < group.getChildCount(); index++) {
                SurfaceView found = findSurface(group.getChildAt(index));
                if (found != null) return found;
            }
        }
        return null;
    }
}
