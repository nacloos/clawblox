local Markers = {}

function Markers.getOrCreate(name)
    local marker = Workspace:FindFirstChild(name)
    if not marker then
        marker = Instance.new("Folder")
        marker.Name = name
        marker.Parent = Workspace
    end
    return marker
end

function Markers.set(name, key, value)
    local marker = Markers.getOrCreate(name)
    marker:SetAttribute(key, value)
    return marker
end

function Markers.setMany(name, values)
    local marker = Markers.getOrCreate(name)
    for key, value in pairs(values) do
        marker:SetAttribute(key, value)
    end
    return marker
end

return Markers
