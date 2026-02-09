local m = ServerScriptService:FindFirstChild("SharedModule")
local a = require(m)
local b = require(m)

local marker = Instance.new("Folder")
marker.Name = "ParityMarker"
marker:SetAttribute("ModuleValue", a.value)
marker:SetAttribute("RunCount", a.run)
marker:SetAttribute("SameRef", a == b)
marker.Parent = Workspace
