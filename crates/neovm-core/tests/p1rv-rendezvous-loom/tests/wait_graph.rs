//! Finite wait-graph checks for the declared rendezvous lock order.
//!
//! These tests check the edges the protocol declares, not production lock
//! acquisition, OS fairness, or the absence of undeclared dependencies. The
//! positive case also exercises a real Loom condvar predicate wait. Negative
//! cases detect a graph cycle without actually deadlocking Loom's threads.

#![forbid(unsafe_code)]

mod support;

use loom::sync::{Arc, Condvar, Mutex};
use loom::thread;

const WAIT_GRAPH_CYCLE: &str = "WAIT_GRAPH_CYCLE";
const NODE_COUNT: usize = 6;

#[derive(Clone, Copy)]
#[repr(usize)]
enum Node {
    Keeper,
    Owner,
    Baton,
    Control,
    Allocator,
    /// One owner's registry entry: its published image and root batch.
    Entry,
}

/// Actor -> resource means waiting; resource -> actor means held.
/// Keeper -> Owner means waiting for the owner's acknowledgement.
#[derive(Default)]
struct WaitGraph {
    edges: [u8; NODE_COUNT],
}

impl WaitGraph {
    fn declare(&mut self, from: Node, to: Node) {
        self.edges[from as usize] |= 1 << to as usize;
    }

    fn add(&mut self, from: Node, to: Node) {
        self.declare(from, to);
        self.assert_acyclic();
    }

    fn remove(&mut self, from: Node, to: Node) {
        self.edges[from as usize] &= !(1 << to as usize);
    }

    fn contains(&self, from: Node, to: Node) -> bool {
        self.edges[from as usize] & (1 << to as usize) != 0
    }

    fn assert_acyclic(&self) {
        // Finite transitive closure: a diagonal bit is exactly a cycle.
        let mut reach = self.edges;
        for via in 0..NODE_COUNT {
            for from in 0..NODE_COUNT {
                if reach[from] & (1 << via) != 0 {
                    reach[from] |= reach[via];
                }
            }
        }
        if (0..NODE_COUNT).any(|node| reach[node] & (1 << node) != 0) {
            panic!("{WAIT_GRAPH_CYCLE}");
        }
    }
}

struct State {
    graph: WaitGraph,
    acknowledged: bool,
    grant_released: bool,
}

/// The keeper may retain its Baton, but ACK has no Baton dependency.
/// Its control ownership ends before waiting; the owner ends its allocator
/// grant before ACK. Notify-before-wait is permitted by the predicate.
#[test]
fn acknowledgement_needs_no_baton_and_wait_releases_control() {
    support::check(2, || {
        let mut graph = WaitGraph::default();
        graph.add(Node::Baton, Node::Keeper);
        graph.add(Node::Control, Node::Keeper);
        graph.remove(Node::Control, Node::Keeper);
        graph.add(Node::Keeper, Node::Owner);
        let state = Arc::new((
            Mutex::new(State {
                graph,
                acknowledged: false,
                grant_released: false,
            }),
            Condvar::new(),
        ));

        let owner_state = Arc::clone(&state);
        let owner = thread::spawn(move || {
            let (control, wake) = &*owner_state;
            {
                let mut state = control.lock().unwrap();
                state.graph.add(Node::Control, Node::Owner);
                state.graph.add(Node::Allocator, Node::Owner);
                state.graph.remove(Node::Control, Node::Owner);
            }
            // A request can be waiting while allocation is completing. Its
            // grant is local and must be relinquished before publication/ACK.
            {
                let mut state = control.lock().unwrap();
                state.graph.add(Node::Control, Node::Owner);
                state.graph.remove(Node::Allocator, Node::Owner);
                state.grant_released = true;
                assert!(!state.graph.contains(Node::Allocator, Node::Owner));
                assert!(!state.graph.contains(Node::Owner, Node::Baton));
                state.acknowledged = true;
                state.graph.remove(Node::Keeper, Node::Owner);
                state.graph.remove(Node::Control, Node::Owner);
                wake.notify_all();
            }
        });

        let (control, wake) = &*state;
        let mut state = control.lock().unwrap();
        while !state.acknowledged {
            // This declared release corresponds to Condvar::wait's atomic
            // release of the actual mutex. No control ownership edge survives.
            state.graph.add(Node::Control, Node::Keeper);
            state.graph.remove(Node::Control, Node::Keeper);
            state.graph.assert_acyclic();
            state = wake.wait(state).unwrap();
        }
        assert!(state.grant_released);
        assert!(state.graph.contains(Node::Baton, Node::Keeper));
        assert!(!state.graph.contains(Node::Keeper, Node::Owner));
        state.graph.assert_acyclic();
        drop(state);
        owner.join().unwrap();
    });
}

fn keeper_dependency_control(resource: Node) {
    support::negative(2, WAIT_GRAPH_CYCLE, move || {
        let mut graph = WaitGraph::default();
        // Wrong protocol: the keeper retains something ACK itself requires.
        graph.add(resource, Node::Keeper);
        let graph = Arc::new(Mutex::new(graph));
        let owner_graph = Arc::clone(&graph);
        let owner = thread::spawn(move || {
            owner_graph.lock().unwrap().declare(Node::Owner, resource);
        });
        // Either insertion order closes the same declared three-edge cycle.
        // We never wait on the resource, so the required witness is our exact
        // cycle panic, not Loom deadlock detection or a branch-limit failure.
        graph.lock().unwrap().declare(Node::Keeper, Node::Owner);
        owner.join().unwrap();
        // Detect on the model's main thread after the worker finishes: join
        // failure/poisoning cannot obscure the exact required witness.
        graph.lock().unwrap().assert_acyclic();
    });
}

#[test]
fn negative_ack_requires_escrowed_baton() {
    keeper_dependency_control(Node::Baton);
}

#[test]
fn negative_keeper_keeps_control_while_waiting_for_ack() {
    keeper_dependency_control(Node::Control);
}

#[test]
fn negative_keeper_keeps_allocator_while_waiting_for_ack() {
    keeper_dependency_control(Node::Allocator);
}

#[test]
fn negative_owner_requests_and_waits_without_releasing_grant() {
    support::negative(2, WAIT_GRAPH_CYCLE, || {
        let mut graph = WaitGraph::default();
        graph.add(Node::Allocator, Node::Owner);
        let graph = Arc::new(Mutex::new(graph));
        let keeper_graph = Arc::clone(&graph);
        let keeper = thread::spawn(move || {
            // Keeper requires allocator exclusivity before collecting.
            keeper_graph
                .lock()
                .unwrap()
                .declare(Node::Keeper, Node::Allocator);
        });
        // Wrong protocol: requester waits for the collector while retaining
        // the grant instead of finishing it and participating in the stop.
        graph.lock().unwrap().declare(Node::Owner, Node::Keeper);
        keeper.join().unwrap();
        graph.lock().unwrap().assert_acyclic();
    });
}

/// The stopped keeper's blocking waits (`lock_for_keeper` on the control
/// gate, then the bounded owner-moment wait on an entry) close no cycle. An
/// owner holds the gate, and its entry under it, only for a moment: it takes
/// the entry with try_lock and waits on nothing while it holds either. The
/// keeper waits for the entry only after it lets go of the gate. Its own hold
/// on an entry across a body is not a moment: a second acquisition refuses
/// without declaring a wait.
#[test]
fn keeper_waits_out_brief_gate_and_entry_holders() {
    support::check(2, || {
        let graph = Arc::new(Mutex::new(WaitGraph::default()));
        let gate = Arc::new(Mutex::new(()));
        let entry = Arc::new(Mutex::new(()));

        let owner_graph = Arc::clone(&graph);
        let owner_gate = Arc::clone(&gate);
        let owner_entry = Arc::clone(&entry);
        let owner = thread::spawn(move || {
            let held_gate = owner_gate.lock().unwrap();
            owner_graph.lock().unwrap().add(Node::Control, Node::Owner);
            if let Ok(held_entry) = owner_entry.try_lock() {
                let mut graph = owner_graph.lock().unwrap();
                graph.add(Node::Entry, Node::Owner);
                assert!(!graph.contains(Node::Owner, Node::Keeper));
                graph.remove(Node::Entry, Node::Owner);
                drop(graph);
                drop(held_entry);
            }
            owner_graph
                .lock()
                .unwrap()
                .remove(Node::Control, Node::Owner);
            drop(held_gate);
        });

        // lock_for_keeper: the keeper holds nothing else while it waits.
        graph.lock().unwrap().add(Node::Keeper, Node::Control);
        let held_gate = gate.lock().unwrap();
        {
            let mut graph = graph.lock().unwrap();
            graph.remove(Node::Keeper, Node::Control);
            graph.add(Node::Control, Node::Keeper);
            graph.remove(Node::Control, Node::Keeper);
        }
        drop(held_gate);

        // The owner-moment wait, off the gate.
        let held_entry = loop {
            graph.lock().unwrap().add(Node::Keeper, Node::Entry);
            match entry.try_lock() {
                Ok(held) => break held,
                Err(_) => thread::yield_now(),
            }
        };
        {
            let mut graph = graph.lock().unwrap();
            graph.remove(Node::Keeper, Node::Entry);
            graph.add(Node::Entry, Node::Keeper);
            assert!(!graph.contains(Node::Control, Node::Keeper));
            // A body reaches the same entry again: the keeper sees its own
            // hold and refuses instead of adding Keeper -> Entry.
            assert!(graph.contains(Node::Entry, Node::Keeper));
            graph.remove(Node::Entry, Node::Keeper);
            graph.assert_acyclic();
        }
        drop(held_entry);
        owner.join().unwrap();
    });
}

/// Wrong protocol: an owner holds RESOURCE while it waits on the keeper, and
/// the keeper's blocking acquisition waits on RESOURCE.
fn owner_holds_while_the_keeper_waits(resource: Node) {
    support::negative(2, WAIT_GRAPH_CYCLE, move || {
        let mut graph = WaitGraph::default();
        graph.add(resource, Node::Owner);
        let graph = Arc::new(Mutex::new(graph));
        let keeper_graph = Arc::clone(&graph);
        let keeper = thread::spawn(move || {
            keeper_graph.lock().unwrap().declare(Node::Keeper, resource);
        });
        graph.lock().unwrap().declare(Node::Owner, Node::Keeper);
        keeper.join().unwrap();
        graph.lock().unwrap().assert_acyclic();
    });
}

#[test]
fn negative_owner_waits_for_the_keeper_holding_the_gate() {
    owner_holds_while_the_keeper_waits(Node::Control);
}

#[test]
fn negative_owner_waits_for_the_keeper_holding_its_entry() {
    owner_holds_while_the_keeper_waits(Node::Entry);
}

/// Wrong protocol: the keeper holds an entry across a body and waits for
/// that entry again instead of refusing.
#[test]
fn negative_keeper_waits_out_its_own_entry_hold() {
    support::negative(1, WAIT_GRAPH_CYCLE, || {
        let mut graph = WaitGraph::default();
        graph.add(Node::Entry, Node::Keeper);
        graph.add(Node::Keeper, Node::Entry);
    });
}
