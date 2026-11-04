local queue, frame = {}, 1
local flush = __selu_sim_flush
local frameTime = 1 / 60
local function safe(fn, ...)
 __selu_sim_budget = __selu_sim_budget - 1
 if __selu_sim_budget < 0 then error('dispatch budget exceeded') end
 local ok, err = pcall(fn, ...)
 if not ok then
  local converted, message = pcall(tostring, err)
  __selu_sim_error(converted and message or 'error object could not be formatted')
 end
 flush()
 return ok
end
local function deadline(origin, index, period, now)
 local due = origin + index * period
 if due <= now then
  origin, index = now, 1
  period = math.max(period, frameTime, math.abs(now) * 2^-51)
  due = origin + period
 end
 return due, origin, index, period
end
local visible = __selu_sim_visible
local function create(delay, callback, iterations, after)
 assert(type(delay) == 'number' and delay == delay and delay < math.huge, 'invalid timer delay')
 assert(type(callback) == 'function', 'invalid timer callback')
 assert(iterations == nil or type(iterations) == 'number', 'invalid iterations')
 local handle = {cancelled=false}
 function handle:Cancel() self.cancelled = true end
 function handle:IsCancelled() return self.cancelled end
 local period = math.max(delay, frameTime)
 -- Positive delays retain exact deadlines, even shorter than a frame.
 if delay > 0 then period = delay end
 local due, origin, index
 due, origin, index, period = deadline(GetTime(), 1, period, GetTime())
 queue[#queue+1] = {due=due, origin=origin, index=index, period=period, fn=callback, handle=handle, remaining=iterations, after=after}
 return handle
end
C_Timer = {}
function C_Timer.After(delay, callback) create(delay, callback, 1, true) end
function C_Timer.NewTimer(delay, callback) return create(delay, callback, 1) end
function C_Timer.NewTicker(delay, callback, iterations) return create(delay, callback, iterations) end
function __selu_sim_next_step()
 local nextTime = frame * frameTime
 for _, timer in ipairs(queue) do
  if not timer.handle.cancelled then nextTime = math.min(nextTime, timer.due) end
 end
 return nextTime
end
function __selu_sim_step()
 local now = GetTime()
 -- Snapshot due timers; additions are handled by the next scheduler step.
 local due = {}
 for i = #queue, 1, -1 do
  local t = queue[i]
  if t.handle.cancelled then table.remove(queue, i)
  elseif t.due <= now then table.insert(due, 1, t); table.remove(queue, i) end
 end
 for _, t in ipairs(due) do
  if not t.handle.cancelled then
   local ok
   if t.after then ok = safe(t.fn) else ok = safe(t.fn, t.handle) end
   if t.remaining then t.remaining = t.remaining - 1 end
   if ok and not t.handle.cancelled and (not t.remaining or t.remaining > 0) then
    t.due, t.origin, t.index, t.period = deadline(t.origin, t.index + 1, t.period, now)
    queue[#queue+1] = t
   end
  end
 end
 if now >= frame * frameTime then
  frame = frame + 1
  local frames = {}
  for i, f in ipairs(__selu_sim_frames) do frames[i] = f end
  for _, f in ipairs(frames) do
   local fn = f.__scripts.OnUpdate
   if visible(f) and fn then safe(fn, f, frameTime) end
  end
 end
end
