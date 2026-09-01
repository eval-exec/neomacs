use std::time::Duration;

use crate::{ANDROID_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ANDROID_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// android-mode drives an Android project from Emacs: it finds the project by
/// its builder's root file, binds `C-c a' to build/run/log commands, shells
/// out to the SDK tools it locates under `android-mode-sdk-dir', and renders
/// `adb logcat' into its own buffer.  These workflows run a real project - a
/// real `gradlew' script, a real `AndroidManifest.xml' parsed by
/// `xml-parse-file', real `.java' sources - written into the per-case sandbox,
/// and enter through the real key bindings.
///
/// The package does not look its tools up on `PATH': `android-tool-path'
/// searches `android-mode-sdk-dir' across `android-mode-sdk-tool-subdirs' with
/// `android-mode-sdk-tool-extensions'.  So the stand-ins are installed into a
/// real SDK tree at exactly those paths, through the package's own documented
/// configuration, and its real discovery logic runs against real files - which
/// is why one workflow can assert both a tool that is found and a tool that is
/// not.
///
/// The build half is not stood in for at all.  `./gradlew' is a shell script
/// checked into an Android project, so the fixture writes a real one and
/// `compile' runs it, giving a real compilation buffer with real output.
///
/// `adb' and the SDK `android' tool are recording stand-ins, and what they
/// stand in for is a *device that is not attached* - the case the standards
/// name explicitly.  Their formats are the real ones: `adb' 1.0.41 was run
/// here to confirm that `adb devices' with nothing attached prints "List of
/// devices attached" and an empty list, that `adb logcat' prints
/// "- waiting for device -", and that `adb shell am start' prints
/// "adb: no devices/emulators found" - which is what the no-device workflow
/// replays, and which is why android-mode reports success from it.
const ANDROID_MODE_TEST_PRELUDE: &str = r##";;; prelude -*- lexical-binding: t; -*-
(require 'cl-lib)
(require 'compile)

(defun amd-test-path (name)
  (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun amd-test-copy (value)
  (if (stringp value) (copy-sequence value) value))

(defun amd-test-write-file (path text)
  (make-directory (file-name-directory path) t)
  (with-temp-buffer
    (insert text)
    (write-region (point-min) (point-max) path nil 'silent))
  path)

(defun amd-test-write-executable (path text)
  (amd-test-write-file path text)
  (set-file-modes path #o755)
  path)

;;; A real Android project.

(defconst amd-test-manifest "\
<?xml version=\"1.0\" encoding=\"utf-8\"?>
<manifest xmlns:android=\"http://schemas.android.com/apk/res/android\"
          package=\"com.warehouse.inventory\">
  <application android:label=\"Inventory\">
    <activity android:name=\".MainActivity\">
      <intent-filter>
        <action android:name=\"android.intent.action.MAIN\" />
        <category android:name=\"android.intent.category.LAUNCHER\" />
      </intent-filter>
    </activity>
    <activity android:name=\"ReportActivity\">
      <intent-filter>
        <action android:name=\"android.intent.action.MAIN\" />
        <category android:name=\"android.intent.category.DEFAULT\" />
      </intent-filter>
    </activity>
    <activity android:name=\"com.warehouse.tools.ScannerActivity\">
      <intent-filter>
        <action android:name=\"android.intent.action.VIEW\" />
      </intent-filter>
    </activity>
  </application>
</manifest>
")

(defconst amd-test-main-activity "\
package com.warehouse.inventory;

import android.app.Activity;
import android.os.Bundle;

public class MainActivity extends Activity {
    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        setContentView(R.layout.main);
    }
}
")

(defconst amd-test-report-activity "\
package com.warehouse.inventory;

import android.app.Activity;

public class ReportActivity extends Activity {
    public void render() {
        throw new IllegalStateException(\"no report yet\");
    }
}
")

;; A real Gradle wrapper: an ordinary shell script checked into the project,
;; which is exactly what `./gradlew' is.  It records its arguments and prints
;; the shape of output `compile' has to render.
(defconst amd-test-gradlew "\
#!/bin/sh
printf 'gradlew %s\\n' \"$*\" >> \"$AMD_TEST_LOG\"
echo \"> Task :app:$1\"
echo 'BUILD SUCCESSFUL in 1s'
echo '1 actionable task: 1 executed'
")

(defun amd-test-write-project (&optional builder)
  "Write a real Android project and return its root."
  (let ((root (amd-test-path "workspace/inventory/")))
    (amd-test-write-executable (expand-file-name "gradlew" root) amd-test-gradlew)
    (amd-test-write-file (expand-file-name "AndroidManifest.xml" root) amd-test-manifest)
    (amd-test-write-file
     (expand-file-name "src/com/warehouse/inventory/MainActivity.java" root)
     amd-test-main-activity)
    (amd-test-write-file
     (expand-file-name "src/com/warehouse/inventory/ReportActivity.java" root)
     amd-test-report-activity)
    (ignore builder)
    root))

;;; The SDK, laid out where the package looks for it.

(defconst amd-test-adb-script "\
#!/bin/sh
# Stands in for adb, whose counterparty is a device that is not attached.
printf 'adb %s\\n' \"$*\" >> \"$AMD_TEST_LOG\"
case \"$1\" in
  logcat)
    cat \"$AMD_TEST_DIR/logcat.txt\"
    while [ -f \"$AMD_TEST_DIR/logcat-hold\" ]; do sleep 0.05; done
    ;;
  shell)
    if [ -f \"$AMD_TEST_DIR/no-device\" ]; then
      echo 'adb: no devices/emulators found'
    else
      echo 'Starting: Intent { cmp='\"$4\"' }'
    fi
    ;;
  devices) printf 'List of devices attached\\n\\n' ;;
esac
")

(defconst amd-test-android-script "\
#!/bin/sh
# Stands in for the SDK `android' tool, which reports the emulator images and
# platforms installed on the machine.
printf 'android %s\\n' \"$*\" >> \"$AMD_TEST_LOG\"
if [ \"$1 $2\" = 'list avd' ]; then
  cat \"$AMD_TEST_DIR/avds.txt\"
elif [ \"$1 $2\" = 'list target' ]; then
  cat \"$AMD_TEST_DIR/targets.txt\"
fi
")

(defconst amd-test-emulator-script "\
#!/bin/sh
printf 'emulator %s\\n' \"$*\" >> \"$AMD_TEST_LOG\"
exec cat > /dev/null
")

(defconst amd-test-logcat-lines "\
I/ActivityManager(  742): Displayed com.warehouse.inventory/.MainActivity: +312ms
D/InventorySync(  742): syncing 3 widgets
W/InventorySync(  742): bucket cache is stale
E/AndroidRuntime(  742): FATAL EXCEPTION: main
E/AndroidRuntime(  742): java.lang.IllegalStateException: no report yet
E/AndroidRuntime(  742): \tat com.warehouse.inventory.ReportActivity.render(ReportActivity.java:7)
E/AndroidRuntime(  742): \tat com.warehouse.missing.Absent.gone(Absent.java:42)
V/InventorySync(  742): done
this line has no level prefix
")

(defconst amd-test-avds "\
Available Android Virtual Devices:
    Name: Pixel_6_API_34
  Device: pixel_6 (Google)
    Path: /home/melpa-test/.android/avd/Pixel_6_API_34.avd
  Target: Google APIs (Google Inc.)
---------
    Name: Nexus_5X_API_29
  Device: Nexus 5X (Google)
    Path: /home/melpa-test/.android/avd/Nexus_5X_API_29.avd
  Target: Default Android System Image
")

(defconst amd-test-targets "\
Available Android targets:
----------
id: 1 or \"android-34\"
     Name: Android API 34
     Type: Platform
     API level: 34
----------
id: 2 or \"Google Inc.:Google APIs:34\"
     Name: Google APIs
     Type: Add-On
")

(defun amd-test-write-sdk (&optional tools)
  "Lay out an SDK where `android-mode' looks for its tools.
TOOLS defaults to the full set; pass a subset to leave one missing."
  (let ((sdk (amd-test-path "android-sdk/"))
        (tools (or tools '("platform-tools/adb" "tools/android" "emulator/emulator"))))
    (dolist (tool tools)
      (amd-test-write-executable
       (expand-file-name tool sdk)
       (cond ((string-suffix-p "adb" tool) amd-test-adb-script)
             ((string-suffix-p "android" tool) amd-test-android-script)
             (t amd-test-emulator-script))))
    (amd-test-write-file (amd-test-path "recordings/logcat.txt") amd-test-logcat-lines)
    (amd-test-write-file (amd-test-path "recordings/avds.txt") amd-test-avds)
    (amd-test-write-file (amd-test-path "recordings/targets.txt") amd-test-targets)
    (directory-file-name sdk)))

(defun amd-test-setup (&optional tools)
  ;; `compile' asks whether to kill a running compilation, and unattended
  ;; execution cannot answer.  A workflow that builds twice is otherwise
  ;; blocked on a prompt no user would see for long.
  (setq compilation-always-kill t
        compilation-ask-about-save nil)
  (let ((root (amd-test-write-project))
        (sdk (amd-test-write-sdk tools)))
    (setenv "AMD_TEST_LOG" (amd-test-path "commands.log"))
    (setenv "AMD_TEST_DIR" (amd-test-path "recordings"))
    (setenv "ANDROID_HOME" sdk)
    (setq android-mode-sdk-dir sdk)
    (list :root root :sdk sdk)))

(defun amd-test-teardown ()
  (dolist (process (process-list))
    (when (string-match-p "android\\|emulator" (process-name process))
      (set-process-query-on-exit-flag process nil)
      (delete-process process)))
  (setq android-exclusive-processes nil)
  (dolist (name (list android-logcat-buffer "*compilation*"))
    (when (get-buffer name) (kill-buffer name))))

(defun amd-test-normalize (value)
  (if (stringp value)
      (replace-regexp-in-string
       (regexp-quote (directory-file-name (or (getenv "NEOMACS_TEST_SANDBOX_ROOT") "")))
       "[SANDBOX]" (copy-sequence value) t t)
    value))

(defun amd-test-commands ()
  "Every external command the package ran, oldest first."
  (let ((path (amd-test-path "commands.log")))
    (when (file-exists-p path)
      (mapcar #'amd-test-normalize
              (split-string (with-temp-buffer (insert-file-contents path) (buffer-string))
                            "\n" t)))))

(defun amd-test-wait (predicate &optional seconds)
  (let ((deadline (+ (float-time) (or seconds 20))))
    (while (and (not (funcall predicate)) (< (float-time) deadline))
      (accept-process-output nil 0.02))
    (and (funcall predicate) t)))

(defun amd-test-visit (&optional file)
  (let ((buffer (find-file-noselect
                 (expand-file-name (or file "src/com/warehouse/inventory/MainActivity.java")
                                   (amd-test-path "workspace/inventory/")))))
    (set-window-buffer (selected-window) buffer)
    (set-buffer buffer)
    buffer))

(defmacro amd-test-with-project (&rest body)
  `(let ((buffer nil))
     (unwind-protect
         (progn
           (amd-test-setup)
           (setq buffer (amd-test-visit))
           ,@body)
       (when (buffer-live-p buffer)
         (with-current-buffer buffer (set-buffer-modified-p nil))
         (kill-buffer buffer))
       (amd-test-teardown))))

(defun amd-test-faces (start end)
  (let ((position start) runs)
    (while (< position end)
      (let ((next (next-single-property-change position 'font-lock-face nil end)))
        (push (list (get-text-property position 'font-lock-face)
                    (buffer-substring-no-properties position next))
              runs)
        (setq position next)))
    (nreverse runs)))

(defun amd-test-messages (regexp)
  (let (matches)
    (with-current-buffer "*Messages*"
      (save-excursion
        (goto-char (point-min))
        (while (re-search-forward regexp nil t)
          (push (amd-test-normalize (match-string-no-properties 0)) matches))))
    (nreverse matches)))
"##;

fn android_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ANDROID_MODE_MELPA_PIN, "android-mode.el")
        .expect("prepare pinned android-mode source below ./tmp")
        .with_prelude(ANDROID_MODE_TEST_PRELUDE)
        .with_timeout(ANDROID_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed android-mode parity test")
        .into()
}

/// Multi-probe batch for `assert_android_mode_parity` cases (2a).
pub(crate) fn assert_android_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(android_mode_oracle(), &name, "android_mode_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn android_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_android_mode_batch(&cases);
}

// END generated package batch tests
