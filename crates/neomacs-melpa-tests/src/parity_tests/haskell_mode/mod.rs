use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, HASKELL_MODE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const HASKELL_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const HASKELL_MODE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'haskell-mode)
(require 'haskell-align-imports)
(require 'haskell-decl-scan)
(require 'haskell-sort-imports)
(require 'haskell-collapse)

(setq haskell-decl-scan-sort-imenu t
      haskell-decl-scan-bindings-as-variables t)

(defun haskell-test-face-at (text &optional occurrence)
  (goto-char (point-min))
  (let (end)
    (dotimes (_ (or occurrence 1))
      (setq end (search-forward text)))
    (get-text-property (- end (length text)) 'face)))

(defun haskell-test-imenu-shape (index)
  (mapcar
   (lambda (item)
     (if (markerp (cdr item))
         (list (car item) (line-number-at-pos (cdr item)))
       (cons (car item)
             (mapcar
              (lambda (entry)
                (list (car entry) (line-number-at-pos (cdr entry))))
              (cdr item)))))
   index))

(defun haskell-test-overlay-shape ()
  (mapcar
   (lambda (overlay)
     (list (line-number-at-pos (overlay-start overlay))
           (line-number-at-pos (overlay-end overlay))
           (overlay-get overlay 'invisible)
           (overlay-get overlay 'hs)))
   (overlays-in (point-min) (point-max))))
"##;

fn haskell_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(HASKELL_MODE_MELPA_PIN, "haskell-mode.el")
        .expect("prepare pinned haskell-mode source below ./tmp")
        .with_prelude(HASKELL_MODE_TEST_PRELUDE)
        .with_timeout(HASKELL_MODE_TEST_TIMEOUT)
}

fn realistic_module_activates_language_services_and_builds_declaration_index() -> ParityBatchCase {
    ParityBatchCase::value(
        "realistic_module_activates_language_services_and_builds_declaration_index",
        r##"
(with-temp-buffer
  (insert
   "{-# LANGUAGE OverloadedStrings #-}\n"
   "module Commerce.Checkout (Order(..), checkoutTotal) where\n\n"
   "import qualified Data.Map.Strict as Map\n"
   "import Data.Text (Text)\n\n"
   "-- | A customer order waiting to be priced.\n"
   "data Order = Order { orderId :: Int, items :: [Int] }\n"
   "  deriving (Show)\n\n"
   "class Priced a where\n"
   "  price :: a -> Int\n\n"
   "instance Priced Order where\n"
   "  price = checkoutTotal\n\n"
   "checkoutTotal :: Order -> Int\n"
   "checkoutTotal order = sum (items order)\n")
  (haskell-mode)
  (font-lock-ensure (point-min) (point-max))
  (goto-char (point-min))
  (search-forward "Map.Strict")
  (let ((qualified-ident (haskell-ident-at-point)))
    (list
     :mode major-mode
     :editing
     (list indent-tabs-mode tab-width comment-start
           forward-sexp-function indent-line-function)
     :completion completion-at-point-functions
     :qualified-ident qualified-ident
     :faces
     (list (haskell-test-face-at "LANGUAGE")
           (haskell-test-face-at "module")
           (haskell-test-face-at "Commerce.Checkout")
           (haskell-test-face-at "data")
           (haskell-test-face-at "Order")
           (haskell-test-face-at "checkoutTotal" 3)
           (haskell-test-face-at "customer order"))
     :imenu (haskell-test-imenu-shape
             (haskell-ds-create-imenu-index)))))
"##,
        expect![[
            r##"OK (:mode haskell-mode :editing (nil 8 "--" haskell-forward-sexp haskell-indentation-indent-line) :completion (haskell-completions-sync-repl-completion-at-point haskell-completions-completion-at-point t) :qualified-ident "Data.Map.Strict" :faces (haskell-pragma-face haskell-keyword-face haskell-constructor-face haskell-constructor-face haskell-constructor-face haskell-definition-face font-lock-doc-face) :imenu (("Variables" ("checkoutTotal" 17)) ("Classes" ("Priced" 11)) ("Imports" ("Data.Map.Strict as Map" 4) ("Data.Text (Text)" 5)) ("Instances" ("Priced Order" 14)) ("Datatypes" ("Order" 8))))"##
        ]],
    )
}

fn import_formatting_sorts_multiline_declarations_and_aligns_columns() -> ParityBatchCase {
    ParityBatchCase::value(
        "import_formatting_sorts_multiline_declarations_and_aligns_columns",
        r##"
(with-temp-buffer
  (insert
   "module Commerce.Checkout where\n\n"
   "import qualified Data.Text as Text\n"
   "import \"aeson\" Data.Aeson (Value, encode)\n"
   "import Data.Map.Strict hiding (map)\n"
   "import Control.Monad (unless, when)\n"
   "import qualified Data.ByteString.Lazy as Lazy\n"
   "import Data.Aeson.Parser.Internal (decodeWith, decodeStrictWith,\n"
   "                                   eitherDecodeWith, jsonEOF)\n\n"
   "checkout = pure ()\n")
  (haskell-mode)
  (goto-char (point-min))
  (search-forward "Data.Map.Strict")
  (let ((before-line (line-number-at-pos))
        (before-column (current-column)))
    (haskell-mode-format-imports)
    (list :source (buffer-substring-no-properties (point-min) (point-max))
          :point (list before-line before-column
                       (line-number-at-pos) (current-column)))))
"##,
        expect![[
            r##"OK (:source "module Commerce.Checkout where\n\nimport           Control.Monad (unless, when)\nimport \"aeson\"   Data.Aeson (Value, encode)\nimport Data.Aeson.Parser.Internal (decodeWith, decodeStrictWith,\n                                   eitherDecodeWith, jsonEOF)\nimport qualified Data.ByteString.Lazy as Lazy\nimport           Data.Map.Strict hiding (map)\nimport qualified Data.Text as Text\n\ncheckout = pure ()\n" :point (5 22 8 22))"##
        ]],
    )
}

fn layout_indentation_formats_a_nested_checkout_workflow() -> ParityBatchCase {
    ParityBatchCase::value(
        "layout_indentation_formats_a_nested_checkout_workflow",
        r##"
(with-temp-buffer
  (haskell-mode)
  (insert "checkout order = do")
  (dolist (line '("items <- loadItems order"
                  "case items of"
                  "[] -> pure 0"
                  "xs -> do"
                  "subtotal <- priceItems xs"
                  "if subtotal > 100"
                  "then applyDiscount subtotal"
                  "else pure subtotal"))
    (haskell-indentation-newline-and-indent)
    (insert line))
  (insert "\n")
  (list :source (buffer-substring-no-properties (point-min) (point-max))
        :indents
        (save-excursion
          (goto-char (point-min))
          (let (columns)
            (while (not (eobp))
              (push (current-indentation) columns)
              (forward-line))
            (nreverse columns)))
        :balanced (condition-case nil
                      (progn (check-parens) t)
                    (error nil))))
"##,
        expect![[
            r##"OK (:source "checkout order = do\n  items <- loadItems order\n  case items of\n    [] -> pure 0\n    xs -> do\n      subtotal <- priceItems xs\n      if subtotal > 100\n        then applyDiscount subtotal\n        else pure subtotal\n" :indents (0 2 2 4 4 6 6 8 8) :balanced t)"##
        ]],
    )
}

fn declaration_and_identifier_navigation_tracks_real_source_shapes() -> ParityBatchCase {
    ParityBatchCase::value(
        "declaration_and_identifier_navigation_tracks_real_source_shapes",
        r##"
(with-temp-buffer
  (insert
   "module Commerce.Checkout where\n\n"
   "import qualified Data.Map.Strict as Map\n\n"
   "data Checkout = Checkout Int\n\n"
   "checkoutTotal :: Checkout -> Int\n"
   "checkoutTotal value = value `withTax` 20\n\n"
   "withTax :: Int -> Int -> Int\n"
   "withTax subtotal tax = subtotal + tax\n\n"
   "qualifiedLookup key orders = Map.lookup key orders\n")
  (haskell-mode)
  (font-lock-ensure (point-min) (point-max))
  (let (forward-lines backward-lines identifiers)
    (goto-char (point-min))
    (dotimes (_ 5)
      (haskell-ds-forward-decl)
      (push (list (line-number-at-pos)
                  (buffer-substring-no-properties
                   (line-beginning-position) (line-end-position)))
            forward-lines))
    (goto-char (point-max))
    (dotimes (_ 3)
      (haskell-ds-backward-decl)
      (push (list (line-number-at-pos)
                  (buffer-substring-no-properties
                   (line-beginning-position) (line-end-position)))
            backward-lines))
    (dolist (needle '("`withTax`" "Map.lookup" "subtotal + tax"))
      (goto-char (point-min))
      (search-forward needle)
      (when (string= needle "subtotal + tax") (backward-char 5))
      (push (list needle
                  (haskell-ident-at-point)
                  (let ((span (haskell-spanable-pos-at-point)))
                    (and span
                         (buffer-substring-no-properties
                          (car span) (cdr span)))))
            identifiers))
    (list :forward (nreverse forward-lines)
          :backward (nreverse backward-lines)
          :identifiers (nreverse identifiers))))
"##,
        expect![[
            r##"OK (:forward ((2 "") (3 "import qualified Data.Map.Strict as Map") (4 "") (5 "data Checkout = Checkout Int") (6 "")) :backward ((13 "qualifiedLookup key orders = Map.lookup key orders") (10 "withTax :: Int -> Int -> Int") (7 "checkoutTotal :: Checkout -> Int")) :identifiers (("`withTax`" "withTax" "`withTax`") ("Map.lookup" "Map.lookup" "Map.lookup") ("subtotal + tax" "+" "+")))"##
        ]],
    )
}

fn folding_and_scc_annotation_support_a_debugging_edit_cycle() -> ParityBatchCase {
    ParityBatchCase::value(
        "folding_and_scc_annotation_support_a_debugging_edit_cycle",
        r##"
(with-temp-buffer
  (insert
   "checkout order = do\n"
   "  items <- loadItems order\n"
   "  subtotal <- priceItems items\n"
   "  pure subtotal\n\n"
   "audit order = writeAudit order\n")
  (haskell-mode)
  (goto-char (point-min))
  (haskell-hide-toggle)
  (let ((folded (haskell-test-overlay-shape)))
    (haskell-hide-toggle)
    (let ((unfolded (haskell-test-overlay-shape)))
      (goto-char (point-min))
      (search-forward "= ")
      (haskell-mode-toggle-scc-at-point)
      (insert "checkout-total")
      (let ((annotated (buffer-substring-no-properties
                        (point-min) (point-max))))
        (haskell-mode-toggle-scc-at-point)
        (list :folded folded
              :unfolded unfolded
              :annotated annotated
              :restored (buffer-substring-no-properties
                         (point-min) (point-max))
              :point (list (line-number-at-pos) (current-column)))))))
"##,
        expect![[
            r##"OK (:folded ((1 5 hs code)) :unfolded nil :annotated "checkout order = {-# SCC \"checkout-total\" #-} do\n  items <- loadItems order\n  subtotal <- priceItems items\n  pure subtotal\n\naudit order = writeAudit order\n" :restored "checkout order = do\n  items <- loadItems order\n  subtotal <- priceItems items\n  pure subtotal\n\naudit order = writeAudit order\n" :point (1 17))"##
        ]],
    )
}

#[test]
fn haskell_mode_package_batch() {
    let cases = vec![
        realistic_module_activates_language_services_and_builds_declaration_index(),
        import_formatting_sorts_multiline_declarations_and_aligns_columns(),
        layout_indentation_formats_a_nested_checkout_workflow(),
        declaration_and_identifier_navigation_tracks_real_source_shapes(),
        folding_and_scc_annotation_support_a_debugging_edit_cycle(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed haskell-mode parity test");
    assert_oracle_batch_cases(
        haskell_mode_oracle(),
        test_name,
        "haskell_mode_parity",
        &cases,
    );
}
