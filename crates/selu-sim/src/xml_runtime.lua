-- This registry is private to the simulator VM. Blizzard sources stay external.
if rawget(_G,'__selu_xml_declare') then return end
local templates, intrinsic, warned = {}, {}, {}
local inheritedParents={}
local function note(s)
    if not warned[s] then warned[s]=true; print('not simulated: XML '..s) end
end
local parsedNames={}
local emptyNames={}
local function names(s)
    if not s or s=='' then return emptyNames end
    if parsedNames[s] then return parsedNames[s] end
    local t={}; for name in string.gmatch(s or '', '[^,%s]+') do t[#t+1]=name end
    parsedNames[s]=t
    return t
end
local function lookup(s)
    if not s or s=='' then return nil end
    if not string.find(s,'.',1,true) then return _G[s] end
    local v=_G
    for key in string.gmatch(s or '', '[^.]+') do
        if type(v)~='table' then return nil end
        v=v[key]
    end
    return v
end
local function expand(s,p)
    if not s or not string.find(s,'$parent',1,true) then return s end
    local name
    for i=1,128 do
        if not p then break end
        name=p:GetName(); if name then break end; p=p:GetParent()
    end
    return (string.gsub(s, '%$parent', function() return name or '' end))
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
local constructionDepth, deferredAnchors=0,{}
local function anchorPoint(frame,value,x,y,defer)
    local point=value.point or 'CENTER'
    local relative=value.relativeKey and relativeKey(frame,value.relativeKey)
    if not relative and value.relativeTo then
        relative=value.relativeTo=='$parent' and frame:GetParent() or lookup(expand(value.relativeTo,string.find(value.relativeTo,'$parent',1,true) and frame:GetParent()))
    end
    if deferredAnchors[frame] then deferredAnchors[frame][point]=nil end
    if (value.relativeKey or value.relativeTo) and not relative then
        if defer then
            deferredAnchors[frame]=deferredAnchors[frame] or {}
            deferredAnchors[frame][point]={value,x,y}
        else note('unresolved anchor '..(value.relativeKey or value.relativeTo)) end
        return
    end
    frame:SetPoint(point,relative or frame:GetParent(),value.relativePoint or point,x,y)
end
local function invoke(fn,...)
    if __selu_secure_invoke then return __selu_secure_invoke(fn,...) end
    return fn(...)
end
local function nativeKind(kind)
    if not intrinsic[kind] then return kind end
    local seen={}
    while intrinsic[kind] do
        assert(not seen[kind],'XML intrinsic inheritance cycle: '..kind)
        seen[kind]=true; kind=intrinsic[kind]
    end
    return kind
end
local function parentAttribute(node,seen)
    if inheritedParents[node]~=nil then return inheritedParents[node] or nil end
    seen=seen or {}
    assert(not seen[node],'XML template inheritance cycle')
    seen[node]=true
    local parent
    if intrinsic[node.tag] then parent=parentAttribute(templates[node.tag],seen) end
    for _,name in ipairs(names(node.a.inherits)) do
        if templates[name] then parent=parentAttribute(templates[name],seen) or parent end
    end
    parent=node.a.parent or parent
    seen[node]=nil;inheritedParents[node]=parent or false
    return parent
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
local templateInfo={}
local function resolveInfo(name,seen)
    if templateInfo[name] then return templateInfo[name] end
    local n=templates[name];if not n then return nil end
    assert(not seen[name],'XML template metadata cycle');seen[name]=true
    local info={width=0,height=0,type=nativeKind(n.tag),keyValues={},sourceLocation='external XML',inherits=n.a.inherits}
    local parents={};if intrinsic[n.tag] then parents[#parents+1]=n.tag end
    for _,parent in ipairs(names(n.a.inherits)) do parents[#parents+1]=parent end
    for _,parent in ipairs(parents) do
        local inherited=resolveInfo(parent,seen)
        if inherited then info.width=inherited.width;info.height=inherited.height end
    end
    for _,child in ipairs(n.children) do
        if child.tag=='Size' then local d=dimension(child).a;info.width=tonumber(d.x) or 0;info.height=tonumber(d.y) or 0
        elseif child.tag=='KeyValues' then for _,kv in ipairs(child.children) do info.keyValues[#info.keyValues+1]={key=kv.a.key,keyType='string',type=kv.a.type or 'string',value=kv.a.value or ''} end end
    end
    seen[name]=nil;templateInfo[name]=info;return info
end
C_XMLUtil=rawget(_G,'C_XMLUtil') or {}
function C_XMLUtil.GetTemplateInfo(name)
    local info=resolveInfo(name,{});if not info then return nil end
    local copy={};for key,value in pairs(info) do copy[key]=value end
    copy.keyValues={};for index,value in ipairs(info.keyValues) do local kv={};for key,field in pairs(value) do kv[key]=field end;copy.keyValues[index]=kv end
    return copy
end
local function script(f,n,intrinsicOwner,plan)
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
        -- Compile each immutable XML script node once; create a fresh handler
        -- closure per instance so function identity and environments stay local.
        n.scriptFactory=n.scriptFactory or assert(loadstring('return function('..args..')\n'..n.text..'\nend', '@XML/'..n.tag))
        fn=n.scriptFactory()
    end
    if not fn then note('script '..(a['function'] or n.tag)); return end
    -- UI.xsd ScriptType defaults autoEnableInput to true. Lua SetScript
    -- alone does not imply this XML input subscription.
    if a.autoEnableInput~='false' then
        if n.tag=='OnMouseWheel' then f:EnableMouseWheel(true)
        elseif n.tag=='OnKeyDown' or n.tag=='OnKeyUp' then f:EnableKeyboard(true)
        elseif n.tag=='OnEnter' or n.tag=='OnLeave' or n.tag=='OnMouseDown' or n.tag=='OnMouseUp' or n.tag=='OnClick' or n.tag=='OnDoubleClick' or n.tag=='OnDragStart' or n.tag=='OnDragStop' then f:EnableMouse(true) end
    end
    if intrinsicOwner and n.tag=='OnLoad' and not a.intrinsicOrder then
        -- Resolve intrinsic methods after final mixins/KeyValues, before
        -- normal OnLoad. Derived secure mixins override native initialization.
        local loaded=rawget(f,'__selu_intrinsic_loaded') or {}
        rawset(f,'__selu_intrinsic_loaded',loaded)
        if not loaded[intrinsicOwner] then
            loaded[intrinsicOwner]=true
            plan.constructors=plan.constructors or {};plan.constructors[#plan.constructors+1]=fn
        end
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
            if n.tag=='OnUpdate' then __selu_index_update(f) end
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
local childHandlers={}
childHandlers['Size']=function(f,c,n,seen)
    local b=c.a
    local d=dimension(c).a; f:SetSize(tonumber(d.x) or 0,tonumber(d.y) or 0)
end
childHandlers['FontHeight']=function(f,c,n,seen)
    local b=c.a
    local d=dimension(c).a; local path,_,flags=f:GetFont()
    f:SetFont(path,tonumber(d.val or d.value) or 12,flags)
end
childHandlers['Anchors']=function(f,c,n,seen)
    local b=c.a
    seen.anchors=true
    for _,anchor in ipairs(c.children) do
        local v=anchor.a; local x,y=tonumber(v.x) or 0,tonumber(v.y) or 0
        for _,offset in ipairs(anchor.children) do
            local d=dimension(offset).a; x=tonumber(d.x) or 0; y=tonumber(d.y) or 0
        end
        anchorPoint(f,v,x,y,true)
    end
end
childHandlers['KeyValues']=function(f,c,n,seen)
    local b=c.a
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
end
childHandlers['Mixins']=function(f,c,n,seen)
    local b=c.a
    for _,entry in ipairs(c.children) do
        local mix=lookup(entry.a.key)
        if type(mix)=='table' then for key,value in pairs(mix) do f[key]=value end
        else note('mixin '..tostring(entry.a.key)) end
        if entry.a.source=='secure' or entry.a.targetPartition or entry.a.inboundPartition then note('secure mixin partitions') end
    end
end
childHandlers['Scripts']=function(f,c,n,seen)
    local b=c.a
 for _,s in ipairs(c.children) do script(f,s,n.a.intrinsic=='true' and n.a.name,seen) end
end
childHandlers['Animations']=function(f,c,n,seen)
    local b=c.a
    for _,group in ipairs(c.children) do
        if group.tag=='AnimationGroup' then animationGroup(f,group)
        else note('animation element '..group.tag) end
    end
end
childHandlers['Frames']=function(f,c,n,seen)
    local b=c.a
    seen.children=seen.children or {}
    for _,child in ipairs(c.children) do seen.children[#seen.children+1]={node=child} end
end
childHandlers['ScrollChild']=function(f,c,n,seen)
    local b=c.a
    seen.children=seen.children or {}
    if c.children[1] then seen.children[#seen.children+1]={node=c.children[1],scroll=true} end
end
childHandlers['ButtonText']=function(f,c,n,seen)
    local b=c.a
 f:SetFontString(create(c,f))
end
childHandlers['NormalFont']=function(f,c,n,seen)
    local b=c.a
    local setter=f['Set'..c.tag..'Object']
    if setter and b.style then setter(f,lookup(b.style)) end
end
childHandlers['HighlightFont']=childHandlers['NormalFont']
childHandlers['DisabledFont']=childHandlers['NormalFont']
childHandlers['Layers']=function(f,c,n,seen)
    local b=c.a
    for _,layer in ipairs(c.children) do
        for _,child in ipairs(layer.children) do create(child,f,layer.a.level or 'ARTWORK',tonumber(layer.a.textureSubLevel)) end
    end
end
childHandlers['Color']=function(f,c,n,seen)
    local b=c.a
    local r,g,bl,al=tonumber(b.r) or 1,tonumber(b.g) or 1,tonumber(b.b) or 1,tonumber(b.a) or 1
    if n.tag=='FontString' or n.tag=='Font' then f:SetTextColor(r,g,bl,al) else f:SetVertexColor(r,g,bl,al) end
end
childHandlers['TexCoords']=function(f,c,n,seen)
    local b=c.a
 f:SetTexCoord(tonumber(b.left) or 0,tonumber(b.right) or 1,tonumber(b.top) or 0,tonumber(b.bottom) or 1)
end
childHandlers['BarTexture']=function(f,c,n,seen)
    local b=c.a
 f:SetStatusBarTexture(create(c,f))
end
childHandlers['BarColor']=function(f,c,n,seen)
    local b=c.a
 f:SetStatusBarColor(tonumber(b.r) or 1,tonumber(b.g) or 1,tonumber(b.b) or 1,tonumber(b.a) or 1)
end
childHandlers['TextureSuffix']=function(f,c,n,seen)
    local b=c.a
    local texture=create(c,f)
    local setter=f['Set'..c.tag]; if setter then setter(f,texture) end
end
apply=function(f,n,seen)
    if intrinsic[n.tag] then
        assert(not seen[n.tag],'XML intrinsic inheritance cycle: '..n.tag)
        -- An object's intrinsic base is constructed once. Replaying it for
        -- each virtual template would overwrite already-applied derived
        -- mixins (in particular their method overrides).
        seen.intrinsicApplied=seen.intrinsicApplied or {}
        if not seen.intrinsicApplied[n.tag] then
            seen.intrinsicApplied[n.tag]=true
            seen[n.tag]=true; apply(f,templates[n.tag],seen); seen[n.tag]=nil
        end
    end
    if n.a.inherits then inherit(f,n.a.inherits,seen) end
    local mixins=n.mixins
    if not mixins then
        mixins={}
        for _,name in ipairs(names(n.a.mixin)) do mixins[#mixins+1]=name end
        for _,name in ipairs(names(n.a.secureMixin)) do mixins[#mixins+1]=name end
        n.mixins=mixins
    end
    for _,name in ipairs(mixins) do
            local mix=lookup(name)
            if type(mix)=='table' then for k,v in pairs(mix) do f[k]=v end
            else note('mixin '..name) end
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
    if a.enableKeyboard then f:EnableKeyboard(a.enableKeyboard=='true') end
    if a.enableMouseWheel then f:EnableMouseWheel(a.enableMouseWheel=='true') end
    if a.movable then f:SetMovable(a.movable=='true') end
    if a.clampedToScreen then f:SetClampedToScreen(a.clampedToScreen=='true') end
    if a.registerForClicks then f:RegisterForClicks(unpack(names(a.registerForClicks))) end
    if a.registerForDrag then f:RegisterForDrag(unpack(names(a.registerForDrag))) end
    if a.autoFocus then f:SetAutoFocus(a.autoFocus=='true') end
    if a.multiLine then f:SetMultiLine(a.multiLine=='true') end
    if a.id then f:SetID(tonumber(a.id)) end
    if a.orientation then f:SetOrientation(a.orientation) end
    if a.drawLayer then f:SetDrawLayer(a.drawLayer,tonumber(a.subLevel) or 0) end
    if a.minValue or a.maxValue then f:SetMinMaxValues(tonumber(a.minValue) or 0,tonumber(a.maxValue) or 1) end
    if a.defaultValue then f:SetValue(tonumber(a.defaultValue)) end
    if a.setAllPoints=='true' then f:SetAllPoints(f:GetParent()) end
    if a.text then seen.text=seen.text or {}; seen.text[#seen.text+1]=a.text end
    if a.file then f:SetTexture(a.file) end
    if a.atlas then f:SetAtlas(a.atlas,a.useAtlasSize=='true') end
    for _,c in ipairs(n.children) do
        local handler=childHandlers[c.tag]
        if handler then handler(f,c,n,seen)
        elseif string.match(c.tag,'Texture$') then childHandlers.TextureSuffix(f,c,n,seen)
        else note('element '..c.tag) end
    end
end
local function construct(f,prepare)
    constructionDepth=constructionDepth+1
    local plan={}
    rawset(f,'__selu_xml_constructing',true)
    local ok,err=pcall(function()
        prepare(plan)
        for _,constructor in ipairs(plan.constructors or {}) do invoke(constructor,f) end
        -- Children may inspect their parent's derived KeyValues/mixins from
        -- OnLoad (ThreeSliceButton's controller initializes its parent art).
        for _,child in ipairs(plan.children or {}) do
            local object=create(child.node,f)
            if child.scroll then f:SetScrollChild(object) end
        end
        for _,text in ipairs(plan.text or {}) do f:SetText(lookup(text) or text) end
    end)
    constructionDepth=constructionDepth-1
    if ok and next(deferredAnchors) then
        local pending=deferredAnchors;deferredAnchors={}
        for frame,points in pairs(pending) do
            for _,point in pairs(points) do anchorPoint(frame,point[1],point[2],point[3],constructionDepth>0) end
        end
    elseif not ok and constructionDepth==0 then deferredAnchors={} end
    rawset(f,'__selu_xml_constructing',nil)
    if not ok then error(err,0) end
end
local native=CreateFrame
create=function(n,parent,layer,subLevel)
    local a=n.a
    parent=lookup(expand(parentAttribute(n),parent)) or parent
    local name=expand(a.name,parent)
    local f
    if n.tag=='MaskTexture' then f=parent:CreateMaskTexture(name,layer)
    elseif n.tag=='Texture' or string.match(n.tag,'Texture$') then f=parent:CreateTexture(name,layer)
    elseif n.tag=='FontString' or n.tag=='ButtonText' then f=parent:CreateFontString(name,layer)
    elseif n.tag=='Font' then f=CreateFont(name)
    else f=native(nativeKind(n.tag),name,parent) end
    if layer and subLevel then f:SetDrawLayer(layer,subLevel) end
    construct(f,function(plan)
        apply(f,n,plan)
        -- XML textures without anchors fill their parent. Lua CreateTexture
        -- stays unanchored; inherited/explicit XML anchors take precedence.
        if not plan.anchors and (n.tag=='Texture' or n.tag=='MaskTexture' or string.match(n.tag,'Texture$')) and f:GetNumPoints()==0 then
            f:SetAllPoints(parent)
        end
    end)
    if n.tag~='Texture' and n.tag~='FontString' and n.tag~='Font' then
        local onload=f.__scripts.OnLoad; if onload then invoke(onload,f) end
    end
    return f
end
function __selu_xml_apply(f,s)
    construct(f,function(plan) inherit(f,s,plan) end)
    local onload=f.__scripts.OnLoad; if onload then invoke(onload,f) end
end
function __selu_xml_intrinsic(kind) if intrinsic[kind] then return nativeKind(kind) end end
function __selu_xml_declare(n)
    if n.tag=='Binding' then
        local actions=__selu_binding_actions
        if actions then
            actions[n.a.name]={header=n.a.header,runOnUp=n.a.runOnUp=='true',handler=assert(loadstring('return function(keystate)\n'..n.text..'\nend','@XML/Binding'))()}
        end
        return
    end
    if n.tag=='ScopedModifier' then
        note('ScopedModifier forbidden-object policy')
        for _,child in ipairs(n.children) do __selu_xml_declare(child) end
        return
    end
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
        inheritedParents={}
        templateInfo={}
        if n.a.intrinsic=='true' then intrinsic[n.a.name]=n.tag end
    else create(n,nil) end
end
