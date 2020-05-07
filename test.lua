function(state)
    function isseq(table)
        idxs = 0
        for _, _ in pairs(table) do
            idxs = idxs + 1
        end
        return idxs == #table
    end

    function pretty(obj)
        if type(obj) == "string" then
            -- terrible way of doing it, i know
            return '"' .. obj .. '"'
        elseif type(obj) == "table" then
            res = ""
            seq = isseq(obj)
            for k, v in pairs(obj) do
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

    print("before: ", pretty(state))
    if state.counter == nil then
        state.counter = 0
    else
        state.counter = state.counter + 1
    end
    print("after:  ", pretty(state), "\n")
end
