use rquickjs::{AsyncContext, AsyncRuntime, Context, Module, Runtime};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[tokio::test]
async fn msvc_modules_and_async_host_calls() {
    let runtime = AsyncRuntime::new().unwrap();
    let context = AsyncContext::full(&runtime).await.unwrap();
    context
        .async_with(async |ctx| {
            ctx.globals()
                .set(
                    "host",
                    rquickjs::Function::new(
                        ctx.clone(),
                        rquickjs::function::Async(|value: i32| async move { value + 1 }),
                    )
                    .unwrap(),
                )
                .unwrap();
            let module = Module::declare(
                ctx.clone(),
                "probe",
                "export const answer = await host(41);",
            )
            .unwrap();
            let (module, promise) = module.eval().unwrap();
            promise.into_future::<()>().await.unwrap();
            assert_eq!(module.get::<_, i32>("answer").unwrap(), 42);
        })
        .await;
}

#[test]
fn msvc_interrupt_and_memory_limit() {
    let runtime = Runtime::new().unwrap();
    runtime.set_memory_limit(4 * 1024 * 1024);
    let context = Context::full(&runtime).unwrap();
    context.with(|ctx| {
        assert!(
            ctx.eval::<(), _>("const a = new Array(10000000).fill('memory');")
                .is_err()
        );
        ctx.catch();
    });
    let cancelled = Arc::new(AtomicBool::new(true));
    let token = cancelled.clone();
    runtime.set_interrupt_handler(Some(Box::new(move || token.load(Ordering::Acquire))));
    context.with(|ctx| assert!(ctx.eval::<(), _>("while (true) {}").is_err()));
}
