-- This registry is private to the simulator VM. Blizzard sources stay external.
if __selu_xml_declare then return end
local templates, intrinsic, warned = {}, {}, {}
local function note(s)
    if not warned[s] then warned[s]=true; print('not simulated: XML '..s) end
end
local function names(s)
    local t={}; for name in string.gmatch(s or '', '[^,%s]+') do t[#t+1]=name end
    return t
end
local function lookup(s)
    if not s or s=='' then return nil end
    local v=_G
    for key in string.gmatch(s or '', '[^.]+') do
        if type(v)~='table' then return nil end
        v=v[key]
    end
    return v
end
local function expand(s,p)
    return s and (string.gsub(s, '%$parent', function() return p and p:GetName() or '' end))
end
local apply, create
local function inherit(f,s,seen)
    for _,name in ipairs(names(s)) do
        if seen[name] then error('XML template inheritance cycle: '..name) end
        local n=templates[name]
        if n then
            seen[name]=true; apply(f,n,seen); seen[name]=nil
        else note('template '..name) end
    end
end
local function dimension(n)
    return n.children[1] or n
end
local function script(f,n)
    local a=n.a
    local fn
    if a.method then fn=function(self,...) return self[a.method](self,...) end
    elseif a['function'] then fn=lookup(a['function'])
    else
        local args='self,...'
        if n.tag=='OnEvent' then args='self,event,...'
        elseif n.tag=='OnUpdate' then args='self,elapsed,...'
        elseif n.tag=='OnClick' or n.tag=='OnMouseDown' or n.tag=='OnMouseUp' then args='self,button,...' end
        fn=assert(loadstring('return function('..args..')\n'..n.text..'\nend', '@XML/'..n.tag))()
    end
    if not fn then note('script '..(a['function'] or n.tag)); return end
    if a.inherit=='prepend' then
        local old=f:GetScript(n.tag)
        f:SetScript(n.tag,function(...) fn(...); if old then old(...) end end)
    elseif a.inherit=='append' then f:HookScript(n.tag,fn)
    else f:SetScript(n.tag,fn) end
end
apply=function(f,n,seen)
    inherit(f,n.a.inherits,seen)
    for _,key in ipairs({'mixin','secureMixin'}) do
        for _,name in ipairs(names(n.a[key])) do
            local mix=lookup(name)
            if type(mix)=='table' then for k,v in pairs(mix) do f[k]=v end
            else note('mixin '..name) end
        end
    end
    local a=n.a
    if a.hidden=='true' then f:Hide() end
    if a.frameStrata then f:SetFrameStrata(a.frameStrata) end
    if a.frameLevel then f:SetFrameLevel(tonumber(a.frameLevel)) end
    if a.alpha then f:SetAlpha(tonumber(a.alpha)) end
    if a.scale then f:SetScale(tonumber(a.scale)) end
    if a.enableMouse then f:EnableMouse(a.enableMouse=='true') end
    if a.text then f:SetText(lookup(a.text) or a.text) end
    if a.file then f:SetTexture(a.file) end
    if a.atlas then f:SetAtlas(a.atlas,a.useAtlasSize=='true') end
    for _,c in ipairs(n.children) do
        local b=c.a
        if c.tag=='Size' then
            local d=dimension(c).a; f:SetSize(tonumber(d.x) or 0,tonumber(d.y) or 0)
        elseif c.tag=='Anchors' then
            for _,anchor in ipairs(c.children) do
                local v=anchor.a; local x,y=tonumber(v.x) or 0,tonumber(v.y) or 0
                for _,offset in ipairs(anchor.children) do
                    local d=dimension(offset).a; x=tonumber(d.x) or 0; y=tonumber(d.y) or 0
                end
                f:SetPoint(v.point or 'CENTER',lookup(expand(v.relativeTo,f:GetParent())) or f:GetParent(),v.relativePoint or v.point or 'CENTER',x,y)
            end
        elseif c.tag=='KeyValues' then
            for _,kv in ipairs(c.children) do
                local v=kv.a; local value=v.value
                if v.type=='number' then value=tonumber(value)
                elseif v.type=='boolean' then value=value=='true'
                elseif v.type=='global' then value=lookup(value) end
                local owner=f; local keys={}
                for key in string.gmatch(v.key or '', '[^.]+') do keys[#keys+1]=key end
                for i=1,#keys-1 do owner[keys[i]]=owner[keys[i]] or {}; owner=owner[keys[i]] end
                if #keys>0 then owner[keys[#keys]]=value end
            end
        elseif c.tag=='Scripts' then for _,s in ipairs(c.children) do script(f,s) end
        elseif c.tag=='Frames' then for _,child in ipairs(c.children) do create(child,f) end
        elseif c.tag=='Layers' then
            for _,layer in ipairs(c.children) do
                for _,child in ipairs(layer.children) do create(child,f,layer.a.level or 'ARTWORK') end
            end
        elseif c.tag=='Color' then
            local r,g,bl,al=tonumber(b.r) or 1,tonumber(b.g) or 1,tonumber(b.b) or 1,tonumber(b.a) or 1
            if n.tag=='FontString' then f:SetTextColor(r,g,bl,al) else f:SetVertexColor(r,g,bl,al) end
        elseif c.tag=='TexCoords' then f:SetTexCoord(tonumber(b.left) or 0,tonumber(b.right) or 1,tonumber(b.top) or 0,tonumber(b.bottom) or 1)
        elseif string.match(c.tag,'Texture$') then
            local texture=create(c,f)
            local setter=f['Set'..c.tag]; if setter then setter(f,texture) end
        else note('element '..c.tag) end
    end
end
local native=CreateFrame
create=function(n,parent,layer)
    local a=n.a
    parent=lookup(a.parent) or parent
    local name=expand(a.name,parent)
    local f
    if n.tag=='Texture' or string.match(n.tag,'Texture$') then f=parent:CreateTexture(name,layer)
    elseif n.tag=='FontString' then f=parent:CreateFontString(name,layer)
    elseif n.tag=='Font' then f=CreateFont(name)
    else f=native(intrinsic[n.tag] or n.tag,name,parent) end
    if parent and a.parentKey then parent[a.parentKey]=f end
    if parent and a.parentArray then
        local array=parent[a.parentArray] or {}; parent[a.parentArray]=array; array[#array+1]=f
    end
    apply(f,n,{})
    if n.tag~='Texture' and n.tag~='FontString' and n.tag~='Font' then
        local onload=f:GetScript('OnLoad'); if onload then onload(f) end
    end
    return f
end
function __selu_xml_apply(f,s) inherit(f,s,{}) end
function __selu_xml_declare(n)
    if n.a.virtual=='true' or n.a.intrinsic=='true' then
        templates[n.a.name]=n
        if n.a.intrinsic=='true' then intrinsic[n.a.name]=n.tag end
    else create(n,nil) end
end
