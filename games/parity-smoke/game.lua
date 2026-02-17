task.delay(0.05, function()
    local delayed = Instance.new("Part")
    delayed.Name = "DelayedPart"
    delayed.Anchored = true
    delayed.Position = Vector3.new(0, 3, 0)
    delayed.Parent = Workspace
end)
