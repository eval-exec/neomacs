#[path = "../host_effect_ui/support/owner_step_owner.rs"]
mod owner;
use owner::owner_step::OwnerFrameSchema;

fn complete_schema_consumer(schema: OwnerFrameSchema) {
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
        OwnerFrameSchema::ContractInjected => {}
    }
}
fn main() {}
