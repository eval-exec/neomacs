use neovm_core::host_effect::contracts;

fn main() {
    let _affinity = contracts::HostAffinity { _local: std::marker::PhantomData };
}
