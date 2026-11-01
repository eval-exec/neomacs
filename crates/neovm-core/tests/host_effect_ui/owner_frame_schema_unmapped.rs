#[path = "support/owner_step_owner.rs"]
mod owner;
use owner::owner_step::OwnerFrameSchema;

// The real lower schema gets ContractInjected only in contract-fixtures.
// This deliberately incomplete consumer must diagnose that ACTUAL variant.
fn incomplete_schema_consumer(schema: OwnerFrameSchema) {
    match schema {
        OwnerFrameSchema::Redisplay {
            arguments: _,
            frame: _,
            restore: _,
        } => {}
        OwnerFrameSchema::DisplayQuery {
            arguments: _,
            frame: _,
            restore: _,
        } => {}
        OwnerFrameSchema::Font {
            arguments: _,
            frame: _,
            restore: _,
        } => {}
        OwnerFrameSchema::Selection {
            arguments: _,
            frame: _,
            restore: _,
        } => {}
        OwnerFrameSchema::Callback {
            arguments: _,
            frame: _,
            restore: _,
        } => {}
        OwnerFrameSchema::Restore {
            arguments: _,
            frame: _,
            restore: _,
        } => {}
    }
}
fn main() {}
