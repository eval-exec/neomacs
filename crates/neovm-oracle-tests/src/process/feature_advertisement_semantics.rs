//! GNU baseline for `make-network-process' feature advertisement.
//!
//! These tests keep feature advertisement conservative: record GNU's full
//! surface, and assert Neomacs only advertises features that have matching
//! runtime behavior.
//!
//! The subfeature list below is compared UNSORTED, and ledger 197 is why.  It
//! used to be wrapped in `(sort (copy-sequence ...))`, which made the only
//! comparison of this list against GNU a comparison of SETS -- and the two
//! editors' sets agreed while their orders did not.  GNU's order is not
//! arbitrary: `src/process.c:9072-9089` conses each `ADD_SUBFEATURE` onto the
//! front and then conses the `socket_options` table on top of those, so the
//! finished list is the reverse of the source order in two runs.  This port
//! built the eight keyword pairs the other way round until ledger 197.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[cfg(target_os = "linux")]
#[test]
fn oracle_gnu_make_network_process_advertises_full_linux_surface() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (featurep 'make-network-process)
 (featurep 'make-network-process '(:family local))
 (featurep 'make-network-process '(:family ipv4))
 (featurep 'make-network-process '(:family ipv6))
 (featurep 'make-network-process '(:service t))
 (featurep 'make-network-process '(:server t))
 (featurep 'make-network-process '(:nowait t))
 (featurep 'make-network-process '(:type datagram))
 (featurep 'make-network-process '(:type seqpacket))
 (featurep 'make-network-process :reuseaddr)
 (featurep 'make-network-process :keepalive)
 (featurep 'make-network-process :bindtodevice)
 (get 'make-network-process 'subfeatures))
"#;

    let expect = expect_test::expect![
        "OK (t t t t t t t t t t t t (:nodelay :reuseaddr :priority :oobinline :linger :keepalive :dontroute :broadcast :bindtodevice (:server t) (:service t) (:family ipv6) (:family ipv4) (:family local) (:type seqpacket) (:type datagram) (:nowait t)))"
    ];
    let oracle = crate::common::run_oracle_eval(form).expect("oracle eval should run");
    expect.assert_eq(&oracle);
}

#[cfg(unix)]
#[test]
fn oracle_make_network_process_seqpacket_featurep_matches_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (featurep 'make-network-process '(:type seqpacket))
  (featurep 'make-network-process '(:type datagram))
  (featurep 'make-network-process '(:family local))
  (featurep 'make-network-process '(:type raw)))"#,
        expect,
    );
}
