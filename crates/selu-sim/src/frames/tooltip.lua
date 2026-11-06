-- Native tooltip state is represented by ordinary simulator child regions.
return function(t)
 local lines = {}
 local function clear()
  for _, pair in ipairs(lines) do pair[1]:Hide();pair[2]:Hide() end
  t.__lineCount = 0
 end
 local function line(left,right,r,g,b,rr,rg,rb,wrap)
  local n=(t.__lineCount or 0)+1;t.__lineCount=n
  local pair=lines[n]
  if not pair then
   local name=t:GetName()
   pair={t:CreateFontString(name and name..'TextLeft'..n),t:CreateFontString(name and name..'TextRight'..n)}
   lines[n]=pair
  end
  local y=-10
  for i=1,n-1 do y=y-lines[i][1]:GetStringHeight()-3 end
  local l,h=pair[1],pair[2]
  l:SetFont('Fonts/FRIZQT__.TTF',12);h:SetFont('Fonts/FRIZQT__.TTF',12)
  l:SetText(left or '');h:SetText(right or '')
  l:SetTextColor(r or 1,g or .82,b or 0);h:SetTextColor(rr or 1,rg or 1,rb or 1)
  l:ClearAllPoints();h:ClearAllPoints()
  l:SetPoint('TOPLEFT',t,'TOPLEFT',10,y);h:SetPoint('TOPRIGHT',t,'TOPRIGHT',-10,y)
  l:SetWordWrap(wrap or false);l:SetWidth(wrap and 300 or l:GetUnboundedStringWidth())
  l:Show();h:SetShown(right~=nil)
  local width=0
  for i=1,n do width=math.max(width,lines[i][1]:GetWidth()+lines[i][2]:GetUnboundedStringWidth()+30) end
  t:SetSize(width, -y+l:GetStringHeight()+10)
 end
 t.ClearLines=clear
 t.NumLines=function() return t.__lineCount or 0 end
 t.AddLine=function(_,text,r,g,b,wrap) line(text,nil,r,g,b,nil,nil,nil,wrap) end
 t.AddDoubleLine=function(_,left,right,r,g,b,rr,rg,rb) line(left,right,r,g,b,rr,rg,rb,false) end
 t.SetText=function(_,text,r,g,b,a,wrap) clear();line(text,nil,r,g,b,nil,nil,nil,wrap) end
 t.GetText=function() return (t.__lineCount or 0)>0 and lines[1][1]:GetText() or nil end
 local owner=t.SetOwner
 t.SetOwner=function(_,o,anchor)
  owner(t,o,anchor);clear();t:ClearAllPoints()
  if anchor=='ANCHOR_LEFT' then t:SetPoint('TOPRIGHT',o,'TOPLEFT',0,0)
  elseif anchor=='ANCHOR_TOP' then t:SetPoint('BOTTOM',o,'TOP',0,0)
  elseif anchor~='ANCHOR_NONE' then t:SetPoint('TOPLEFT',o,'TOPRIGHT',0,0) end
 end
 local bg=t:CreateTexture(nil,'BACKGROUND');bg:SetAllPoints(t)
 bg:SetTexture('Interface/Tooltips/UI-Tooltip-Background');bg:SetVertexColor(0.1,0.1,0.1,1)
 -- The client tooltip edge file stores eight consecutive edge/corner tiles.
 local anchors={'LEFT','RIGHT','TOP','BOTTOM','TOPLEFT','TOPRIGHT','BOTTOMLEFT','BOTTOMRIGHT'}
 for i,anchor in ipairs(anchors) do
  local edge=t:CreateTexture(nil,'BORDER');edge:SetTexture('Interface/Tooltips/UI-Tooltip-Border')
  edge:SetTexCoord((i-1)/8,i/8,0,1);edge:SetSize(16,16)
  if i==1 or i==2 then
   edge:SetPoint(anchor=='LEFT' and 'TOPLEFT' or 'TOPRIGHT',t,anchor=='LEFT' and 'TOPLEFT' or 'TOPRIGHT',0,-16)
   edge:SetPoint(anchor=='LEFT' and 'BOTTOMLEFT' or 'BOTTOMRIGHT',t,anchor=='LEFT' and 'BOTTOMLEFT' or 'BOTTOMRIGHT',0,16)
  elseif i==3 or i==4 then
   edge:SetPoint(anchor=='TOP' and 'TOPLEFT' or 'BOTTOMLEFT',t,anchor=='TOP' and 'TOPLEFT' or 'BOTTOMLEFT',16,0)
   edge:SetPoint(anchor=='TOP' and 'TOPRIGHT' or 'BOTTOMRIGHT',t,anchor=='TOP' and 'TOPRIGHT' or 'BOTTOMRIGHT',-16,0)
  else edge:SetPoint(anchor,t,anchor,0,0) end
 end
 t:SetFrameStrata('TOOLTIP');t:Hide()
end
