return function(state, name)
    log(pretty(state))
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
