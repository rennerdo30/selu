assert(XmlSurvives:GetWidth() == 32)
AfterBrokenBlizzardChunk = true
secureexecuterange({function() error('synthetic native callback failure') end,function() NativeCallbackContinued=true end},function(_,fn) fn() end)
assert(NativeCallbackContinued)
