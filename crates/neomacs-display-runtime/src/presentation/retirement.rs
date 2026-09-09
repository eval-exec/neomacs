//! Detached native resources survive dispatch, but no longer receive input.

/// Resources enter in child-first order. Commit releases them in that same
/// order; neither Vec's implicit drop order nor caller field layout is relied on.
pub(crate) struct Retirements<T>(Vec<T>);

impl<T> Default for Retirements<T> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<T> Retirements<T> {
    pub fn push(&mut self, resource: T) {
        self.0.push(resource);
    }

    pub fn commit(&mut self) {
        for resource in self.0.drain(..) {
            drop(resource);
        }
    }
}

impl<T> Drop for Retirements<T> {
    fn drop(&mut self) {
        self.commit();
    }
}

#[cfg(test)]
#[path = "retirement_test.rs"]
mod tests;
