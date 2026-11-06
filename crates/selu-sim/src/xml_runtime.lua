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
    local name
    for i=1,128 do
        if not p then break end
        name=p:GetName(); if name then break end; p=p:GetParent()
    end
    return s and (string.gsub(s, '%$parent', function() return name or '' end))
end
local function relativeKey(frame,path)
    local object=frame
    for key in string.gmatch(path,'[^.]+') do
        if not object then return nil end
        if key=='$parent' then object=object:GetParent()
        else object=object[key] end
    end
    return object
end
local apply, create
local function invoke(fn,...)
    if __selu_secure_invoke then return __selu_secure_invoke(fn,...) end
    return fn(...)
end
local function nativeKind(kind)
    local seen={}
    while intrinsic[kind] do
        assert(not seen[kind],'XML intrinsic inheritance cycle: '..kind)
        seen[kind]=true; kind=intrinsic[kind]
    end
    return kind
end
local function inherit(f,s,seen)
    for _,name in ipairs(names(s)) do
        if seen[name] then error('XML template inheritance cycle: '..name) end
        local n=templates[name]
        if n then
            seen[name]=true; apply(f,n,seen); seen[name]=nil
        elseif type(rawget(_G,name))=='table' and rawget(_G,name).GetObjectType and rawget(_G,name):GetObjectType()=='Font' then
            f:SetFontObject(rawget(_G,name))
        else note('template '..name) end
    end
end
local function dimension(n)
    return n.children[1] or n
end
local function script(f,n,intrinsicOwner)
    local a=n.a
    if a.font then f:SetFont(a.font,tonumber(a.height) or 12,a.flags or '') end
    if a.justifyH then f:SetJustifyH(a.justifyH) end
    if a.justifyV then f:SetJustifyV(a.justifyV) end
    local fn
    if a.method then fn=function(self,...) return self[a.method](self,...) end
    elseif a['function'] then fn=lookup(a['function'])
    else
        local arguments={OnEvent='event',OnUpdate='elapsed',OnClick='button,down',
            OnMouseDown='button',OnMouseUp='button',OnValueChanged='value,userInput',
            OnSizeChanged='width,height',OnAttributeChanged='name,value',OnMouseWheel='delta',
            OnKeyDown='key',OnKeyUp='key',OnChar='text',OnTextChanged='userInput',
            OnCursorChanged='x,y,width,height',OnDragStart='button',OnDragStop='button'}
        local args='self,'..(arguments[n.tag] and arguments[n.tag]..',' or '')..'...'
        fn=assert(loadstring('return function('..args..')\n'..n.text..'\nend', '@XML/'..n.tag))()
    end
    if not fn then note('script '..(a['function'] or n.tag)); return end
    if intrinsicOwner and n.tag=='OnLoad' and not a.intrinsicOrder then
        -- Intrinsic OnLoad is a constructor, before derived instance setup.
        local loaded=rawget(f,'__selu_intrinsic_loaded') or {}
        rawset(f,'__selu_intrinsic_loaded',loaded)
        if not loaded[intrinsicOwner] then loaded[intrinsicOwner]=true; invoke(fn,f) end
        return
    end
    if intrinsicOwner and (a.intrinsicOrder=='precall' or a.intrinsicOrder=='postcall') then
        local handlers=rawget(f,'__selu_intrinsic_scripts') or {}
        rawset(f,'__selu_intrinsic_scripts',handlers)
        local slot=handlers[n.tag]
        if not slot then
            slot={precall={},postcall={},owners={},normal=f:GetScript(n.tag)}
            handlers[n.tag]=slot
            f.__scripts[n.tag]=function(...)
                for _,handler in ipairs(slot.precall) do invoke(handler,...) end
                if slot.normal then invoke(slot.normal,...) end
                for _,handler in ipairs(slot.postcall) do invoke(handler,...) end
            end
        end
        if not slot.owners[intrinsicOwner] then
            slot.owners[intrinsicOwner]=true
            table.insert(slot[a.intrinsicOrder],fn)
        end
        return
    end
    if a.inherit=='prepend' or a.inherit=='append' then
        local old=f:GetScript(n.tag)
        f:SetScript(n.tag,function(...)
            -- The attribute places the inherited handler, not the local one.
            -- FloatingChatFrame.xml's append handler sets chatFrame before
            -- inherited ChatFrameEditBoxMixin.OnLoad reads that field.
            if a.inherit=='append' then invoke(fn,...); if old then invoke(old,...) end
            else if old then invoke(old,...) end; invoke(fn,...) end
        end)
    else f:SetScript(n.tag,fn) end
end
local function animationGroup(frame,node)
    local group=frame:CreateAnimationGroup()
    local a=node.a
    if a.parentKey then frame[a.parentKey]=group end
    if a.parentArray then
        frame[a.parentArray]=frame[a.parentArray] or {}
        table.insert(frame[a.parentArray],group)
    end
    if a.name then _G[expand(a.name,frame)]=group end
    if a.looping then group:SetLooping(a.looping) end
    for _,child in ipairs(node.children) do
        if child.tag=='Scripts' then
            for _,s in ipairs(child.children) do script(group,s) end
        else
            local animation=group:CreateAnimation(child.tag)
            if animation then
                local v=child.a
                if v.parentKey then group[v.parentKey]=animation end
                for key,method in pairs({duration='SetDuration',order='SetOrder',startDelay='SetStartDelay',endDelay='SetEndDelay',fromAlpha='SetFromAlpha',toAlpha='SetToAlpha'}) do
                    if v[key] then animation[method](animation,tonumber(v[key])) end
                end
                if v.smoothing then animation:SetSmoothing(v.smoothing) end
                if v.targetKey then
                    local target=relativeKey(frame,v.targetKey)
                    if target then animation:SetTarget(target) else note('animation targetKey '..v.targetKey) end
                end
                for _,property in ipairs(child.children) do
                    if property.tag=='Offset' then
                        local offset=dimension(property).a
                        animation:SetOffset(tonumber(offset.x) or 0,tonumber(offset.y) or 0)
                    else note('animation element '..property.tag) end
                end
            end
        end
    end
end
apply=function(f,n,seen)
    if intrinsic[n.tag] then
        assert(not seen[n.tag],'XML intrinsic inheritance cycle: '..n.tag)
        seen[n.tag]=true; apply(f,templates[n.tag],seen); seen[n.tag]=nil
    end
    inherit(f,n.a.inherits,seen)
    for _,key in ipairs({'mixin','secureMixin'}) do
        for _,name in ipairs(names(n.a[key])) do
            local mix=lookup(name)
            if type(mix)=='table' then for k,v in pairs(mix) do f[k]=v end
            else note('mixin '..name) end
        end
    end
    local a=n.a
    if a.parentKey or a.parentArray then
        local parent=f:GetParent()
        if parent and a.parentKey then parent[a.parentKey]=f end
        if parent and a.parentArray then
            local array=parent[a.parentArray] or {}; parent[a.parentArray]=array
            local found=false; for _,child in ipairs(array) do if child==f then found=true; break end end
            if not found then array[#array+1]=f end
        end
    end
    if a.hidden~=nil then
        -- This is the initial XML state, before the instance's OnLoad.
        -- Inherited visibility scripts must not observe a half-built frame.
        local event=a.hidden=='true' and 'OnHide' or 'OnShow'
        local previous=f:GetScript(event); f:SetScript(event,nil)
        rawset(f,'__selu_initial_visibility',true)
        local ok,err=pcall(function() if a.hidden=='true' then f:Hide() else f:Show() end end)
        rawset(f,'__selu_initial_visibility',nil)
        f:SetScript(event,previous)
        if not ok then error(err,0) end
    end
    if a.frameStrata then f:SetFrameStrata(a.frameStrata) end
    if a.frameLevel then f:SetFrameLevel(tonumber(a.frameLevel)) end
    if a.alpha then f:SetAlpha(tonumber(a.alpha)) end
    if a.scale then f:SetScale(tonumber(a.scale)) end
    if a.enableMouse then f:EnableMouse(a.enableMouse=='true') end
    if a.id then f:SetID(tonumber(a.id)) end
    if a.orientation then f:SetOrientation(a.orientation) end
    if a.drawLayer then f:SetDrawLayer(a.drawLayer,tonumber(a.subLevel) or 0) end
    if a.minValue or a.maxValue then f:SetMinMaxValues(tonumber(a.minValue) or 0,tonumber(a.maxValue) or 1) end
    if a.defaultValue then f:SetValue(tonumber(a.defaultValue)) end
    if a.setAllPoints=='true' then f:SetAllPoints(f:GetParent()) end
    if a.text then f:SetText(lookup(a.text) or a.text) end
    if a.file then f:SetTexture(a.file) end
    if a.atlas then f:SetAtlas(a.atlas,a.useAtlasSize=='true') end
    for _,c in ipairs(n.children) do
        local b=c.a
        if c.tag=='Size' then
            local d=dimension(c).a; f:SetSize(tonumber(d.x) or 0,tonumber(d.y) or 0)
        elseif c.tag=='FontHeight' then
            local d=dimension(c).a; local path,_,flags=f:GetFont()
            f:SetFont(path,tonumber(d.val or d.value) or 12,flags)
        elseif c.tag=='Anchors' then
            for _,anchor in ipairs(c.children) do
                local v=anchor.a; local x,y=tonumber(v.x) or 0,tonumber(v.y) or 0
                for _,offset in ipairs(anchor.children) do
                    local d=dimension(offset).a; x=tonumber(d.x) or 0; y=tonumber(d.y) or 0
                end
                local relative=v.relativeKey and relativeKey(f,v.relativeKey) or lookup(expand(v.relativeTo,f:GetParent()))
                if v.relativeKey and not relative then note('relativeKey '..v.relativeKey) end
                f:SetPoint(v.point or 'CENTER',relative or f:GetParent(),v.relativePoint or v.point or 'CENTER',x,y)
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
        elseif c.tag=='Scripts' then for _,s in ipairs(c.children) do script(f,s,n.a.intrinsic=='true' and n.a.name) end
        elseif c.tag=='Animations' then
            for _,group in ipairs(c.children) do
                if group.tag=='AnimationGroup' then animationGroup(f,group)
                else note('animation element '..group.tag) end
            end
        elseif c.tag=='Frames' then for _,child in ipairs(c.children) do create(child,f) end
        elseif c.tag=='ScrollChild' then
            if c.children[1] then f:SetScrollChild(create(c.children[1],f)) end
        elseif c.tag=='ButtonText' then f:SetFontString(create(c,f))
        elseif c.tag=='NormalFont' or c.tag=='HighlightFont' or c.tag=='DisabledFont' then
            local setter=f['Set'..c.tag..'Object']
            if setter and b.style then setter(f,lookup(b.style)) end
        elseif c.tag=='Layers' then
            for _,layer in ipairs(c.children) do
                for _,child in ipairs(layer.children) do create(child,f,layer.a.level or 'ARTWORK') end
            end
        elseif c.tag=='Color' then
            local r,g,bl,al=tonumber(b.r) or 1,tonumber(b.g) or 1,tonumber(b.b) or 1,tonumber(b.a) or 1
            if n.tag=='FontString' or n.tag=='Font' then f:SetTextColor(r,g,bl,al) else f:SetVertexColor(r,g,bl,al) end
        elseif c.tag=='TexCoords' then f:SetTexCoord(tonumber(b.left) or 0,tonumber(b.right) or 1,tonumber(b.top) or 0,tonumber(b.bottom) or 1)
        elseif c.tag=='BarTexture' then f:SetStatusBarTexture(create(c,f))
        elseif c.tag=='BarColor' then f:SetStatusBarColor(tonumber(b.r) or 1,tonumber(b.g) or 1,tonumber(b.b) or 1,tonumber(b.a) or 1)
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
    if n.tag=='MaskTexture' then f=parent:CreateMaskTexture(name,layer)
    elseif n.tag=='Texture' or string.match(n.tag,'Texture$') then f=parent:CreateTexture(name,layer)
    elseif n.tag=='FontString' or n.tag=='ButtonText' then f=parent:CreateFontString(name,layer)
    elseif n.tag=='Font' then f=CreateFont(name)
    else f=native(nativeKind(n.tag),name,parent) end
    apply(f,n,{})
    if n.tag~='Texture' and n.tag~='FontString' and n.tag~='Font' then
        local onload=f.__scripts.OnLoad; if onload then invoke(onload,f) end
    end
    return f
end
function __selu_xml_apply(f,s)
    inherit(f,s,{})
    local onload=f.__scripts.OnLoad; if onload then invoke(onload,f) end
end
function __selu_xml_intrinsic(kind) if intrinsic[kind] then return nativeKind(kind) end end
function __selu_xml_declare(n)
    if n.tag=='FontFamily' then
        for _,member in ipairs(n.children) do
            if member.a.alphabet=='roman' and member.children[1] then
                local font=member.children[1]
                font.a.name=n.a.name; create(font,nil); return
            end
        end
        note('font alphabet '..tostring(n.a.name)); return
    end
    if n.tag=='Font' then create(n,nil); return end
    if n.a.virtual=='true' or n.a.intrinsic=='true' then
        templates[n.a.name]=n
        if n.a.intrinsic=='true' then intrinsic[n.a.name]=n.tag end
    else create(n,nil) end
end
