pub(super) const ADMIT_SCRIPT: &str = r#"
redis.replicate_commands()
if #KEYS == 0 or #ARGV ~= (#KEYS * 2) then
    return {2, 0, 0}
end

local time = redis.call('TIME')
local now = tonumber(time[1]) * 1000 + math.floor(tonumber(time[2]) / 1000)
local buckets = {}
local counts = {}
local reset_ats = {}

for index = 1, #KEYS do
    local argument_index = (index - 1) * 2
    local limit = tonumber(ARGV[argument_index + 1])
    local window = tonumber(ARGV[argument_index + 2])
    if limit == nil or window == nil or limit <= 0 or window <= 0
        or limit % 1 ~= 0 or window % 1 ~= 0 then
        return {2, index, 0}
    end

    local bucket = math.floor(now / window)
    local reset_at = (bucket + 1) * window
    local stored = redis.call('HMGET', KEYS[index], 'bucket', 'count')
    local count = 0
    if stored[1] ~= false or stored[2] ~= false then
        if stored[1] == false or stored[2] == false then
            return {2, index, 0}
        end
        local stored_bucket = tonumber(stored[1])
        local stored_count = tonumber(stored[2])
        if stored_bucket == nil or stored_count == nil
            or stored_bucket % 1 ~= 0 or stored_count % 1 ~= 0
            or stored_bucket < 0 or stored_count < 0 or stored_bucket > bucket then
            return {2, index, 0}
        end
        if stored_bucket == bucket then
            count = stored_count
        end
    end

    if count >= limit then
        return {1, index, reset_at - now}
    end
    buckets[index] = bucket
    counts[index] = count
    reset_ats[index] = reset_at
end

for index = 1, #KEYS do
    redis.call('HSET', KEYS[index], 'bucket', buckets[index], 'count', counts[index] + 1)
    redis.call('PEXPIREAT', KEYS[index], reset_ats[index] + 1000)
end
return {0, 0, 0}
"#;
