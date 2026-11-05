use selu_sim::Sim;
#[test]
fn original_error_and_single_capture_survive_nested_native_calls() {
    let sim = Sim::new().unwrap();
    sim.eval(
        r#"
        original = setmetatable({}, { __tostring = function() return 'original failure' end })
        calls = 0
        seterrorhandler(function(e) assert(e == original); calls=calls+1; error('sink failed') end)
        local f=CreateFrame('Frame'); f:Hide()
        f:SetScript('OnShow', function() error(original) end)
        C_Timer.After(.1, function() f:Show() end)
    "#,
    )
    .unwrap();
    sim.advance_time(0.1);
    sim.eval("assert(calls == 1)").unwrap();
    let failures = sim.sim().ctx().runtime_failures();
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert!(format!("{failures:?}").contains("original failure"));
}
#[test]
fn caught_errors_stay_local_and_new_errors_get_independent_delivery() {
    let sim = Sim::new().unwrap();
    sim.eval(
        r#"
        calls=0; seterrorhandler(function() calls=calls+1 end)
        local f=CreateFrame('Frame'); f:Hide(); f:SetScript('OnShow', function() error('first') end)
        C_Timer.After(.1, function() pcall(function() error('caught locally') end) end)
        C_Timer.After(.1, function() pcall(function() f:Show() end); error('second') end)
    "#,
    )
    .unwrap();
    sim.advance_time(0.1);
    sim.eval("assert(calls == 2)").unwrap();
    assert_eq!(sim.sim().ctx().runtime_failures().len(), 2);
}
