fn register(ctx: &mut Context) {
    ctx.register_subr(world_subr!(SubrSpec::fixed0("pentry-duplicate", no_op)));
    ctx.register_subr(world_subr!(SubrSpec::fixed0("pentry-duplicate", no_op)));
}
