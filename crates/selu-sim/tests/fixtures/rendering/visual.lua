local function frame(kind,name,w,h,x,y)
 local f=CreateFrame(kind,name,UIParent); f:SetSize(w,h); f:SetPoint("BOTTOMLEFT",x,y); return f
end
local function fill(f,r,g,b) local t=f:CreateTexture(nil,"BACKGROUND");t:SetAllPoints();t:SetColorTexture(r,g,b,1);return t end
local function label(f,text,y)
 local t=f:CreateFontString(); t:SetSize(330,36);t:SetPoint("TOPLEFT",12,y or -12);t:SetFont("fallback",18,"OUTLINE");t:SetText(text);return t
end
local panel=frame("Frame","TextureFixture",400,250,100,650);fill(panel,.08,.09,.12);label(panel,"Textures: TGA / cropped + tinted")
local t=panel:CreateTexture(nil,"ARTWORK");t:SetSize(150,150);t:SetPoint("BOTTOMLEFT",20,20);t:SetTexture("checker.tga")
local u=panel:CreateTexture(nil,"OVERLAY");u:SetSize(150,150);u:SetPoint("BOTTOMLEFT",220,20);u:SetTexture("checker.tga");u:SetTexCoord(0,.5,0,.5);u:SetVertexColor(.5,1,1,.75)
local text=frame("Frame","TextFixture",400,250,550,650);fill(text,.08,.09,.12);label(text,"|cffffcc00Gold|r and white text")
local l=label(text,"Red, centered with shadow",-80);l:SetTextColor(1,.2,.2,1);l:SetJustifyH("CENTER");l:SetShadowColor(0,0,0,1);l:SetShadowOffset(2,-2)
local a=frame("Frame","OrderBack",280,200,100,350);fill(a,.7,.1,.1);label(a,"Low red")
local b=frame("Frame","OrderFront",280,200,200,300);fill(b,.1,.3,.8);b:Raise();label(b,"Raised blue")
local v=frame("ScrollFrame","Viewport",400,220,550,300);fill(v,.1,.1,.1)
local c=CreateFrame("Frame","ScrollContent",v);c:SetSize(500,500);v:SetScrollChild(c)
fill(c,.15,.45,.25);label(c,"Clipped scroll content",-20);label(c,"Visible after scrolling",-200)
v:SetVerticalScroll(140);v:SetHorizontalScroll(30)
