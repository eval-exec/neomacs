FORMS ?= cases/core.forms
EXPECTED ?= cases/core.expected.tsv
NEOVM_OUT ?= cases/core.neovm.tsv
CASES ?= cases/core cases/fboundp-core cases/fboundp-dispatch cases/fboundp-macro-boundary cases/symbol-function-core cases/symbol-function-error-paths cases/indirect-function-core cases/indirect-function-alias-chains cases/condition-case-unwind-introspection cases/subr-introspection-core cases/macro-special-introspection-core cases/function-special-form-edges cases/autoload-macro-introspection cases/functionp-callable-matrix cases/funcall-apply-non-callable-matrix cases/special-form-edges cases/special-form-arity-errors cases/special-form-type-errors cases/def-special-form-errors cases/func-arity-fallback-macros cases/printing cases/prin1-to-string-noescape cases/chars cases/bindings cases/buffer-locals cases/buffer-local-value cases/defvar-local cases/setq-local cases/undo-basics cases/undo-buffer-arg cases/minibuffer-batch cases/input-batch-readers cases/default-directory cases/expand-file-name-default-directory cases/directory-file-name cases/directory-files cases/directory-files-and-attributes cases/file-attributes cases/file-ops-relative cases/fileio-relative cases/file-error-conditions cases/error-message-string-formatting cases/process-error-conditions cases/process-basics cases/file-name-concat cases/path-predicates cases/file-name-types cases/file-name-completion cases/substitute-in-file-name cases/features-sync cases/pcase cases/seq-position cases/seq-misc cases/seq-uniq cases/seq-slice cases/seq-more cases/seq-concat cases/seq-reverse cases/take cases/match-data cases/search-buffer cases/search-count cases/replace-match cases/load-require cases/require-recursive cases/load-flags cases/load-relative cases/load-history cases/load-file-name cases/load-path-nil cases/load-path-order cases/reader-hash cases/hash-basics cases/locate-file cases/thread-introspection cases/thread-handle-identity cases/thread-handle-printing cases/thread-mutex-error-payloads cases/condition-variable-mutex-ownership
NEOVM_ONLY_CASES ?= cases/load-suffixes cases/load-policy cases/precompile
ALL_NEOVM_CASES ?= $(CASES) $(NEOVM_ONLY_CASES)
INTROSPECTION_CASES ?= cases/fboundp-core cases/fboundp-dispatch cases/fboundp-macro-boundary cases/symbol-function-core cases/symbol-function-error-paths cases/indirect-function-core cases/indirect-function-alias-chains cases/condition-case-unwind-introspection cases/subr-introspection-core cases/macro-special-introspection-core cases/function-special-form-edges cases/autoload-macro-introspection cases/functionp-callable-matrix cases/funcall-apply-non-callable-matrix cases/special-form-edges cases/special-form-arity-errors cases/special-form-type-errors cases/def-special-form-errors cases/func-arity-fallback-macros cases/thread-introspection cases/thread-handle-identity cases/thread-handle-printing cases/thread-mutex-error-payloads cases/condition-variable-mutex-ownership
BENCH_SOURCE ?= cases/load-policy-fixtures/vm-policy-cache-probe.el
BENCH_ITERS ?= 100
ERT_ALLOWLIST ?= cases/ert-allowlist-smoke.txt
ERT_EXPECTED ?= cases/ert-allowlist-smoke.expected.tsv
ERT_LOAD_FILES ?= cases/ert-allowlist-fixtures/smoke-tests.el

.PHONY: oracle neovm record check compare check-neovm record-all check-all check-all-neovm check-all-neovm-only check-introspection-neovm ert-oracle record-ert-allowlist check-ert-allowlist bench-load-cache

oracle:
	./run-oracle.sh $(FORMS)

neovm:
	./run-neovm.sh $(FORMS)

record:
	./run-oracle.sh $(FORMS) > $(EXPECTED)
	@echo "recorded oracle baseline: $(EXPECTED)"

check:
	@set -e; \
	tmp_file="$$(mktemp)"; \
	./run-oracle.sh $(FORMS) > "$$tmp_file"; \
	diff -u $(EXPECTED) "$$tmp_file"; \
	rm -f "$$tmp_file"; \
	echo "oracle output matches expected baseline"

compare:
	./compare-results.sh $(EXPECTED) $(NEOVM_OUT)

check-neovm:
	@set -e; \
	tmp_file="$$(mktemp)"; \
	./run-neovm.sh $(FORMS) > "$$tmp_file"; \
	STRICT_FORM=1 ./compare-results.sh $(EXPECTED) "$$tmp_file"; \
	rm -f "$$tmp_file"; \
	echo "neovm output matches oracle baseline"

record-all:
	@set -e; \
	for case in $(CASES); do \
		echo "== $$case =="; \
		$(MAKE) --no-print-directory record FORMS="$$case.forms" EXPECTED="$$case.expected.tsv"; \
	done

check-all:
	@set -e; \
	for case in $(CASES); do \
		echo "== $$case =="; \
		$(MAKE) --no-print-directory check FORMS="$$case.forms" EXPECTED="$$case.expected.tsv"; \
	done

check-all-neovm:
	@set -e; \
	for case in $(ALL_NEOVM_CASES); do \
		echo "== $$case =="; \
		$(MAKE) --no-print-directory check-neovm FORMS="$$case.forms" EXPECTED="$$case.expected.tsv"; \
	done

check-all-neovm-only:
	@set -e; \
	for case in $(NEOVM_ONLY_CASES); do \
		echo "== $$case =="; \
		$(MAKE) --no-print-directory check-neovm FORMS="$$case.forms" EXPECTED="$$case.expected.tsv"; \
	done

check-introspection-neovm:
	@set -e; \
	for case in $(INTROSPECTION_CASES); do \
		echo "== $$case =="; \
		$(MAKE) --no-print-directory check-neovm FORMS="$$case.forms" EXPECTED="$$case.expected.tsv"; \
	done

ert-oracle:
	./run-ert-allowlist.sh $(ERT_ALLOWLIST) $(ERT_LOAD_FILES)

record-ert-allowlist:
	./run-ert-allowlist.sh $(ERT_ALLOWLIST) $(ERT_LOAD_FILES) > $(ERT_EXPECTED)
	@echo "recorded ERT allowlist baseline: $(ERT_EXPECTED)"

check-ert-allowlist:
	@set -e; \
	tmp_file="$$(mktemp)"; \
	./run-ert-allowlist.sh $(ERT_ALLOWLIST) $(ERT_LOAD_FILES) > "$$tmp_file"; \
	diff -u $(ERT_EXPECTED) "$$tmp_file"; \
	rm -f "$$tmp_file"; \
	echo "ERT allowlist output matches expected baseline"

bench-load-cache:
	./bench-load-cache.sh $(BENCH_SOURCE) $(BENCH_ITERS)
