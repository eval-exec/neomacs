use crate::isolate::{IsolateBoot, boot};
use crate::transfer::{self, Call, Transfer};
use crate::{ElispFactoryFn, WorkerConfig, WorkerRuntime};
use neovm_core::TaskScheduler;
use neovm_host_abi::{LispValue, TaskOptions};
use std::sync::Arc;

#[test]
fn cached_bootstrap_owner_executes_worker_call_after_startup() {
    let boot_recipe = IsolateBoot::CachedBootstrap;
    let factory: Arc<ElispFactoryFn> = Arc::new(move || boot::context(&boot_recipe));
    let runtime = WorkerRuntime::with_isolate_executor(
        WorkerConfig {
            threads: 1,
            queue_capacity: 8,
        },
        factory,
    );
    let owners = runtime.try_start_workers().expect("worker owner starts");
    let call = Call {
        function: Transfer::Symbol("+".to_string()),
        args: vec![Transfer::Fixnum(20), Transfer::Fixnum(22)],
    };
    let task = runtime
        .spawn_task(
            LispValue {
                bytes: transfer::encode_call(&call),
            },
            TaskOptions::default(),
        )
        .expect("worker task queues");
    let result = TaskScheduler::task_await(&runtime, task, None).expect("worker call completes");
    assert_eq!(
        transfer::decode(&result.bytes).expect("worker result decodes"),
        Transfer::Fixnum(42)
    );
    runtime.forget_task(task);
    runtime.close();
    for owner in owners {
        owner.join().expect("worker owner shuts down");
    }
}
