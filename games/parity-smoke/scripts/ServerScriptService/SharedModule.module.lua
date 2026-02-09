_G.shared_runs = (_G.shared_runs or 0) + 1

return {
    value = 321,
    run = _G.shared_runs,
}
