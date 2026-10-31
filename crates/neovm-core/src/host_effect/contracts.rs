//! Dormant host-effect boundary contracts; this module activates no host service.
//!
//! A registered reference names a future control-owner record. It neither
//! registers roots nor retains heap backing, and offers no materialization API.
//! Production constructors remain absent until the owner/lifecycle integration
//! can supply those obligations. Family envelopes deliberately do not claim to
//! implement the individual DisplayHost methods or the projection dispatcher.

use std::marker::PhantomData;
use std::num::NonZeroU64;
use std::rc::Rc;
use std::time::Instant;

/// Evaluator-host confinement, distinct from process-main toolkit ownership.
///
/// There is deliberately no production constructor. An arbitrary Context,
/// publisher, host trait object, or numeric thread identity is not mint proof.
/// Future activation must supply the approved primary-owner provenance seam.
pub struct HostAffinity {
    _local: PhantomData<Rc<()>>,
}

impl std::fmt::Debug for HostAffinity {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HostAffinity")
            .finish_non_exhaustive()
    }
}

static_assertions::assert_not_impl_any!(HostAffinity: Send, Sync, Clone, Copy, Default);
static_assertions::assert_impl_all!(HostAffinity: std::fmt::Debug);

macro_rules! identity {
    ($($name:ident),+ $(,)?) => {$(
        #[doc = "Opaque correlation identity; construction is reserved to its owner."]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct $name(NonZeroU64);
    )+};
}

identity!(
    HostSessionId,
    HostRequestId,
    RootRecordId,
    OwnerContinuationId
);

mod sealed {
    use super::{HostRequestId, HostSessionId};

    /// Private-module identity shared by registered records and reply authority.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct RequestIdentity {
        pub(super) session: HostSessionId,
        pub(super) request: HostRequestId,
    }

    pub trait Record {}
    pub trait Plan {}
    pub trait Reply {
        fn identity(&self) -> RequestIdentity;
    }
}

use sealed::RequestIdentity;

/// Closed record kinds; downstream code cannot admit arbitrary payload types.
pub trait RegisteredRecord: sealed::Record + Send + Sync + 'static {}

/// An opaque reference to an already registered, session-owned root record.
///
/// The invariant lifetime prevents references from escaping a scoped fixture
/// or being reassociated with another session. This key is not root ownership:
/// the future control owner must retain roots/backing until owner acknowledgement.
pub struct RegisteredRootRef<'session, K: RegisteredRecord> {
    binding: RequestIdentity,
    record: RootRecordId,
    _kind: PhantomData<fn() -> K>,
    _session: PhantomData<fn(&'session ()) -> &'session ()>,
}

impl<K: RegisteredRecord> RegisteredRootRef<'_, K> {
    pub fn session(&self) -> HostSessionId {
        self.binding.session
    }

    pub fn request(&self) -> HostRequestId {
        self.binding.request
    }

    pub fn record(&self) -> RootRecordId {
        self.record
    }
}

/// Small closed native-owned vocabulary, without a generic Send escape hatch.
///
/// Method-specific units and validation belong to the later owned adapters;
/// these envelopes make no claim that arbitrary bytes implement a host method.
#[derive(Debug, PartialEq, Eq)]
pub enum OwnedNativeData {
    None,
    Text(String),
    Bytes(Vec<u8>),
    Truth(NativeTruth),
}

/// A native truth result has explicit meaning at the owned boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeTruth {
    False,
    True,
}

/// One absolute deadline survives every service/owner transport hop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadDeadline {
    Immediate,
    At(Instant),
    Unbounded,
}

/// Only the declared family plans implement this request/reply association.
pub trait HostPlan<'session>: sealed::Plan + Send + 'session {
    type Reply: HostReply<'session>;
}

/// Only closed reply families can inhabit reply ports and terminal results.
pub trait HostReply<'session>: sealed::Reply + Send + 'session {}

macro_rules! families {
    ($(($family:ident, $plan:ident, $reply:ident, $input:ident, $output:ident)),+ $(,)?) => {
        $(
            #[doc = "Registered owner-prepared argument record kind."]
            pub enum $input {}
            impl sealed::Record for $input {}
            impl RegisteredRecord for $input {}

            #[doc = "Registered owner-completed result record kind."]
            pub enum $output {}
            impl sealed::Record for $output {}
            impl RegisteredRecord for $output {}

            #[doc = "Dormant family envelope, not an implementation of its host methods."]
            pub struct $plan<'session> {
                native: OwnedNativeData,
                record: RegisteredRootRef<'session, $input>,
            }

            impl<'session> $plan<'session> {
                pub fn native(&self) -> &OwnedNativeData { &self.native }
                pub fn record(&self) -> &RegisteredRootRef<'session, $input> { &self.record }
            }

            impl sealed::Plan for $plan<'_> {}
            impl<'session> HostPlan<'session> for $plan<'session> {
                type Reply = $reply<'session>;
            }

            #[doc = "Owned native reply plus an opaque registered owner-result reference."]
            pub struct $reply<'session> {
                native: OwnedNativeData,
                record: RegisteredRootRef<'session, $output>,
            }

            impl<'session> $reply<'session> {
                pub fn native(&self) -> &OwnedNativeData { &self.native }
                pub fn record(&self) -> &RegisteredRootRef<'session, $output> { &self.record }
            }

            impl sealed::Reply for $reply<'_> {
                fn identity(&self) -> RequestIdentity { self.record.binding }
            }
            impl<'session> HostReply<'session> for $reply<'session> {}
        )+

        /// Closed effect-family vocabulary; individual native adapters are not activated.
        pub enum HostRequest<'session> {
            $($family(Submitted<'session, $plan<'session>>),)+
        }

        /// The response family is determined by the matching sealed request plan.
        pub enum HostResponse<'session> {
            $($family(TerminalResponse<'session, $reply<'session>>),)+
        }
    };
}

families!(
    (
        Message,
        MessagePlan,
        MessageReply,
        MessageArguments,
        MessageResult
    ),
    (
        Redisplay,
        RedisplayPlan,
        RedisplayReply,
        RedisplayArguments,
        RedisplayResult
    ),
    (
        GuiFrame,
        GuiFramePlan,
        GuiFrameReply,
        GuiFrameArguments,
        GuiFrameResult
    ),
    (
        DisplayQuery,
        DisplayQueryPlan,
        DisplayQueryReply,
        DisplayQueryArguments,
        DisplayQueryResult
    ),
    (Font, FontPlan, FontReply, FontArguments, FontResult),
    (Image, ImagePlan, ImageReply, ImageArguments, ImageResult),
    (
        Selection,
        SelectionPlan,
        SelectionReply,
        SelectionArguments,
        SelectionResult
    ),
    (Input, InputPlan, InputReply, InputArguments, InputResult),
    (
        Terminal,
        TerminalPlan,
        TerminalReply,
        TerminalArguments,
        TerminalResult
    ),
    (
        NativeResource,
        NativeResourcePlan,
        NativeResourceReply,
        NativeResourceArguments,
        NativeResourceResult
    ),
    (Menu, MenuPlan, MenuReply, MenuArguments, MenuResult),
    (Video, VideoPlan, VideoReply, VideoArguments, VideoResult),
    (
        WebKit,
        WebKitPlan,
        WebKitReply,
        WebKitArguments,
        WebKitResult
    ),
    (
        Shader,
        ShaderPlan,
        ShaderReply,
        ShaderArguments,
        ShaderResult
    ),
    (
        VisualConfig,
        VisualConfigPlan,
        VisualConfigReply,
        VisualConfigArguments,
        VisualConfigResult
    ),
    (
        Callback,
        CallbackPlan,
        CallbackReply,
        CallbackArguments,
        CallbackResult
    ),
);

/// Error payload identity; the symbol/data remain in the registered control record.
pub enum ConditionRecord {}
impl sealed::Record for ConditionRecord {}
impl RegisteredRecord for ConditionRecord {}

/// Target-local thread signal identity, not a cross-thread Flow or raw Value.
pub enum ThreadSignalRecord {}
impl sealed::Record for ThreadSignalRecord {}
impl RegisteredRecord for ThreadSignalRecord {}

pub struct RootedCondition<'session>(RegisteredRootRef<'session, ConditionRecord>);
pub struct RootedThreadSignal<'session>(RegisteredRootRef<'session, ThreadSignalRecord>);

impl<'session> RootedCondition<'session> {
    pub fn record(&self) -> &RegisteredRootRef<'session, ConditionRecord> {
        &self.0
    }
}

impl<'session> RootedThreadSignal<'session> {
    pub fn record(&self) -> &RegisteredRootRef<'session, ThreadSignalRecord> {
        &self.0
    }
}

/// Recoverable closure is distinct from process shutdown and native effect rollback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostClosure {
    Closed,
    Replaced,
}

pub enum HostResult<'session, R: HostReply<'session>> {
    Returned(R),
    LispError(RootedCondition<'session>),
    Interrupted(RootedThreadSignal<'session>),
    Closed(HostClosure),
}

impl<'session, R: HostReply<'session>> HostResult<'session, R> {
    fn binding(&self) -> Option<RequestIdentity> {
        match self {
            Self::Returned(reply) => Some(sealed::Reply::identity(reply)),
            Self::LispError(error) => Some(error.0.binding),
            Self::Interrupted(signal) => Some(signal.0.binding),
            Self::Closed(_) => None,
        }
    }
}

/// A published envelope cannot be constructed from arbitrary IDs or payloads.
pub struct Submitted<'session, P: HostPlan<'session>> {
    plan: P,
    port: ReplyPort<'session, P::Reply>,
}

impl<'session, P: HostPlan<'session>> Submitted<'session, P> {
    pub fn into_parts(self) -> (P, ReplyPort<'session, P::Reply>) {
        (self.plan, self.port)
    }
}

/// Linear terminal-reply authority, privately bound to one session/request.
///
/// No channel or Drop-side action is hidden here. Terminal settlement is not
/// owner acknowledgement, so consuming this port cannot retire root records.
#[must_use = "a reply authority must be settled by its service/lifecycle owner"]
pub struct ReplyPort<'session, R: HostReply<'session>> {
    binding: RequestIdentity,
    _reply: PhantomData<fn(R) -> R>,
    _session: PhantomData<fn(&'session ()) -> &'session ()>,
}

impl<'session, R: HostReply<'session>> ReplyPort<'session, R> {
    /// Validate erased transport correlation before accepting terminal settlement.
    /// A rejection retains both obligations so neither port nor payload is lost.
    #[cold]
    #[inline(never)]
    pub fn complete(
        self,
        result: HostResult<'session, R>,
    ) -> Result<TerminalResponse<'session, R>, RejectedReply<'session, R>> {
        if let Some(binding) = result.binding()
            && binding != self.binding
        {
            return Err(RejectedReply { port: self, result });
        }
        Ok(TerminalResponse {
            binding: self.binding,
            result,
        })
    }
}

/// A mismatched reply retains its original identity and the unconsumed port.
#[must_use = "recover both the reply authority and rejected payload"]
pub struct RejectedReply<'session, R: HostReply<'session>> {
    port: ReplyPort<'session, R>,
    result: HostResult<'session, R>,
}

impl<'session, R: HostReply<'session>> RejectedReply<'session, R> {
    pub fn into_parts(self) -> (ReplyPort<'session, R>, HostResult<'session, R>) {
        (self.port, self.result)
    }
}

impl<'session, R: HostReply<'session>> std::fmt::Debug for RejectedReply<'session, R> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RejectedReply")
            .finish_non_exhaustive()
    }
}

/// Owned terminal result; no conversion to a local Lisp value is exposed.
#[must_use = "terminal settlement must reach its owner acknowledgement boundary"]
pub struct TerminalResponse<'session, R: HostReply<'session>> {
    binding: RequestIdentity,
    result: HostResult<'session, R>,
}

impl<'session, R: HostReply<'session>> TerminalResponse<'session, R> {
    pub fn session(&self) -> HostSessionId {
        self.binding.session
    }

    pub fn request(&self) -> HostRequestId {
        self.binding.request
    }

    /// Inspecting/consuming this result does not mint owner acknowledgement.
    pub fn into_result(self) -> HostResult<'session, R> {
        self.result
    }
}

/// The lifecycle milestones remain distinct; no runtime transition is implemented here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionMilestone {
    ServiceTerminal,
    OwnerAcknowledged,
    RootsRetired,
}

static_assertions::assert_impl_all!(HostRequest<'static>: Send);
static_assertions::assert_impl_all!(HostResponse<'static>: Send);
static_assertions::assert_impl_all!(RegisteredRootRef<'static, MessageArguments>: Send, Sync);
static_assertions::assert_not_impl_any!(ReplyPort<'static, MessageReply<'static>>: Clone, Copy);
static_assertions::assert_not_impl_any!(Submitted<'static, MessagePlan<'static>>: Clone, Copy);

/// Isolated compiler-contract setup, never production owner provenance.
///
/// It lends affinity and invariantly branded synthetic record references only
/// inside the callback. No heap is installed, no roots are registered, and no
/// host/registry can be attached. The fixture cannot mint a production session.
#[cfg(feature = "contract-fixtures")]
#[doc(hidden)]
pub mod contract_fixture {
    use super::*;
    use std::cell::Cell;

    pub struct Fixture<'session> {
        affinity: &'session mut HostAffinity,
        session: HostSessionId,
        next_request: Cell<NonZeroU64>,
        _session: PhantomData<fn(&'session ()) -> &'session ()>,
    }

    pub fn with_fixture<R>(body: impl for<'session> FnOnce(Fixture<'session>) -> R) -> R {
        let mut affinity = HostAffinity {
            _local: PhantomData,
        };
        body(Fixture {
            affinity: &mut affinity,
            session: HostSessionId(NonZeroU64::MAX),
            next_request: Cell::new(NonZeroU64::MIN),
            _session: PhantomData,
        })
    }

    impl<'session> Fixture<'session> {
        pub fn affinity(&mut self) -> &mut HostAffinity {
            self.affinity
        }

        fn next_binding(&self) -> RequestIdentity {
            let request = self.next_request.get();
            self.next_request
                .set(request.checked_add(1).expect("finite contract fixture"));
            RequestIdentity {
                session: self.session,
                request: HostRequestId(request),
            }
        }

        fn record<K: RegisteredRecord>(
            &self,
            binding: RequestIdentity,
        ) -> RegisteredRootRef<'session, K> {
            RegisteredRootRef {
                binding,
                record: RootRecordId(binding.request.0),
                _kind: PhantomData,
                _session: PhantomData,
            }
        }

        pub fn message(
            &self,
            native: OwnedNativeData,
        ) -> Submitted<'session, MessagePlan<'session>> {
            let binding = self.next_binding();
            Submitted {
                plan: MessagePlan {
                    native,
                    record: self.record(binding),
                },
                port: ReplyPort {
                    binding,
                    _reply: PhantomData,
                    _session: PhantomData,
                },
            }
        }

        pub fn message_reply_for(
            &self,
            plan: &MessagePlan<'session>,
            native: OwnedNativeData,
        ) -> MessageReply<'session> {
            MessageReply {
                native,
                record: self.record(plan.record.binding),
            }
        }

        pub fn font(&self, native: OwnedNativeData) -> Submitted<'session, FontPlan<'session>> {
            let binding = self.next_binding();
            Submitted {
                plan: FontPlan {
                    native,
                    record: self.record(binding),
                },
                port: ReplyPort {
                    binding,
                    _reply: PhantomData,
                    _session: PhantomData,
                },
            }
        }

        pub fn font_reply_for(
            &self,
            plan: &FontPlan<'session>,
            native: OwnedNativeData,
        ) -> FontReply<'session> {
            FontReply {
                native,
                record: self.record(plan.record.binding),
            }
        }

        pub fn message_error_for(&self, plan: &MessagePlan<'session>) -> RootedCondition<'session> {
            RootedCondition(self.record(plan.record.binding))
        }

        pub fn message_signal_for(
            &self,
            plan: &MessagePlan<'session>,
        ) -> RootedThreadSignal<'session> {
            RootedThreadSignal(self.record(plan.record.binding))
        }
    }
}
