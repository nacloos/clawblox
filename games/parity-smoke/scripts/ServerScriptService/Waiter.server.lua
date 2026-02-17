local result = Workspace:WaitForChild("DelayedPart", 1.0)

local marker = Instance.new("Folder")
marker.Name = "WaitMarker"
marker:SetAttribute("Found", result ~= nil)
marker.Parent = Workspace
