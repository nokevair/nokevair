function isseq(table)
    local len = 0
    for _, _ in pairs(table) do
        len = len + 1
    end
    return len == #table
end

function pretty(obj)
    if type(obj) == "string" then
        -- terrible way to do it, i know
        return '"' .. obj .. '"'
    elseif type(obj) == "table" then
        local seq = isseq(obj)
        local res = ""
        for k, v in pairs(obj) do
            local key
            if seq then
                key = ""
            elseif type(k) == "string" then
                key = k .. " = "
            else
                key = "[" .. pretty(k) .. "] = "
            end
            if res ~= "" then
                res = res .. ", "
            end
            res = res .. key .. pretty(v)
        end
        return "{" .. res .. "}"
    else
        return tostring(obj)
    end
end

return function(state, name)
    print(pretty(state))
    if state == nil then
        return {people = {}}
    else
        for _, p in ipairs(state.people) do
            case = rand(4)
            if case == 0 then
                p[1] = p[1] + 1
            elseif case == 1 then
                p[1] = p[1] - 1
            elseif case == 2 then
                p[2] = p[2] + 1
            elseif case == 3 then
                p[2] = p[2] - 1
            end
        end
        if rand(4) == 0 then
            table.insert(state.people, {0, 0})
        end
        return state
    end
end