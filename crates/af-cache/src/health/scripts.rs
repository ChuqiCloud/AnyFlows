pub(super) const READ_STATES_SCRIPT: &str = r#"
local half_life = tonumber(ARGV[1])
local time = redis.call('TIME')
local now = tonumber(time[1]) * 1000 + math.floor(tonumber(time[2]) / 1000)
local result = {}

local function append_state(key)
    local values = redis.call('HMGET', key, 'penalty', 'updated_at', 'level', 'cooldown_until')
    if values[1] == false and values[2] == false and values[3] == false and values[4] == false then
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
    if penalty == nil or updated_at == nil or level == nil or cooldown_until == nil
        or penalty < 0 or updated_at < 0 or level < 0 or level > 3 or cooldown_until < 0 then
        return redis.error_reply('invalid scheduler health state')
    end
    local elapsed = math.max(0, now - updated_at)
    local decayed = math.floor(penalty * math.pow(0.5, elapsed / half_life) + 0.5)
    if decayed < 1 then decayed = 0 end
    local remaining = math.max(0, cooldown_until - now)
    table.insert(result, decayed)
    table.insert(result, remaining > 0 and 1 or 0)
    table.insert(result, remaining)
    table.insert(result, level)
end

for index = 1, #KEYS do
    append_state(KEYS[index])
end
return result
"#;

pub(super) const APPLY_EVENTS_SCRIPT: &str = r#"
redis.replicate_commands()
local half_life = tonumber(ARGV[1])
local streak_window = tonumber(ARGV[2])
local state_ttl = tonumber(ARGV[3])
local breaker_threshold = tonumber(ARGV[4])
local cooldowns = {tonumber(ARGV[5]), tonumber(ARGV[6]), tonumber(ARGV[7])}
local time = redis.call('TIME')
local now = tonumber(time[1]) * 1000 + math.floor(tonumber(time[2]) / 1000)
local result = {}

local function read_state(key)
    local values = redis.call('HMGET', key,
        'penalty', 'updated_at', 'streak', 'last_transient_at', 'level', 'cooldown_until')
    if values[1] == false and values[2] == false and values[3] == false
        and values[4] == false and values[5] == false and values[6] == false then
        return 0, 0, 0, 0, 0, 0, false
    end
    local penalty = tonumber(values[1])
    local updated_at = tonumber(values[2])
    local streak = tonumber(values[3])
    local last_transient_at = tonumber(values[4])
    local level = tonumber(values[5])
    local cooldown_until = tonumber(values[6])
    if penalty == nil or updated_at == nil or streak == nil or last_transient_at == nil
        or level == nil or cooldown_until == nil or penalty < 0 or updated_at < 0
        or streak < 0 or last_transient_at < 0 or level < 0 or level > 3
        or cooldown_until < 0 then
        return nil
    end
    return penalty, updated_at, streak, last_transient_at, level, cooldown_until, true
end

local function append_result(penalty, level, cooldown_until)
    local remaining = math.max(0, cooldown_until - now)
    table.insert(result, penalty)
    table.insert(result, remaining > 0 and 1 or 0)
    table.insert(result, remaining)
    table.insert(result, level)
end

for index = 1, #KEYS do
    local offset = 7 + ((index - 1) * 3)
    local is_failure = tonumber(ARGV[offset + 1])
    local added_penalty = tonumber(ARGV[offset + 2])
    local is_transient = tonumber(ARGV[offset + 3])
    if (is_failure ~= 0 and is_failure ~= 1) or added_penalty == nil or added_penalty < 0
        or (is_transient ~= 0 and is_transient ~= 1) then
        return redis.error_reply('invalid scheduler health event')
    end

    local penalty, updated_at, streak, last_transient_at, level, cooldown_until, exists = read_state(KEYS[index])
    if penalty == nil then
        return redis.error_reply('invalid scheduler health state')
    end
    local elapsed = math.max(0, now - updated_at)
    penalty = math.floor(penalty * math.pow(0.5, elapsed / half_life) + 0.5)
    if penalty < 1 then penalty = 0 end

    if is_failure == 0 then
        if exists then
            penalty = math.max(0, math.floor(penalty * 0.2 - 300000 + 0.5))
            streak = 0
            last_transient_at = 0
            level = 0
            cooldown_until = 0
            if penalty == 0 then
                redis.call('DEL', KEYS[index])
            else
                redis.call('HSET', KEYS[index],
                    'penalty', penalty,
                    'updated_at', now,
                    'streak', streak,
                    'last_transient_at', last_transient_at,
                    'level', level,
                    'cooldown_until', cooldown_until)
                redis.call('PEXPIRE', KEYS[index], state_ttl)
            end
        end
    else
        penalty = math.min(64000000, penalty + added_penalty)
        if is_transient == 1 then
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
        redis.call('HSET', KEYS[index],
            'penalty', penalty,
            'updated_at', now,
            'streak', streak,
            'last_transient_at', last_transient_at,
            'level', level,
            'cooldown_until', cooldown_until)
        redis.call('PEXPIRE', KEYS[index], state_ttl)
    end
    append_result(penalty, level, cooldown_until)
end
return result
"#;
