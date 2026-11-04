use selu_sim::session::{HostCommand, HostValue};
use selu_sim::{Session, SessionConfig, SimValue, TraceKind};
fn session() -> Session {
    Session::new(SessionConfig {
        trace: true,
        seed: 12345,
        ..Default::default()
    })
    .unwrap()
}
#[test]
fn error_objects_do_not_leak_addresses() {
    let a = session();
    let b = session();
    // Lua code must retain the original error object, including coroutine errors.
    let identity = "local e={};local ok,err=pcall(error,e);assert(not ok and err==e);local c=coroutine.create(function()error(e)end);ok,err=coroutine.resume(c);assert(not ok and err==e)";
    a.eval(identity).unwrap();
    b.eval(identity).unwrap();
    let x = a.eval("error({})").unwrap_err().to_string();
    let y = b.eval("error({})").unwrap_err().to_string();
    assert_eq!(x, y);
    assert!(!x.contains("table:"));
    assert_eq!(a.logs(), b.logs());
    let errors = |s: &Session| {
        s.trace_since(0)
            .into_iter()
            .filter(|r| r.kind == TraceKind::Error)
            .collect::<Vec<_>>()
    };
    assert!(!errors(&a).is_empty());
    assert_eq!(errors(&a), errors(&b));
}
#[test]
fn protected_tostring_keeps_metamethod() {
    let s = session();
    s.eval("local o=setmetatable({}, {__metatable='protected', __tostring=function()return 'custom' end});assert(tostring(o)=='custom');assert(getmetatable(o)=='protected')").unwrap();
}
#[test]
fn replay_keeps_binary_strings() {
    let s = session();
    let table = s.lua().create_table().unwrap();
    table
        .raw_set(s.lua().create_string([0xff, 0, 0x61]).unwrap(), 1)
        .unwrap();
    table
        .raw_set(s.lua().create_string([0xfe, 0, 0x61]).unwrap(), 2)
        .unwrap();
    s.fire(
        "BINARY",
        vec![
            SimValue::String(s.lua().create_string([0xff, 0, 0x61]).unwrap()),
            SimValue::Table(table),
        ],
    )
    .unwrap();
    let bytes = serde_json::to_vec(&s.host_commands()).unwrap();
    let commands: Vec<HostCommand> = serde_json::from_slice(&bytes).unwrap();
    let HostCommand::Fire { args, .. } = &commands[0] else {
        panic!("not fire")
    };
    let replay = session();
    let value = args[0].to_lua(replay.lua()).unwrap();
    let SimValue::String(value) = value else {
        panic!("not a string")
    };
    assert_eq!(value.as_bytes().as_ref(), &[0xff, 0, 0x61]);
    let SimValue::Table(table) = args[1].to_lua(replay.lua()).unwrap() else {
        panic!("not a table")
    };
    assert_eq!(
        table
            .raw_get::<i64>(replay.lua().create_string([0xff, 0, 0x61]).unwrap())
            .unwrap(),
        1
    );
    assert_eq!(
        table
            .raw_get::<i64>(replay.lua().create_string([0xfe, 0, 0x61]).unwrap())
            .unwrap(),
        2
    );
    replay.eval("local f=CreateFrame('Frame');f:RegisterEvent('BINARY');f:SetScript('OnEvent',function(_,_,value)assert(string.byte(value,1)==255 and string.byte(value,2)==0 and string.byte(value,3)==97)end)").unwrap();
    replay
        .fire(
            "BINARY",
            args.iter()
                .map(|v| v.to_lua(replay.lua()).unwrap())
                .collect(),
        )
        .unwrap();
    assert!(matches!(args[0], HostValue::Bytes { .. }));
}
