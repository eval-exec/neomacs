// Include the production completion tokens AND the production consuming
// ThreadManager methods. This adapter supplies only their runtime neighbours;
// it never declares a substitute token, constructor or retirement signature.
#![allow(dead_code)]

use crate::tagged::value::TaggedValue;
use crate::threading::{RunState, ThreadId, ThreadIds, ThreadPayload, ThreadPhase};
use std::collections::HashMap;

/// Compile-only manager neighbours. The maps retain real heap words and real
/// transport leases; issuer/payload/phase types are the included production
/// definitions. No instance is constructed or executed by these clients.
/// Lookup/publication require the real World's authority and are deliberately
/// unreachable here, rather than fabricating a payload or a successful permit.
pub(crate) struct ThreadManager {
    ids: ThreadIds,
    threads: HashMap<ThreadId, TaggedValue>,
}

impl ThreadManager {
    pub(crate) fn get_thread(&self, _: u64) -> Option<&ThreadPayload> {
        unreachable!("compile-only World payload lookup")
    }

    pub(crate) fn mark_thread_exited(&mut self, _: u64) {
        unreachable!("compile-only World terminal publication")
    }
}

#[path = "../../../neovm-eval/src/emacs_core/runtime/threads/completion.rs"]
pub(crate) mod completion;
