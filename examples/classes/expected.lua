local __selu_addon, __selu_ns = ...
local __selu_on = __selu_ns.__selu_events
local __selu_setmetatable = setmetatable
__selu_ns["main"] = {}
local Tracker = {}
Tracker.methods = {}
Tracker.methods.__index = Tracker.methods
function Tracker.__init(self, name)
  self.total = 0
  self.name = name
  Tracker.created = Tracker.created + 1
end
function Tracker.new(name)
  local self = __selu_setmetatable({}, Tracker.methods)
  Tracker.__init(self, name)
  return self
end
function Tracker.methods:record(amount)
  self.total = self.total + amount
end
function Tracker.methods:summary()
  return string.format("%s: %s", self.name, self.total)
end
Tracker.created = 0
local LootTracker = {}
LootTracker.methods = {}
LootTracker.methods.__index = LootTracker.methods
__selu_setmetatable(LootTracker, { __index = Tracker })
__selu_setmetatable(LootTracker.methods, { __index = Tracker.methods })
function LootTracker.new(name)
  local self = __selu_setmetatable({}, LootTracker.methods)
  Tracker.__init(self, name)
  self.multiplier = 2
  return self
end
function LootTracker.make()
  return LootTracker.new("Loot")
end
function LootTracker.methods:record(amount)
  Tracker.methods.record(self, amount * self.multiplier)
end
function LootTracker.methods:collect()
  local add = function(amount)
    self:record(amount)
  end
  add(3)
  add(2)
end
__selu_on("PLAYER_LOGIN", function()
  local quests = Tracker.new("Quests")
  quests:record(2)
  local loot = LootTracker.make()
  loot:collect()
  print(quests:summary())
  print(loot:summary())
  print(string.format("Trackers: %s", Tracker.created))
end)
