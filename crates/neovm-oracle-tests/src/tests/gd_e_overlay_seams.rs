//! GNU 31.1 overlay semantics, refreshed with UPDATE_EXPECT=1.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[cfg(test)]
#[path = "gd_e_overlay_seams/cycle_and_property_filtering.rs"]
mod cycle_and_property_filtering;

#[cfg(test)]
#[path = "gd_e_overlay_seams/priority_components_and_range_rules.rs"]
mod priority_components_and_range_rules;

#[cfg(test)]
#[path = "gd_e_overlay_seams/category_priority_alias_and_direct_nil.rs"]
mod category_priority_alias_and_direct_nil;

#[cfg(test)]
#[path = "gd_e_overlay_seams/multibyte_unibyte_raw_byte_positions.rs"]
mod multibyte_unibyte_raw_byte_positions;

#[cfg(test)]
#[path = "gd_e_overlay_seams/missing_overlay_property_falls_back_to_text.rs"]
mod missing_overlay_property_falls_back_to_text;

#[cfg(test)]
#[path = "gd_e_overlay_seams/nontransitive_sorted_queries.rs"]
mod nontransitive_sorted_queries;

#[cfg(test)]
#[path = "gd_e_overlay_seams/collapsed_start_query_order.rs"]
mod collapsed_start_query_order;

#[cfg(test)]
#[path = "gd_e_overlay_seams/end_default_properties.rs"]
mod end_default_properties;

#[cfg(test)]
#[path = "gd_e_overlay_seams/nil_priority_identity_invariants.rs"]
mod nil_priority_identity_invariants;

#[cfg(test)]
#[path = "gd_e_overlay_seams/compiled_property_queries.rs"]
mod compiled_property_queries;

#[cfg(test)]
#[path = "gd_e_overlay_seams/sorted_category_and_window.rs"]
mod sorted_category_and_window;

#[cfg(test)]
#[path = "gd_e_overlay_seams/default_local_map_at_end.rs"]
mod default_local_map_at_end;
