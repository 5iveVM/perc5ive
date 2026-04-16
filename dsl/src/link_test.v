// Minimal DSL file for testing the sentinel-based linker pipeline.
//
// Each `pub` function below is a sentinel stub: its body returns a unique u64
// constant that the linker can find and rewrite to a CALL into appended
// hand-written bytecode. The surrounding DSL structure (account types,
// parameter signatures) exercises the real five-dsl-compiler just enough
// to prove the .fbin format is stable for linker consumption.

account State {
    value_a: u64;
    value_b: u64;
}

// Sentinel constants — each identifies a unique hand-written bytecode target.
// The linker scans for `PUSH_U64 <VLE sentinel>; RETURN_VALUE` in the .fbin.
sentinel_cmp() -> u64    { return 18369614221190020097; }  // 0xFEED_FACE_DEAD_C001
sentinel_add() -> u64    { return 18369614221190020098; }  // 0xFEED_FACE_DEAD_C002
sentinel_sub() -> u64    { return 18369614221190020099; }  // 0xFEED_FACE_DEAD_C003

// Stub: the linker rewrites this body to CALL into the appended CMP callee.
pub do_cmp(s: State) -> u64 {
    return sentinel_cmp();
}

// Stub: the linker rewrites this body to CALL into the appended ADD callee.
pub do_add(s: State) -> u64 {
    return sentinel_add();
}

// Stub: the linker rewrites this body to CALL into the appended SUB callee.
pub do_sub(s: State) -> u64 {
    return sentinel_sub();
}
