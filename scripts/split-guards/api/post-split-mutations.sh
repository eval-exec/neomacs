#!/usr/bin/env bash
# Execute only during the post-split test pass. No Rust/product code is run.
# Negative controls pass only when the unchanged canonical tests observe red.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")"

python3 -m unittest -v selftest.Controls.test_identity
python3 -m unittest -v \
  selftest.Controls.test_unaccepted_added_method_red \
  selftest.Controls.test_exception_cannot_change_previous_private_signature \
  selftest.Controls.test_wrong_stale_and_extra_exception_red \
  selftest.Controls.test_public_method_removal_rename_and_signature_red \
  selftest.Controls.test_public_trait_reexport_and_methods_are_visible \
  selftest.Controls.test_alias_removal_and_added_alias_red \
  selftest.Controls.test_missing_moved_definition_refuses \
  selftest.Controls.test_trait_required_default_change_red \
  selftest.Controls.test_opaque_dependency_needs_exact_paired_evidence \
  selftest.Controls.test_non_nominal_alias_requires_builtin_identity_and_detects_signature_change \
  selftest.Controls.test_owned_trait_impl_on_builtin_alias_is_not_hidden \
  selftest.Controls.test_borrowed_alias_inherent_methods_and_mutability \
  selftest.Controls.test_unknown_schema_refuses
python3 -m unittest -v selftest.Controls.test_identity
