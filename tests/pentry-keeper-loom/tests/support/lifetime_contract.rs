//! Source-only serial-loan model. This is not a production admission protocol.
//!
//! The mutable World borrow stays inside `with_world`. The callback receives
//! only a generative serial context and witness; a view borrows that scope.
//! Nested calls reuse its witness rather than reacquire a second World loan.

use std::marker::PhantomData;
use std::rc::Rc;

type Invariant<'scope> = PhantomData<fn(&'scope mut ()) -> &'scope mut ()>;
type ThreadConfined = PhantomData<Rc<()>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelWord(u32);

impl ModelWord {
    pub const INITIAL: Self = Self(17);
}

pub struct ModelWorld {
    word: ModelWord,
}

impl ModelWorld {
    pub fn new(word: ModelWord) -> Self {
        Self { word }
    }
}

/// Its private construction retains the unique incoming World borrow.
pub struct WorldBatonGuard<'stop> {
    world: &'stop ModelWorld,
    _confined: ThreadConfined,
}

pub struct BatonWitness<'g> {
    _guard: Invariant<'g>,
    _confined: ThreadConfined,
}

pub struct SerialContext<'g> {
    world: &'g ModelWorld,
    _guard: Invariant<'g>,
    _confined: ThreadConfined,
}

pub struct View<'g> {
    world: &'g ModelWorld,
    _guard: Invariant<'g>,
    _confined: ThreadConfined,
}

impl View<'_> {
    pub fn word(&self) -> ModelWord {
        self.world.word
    }
}

impl<'g> SerialContext<'g> {
    pub fn view(&self) -> View<'g> {
        View {
            world: self.world,
            _guard: PhantomData,
            _confined: PhantomData,
        }
    }

    pub fn with_nested<R>(
        &mut self,
        body: impl FnOnce(&mut Self) -> R,
    ) -> R {
        body(self)
    }
}

pub fn with_world<'stop, R>(
    world: &'stop mut ModelWorld,
    body: impl for<'g> FnOnce(&mut SerialContext<'g>, &BatonWitness<'g>) -> R,
) -> R {
    let guard = WorldBatonGuard {
        world,
        _confined: PhantomData,
    };
    let mut context = SerialContext {
        world: guard.world,
        _guard: PhantomData,
        _confined: PhantomData,
    };
    let witness = BatonWitness {
        _guard: PhantomData,
        _confined: PhantomData,
    };
    body(&mut context, &witness)
}

static_assertions::assert_not_impl_any!(WorldBatonGuard<'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(BatonWitness<'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(SerialContext<'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(View<'static>: Send, Sync, Clone, Copy);
