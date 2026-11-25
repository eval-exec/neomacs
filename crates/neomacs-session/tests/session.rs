//! Session-layer contracts (DESIGN §6.3-§6.6, §6.12, §6.18-§6.19; MILESTONES M2).
//!
//! Two real endpoints talk to each other through in-memory transports, so a
//! property of one side is always checked against the other side's decoder.

use std::num::NonZeroU64;
use std::time::Duration;

use neomacs_session::handshake::{
    BuildId, BuildPolicy, CloseReason, DisplayLabel, Durability, FrontendIncarnation, LabelState,
    Limits, MinorRange, OfferMode, ResumeCursors, ResumeOutcome, ServiceName, ServiceOffer,
    SessionHello, SessionId, SessionIntent, SessionMessage, SessionRefusal, WireSchemaId,
};
use neomacs_session::lane::{Lane, LaneCounts, MessageIndex, chunk_size_for_rate};
use neomacs_session::preface::{PREFACE_LEN, Preface, PrefaceError, REFUSAL_LINE, Role};
use neomacs_session::pump::{EvaluatorLink, Pump, StepOutcome};
use neomacs_session::record::{
    Compression, Continuation, HEADER_LEN, PRE_WELCOME_MAX_RECORD, ProtocolError, RecordHeader,
    RecordReader, ServiceSlot,
};
use neomacs_session::relay::{
    MAX_RELAY_RECORD, ProbeReply, RelayError, RelayFile, RelayRefusal, RelayRequest, RelayStatus,
    ServerName,
};
use neomacs_session::session::{
    Attachment, CloseOutcome, ConnectIntent, EvaluatorConfig, EvaluatorEndpoint, EvaluatorEvent,
    FrontendConfig, FrontendEndpoint, FrontendEvent, LinkId, LinkLocality, SessionState,
};
use neomacs_session::testing::{
    CutAfterRecords, EofAfter, FakeClock, HalfClose, MemoryEnd, OneByteIo, RandomSplit, SimLink,
    Stall, memory_pipe, service_record,
};
use neomacs_session::{Clock, SessionTime};

const T0: SessionTime = SessionTime::ZERO;

fn schema(byte: u8) -> WireSchemaId {
    WireSchemaId::from_bytes([byte; 16])
}

fn build(commit: &str, schema_byte: u8) -> BuildId {
    BuildId::new("0.0.21", commit, schema(schema_byte))
}

fn display(major: u16, min: u16, max: u16) -> ServiceOffer {
    ServiceOffer::new(
        ServiceName::new("display").unwrap(),
        major,
        MinorRange::new(min, max).unwrap(),
    )
}

fn label(text: &str) -> DisplayLabel {
    DisplayLabel::new(text).unwrap()
}

fn evaluator_with(
    build: BuildId,
    policy: BuildPolicy,
    services: Vec<ServiceOffer>,
) -> EvaluatorEndpoint {
    let mut config = EvaluatorConfig::new(build);
    config.policy = policy;
    config.services = services;
    EvaluatorEndpoint::new(config)
}

fn evaluator() -> EvaluatorEndpoint {
    evaluator_with(
        build("aaaa", 1),
        BuildPolicy::Schema,
        vec![display(1, 0, 0)],
    )
}

fn frontend_with(
    build: BuildId,
    incarnation: u64,
    services: Vec<ServiceOffer>,
) -> FrontendEndpoint {
    let mut config = FrontendConfig::new(build, FrontendIncarnation::from_raw(incarnation));
    config.services = services;
    FrontendEndpoint::new(config)
}

fn frontend(incarnation: u64) -> FrontendEndpoint {
    frontend_with(build("aaaa", 1), incarnation, vec![display(1, 0, 0)])
}

/// Move bytes both ways until neither side has anything to send.
fn shuttle(fe: &mut FrontendEndpoint, ev: &mut EvaluatorEndpoint, link: LinkId, now: SessionTime) {
    loop {
        let mut moved = false;
        while let Some(bytes) = fe.poll_transmit(now) {
            ev.receive(link, &bytes, now);
            moved = true;
        }
        while let Some(bytes) = ev.poll_transmit(link, now) {
            fe.receive(&bytes, now);
            moved = true;
        }
        if !moved {
            break;
        }
    }
}

fn frontend_events(fe: &mut FrontendEndpoint) -> Vec<FrontendEvent> {
    std::iter::from_fn(|| fe.poll_event()).collect()
}

fn evaluator_events(ev: &mut EvaluatorEndpoint) -> Vec<EvaluatorEvent> {
    std::iter::from_fn(|| ev.poll_event()).collect()
}

fn established(events: &[FrontendEvent]) -> &neomacs_session::handshake::SessionWelcome {
    events
        .iter()
        .find_map(|event| match event {
            FrontendEvent::Established { welcome, .. } => Some(&**welcome),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no Established event in {events:?}"))
}

fn refused(events: &[FrontendEvent]) -> SessionRefusal {
    events
        .iter()
        .find_map(|event| match event {
            FrontendEvent::Refused(refusal) => Some(refusal.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no Refused event in {events:?}"))
}

/// Connect a fresh frontend to `ev` with an offer for `label`.
fn connect(
    fe: &mut FrontendEndpoint,
    ev: &mut EvaluatorEndpoint,
    label_text: &str,
    now: SessionTime,
) -> (LinkId, neomacs_session::handshake::SessionWelcome) {
    let link = ev.accept(LinkLocality::Dialed, now);
    fe.connect(
        ConnectIntent::Offer {
            label: label(label_text),
            mode: OfferMode::Exclusive,
        },
        now,
    );
    shuttle(fe, ev, link, now);
    let events = frontend_events(fe);
    (link, established(&events).clone())
}

fn display_slot(welcome: &neomacs_session::handshake::SessionWelcome) -> ServiceSlot {
    welcome.accepted[0].slot
}

fn delivered_on(events: &[EvaluatorEvent], lane: Lane) -> Vec<Vec<u8>> {
    events
        .iter()
        .filter_map(|event| match event {
            EvaluatorEvent::Message { message, .. } if message.lane == lane => {
                Some(message.body.clone())
            }
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Preface and records
// ---------------------------------------------------------------------------

#[test]
fn preface_rejects_wrong_magic_with_readable_line() {
    let server_el_reply = *b"-error Unknown c";
    let error = Preface::parse(&server_el_reply).unwrap_err();
    assert!(matches!(error, PrefaceError::BadMagic { .. }), "{error:?}");
    assert!(error.to_string().contains("-error Unknown c"), "{error}");

    let good = Preface::current(Role::Frontend).to_bytes();
    assert_eq!(good.len(), PREFACE_LEN);
    assert_eq!(Preface::parse(&good), Ok(Preface::current(Role::Frontend)));
    let mut flagged = good;
    flagged[13] = 1;
    assert_eq!(Preface::parse(&flagged), Err(PrefaceError::NonZeroFlags(1)));

    // An evaluator link answers a non-session peer with one human-readable
    // line, and nothing else: it has not sent its own preface yet.
    let mut ev = evaluator();
    let link = ev.accept(LinkLocality::Dialed, T0);
    ev.receive(link, b"-error Unknown command\n", T0);
    let written: Vec<u8> = std::iter::from_fn(|| ev.poll_transmit(link, T0))
        .flatten()
        .collect();
    assert_eq!(written, REFUSAL_LINE.as_bytes());
    assert!(REFUSAL_LINE.is_ascii() && REFUSAL_LINE.ends_with('\n'));
    let events = evaluator_events(&mut ev);
    assert!(
        events.iter().any(|event| matches!(
            event,
            EvaluatorEvent::LinkClosed {
                outcome: CloseOutcome::BadPreface(PrefaceError::BadMagic { .. }),
                ..
            }
        )),
        "{events:?}"
    );

    // A frontend that reached something else reports what it read.
    let mut fe = frontend(1);
    fe.connect(ConnectIntent::Inherited, T0);
    fe.receive(b"-error Unknown command\n", T0);
    let events = frontend_events(&mut fe);
    assert!(
        events.iter().any(|event| matches!(
            event,
            FrontendEvent::Closed(CloseOutcome::BadPreface(PrefaceError::BadMagic { .. }))
        )),
        "{events:?}"
    );
}

#[test]
fn oversized_record_rejected_before_allocation() {
    let mut reader = RecordReader::new(PRE_WELCOME_MAX_RECORD);
    let header = RecordHeader {
        len: u32::MAX,
        lane: Lane::Bulk,
        compression: Compression::Raw,
        continuation: Continuation::Last,
        service: ServiceSlot::SESSION,
    };
    reader.push(&header.to_bytes());
    assert_eq!(
        reader.next_record(),
        Err(ProtocolError::RecordTooLarge {
            len: u32::MAX,
            limit: PRE_WELCOME_MAX_RECORD,
        })
    );
    assert!(
        reader.buffered_capacity() <= 4 * HEADER_LEN,
        "the reader grew to {} bytes for a header",
        reader.buffered_capacity()
    );

    // Unknown lanes and reserved bits are typed errors, not panics.
    let mut bytes = header.to_bytes();
    bytes[0..4].copy_from_slice(&4u32.to_le_bytes());
    bytes[4] = 9;
    assert_eq!(
        RecordHeader::parse(&bytes),
        Err(ProtocolError::UnknownLane(9))
    );
    bytes[4] = 0;
    bytes[5] = 0b0000_0100;
    assert_eq!(
        RecordHeader::parse(&bytes),
        Err(ProtocolError::ReservedFlags(0b0000_0100))
    );

    // A Hello above the pre-Welcome cap closes the link with a typed error.
    let mut ev = evaluator();
    let link = ev.accept(LinkLocality::Dialed, T0);
    ev.receive(link, &Preface::current(Role::Frontend).to_bytes(), T0);
    let big = RecordHeader {
        len: PRE_WELCOME_MAX_RECORD + 1,
        lane: Lane::Control,
        compression: Compression::Raw,
        continuation: Continuation::Last,
        service: ServiceSlot::SESSION,
    };
    ev.receive(link, &big.to_bytes(), T0);
    let events = evaluator_events(&mut ev);
    assert!(
        events.iter().any(|event| matches!(
            event,
            EvaluatorEvent::LinkClosed {
                outcome: CloseOutcome::Protocol(ProtocolError::RecordTooLarge { .. }),
                ..
            }
        )),
        "{events:?}"
    );
}

#[test]
fn chunk_size_follows_the_link_rate() {
    assert_eq!(chunk_size_for_rate(1_000_000), 5_000);
    assert_eq!(chunk_size_for_rate(10), 1024);
    assert_eq!(chunk_size_for_rate(1_000_000_000), 64 * 1024);
}

// ---------------------------------------------------------------------------
// Coalesced reads: records apply in arrival order, one at a time
// ---------------------------------------------------------------------------

/// The display slot the evaluator assigns: `negotiate` numbers accepted
/// services from 1 in the evaluator's order, and it speaks only display.
const DISPLAY_SLOT: ServiceSlot = ServiceSlot::new(1);

/// Have `ev` admit an offer from `fe` and return the attached session. The
/// evaluator's preface and Welcome stay unread by the frontend.
fn admit_offer(
    fe: &mut FrontendEndpoint,
    ev: &mut EvaluatorEndpoint,
    now: SessionTime,
) -> (LinkId, SessionId) {
    let link = ev.accept(LinkLocality::Dialed, now);
    fe.connect(
        ConnectIntent::Offer {
            label: label("laptop"),
            mode: OfferMode::Exclusive,
        },
        now,
    );
    while let Some(bytes) = fe.poll_transmit(now) {
        ev.receive(link, &bytes, now);
    }
    let events = evaluator_events(ev);
    let session = events
        .iter()
        .find_map(|event| match event {
            EvaluatorEvent::Attached { session, .. } => Some(*session),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no Attached event in {events:?}"));
    (link, session)
}

fn messages_on(events: &[FrontendEvent], lane: Lane) -> Vec<Vec<u8>> {
    events
        .iter()
        .filter_map(|event| match event {
            FrontendEvent::Message(message) if message.lane == lane => Some(message.body.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_message_sent_on_attach_arrives_in_the_welcomes_read() {
    let mut ev = evaluator();
    let mut fe = frontend(1);
    let (link, session) = admit_offer(&mut fe, &mut ev, T0);
    // A display host sends its keyframe as soon as the session attaches
    // (DESIGN §6.10, "Keyframes are sent on attach").
    let keyframe = vec![7u8; 4096];
    ev.send(session, Lane::Present, DISPLAY_SLOT, keyframe.clone())
        .unwrap();
    // A stream transport returns the preface, the Welcome and the keyframe
    // to one read.
    let coalesced: Vec<u8> = std::iter::from_fn(|| ev.poll_transmit(link, T0))
        .flatten()
        .collect();
    fe.receive(&coalesced, T0);
    let events = frontend_events(&mut fe);
    assert_eq!(display_slot(established(&events)), DISPLAY_SLOT);
    assert_eq!(messages_on(&events, Lane::Present), vec![keyframe]);
    assert_eq!(fe.closed(), None, "{events:?}");
}

/// M2 review r1 (correctness m4): the frontend applied a Welcome's limits
/// as sent. A Welcome carries the minimum of both offers, so limits above
/// what the frontend offered are a protocol violation, not a new bound.
#[test]
fn a_welcome_with_limits_beyond_the_offer_is_a_typed_protocol_error() {
    let mut ev = evaluator();
    let mut honest = frontend(1);
    let (_, welcome) = connect(&mut honest, &mut ev, "laptop", T0);
    assert_eq!(welcome.limits, Limits::default().agree(Limits::default()));
    let offered = Limits::default();
    for forged in [
        Limits {
            max_record: offered.max_record * 4,
            ..offered
        },
        Limits {
            store_rows: offered.store_rows + 1,
            ..offered
        },
    ] {
        let mut fe = frontend(2);
        fe.connect(ConnectIntent::Inherited, T0);
        while fe.poll_transmit(T0).is_some() {}
        let mut lying = welcome.clone();
        lying.limits = forged;
        let mut bytes = Preface::current(Role::Evaluator).to_bytes().to_vec();
        bytes.extend(SessionMessage::Welcome(lying).to_record().unwrap());
        fe.receive(&bytes, T0);
        let events = frontend_events(&mut fe);
        assert_eq!(
            events,
            vec![FrontendEvent::Closed(CloseOutcome::Protocol(
                ProtocolError::LimitsBeyondOffer {
                    offered,
                    welcome: forged,
                }
            ))]
        );
    }
}

#[test]
fn records_after_the_welcome_in_one_read_use_the_agreed_record_limit() {
    let mut ev = evaluator();
    let mut fe = frontend(1);
    let (link, _) = admit_offer(&mut fe, &mut ev, T0);
    let mut bytes: Vec<u8> = std::iter::from_fn(|| ev.poll_transmit(link, T0))
        .flatten()
        .collect();
    // Above the 64 KiB pre-Welcome limit, within the agreed 1 MiB.
    let body = vec![9u8; 100 * 1024];
    bytes.extend(service_record(Lane::Present, DISPLAY_SLOT, &body));
    fe.receive(&bytes, T0);
    let events = frontend_events(&mut fe);
    assert!(established(&events).limits.max_record >= 1024 * 1024);
    assert_eq!(messages_on(&events, Lane::Present), vec![body]);
    assert_eq!(fe.closed(), None, "{events:?}");
}

// ---------------------------------------------------------------------------
// Handshake
// ---------------------------------------------------------------------------

#[test]
fn handshake_matrix() {
    // Same build, same minor.
    let mut ev = evaluator();
    let mut fe = frontend(1);
    let (_, welcome) = connect(&mut fe, &mut ev, "laptop", T0);
    assert_eq!(welcome.accepted.len(), 1);
    assert_eq!(
        (welcome.accepted[0].major, welcome.accepted[0].minor),
        (1, 0)
    );
    assert_eq!(welcome.build, build("aaaa", 1));

    // A newer frontend minor meets an older evaluator: the lower one is agreed.
    let mut ev = evaluator_with(
        build("aaaa", 1),
        BuildPolicy::Schema,
        vec![display(1, 0, 1)],
    );
    let mut fe = frontend_with(build("aaaa", 1), 1, vec![display(1, 0, 3)]);
    let (_, welcome) = connect(&mut fe, &mut ev, "laptop", T0);
    assert_eq!(welcome.accepted[0].minor, 1);
    // And an older frontend meets a newer evaluator.
    let mut ev = evaluator_with(
        build("aaaa", 1),
        BuildPolicy::Schema,
        vec![display(1, 0, 2)],
    );
    let mut fe = frontend_with(build("aaaa", 1), 1, vec![display(1, 0, 0)]);
    let (_, welcome) = connect(&mut fe, &mut ev, "laptop", T0);
    assert_eq!(welcome.accepted[0].minor, 0);

    // A service major mismatch is refused with both majors.
    let mut ev = evaluator();
    let mut fe = frontend_with(build("aaaa", 1), 1, vec![display(2, 0, 0)]);
    let link = ev.accept(LinkLocality::Dialed, T0);
    fe.connect(ConnectIntent::Inherited, T0);
    shuttle(&mut fe, &mut ev, link, T0);
    assert_eq!(
        refused(&frontend_events(&mut fe)),
        SessionRefusal::ProtocolMajor { ours: 1, theirs: 2 }
    );

    // A session-preface major mismatch is refused the same way.
    let mut ev = evaluator();
    let link = ev.accept(LinkLocality::Dialed, T0);
    let mut newer = Preface::current(Role::Frontend);
    newer.major = 2;
    ev.receive(link, &newer.to_bytes(), T0);
    let bytes: Vec<u8> = std::iter::from_fn(|| ev.poll_transmit(link, T0))
        .flatten()
        .collect();
    let mut fe = frontend(1);
    fe.connect(ConnectIntent::Inherited, T0);
    while fe.poll_transmit(T0).is_some() {}
    fe.receive(&bytes, T0);
    assert_eq!(
        refused(&frontend_events(&mut fe)),
        SessionRefusal::ProtocolMajor { ours: 1, theirs: 2 }
    );

    // The default `schema` policy refuses another wire schema, naming both builds.
    let mut ev = evaluator();
    let mut fe = frontend_with(build("bbbb", 2), 1, vec![display(1, 0, 0)]);
    let link = ev.accept(LinkLocality::Dialed, T0);
    fe.connect(ConnectIntent::Inherited, T0);
    shuttle(&mut fe, &mut ev, link, T0);
    assert_eq!(
        refused(&frontend_events(&mut fe)),
        SessionRefusal::SchemaMismatch {
            ours: Box::new(build("aaaa", 1)),
            theirs: Box::new(build("bbbb", 2)),
        }
    );
    // ...but accepts another commit with the same schema.
    let mut ev = evaluator();
    let mut fe = frontend_with(build("cccc", 1), 1, vec![display(1, 0, 0)]);
    let (_, welcome) = connect(&mut fe, &mut ev, "laptop", T0);
    assert_eq!(welcome.build, build("aaaa", 1));

    // `exact-build` refuses another commit even with the same schema.
    let mut ev = evaluator_with(
        build("aaaa", 1),
        BuildPolicy::ExactBuild,
        vec![display(1, 0, 0)],
    );
    let mut fe = frontend_with(build("cccc", 1), 1, vec![display(1, 0, 0)]);
    let link = ev.accept(LinkLocality::Dialed, T0);
    fe.connect(ConnectIntent::Inherited, T0);
    shuttle(&mut fe, &mut ev, link, T0);
    assert_eq!(
        refused(&frontend_events(&mut fe)),
        SessionRefusal::BuildMismatch {
            ours: Box::new(build("aaaa", 1)),
            theirs: Box::new(build("cccc", 1)),
        }
    );

    // The `schema` policy intersects services: an unknown one is dropped.
    let workspace = ServiceOffer::new(
        ServiceName::new("workspace").unwrap(),
        1,
        MinorRange::new(0, 0).unwrap(),
    );
    let mut ev = evaluator_with(
        build("aaaa", 1),
        BuildPolicy::Schema,
        vec![display(1, 0, 1)],
    );
    let mut fe = frontend_with(
        build("aaaa", 1),
        1,
        vec![display(1, 0, 2), workspace.clone()],
    );
    let (_, welcome) = connect(&mut fe, &mut ev, "laptop", T0);
    assert_eq!(welcome.accepted.len(), 1);
    assert_eq!(welcome.accepted[0].name.as_str(), "display");
    assert_eq!(welcome.accepted[0].minor, 1);
    assert_ne!(welcome.accepted[0].slot, ServiceSlot::SESSION);
    // With nothing in common the evaluator names the service it offers.
    let mut ev = evaluator();
    let mut fe = frontend_with(build("aaaa", 1), 1, vec![workspace]);
    let link = ev.accept(LinkLocality::Dialed, T0);
    fe.connect(ConnectIntent::Inherited, T0);
    shuttle(&mut fe, &mut ev, link, T0);
    assert_eq!(
        refused(&frontend_events(&mut fe)),
        SessionRefusal::ServiceUnavailable {
            name: ServiceName::new("display").unwrap(),
        }
    );
}

fn every_refusal() -> Vec<SessionRefusal> {
    vec![
        SessionRefusal::ProtocolMajor { ours: 1, theirs: 7 },
        SessionRefusal::SchemaMismatch {
            ours: Box::new(build("aaaa", 1)),
            theirs: Box::new(build("bbbb", 2)),
        },
        SessionRefusal::BuildMismatch {
            ours: Box::new(build("aaaa", 1)),
            theirs: Box::new(build("bbbb", 1)),
        },
        SessionRefusal::ServiceUnavailable {
            name: ServiceName::new("display").unwrap(),
        },
        SessionRefusal::LabelInUse {
            label: label("laptop"),
            state: LabelState::Connected,
        },
        SessionRefusal::LabelInUse {
            label: label("laptop"),
            state: LabelState::Detached,
        },
        SessionRefusal::ResumeUnknown,
        SessionRefusal::ResumeExpired,
        SessionRefusal::OfferExpired,
        SessionRefusal::Unauthorized,
        SessionRefusal::ResourceLimit {
            kind: neomacs_session::handshake::LimitKind::Handshakes,
        },
    ]
}

#[test]
fn refusal_decodable_by_both_ends() {
    for refusal in every_refusal() {
        let record = SessionMessage::Refusal(refusal.clone())
            .to_record()
            .expect("a refusal fits one record");

        // The frontend reads it after its Hello.
        let mut fe = frontend(1);
        fe.connect(ConnectIntent::Inherited, T0);
        while fe.poll_transmit(T0).is_some() {}
        fe.receive(&Preface::current(Role::Evaluator).to_bytes(), T0);
        fe.receive(&record, T0);
        assert_eq!(refused(&frontend_events(&mut fe)), refusal);

        // The evaluator reads the same bytes when a frontend refuses it.
        let mut ev = evaluator();
        let link = ev.accept(LinkLocality::Dialed, T0);
        ev.receive(link, &Preface::current(Role::Frontend).to_bytes(), T0);
        ev.receive(link, &record, T0);
        let events = evaluator_events(&mut ev);
        assert!(
            events.iter().any(|event| matches!(
                event,
                EvaluatorEvent::LinkClosed { outcome: CloseOutcome::RefusedByPeer(got), .. }
                    if *got == refusal
            )),
            "{refusal:?}: {events:?}"
        );
    }
}

#[test]
fn frozen_records_golden_bytes() {
    let hex = |bytes: &[u8]| {
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    };
    expect_test::expect!["4e454f53455353000100000002000000"]
        .assert_eq(&hex(&Preface::current(Role::Frontend).to_bytes()));
    let refusals = every_refusal()
        .into_iter()
        .map(|refusal| {
            hex(&SessionMessage::Refusal(refusal)
                .to_record()
                .expect("a refusal fits one record"))
        })
        .collect::<Vec<_>>()
        .join("\n");
    expect_test::expect![[r#"
        040000000000000002000107
        3a00000000000000020106302e302e323104616161610101010101010101010101010101010106302e302e3231046262626202020202020202020202020202020202
        3a00000000000000020206302e302e323104616161610101010101010101010101010101010106302e302e3231046262626201010101010101010101010101010101
        0a00000000000000020307646973706c6179
        0a000000000000000204066c6170746f7000
        0a000000000000000204066c6170746f7001
        02000000000000000205
        02000000000000000206
        02000000000000000207
        02000000000000000208
        0300000000000000020900"#]]
    .assert_eq(&refusals);
}

// ---------------------------------------------------------------------------
// Lanes and scheduling
// ---------------------------------------------------------------------------

#[test]
fn priority_control_and_input_overtake_bulk_within_one_chunk() {
    const RATE: u64 = 1_000_000; // 1 MB/s
    let clock = FakeClock::new();
    let mut ev = evaluator();
    let mut fe = frontend(1);
    let (link, welcome) = connect(&mut fe, &mut ev, "laptop", clock.now());
    let slot = display_slot(&welcome);
    let session = welcome.session;
    evaluator_events(&mut ev);

    // 10 MB of bulk queued at once.
    for _ in 0..10 {
        ev.send(session, Lane::Bulk, slot, vec![0xb0; 1_000_000])
            .expect("bulk is admitted");
    }
    let mut down = SimLink::new(RATE);
    let mut enqueued_at_bulk_bytes = None;
    let mut chunk_at_enqueue = None;
    let mut control_at = None;
    let mut input_at = None;
    for millis in 1..=2_000u64 {
        clock.advance(Duration::from_millis(1));
        let now = clock.now();
        ev.tick(now);
        fe.tick(now);
        if millis == 300 {
            ev.send(session, Lane::Control, slot, b"control".to_vec())
                .unwrap();
            ev.send(session, Lane::Input, slot, b"input".to_vec())
                .unwrap();
            enqueued_at_bulk_bytes = Some(fe_bulk_bytes(&fe));
            chunk_at_enqueue = ev.chunk_size(session);
        }
        // The link accepts the next record only once it has drained, like a
        // full kernel buffer that becomes writable again.
        if down.is_writable()
            && let Some(bytes) = ev.poll_transmit(link, now)
        {
            down.write(&bytes);
        }
        for bytes in down.deliver(Duration::from_millis(1)) {
            fe.receive(&bytes, now);
        }
        // Acknowledgements travel back at once.
        while let Some(bytes) = fe.poll_transmit(now) {
            ev.receive(link, &bytes, now);
        }
        for event in frontend_events(&mut fe) {
            if let FrontendEvent::Message(message) = event {
                match message.lane {
                    Lane::Control => control_at = Some(fe_bulk_bytes(&fe)),
                    Lane::Input => input_at = Some(fe_bulk_bytes(&fe)),
                    _ => {}
                }
            }
        }
        if input_at.is_some() {
            break;
        }
    }
    let chunk = chunk_at_enqueue.expect("connected session");
    let before = enqueued_at_bulk_bytes.expect("enqueued");
    let control_after = control_at.expect("control arrived") - before;
    let input_after = input_at.expect("input arrived") - before;
    assert!(
        control_after <= chunk as u64,
        "control waited behind {control_after} bulk bytes (chunk {chunk})"
    );
    assert_eq!(input_after, control_after, "input follows control directly");
    // The chunk size follows the measured rate (1 ms steps quantize it).
    let expected = chunk_size_for_rate(RATE) as f64;
    assert!(
        (chunk as f64 - expected).abs() <= expected * 0.1,
        "chunk {chunk} for a measured 1 MB/s link"
    );
}

/// Bulk payload bytes the frontend has received so far, across partial chunks.
fn fe_bulk_bytes(fe: &FrontendEndpoint) -> u64 {
    fe.received_payload_bytes(Lane::Bulk)
}

// ---------------------------------------------------------------------------
// Liveness
// ---------------------------------------------------------------------------

#[test]
fn keepalive_timeout_with_fake_clock() {
    let clock = FakeClock::new();
    let mut ev = evaluator();
    let mut fe = frontend(1);
    let (link, welcome) = connect(&mut fe, &mut ev, "laptop", clock.now());
    assert_eq!(welcome.keepalive_timeout_ms, 10_000);
    evaluator_events(&mut ev);

    // A peer that answers pings is never timed out, however idle.
    for _ in 0..60 {
        clock.advance(Duration::from_secs(1));
        ev.tick(clock.now());
        fe.tick(clock.now());
        shuttle(&mut fe, &mut ev, link, clock.now());
    }
    assert!(matches!(
        ev.session_state(welcome.session),
        Some(SessionState::Connected { .. })
    ));
    evaluator_events(&mut ev);

    // A silent peer: the evaluator pings after one second of send silence...
    clock.advance(Duration::from_millis(1_100));
    ev.tick(clock.now());
    let ping: Vec<u8> = std::iter::from_fn(|| ev.poll_transmit(link, clock.now()))
        .flatten()
        .collect();
    assert!(!ping.is_empty(), "a ping after one second of send silence");
    // ...and gives up after the agreed timeout, not before.
    clock.advance(Duration::from_millis(8_800));
    ev.tick(clock.now());
    assert!(matches!(
        ev.session_state(welcome.session),
        Some(SessionState::Connected { .. })
    ));
    clock.advance(Duration::from_millis(200));
    ev.tick(clock.now());
    let events = evaluator_events(&mut ev);
    assert!(
        events.iter().any(|event| matches!(
            event,
            EvaluatorEvent::LinkClosed {
                outcome: CloseOutcome::KeepaliveTimeout,
                ..
            }
        )),
        "{events:?}"
    );
    assert!(matches!(
        ev.session_state(welcome.session),
        Some(SessionState::Detached { .. })
    ));
}

/// The Welcome's keepalive timeout has no lower bound (DESIGN §6.19 gives
/// none). A timeout of zero is not a panic or a link that never expires: both
/// sides end the link with the typed timeout at their first tick.
#[test]
fn a_zero_keepalive_timeout_ends_in_a_typed_timeout() {
    let mut config = EvaluatorConfig::new(build("aaaa", 1));
    config.keepalive_timeout = Duration::ZERO;
    let mut ev = EvaluatorEndpoint::new(config);
    let mut fe = frontend(1);
    let (link, welcome) = connect(&mut fe, &mut ev, "laptop", T0);
    assert_eq!(welcome.keepalive_timeout_ms, 0);
    evaluator_events(&mut ev);

    fe.tick(T0);
    assert_eq!(
        frontend_events(&mut fe),
        vec![FrontendEvent::Closed(CloseOutcome::KeepaliveTimeout)]
    );
    ev.tick(T0);
    let events = evaluator_events(&mut ev);
    assert!(
        events.contains(&EvaluatorEvent::LinkClosed {
            link,
            outcome: CloseOutcome::KeepaliveTimeout,
        }),
        "{events:?}"
    );
    assert!(matches!(
        ev.session_state(welcome.session),
        Some(SessionState::Detached { .. })
    ));
}

// ---------------------------------------------------------------------------
// Resume and takeover (D34)
// ---------------------------------------------------------------------------

fn resume(
    fe: &mut FrontendEndpoint,
    ev: &mut EvaluatorEndpoint,
    now: SessionTime,
) -> (LinkId, Vec<FrontendEvent>) {
    let link = ev.accept(LinkLocality::Dialed, now);
    fe.resume(now)
        .expect("the frontend has a session to resume");
    shuttle(fe, ev, link, now);
    (link, frontend_events(fe))
}

#[test]
fn two_phase_token_accepts_previous_until_proven() {
    let mut ev = evaluator();
    let mut fe = frontend(1);
    let (link, welcome) = connect(&mut fe, &mut ev, "laptop", T0);
    let first_token = fe.resume_token().expect("token").clone();
    ev.receive_eof(link, T0);
    fe.receive_eof(T0);

    // Resume with token n; the Welcome carrying n+1 is lost in flight.
    let link = ev.accept(LinkLocality::Dialed, T0);
    fe.resume(T0).unwrap();
    while let Some(bytes) = fe.poll_transmit(T0) {
        ev.receive(link, &bytes, T0);
    }
    let lost_welcome: Vec<u8> = std::iter::from_fn(|| ev.poll_transmit(link, T0))
        .flatten()
        .collect();
    assert!(!lost_welcome.is_empty());
    ev.receive_eof(link, T0);
    fe.receive_eof(T0);
    frontend_events(&mut fe);

    // The frontend still holds n, which stays valid until n+1 is proven.
    assert_eq!(fe.resume_token(), Some(&first_token));
    let (link, events) = resume(&mut fe, &mut ev, T0);
    let resumed = established(&events);
    assert_eq!(resumed.session, welcome.session);
    let current = fe.resume_token().expect("token").clone();
    assert_ne!(current, first_token);

    // The frontend's first record after that Welcome proves it holds the new
    // token; the old one is refused from then on.
    fe.send(Lane::Control, display_slot(resumed), b"proof".to_vec())
        .unwrap();
    shuttle(&mut fe, &mut ev, link, T0);
    ev.receive_eof(link, T0);
    fe.receive_eof(T0);
    let mut stale = frontend(1);
    stale.restore_session(welcome.session, first_token);
    let (_, events) = resume(&mut stale, &mut ev, T0);
    assert_eq!(refused(&events), SessionRefusal::Unauthorized);
    let (_, events) = resume(&mut fe, &mut ev, T0);
    assert_eq!(established(&events).session, welcome.session);
}

/// A fresh session whose first link carried one control message and then
/// ended. A new session has no previous token, so the evaluator accepts only
/// the one its Welcome issued.
fn detached_session(
    fe: &mut FrontendEndpoint,
    ev: &mut EvaluatorEndpoint,
) -> neomacs_session::handshake::SessionWelcome {
    let (link, welcome) = connect(fe, ev, "laptop", T0);
    fe.send(Lane::Control, display_slot(&welcome), b"proof".to_vec())
        .unwrap();
    shuttle(fe, ev, link, T0);
    ev.receive_eof(link, T0);
    fe.receive_eof(T0);
    frontend_events(fe);
    evaluator_events(ev);
    welcome
}

/// How the evaluator reads the frontend's Hello and the Goodbye after it.
#[derive(Clone, Copy, Debug)]
enum Delivery {
    /// The Hello, then the Goodbye: the evaluator writes its Welcome while
    /// the Goodbye is on the way.
    SeparateReads,
    /// Both in one read.
    OneRead,
}

/// A frontend resumes and detaches before the Welcome arrives, then resumes
/// again with the token it still holds.
fn detach_before_the_welcome_is_read(delivery: Delivery) {
    let mut ev = evaluator();
    let mut fe = frontend(1);
    let welcome = detached_session(&mut fe, &mut ev);
    let held = fe.resume_token().expect("token").clone();

    // The preface and the Hello are written; the user detaches before the
    // Welcome arrives.
    let link = ev.accept(LinkLocality::Dialed, T0);
    fe.resume(T0).unwrap();
    let hello: Vec<u8> = std::iter::from_fn(|| fe.poll_transmit(T0))
        .flatten()
        .collect();
    fe.close(CloseReason::Detached, T0);
    let goodbye: Vec<u8> = std::iter::from_fn(|| fe.poll_transmit(T0))
        .flatten()
        .collect();
    let mut in_flight = Vec::new();
    match delivery {
        Delivery::SeparateReads => {
            ev.receive(link, &hello, T0);
            in_flight.extend(std::iter::from_fn(|| ev.poll_transmit(link, T0)).flatten());
            ev.receive(link, &goodbye, T0);
        }
        Delivery::OneRead => ev.receive(link, &[hello, goodbye].concat(), T0),
    }
    in_flight.extend(std::iter::from_fn(|| ev.poll_transmit(link, T0)).flatten());
    let events = evaluator_events(&mut ev);
    assert!(
        events.iter().any(|event| matches!(
            event,
            EvaluatorEvent::Attached {
                link: attached,
                attachment: Attachment::Warm { .. },
                ..
            } if *attached == link
        )),
        "the resume was admitted before the Goodbye: {events:?}"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            EvaluatorEvent::LinkClosed {
                link: closed,
                outcome: CloseOutcome::Goodbye(CloseReason::Detached),
            } if *closed == link
        )),
        "{events:?}"
    );
    assert!(matches!(
        ev.session_state(welcome.session),
        Some(SessionState::Detached { .. })
    ));
    // What the evaluator wrote reaches a link the frontend already closed.
    fe.receive(&in_flight, T0);
    frontend_events(&mut fe);
    assert_eq!(fe.resume_token(), Some(&held));

    // The token the frontend still holds resumes the session.
    let (_, events) = resume(&mut fe, &mut ev, T0);
    let resumed = established(&events);
    assert_eq!(resumed.session, welcome.session);
    assert!(matches!(resumed.resume, Some(ResumeOutcome::Warm { .. })));
}

/// M2 review r2 (correctness M1): the evaluator took any record after a
/// resume's Welcome as proof that the frontend holds the new token, and
/// stopped accepting the one it presented. A frontend that detaches before
/// the Welcome arrives sends only a Goodbye, never reads the Welcome, and
/// was then refused `Unauthorized` on every later resume.
#[test]
fn a_goodbye_sent_before_the_welcome_is_read_keeps_the_presented_token() {
    detach_before_the_welcome_is_read(Delivery::SeparateReads);
}

#[test]
fn a_goodbye_read_with_the_hello_keeps_the_presented_token() {
    detach_before_the_welcome_is_read(Delivery::OneRead);
}

/// How a test corrupts the evaluator's answer to a resume, so the frontend
/// ends the link without applying the Welcome.
#[derive(Clone, Copy, Debug)]
enum Corruption {
    /// The Welcome's limits exceed the offer: the frontend refuses it with
    /// `Goodbye{ProtocolViolation}`.
    WelcomeLimits,
    /// The evaluator's preface names another major: the frontend answers
    /// with `Refusal{ProtocolMajor}` before it reads the Welcome.
    PrefaceMajor,
}

/// A frontend resumes, ends the link without applying the corrupted answer,
/// then resumes again with the token it still holds.
fn refuse_the_answer_to_a_resume(corruption: Corruption) {
    let mut ev = evaluator();
    let mut fe = frontend(1);
    let welcome = detached_session(&mut fe, &mut ev);
    let held = fe.resume_token().expect("token").clone();

    let link = ev.accept(LinkLocality::Dialed, T0);
    fe.resume(T0).unwrap();
    while let Some(bytes) = fe.poll_transmit(T0) {
        ev.receive(link, &bytes, T0);
    }
    let mut preface = ev.poll_transmit(link, T0).expect("the evaluator's preface");
    assert_eq!(preface.len(), PREFACE_LEN);
    let mut record = ev.poll_transmit(link, T0).expect("the Welcome");
    assert_eq!(ev.poll_transmit(link, T0), None);
    let expected = match corruption {
        Corruption::WelcomeLimits => {
            let Ok(SessionMessage::Welcome(mut lying)) =
                SessionMessage::decode(&record[HEADER_LEN..])
            else {
                panic!("not a Welcome: {record:?}");
            };
            lying.limits.max_record *= 4;
            record = SessionMessage::Welcome(lying).to_record().unwrap();
            CloseOutcome::Goodbye(CloseReason::ProtocolViolation)
        }
        Corruption::PrefaceMajor => {
            preface[8..10].copy_from_slice(&2u16.to_le_bytes());
            CloseOutcome::RefusedByPeer(SessionRefusal::ProtocolMajor { ours: 1, theirs: 2 })
        }
    };
    preface.extend(record);
    fe.receive(&preface, T0);
    while let Some(bytes) = fe.poll_transmit(T0) {
        ev.receive(link, &bytes, T0);
    }
    let events = evaluator_events(&mut ev);
    assert!(
        events.iter().any(|event| matches!(
            event,
            EvaluatorEvent::LinkClosed { link: closed, outcome }
                if *closed == link && *outcome == expected
        )),
        "{events:?}"
    );
    let events = frontend_events(&mut fe);
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, FrontendEvent::Established { .. })),
        "{events:?}"
    );
    assert_eq!(fe.resume_token(), Some(&held));

    let (_, events) = resume(&mut fe, &mut ev, T0);
    assert_eq!(established(&events).session, welcome.session);
}

/// M2 review r2 (correctness M1), second path: a frontend that refuses the
/// Welcome keeps the token it presented, so the evaluator must too.
#[test]
fn a_frontend_that_refuses_the_welcome_keeps_the_presented_token() {
    refuse_the_answer_to_a_resume(Corruption::WelcomeLimits);
}

/// The same for the other record a frontend ends a link with, a Refusal.
#[test]
fn a_frontend_that_refuses_the_evaluator_preface_keeps_the_presented_token() {
    refuse_the_answer_to_a_resume(Corruption::PrefaceMajor);
}

/// A frontend that sends nothing after a resume still proves the new token
/// promptly: its first keepalive ping, after one second of send silence,
/// is a record only an established link sends.
#[test]
fn a_quiet_frontend_proves_the_new_token_with_its_first_ping() {
    let clock = FakeClock::new();
    let mut ev = evaluator();
    let mut fe = frontend(1);
    let welcome = detached_session(&mut fe, &mut ev);
    let presented = fe.resume_token().expect("token").clone();
    let (link, events) = resume(&mut fe, &mut ev, clock.now());
    established(&events);

    clock.advance(Duration::from_millis(1_100));
    fe.tick(clock.now());
    let ping: Vec<u8> = std::iter::from_fn(|| fe.poll_transmit(clock.now()))
        .flatten()
        .collect();
    let Ok(SessionMessage::Ping { .. }) = SessionMessage::decode(&ping[HEADER_LEN..]) else {
        panic!("not a lone Ping: {ping:?}");
    };
    ev.receive(link, &ping, clock.now());
    ev.receive_eof(link, clock.now());
    fe.receive_eof(clock.now());

    let mut stale = frontend(1);
    stale.restore_session(welcome.session, presented);
    let (_, events) = resume(&mut stale, &mut ev, clock.now());
    assert_eq!(refused(&events), SessionRefusal::Unauthorized);
    let (_, events) = resume(&mut fe, &mut ev, clock.now());
    assert_eq!(established(&events).session, welcome.session);
}

#[test]
fn implicit_takeover_by_valid_token() {
    let mut ev = evaluator();
    let mut fe = frontend(1);
    let (old_link, welcome) = connect(&mut fe, &mut ev, "laptop", T0);
    evaluator_events(&mut ev);

    // The frontend roams: the old link still looks connected to the evaluator.
    fe.receive_eof(T0);
    let (new_link, events) = resume(&mut fe, &mut ev, T0);
    let resumed = established(&events);
    assert_eq!(resumed.session, welcome.session);
    assert!(matches!(resumed.resume, Some(ResumeOutcome::Warm { .. })));
    let events = evaluator_events(&mut ev);
    assert!(
        events.iter().any(|event| matches!(
            event,
            EvaluatorEvent::LinkClosed {
                link,
                outcome: CloseOutcome::Local(CloseReason::TakenOver),
            } if *link == old_link
        )),
        "{events:?}"
    );
    assert_eq!(
        ev.session_state(welcome.session),
        Some(SessionState::Connected { link: new_link })
    );
    // The old link carries a Goodbye{TakenOver} and then nothing.
    let mut old_peer = frontend(9);
    old_peer.connect(ConnectIntent::Inherited, T0);
    while old_peer.poll_transmit(T0).is_some() {}
    old_peer.receive(&Preface::current(Role::Evaluator).to_bytes(), T0);
    let goodbye: Vec<u8> = std::iter::from_fn(|| ev.poll_transmit(old_link, T0))
        .flatten()
        .collect();
    old_peer.receive(&goodbye, T0);
    assert!(frontend_events(&mut old_peer).iter().any(|event| matches!(
        event,
        FrontendEvent::Closed(CloseOutcome::Goodbye(CloseReason::TakenOver))
    )));
}

#[test]
fn tokenless_offer_on_held_label_needs_takeover() {
    let mut ev = evaluator();
    let mut fe = frontend(1);
    let (held_link, welcome) = connect(&mut fe, &mut ev, "laptop", T0);
    evaluator_events(&mut ev);

    let mut other = frontend(2);
    let link = ev.accept(LinkLocality::Dialed, T0);
    other.connect(
        ConnectIntent::Offer {
            label: label("laptop"),
            mode: OfferMode::Exclusive,
        },
        T0,
    );
    shuttle(&mut other, &mut ev, link, T0);
    assert_eq!(
        refused(&frontend_events(&mut other)),
        SessionRefusal::LabelInUse {
            label: label("laptop"),
            state: LabelState::Connected,
        }
    );

    let mut taker = frontend(3);
    let link = ev.accept(LinkLocality::Dialed, T0);
    taker.connect(
        ConnectIntent::Offer {
            label: label("laptop"),
            mode: OfferMode::Takeover,
        },
        T0,
    );
    shuttle(&mut taker, &mut ev, link, T0);
    let events = frontend_events(&mut taker);
    let adopted = established(&events);
    assert_eq!(adopted.session, welcome.session, "the terminal is adopted");
    assert_eq!(adopted.resume, Some(ResumeOutcome::Cold));
    let events = evaluator_events(&mut ev);
    assert!(events.iter().any(|event| matches!(
        event,
        EvaluatorEvent::LinkClosed {
            link,
            outcome: CloseOutcome::Local(CloseReason::TakenOver),
        } if *link == held_link
    )));
    assert!(events.iter().any(|event| matches!(
        event,
        EvaluatorEvent::Attached {
            attachment: Attachment::Cold,
            ..
        }
    )));
}

#[test]
fn incarnation_change_is_a_cold_resume() {
    let mut ev = evaluator();
    let mut fe = frontend(1);
    let (link, welcome) = connect(&mut fe, &mut ev, "laptop", T0);
    let slot = display_slot(&welcome);
    for serial in 0..3u8 {
        fe.send(Lane::Input, slot, vec![serial]).unwrap();
    }
    shuttle(&mut fe, &mut ev, link, T0);
    ev.receive_eof(link, T0);
    evaluator_events(&mut ev);

    // A restarted frontend process presents the token with a new incarnation.
    let token = fe.resume_token().expect("token").clone();
    let mut restarted = frontend(2);
    restarted.restore_session(welcome.session, token);
    let (_, events) = resume(&mut restarted, &mut ev, T0);
    let resumed = established(&events);
    assert_eq!(resumed.session, welcome.session);
    assert_eq!(resumed.resume, Some(ResumeOutcome::Cold));
    let events = evaluator_events(&mut ev);
    assert!(events.iter().any(|event| matches!(
        event,
        EvaluatorEvent::Attached {
            attachment: Attachment::Cold,
            ..
        }
    )));
    // The input stream starts afresh: no replay, numbering from one.
    restarted.send(Lane::Input, slot, vec![42]).unwrap();
    let link = match ev.session_state(welcome.session) {
        Some(SessionState::Connected { link }) => link,
        other => panic!("{other:?}"),
    };
    shuttle(&mut restarted, &mut ev, link, T0);
    let delivered = evaluator_events(&mut ev);
    let indices: Vec<u64> = delivered
        .iter()
        .filter_map(|event| match event {
            EvaluatorEvent::Message { message, .. } if message.lane == Lane::Input => {
                Some(message.index.get())
            }
            _ => None,
        })
        .collect();
    assert_eq!(indices, vec![1]);
}

#[test]
fn replay_dedupes_by_received_through() {
    let mut ev = evaluator();
    let mut fe = frontend(1);
    let (link, welcome) = connect(&mut fe, &mut ev, "laptop", T0);
    let slot = display_slot(&welcome);
    evaluator_events(&mut ev);

    // Ten input messages; the link dies after the evaluator read six.
    for serial in 1..=10u8 {
        fe.send(Lane::Input, slot, vec![serial]).unwrap();
    }
    let mut records = Vec::new();
    while let Some(bytes) = fe.poll_transmit(T0) {
        records.push(bytes);
    }
    for bytes in &records[..6] {
        ev.receive(link, bytes, T0);
    }
    ev.receive_eof(link, T0);
    fe.receive_eof(T0);
    let first = delivered_on(&evaluator_events(&mut ev), Lane::Input);
    assert_eq!(
        first,
        (1..=6u8).map(|serial| vec![serial]).collect::<Vec<_>>()
    );

    // A warm resume reports received_through = 6 and the frontend replays
    // only what is above it.
    let (_, events) = resume(&mut fe, &mut ev, T0);
    match established(&events).resume {
        Some(ResumeOutcome::Warm { received }) => assert_eq!(received.get(Lane::Input), 6),
        ref other => panic!("{other:?}"),
    }
    let replayed = delivered_on(&evaluator_events(&mut ev), Lane::Input);
    assert_eq!(
        replayed,
        (7..=10u8).map(|serial| vec![serial]).collect::<Vec<_>>()
    );

    // A replay that starts at or below received_through is deduplicated:
    // the evaluator drops what it already delivered and delivers the rest.
    let link = match ev.session_state(welcome.session) {
        Some(SessionState::Connected { link }) => link,
        other => panic!("{other:?}"),
    };
    let mut late = SessionMessage::ReplayFrom {
        lane: Lane::Input,
        first: MessageIndex::new(NonZeroU64::new(8).unwrap()),
    }
    .to_record()
    .unwrap();
    for serial in 8..=11u8 {
        late.extend(service_record(Lane::Input, slot, &[serial]));
    }
    ev.receive(link, &late, T0);
    assert_eq!(
        delivered_on(&evaluator_events(&mut ev), Lane::Input),
        vec![vec![11u8]],
        "serials 8..=10 were delivered once already"
    );
}

/// M2 review r2 (correctness m2): a replay the evaluator began on a link that
/// died numbered the first message of the next link. The frontend kept the
/// replay's index across a warm Welcome, so the evaluator's next message was
/// dropped as a duplicate. The evaluator already clears it on a warm resume.
#[test]
fn a_warm_welcome_ends_a_replay_begun_on_the_old_link() {
    let mut ev = evaluator();
    let mut fe = frontend(1);
    let (link, welcome) = connect(&mut fe, &mut ev, "laptop", T0);
    let slot = display_slot(&welcome);
    for body in [b"a", b"b"] {
        ev.send(welcome.session, Lane::Present, slot, body.to_vec())
            .unwrap();
    }
    shuttle(&mut fe, &mut ev, link, T0);
    assert_eq!(
        messages_on(&frontend_events(&mut fe), Lane::Present),
        vec![b"a".to_vec(), b"b".to_vec()]
    );
    // The evaluator starts re-sending from message 1; the link dies before
    // any re-sent message arrives.
    fe.receive(&replay_from_record(Lane::Present, 1), T0);
    ev.receive_eof(link, T0);
    fe.receive_eof(T0);
    frontend_events(&mut fe);

    let (link, events) = resume(&mut fe, &mut ev, T0);
    assert!(matches!(
        established(&events).resume,
        Some(ResumeOutcome::Warm { .. })
    ));
    let index = ev
        .send(welcome.session, Lane::Present, slot, b"c".to_vec())
        .unwrap();
    assert_eq!(index.get(), 3, "the lane continues after what was received");
    shuttle(&mut fe, &mut ev, link, T0);
    let events = frontend_events(&mut fe);
    assert_eq!(
        messages_on(&events, Lane::Present),
        vec![b"c".to_vec()],
        "{events:?}"
    );
    assert_eq!(fe.closed(), None, "{events:?}");
}

/// bincode 2 `standard()` varint of a `u64`.
fn varint(value: u64) -> Vec<u8> {
    if let Ok(small) = u8::try_from(value)
        && small < 251
    {
        return vec![small];
    }
    let mut bytes = Vec::new();
    if let Ok(value) = u16::try_from(value) {
        bytes.push(251);
        bytes.extend(value.to_le_bytes());
    } else if let Ok(value) = u32::try_from(value) {
        bytes.push(252);
        bytes.extend(value.to_le_bytes());
    } else {
        bytes.push(253);
        bytes.extend(value.to_le_bytes());
    }
    bytes
}

/// A peer's `ReplayFrom` record with any `first`, as raw bytes.
fn replay_from_record(lane: Lane, first: u64) -> Vec<u8> {
    // Variant 8 of `SessionMessage`, then the lane's variant, then `first`.
    let mut body = vec![8, lane as u8];
    body.extend(varint(first));
    let header = RecordHeader {
        len: u32::try_from(body.len()).unwrap(),
        lane: Lane::Control,
        compression: Compression::Raw,
        continuation: Continuation::Last,
        service: ServiceSlot::SESSION,
    };
    let mut record = header.to_bytes().to_vec();
    record.extend(body);
    record
}

fn closed_by_protocol(events: &[EvaluatorEvent], link: LinkId) -> Option<ProtocolError> {
    events.iter().find_map(|event| match event {
        EvaluatorEvent::LinkClosed {
            link: closed,
            outcome: CloseOutcome::Protocol(error),
        } if *closed == link => Some(error.clone()),
        _ => None,
    })
}

#[test]
fn replay_from_zero_is_a_typed_protocol_error() {
    let well_formed = replay_from_record(Lane::Input, 8);
    assert!(matches!(
        SessionMessage::decode(&well_formed[HEADER_LEN..]),
        Ok(SessionMessage::ReplayFrom {
            lane: Lane::Input,
            ..
        })
    ));

    // Message indices start at 1: `first: 0` names no message. It used to
    // drop the next message silently as a "duplicate".
    let mut ev = evaluator();
    let mut fe = frontend(1);
    let (link, welcome) = connect(&mut fe, &mut ev, "laptop", T0);
    evaluator_events(&mut ev);
    let mut bytes = replay_from_record(Lane::Input, 0);
    bytes.extend(service_record(Lane::Input, display_slot(&welcome), b"key"));
    ev.receive(link, &bytes, T0);
    let events = evaluator_events(&mut ev);
    assert!(delivered_on(&events, Lane::Input).is_empty(), "{events:?}");
    assert!(
        matches!(
            closed_by_protocol(&events, link),
            Some(ProtocolError::SessionRecord(_))
        ),
        "{events:?}"
    );
}

#[test]
fn replay_from_the_end_of_the_sequence_is_a_typed_protocol_error() {
    // A replay from the end of the sequence space numbers one message; the
    // next one is a typed error, not an overflow (a panic in a debug build,
    // a wrap to 0 that drops every later message in release).
    let mut ev = evaluator();
    let mut fe = frontend(1);
    let (link, welcome) = connect(&mut fe, &mut ev, "laptop", T0);
    evaluator_events(&mut ev);
    let mut bytes = replay_from_record(Lane::Input, u64::MAX);
    bytes.extend(service_record(Lane::Input, display_slot(&welcome), b"a"));
    bytes.extend(service_record(Lane::Input, display_slot(&welcome), b"b"));
    ev.receive(link, &bytes, T0);
    let events = evaluator_events(&mut ev);
    assert_eq!(delivered_on(&events, Lane::Input), vec![b"a".to_vec()]);
    assert!(
        events.iter().any(|event| matches!(
            event,
            EvaluatorEvent::ReplayGap {
                lane: Lane::Input,
                first: 1,
                last,
                ..
            } if *last == u64::MAX - 1
        )),
        "{events:?}"
    );
    assert_eq!(
        closed_by_protocol(&events, link),
        Some(ProtocolError::LaneExhausted { lane: Lane::Input })
    );

    // The frontend shares the rule.
    let mut ev = evaluator();
    let mut fe = frontend(1);
    let (_, welcome) = connect(&mut fe, &mut ev, "laptop", T0);
    let mut bytes = replay_from_record(Lane::Present, u64::MAX);
    bytes.extend(service_record(Lane::Present, display_slot(&welcome), b"a"));
    bytes.extend(service_record(Lane::Present, display_slot(&welcome), b"b"));
    fe.receive(&bytes, T0);
    let events = frontend_events(&mut fe);
    assert_eq!(messages_on(&events, Lane::Present), vec![b"a".to_vec()]);
    assert!(
        events.contains(&FrontendEvent::Closed(CloseOutcome::Protocol(
            ProtocolError::LaneExhausted {
                lane: Lane::Present
            }
        ))),
        "{events:?}"
    );
}

#[test]
fn resume_cursors_beyond_what_was_sent_are_a_typed_protocol_error() {
    let mut ev = evaluator();
    let mut fe = frontend(1);
    let (link, welcome) = connect(&mut fe, &mut ev, "laptop", T0);
    let slot = display_slot(&welcome);
    ev.send(welcome.session, Lane::Present, slot, vec![1])
        .unwrap();
    shuttle(&mut fe, &mut ev, link, T0);
    ev.receive_eof(link, T0);
    fe.receive_eof(T0);
    evaluator_events(&mut ev);

    // A peer with the token claims messages the evaluator never sent.
    let mut received = LaneCounts::default();
    received.set(Lane::Present, u64::MAX);
    let hello = SessionMessage::Hello(SessionHello {
        build: build("aaaa", 1),
        incarnation: FrontendIncarnation::from_raw(1),
        services: vec![display(1, 0, 0)],
        intent: SessionIntent::Resume {
            session: welcome.session,
            token: fe.resume_token().expect("token").clone(),
            cursors: ResumeCursors { received },
        },
        limits: Limits::default(),
        ext: Vec::new(),
    });
    let forged = ev.accept(LinkLocality::Dialed, T0);
    let mut bytes = Preface::current(Role::Frontend).to_bytes().to_vec();
    bytes.extend(hello.to_record().unwrap());
    ev.receive(forged, &bytes, T0);
    // Trusting the cursor rewound the sent count to u64::MAX; a send and an
    // acknowledgement then overflowed the link's accounting.
    let _ = ev.send(welcome.session, Lane::Present, slot, vec![2]);
    while ev.poll_transmit(forged, T0).is_some() {}
    let ack = SessionMessage::Ack {
        lane: Lane::Present,
        through: 1,
    };
    ev.receive(forged, &ack.to_record().unwrap(), T0);
    let events = evaluator_events(&mut ev);
    assert_eq!(
        closed_by_protocol(&events, forged),
        Some(ProtocolError::ReceivedBeyondSent {
            lane: Lane::Present,
            received: u64::MAX,
            sent: 1,
        }),
        "{events:?}"
    );
    // Nothing was attached: the session still resumes with honest cursors.
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, EvaluatorEvent::Attached { .. })),
        "{events:?}"
    );
    let (_, events) = resume(&mut fe, &mut ev, T0);
    assert_eq!(established(&events).session, welcome.session);
}

#[test]
fn warm_welcome_counts_beyond_what_was_sent_are_a_typed_protocol_error() {
    let mut ev = evaluator();
    let mut fe = frontend(1);
    let (link, welcome) = connect(&mut fe, &mut ev, "laptop", T0);
    ev.receive_eof(link, T0);
    fe.receive_eof(T0);
    frontend_events(&mut fe);

    // The frontend resumes; a peer answers with a Welcome that claims it
    // received facts the frontend never sent.
    fe.resume(T0).unwrap();
    while fe.poll_transmit(T0).is_some() {}
    let mut received = LaneCounts::default();
    received.set(Lane::Facts, u64::MAX);
    let mut forged = welcome.clone();
    forged.resume = Some(ResumeOutcome::Warm { received });
    let mut bytes = Preface::current(Role::Evaluator).to_bytes().to_vec();
    bytes.extend(SessionMessage::Welcome(forged).to_record().unwrap());
    fe.receive(&bytes, T0);
    // Trusting it rewound the facts count to u64::MAX; a send and an
    // acknowledgement then overflowed the link's accounting.
    let _ = fe.send(Lane::Facts, display_slot(&welcome), vec![1]);
    while fe.poll_transmit(T0).is_some() {}
    let ack = SessionMessage::Ack {
        lane: Lane::Facts,
        through: 1,
    };
    fe.receive(&ack.to_record().unwrap(), T0);
    let events = frontend_events(&mut fe);
    assert!(
        events.contains(&FrontendEvent::Closed(CloseOutcome::Protocol(
            ProtocolError::ReceivedBeyondSent {
                lane: Lane::Facts,
                received: u64::MAX,
                sent: 0,
            }
        ))),
        "{events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, FrontendEvent::Established { .. })),
        "{events:?}"
    );
}

// ---------------------------------------------------------------------------
// Pump over adversarial transports
// ---------------------------------------------------------------------------

const MESSAGES: u8 = 40;

/// How one side of [`run_pair`] ended.
#[derive(Debug, PartialEq, Eq)]
enum PumpOutcome {
    Closed(CloseOutcome),
    StepBudgetExhausted,
}

/// Run a frontend and an evaluator over `fe_io`/`ev_io` until the frontend
/// closes after receiving `MESSAGES` echoes. Returns each side's outcome.
fn run_pair<A, B>(fe_io: A, ev_io: B, step_budget: usize) -> (PumpOutcome, PumpOutcome)
where
    A: std::io::Read + std::io::Write,
    B: std::io::Read + std::io::Write,
{
    let clock = FakeClock::auto_advancing(Duration::from_millis(1));
    let mut ev = evaluator();
    let mut fe = frontend(1);
    let link = ev.accept(LinkLocality::Dialed, clock.now());
    fe.connect(
        ConnectIntent::Offer {
            label: label("laptop"),
            mode: OfferMode::Exclusive,
        },
        clock.now(),
    );
    let mut fe_pump = Pump::new(fe_io, clock.clone());
    let mut ev_pump = Pump::new(ev_io, clock.clone());
    let mut fe_outcome = None;
    let mut ev_outcome = None;
    let mut sent = 0u8;
    let mut echoed = 0u8;
    let mut slot = None;
    for _ in 0..step_budget {
        if fe_outcome.is_none()
            && let StepOutcome::Closed(outcome) = fe_pump.step(&mut fe)
        {
            fe_outcome = Some(PumpOutcome::Closed(outcome));
        }
        if ev_outcome.is_none()
            && let StepOutcome::Closed(outcome) =
                ev_pump.step(&mut EvaluatorLink::new(&mut ev, link))
        {
            ev_outcome = Some(PumpOutcome::Closed(outcome));
        }
        while let Some(event) = fe.poll_event() {
            match event {
                FrontendEvent::Established { welcome, .. } => slot = Some(display_slot(&welcome)),
                FrontendEvent::Message(_) => echoed += 1,
                _ => {}
            }
        }
        if let Some(slot) = slot
            && sent < MESSAGES
            && fe_outcome.is_none()
        {
            fe.send(Lane::Input, slot, vec![sent; 300]).unwrap();
            sent += 1;
        }
        if echoed == MESSAGES && fe_outcome.is_none() {
            fe.close(CloseReason::Normal, clock.now());
        }
        while let Some(event) = ev.poll_event() {
            if let EvaluatorEvent::Message { session, message } = event {
                ev.send(session, Lane::Control, message.service, message.body)
                    .unwrap();
            }
        }
        if fe_outcome.is_some() && ev_outcome.is_some() {
            break;
        }
    }
    (
        fe_outcome.unwrap_or(PumpOutcome::StepBudgetExhausted),
        ev_outcome.unwrap_or(PumpOutcome::StepBudgetExhausted),
    )
}

#[test]
fn pump_adversarial_one_byte_io() {
    let (a, b) = memory_pipe();
    let (fe, ev) = run_pair(OneByteIo::new(a), OneByteIo::new(b), 400_000);
    assert_eq!(
        fe,
        PumpOutcome::Closed(CloseOutcome::Local(CloseReason::Normal))
    );
    assert_eq!(
        ev,
        PumpOutcome::Closed(CloseOutcome::Goodbye(CloseReason::Normal))
    );
}

#[test]
fn pump_adversarial_random_splits() {
    for seed in 1..=8 {
        let (a, b) = memory_pipe();
        let (fe, ev) = run_pair(
            RandomSplit::new(a, seed),
            RandomSplit::new(b, seed * 7),
            100_000,
        );
        assert_eq!(
            fe,
            PumpOutcome::Closed(CloseOutcome::Local(CloseReason::Normal)),
            "seed {seed}"
        );
        assert_eq!(
            ev,
            PumpOutcome::Closed(CloseOutcome::Goodbye(CloseReason::Normal)),
            "seed {seed}"
        );
    }
}

#[test]
fn pump_adversarial_half_close() {
    // The frontend closes its write half once the handshake is under way.
    let (a, b) = memory_pipe();
    let (_, ev) = run_pair(HalfClose::after_written(a, 400), b, 100_000);
    assert_eq!(ev, PumpOutcome::Closed(CloseOutcome::PeerClosed));
}

#[test]
fn pump_adversarial_eof_mid_record() {
    let (a, b) = memory_pipe();
    // 16 preface bytes, then 5 bytes into the Hello record.
    let (_, ev) = run_pair(a, EofAfter::new(b, PREFACE_LEN + 5), 100_000);
    assert!(
        matches!(ev, PumpOutcome::Closed(CloseOutcome::Truncated { .. })),
        "{ev:?}"
    );
}

#[test]
fn pump_adversarial_stall_without_eof() {
    let (a, b) = memory_pipe();
    // The evaluator stops hearing anything after the handshake, without EOF.
    let (_, ev) = run_pair(a, Stall::after_read(b, 2_000), 100_000);
    assert_eq!(ev, PumpOutcome::Closed(CloseOutcome::KeepaliveTimeout));
}

#[test]
fn pump_adversarial_cut_after_nth_record() {
    for records in [1usize, 2, 5] {
        let (a, b) = memory_pipe();
        let (_, ev) = run_pair(a, CutAfterRecords::new(b, records), 100_000);
        assert_eq!(
            ev,
            PumpOutcome::Closed(CloseOutcome::PeerClosed),
            "cut after {records}"
        );
    }
}

#[cfg(unix)]
#[test]
fn pump_over_a_socketpair() {
    let (a, b) = neomacs_session::pump::socket_pair().expect("socketpair");
    let (fe, ev) = run_pair(a, b, 1_000_000);
    assert_eq!(
        fe,
        PumpOutcome::Closed(CloseOutcome::Local(CloseReason::Normal))
    );
    assert_eq!(
        ev,
        PumpOutcome::Closed(CloseOutcome::Goodbye(CloseReason::Normal))
    );
}

/// Pump a frontend and an evaluator whose display host sends a 4 KiB
/// keyframe the moment the session attaches. Returns the frontend's events
/// up to its first message or its close.
fn keyframe_on_attach<A, B>(fe_io: A, ev_io: B) -> Vec<FrontendEvent>
where
    A: std::io::Read + std::io::Write,
    B: std::io::Read + std::io::Write,
{
    let clock = FakeClock::auto_advancing(Duration::from_millis(1));
    let mut ev = evaluator();
    let mut fe = frontend(1);
    let link = ev.accept(LinkLocality::Dialed, clock.now());
    fe.connect(
        ConnectIntent::Offer {
            label: label("laptop"),
            mode: OfferMode::Exclusive,
        },
        clock.now(),
    );
    let mut fe_pump = Pump::new(fe_io, clock.clone());
    let mut ev_pump = Pump::new(ev_io, clock.clone());
    let mut events = Vec::new();
    for _ in 0..10_000 {
        fe_pump.step(&mut fe);
        ev_pump.step(&mut EvaluatorLink::new(&mut ev, link));
        while let Some(event) = ev.poll_event() {
            if let EvaluatorEvent::Attached { session, .. } = event {
                ev.send(session, Lane::Present, DISPLAY_SLOT, vec![7; 4096])
                    .unwrap();
            }
        }
        events.extend(std::iter::from_fn(|| fe.poll_event()));
        if events
            .iter()
            .any(|event| matches!(event, FrontendEvent::Message(_) | FrontendEvent::Closed(_)))
        {
            break;
        }
    }
    events
}

#[test]
fn pump_delivers_a_keyframe_sent_on_attach_over_a_memory_pipe() {
    let (a, b) = memory_pipe();
    let events = keyframe_on_attach(a, b);
    established(&events);
    assert_eq!(
        messages_on(&events, Lane::Present),
        vec![vec![7; 4096]],
        "{events:?}"
    );
}

#[cfg(unix)]
#[test]
fn pump_delivers_a_keyframe_sent_on_attach_over_a_socketpair() {
    let (a, b) = neomacs_session::pump::socket_pair().expect("socketpair");
    let events = keyframe_on_attach(a, b);
    established(&events);
    assert_eq!(
        messages_on(&events, Lane::Present),
        vec![vec![7; 4096]],
        "{events:?}"
    );
}

#[test]
fn memory_pipe_is_a_duplex_byte_stream() {
    use std::io::{Read, Write};
    let (mut a, mut b): (MemoryEnd, MemoryEnd) = memory_pipe();
    a.write_all(b"hi").unwrap();
    let mut buf = [0u8; 4];
    assert_eq!(b.read(&mut buf).unwrap(), 2);
    assert_eq!(
        b.read(&mut buf).unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    a.shutdown_write();
    assert_eq!(b.read(&mut buf).unwrap(), 0);
}

// ---------------------------------------------------------------------------
// Relay records (frozen, DESIGN §6.3)
// ---------------------------------------------------------------------------

#[test]
fn relay_request_roundtrip_and_bounds() {
    let request = RelayRequest {
        server_name: Some(ServerName::new("work.2").unwrap()),
        label: label("laptop"),
        resume: neomacs_session::relay::RelayResume::Fresh,
        dir: Some(b"/home/me/src".to_vec()),
        files: vec![RelayFile {
            path: b"README".to_vec(),
            line: Some(3),
            column: None,
        }],
        ext: vec![],
    };
    let bytes = request.encode().unwrap();
    assert_eq!(RelayRequest::decode(&bytes), Ok(request.clone()));
    // A path server name is allowed when absolute; anything shell-like is not.
    assert!(ServerName::new("/run/user/1000/emacs/server").is_ok());
    for bad in ["", "a b", "../x", "x;rm", &"x".repeat(65)] {
        assert!(ServerName::new(bad).is_err(), "{bad:?}");
    }
    // 64 KiB is the whole preamble budget, enforced on both sides.
    let mut huge = request.clone();
    huge.files = vec![
        RelayFile {
            path: vec![b'f'; 1024],
            line: None,
            column: None,
        };
        64
    ];
    assert!(matches!(huge.encode(), Err(RelayError::TooLarge { .. })));
    let mut oversized = vec![0u8; MAX_RELAY_RECORD + 1];
    oversized[..bytes.len()].copy_from_slice(&bytes);
    assert!(matches!(
        RelayRequest::decode(&oversized),
        Err(RelayError::TooLarge { .. })
    ));
    assert!(RelayRequest::decode(&bytes[..bytes.len() - 1]).is_err());
}

#[test]
fn relay_status_decodable_by_both_ends() {
    let statuses = vec![
        RelayStatus::Connected {
            durability: Durability::Persistent,
            daemon_build: build("aaaa", 1),
            started: neomacs_session::relay::DaemonStart::Started,
        },
        RelayStatus::Refused {
            reason: RelayRefusal::DaemonUnreachable {
                path: b"/tmp/emacs1000/server".to_vec(),
                errno: 111,
            },
        },
        RelayStatus::Refused {
            reason: RelayRefusal::ServerSocketUnreachable {
                path: b"/tmp/x".to_vec(),
            },
        },
        RelayStatus::Refused {
            reason: RelayRefusal::UnsafeRendezvous {
                path: b"/tmp/y".to_vec(),
            },
        },
        RelayStatus::Refused {
            reason: RelayRefusal::DaemonTooOld {
                build: build("0000", 0),
            },
        },
        RelayStatus::Refused {
            reason: RelayRefusal::StartFailed {
                detail: "exit 1".to_string(),
            },
        },
    ];
    for status in statuses {
        // The relay writes it; the frontend reads it.
        let bytes = status.encode().unwrap();
        assert_eq!(RelayStatus::decode(&bytes), Ok(status));
    }
    let probe = ProbeReply {
        build: build("aaaa", 1),
        durability: Durability::Unknown,
        server_socket: b"/tmp/emacs1000/server".to_vec(),
    };
    assert_eq!(ProbeReply::decode(&probe.encode().unwrap()), Ok(probe));
}
