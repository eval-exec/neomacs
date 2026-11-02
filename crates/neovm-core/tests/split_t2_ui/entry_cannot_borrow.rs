use neovm_core::tagged::header::SubrObj;
fn borrowed<'a>(_: &'a u8) {
    let _: Option<SubrObj<&'a u8>> = None;
}
fn main() {}
