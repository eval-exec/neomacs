use std::time::Duration;

use crate::{AC_PHP_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AC_PHP_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// ac-php is the auto-complete front end for PHP.  Its value is entirely
/// downstream of an index: ac-php-core runs the bundled `phpctags' PHAR under
/// a PHP interpreter, writes an index of the project's classes, members,
/// functions and inheritance, and ac-php then completes, documents and expands
/// arguments from it.
///
/// There is no PHP interpreter on the test host, so `php' is the one stand-in.
/// It is a recording, not an invention: the index it replays is the exact
/// output of the package's own indexer run once under PHP 8.4.20 against the
/// three fixture files below, copied verbatim with the absolute project root
/// replaced by a token so it can be re-rooted into whichever sandbox the test
/// gets.  Authoring that index by hand was rejected -- it would pin a
/// reconstruction of phpctags' format rather than the package's behaviour, and
/// the real output turned out to differ from a careful reading of the format
/// in four ways (methods are kind "m" not "f", names carry a trailing "(",
/// positions are FILE-INDEX:LINE rather than path:line:column, and the
/// parameter list lives in the doc slot).
///
/// Everything downstream of the stand-in is the package running for real:
/// loading and merging the index, resolving `extends', inferring the type of a
/// local from `new', applying `use' aliases and namespaces, generating and
/// ranking candidates, propertizing them, rendering documentation, and
/// expanding argument templates through yasnippet.
const AC_PHP_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'auto-complete)

(setq make-backup-files nil create-lockfiles nil)

(defvar ac-php-test-root
  (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defvar ac-php-test-project
  (file-name-as-directory (expand-file-name "shop" ac-php-test-root)))

(defun ac-php-test-write (path text)
  (make-directory (file-name-directory path) t)
  (let ((coding-system-for-write 'utf-8-unix))
    (with-temp-buffer
      (insert text)
      (write-region (point-min) (point-max) path nil 'silent)))
  path)

(defun ac-php-test-read (path)
  (with-temp-buffer
    (let ((coding-system-for-read 'utf-8-unix))
      (insert-file-contents path))
    (buffer-string)))

;; --- the fixture PHP project -------------------------------------
;; These three files are the input the recorded index below was
;; produced from, by the package's own bundled `phpctags' running under
;; PHP 8.4.20.

(defconst ac-php-test-product-php
  "<?php
namespace Shop\\Model;

/**
 * A product on sale.
 */
class Product
{
    const CURRENCY = 'EUR';

    /** @var string */
    private $name;

    /** @var int */
    public $priceCents;

    public function __construct($name, $priceCents)
    {
$this->name = $name;
$this->priceCents = $priceCents;
    }

    public function getName()
    {
return $this->name;
    }

    public function setPrice($cents, $vat = 19)
    {
$this->priceCents = $cents;
    }

    protected function auditLog($message)
    {
    }
}

function formatMoney($cents, $currency = 'EUR')
{
    return $cents . ' ' . $currency;
}
")

(defconst ac-php-test-basecart-php
  "<?php
namespace Shop\\Service;

abstract class BaseCart
{
    public function reset()
    {
    }

    public function itemCount()
    {
return 0;
    }
}
")

(defconst ac-php-test-cart-php
  "<?php
namespace Shop\\Service;

use Shop\\Model\\Product;

class Cart extends BaseCart
{
    public function total()
    {
$product = new Product('Grüße', 500);
return $product;
    }
}
")

(defun ac-php-test-make-project ()
  "Write the fixture PHP project and return its root.
`.ac-php-conf.json' is deliberately not written here: ac-php-core creates
it itself on first use, and letting it do so keeps the configuration the
package's own rather than this file's idea of it."
  ;; ac-php resolves the project root by walking up for one of
  ;; `.ac-php-conf.json', `.projectile' or `vendor/autoload.php'.  The
  ;; first does not exist yet -- ac-php-core writes it -- so the
  ;; fixture plants the neutral one.  phpctags only reads *.php, and a
  ;; re-recording with this file present produced a byte-identical
  ;; index.
  (ac-php-test-write (expand-file-name ".projectile" ac-php-test-project) "")
  (ac-php-test-write (expand-file-name "src/Model/Product.php" ac-php-test-project)
                     ac-php-test-product-php)
  (ac-php-test-write (expand-file-name "src/Service/BaseCart.php" ac-php-test-project)
                     ac-php-test-basecart-php)
  (ac-php-test-write (expand-file-name "src/Service/Cart.php" ac-php-test-project)
                     ac-php-test-cart-php)
  ac-php-test-project)

;; --- the recorded index ------------------------------------------
;; ac-php completes from an index that ac-php-core builds by running
;; the bundled `phpctags' PHAR under a PHP interpreter.  There is no
;; PHP on the test host, so `php' is the one stand-in: it records the
;; argument vector it was given, replays the progress lines the real
;; indexer printed, and writes out the index below.
;;
;; That index is not written by hand.  It is the exact output of
;;
;;   php <elpa>/ac-php-core-20260210.846/phpctags \
;;       --config-file=<project>/./.ac-php-conf.json \
;;       --tags_dir=<cache> --rebuild=yes --realpath_flag=no
;;
;; under PHP 8.4.20 against the three fixture files above, copied
;; verbatim with only the absolute project root replaced by a token so
;; that it can be re-rooted into whichever sandbox the test gets.
;; Everything downstream is the package running for real: loading and
;; merging the index, resolving `extends', inferring the type of a
;; local from `new', generating and ranking candidates, propertizing
;; them, rendering documentation and expanding argument templates.

(defconst ac-php-test-recorded-tags
  "(setq  g-ac-php-tmp-tags  [
(
  (\"\\\\Shop\\\\Model\\\\Product\".[
    [\"d\" \"CURRENCY\" \"\"  \"0:9\"  \"void\" \"\\\\Shop\\\\Model\\\\Product\" \"public\" \"\" ]
    [\"p\" \"name\" \"\"  \"0:12\"  \"string\" \"\\\\Shop\\\\Model\\\\Product\" \"private\" \"\" ]
    [\"p\" \"priceCents\" \"\"  \"0:15\"  \"int\" \"\\\\Shop\\\\Model\\\\Product\" \"public\" \"\" ]
    [\"m\" \"__construct(\" \"$name, $priceCents\"  \"0:17\"  \"\" \"\\\\Shop\\\\Model\\\\Product\" \"public\" \"\" ]
    [\"m\" \"getName(\" \"\"  \"0:23\"  \"\" \"\\\\Shop\\\\Model\\\\Product\" \"public\" \"\" ]
    [\"m\" \"setPrice(\" \"$cents, $vat=19\"  \"0:28\"  \"\" \"\\\\Shop\\\\Model\\\\Product\" \"public\" \"\" ]
    [\"m\" \"auditLog(\" \"$message\"  \"0:33\"  \"\" \"\\\\Shop\\\\Model\\\\Product\" \"protected\" \"\" ]
  ])
  (\"\\\\Shop\\\\Service\\\\BaseCart\".[
    [\"m\" \"reset(\" \"\"  \"1:6\"  \"\" \"\\\\Shop\\\\Service\\\\BaseCart\" \"public\" \"\" ]
    [\"m\" \"itemCount(\" \"\"  \"1:10\"  \"\" \"\\\\Shop\\\\Service\\\\BaseCart\" \"public\" \"\" ]
  ])
  (\"\\\\Shop\\\\Service\\\\Cart\".[
    [\"m\" \"total(\" \"\"  \"2:8\"  \"\" \"\\\\Shop\\\\Service\\\\Cart\" \"public\" \"\" ]
  ])
)
[
  [\"c\" \"\\\\Shop\\\\Model\\\\Product\" \"\"  \"0:7\"  \"\\\\Shop\\\\Model\\\\Product\"  ]
  [\"f\" \"\\\\Shop\\\\Model\\\\formatMoney(\" \"$cents, $currency=\\'EUR\\'\"  \"0:38\"  \"\"  ]
  [\"c\" \"\\\\Shop\\\\Service\\\\BaseCart\" \"\"  \"1:4\"  \"\\\\Shop\\\\Service\\\\BaseCart\"  ]
  [\"c\" \"\\\\Shop\\\\Service\\\\Cart\" \"\"  \"2:6\"  \"\\\\Shop\\\\Service\\\\Cart\"  ]
  [\"f\" \"\\\\Shop\\\\Model\\\\Product(\" \"$name, $priceCents\"  \"0:17\"  \"\\\\Shop\\\\Model\\\\Product\"  ]
  [\"f\" \"\\\\Shop\\\\Service\\\\BaseCart(\" \"\"  \"1:4\"  \"\\\\Shop\\\\Service\\\\BaseCart\"  ]
  [\"f\" \"\\\\Shop\\\\Service\\\\Cart(\" \"\"  \"2:6\"  \"\\\\Shop\\\\Service\\\\Cart\"  ]
]
(
  (\"\\\\Shop\\\\Service\\\\Cart\". [ \"\\\\Shop\\\\Service\\\\BaseCart\" ])
)
[
  \"@@PROJECT@@/src/Model/Product.php\"
  \"@@PROJECT@@/src/Service/BaseCart.php\"
  \"@@PROJECT@@/src/Service/Cart.php\"
]
])
")

;; The recorded shape of an index with nothing in it -- also real
;; phpctags output, from a run against an empty project.  It stands in
;; for `tags-vendor.el', which for this project is a 2.5 MB dump of the
;; PHP distribution's own built-in classes: no workflow here completes
;; a built-in, and a copy of PHP's symbol table does not belong in this
;; repository.
(defconst ac-php-test-recorded-empty-index
  "(setq  g-ac-php-tmp-tags  [
(
)
[
]
(
)
[
]
])
")

;; The progress the real indexer printed, which ac-php's process
;; filter parses into `ac-php-phptags-index-progress'.
(defconst ac-php-test-recorded-progress
  "50% @@PROJECT@@/src/Model/Product.php
66% @@PROJECT@@/src/Service/BaseCart.php
83% @@PROJECT@@/src/Service/Cart.php
")

(defun ac-php-test-install-php ()
  "Install the recording/replaying `php' stand-in ahead of PATH."
  (let* ((bin (expand-file-name "bin" ac-php-test-root))
         (program (expand-file-name "php" bin))
         (root (directory-file-name ac-php-test-project)))
    (ac-php-test-write (expand-file-name "recorded-tags.el" ac-php-test-root)
                       (replace-regexp-in-string
                        "@@PROJECT@@" root ac-php-test-recorded-tags t t))
    (ac-php-test-write (expand-file-name "recorded-empty.el" ac-php-test-root)
                       ac-php-test-recorded-empty-index)
    (ac-php-test-write (expand-file-name "recorded-progress.txt" ac-php-test-root)
                       (replace-regexp-in-string
                        "@@PROJECT@@" root ac-php-test-recorded-progress t t))
    (ac-php-test-write
     program
     (concat
      "#!/bin/sh\n"
      "dir=\"$AC_PHP_TEST_DIR\"\n"
      "{ for a in \"$@\"; do printf '[%s]' \"$a\"; done; printf '\\n'; } >> \"$dir/php.log\"\n"
      "config=\n"
      "tags=\n"
      "for a in \"$@\"; do\n"
      "  case \"$a\" in\n"
      "    --config-file=*) config=${a#--config-file=} ;;\n"
      "    --tags_dir=*) tags=${a#--tags_dir=} ;;\n"
      "  esac\n"
      "done\n"
      ;; phpctags names the per-project directory after the project
      ;; root with every slash turned into a dash.  The root is the
      ;; directory of the config file, which ac-php spells with a
      ;; trailing `/.', and that dot is part of the name.
      "root=${config%/.ac-php-conf.json}\n"
      "save=\"$tags/tags$(printf '%s' \"$root\" | tr '/' '-')\"\n"
      "mkdir -p \"$save\"\n"
      "cp \"$dir/recorded-tags.el\" \"$save/tags.el\"\n"
      "cp \"$dir/recorded-empty.el\" \"$save/tags-vendor.el\"\n"
      "cat \"$dir/recorded-progress.txt\"\n"
      "exit 0\n"))
    (set-file-modes program #o755)
    (setenv "AC_PHP_TEST_DIR" (directory-file-name ac-php-test-root))
    (setenv "PATH" (concat bin path-separator (getenv "PATH")))
    (unless (member bin exec-path) (setq exec-path (cons bin exec-path)))
    (setq ac-php-php-executable program
          ac-php-tags-path (directory-file-name
                            (expand-file-name "cache" ac-php-test-root)))
    program))

(defun ac-php-test-php-calls ()
  "Every argument vector the stand-in was invoked with."
  (let ((path (expand-file-name "php.log" ac-php-test-root)))
    (if (file-regular-p path)
        (mapcar (lambda (line)
                  (let ((start 0) (result nil))
                    (while (string-match "\\[\\([^]]*\\)\\]" line start)
                      (push (match-string 1 line) result)
                      (setq start (match-end 0)))
                    (nreverse result)))
                (split-string (ac-php-test-read path) "\n" t))
      'no-invocation)))

(defun ac-php-test-wait-for-index ()
  "Block until the indexing subprocess ac-php started has finished."
  (let ((limit 400))
    (while (and (> limit 0) (get-process "ac-phptags"))
      (setq limit (1- limit))
      (accept-process-output nil 0.05))
    (accept-process-output nil 0.05)
    (null (get-process "ac-phptags"))))

(defmacro ac-php-test-in-php-buffer (relative &rest body)
  "Visit RELATIVE inside the fixture project with ac-php armed."
  `(let* ((path (expand-file-name ,relative ac-php-test-project))
          ;; The sandbox is inside the neomacs worktree, whose
          ;; .dir-locals.el would otherwise land on this buffer.
          (enable-dir-local-variables nil)
          (buffer (find-file-noselect path)))
     (unwind-protect
         (progn
           (set-window-buffer (selected-window) buffer)
           (set-buffer buffer)
           (php-mode)
           (auto-complete-mode 1)
           (setq ac-sources '(ac-source-php))
           ,@body)
       (when (buffer-live-p buffer)
         (with-current-buffer buffer (set-buffer-modified-p nil))
         (kill-buffer buffer)))))

(defun ac-php-test-candidates ()
  "Start completion at point and return the candidate strings."
  (ac-start :force-init t)
  (ac-update t)
  (mapcar #'copy-sequence ac-candidates))

(defun ac-php-test-plain (candidates)
  (sort (mapcar #'substring-no-properties candidates) #'string<))

(defun ac-php-test-annotated (candidates)
  "Each candidate with the properties ac-php attaches to it."
  (mapcar (lambda (name)
            (list (substring-no-properties name)
                  :tag-type (get-text-property 0 'ac-php-tag-type name)
                  :access (get-text-property 0 'ac-php-access name)
                  :return-type (get-text-property 0 'ac-php-return-type name)
                  :from (get-text-property 0 'ac-php-from name)
                  :help (let ((help (get-text-property 0 'ac-php-help name)))
                          (and help (substring-no-properties help)))))
          (sort (copy-sequence candidates)
                (lambda (a b) (string< (substring-no-properties a)
                                       (substring-no-properties b))))))

(defmacro ac-php-test-outcome (&rest body)
  `(condition-case error (list :ok (progn ,@body))
     (error (list :error (car error) (cdr error)))))
"##;

fn ac_php_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AC_PHP_MELPA_PIN, "ac-php.el")
        .expect("prepare pinned ac-php source and dependencies below ./tmp")
        .with_prelude(AC_PHP_TEST_PRELUDE)
        .with_timeout(AC_PHP_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed ac-php parity test").into()
}

/// Multi-probe batch for `assert_ac_php_parity` cases (2a).
pub(crate) fn assert_ac_php_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ac_php_oracle(), &name, "ac_php_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ac_php_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ac_php_batch(&cases);
}

// END generated package batch tests
