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
    @Override public void onCreate(Bundle arguments) {
        super.onCreate(arguments);
        start();
    }

    @Override public void onStart() {
        Bundle result = new Bundle();
        try {
            Activity activity = startActivitySync(getTargetContext().getPackageManager()
                    .getLaunchIntentForPackage(getTargetContext().getPackageName()));
            waitForIdleSync();
            final String[] failure = {"surface did not become ready"};
            for (int attempt = 0; attempt < 100; attempt++) {
                runOnMainSync(() -> {
                    View root = activity.getWindow().getDecorView();
                    SurfaceView surface = findSurface(root);
                    WindowInsetsCompat insets = ViewCompat.getRootWindowInsets(root);
                    if (surface == null || insets == null || surface.getWidth() == 0) return;
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
                if (failure[0] == null) break;
                android.os.SystemClock.sleep(50);
            }
            if (failure[0] != null) throw new AssertionError(failure[0]);
            result.putString("stream", "PASS: drawable surface respects system and IME insets\n");
            finish(Activity.RESULT_OK, result);
        } catch (Throwable error) {
            result.putString("stream", "FAIL: " + error + "\n");
            finish(Activity.RESULT_CANCELED, result);
        }
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
