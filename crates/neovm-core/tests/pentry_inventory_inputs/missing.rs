fn register(ctx: &mut Context) {
    ctx.register_subr(SubrSpec::fixed0("pentry-missing", no_op));
}
