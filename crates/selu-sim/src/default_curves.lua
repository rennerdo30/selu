-- Baseline native curve objects used by CurveConstants and health colours.
-- Shapes follow Blizzard's LuaCurveObject{Base}/LuaColorCurveObject API data.
local warned={}
local function curve(colour)
    local points,kind={},Enum and Enum.LuaCurveType and Enum.LuaCurveType.Linear or 0
    local object={}
    local function public(n)
        assert(not issecretvalue(n),'secret curve values are not simulated')
        assert(type(n)=='number','curve number expected')
        return n
    end
    local function output(y) return colour and CreateColor(unpack(y)) or y end
    function object:AddPoint(x,y)
        public(x)
        if colour then
            local r,g,b,a=y:GetRGBA(); y={public(r),public(g),public(b),public(a)}
        else public(y) end
        for _,point in ipairs(points) do if point.x==x then point.y=y; return end end
        points[#points+1]={x=x,y=y}; table.sort(points,function(a,b) return a.x<b.x end)
    end
    function object:SetType(value)
        public(value); kind=value
        local types=Enum and Enum.LuaCurveType or {}
        if value~=types.Linear and value~=types.Step and not warned[value] then
            warned[value]=true; __selu_note_gap('curve interpolation '..value..' (linear approximation)')
        end
    end
    function object:GetType() return kind end
    function object:HasSecretValues() return false end
    function object:ClearPoints() points={} end
    function object:GetPointCount() return #points end
    function object:GetPoint(index)
        local p=points[index]; return p and {x=p.x,y=output(p.y)} or nil
    end
    function object:GetPoints()
        local result={}; for i=1,#points do result[i]=self:GetPoint(i) end; return result
    end
    function object:RemovePoint(index) table.remove(points,index) end
    function object:Copy()
        local copy=curve(colour); copy:SetType(kind)
        for _,p in ipairs(points) do copy:AddPoint(p.x,output(p.y)) end
        return copy
    end
    function object:Evaluate(x)
        public(x)
        if #points==0 then return colour and CreateColor(0,0,0,1) or 0 end
        if x<=points[1].x then return output(points[1].y) end
        for i=2,#points do
            local a,b=points[i-1],points[i]
            if x<=b.x then
                local types=Enum and Enum.LuaCurveType or {}
                if types.Step and kind==types.Step and x<b.x then return output(a.y) end
                local t=(x-a.x)/(b.x-a.x)
                if not colour then return a.y+(b.y-a.y)*t end
                local c={}; for j=1,4 do c[j]=a.y[j]+(b.y[j]-a.y[j])*t end
                return CreateColor(unpack(c))
            end
        end
        return output(points[#points].y)
    end
    function object:EvaluateUnpacked(x)
        local result=self:Evaluate(x); if colour then return result:GetRGBA() end; return result
    end
    return object
end
C_CurveUtil=C_CurveUtil or {}
local standins=rawget(_G,'__selu_native_standins') or {}
if not C_CurveUtil.CreateCurve or standins['C_CurveUtil.CreateCurve']==C_CurveUtil.CreateCurve then
    function C_CurveUtil.CreateCurve() return curve(false) end
end
if not C_CurveUtil.CreateColorCurve or standins['C_CurveUtil.CreateColorCurve']==C_CurveUtil.CreateColorCurve then
    function C_CurveUtil.CreateColorCurve() return curve(true) end
end
local function booleanValue(value,yes,no)
    assert(canaccessvalue(value),'secret curve conditions are not simulated')
    return value and yes or no
end
if not C_CurveUtil.EvaluateColorFromBoolean or standins['C_CurveUtil.EvaluateColorFromBoolean']==C_CurveUtil.EvaluateColorFromBoolean then C_CurveUtil.EvaluateColorFromBoolean=booleanValue end
if not C_CurveUtil.EvaluateColorValueFromBoolean or standins['C_CurveUtil.EvaluateColorValueFromBoolean']==C_CurveUtil.EvaluateColorValueFromBoolean then C_CurveUtil.EvaluateColorValueFromBoolean=booleanValue end
