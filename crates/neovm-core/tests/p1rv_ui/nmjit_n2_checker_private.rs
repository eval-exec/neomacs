use neovm_value::tagged::gc::Gen1Admission;

// The checked admission owns its quiescent-collector loan privately; only the
// gc module can reclaim the collector from an admission.
fn unchecked_checker(admission: Gen1Admission<'static>) {
    let _collector = admission.collector;
}

fn main() {}
