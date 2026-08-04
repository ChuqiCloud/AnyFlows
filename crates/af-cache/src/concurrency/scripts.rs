pub(super) const ACQUIRE_SCRIPT: &str = r#"
redis.replicate_commands()
local active_index = KEYS[1]
local ttl = tonumber(ARGV[1])
local member = ARGV[2]
local time = redis.call('TIME')
local now = tonumber(time[1]) * 1000 + math.floor(tonumber(time[2]) / 1000)
local expires_at = now + ttl

local function refresh_index(key)
    local first = redis.call('ZRANGE', key, 0, 0, 'WITHSCORES')
    if #first == 0 then
        redis.call('ZREM', active_index, key)
    else
        redis.call('ZADD', active_index, tonumber(first[2]), key)
        redis.call('PEXPIRE', key, ttl + 1000)
    end
end

local existing = 0
for index = 2, #KEYS do
    redis.call('ZREMRANGEBYSCORE', KEYS[index], '-inf', now)
    if redis.call('ZSCORE', KEYS[index], member) ~= false then
        existing = existing + 1
    end
end

if existing > 0 and existing ~= (#KEYS - 1) then
    return 2
end

if existing == 0 then
    for index = 2, #KEYS do
        local limit = tonumber(ARGV[index + 1])
        if limit > 0 and redis.call('ZCARD', KEYS[index]) >= limit then
            return 1
        end
    end
end

for index = 2, #KEYS do
    redis.call('ZADD', KEYS[index], expires_at, member)
    refresh_index(KEYS[index])
end
return 0
"#;

pub(super) const RENEW_SCRIPT: &str = r#"
redis.replicate_commands()
local active_index = KEYS[1]
local ttl = tonumber(ARGV[1])
local member = ARGV[2]
local time = redis.call('TIME')
local now = tonumber(time[1]) * 1000 + math.floor(tonumber(time[2]) / 1000)
local expires_at = now + ttl

local function refresh_index(key)
    local first = redis.call('ZRANGE', key, 0, 0, 'WITHSCORES')
    if #first == 0 then
        redis.call('ZREM', active_index, key)
    else
        redis.call('ZADD', active_index, tonumber(first[2]), key)
        redis.call('PEXPIRE', key, ttl + 1000)
    end
end

local existing = 0
for index = 2, #KEYS do
    redis.call('ZREMRANGEBYSCORE', KEYS[index], '-inf', now)
    if redis.call('ZSCORE', KEYS[index], member) ~= false then
        existing = existing + 1
    end
end
if existing ~= (#KEYS - 1) then
    for index = 2, #KEYS do
        redis.call('ZREM', KEYS[index], member)
        refresh_index(KEYS[index])
    end
    return 0
end
for index = 2, #KEYS do
    redis.call('ZADD', KEYS[index], expires_at, member)
    refresh_index(KEYS[index])
end
return 1
"#;

pub(super) const RELEASE_SCRIPT: &str = r#"
local active_index = KEYS[1]
local ttl = tonumber(ARGV[1])
local member = ARGV[2]
local removed = 0

local function refresh_index(key)
    local first = redis.call('ZRANGE', key, 0, 0, 'WITHSCORES')
    if #first == 0 then
        redis.call('DEL', key)
        redis.call('ZREM', active_index, key)
    else
        redis.call('ZADD', active_index, tonumber(first[2]), key)
        redis.call('PEXPIRE', key, ttl + 1000)
    end
end

for index = 2, #KEYS do
    removed = removed + redis.call('ZREM', KEYS[index], member)
    refresh_index(KEYS[index])
end
return removed
"#;

pub(super) const ACCOUNT_LOADS_SCRIPT: &str = r#"
redis.replicate_commands()
local active_index = KEYS[1]
local count = tonumber(ARGV[1])
local ttl = tonumber(ARGV[2])
local time = redis.call('TIME')
local now = tonumber(time[1]) * 1000 + math.floor(tonumber(time[2]) / 1000)
local result = {}

local function clean_and_count(key)
    redis.call('ZREMRANGEBYSCORE', key, '-inf', now)
    local first = redis.call('ZRANGE', key, 0, 0, 'WITHSCORES')
    if #first == 0 then
        redis.call('DEL', key)
        redis.call('ZREM', active_index, key)
        return 0
    end
    redis.call('ZADD', active_index, tonumber(first[2]), key)
    redis.call('PEXPIRE', key, ttl + 1000)
    return redis.call('ZCARD', key)
end

for index = 1, count do
    table.insert(result, clean_and_count(KEYS[index + 1]))
    table.insert(result, clean_and_count(KEYS[index + 1 + count]))
end
return result
"#;

pub(super) const EXPIRED_KEYS_SCRIPT: &str = r#"
local time = redis.call('TIME')
local now = tonumber(time[1]) * 1000 + math.floor(tonumber(time[2]) / 1000)
return redis.call('ZRANGEBYSCORE', KEYS[1], '-inf', now, 'LIMIT', 0, tonumber(ARGV[1]))
"#;

pub(super) const CLEAN_KEY_SCRIPT: &str = r#"
redis.replicate_commands()
local active_index = KEYS[1]
local key = KEYS[2]
local ttl = tonumber(ARGV[1])
local time = redis.call('TIME')
local now = tonumber(time[1]) * 1000 + math.floor(tonumber(time[2]) / 1000)
local removed = redis.call('ZREMRANGEBYSCORE', key, '-inf', now)
local first = redis.call('ZRANGE', key, 0, 0, 'WITHSCORES')
if #first == 0 then
    redis.call('DEL', key)
    redis.call('ZREM', active_index, key)
else
    redis.call('ZADD', active_index, tonumber(first[2]), key)
    redis.call('PEXPIRE', key, ttl + 1000)
end
return removed
"#;
