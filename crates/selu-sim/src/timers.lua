local queue, frame = {}, 1
local frameTime = 1 / 60
local function safe(fn, ...)
 local ok, err = pcall(fn, ...)
 if not ok then __selu_sim_error(tostring(err)) end
end
local function create(delay, callback, iterations, after)
 assert(type(delay) == 'number' and delay == delay and delay < math.huge, 'invalid timer delay')
 assert(type(callback) == 'function', 'invalid timer callback')
 assert(iterations == nil or (iterations > 0 and iterations < math.huge and iterations == math.floor(iterations)), 'invalid iterations')
 local handle = {cancelled=false}
 function handle:Cancel() self.cancelled = true end
 function handle:IsCancelled() return self.cancelled end
 local period = math.max(delay, frameTime)
 -- Positive delays retain exact deadlines, even shorter than a frame.
 if delay > 0 then period = delay end
 queue[#queue+1] = {due=GetTime()+period, period=period, fn=callback, handle=handle, remaining=iterations, after=after}
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
   if t.after then safe(t.fn) else safe(t.fn, t.handle) end
   if t.remaining then t.remaining = t.remaining - 1 end
   if not t.handle.cancelled and (not t.remaining or t.remaining > 0) then
    t.due = t.due + t.period; queue[#queue+1] = t
   end
  end
 end
 if now >= frame * frameTime then
  frame = frame + 1
  local frames = {}
  for i, f in ipairs(__selu_sim_frames) do frames[i] = f end
  for _, f in ipairs(frames) do
   local fn = f.__scripts.OnUpdate
   if f.__shown and fn then safe(fn, f, frameTime) end
  end
 end
end
