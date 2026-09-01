use expect_test::expect;

use super::ParityBatchCase;

fn mode_lifecycle_wraps_real_company_without_duplicates() -> ParityBatchCase {
    ParityBatchCase::value(
        "mode_lifecycle_wraps_real_company_without_duplicates",
        r####"
(company-statistics361-test-run
 "lifecycle"
 (lambda (world)
   (let* ((history (plist-get world :history))
          (enabled
           (company-statistics361-test-configure
            4 'heavy nil nil history))
          (reenabled (company-statistics-mode 1))
          (registrations (company-statistics361-test-mode-state))
          (chosen
           (company-statistics361-test-session
            '(:file "project α/lifecycle.el" :mode emacs-lisp-mode
              :keyword "if" :keyword-face t :parent "ledger"
              :steps 2 :finish t)))
          (trained
           (company-statistics361-test-session
            '(:file "project α/lifecycle.el" :mode emacs-lisp-mode
              :keyword "if" :keyword-face t :parent "ledger")))
          (ledger (company-statistics361-test-ledger)))
     (company-statistics-mode -1)
     (let* ((disabled-state (company-statistics361-test-mode-state))
            (disabled-order
             (company-statistics361-test-session
              '(:file "project α/lifecycle.el" :mode emacs-lisp-mode
                :keyword "if" :keyword-face t :parent "ledger")))
            (preserved (company-statistics361-test-ledger))
            (enabled-again (company-statistics-mode 1))
            (enabled-again-state (company-statistics361-test-mode-state))
            (restored-order
             (company-statistics361-test-session
              '(:file "project α/lifecycle.el" :mode emacs-lisp-mode
                :keyword "if" :keyword-face t :parent "ledger"))))
       (list :enable enabled :reenable reenabled
             :registrations registrations :chosen chosen :trained trained
             :ledger ledger :disabled disabled-state
             :disabled-order disabled-order :preserved preserved
             :enable-again enabled-again :enabled-again enabled-again-state
             :restored-order restored-order)))))
"####,
        expect![[
            r#"OK (:result (:enable t :reenable t :registrations (:mode t :transformers 1 :transformer-last t :started 1 :finished 1) :chosen (:before "(if ledger.ca" :keys "C-c c C-n C-n RET" :setup-calls ((init nil)) :events ((:started :manual t :prefix "ca" :candidates ("cache-alpha" "cache-beta" "cache-界") :selection 0 :subject-active t :context ((:keyword "if") (:file "[ROOT]/project α/lifecycle.el"))) (:finished "cache-界" :result-properties (company-statistics361-source-index 2) :index 1) (:started :manual nil :prefix "cache-界" :candidates ("cache-界") :selection 0 :subject-active t :context ((:keyword "if") (:file "[ROOT]/project α/lifecycle.el")))) :calls ((prefix nil) (candidates "ca") (sorted nil) (duplicates nil) (ignore-case nil) (expand-common "ca") (candidates "ca") (adjust-boundaries "cache-alpha") (ignore-case nil) (ignore-case nil) (ignore-case nil) (no-cache "ca") (prefix nil) (ignore-case nil) (ignore-case nil) (adjust-boundaries "cache-界") (ignore-case nil) (ignore-case nil) (post-completion "cache-界") (prefix nil) (ignore-case nil) (set-min-prefix 0) (candidates "cache-界") (sorted nil) (duplicates nil) (ignore-case nil) (adjust-boundaries "cache-界") (ignore-case nil)) :backend ((:post "cache-界" :properties (company-statistics361-source-index 2))) :after "(if ledger.cache-界" :point 19 :modified t :active nil :tooltip nil :runtime (:timer nil :tooltip-timer nil :echo-timer nil :cache-count 0) :unread nil) :trained (:before "(if ledger.ca" :keys "C-c c C-g" :setup-calls nil :events ((:started :manual t :prefix "ca" :candidates ("cache-界" "cache-alpha" "cache-beta") :selection 0 :subject-active t :context ((:keyword "if") (:file "[ROOT]/project α/lifecycle.el")))) :calls ((prefix nil) (candidates "ca") (sorted nil) (duplicates nil) (ignore-case nil) (expand-common "ca") (candidates "ca") (adjust-boundaries "cache-界") (ignore-case nil) (ignore-case nil) (ignore-case nil) (no-cache "ca") (prefix nil) (ignore-case nil) (ignore-case nil)) :backend nil :after "(if ledger.cache-" :point 18 :modified t :active nil :tooltip nil :runtime (:timer nil :tooltip-timer nil :echo-timer nil :cache-count 0) :unread nil) :ledger (:size 4 :scores (("cache-界" :key-properties nil :updates (((:file "[ROOT]/project α/lifecycle.el") . 1) ((:keyword "if") . 1) (emacs-lisp-mode . 1) (:global . 1)))) :log [("cache-界" (:global . 1) (emacs-lisp-mode . 1) ((:keyword "if") . 1) ((:file "[ROOT]/project α/lifecycle.el") . 1)) nil nil nil] :index 1 :alias nil) :disabled (:mode nil :transformers 0 :transformer-last nil :started 0 :finished 0) :disabled-order (:before "(if ledger.ca" :keys "C-c c C-g" :setup-calls nil :events ((:started :manual t :prefix "ca" :candidates ("cache-alpha" "cache-beta" "cache-界") :selection 0 :subject-active nil :context nil)) :calls ((prefix nil) (candidates "ca") (sorted nil) (duplicates nil) (ignore-case nil) (expand-common "ca") (candidates "ca") (adjust-boundaries "cache-alpha") (ignore-case nil) (ignore-case nil) (ignore-case nil) (no-cache "ca") (prefix nil) (ignore-case nil) (ignore-case nil)) :backend nil :after "(if ledger.cache-" :point 18 :modified t :active nil :tooltip nil :runtime (:timer nil :tooltip-timer nil :echo-timer nil :cache-count 0) :unread nil) :preserved (:size 4 :scores (("cache-界" :key-properties nil :updates (((:file "[ROOT]/project α/lifecycle.el") . 1) ((:keyword "if") . 1) (emacs-lisp-mode . 1) (:global . 1)))) :log [("cache-界" (:global . 1) (emacs-lisp-mode . 1) ((:keyword "if") . 1) ((:file "[ROOT]/project α/lifecycle.el") . 1)) nil nil nil] :index 1 :alias nil) :enable-again t :enabled-again (:mode t :transformers 1 :transformer-last t :started 1 :finished 1) :restored-order (:before "(if ledger.ca" :keys "C-c c C-g" :setup-calls nil :events ((:started :manual t :prefix "ca" :candidates ("cache-界" "cache-alpha" "cache-beta") :selection 0 :subject-active t :context ((:keyword "if") (:file "[ROOT]/project α/lifecycle.el")))) :calls ((prefix nil) (candidates "ca") (sorted nil) (duplicates nil) (ignore-case nil) (expand-common "ca") (candidates "ca") (adjust-boundaries "cache-界") (ignore-case nil) (ignore-case nil) (ignore-case nil) (no-cache "ca") (prefix nil) (ignore-case nil) (ignore-case nil)) :backend nil :after "(if ledger.cache-" :point 18 :modified t :active nil :tooltip nil :runtime (:timer nil :tooltip-timer nil :echo-timer nil :cache-count 0) :unread nil)) :cleanup clean)"#
        ]],
    )
}

fn heavy_context_ranks_real_choices_by_keyword_parent_file_and_mode() -> ParityBatchCase {
    ParityBatchCase::value(
        "heavy_context_ranks_real_choices_by_keyword_parent_file_and_mode",
        r####"
(company-statistics361-test-run
 "heavy-context"
 (lambda (world)
   (company-statistics361-test-configure
    8 'heavy nil nil (plist-get world :history))
   (let* ((billing-request
           '(:file "project α/src/billing.c" :mode c-mode
             :keyword "if" :keyword-face t :parent "service"))
          (search-request
           '(:file "project β/notes/search.txt" :mode text-mode
             :keyword "release" :parent "client"))
          (neutral-request '(:mode fundamental-mode))
          (keyword-request
           '(:file "probes/keyword-only.el" :mode emacs-lisp-mode
             :keyword "if" :keyword-face t :parent "other"))
          (parent-request
           '(:file "probes/parent-only.txt" :mode fundamental-mode
             :parent "service"))
          (file-request
           '(:file "project α/src/billing.c" :mode fundamental-mode
             :parent "other"))
          (beta
           (company-statistics361-test-session
            '(:file "project α/src/billing.c" :mode c-mode
              :keyword "if" :keyword-face t :parent "service"
              :steps 1 :finish t)))
          (alpha
           (company-statistics361-test-session
            '(:file "project β/notes/search.txt" :mode text-mode
              :keyword "release" :parent "client"
              :steps 1 :finish t)))
          (billing-prime
           (company-statistics361-test-session
            billing-request))
          (billing
           (company-statistics361-test-session
            billing-request))
          (search-prime
           (company-statistics361-test-session
            search-request))
          (search
           (company-statistics361-test-session
            search-request))
          (neutral-prime
           (company-statistics361-test-session
            neutral-request))
          (neutral
           (company-statistics361-test-session
            neutral-request))
          (mode-only
           (company-statistics361-test-session
            '(:file "probes/mode-only.c" :mode c-mode
              :keyword "deploy" :parent "other")))
          (keyword-prime
           (company-statistics361-test-session
            keyword-request))
          (keyword-only
           (company-statistics361-test-session
            keyword-request))
          (parent-prime
           (company-statistics361-test-session
            parent-request))
          (parent-only
           (company-statistics361-test-session
            parent-request))
          (file-prime
           (company-statistics361-test-session
            file-request))
          (file-only
           (company-statistics361-test-session
            file-request)))
     (list :beta beta :alpha alpha
           :exact
           (list :billing (list :prime billing-prime :actual billing)
                 :search (list :prime search-prime :actual search)
                 :neutral (list :prime neutral-prime :actual neutral))
           :independent
           (list :mode mode-only
                 :keyword (list :prime keyword-prime :actual keyword-only)
                 :parent (list :prime parent-prime :actual parent-only)
                 :file (list :prime file-prime :actual file-only))
           :ledger (company-statistics361-test-ledger)
           :registrations (company-statistics361-test-mode-state)))))
"####,
        expect![[
            r#"OK (:result (:beta (:before "if service.ca" :keys "C-c c C-n RET" :setup-calls ((init nil)) :events ((:started :manual t :prefix "ca" :candidates ("cache-alpha" "cache-beta" "cache-界") :selection 0 :subject-active t :context ((:keyword "if") (:symbol "service") (:file "[ROOT]/project α/src/billing.c"))) (:finished "cache-beta" :result-properties (company-statistics361-source-index 1) :index 1) (:started :manual nil :prefix "cache-beta" :candidates ("cache-beta") :selection 0 :subject-active t :context ((:keyword "if") (:symbol "service") (:file "[ROOT]/project α/src/billing.c")))) :calls ((prefix nil) (candidates "ca") (sorted nil) (duplicates nil) (ignore-case nil) (expand-common "ca") (candidates "ca") (adjust-boundaries "cache-alpha") (ignore-case nil) (ignore-case nil) (ignore-case nil) (no-cache "ca") (prefix nil) (ignore-case nil) (ignore-case nil) (adjust-boundaries "cache-beta") (ignore-case nil) (ignore-case nil) (post-completion "cache-beta") (prefix nil) (ignore-case nil) (set-min-prefix 0) (candidates "cache-beta") (sorted nil) (duplicates nil) (ignore-case nil) (adjust-boundaries "cache-beta") (ignore-case nil)) :backend ((:post "cache-beta" :properties (company-statistics361-source-index 1))) :after "if service.cache-beta" :point 22 :modified t :active nil :tooltip nil :runtime (:timer nil :tooltip-timer nil :echo-timer nil :cache-count 0) :unread nil) :alpha (:before "release client.ca" :keys "C-c c C-n RET" :setup-calls ((init nil)) :events ((:started :manual t :prefix "ca" :candidates ("cache-beta" "cache-alpha" "cache-界") :selection 0 :subject-active t :context ((:symbol "client") (:file "[ROOT]/project β/notes/search.txt"))) (:finished "cache-alpha" :result-properties (company-statistics361-source-index 0) :index 2) (:started :manual nil :prefix "cache-alpha" :candidates ("cache-alpha") :selection 0 :subject-active t :context ((:symbol "client") (:file "[ROOT]/project β/notes/search.txt")))) :calls ((prefix nil) (candidates "ca") (sorted nil) (duplicates nil) (ignore-case nil) (expand-common "ca") (candidates "ca") (adjust-boundaries "cache-beta") (ignore-case nil) (ignore-case nil) (ignore-case nil) (no-cache "ca") (prefix nil) (ignore-case nil) (ignore-case nil) (adjust-boundaries "cache-alpha") (ignore-case nil) (ignore-case nil) (post-completion "cache-alpha") (prefix nil) (ignore-case nil) (set-min-prefix 0) (candidates "cache-alpha") (sorted nil) (duplicates nil) (ignore-case nil) (adjust-boundaries "cache-alpha") (ignore-case nil)) :backend ((:post "cache-alpha" :properties (company-statistics361-source-index 0))) :after "release client.cache-alpha" :point 27 :modified t :active nil :tooltip nil :runtime (:timer nil :tooltip-timer nil :echo-timer nil :cache-count 0) :unread nil) :exact (:billing (:prime (:before "if service.ca" :keys "C-c c C-g" :setup-calls nil :events ((:started :manual t :prefix "ca" :candidates ("cache-alpha" "cache-beta" "cache-界") :selection 0 :subject-active t :context ((:keyword "if") (:symbol "service") (:file "[ROOT]/project α/src/billing.c")))) :calls ((prefix nil) (candidates "ca") (sorted nil) (duplicates nil) (ignore-case nil) (expand-common "ca") (candidates "ca") (adjust-boundaries "cache-alpha") (ignore-case nil) (ignore-case nil) (ignore-case nil) (no-cache "ca") (prefix nil) (ignore-case nil) (ignore-case nil)) :backend nil :after "if service.cache-" :point 18 :modified t :active nil :tooltip nil :runtime (:timer nil :tooltip-timer nil :echo-timer nil :cache-count 0) :unread nil) :actual (:before "if service.ca" :keys "C-c c C-g" :setup-calls nil :events ((:started :manual t :prefix "ca" :candidates ("cache-beta" "cache-alpha" "cache-界") :selection 0 :subject-active t :context ((:keyword "if") (:symbol "service") (:file "[ROOT]/project α/src/billing.c")))) :calls ((prefix nil) (candidates "ca") (sorted nil) (duplicates nil) (ignore-case nil) (expand-common "ca") (candidates "ca") (adjust-boundaries "cache-beta") (ignore-case nil) (ignore-case nil) (ignore-case nil) (no-cache "ca") (prefix nil) (ignore-case nil) (ignore-case nil)) :backend nil :after "if service.cache-" :point 18 :modified t :active nil :tooltip nil :runtime (:timer nil :tooltip-timer nil :echo-timer nil :cache-count 0) :unread nil)) :search (:prime (:before "release client.ca" :keys "C-c c C-g" :setup-calls nil :events ((:started :manual t :prefix "ca" :candidates ("cache-beta" "cache-alpha" "cache-界") :selection 0 :subject-active t :context ((:symbol "client") (:file "[ROOT]/project β/notes/search.txt")))) :calls ((prefix nil) (candidates "ca") (sorted nil) (duplicates nil) (ignore-case nil) (expand-common "ca") (candidates "ca") (adjust-boundaries "cache-beta") (ignore-case nil) (ignore-case nil) (ignore-case nil) (no-cache "ca") (prefix nil) (ignore-case nil) (ignore-case nil)) :backend nil :after "release client.cache-" :point 22 :modified t :active nil :tooltip nil :runtime (:timer nil :tooltip-timer nil :echo-timer nil :cache-count 0) :unread nil) :actual (:before "release client.ca" :keys "C-c c C-g" :setup-calls nil :events ((:started :manual t :prefix "ca" :candidates ("cache-alpha" "cache-beta" "cache-界") :selection 0 :subject-active t :context ((:symbol "client") (:file "[ROOT]/project β/notes/search.txt")))) :calls ((prefix nil) (candidates "ca") (sorted nil) (duplicates nil) (ignore-case nil) (expand-common "ca") (candidates "ca") (adjust-boundaries "cache-alpha") (ignore-case nil) (ignore-case nil) (ignore-case nil) (no-cache "ca") (prefix nil) (ignore-case nil) (ignore-case nil)) :backend nil :after "release client.cache-" :point 22 :modified t :active nil :tooltip nil :runtime (:timer nil :tooltip-timer nil :echo-timer nil :cache-count 0) :unread nil)) :neutral (:prime (:before "ca" :keys "C-c c C-g" :setup-calls ((init nil)) :events ((:started :manual t :prefix "ca" :candidates ("cache-alpha" "cache-beta" "cache-界") :selection 0 :subject-active t :context nil)) :calls ((prefix nil) (candidates "ca") (sorted nil) (duplicates nil) (ignore-case nil) (expand-common "ca") (candidates "ca") (adjust-boundaries "cache-alpha") (ignore-case nil) (ignore-case nil) (ignore-case nil) (no-cache "ca") (prefix nil) (ignore-case nil) (ignore-case nil)) :backend nil :after "cache-" :point 7 :modified t :active nil :tooltip nil :runtime (:timer nil :tooltip-timer nil :echo-timer nil :cache-count 0) :unread nil) :actual (:before "ca" :keys "C-c c C-g" :setup-calls nil :events ((:started :manual t :prefix "ca" :candidates ("cache-alpha" "cache-beta" "cache-界") :selection 0 :subject-active t :context nil)) :calls ((prefix nil) (candidates "ca") (sorted nil) (duplicates nil) (ignore-case nil) (expand-common "ca") (candidates "ca") (adjust-boundaries "cache-alpha") (ignore-case nil) (ignore-case nil) (ignore-case nil) (no-cache "ca") (prefix nil) (ignore-case nil) (ignore-case nil)) :backend nil :after "cache-" :point 7 :modified t :active nil :tooltip nil :runtime (:timer nil :tooltip-timer nil :echo-timer nil :cache-count 0) :unread nil))) :independent (:mode (:before "deploy other.ca" :keys "C-c c C-g" :setup-calls ((init nil)) :events ((:started :manual t :prefix "ca" :candidates ("cache-beta" "cache-alpha" "cache-界") :selection 0 :subject-active t :context ((:symbol "other") (:file "[ROOT]/probes/mode-only.c")))) :calls ((prefix nil) (candidates "ca") (sorted nil) (duplicates nil) (ignore-case nil) (expand-common "ca") (candidates "ca") (adjust-boundaries "cache-beta") (ignore-case nil) (ignore-case nil) (ignore-case nil) (no-cache "ca") (prefix nil) (ignore-case nil) (ignore-case nil)) :backend nil :after "deploy other.cache-" :point 20 :modified t :active nil :tooltip nil :runtime (:timer nil :tooltip-timer nil :echo-timer nil :cache-count 0) :unread nil) :keyword (:prime (:before "(if other.ca" :keys "C-c c C-g" :setup-calls ((init nil)) :events ((:started :manual t :prefix "ca" :candidates ("cache-alpha" "cache-beta" "cache-界") :selection 0 :subject-active t :context ((:keyword "if") (:file "[ROOT]/probes/keyword-only.el")))) :calls ((prefix nil) (candidates "ca") (sorted nil) (duplicates nil) (ignore-case nil) (expand-common "ca") (candidates "ca") (adjust-boundaries "cache-alpha") (ignore-case nil) (ignore-case nil) (ignore-case nil) (no-cache "ca") (prefix nil) (ignore-case nil) (ignore-case nil)) :backend nil :after "(if other.cache-" :point 17 :modified t :active nil :tooltip nil :runtime (:timer nil :tooltip-timer nil :echo-timer nil :cache-count 0) :unread nil) :actual (:before "(if other.ca" :keys "C-c c C-g" :setup-calls nil :events ((:started :manual t :prefix "ca" :candidates ("cache-beta" "cache-alpha" "cache-界") :selection 0 :subject-active t :context ((:keyword "if") (:file "[ROOT]/probes/keyword-only.el")))) :calls ((prefix nil) (candidates "ca") (sorted nil) (duplicates nil) (ignore-case nil) (expand-common "ca") (candidates "ca") (adjust-boundaries "cache-beta") (ignore-case nil) (ignore-case nil) (ignore-case nil) (no-cache "ca") (prefix nil) (ignore-case nil) (ignore-case nil)) :backend nil :after "(if other.cache-" :point 17 :modified t :active nil :tooltip nil :runtime (:timer nil :tooltip-timer nil :echo-timer nil :cache-count 0) :unread nil)) :parent (:prime (:before "service.ca" :keys "C-c c C-g" :setup-calls ((init nil)) :events ((:started :manual t :prefix "ca" :candidates ("cache-beta" "cache-alpha" "cache-界") :selection 0 :subject-active t :context ((:symbol "service") (:file "[ROOT]/probes/parent-only.txt")))) :calls ((prefix nil) (candidates "ca") (sorted nil) (duplicates nil) (ignore-case nil) (expand-common "ca") (candidates "ca") (adjust-boundaries "cache-beta") (ignore-case nil) (ignore-case nil) (ignore-case nil) (no-cache "ca") (prefix nil) (ignore-case nil) (ignore-case nil)) :backend nil :after "service.cache-" :point 15 :modified t :active nil :tooltip nil :runtime (:timer nil :tooltip-timer nil :echo-timer nil :cache-count 0) :unread nil) :actual (:before "service.ca" :keys "C-c c C-g" :setup-calls nil :events ((:started :manual t :prefix "ca" :candidates ("cache-beta" "cache-alpha" "cache-界") :selection 0 :subject-active t :context ((:symbol "service") (:file "[ROOT]/probes/parent-only.txt")))) :calls ((prefix nil) (candidates "ca") (sorted nil) (duplicates nil) (ignore-case nil) (expand-common "ca") (candidates "ca") (adjust-boundaries "cache-beta") (ignore-case nil) (ignore-case nil) (ignore-case nil) (no-cache "ca") (prefix nil) (ignore-case nil) (ignore-case nil)) :backend nil :after "service.cache-" :point 15 :modified t :active nil :tooltip nil :runtime (:timer nil :tooltip-timer nil :echo-timer nil :cache-count 0) :unread nil)) :file (:prime (:before "other.ca" :keys "C-c c C-g" :setup-calls ((init nil)) :events ((:started :manual t :prefix "ca" :candidates ("cache-beta" "cache-alpha" "cache-界") :selection 0 :subject-active t :context ((:symbol "other") (:file "[ROOT]/project α/src/billing.c")))) :calls ((prefix nil) (candidates "ca") (sorted nil) (duplicates nil) (ignore-case nil) (expand-common "ca") (candidates "ca") (adjust-boundaries "cache-beta") (ignore-case nil) (ignore-case nil) (ignore-case nil) (no-cache "ca") (prefix nil) (ignore-case nil) (ignore-case nil)) :backend nil :after "other.cache-" :point 13 :modified t :active nil :tooltip nil :runtime (:timer nil :tooltip-timer nil :echo-timer nil :cache-count 0) :unread nil) :actual (:before "other.ca" :keys "C-c c C-g" :setup-calls nil :events ((:started :manual t :prefix "ca" :candidates ("cache-beta" "cache-alpha" "cache-界") :selection 0 :subject-active t :context ((:symbol "other") (:file "[ROOT]/project α/src/billing.c")))) :calls ((prefix nil) (candidates "ca") (sorted nil) (duplicates nil) (ignore-case nil) (expand-common "ca") (candidates "ca") (adjust-boundaries "cache-beta") (ignore-case nil) (ignore-case nil) (ignore-case nil) (no-cache "ca") (prefix nil) (ignore-case nil) (ignore-case nil)) :backend nil :after "other.cache-" :point 13 :modified t :active nil :tooltip nil :runtime (:timer nil :tooltip-timer nil :echo-timer nil :cache-count 0) :unread nil))) :ledger (:size 8 :scores (("cache-alpha" :key-properties nil :updates (((:file "[ROOT]/project β/notes/search.txt") . 1) ((:symbol "client") . 1) (text-mode . 1) (:global . 1))) ("cache-beta" :key-properties nil :updates (((:file "[ROOT]/project α/src/billing.c") . 1) ((:symbol "service") . 1) ((:keyword "if") . 1) (c-mode . 1) (:global . 1)))) :log [("cache-beta" (:global . 1) (c-mode . 1) ((:keyword "if") . 1) ((:symbol "service") . 1) ((:file "[ROOT]/project α/src/billing.c") . 1)) ("cache-alpha" (:global . 1) (text-mode . 1) ((:symbol "client") . 1) ((:file "[ROOT]/project β/notes/search.txt") . 1)) nil nil nil nil nil nil] :index 2 :alias nil) :registrations (:mode t :transformers 1 :transformer-last t :started 1 :finished 1)) :cleanup clean)"#
        ]],
    )
}

fn light_ring_wrap_and_public_resize_surface_pinned_decay() -> ParityBatchCase {
    ParityBatchCase::value(
        "light_ring_wrap_and_public_resize_surface_pinned_decay",
        r####"
(company-statistics361-test-run
 "light-decay"
 (lambda (world)
   (company-statistics361-test-configure
    3 'light nil nil (plist-get world :history))
   (let* ((beta-one
           (company-statistics361-test-session
            '(:mode fundamental-mode :steps 1 :finish t)))
          (beta-two
           (company-statistics361-test-session
            '(:mode fundamental-mode :steps 0 :finish t)))
          (alpha
           (company-statistics361-test-session
            '(:mode fundamental-mode :steps 1 :finish t)))
          (before-wrap
           (company-statistics361-test-session
            '(:mode fundamental-mode)))
          (aliased
           (company-statistics361-test-ledger "cache-beta" 0))
          (unicode
           (company-statistics361-test-session
            '(:mode fundamental-mode :steps 2 :finish t)))
          (after-wrap
           (company-statistics361-test-session
            '(:mode fundamental-mode)))
          (wrapped (company-statistics361-test-ledger)))
     (customize-set-variable 'company-statistics-size 2)
     (let ((shrunk-order
            (company-statistics361-test-session
             '(:mode fundamental-mode)))
           (shrunk-ledger (company-statistics361-test-ledger)))
       (customize-set-variable 'company-statistics-size 5)
       (let ((enlarged-order
              (company-statistics361-test-session
               '(:mode fundamental-mode)))
             (enlarged-ledger (company-statistics361-test-ledger)))
         (let* ((post-enlarge-choice
                 (company-statistics361-test-session
                  '(:mode fundamental-mode :steps 1 :finish t)))
                (post-enlarge-order
                 (company-statistics361-test-session
                  '(:mode fundamental-mode)))
                (post-enlarge-ledger
                 (company-statistics361-test-ledger)))
           (list :choices (list beta-one beta-two alpha unicode)
                 :before-wrap before-wrap :aliased aliased
                 :after-wrap after-wrap :wrapped wrapped
                 :shrunk (list :order shrunk-order :ledger shrunk-ledger)
                 :enlarged (list :order enlarged-order
                                 :ledger enlarged-ledger)
                 :post-enlarge
                 (list :choice post-enlarge-choice
                       :order post-enlarge-order
                       :ledger post-enlarge-ledger))))))))
"####,
        expect![[
            r#"OK (:result (:choices ((:before "ca" :keys "C-c c C-n RET" :setup-calls ((init nil)) :events ((:started :manual t :prefix "ca" :candidates ("cache-alpha" "cache-beta" "cache-界") :selection 0 :subject-active t :context nil) (:finished "cache-beta" :result-properties (company-statistics361-source-index 1) :index 1) (:started :manual nil :prefix "cache-beta" :candidates ("cache-beta") :selection 0 :subject-active t :context nil)) :calls ((prefix nil) (candidates "ca") (sorted nil) (duplicates nil) (ignore-case nil) (expand-common "ca") (candidates "ca") (adjust-boundaries "cache-alpha") (ignore-case nil) (ignore-case nil) (ignore-case nil) (no-cache "ca") (prefix nil) (ignore-case nil) (ignore-case nil) (adjust-boundaries "cache-beta") (ignore-case nil) (ignore-case nil) (post-completion "cache-beta") (prefix nil) (ignore-case nil) (set-min-prefix 0) (candidates "cache-beta") (sorted nil) (duplicates nil) (ignore-case nil) (adjust-boundaries "cache-beta") (ignore-case nil)) :backend ((:post "cache-beta" :properties (company-statistics361-source-index 1))) :after "cache-beta" :point 11 :modified t :active nil :tooltip nil :runtime (:timer nil :tooltip-timer nil :echo-timer nil :cache-count 0) :unread nil) (:before "ca" :keys "C-c c RET" :setup-calls nil :events ((:started :manual t :prefix "ca" :candidates ("cache-beta" "cache-alpha" "cache-界") :selection 0 :subject-active t :context nil) (:finished "cache-beta" :result-properties (company-statistics361-source-index 1) :index 2) (:started :manual nil :prefix "cache-beta" :candidates ("cache-beta") :selection 0 :subject-active t :context nil)) :calls ((prefix nil) (candidates "ca") (sorted nil) (duplicates nil) (ignore-case nil) (expand-common "ca") (candidates "ca") (adjust-boundaries "cache-beta") (ignore-case nil) (ignore-case nil) (ignore-case nil) (no-cache "ca") (prefix nil) (ignore-case nil) (ignore-case nil) (adjust-boundaries "cache-beta") (ignore-case nil) (ignore-case nil) (post-completion "cache-beta") (prefix nil) (ignore-case nil) (set-min-prefix 0) (candidates "cache-beta") (sorted nil) (duplicates nil) (ignore-case nil) (adjust-boundaries "cache-beta") (ignore-case nil)) :backend ((:post "cache-beta" :properties (company-statistics361-source-index 1))) :after "cache-beta" :point 11 :modified t :active nil :tooltip nil :runtime (:timer nil :tooltip-timer nil :echo-timer nil :cache-count 0) :unread nil) (:before "ca" :keys "C-c c C-n RET" :setup-calls nil :events ((:started :manual t :prefix "ca" :candidates ("cache-beta" "cache-alpha" "cache-界") :selection 0 :subject-active t :context nil) (:finished "cache-alpha" :result-properties (company-statistics361-source-index 0) :index 0) (:started :manual nil :prefix "cache-alpha" :candidates ("cache-alpha") :selection 0 :subject-active t :context nil)) :calls ((prefix nil) (candidates "ca") (sorted nil) (duplicates nil) (ignore-case nil) (expand-common "ca") (candidates "ca") (adjust-boundaries "cache-beta") (ignore-case nil) (ignore-case nil) (ignore-case nil) (no-cache "ca") (prefix nil) (ignore-case nil) (ignore-case nil) (adjust-boundaries "cache-alpha") (ignore-case nil) (ignore-case nil) (post-completion "cache-alpha") (prefix nil) (ignore-case nil) (set-min-prefix 0) (candidates "cache-alpha") (sorted nil) (duplicates nil) (ignore-case nil) (adjust-boundaries "cache-alpha") (ignore-case nil)) :backend ((:post "cache-alpha" :properties (company-statistics361-source-index 0))) :after "cache-alpha" :point 12 :modified t :active nil :tooltip nil :runtime (:timer nil :tooltip-timer nil :echo-timer nil :cache-count 0) :unread nil) (:before "ca" :keys "C-c c C-n C-n RET" :setup-calls nil :events ((:started :manual t :prefix "ca" :candidates ("cache-beta" "cache-alpha" "cache-界") :selection 0 :subject-active t :context nil) (:finished "cache-界" :result-properties (company-statistics361-source-index 2) :index 1) (:started :manual nil :prefix "cache-界" :candidates ("cache-界") :selection 0 :subject-active t :context nil)) :calls ((prefix nil) (candidates "ca") (sorted nil) (duplicates nil) (ignore-case nil) (expand-common "ca") (candidates "ca") (adjust-boundaries "cache-beta") (ignore-case nil) (ignore-case nil) (ignore-case nil) (no-cache "ca") (prefix nil) (ignore-case nil) (ignore-case nil) (adjust-boundaries "cache-界") (ignore-case nil) (ignore-case nil) (post-completion "cache-界") (prefix nil) (ignore-case nil) (set-min-prefix 0) (candidates "cache-界") (sorted nil) (duplicates nil) (ignore-case nil) (adjust-boundaries "cache-界") (ignore-case nil)) :backend ((:post "cache-界" :properties (company-statistics361-source-index 2))) :after "cache-界" :point 8 :modified t :active nil :tooltip nil :runtime (:timer nil :tooltip-timer nil :echo-timer nil :cache-count 0) :unread nil)) :before-wrap (:before "ca" :keys "C-c c C-g" :setup-calls nil :events ((:started :manual t :prefix "ca" :candidates ("cache-beta" "cache-alpha" "cache-界") :selection 0 :subject-active t :context nil)) :calls ((prefix nil) (candidates "ca") (sorted nil) (duplicates nil) (ignore-case nil) (expand-common "ca") (candidates "ca") (adjust-boundaries "cache-beta") (ignore-case nil) (ignore-case nil) (ignore-case nil) (no-cache "ca") (prefix nil) (ignore-case nil) (ignore-case nil)) :backend nil :after "cache-" :point 7 :modified t :active nil :tooltip nil :runtime (:timer nil :tooltip-timer nil :echo-timer nil :cache-count 0) :unread nil) :aliased (:size 3 :scores (("cache-alpha" :key-properties nil :updates ((fundamental-mode . 1) (:global . 1))) ("cache-beta" :key-properties nil :updates ((fundamental-mode . 2) (:global . 2)))) :log [("cache-beta" (:global . 2) (fundamental-mode . 2)) ("cache-beta" (:global . 1) (fundamental-mode . 1)) ("cache-alpha" (:global . 1) (fundamental-mode . 1))] :index 0 :alias (:global t :mode t)) :after-wrap (:before "ca" :keys "C-c c C-g" :setup-calls nil :events ((:started :manual t :prefix "ca" :candidates ("cache-alpha" "cache-界" "cache-beta") :selection 0 :subject-active t :context nil)) :calls ((prefix nil) (candidates "ca") (sorted nil) (duplicates nil) (ignore-case nil) (expand-common "ca") (candidates "ca") (adjust-boundaries "cache-alpha") (ignore-case nil) (ignore-case nil) (ignore-case nil) (no-cache "ca") (prefix nil) (ignore-case nil) (ignore-case nil)) :backend nil :after "cache-" :point 7 :modified t :active nil :tooltip nil :runtime (:timer nil :tooltip-timer nil :echo-timer nil :cache-count 0) :unread nil) :wrapped (:size 3 :scores (("cache-alpha" :key-properties nil :updates ((fundamental-mode . 1) (:global . 1))) ("cache-界" :key-properties nil :updates ((fundamental-mode . 1) (:global . 1)))) :log [("cache-界" (:global . 1) (fundamental-mode . 1)) ("cache-beta" (:global . 1) (fundamental-mode . 1)) ("cache-alpha" (:global . 1) (fundamental-mode . 1))] :index 1 :alias nil) :shrunk (:order (:before "ca" :keys "C-c c C-g" :setup-calls nil :events ((:started :manual t :prefix "ca" :candidates ("cache-alpha" "cache-beta" "cache-界") :selection 0 :subject-active t :context nil)) :calls ((prefix nil) (candidates "ca") (sorted nil) (duplicates nil) (ignore-case nil) (expand-common "ca") (candidates "ca") (adjust-boundaries "cache-alpha") (ignore-case nil) (ignore-case nil) (ignore-case nil) (no-cache "ca") (prefix nil) (ignore-case nil) (ignore-case nil)) :backend nil :after "cache-" :point 7 :modified t :active nil :tooltip nil :runtime (:timer nil :tooltip-timer nil :echo-timer nil :cache-count 0) :unread nil) :ledger (:size 2 :scores (("cache-alpha" :key-properties nil :updates ((fundamental-mode . 1) (:global . 1)))) :log [("cache-alpha" (:global . 1) (fundamental-mode . 1)) ("cache-界" (:global . 1) (fundamental-mode . 1))] :index 0 :alias nil)) :enlarged (:order (:before "ca" :keys "C-c c C-g" :setup-calls nil :events ((:started :manual t :prefix "ca" :candidates ("cache-alpha" "cache-beta" "cache-界") :selection 0 :subject-active t :context nil)) :calls ((prefix nil) (candidates "ca") (sorted nil) (duplicates nil) (ignore-case nil) (expand-common "ca") (candidates "ca") (adjust-boundaries "cache-alpha") (ignore-case nil) (ignore-case nil) (ignore-case nil) (no-cache "ca") (prefix nil) (ignore-case nil) (ignore-case nil)) :backend nil :after "cache-" :point 7 :modified t :active nil :tooltip nil :runtime (:timer nil :tooltip-timer nil :echo-timer nil :cache-count 0) :unread nil) :ledger (:size 5 :scores (("cache-alpha" :key-properties nil :updates ((fundamental-mode . 1) (:global . 1)))) :log [("cache-界" (:global . 1) (fundamental-mode . 1)) ("cache-alpha" (:global . 1) (fundamental-mode . 1)) nil nil nil] :index 2 :alias nil)) :post-enlarge (:choice (:before "ca" :keys "C-c c C-n RET" :setup-calls nil :events ((:started :manual t :prefix "ca" :candidates ("cache-alpha" "cache-beta" "cache-界") :selection 0 :subject-active t :context nil) (:finished "cache-beta" :result-properties (company-statistics361-source-index 1) :index 3) (:started :manual nil :prefix "cache-beta" :candidates ("cache-beta") :selection 0 :subject-active t :context nil)) :calls ((prefix nil) (candidates "ca") (sorted nil) (duplicates nil) (ignore-case nil) (expand-common "ca") (candidates "ca") (adjust-boundaries "cache-alpha") (ignore-case nil) (ignore-case nil) (ignore-case nil) (no-cache "ca") (prefix nil) (ignore-case nil) (ignore-case nil) (adjust-boundaries "cache-beta") (ignore-case nil) (ignore-case nil) (post-completion "cache-beta") (prefix nil) (ignore-case nil) (set-min-prefix 0) (candidates "cache-beta") (sorted nil) (duplicates nil) (ignore-case nil) (adjust-boundaries "cache-beta") (ignore-case nil)) :backend ((:post "cache-beta" :properties (company-statistics361-source-index 1))) :after "cache-beta" :point 11 :modified t :active nil :tooltip nil :runtime (:timer nil :tooltip-timer nil :echo-timer nil :cache-count 0) :unread nil) :order (:before "ca" :keys "C-c c C-g" :setup-calls nil :events ((:started :manual t :prefix "ca" :candidates ("cache-alpha" "cache-beta" "cache-界") :selection 0 :subject-active t :context nil)) :calls ((prefix nil) (candidates "ca") (sorted nil) (duplicates nil) (ignore-case nil) (expand-common "ca") (candidates "ca") (adjust-boundaries "cache-alpha") (ignore-case nil) (ignore-case nil) (ignore-case nil) (no-cache "ca") (prefix nil) (ignore-case nil) (ignore-case nil)) :backend nil :after "cache-" :point 7 :modified t :active nil :tooltip nil :runtime (:timer nil :tooltip-timer nil :echo-timer nil :cache-count 0) :unread nil) :ledger (:size 5 :scores (("cache-alpha" :key-properties nil :updates ((fundamental-mode . 1) (:global . 1))) ("cache-beta" :key-properties nil :updates ((fundamental-mode . 1) (:global . 1)))) :log [("cache-界" (:global . 1) (fundamental-mode . 1)) ("cache-alpha" (:global . 1) (fundamental-mode . 1)) ("cache-beta" (:global . 1) (fundamental-mode . 1)) nil nil] :index 3 :alias nil))) :cleanup clean)"#
        ]],
    )
}

fn exit_save_and_public_activation_restore_a_real_ranking() -> ParityBatchCase {
    ParityBatchCase::value(
        "exit_save_and_public_activation_restore_a_real_ranking",
        r####"
(company-statistics361-test-run
 "save-restore"
 (lambda (world)
   (let* ((history (plist-get world :history))
          (warning-buffer (get-buffer "*Warnings*"))
          (warning-before
           (if warning-buffer
               (with-current-buffer warning-buffer (point-max))
             1)))
     (company-statistics361-test-configure 4 'light t t history)
     (let* ((choice
             (company-statistics361-test-session
              '(:file "project α/state.txt" :mode fundamental-mode
                :parent "ledger" :steps 2 :finish t)))
            (before-save (company-statistics361-test-ledger))
            (absent (not (file-exists-p history)))
            (save (company-statistics361-test-condition
                   #'company-statistics361-test-exit-save))
            (saved (company-statistics361-test-read-history))
            (after-save (company-statistics361-test-ledger)))
       (company-statistics-mode -1)
       (company-statistics361-test-fixture-reset-store)
       (setq company-statistics-size 6
             company-statistics-auto-restore t)
       (let* ((restore
               (company-statistics361-test-condition
                (lambda () (company-statistics-mode 1))))
              (warning
               (company-statistics361-test-warning-delta warning-before))
              (restored
               (company-statistics361-test-ledger "cache-界" 1))
              (restored-order
               (company-statistics361-test-session
                '(:file "project α/state.txt" :mode fundamental-mode
                  :parent "ledger")))
              (continued
               (company-statistics361-test-session
                '(:file "project α/state.txt" :mode fundamental-mode
                  :parent "ledger" :steps 1 :finish t)))
              (continued-order
               (company-statistics361-test-session
                '(:file "project α/state.txt" :mode fundamental-mode
                  :parent "ledger")))
              (bytes-before-disabled-save
               (company-statistics361-test-read-history)))
         (setq company-statistics-auto-save nil)
         (let ((disabled-save
                (company-statistics361-test-condition
                 #'company-statistics361-test-exit-save))
               (bytes-after-disabled-save
                (company-statistics361-test-read-history)))
           (list :choice choice :absent absent
                 :save save :saved saved
                 :state-preserved (equal before-save after-save)
                 :restore restore :warning warning
                 :restored restored :restored-order restored-order
                 :continued continued :continued-order continued-order
                 :auto-save-disabled disabled-save
                 :bytes-unchanged
                 (equal bytes-before-disabled-save bytes-after-disabled-save)
                 :tree (company-statistics361-test-tree))))))))
"####,
        expect![[
            r#"OK (:result (:choice (:before "ledger.ca" :keys "C-c c C-n C-n RET" :setup-calls ((init nil)) :events ((:started :manual t :prefix "ca" :candidates ("cache-alpha" "cache-beta" "cache-界") :selection 0 :subject-active t :context ((:symbol "ledger") (:file "[ROOT]/project α/state.txt"))) (:finished "cache-界" :result-properties (company-statistics361-source-index 2) :index 1) (:started :manual nil :prefix "cache-界" :candidates ("cache-界") :selection 0 :subject-active t :context ((:symbol "ledger") (:file "[ROOT]/project α/state.txt")))) :calls ((prefix nil) (candidates "ca") (sorted nil) (duplicates nil) (ignore-case nil) (expand-common "ca") (candidates "ca") (adjust-boundaries "cache-alpha") (ignore-case nil) (ignore-case nil) (ignore-case nil) (no-cache "ca") (prefix nil) (ignore-case nil) (ignore-case nil) (adjust-boundaries "cache-界") (ignore-case nil) (ignore-case nil) (post-completion "cache-界") (prefix nil) (ignore-case nil) (set-min-prefix 0) (candidates "cache-界") (sorted nil) (duplicates nil) (ignore-case nil) (adjust-boundaries "cache-界") (ignore-case nil)) :backend ((:post "cache-界" :properties (company-statistics361-source-index 2))) :after "ledger.cache-界" :point 15 :modified t :active nil :tooltip nil :runtime (:timer nil :tooltip-timer nil :echo-timer nil :cache-count 0) :unread nil) :absent t :save (:value nil) :saved (:bytes 226 :sha256 "e1c39ae65e55aaf2ad75bc56aa6edfbf5797577bcbe3a24c2f6686335c08f437" :text "(setq company-statistics--scores #s(hash-table test equal data (\"cache-界\" ((fundamental-mode . 1) (nil . 1)))) company-statistics--log [(\"cache-界\" (nil . 1) (fundamental-mode . 1)) nil nil nil] company-statistics--index 1)") :state-preserved t :restore (:value t) :warning "Warning (files): Missing ‘lexical-binding’ cookie in \"[ROOT]/state/history.el\".\nYou can add one with ‘M-x elisp-enable-lexical-binding RET’.\nSee ‘(elisp)Selecting Lisp Dialect’ and ‘(elisp)Converting to Lexical Binding’\nfor more information.\n" :restored (:size 6 :scores (("cache-界" :key-properties nil :updates ((fundamental-mode . 1) (:global . 1)))) :log [nil ("cache-界" (:global . 1) (fundamental-mode . 1)) nil nil nil nil] :index 4 :alias (:global nil :mode nil)) :restored-order (:before "ledger.ca" :keys "C-c c C-g" :setup-calls nil :events ((:started :manual t :prefix "ca" :candidates ("cache-界" "cache-alpha" "cache-beta") :selection 0 :subject-active t :context ((:symbol "ledger") (:file "[ROOT]/project α/state.txt")))) :calls ((prefix nil) (candidates "ca") (sorted nil) (duplicates nil) (ignore-case nil) (expand-common "ca") (candidates "ca") (adjust-boundaries "cache-界") (ignore-case nil) (ignore-case nil) (ignore-case nil) (no-cache "ca") (prefix nil) (ignore-case nil) (ignore-case nil)) :backend nil :after "ledger.cache-" :point 14 :modified t :active nil :tooltip nil :runtime (:timer nil :tooltip-timer nil :echo-timer nil :cache-count 0) :unread nil) :continued (:before "ledger.ca" :keys "C-c c C-n RET" :setup-calls nil :events ((:started :manual t :prefix "ca" :candidates ("cache-界" "cache-alpha" "cache-beta") :selection 0 :subject-active t :context ((:symbol "ledger") (:file "[ROOT]/project α/state.txt"))) (:finished "cache-alpha" :result-properties (company-statistics361-source-index 0) :index 5) (:started :manual nil :prefix "cache-alpha" :candidates ("cache-alpha") :selection 0 :subject-active t :context ((:symbol "ledger") (:file "[ROOT]/project α/state.txt")))) :calls ((prefix nil) (candidates "ca") (sorted nil) (duplicates nil) (ignore-case nil) (expand-common "ca") (candidates "ca") (adjust-boundaries "cache-界") (ignore-case nil) (ignore-case nil) (ignore-case nil) (no-cache "ca") (prefix nil) (ignore-case nil) (ignore-case nil) (adjust-boundaries "cache-alpha") (ignore-case nil) (ignore-case nil) (post-completion "cache-alpha") (prefix nil) (ignore-case nil) (set-min-prefix 0) (candidates "cache-alpha") (sorted nil) (duplicates nil) (ignore-case nil) (adjust-boundaries "cache-alpha") (ignore-case nil)) :backend ((:post "cache-alpha" :properties (company-statistics361-source-index 0))) :after "ledger.cache-alpha" :point 19 :modified t :active nil :tooltip nil :runtime (:timer nil :tooltip-timer nil :echo-timer nil :cache-count 0) :unread nil) :continued-order (:before "ledger.ca" :keys "C-c c C-g" :setup-calls nil :events ((:started :manual t :prefix "ca" :candidates ("cache-alpha" "cache-界" "cache-beta") :selection 0 :subject-active t :context ((:symbol "ledger") (:file "[ROOT]/project α/state.txt")))) :calls ((prefix nil) (candidates "ca") (sorted nil) (duplicates nil) (ignore-case nil) (expand-common "ca") (candidates "ca") (adjust-boundaries "cache-alpha") (ignore-case nil) (ignore-case nil) (ignore-case nil) (no-cache "ca") (prefix nil) (ignore-case nil) (ignore-case nil)) :backend nil :after "ledger.cache-" :point 14 :modified t :active nil :tooltip nil :runtime (:timer nil :tooltip-timer nil :echo-timer nil :cache-count 0) :unread nil) :auto-save-disabled (:value nil) :bytes-unchanged t :tree ("project α/" "project α/state.txt" "state/" "state/history.el")) :cleanup clean)"#
        ]],
    )
}

fn missing_and_truncated_cache_fail_then_recover_in_process() -> ParityBatchCase {
    ParityBatchCase::value(
        "missing_and_truncated_cache_fail_then_recover_in_process",
        r####"
(company-statistics361-test-run
 "cache-failure"
 (lambda (world)
   (let ((missing (company-statistics361-test-path "missing/history.el"))
         (truncated (company-statistics361-test-path "truncated/history.el")))
     (company-statistics361-test-configure 3 'light nil t missing)
     (let ((missing-state (company-statistics361-test-mode-state))
           (missing-ledger (company-statistics361-test-ledger))
           (missing-order
            (company-statistics361-test-session
             '(:mode fundamental-mode))))
       (company-statistics-mode -1)
       (company-statistics361-test-fixture-reset-store)
       (company-statistics361-test-write
        "truncated/history.el"
        "(setq company-statistics--scores #s(hash-table")
       (setq company-statistics-file truncated
             company-statistics-auto-restore t)
       (setq company-statistics361-test-events nil
             company-statistics361-test-backend-events nil
             company-statistics361-test-backend-calls nil)
       (let* ((file-before (company-statistics361-test-read-history))
              (failure
               (company-statistics361-test-condition
                (lambda () (company-statistics-mode 1))))
              (failed-state (company-statistics361-test-mode-state))
              (failed-ledger (company-statistics361-test-ledger))
              (failed-company-events
               (nreverse company-statistics361-test-events))
              (failed-backend-events
               (nreverse company-statistics361-test-backend-events))
              (failed-backend-calls
               (nreverse company-statistics361-test-backend-calls))
              (file-after (company-statistics361-test-read-history)))
         (company-statistics-mode -1)
         (delete-file truncated)
         (company-statistics361-test-fixture-reset-store)
         (let* ((recovery
                 (company-statistics361-test-condition
                  (lambda () (company-statistics-mode 1))))
                (choice
                 (company-statistics361-test-session
                  '(:mode fundamental-mode :steps 1 :finish t)))
                (recovered-order
                 (company-statistics361-test-session
                  '(:mode fundamental-mode))))
           (list :missing (list :state missing-state :ledger missing-ledger
                                :order missing-order)
                 :truncated (list :condition failure :state failed-state
                                  :ledger failed-ledger
                                  :company-events failed-company-events
                                  :backend-events failed-backend-events
                                  :backend-calls failed-backend-calls
                                  :file-unchanged
                                  (equal file-before file-after))
                 :recovery recovery :choice choice
                 :recovered-order recovered-order
                 :recovered-ledger
                 (company-statistics361-test-ledger))))))))
"####,
        expect![[
            r#"OK (:result (:missing (:state (:mode t :transformers 1 :transformer-last t :started 1 :finished 1) :ledger (:size 3 :scores nil :log [nil nil nil] :index 0 :alias nil) :order (:before "ca" :keys "C-c c C-g" :setup-calls ((init nil)) :events ((:started :manual t :prefix "ca" :candidates ("cache-alpha" "cache-beta" "cache-界") :selection 0 :subject-active t :context nil)) :calls ((prefix nil) (candidates "ca") (sorted nil) (duplicates nil) (ignore-case nil) (expand-common "ca") (candidates "ca") (adjust-boundaries "cache-alpha") (ignore-case nil) (ignore-case nil) (ignore-case nil) (no-cache "ca") (prefix nil) (ignore-case nil) (ignore-case nil)) :backend nil :after "cache-" :point 7 :modified t :active nil :tooltip nil :runtime (:timer nil :tooltip-timer nil :echo-timer nil :cache-count 0) :unread nil)) :truncated (:condition (:signal end-of-file :data (:killed-buffer) :message "End of file during parsing: #<killed buffer>") :state (:mode t :transformers 0 :transformer-last nil :started 0 :finished 0) :ledger (:size 3 :scores nil :log nil :index nil :alias nil) :company-events nil :backend-events nil :backend-calls nil :file-unchanged t) :recovery (:value t) :choice (:before "ca" :keys "C-c c C-n RET" :setup-calls nil :events ((:started :manual t :prefix "ca" :candidates ("cache-alpha" "cache-beta" "cache-界") :selection 0 :subject-active t :context nil) (:finished "cache-beta" :result-properties (company-statistics361-source-index 1) :index 1) (:started :manual nil :prefix "cache-beta" :candidates ("cache-beta") :selection 0 :subject-active t :context nil)) :calls ((prefix nil) (candidates "ca") (sorted nil) (duplicates nil) (ignore-case nil) (expand-common "ca") (candidates "ca") (adjust-boundaries "cache-alpha") (ignore-case nil) (ignore-case nil) (ignore-case nil) (no-cache "ca") (prefix nil) (ignore-case nil) (ignore-case nil) (adjust-boundaries "cache-beta") (ignore-case nil) (ignore-case nil) (post-completion "cache-beta") (prefix nil) (ignore-case nil) (set-min-prefix 0) (candidates "cache-beta") (sorted nil) (duplicates nil) (ignore-case nil) (adjust-boundaries "cache-beta") (ignore-case nil)) :backend ((:post "cache-beta" :properties (company-statistics361-source-index 1))) :after "cache-beta" :point 11 :modified t :active nil :tooltip nil :runtime (:timer nil :tooltip-timer nil :echo-timer nil :cache-count 0) :unread nil) :recovered-order (:before "ca" :keys "C-c c C-g" :setup-calls nil :events ((:started :manual t :prefix "ca" :candidates ("cache-beta" "cache-alpha" "cache-界") :selection 0 :subject-active t :context nil)) :calls ((prefix nil) (candidates "ca") (sorted nil) (duplicates nil) (ignore-case nil) (expand-common "ca") (candidates "ca") (adjust-boundaries "cache-beta") (ignore-case nil) (ignore-case nil) (ignore-case nil) (no-cache "ca") (prefix nil) (ignore-case nil) (ignore-case nil)) :backend nil :after "cache-" :point 7 :modified t :active nil :tooltip nil :runtime (:timer nil :tooltip-timer nil :echo-timer nil :cache-count 0) :unread nil) :recovered-ledger (:size 3 :scores (("cache-beta" :key-properties nil :updates ((fundamental-mode . 1) (:global . 1)))) :log [("cache-beta" (:global . 1) (fundamental-mode . 1)) nil nil] :index 1 :alias nil)) :cleanup clean)"#
        ]],
    )
}

fn exit_save_propagates_owned_filesystem_failure_and_retries() -> ParityBatchCase {
    ParityBatchCase::value(
        "exit_save_propagates_owned_filesystem_failure_and_retries",
        r####"
(company-statistics361-test-run
 "save-failure"
 (lambda (world)
   (let ((history (plist-get world :history))
         (directory-target
          (company-statistics361-test-path "unwritable-directory")))
     (company-statistics361-test-configure 4 'light t t history)
     (let* ((choice
             (company-statistics361-test-session
              '(:file "project β/retry.txt" :mode fundamental-mode
                :parent "cache" :steps 2 :finish t)))
            (state-before (company-statistics361-test-ledger)))
       (make-directory directory-target)
       (setq company-statistics-file directory-target)
       (let* ((failure
               (company-statistics361-test-condition
                #'company-statistics361-test-exit-save))
              (state-after-failure (company-statistics361-test-ledger))
              (tree-after-failure (company-statistics361-test-tree)))
         (setq company-statistics-file history)
         (let* ((retry
                 (company-statistics361-test-condition
                  #'company-statistics361-test-exit-save))
                (saved (company-statistics361-test-read-history))
                (tree-after-retry (company-statistics361-test-tree)))
           (company-statistics-mode -1)
           (company-statistics361-test-fixture-reset-store)
           (setq company-statistics-auto-restore t)
           (let ((restore
                  (company-statistics361-test-condition
                   (lambda () (company-statistics-mode 1))))
                 (restored-order
                  (company-statistics361-test-session
                   '(:file "project β/retry.txt" :mode fundamental-mode
                     :parent "cache"))))
             (list :choice choice :failure failure
                   :state-unchanged (equal state-before state-after-failure)
                   :tree-after-failure tree-after-failure
                   :retry retry :saved saved
                   :tree-after-retry tree-after-retry
                   :restore restore :restored-order restored-order
                   :restored-ledger
                   (company-statistics361-test-ledger)))))))))
"####,
        expect![[
            r#"OK (:result (:choice (:before "cache.ca" :keys "C-c c C-n C-n RET" :setup-calls ((init nil)) :events ((:started :manual t :prefix "ca" :candidates ("cache-alpha" "cache-beta" "cache-界") :selection 0 :subject-active t :context ((:symbol "cache") (:file "[ROOT]/project β/retry.txt"))) (:finished "cache-界" :result-properties (company-statistics361-source-index 2) :index 1) (:started :manual nil :prefix "cache-界" :candidates ("cache-界") :selection 0 :subject-active t :context ((:symbol "cache") (:file "[ROOT]/project β/retry.txt")))) :calls ((prefix nil) (candidates "ca") (sorted nil) (duplicates nil) (ignore-case nil) (expand-common "ca") (candidates "ca") (adjust-boundaries "cache-alpha") (ignore-case nil) (ignore-case nil) (ignore-case nil) (no-cache "ca") (prefix nil) (ignore-case nil) (ignore-case nil) (adjust-boundaries "cache-界") (ignore-case nil) (ignore-case nil) (post-completion "cache-界") (prefix nil) (ignore-case nil) (set-min-prefix 0) (candidates "cache-界") (sorted nil) (duplicates nil) (ignore-case nil) (adjust-boundaries "cache-界") (ignore-case nil)) :backend ((:post "cache-界" :properties (company-statistics361-source-index 2))) :after "cache.cache-界" :point 14 :modified t :active nil :tooltip nil :runtime (:timer nil :tooltip-timer nil :echo-timer nil :cache-count 0) :unread nil) :failure (:signal file-error :data ("Opening output file" "Is a directory" "[ROOT]/unwritable-directory") :message "Opening output file: Is a directory, [ROOT]/unwritable-directory") :state-unchanged t :tree-after-failure ("project β/" "project β/retry.txt" "state/" "unwritable-directory/") :retry (:value nil) :saved (:bytes 226 :sha256 "e1c39ae65e55aaf2ad75bc56aa6edfbf5797577bcbe3a24c2f6686335c08f437" :text "(setq company-statistics--scores #s(hash-table test equal data (\"cache-界\" ((fundamental-mode . 1) (nil . 1)))) company-statistics--log [(\"cache-界\" (nil . 1) (fundamental-mode . 1)) nil nil nil] company-statistics--index 1)") :tree-after-retry ("project β/" "project β/retry.txt" "state/" "state/history.el" "unwritable-directory/") :restore (:value t) :restored-order (:before "cache.ca" :keys "C-c c C-g" :setup-calls nil :events ((:started :manual t :prefix "ca" :candidates ("cache-界" "cache-alpha" "cache-beta") :selection 0 :subject-active t :context ((:symbol "cache") (:file "[ROOT]/project β/retry.txt")))) :calls ((prefix nil) (candidates "ca") (sorted nil) (duplicates nil) (ignore-case nil) (expand-common "ca") (candidates "ca") (adjust-boundaries "cache-界") (ignore-case nil) (ignore-case nil) (ignore-case nil) (no-cache "ca") (prefix nil) (ignore-case nil) (ignore-case nil)) :backend nil :after "cache.cache-" :point 13 :modified t :active nil :tooltip nil :runtime (:timer nil :tooltip-timer nil :echo-timer nil :cache-count 0) :unread nil) :restored-ledger (:size 4 :scores (("cache-界" :key-properties nil :updates ((fundamental-mode . 1) (:global . 1)))) :log [nil nil nil ("cache-界" (:global . 1) (fundamental-mode . 1))] :index 0 :alias nil)) :cleanup clean)"#
        ]],
    )
}

pub(super) fn company_statistics_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        mode_lifecycle_wraps_real_company_without_duplicates(),
        heavy_context_ranks_real_choices_by_keyword_parent_file_and_mode(),
        light_ring_wrap_and_public_resize_surface_pinned_decay(),
        exit_save_and_public_activation_restore_a_real_ranking(),
        missing_and_truncated_cache_fail_then_recover_in_process(),
        exit_save_propagates_owned_filesystem_failure_and_retries(),
    ]
}
