use std::time::Duration;

use crate::{ANAKONDO_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ANAKONDO_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// anakondo gives Clojure buffers completion by shelling out: `clojure -Spath'
/// for the project classpath, `clj-kondo --lint ... --config
/// '{:output {:analysis true :format :json}}'' for the static analysis, and
/// `jar tf' plus `javap -cp ... -public' for the Java classes on that
/// classpath.  These workflows run a real project through the real public
/// route - a `deps.edn' and two namespaces written into the per-case sandbox,
/// `anakondo-minor-mode' turned on, and candidates taken from
/// `completion-at-point-functions'.
///
/// Two of those four tools are real here and two are not, so the boundary is
/// drawn tool by tool rather than all at once.
///
/// `jar' and `javap' are a real JDK, and nothing about the Java half is faked:
/// the prelude compiles a real `com.warehouse.Barcode' with `javac', packages
/// it with `jar cf', and the package's own scan and parse run against that
/// jar.  `javac' and `jar' are checked for rather than assumed, and their
/// absence is reported in the workflow's own output instead of being worked
/// around.  The expectations were taken on OpenJDK 21.0.10; the version
/// matters for reading them, because JDK 9 removed the `sun.boot.class.path'
/// property anakondo relies on, which is why the default `java.lang' imports
/// resolve to nothing.  Nothing derived from `jar' or `javap' is asserted in
/// the order the tool emitted it - member and class lists are sorted first, so
/// a JDK that orders them differently does not move a snapshot.
///
/// clj-kondo is not installed here, so it is a recording stand-in that logs
/// the exact argv anakondo built and the text it piped in.  What it replays is
/// not invented: clj-kondo v2025.09.22 was run against this exact project with
/// these exact command lines, and the JSON it printed is what
/// `ak-test-project-analysis' and `ak-test-buffer-analysis' contain, minus the
/// `var-usages' array that anakondo never reads.  `clojure -Spath' is a
/// stand-in too, but its entire output is a colon-separated classpath of paths
/// the prelude really created - including the trailing newline
/// `shell-command-to-string' keeps, which anakondo passes into `--lint'
/// verbatim and the command log preserves as `\n'.
const ANAKONDO_TEST_PRELUDE: &str = r##";;; prelude -*- lexical-binding: t; -*-
(require 'cl-lib)
(require 'json)

(defun ak-test-path (name)
  (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun ak-test-copy (value)
  (if (stringp value) (copy-sequence value) value))

;;; The Clojure project the user is editing.

(defconst ak-test-core-clj "(ns inventory.core
  \"Warehouse inventory for the anakondo parity fixture.\"
  (:require [inventory.util :as util]
            [clojure.string :as str]))

(defrecord Widget [name price])

(defn make-widget
  \"Build a widget from NAME, priced at util/default-price.\"
  [name]
  (->Widget (util/normalize-name name) util/default-price))

(defn build-catalogue
  \"Make one widget per entry of NAMES.\"
  [names]
  (mapv make-widget names))

(defn total-price
  \"Sum the price of every widget in WIDGETS.\"
  [widgets]
  (reduce + (map :price widgets)))

(defn describe
  \"Render WIDGET as a human readable line.\"
  [widget]
  (str/join \" \" [(:name widget) (:price widget)]))

(def catalogue (build-catalogue [\"bolt\" \"nut\" \"washer\"]))
")
(defconst ak-test-util-clj "(ns inventory.util
  \"Helpers shared by the inventory namespaces.\")

(def default-price 12)

(defn normalize-name
  \"Trim and upper-case NAME.\"
  [name]
  (clojure.string/upper-case (clojure.string/trim name)))

(defn apply-discount
  \"Return PRICE with PERCENT taken off.\"
  [price percent]
  (/ (* price (- 100 percent)) 100))
")
(defconst ak-test-barcode-java "package com.warehouse;

public class Barcode {
    public static final String PREFIX = \"WH-\";
    public static final int LENGTH = 12;

    public static String format(String sku) {
        return PREFIX + sku;
    }

    public static boolean valid(String code) {
        return code != null && code.length() == LENGTH;
    }

    public String instanceOnly() {
        return \"not static\";
    }
}
")

;;; Analyses recorded from the real clj-kondo.
;;
;; clj-kondo v2025.09.22 was run against this exact project with the exact
;; command lines anakondo builds - `--lint <classpath>' for the project and
;; `--lint -' with the buffer on stdin for completion, both with
;; `{:output {:analysis true :format :json}}' and `--lang clj' - and the JSON it
;; printed is reproduced here.  The one thing removed is the `var-usages'
;; array: anakondo reads only `var-definitions', `namespace-definitions' and
;; `namespace-usages', and `var-usages' is two thirds of the bytes.  Everything
;; anakondo looks at is exactly what the tool said.

(defconst ak-test-project-analysis "{\"analysis\":{\"namespace-definitions\":[{\"col\":1,\"doc\":\"Helpers shared by the inventory namespaces.\",\"end-col\":49,\"end-row\":2,\"filename\":\"src/inventory/util.clj\",\"name\":\"inventory.util\",\"name-col\":5,\"name-end-col\":19,\"name-end-row\":1,\"name-row\":1,\"row\":1},{\"col\":1,\"doc\":\"Warehouse inventory for the anakondo parity fixture.\",\"end-col\":39,\"end-row\":4,\"filename\":\"src/inventory/core.clj\",\"name\":\"inventory.core\",\"name-col\":5,\"name-end-col\":19,\"name-end-row\":1,\"name-row\":1,\"row\":1}],\"namespace-usages\":[{\"alias\":\"util\",\"alias-col\":33,\"alias-end-col\":37,\"alias-end-row\":3,\"alias-row\":3,\"col\":14,\"filename\":\"src/inventory/core.clj\",\"from\":\"inventory.core\",\"name-col\":14,\"name-end-col\":28,\"name-end-row\":3,\"name-row\":3,\"row\":3,\"to\":\"inventory.util\"},{\"alias\":\"str\",\"alias-col\":33,\"alias-end-col\":36,\"alias-end-row\":4,\"alias-row\":4,\"col\":14,\"filename\":\"src/inventory/core.clj\",\"from\":\"inventory.core\",\"name-col\":14,\"name-end-col\":28,\"name-end-row\":4,\"name-row\":4,\"row\":4,\"to\":\"clojure.string\"}],\"var-definitions\":[{\"col\":1,\"defined-by\":\"clojure.core/def\",\"defined-by->lint-as\":\"clojure.core/def\",\"end-col\":23,\"end-row\":4,\"filename\":\"src/inventory/util.clj\",\"name\":\"default-price\",\"name-col\":6,\"name-end-col\":19,\"name-end-row\":4,\"name-row\":4,\"ns\":\"inventory.util\",\"row\":4},{\"col\":1,\"defined-by\":\"clojure.core/defn\",\"defined-by->lint-as\":\"clojure.core/defn\",\"doc\":\"Trim and upper-case NAME.\",\"end-col\":58,\"end-row\":9,\"filename\":\"src/inventory/util.clj\",\"fixed-arities\":[1],\"name\":\"normalize-name\",\"name-col\":7,\"name-end-col\":21,\"name-end-row\":6,\"name-row\":6,\"ns\":\"inventory.util\",\"row\":6},{\"col\":1,\"defined-by\":\"clojure.core/defn\",\"defined-by->lint-as\":\"clojure.core/defn\",\"doc\":\"Return PRICE with PERCENT taken off.\",\"end-col\":37,\"end-row\":14,\"filename\":\"src/inventory/util.clj\",\"fixed-arities\":[2],\"name\":\"apply-discount\",\"name-col\":7,\"name-end-col\":21,\"name-end-row\":11,\"name-row\":11,\"ns\":\"inventory.util\",\"row\":11},{\"col\":1,\"defined-by\":\"clojure.core/defrecord\",\"defined-by->lint-as\":\"clojure.core/defrecord\",\"end-col\":32,\"end-row\":6,\"filename\":\"src/inventory/core.clj\",\"name\":\"Widget\",\"name-col\":12,\"name-end-col\":18,\"name-end-row\":6,\"name-row\":6,\"ns\":\"inventory.core\",\"row\":6},{\"col\":1,\"defined-by\":\"clojure.core/defrecord\",\"defined-by->lint-as\":\"clojure.core/defrecord\",\"end-col\":32,\"end-row\":6,\"filename\":\"src/inventory/core.clj\",\"fixed-arities\":[2],\"name\":\"->Widget\",\"name-col\":12,\"name-end-col\":18,\"name-end-row\":6,\"name-row\":6,\"ns\":\"inventory.core\",\"row\":6},{\"col\":1,\"defined-by\":\"clojure.core/defrecord\",\"defined-by->lint-as\":\"clojure.core/defrecord\",\"end-col\":32,\"end-row\":6,\"filename\":\"src/inventory/core.clj\",\"fixed-arities\":[1],\"name\":\"map->Widget\",\"name-col\":12,\"name-end-col\":18,\"name-end-row\":6,\"name-row\":6,\"ns\":\"inventory.core\",\"row\":6},{\"col\":1,\"defined-by\":\"clojure.core/defn\",\"defined-by->lint-as\":\"clojure.core/defn\",\"doc\":\"Build a widget from NAME, priced at util/default-price.\",\"end-col\":60,\"end-row\":11,\"filename\":\"src/inventory/core.clj\",\"fixed-arities\":[1],\"name\":\"make-widget\",\"name-col\":7,\"name-end-col\":18,\"name-end-row\":8,\"name-row\":8,\"ns\":\"inventory.core\",\"row\":8},{\"col\":1,\"defined-by\":\"clojure.core/defn\",\"defined-by->lint-as\":\"clojure.core/defn\",\"doc\":\"Make one widget per entry of NAMES.\",\"end-col\":28,\"end-row\":16,\"filename\":\"src/inventory/core.clj\",\"fixed-arities\":[1],\"name\":\"build-catalogue\",\"name-col\":7,\"name-end-col\":22,\"name-end-row\":13,\"name-row\":13,\"ns\":\"inventory.core\",\"row\":13},{\"col\":1,\"defined-by\":\"clojure.core/defn\",\"defined-by->lint-as\":\"clojure.core/defn\",\"doc\":\"Sum the price of every widget in WIDGETS.\",\"end-col\":35,\"end-row\":21,\"filename\":\"src/inventory/core.clj\",\"fixed-arities\":[1],\"name\":\"total-price\",\"name-col\":7,\"name-end-col\":18,\"name-end-row\":18,\"name-row\":18,\"ns\":\"inventory.core\",\"row\":18},{\"col\":1,\"defined-by\":\"clojure.core/defn\",\"defined-by->lint-as\":\"clojure.core/defn\",\"doc\":\"Render WIDGET as a human readable line.\",\"end-col\":51,\"end-row\":26,\"filename\":\"src/inventory/core.clj\",\"fixed-arities\":[1],\"name\":\"describe\",\"name-col\":7,\"name-end-col\":15,\"name-end-row\":23,\"name-row\":23,\"ns\":\"inventory.core\",\"row\":23},{\"col\":1,\"defined-by\":\"clojure.core/def\",\"defined-by->lint-as\":\"clojure.core/def\",\"end-col\":58,\"end-row\":28,\"filename\":\"src/inventory/core.clj\",\"name\":\"catalogue\",\"name-col\":6,\"name-end-col\":15,\"name-end-row\":28,\"name-row\":28,\"ns\":\"inventory.core\",\"row\":28}]}}")
(defconst ak-test-buffer-analysis "{\"analysis\":{\"namespace-definitions\":[{\"col\":1,\"doc\":\"Warehouse inventory for the anakondo parity fixture.\",\"end-col\":39,\"end-row\":4,\"filename\":\"<stdin>\",\"name\":\"inventory.core\",\"name-col\":5,\"name-end-col\":19,\"name-end-row\":1,\"name-row\":1,\"row\":1}],\"namespace-usages\":[{\"alias\":\"util\",\"alias-col\":33,\"alias-end-col\":37,\"alias-end-row\":3,\"alias-row\":3,\"col\":14,\"filename\":\"<stdin>\",\"from\":\"inventory.core\",\"name-col\":14,\"name-end-col\":28,\"name-end-row\":3,\"name-row\":3,\"row\":3,\"to\":\"inventory.util\"},{\"alias\":\"str\",\"alias-col\":33,\"alias-end-col\":36,\"alias-end-row\":4,\"alias-row\":4,\"col\":14,\"filename\":\"<stdin>\",\"from\":\"inventory.core\",\"name-col\":14,\"name-end-col\":28,\"name-end-row\":4,\"name-row\":4,\"row\":4,\"to\":\"clojure.string\"}],\"var-definitions\":[{\"col\":1,\"defined-by\":\"clojure.core/defrecord\",\"defined-by->lint-as\":\"clojure.core/defrecord\",\"end-col\":32,\"end-row\":6,\"filename\":\"<stdin>\",\"name\":\"Widget\",\"name-col\":12,\"name-end-col\":18,\"name-end-row\":6,\"name-row\":6,\"ns\":\"inventory.core\",\"row\":6},{\"col\":1,\"defined-by\":\"clojure.core/defrecord\",\"defined-by->lint-as\":\"clojure.core/defrecord\",\"end-col\":32,\"end-row\":6,\"filename\":\"<stdin>\",\"fixed-arities\":[2],\"name\":\"->Widget\",\"name-col\":12,\"name-end-col\":18,\"name-end-row\":6,\"name-row\":6,\"ns\":\"inventory.core\",\"row\":6},{\"col\":1,\"defined-by\":\"clojure.core/defrecord\",\"defined-by->lint-as\":\"clojure.core/defrecord\",\"end-col\":32,\"end-row\":6,\"filename\":\"<stdin>\",\"fixed-arities\":[1],\"name\":\"map->Widget\",\"name-col\":12,\"name-end-col\":18,\"name-end-row\":6,\"name-row\":6,\"ns\":\"inventory.core\",\"row\":6},{\"col\":1,\"defined-by\":\"clojure.core/defn\",\"defined-by->lint-as\":\"clojure.core/defn\",\"doc\":\"Build a widget from NAME, priced at util/default-price.\",\"end-col\":60,\"end-row\":11,\"filename\":\"<stdin>\",\"fixed-arities\":[1],\"name\":\"make-widget\",\"name-col\":7,\"name-end-col\":18,\"name-end-row\":8,\"name-row\":8,\"ns\":\"inventory.core\",\"row\":8},{\"col\":1,\"defined-by\":\"clojure.core/defn\",\"defined-by->lint-as\":\"clojure.core/defn\",\"doc\":\"Make one widget per entry of NAMES.\",\"end-col\":28,\"end-row\":16,\"filename\":\"<stdin>\",\"fixed-arities\":[1],\"name\":\"build-catalogue\",\"name-col\":7,\"name-end-col\":22,\"name-end-row\":13,\"name-row\":13,\"ns\":\"inventory.core\",\"row\":13},{\"col\":1,\"defined-by\":\"clojure.core/defn\",\"defined-by->lint-as\":\"clojure.core/defn\",\"doc\":\"Sum the price of every widget in WIDGETS.\",\"end-col\":35,\"end-row\":21,\"filename\":\"<stdin>\",\"fixed-arities\":[1],\"name\":\"total-price\",\"name-col\":7,\"name-end-col\":18,\"name-end-row\":18,\"name-row\":18,\"ns\":\"inventory.core\",\"row\":18},{\"col\":1,\"defined-by\":\"clojure.core/defn\",\"defined-by->lint-as\":\"clojure.core/defn\",\"doc\":\"Render WIDGET as a human readable line.\",\"end-col\":51,\"end-row\":26,\"filename\":\"<stdin>\",\"fixed-arities\":[1],\"name\":\"describe\",\"name-col\":7,\"name-end-col\":15,\"name-end-row\":23,\"name-row\":23,\"ns\":\"inventory.core\",\"row\":23},{\"col\":1,\"defined-by\":\"clojure.core/def\",\"defined-by->lint-as\":\"clojure.core/def\",\"end-col\":58,\"end-row\":28,\"filename\":\"<stdin>\",\"name\":\"catalogue\",\"name-col\":6,\"name-end-col\":15,\"name-end-row\":28,\"name-row\":28,\"ns\":\"inventory.core\",\"row\":28}]}}")
(defconst ak-test-buffer-util-analysis "{\"analysis\":{\"namespace-definitions\":[{\"col\":1,\"doc\":\"Helpers shared by the inventory namespaces.\",\"end-col\":49,\"end-row\":2,\"filename\":\"<stdin>\",\"name\":\"inventory.util\",\"name-col\":5,\"name-end-col\":19,\"name-end-row\":1,\"name-row\":1,\"row\":1}],\"namespace-usages\":[],\"var-definitions\":[{\"col\":1,\"defined-by\":\"clojure.core/def\",\"defined-by->lint-as\":\"clojure.core/def\",\"end-col\":23,\"end-row\":4,\"filename\":\"<stdin>\",\"name\":\"default-price\",\"name-col\":6,\"name-end-col\":19,\"name-end-row\":4,\"name-row\":4,\"ns\":\"inventory.util\",\"row\":4},{\"col\":1,\"defined-by\":\"clojure.core/defn\",\"defined-by->lint-as\":\"clojure.core/defn\",\"doc\":\"Trim and upper-case NAME.\",\"end-col\":58,\"end-row\":9,\"filename\":\"<stdin>\",\"fixed-arities\":[1],\"name\":\"normalize-name\",\"name-col\":7,\"name-end-col\":21,\"name-end-row\":6,\"name-row\":6,\"ns\":\"inventory.util\",\"row\":6},{\"col\":1,\"defined-by\":\"clojure.core/defn\",\"defined-by->lint-as\":\"clojure.core/defn\",\"doc\":\"Return PRICE with PERCENT taken off.\",\"end-col\":37,\"end-row\":14,\"filename\":\"<stdin>\",\"fixed-arities\":[2],\"name\":\"apply-discount\",\"name-col\":7,\"name-end-col\":21,\"name-end-row\":11,\"name-row\":11,\"ns\":\"inventory.util\",\"row\":11}]}}")

;;; The stand-in tools.

(defconst ak-test-clj-kondo-script "\
#!/bin/sh
# Stands in for clj-kondo, which is not installed here.  Records the exact argv
# anakondo built and the text it piped in, then replays an analysis recorded
# from the real tool.
for arg; do printf 'arg %s\\n' \"$(printf '%s' \"$arg\" | tr '\\n' '\\001')\"; done >> \"$AK_TEST_LOG\"
printf 'cwd %s\\n' \"$PWD\" >> \"$AK_TEST_LOG\"
lint=''
want=0
for arg; do
  if [ \"$want\" = 1 ]; then lint=\"$arg\"; want=0; fi
  if [ \"$arg\" = --lint ]; then want=1; fi
done
if [ \"$lint\" = - ]; then
  cat > \"$AK_TEST_DIR/stdin.clj\"
  printf 'stdin-bytes %s\\n' \"$(wc -c < \"$AK_TEST_DIR/stdin.clj\" | tr -d ' ')\" >> \"$AK_TEST_LOG\"
  printf -- '--\\n' >> \"$AK_TEST_LOG\"
  # Answer for the namespace that was actually piped in, so a second buffer
  # gets its own recorded analysis rather than the first buffer's.
  ns=$(sed -n 's/^(ns \\([^ )]*\\).*/\\1/p' \"$AK_TEST_DIR/stdin.clj\" | head -1)
  if [ -f \"$AK_TEST_DIR/buffer-$ns.json\" ]; then
    cat \"$AK_TEST_DIR/buffer-$ns.json\"
  else
    cat \"$AK_TEST_DIR/buffer.json\"
  fi
else
  printf -- '--\\n' >> \"$AK_TEST_LOG\"
  cat \"$AK_TEST_DIR/project.json\"
fi
exit ${AK_TEST_KONDO_STATUS:-0}
")

(defconst ak-test-broken-clj-kondo-script "\
#!/bin/sh
# Stands in for a clj-kondo that is present but cannot run - the shape of a
# failed install, a bad --config, or an unreadable classpath.
for arg; do printf 'arg %s\\n' \"$(printf '%s' \"$arg\" | tr '\\n' '\\001')\"; done >> \"$AK_TEST_LOG\"
printf -- '--\\n' >> \"$AK_TEST_LOG\"
echo 'clj-kondo: could not parse config: {:output {:analysis true' >&2
exit 1
")

(defconst ak-test-clojure-script "\
#!/bin/sh
# Stands in for the Clojure CLI.  `clojure -Spath' prints the project
# classpath and a newline, which is what anakondo passes to --lint verbatim.
for arg; do printf 'arg %s\\n' \"$(printf '%s' \"$arg\" | tr '\\n' '\\001')\"; done >> \"$AK_TEST_LOG\"
printf -- '--\\n' >> \"$AK_TEST_LOG\"
printf '%s\\n' \"$AK_TEST_CLASSPATH\"
")

(defun ak-test-write-executable (name script)
  (let ((path (ak-test-path (concat "bin/" name))))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer
      (insert script)
      (write-region (point-min) (point-max) path nil 'silent))
    (set-file-modes path #o755)
    path))

(defun ak-test-write-file (path text)
  (make-directory (file-name-directory path) t)
  (with-temp-buffer
    (insert text)
    (write-region (point-min) (point-max) path nil 'silent))
  path)

(defconst ak-test-deps-edn "{:paths [\"src\"]\n :deps {}}\n")

(defun ak-test-write-project ()
  "Write the Clojure sources and return the project root."
  (let ((root (ak-test-path "project/")))
    (ak-test-write-file (expand-file-name "deps.edn" root) ak-test-deps-edn)
    (ak-test-write-file (expand-file-name "src/inventory/core.clj" root) ak-test-core-clj)
    (ak-test-write-file (expand-file-name "src/inventory/util.clj" root) ak-test-util-clj)
    root))

(defun ak-test-build-jar ()
  "Compile a real class and package it with the real JDK tools.
Returns the jar path, or a `:missing' marker naming what is absent."
  (let* ((root (ak-test-path "project/"))
         (source (expand-file-name "javasrc/com/warehouse/Barcode.java" root))
         (classes (expand-file-name "javaclasses" root))
         (jar (expand-file-name "lib/warehouse.jar" root)))
    (if (not (and (executable-find "javac") (executable-find "jar")))
        (list :missing (list :javac (and (executable-find "javac") t)
                             :jar (and (executable-find "jar") t)))
      (ak-test-write-file source ak-test-barcode-java)
      (make-directory classes t)
      (make-directory (file-name-directory jar) t)
      (call-process "javac" nil nil nil "-d" classes source)
      (let ((default-directory classes))
        (call-process "jar" nil nil nil "cf" jar "com"))
      jar)))

(defun ak-test-setup (&optional kondo-script)
  "Write the project, build the jar, install the stand-in tools on PATH."
  (let* ((root (ak-test-write-project))
         (jar (ak-test-build-jar))
         (bin (file-name-directory (ak-test-write-executable
                                    "clj-kondo" (or kondo-script ak-test-clj-kondo-script)))))
    (ak-test-write-executable "clojure" ak-test-clojure-script)
    (ak-test-write-file (ak-test-path "recordings/project.json") ak-test-project-analysis)
    (ak-test-write-file (ak-test-path "recordings/buffer.json") ak-test-buffer-analysis)
    (ak-test-write-file (ak-test-path "recordings/buffer-inventory.util.json")
                        ak-test-buffer-util-analysis)
    (setenv "AK_TEST_LOG" (ak-test-path "commands.log"))
    (setenv "AK_TEST_DIR" (ak-test-path "recordings"))
    (setenv "AK_TEST_CLASSPATH"
            (concat (expand-file-name "src" root) ":" (if (stringp jar) jar "")))
    (setenv "PATH" (concat (directory-file-name bin) path-separator (getenv "PATH")))
    (setq exec-path (cons (directory-file-name bin) exec-path))
    (list :root root :jar jar)))

(defun ak-test-teardown ()
  (setq anakondo--cache nil)
  (dolist (name '("*anakondo*" "*Completions*"))
    (when (get-buffer name) (kill-buffer name))))

(defun ak-test-normalize (value)
  "Replace the JDK's store path so a different JDK does not change a snapshot."
  (if (stringp value)
      (replace-regexp-in-string "/nix/store/[^/\n:]+/" "[JDK]/" (copy-sequence value) t t)
    value))

(defun ak-test-commands ()
  "Every external command run, oldest first, as (ARGUMENT... EXTRA)."
  (let ((path (ak-test-path "commands.log")))
    (when (file-exists-p path)
      (mapcar
       (lambda (record)
         (mapcar (lambda (line)
                   (ak-test-normalize
                    (replace-regexp-in-string
                     "\001" "\\n"
                     (if (string-prefix-p "arg " line) (substring line 4) line)
                     t t)))
                 (split-string record "\n" t)))
       (split-string
        (with-temp-buffer (insert-file-contents path) (buffer-string))
        "^--\n" t)))))

(defun ak-test-kondo-stdin ()
  "The text anakondo piped into clj-kondo for the last buffer analysis."
  (let ((path (ak-test-path "recordings/stdin.clj")))
    (when (file-exists-p path)
      (with-temp-buffer (insert-file-contents path) (buffer-string)))))

(defun ak-test-visit (&optional file)
  "Visit a project source file in the selected window."
  (let ((buffer (find-file-noselect
                 (expand-file-name (or file "src/inventory/core.clj")
                                   (ak-test-path "project/")))))
    (set-window-buffer (selected-window) buffer)
    (set-buffer buffer)
    buffer))

(defmacro ak-test-with-project (&rest body)
  "Edit the fixture project with the stand-in tools installed."
  `(let ((buffer nil))
     (unwind-protect
         (progn
           (ak-test-setup)
           (setq buffer (ak-test-visit))
           ,@body)
       (when (buffer-live-p buffer)
         (with-current-buffer buffer (set-buffer-modified-p nil))
         (kill-buffer buffer))
       (ak-test-teardown))))

(defun ak-test-goto (line column)
  (goto-char (point-min))
  (forward-line (1- line))
  (forward-char column)
  (list (line-number-at-pos) (- (point) (line-beginning-position))))

(defun ak-test-here ()
  (list :line (line-number-at-pos)
        :column (- (point) (line-beginning-position))
        :text (buffer-substring-no-properties (line-beginning-position) (line-end-position))))

(defun ak-test-candidates ()
  "The completion candidates anakondo offers at point, sorted."
  (let* ((capf (run-hook-with-args-until-success 'completion-at-point-functions)))
    (when capf
      (cl-destructuring-bind (start end table &rest _) capf
        (let ((prefix (buffer-substring-no-properties start end)))
          (list :start-column (save-excursion (goto-char start)
                                              (- (point) (line-beginning-position)))
                :prefix (copy-sequence prefix)
                :candidates (sort (mapcar #'copy-sequence
                                          (all-completions prefix table))
                                  #'string<)))))))

(defun ak-test-messages (regexp)
  (let (matches)
    (with-current-buffer "*Messages*"
      (save-excursion
        (goto-char (point-min))
        (while (re-search-forward regexp nil t)
          (push (match-string-no-properties 0) matches))))
    (nreverse matches)))
"##;

/// anakondo declares no package dependencies, but it is a Clojure completion
/// package and its own project-root search asks clojure-mode first, so a
/// session without a Clojure major mode is not a session anybody has.  The pin
/// is written out here rather than taken from a shared constant because
/// clojure-mode is a realistic companion for these workflows, not part of
/// anakondo's declared closure.
const CLOJURE_MODE_PIN: (&str, &str) = ("clojure-mode", "20260709.952");

fn anakondo_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ANAKONDO_MELPA_PIN, "anakondo.el")
        .expect("prepare pinned anakondo source below ./tmp")
        .with_melpa_dependency(CLOJURE_MODE_PIN)
        .expect("prepare pinned clojure-mode source below ./tmp")
        .with_prelude(ANAKONDO_TEST_PRELUDE)
        .with_timeout(ANAKONDO_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed anakondo parity test")
        .into()
}

/// Multi-probe batch for `assert_anakondo_parity` cases (2a).
pub(crate) fn assert_anakondo_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(anakondo_oracle(), &name, "anakondo_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn anakondo_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_anakondo_batch(&cases);
}

// END generated package batch tests
