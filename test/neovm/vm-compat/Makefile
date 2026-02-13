FORMS ?= cases/core.forms
EXPECTED ?= cases/core.expected.tsv
NEOVM_OUT ?= cases/core.neovm.tsv
CASES ?= cases/core cases/bindings cases/default-directory cases/expand-file-name-default-directory cases/directory-file-name cases/directory-files cases/directory-files-and-attributes cases/file-attributes cases/file-ops-relative cases/fileio-relative cases/file-error-conditions cases/process-error-conditions cases/file-name-concat cases/path-predicates cases/file-name-types cases/file-name-completion cases/substitute-in-file-name cases/features-sync cases/pcase cases/seq-position cases/seq-misc cases/seq-uniq cases/seq-slice cases/seq-more cases/seq-concat cases/seq-reverse cases/take cases/match-data cases/search-buffer cases/search-count cases/replace-match cases/load-require cases/require-recursive cases/load-flags cases/load-relative cases/load-history cases/load-file-name cases/load-path-nil cases/load-path-order cases/reader-hash cases/locate-file
NEOVM_ONLY_CASES ?= cases/load-suffixes cases/load-policy cases/precompile
ALL_NEOVM_CASES ?= $(CASES) $(NEOVM_ONLY_CASES)
BENCH_SOURCE ?= cases/load-policy-fixtures/vm-policy-cache-probe.el
BENCH_ITERS ?= 100

.PHONY: oracle neovm record check compare check-neovm record-all check-all check-all-neovm check-all-neovm-only bench-load-cache

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

bench-load-cache:
	./bench-load-cache.sh $(BENCH_SOURCE) $(BENCH_ITERS)
