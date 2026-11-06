-- Animation lifecycle runs on the simulator's existing 60Hz OnUpdate clock.
return function(target, note, translate)
    local g={animations={},scripts={},looping="NONE",state="stopped",elapsed=0}
    local driver=CreateFrame("Frame")
    driver:Hide()
    local function fire(name,...) if g.scripts[name] then g.scripts[name](g,...) end end
    function g:SetScript(name,f) self.scripts[name]=f end
    function g:GetScript(name) return self.scripts[name] end
    function g:SetLooping(v) assert(v=="NONE" or v=="REPEAT" or v=="BOUNCE");self.looping=v end
    function g:GetLooping() return self.looping end
    function g:IsPlaying() return self.state=="playing" end
    function g:IsPaused() return self.state=="paused" end
    function g:IsStopped() return self.state=="stopped" end
    function g:CreateAnimation(kind)
        if kind~="Alpha" and kind~="Translation" then note("Animation:"..tostring(kind));return nil end
        local a={kind=kind,x=0,y=0,duration=0,delay=0,finishDelay=0,order=1,from=0,to=1,smoothing="NONE",target=target}
        function a:SetDuration(v) assert(v>=0);self.duration=v end
        function a:GetDuration() return self.duration end
        function a:SetStartDelay(v) assert(v>=0);self.delay=v end
        function a:GetStartDelay() return self.delay end
        function a:SetEndDelay(v) assert(v>=0);self.finishDelay=v end
        function a:GetEndDelay() return self.finishDelay end
        function a:SetOrder(v) self.order=v end
        function a:GetOrder() return self.order end
        function a:SetFromAlpha(v) self.from=v end
        function a:GetFromAlpha() return self.from end
        function a:SetToAlpha(v) self.to=v end
        function a:GetToAlpha() return self.to end
        function a:SetSmoothing(v) assert(v=="NONE" or v=="IN" or v=="OUT" or v=="IN_OUT");self.smoothing=v end
        function a:GetSmoothing() return self.smoothing end
        function a:SetOffset(x,y) self.x=x;self.y=y end
        function a:GetOffset() return self.x,self.y end
        function a:SetTarget(v) self.target=v end
        function a:GetTarget() return self.target end
        function a:GetParent() return g end
        table.insert(self.animations,a);return a
    end
    function g:GetDuration()
        local orders={};for _,a in ipairs(self.animations) do orders[a.order]=math.max(orders[a.order] or 0,a.delay+a.duration+a.finishDelay) end
        local duration=0;for _,v in pairs(orders) do duration=duration+v end;return duration
    end
    local originals={}
    local function restore() for t,v in pairs(originals) do t:SetAlpha(v);translate(t,0,0) end end
    function g:Play()
        if self.state=="playing" then return end
        if self.state~="paused" then
            self.elapsed=0;self.reverse=false;originals={}
            for _,a in ipairs(self.animations) do originals[a.target]=a.target:GetAlpha() end
        end
        self.state="playing";driver:Show();fire("OnPlay")
    end
    function g:Pause() if self.state=="playing" then self.state="paused";driver:Hide() end end
    function g:Stop() self.state="stopped";driver:Hide();self.elapsed=0;restore() end
    driver:SetScript("OnUpdate",function(_,dt)
        if g.state~="playing" then return end
        local duration=g:GetDuration()
        if duration<=0 then g:Stop();fire("OnFinished");return end
        g.elapsed=g.elapsed+dt
        local finished=false
        if g.elapsed>=duration then
            if g.looping=="NONE" then g.elapsed=duration;finished=true
            else
                g.elapsed=g.elapsed-duration
                if g.looping=="BOUNCE" then g.reverse=not g.reverse end
                fire("OnLoop",g.reverse and "REVERSE" or "FORWARD")
            end
        end
        local time=g.reverse and duration-g.elapsed or g.elapsed
        local orders={};for _,a in ipairs(g.animations) do orders[a.order]=math.max(orders[a.order] or 0,a.delay+a.duration+a.finishDelay) end
        local keys={};for k in pairs(orders) do table.insert(keys,k) end;table.sort(keys)
        local start=0
        for _,order in ipairs(keys) do
            for _,a in ipairs(g.animations) do if a.order==order and time>=start then
                local p=a.duration==0 and 1 or math.max(0,math.min(1,(time-start-a.delay)/a.duration))
                if a.smoothing=="IN" then p=p*p elseif a.smoothing=="OUT" then p=1-(1-p)^2 elseif a.smoothing=="IN_OUT" then p=p*p*(3-2*p) end
                if a.kind=="Alpha" then a.target:SetAlpha(a.from+(a.to-a.from)*p) else translate(a.target,a.x*p,a.y*p) end
            end end
            start=start+orders[order]
        end
        if finished then g.state="stopped";driver:Hide();restore();fire("OnFinished",true) end
    end)
    return g
end
