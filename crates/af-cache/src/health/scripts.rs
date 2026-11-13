pub(super) const READ_STATES_SCRIPT: &str = r#"
local max_safe_integer = 9007199254740991
local function bounded_integer(value, maximum)
    return value ~= nil and value == value and value >= 0 and value <= maximum
        and value == math.floor(value)
end
local half_life = tonumber(ARGV[1])
local outcome_half_life = tonumber(ARGV[2])
local latency_baseline = tonumber(ARGV[3])
if not bounded_integer(half_life, max_safe_integer) or half_life == 0
    or not bounded_integer(outcome_half_life, max_safe_integer) or outcome_half_life == 0
    or not bounded_integer(latency_baseline, 3600000) then
    return redis.error_reply('invalid scheduler health policy')
end
local time = redis.call('TIME')
local now = tonumber(time[1]) * 1000 + math.floor(tonumber(time[2]) / 1000)
local result = {}

local function append_state(key)
    local values = redis.call('HMGET', key,
        'penalty', 'updated_at', 'level', 'cooldown_until',
        'successes', 'failures', 'latency_ema', 'outcomes_updated_at', 'latency_updated_at',
        'streak', 'last_transient_at')
    if values[1] == false and values[2] == false and values[3] == false and values[4] == false
        and values[5] == false and values[6] == false and values[7] == false
        and values[8] == false and values[9] == false then
        if redis.call('EXISTS', key) ~= 0 then
            return redis.error_reply('invalid scheduler health state')
        end
        table.insert(result, 0)
        table.insert(result, 0)
        table.insert(result, 0)
        table.insert(result, 0)
        table.insert(result, 0)
        table.insert(result, 0)
        table.insert(result, 0)
        table.insert(result, 0)
        return
    end
    local penalty = tonumber(values[1])
    local updated_at = tonumber(values[2])
    local level = tonumber(values[3])
    local cooldown_until = tonumber(values[4])
    local successes = tonumber(values[5])
    local failures = tonumber(values[6])
    local latency_ema = tonumber(values[7])
    local outcomes_updated_at = tonumber(values[8])
    local latency_updated_at = tonumber(values[9])
    local streak = tonumber(values[10])
    local last_transient_at = tonumber(values[11])
    -- Hashes written by older releases have only the six core breaker fields.
    if values[5] == false and values[6] == false and values[7] == false
        and values[8] == false and values[9] == false then
        successes = 0
        failures = 0
        latency_ema = 0
        outcomes_updated_at = now
        latency_updated_at = now
    end
    if not bounded_integer(penalty, 64000000)
        or not bounded_integer(updated_at, max_safe_integer)
        or not bounded_integer(streak, 64)
        or not bounded_integer(last_transient_at, max_safe_integer)
        or not bounded_integer(level, 3)
        or not bounded_integer(cooldown_until, max_safe_integer)
        or not bounded_integer(successes, 1000000000000)
        or not bounded_integer(failures, 1000000000000)
        or not bounded_integer(latency_ema, 3600000)
        or not bounded_integer(outcomes_updated_at, max_safe_integer)
        or not bounded_integer(latency_updated_at, max_safe_integer) then
        return redis.error_reply('invalid scheduler health state')
    end
    local elapsed = math.max(0, now - updated_at)
    local decayed = math.floor(penalty * math.pow(0.5, elapsed / half_life) + 0.5)
    if decayed < 1 then decayed = 0 end
    local outcome_elapsed = math.max(0, now - outcomes_updated_at)
    successes = math.floor(successes * math.pow(0.5, outcome_elapsed / outcome_half_life) + 0.5)
    failures = math.floor(failures * math.pow(0.5, outcome_elapsed / outcome_half_life) + 0.5)
    local latency_elapsed = math.max(0, now - latency_updated_at)
    if latency_ema > 0 then
        latency_ema = math.floor(latency_baseline
            + (latency_ema - latency_baseline) * math.pow(0.5, latency_elapsed / outcome_half_life) + 0.5)
    end
    local remaining = math.max(0, cooldown_until - now)
    table.insert(result, decayed)
    table.insert(result, remaining > 0 and 1 or 0)
    table.insert(result, remaining)
    table.insert(result, level)
    table.insert(result, successes)
    table.insert(result, failures)
    table.insert(result, latency_ema)
    local soft_values = redis.call('HMGET', key, 'soft_until', 'soft_failures')
    local soft_until = 0
    if soft_values[1] ~= false or soft_values[2] ~= false then
        local soft_failures = tonumber(soft_values[2])
        soft_until = tonumber(soft_values[1])
        if not bounded_integer(soft_until, max_safe_integer)
            or not bounded_integer(soft_failures, 32) then
            return redis.error_reply('invalid scheduler health soft cooldown')
        end
    end
    table.insert(result, math.max(0, soft_until - now))
end

for index = 1, #KEYS do
    local failure = append_state(KEYS[index])
    if failure then return failure end
end
return result
"#;

pub(super) const APPLY_EVENTS_SCRIPT: &str = r#"
redis.replicate_commands()
local max_safe_integer = 9007199254740991
local function bounded_integer(value, maximum)
    return value ~= nil and value == value and value >= 0 and value <= maximum
        and value == math.floor(value)
end
local half_life = tonumber(ARGV[1])
local streak_window = tonumber(ARGV[2])
local outcome_half_life = tonumber(ARGV[3])
local state_ttl = tonumber(ARGV[4])
local breaker_threshold = tonumber(ARGV[5])
local cooldowns = {tonumber(ARGV[6]), tonumber(ARGV[7]), tonumber(ARGV[8])}
local latency_baseline = tonumber(ARGV[9])
local latency_alpha = tonumber(ARGV[10])
local soft_base = tonumber(ARGV[11])
local soft_max = tonumber(ARGV[12])
local time = redis.call('TIME')
local now = tonumber(time[1]) * 1000 + math.floor(tonumber(time[2]) / 1000)
local result = {}
if not bounded_integer(half_life, max_safe_integer) or half_life == 0
    or not bounded_integer(streak_window, max_safe_integer) or streak_window == 0
    or not bounded_integer(outcome_half_life, max_safe_integer) or outcome_half_life == 0
    or not bounded_integer(state_ttl, max_safe_integer) or state_ttl == 0
    or not bounded_integer(breaker_threshold, 64) or breaker_threshold == 0
    or not bounded_integer(cooldowns[1], max_safe_integer - now) or cooldowns[1] == 0
    or not bounded_integer(cooldowns[2], max_safe_integer - now) or cooldowns[2] == 0
    or not bounded_integer(cooldowns[3], max_safe_integer - now) or cooldowns[3] == 0
    or not bounded_integer(latency_baseline, 3600000)
    or latency_alpha == nil or latency_alpha ~= latency_alpha or latency_alpha < 0 or latency_alpha > 1
    or not bounded_integer(soft_base, max_safe_integer - now) or soft_base == 0
    or not bounded_integer(soft_max, max_safe_integer - now) or soft_max == 0
    or soft_base > soft_max then
    return redis.error_reply('invalid scheduler health policy')
end

local function read_state(key)
    local values = redis.call('HMGET', key,
        'penalty', 'updated_at', 'streak', 'last_transient_at', 'level', 'cooldown_until',
        'successes', 'failures', 'latency_ema', 'outcomes_updated_at', 'latency_updated_at')
    if values[1] == false and values[2] == false and values[3] == false
        and values[4] == false and values[5] == false and values[6] == false
        and values[7] == false and values[8] == false and values[9] == false
        and values[10] == false and values[11] == false then
        if redis.call('EXISTS', key) ~= 0 then return nil end
        return 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, false
    end
    local penalty = tonumber(values[1])
    local updated_at = tonumber(values[2])
    local streak = tonumber(values[3])
    local last_transient_at = tonumber(values[4])
    local level = tonumber(values[5])
    local cooldown_until = tonumber(values[6])
    local successes = tonumber(values[7])
    local failures = tonumber(values[8])
    local latency_ema = tonumber(values[9])
    local outcomes_updated_at = tonumber(values[10])
    local latency_updated_at = tonumber(values[11])
    if values[7] == false and values[8] == false and values[9] == false
        and values[10] == false and values[11] == false then
        successes = 0
        failures = 0
        latency_ema = 0
        outcomes_updated_at = updated_at
        latency_updated_at = updated_at
    end
    if not bounded_integer(penalty, 64000000)
        or not bounded_integer(updated_at, max_safe_integer)
        or not bounded_integer(streak, 64)
        or not bounded_integer(last_transient_at, max_safe_integer)
        or not bounded_integer(level, 3)
        or not bounded_integer(cooldown_until, max_safe_integer)
        or not bounded_integer(successes, 1000000000000)
        or not bounded_integer(failures, 1000000000000)
        or not bounded_integer(latency_ema, 3600000)
        or not bounded_integer(outcomes_updated_at, max_safe_integer)
        or not bounded_integer(latency_updated_at, max_safe_integer) then
        return nil
    end
    return penalty, updated_at, streak, last_transient_at, level, cooldown_until,
        successes, failures, latency_ema, outcomes_updated_at, latency_updated_at, true
end

local function read_soft_state(key)
    local values = redis.call('HMGET', key, 'soft_until', 'soft_failures')
    if values[1] == false and values[2] == false then return 0, 0 end
    local soft_until = tonumber(values[1])
    local soft_failures = tonumber(values[2])
    if not bounded_integer(soft_until, max_safe_integer)
        or not bounded_integer(soft_failures, 32) then
        return nil
    end
    return soft_until, soft_failures
end

local function append_result(penalty, level, cooldown_until, successes, failures, latency_ema, soft_until)
    local remaining = math.max(0, cooldown_until - now)
    table.insert(result, penalty)
    table.insert(result, remaining > 0 and 1 or 0)
    table.insert(result, remaining)
    table.insert(result, level)
    table.insert(result, successes)
    table.insert(result, failures)
    table.insert(result, latency_ema)
    table.insert(result, math.max(0, soft_until - now))
end

-- Redis does not roll back writes when a script returns an error. Validate the
-- entire batch before its first mutation, including all existing hashes.
-- The application pass rereads state so duplicate keys retain event order.
for index = 1, #KEYS do
    local offset = 12 + ((index - 1) * 4)
    local is_failure = tonumber(ARGV[offset + 1])
    local added_penalty = tonumber(ARGV[offset + 2])
    local is_transient = tonumber(ARGV[offset + 3])
    local latency_millis = tonumber(ARGV[offset + 4])
    if not bounded_integer(is_failure, 1) or not bounded_integer(added_penalty, 64000000)
        or not bounded_integer(is_transient, 1)
        or not bounded_integer(latency_millis, 3600000) then
        return redis.error_reply('invalid scheduler health event')
    end
    local penalty = read_state(KEYS[index])
    if penalty == nil then
        return redis.error_reply('invalid scheduler health state')
    end
    local soft_until = read_soft_state(KEYS[index])
    if soft_until == nil then
        return redis.error_reply('invalid scheduler health soft cooldown')
    end
end

for index = 1, #KEYS do
    local offset = 12 + ((index - 1) * 4)
    local is_failure = tonumber(ARGV[offset + 1])
    local added_penalty = tonumber(ARGV[offset + 2])
    local is_transient = tonumber(ARGV[offset + 3])
    local latency_millis = tonumber(ARGV[offset + 4])
    local penalty, updated_at, streak, last_transient_at, level, cooldown_until,
        successes, failures, latency_ema, outcomes_updated_at, latency_updated_at, exists = read_state(KEYS[index])
    local soft_until, soft_failures = read_soft_state(KEYS[index])
    local elapsed = math.max(0, now - updated_at)
    penalty = math.floor(penalty * math.pow(0.5, elapsed / half_life) + 0.5)
    if penalty < 1 then penalty = 0 end
    local outcome_elapsed = math.max(0, now - outcomes_updated_at)
    successes = math.floor(successes * math.pow(0.5, outcome_elapsed / outcome_half_life) + 0.5)
    failures = math.floor(failures * math.pow(0.5, outcome_elapsed / outcome_half_life) + 0.5)
    local latency_elapsed = math.max(0, now - latency_updated_at)
    if latency_ema > 0 then
        latency_ema = math.floor(latency_baseline
            + (latency_ema - latency_baseline) * math.pow(0.5, latency_elapsed / outcome_half_life) + 0.5)
        latency_updated_at = now
    end
    outcomes_updated_at = now
    if is_failure == 1 then
        failures = math.min(1000000000000, failures + 1000000)
    else
        successes = math.min(1000000000000, successes + 1000000)
    end
    if is_failure == 0 and latency_millis > 0 then
        if latency_ema == 0 then
            latency_ema = latency_millis
        else
            latency_ema = math.floor(latency_ema * (1 - latency_alpha) + latency_millis * latency_alpha + 0.5)
        end
        latency_updated_at = now
    end

    if is_failure == 0 then
        penalty = math.max(0, math.floor(penalty * 0.2 - 300000 + 0.5))
        streak = 0
        last_transient_at = 0
        level = 0
        cooldown_until = 0
        soft_until = 0
        soft_failures = 0
    else
        penalty = math.min(64000000, penalty + added_penalty)
        if is_transient == 1 then
            -- Concurrent failures arriving during the same soft window do not escalate it.
            if soft_until <= now then
                local transient_expired = last_transient_at == 0
                    or (now - last_transient_at) > streak_window
                soft_failures = transient_expired and 1 or math.min(soft_failures + 1, 32)
                local previous = 1
                local current = 1
                for step = 3, soft_failures do
                    local next_value = previous + current
                    previous = current
                    current = next_value
                end
                soft_until = now + math.min(soft_max, soft_base * current)
            end
            if cooldown_until > now then
                -- 已经在冷却中的并发迟到失败只加分，不升级冷却层。
                streak = 0
            else
                if last_transient_at > 0 and (now - last_transient_at) <= streak_window then
                    streak = streak + 1
                else
                    streak = 1
                end
                last_transient_at = now
                if streak >= breaker_threshold then
                    level = math.min(level + 1, 3)
                    cooldown_until = now + cooldowns[level]
                    streak = 0
                end
            end
        else
            streak = 0
            last_transient_at = 0
        end
    end
    redis.call('HSET', KEYS[index],
            'penalty', penalty,
            'updated_at', now,
            'streak', streak,
            'last_transient_at', last_transient_at,
            'level', level,
            'cooldown_until', cooldown_until,
            'successes', successes,
            'failures', failures,
            'latency_ema', latency_ema,
            'outcomes_updated_at', outcomes_updated_at,
            'latency_updated_at', latency_updated_at)
    redis.call('HSET', KEYS[index], 'soft_until', soft_until, 'soft_failures', soft_failures)
    redis.call('PEXPIRE', KEYS[index], state_ttl)
    append_result(penalty, level, cooldown_until, successes, failures, latency_ema, soft_until)
end
return result
"#;
