#[path = "../host_effect_ui/support/owner_step_owner.rs"]
mod owner;
use owner::contracts::RecordKind;
use owner::owner_step::{OwnerStepResult, RegisteredOwnerFlow};

fn complete_result_consumer(result: OwnerStepResult<'_>) {
    let expected = match &result {
        OwnerStepResult::Redisplay(_) => RecordKind::RedisplayStepResult,
        OwnerStepResult::DisplayQuery(_) => RecordKind::DisplayQueryStepResult,
        OwnerStepResult::Font(_) => RecordKind::FontStepResult,
        OwnerStepResult::Selection(_) => RecordKind::SelectionStepResult,
        OwnerStepResult::Callback(_) => RecordKind::CallbackStepResult,
        OwnerStepResult::Restore(_) => RecordKind::RestoreStepResult,
        OwnerStepResult::Flow(flow) => match flow {
            RegisteredOwnerFlow::Signal(_) => RecordKind::OwnerSignalFlow,
            RegisteredOwnerFlow::Throw(_) => RecordKind::OwnerThrowFlow,
            RegisteredOwnerFlow::ThreadBlocked(_) => RecordKind::OwnerBlockedFlow,
            RegisteredOwnerFlow::Shutdown(_) => RecordKind::OwnerShutdownFlow,
        },
        OwnerStepResult::ContractInjected(_) => RecordKind::CallbackStepResult,
    };
    assert_eq!(result.result_slot().kind(), expected);
}
fn main() {}
