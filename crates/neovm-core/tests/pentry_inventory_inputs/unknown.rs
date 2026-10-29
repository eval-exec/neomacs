fn register(ctx: &mut Context) {
    ctx.register_subr(world_subr!(SubrSpec::future_constructor("pentry-unknown", no_op)));
}
