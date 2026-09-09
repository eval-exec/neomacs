package org.neomacs;

import android.view.View;
import android.widget.FrameLayout;
import androidx.core.graphics.Insets;
import androidx.core.view.WindowCompat;
import androidx.core.view.WindowInsetsCompat;
import com.google.androidgamesdk.GameActivity;

/** Android owns drawable placement; Rust renders and receives input in surface-local coordinates. */
public final class NeomacsActivity extends GameActivity {
    @Override protected void onSetUpWindow() {
        super.onSetUpWindow();
        WindowCompat.setDecorFitsSystemWindows(getWindow(), false);
    }

    @Override public WindowInsetsCompat onApplyWindowInsets(View view, WindowInsetsCompat insets) {
        // Preserve GameActivity's IME and native inset notifications.
        WindowInsetsCompat result = super.onApplyWindowInsets(view, insets);
        Insets safe = insets.getInsets(WindowInsetsCompat.Type.systemBars()
                | WindowInsetsCompat.Type.displayCutout() | WindowInsetsCompat.Type.ime());
        FrameLayout.LayoutParams layout = (FrameLayout.LayoutParams) view.getLayoutParams();
        if (layout.leftMargin != safe.left || layout.topMargin != safe.top
                || layout.rightMargin != safe.right || layout.bottomMargin != safe.bottom) {
            layout.setMargins(safe.left, safe.top, safe.right, safe.bottom);
            view.setLayoutParams(layout);
        }
        return result;
    }
}
