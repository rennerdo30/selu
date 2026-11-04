use selu_sim::Sim;
use std::{
    io::Write,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
#[test]
fn visibility_pingpong_is_bounded_in_child() {
    if std::env::var_os("SELU_CALLBACK_CHILD").is_some() {
        let s = Sim::new().unwrap();
        s.ctx().set_budget(100000);
        let result=s.eval("local f=CreateFrame('Frame');local count=0;f:SetScript('OnShow',function(self) count=count+1;if count<500 then self:Hide() end end);f:SetScript('OnHide',function(self) count=count+1;if count<500 then self:Show() end end);f:Hide()");
        assert!(result.is_err());
        assert!(
            s.logs()
                .iter()
                .any(|l| l.contains("error: script callbacks nested too deeply")),
            "{:?}",
            s.logs()
        );
        s.eval("assert(1+1==2)").unwrap();
        return;
    }
    let receipt = std::env::temp_dir().join("selu-S1-visibility-child-pids.log");
    let stderr = std::env::temp_dir().join(format!(
        "selu-S1-visibility-child-{}.err",
        std::process::id()
    ));
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "visibility_pingpong_is_bounded_in_child",
            "--nocapture",
        ])
        .env("SELU_CALLBACK_CHILD", "1")
        .stdout(Stdio::null())
        .stderr(std::fs::File::create(&stderr).unwrap())
        .spawn()
        .unwrap();
    eprintln!("callback test child PID {}", child.id());
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(receipt)
    {
        let _ = writeln!(
            f,
            "parent {}, child {}, stderr {}",
            std::process::id(),
            child.id(),
            stderr.display()
        );
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(
                status.success(),
                "child exit {status}: {}",
                std::fs::read_to_string(stderr).unwrap()
            );
            break;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("callback child timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn flat_visibility_callbacks_share_outer_budget() {
    let s = Sim::new().unwrap();
    s.ctx().set_budget(100000);
    let result=s.eval("f=CreateFrame('Frame');count=0;f:SetScript('OnShow',function() count=count+1 end);f:SetScript('OnHide',function() count=count+1 end);for i=1,6000 do f:Hide();f:Show() end");
    let count: i64 = s.lua().globals().get("count").unwrap();
    assert!(
        result.is_err() && count <= 10000 && s.logs().iter().any(|l| l.contains("budget")),
        "count={count}, logs={:?}",
        s.logs()
    );
    s.eval("assert(1+1==2)").unwrap();
}
#[test]
fn event_subscribers_flush_between_handlers() {
    let s = Sim::new().unwrap();
    s.eval("sized=false;a=CreateFrame('Frame');a:SetPoint('CENTER');a:SetSize(10,10);b=CreateFrame('Frame');a:RegisterEvent('TEST');b:RegisterEvent('TEST');a:SetScript('OnSizeChanged',function() sized=true end);a:SetScript('OnEvent',function() a:SetWidth(20);print('first') end);b:SetScript('OnEvent',function() print('second',sized) end)").unwrap();
    s.eval("sized=false").unwrap();
    s.fire("TEST", vec![]).unwrap();
    assert_eq!(s.prints(), ["first", "second\ttrue"]);
}
#[test]
fn hook_continuations_flush_between_handlers() {
    let s = Sim::new().unwrap();
    s.eval("sized=false;a=CreateFrame('Frame');a:SetPoint('CENTER');a:SetSize(10,10);a:RegisterEvent('TEST');a:SetScript('OnSizeChanged',function() sized=true end);a:SetScript('OnEvent',function() a:SetWidth(20) end);a:HookScript('OnEvent',function() print(sized) end)").unwrap();
    s.eval("sized=false").unwrap();
    s.fire("TEST", vec![]).unwrap();
    assert_eq!(s.prints(), ["true"]);
}
